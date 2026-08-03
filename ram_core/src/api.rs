//! Roblox REST API wrappers — avatar thumbnails, presence, place resolution.

use serde::Deserialize;

use crate::auth::RobloxClient;
use crate::error::CoreError;
use crate::models::{ModerationInfo, Presence};

// ---------------------------------------------------------------------------
// Avatar thumbnails
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ThumbnailResponse {
    data: Vec<ThumbnailEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThumbnailEntry {
    target_id: u64,
    image_url: Option<String>,
}

/// Fetch avatar headshot thumbnail URLs for a batch of user IDs.
/// Returns a vec of `(user_id, url)` pairs.
///
/// Deliberately unauthenticated: the thumbnails endpoint is public (same as
/// [`fetch_game_icons`]), and this is one batched call covering every account
/// at once. Signing it with some account's cookie meant a single revoked
/// cookie turned a would-be anonymous 200 into a 403, blanking the avatars
/// for every account in the list.
pub async fn fetch_avatars(
    client: &RobloxClient,
    user_ids: &[u64],
) -> Result<Vec<(u64, String)>, CoreError> {
    if user_ids.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<String> = user_ids.iter().map(|id| id.to_string()).collect();
    let ids_param = ids.join(",");
    let url = format!(
        "https://thumbnails.roblox.com/v1/users/avatar-headshot\
         ?userIds={ids_param}&size=150x150&format=Png&isCircular=false"
    );

    let resp: ThumbnailResponse = client.get_json(&url, "").await?;
    Ok(pair_thumbnails(user_ids, resp.data))
}

/// Pair thumbnail entries back to the IDs that asked for them.
///
/// Keyed on `targetId`, never on position. The thumbnails service does not
/// promise request order, and it omits IDs it can't resolve (deleted accounts,
/// typo'd IDs) rather than returning a null entry. Both were observed live:
/// `/games/icons` echoes a reordered array, and `/users/avatar-headshot`
/// returns two entries for a three-ID request containing one bad ID. Zipping
/// the request against the response therefore handed images to the wrong
/// account or universe and dropped the trailing one.
fn pair_thumbnails(requested_ids: &[u64], data: Vec<ThumbnailEntry>) -> Vec<(u64, String)> {
    data.into_iter()
        .filter(|entry| requested_ids.contains(&entry.target_id))
        .filter_map(|entry| entry.image_url.map(|url| (entry.target_id, url)))
        .collect()
}

/// Download the actual image bytes for each avatar URL.
/// Returns a vec of `(user_id, raw_bytes)` pairs (skips failures).
///
/// Unauthenticated for the same reason as [`fetch_avatars`]: these URLs point
/// at the public `rbxcdn` image host, which has no use for a session cookie.
pub async fn download_avatar_images(
    client: &RobloxClient,
    avatars: &[(u64, String)],
) -> Vec<(u64, Vec<u8>)> {
    let mut results = Vec::new();
    for (id, url) in avatars {
        match client.get_bytes(url, "").await {
            Ok(bytes) => results.push((*id, bytes)),
            Err(e) => tracing::warn!("Failed to download avatar for {id}: {e}"),
        }
    }
    results
}

// ---------------------------------------------------------------------------
// Presence
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresenceResponse {
    user_presences: Vec<PresenceEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresenceEntry {
    user_presence_type: u8,
    place_id: Option<u64>,
    game_id: Option<String>,
    last_location: Option<String>,
}

/// Fetch presence info for multiple user IDs.
pub async fn fetch_presences(
    client: &RobloxClient,
    cookie: &str,
    user_ids: &[u64],
) -> Result<Vec<(u64, Presence)>, CoreError> {
    if user_ids.is_empty() {
        return Ok(vec![]);
    }
    let body = serde_json::json!({ "userIds": user_ids });
    let resp: PresenceResponse = client
        .post_json(
            "https://presence.roblox.com/v1/presence/users",
            cookie,
            Some(&body),
        )
        .await?;

    Ok(user_ids
        .iter()
        .zip(resp.user_presences.iter())
        .map(|(id, p)| {
            (
                *id,
                Presence {
                    user_presence_type: p.user_presence_type,
                    place_id: p.place_id,
                    game_id: p.game_id.clone(),
                    last_location: p.last_location.clone().unwrap_or_default(),
                },
            )
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Place / Universe resolution
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UniverseDetails {
    name: String,
}

#[derive(Deserialize)]
struct UniverseResponse {
    data: Vec<UniverseDetails>,
}

/// Resolve a universe ID to its game name. Works unauthenticated.
pub async fn resolve_universe_name(
    client: &RobloxClient,
    universe_id: u64,
) -> Result<String, CoreError> {
    let url = format!("https://games.roblox.com/v1/games?universeIds={universe_id}");
    let resp: UniverseResponse = client.get_json(&url, "").await?;
    resp.data
        .into_iter()
        .next()
        .map(|d| d.name)
        .ok_or_else(|| CoreError::RobloxApi {
            status: 404,
            message: format!("universe {universe_id} not found"),
        })
}

/// Fetch game icon thumbnail URLs for a batch of universe IDs.
/// Returns a vec of `(universe_id, url)` pairs.
pub async fn fetch_game_icons(
    client: &RobloxClient,
    _cookie: &str,
    universe_ids: &[u64],
) -> Result<Vec<(u64, String)>, CoreError> {
    if universe_ids.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<String> = universe_ids.iter().map(|id| id.to_string()).collect();
    let ids_param = ids.join(",");
    let url = format!(
        "https://thumbnails.roblox.com/v1/games/icons\
         ?universeIds={ids_param}&returnPolicy=PlaceHolder&size=150x150&format=Png&isCircular=false"
    );

    let resp: ThumbnailResponse = client.get_json(&url, "").await?;
    Ok(pair_thumbnails(universe_ids, resp.data))
}

// ---------------------------------------------------------------------------
// Server list (for Job ID joining)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameServer {
    pub id: String,
    pub max_players: u32,
    pub playing: u32,
    pub fps: f32,
    pub ping: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerListResponse {
    data: Vec<GameServer>,
    next_page_cursor: Option<String>,
}

/// Fetch one page of public servers for a place.
pub async fn fetch_servers(
    client: &RobloxClient,
    cookie: &str,
    place_id: u64,
    cursor: Option<&str>,
) -> Result<(Vec<GameServer>, Option<String>), CoreError> {
    let mut url = format!(
        "https://games.roblox.com/v1/games/{place_id}/servers/0?sortOrder=Asc&limit=25"
    );
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={c}"));
    }
    let resp: ServerListResponse = client.get_json(&url, cookie).await?;
    Ok((resp.data, resp.next_page_cursor))
}

// ---------------------------------------------------------------------------
// Share link resolution
// ---------------------------------------------------------------------------

/// Resolve a Roblox share link code (from `/share?code=CODE&type=Server`)
/// into `(place_id, universe_id, link_code, access_code)`.
///
/// Two-step process:
/// 1. POST `apis.roblox.com/sharelinks/v1/resolve-link` to get placeId + linkCode.
/// 2. GET `/games/{placeId}/game?privateServerLinkCode={linkCode}` and scrape
///    the UUID access code from the `joinPrivateGame(...)` JS call.
pub async fn resolve_share_link(
    client: &RobloxClient,
    cookie: &str,
    share_code: &str,
) -> Result<(u64, Option<u64>, String, String), CoreError> {
    use regex::Regex;

    // --- Step 1: Resolve share code → placeId + linkCode via API ---
    let body = serde_json::json!({
        "linkId": share_code,
        "linkType": "Server",
    });
    let resp: serde_json::Value = client
        .post_json(
            "https://apis.roblox.com/sharelinks/v1/resolve-link",
            cookie,
            Some(&body),
        )
        .await?;

    let ps_data = resp
        .get("privateServerInviteData")
        .ok_or_else(|| CoreError::RobloxApi {
            status: 400,
            message: "share link response missing privateServerInviteData".into(),
        })?;

    let place_id = ps_data
        .get("placeId")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| CoreError::RobloxApi {
            status: 400,
            message: "share link response missing placeId".into(),
        })?;

    let link_code = ps_data
        .get("linkCode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::RobloxApi {
            status: 400,
            message: "share link response missing linkCode".into(),
        })?
        .to_string();

    let universe_id = ps_data.get("universeId").and_then(|v| v.as_u64());

    let status = ps_data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    if status != "Valid" {
        return Err(CoreError::RobloxApi {
            status: 400,
            message: format!("private server invite status: {status}"),
        });
    }

    tracing::info!("Share link resolved → placeId={place_id}, linkCode={link_code}");

    // --- Step 2: Scrape accessCode (UUID) from the game page ---
    let game_url = format!(
        "https://www.roblox.com/games/{place_id}/game?privateServerLinkCode={link_code}"
    );
    let html = client.get_text(&game_url, cookie).await?;

    let access_re = Regex::new(
        r"Roblox\.GameLauncher\.joinPrivateGame\(\d+\s*,\s*'([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})'"
    ).expect("invalid regex");

    let access_code = access_re
        .captures(&html)
        .and_then(|cap| cap.get(1))
        .ok_or_else(|| CoreError::RobloxApi {
            status: 400,
            message: "could not scrape accessCode from game page".into(),
        })?
        .as_str()
        .to_string();

    tracing::info!("Access code resolved → {access_code}");

    Ok((place_id, universe_id, link_code, access_code))
}

// ---------------------------------------------------------------------------
// GitLab update check
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ReleaseLinks {
    #[serde(rename = "self")]
    self_url: String,
}

#[derive(Deserialize)]
struct GitLabRelease {
    tag_name: String,
    _links: ReleaseLinks,
}

/// Check for a newer release on GitLab. Returns `Some((version, url))` if an
/// update is available, `None` if already on the latest.
pub async fn check_for_updates(current_version: &str) -> Result<Option<(String, String)>, CoreError> {
    let client = reqwest::Client::builder()
        .user_agent("RM-update-check")
        .build()?;

    let release: GitLabRelease = client
        .get("https://gitlab.com/api/v4/projects/centerepic%2Frobloxmanager/releases/permalink/latest")
        .send()
        .await?
        .json()
        .await?;

    let remote = release.tag_name.trim_start_matches('v');
    let local = current_version.trim_start_matches('v');

    if remote != local {
        Ok(Some((remote.to_string(), release._links.self_url)))
    } else {
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Moderation / enforcement detection
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicUserResponse {
    #[serde(default)]
    is_banned: bool,
}

/// Check whether a Roblox user is **permanently terminated** via the public
/// profile endpoint. Works without a cookie. Temporary moderations are NOT
/// reflected here — use [`fetch_moderation_message`] alongside this for those.
pub async fn fetch_public_ban_status(
    client: &RobloxClient,
    user_id: u64,
) -> Result<bool, CoreError> {
    let url = format!("https://users.roblox.com/v1/users/{user_id}");
    let resp: PublicUserResponse = client.get_json(&url, "").await?;
    Ok(resp.is_banned)
}

#[derive(Deserialize)]
struct UsernameLookupResponse {
    data: Vec<UsernameLookupEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsernameLookupEntry {
    id: u64,
    name: String,
    display_name: String,
}

/// Look up a Roblox user by username. Returns `Ok(None)` if no such user
/// exists. Crucially, this passes `excludeBannedUsers: false` so the lookup
/// works for terminated accounts too — used by the "add anyway" flow when
/// the cookie itself has been revoked.
pub async fn lookup_username(
    client: &RobloxClient,
    username: &str,
) -> Result<Option<(u64, String, String)>, CoreError> {
    let body = serde_json::json!({
        "usernames": [username],
        "excludeBannedUsers": false,
    });
    let resp: UsernameLookupResponse = client
        .post_json(
            "https://users.roblox.com/v1/usernames/users",
            "",
            Some(&body),
        )
        .await?;
    Ok(resp
        .data
        .into_iter()
        .next()
        .map(|e| (e.id, e.name, e.display_name)))
}

/// v1 payload from `usermoderation.roblox.com/v1/not-approved`. Carries the
/// human-readable message and a punishment-type label. Fields we don't use
/// are intentionally left off.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct NotApprovedV1 {
    #[serde(default)]
    message_to_user: String,
    #[serde(default)]
    end_date: String,
}

/// v2 payload from `usermoderation.roblox.com/v2/not-approved`. Has the cleanest
/// machine-readable timestamps, so we use it for expiry resolution.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct NotApprovedV2 {
    restriction: Option<NotApprovedV2Restriction>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotApprovedV2Restriction {
    #[serde(default)]
    end_time: Option<String>,
    #[serde(default)]
    duration_seconds: Option<i64>,
}

/// Cookie-only moderation probe. Hits the two `usermoderation.roblox.com`
/// endpoints (v1 for the localized message, v2 for the structured expiry).
/// Returns `(reason, expires_at)` when the cookie is recognised AND the
/// account is currently under an enforcement action, else `None`.
///
/// Doesn't need the user ID, so this works even when `validate_cookie` has
/// failed (e.g. on a terminated account whose cookie has been revoked enough
/// to break `users/authenticated` but not the moderation endpoints).
pub async fn fetch_moderation_message(
    client: &RobloxClient,
    cookie: &str,
) -> Option<(String, Option<chrono::DateTime<chrono::Utc>>)> {
    let v1: Option<NotApprovedV1> = client
        .get_json("https://usermoderation.roblox.com/v1/not-approved", cookie)
        .await
        .ok();
    let v2: Option<NotApprovedV2> = client
        .get_json("https://usermoderation.roblox.com/v2/not-approved", cookie)
        .await
        .ok();

    let reason = v1.as_ref().and_then(|p| {
        let m = p.message_to_user.trim();
        if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        }
    })?;

    let expires_at = v2
        .as_ref()
        .and_then(|p| p.restriction.as_ref())
        .and_then(|r| {
            r.end_time
                .as_ref()
                .filter(|s| !s.is_empty())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&chrono::Utc))
                .or_else(|| {
                    r.duration_seconds.and_then(|d| {
                        if d > 0 {
                            Some(chrono::Utc::now() + chrono::Duration::seconds(d))
                        } else {
                            None
                        }
                    })
                })
        })
        .or_else(|| {
            v1.as_ref().and_then(|p| {
                let s = p.end_date.trim();
                if s.is_empty() {
                    None
                } else {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| d.with_timezone(&chrono::Utc))
                }
            })
        });
    Some((reason, expires_at))
}

/// Fetch the current moderation snapshot for the signed-in account, combining
/// the public `isBanned` check with the cookie-only moderation message.
pub async fn fetch_moderation_status(
    client: &RobloxClient,
    user_id: u64,
    cookie: &str,
) -> Result<Option<ModerationInfo>, CoreError> {
    let is_banned = fetch_public_ban_status(client, user_id)
        .await
        .unwrap_or(false);
    let msg = fetch_moderation_message(client, cookie).await;

    if !is_banned && msg.is_none() {
        return Ok(None);
    }

    let (reason, expires_at) = match msg {
        Some((r, e)) => (Some(r), e),
        None => (None, None),
    };

    Ok(Some(ModerationInfo {
        is_banned,
        // No fabricated fallback: leave `reason` as `None` when we don't have
        // a real one from the moderation endpoint. The UI can fall back to a
        // generic title and, crucially, the caller's merge logic can preserve
        // a previously-known specific reason instead of being clobbered by a
        // generic string on subsequent revalidations.
        reason,
        expires_at,
        last_checked: Some(chrono::Utc::now()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(target_id: u64, image_url: Option<&str>) -> ThumbnailEntry {
        ThumbnailEntry {
            target_id,
            image_url: image_url.map(|s| s.to_string()),
        }
    }

    /// Roblox drops IDs it can't resolve rather than returning a null entry.
    /// Verified against the live endpoint: requesting `1,999999999999,156`
    /// returns two entries, for 1 and 156.
    #[test]
    fn missing_entries_do_not_shift_avatars_onto_wrong_accounts() {
        let requested = [1u64, 999_999_999_999, 156];
        let responded = vec![entry(1, Some("url-1")), entry(156, Some("url-156"))];

        let paired = pair_thumbnails(&requested, responded);

        assert_eq!(
            paired,
            vec![(1, "url-1".to_string()), (156, "url-156".to_string())]
        );
    }

    #[test]
    fn out_of_order_responses_pair_correctly() {
        let requested = [1u64, 156];
        let responded = vec![entry(156, Some("url-156")), entry(1, Some("url-1"))];

        let paired = pair_thumbnails(&requested, responded);

        assert_eq!(
            paired,
            vec![(156, "url-156".to_string()), (1, "url-1".to_string())]
        );
    }

    #[test]
    fn entries_without_an_image_are_skipped() {
        let requested = [1u64, 156];
        let responded = vec![entry(1, None), entry(156, Some("url-156"))];

        let paired = pair_thumbnails(&requested, responded);

        assert_eq!(paired, vec![(156, "url-156".to_string())]);
    }

    #[test]
    fn unrequested_ids_are_ignored() {
        let requested = [1u64];
        let responded = vec![entry(1, Some("url-1")), entry(999, Some("url-999"))];

        let paired = pair_thumbnails(&requested, responded);

        assert_eq!(paired, vec![(1, "url-1".to_string())]);
    }
}
