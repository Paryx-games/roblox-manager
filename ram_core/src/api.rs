//! Roblox REST API wrappers — avatar thumbnails, presence, place resolution.

use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Deserialize;

use crate::auth::RobloxClient;
use crate::error::CoreError;
use crate::models::{ModerationInfo, Presence};

pub fn trusted_roblox_image_url(value: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(value).ok()?;
    let host = parsed.host_str()?;
    (parsed.scheme() == "https" && (host == "rbxcdn.com" || host.ends_with(".rbxcdn.com")))
        .then(|| value.to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserSearchResult {
    pub user_id: u64,
    pub username: String,
    pub display_name: String,
}

#[derive(Deserialize)]
struct UserSearchResponse {
    data: Vec<UserSearchResultEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserSearchResultEntry {
    id: u64,
    name: String,
    display_name: String,
}

fn normalize_search_keyword(keyword: &str) -> Result<String, CoreError> {
    let trimmed = keyword.trim();
    if trimmed.len() < 3 {
        return Err(CoreError::RobloxApi {
            status: 400,
            message: "The keyword is too short.".into(),
        });
    }

    Ok(trimmed
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => vec![byte as char],
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchTargetKind {
    UserId,
    Username,
    BroadSearch,
}

fn classify_search_target(keyword: &str) -> SearchTargetKind {
    let trimmed = keyword.trim();
    if trimmed.is_empty() || trimmed.len() < 3 {
        return SearchTargetKind::BroadSearch;
    }

    let has_letters = trimmed.chars().any(|ch| ch.is_ascii_alphabetic());
    let has_spaces = trimmed.contains(char::is_whitespace);
    let is_numeric = trimmed.chars().all(|ch| ch.is_ascii_digit());

    if is_numeric {
        SearchTargetKind::UserId
    } else if !has_spaces && !has_letters {
        SearchTargetKind::BroadSearch
    } else if !has_spaces {
        SearchTargetKind::Username
    } else {
        SearchTargetKind::BroadSearch
    }
}

/// Try the exact path first: user IDs resolve without a broad search, and exact
/// usernames are a single targeted lookup rather than a fuzzy keyword query.
pub async fn search_users(
    client: &RobloxClient,
    keyword: &str,
) -> Result<Vec<UserSearchResult>, CoreError> {
    let trimmed = keyword.trim();
    let target_kind = classify_search_target(trimmed);
    println!(
        "DEBUG search_users input='{trimmed}' kind={:?} len={}",
        target_kind,
        trimmed.len()
    );
    tracing::debug!(
        keyword = trimmed,
        class = ?target_kind,
        "Searching Roblox users"
    );

    let results = match target_kind {
        SearchTargetKind::UserId => {
            println!("DEBUG exact user-id lookup path");
            if let Ok(user_id) = trimmed.parse::<u64>() {
                let url = format!("https://users.roblox.com/v1/users/{user_id}");
                match client.get_json::<UserSearchResultEntry>(&url, "").await {
                    Ok(user) => vec![UserSearchResult {
                        user_id: user.id,
                        username: user.name,
                        display_name: user.display_name,
                    }],
                    Err(err) => {
                        println!("DEBUG exact user-id lookup failed: {err}");
                        tracing::warn!(
                            user_id,
                            error = %err,
                            "Exact user-id lookup failed; falling back to broad search"
                        );
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            }
        }
        SearchTargetKind::Username => match lookup_username(client, trimmed).await {
            Ok(Some((user_id, username, display_name))) => {
                println!("DEBUG exact username lookup hit: {username} ({user_id})");
                vec![UserSearchResult {
                    user_id,
                    username,
                    display_name,
                }]
            }
            Ok(None) => {
                println!("DEBUG exact username lookup missed: {trimmed}");
                Vec::new()
            }
            Err(err) => {
                println!("DEBUG exact username lookup failed: {err}");
                tracing::warn!(
                    keyword = trimmed,
                    error = %err,
                    "Exact username lookup failed; falling back to broad search"
                );
                Vec::new()
            }
        },
        SearchTargetKind::BroadSearch => {
            println!("DEBUG broad keyword search path");
            let keyword = normalize_search_keyword(trimmed)?;
            let url =
                format!("https://users.roblox.com/v1/users/search?keyword={keyword}&limit=10");
            let response: UserSearchResponse = client.get_json(&url, "").await?;
            response
                .data
                .into_iter()
                .map(|user| UserSearchResult {
                    user_id: user.id,
                    username: user.name,
                    display_name: user.display_name,
                })
                .collect()
        }
    };

    tracing::debug!(
        count = results.len(),
        keyword = trimmed,
        "Roblox user search completed"
    );
    Ok(results)
}

/// Fetch the authenticated user's friend IDs. Results are cached per account so
/// friends-only joins can be filtered without re-requesting the friend list every
/// time the user clicks a join action.
pub async fn fetch_friends(
    client: &RobloxClient,
    cookie: &str,
    user_id: u64,
) -> Result<Vec<u64>, CoreError> {
    let mut friends = Vec::new();
    let mut cursor = None;
    loop {
        let mut url = format!("https://friends.roblox.com/v1/users/{user_id}/friends");
        if let Some(next_cursor) = cursor.as_deref() {
            url.push_str(&format!("?cursor={next_cursor}"));
        }
        #[derive(Deserialize)]
        struct FriendsPage {
            data: Vec<FriendEntry>,
            next_page_cursor: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FriendEntry {
            id: u64,
        }

        let page: FriendsPage = client.get_json(&url, cookie).await?;
        let ids: Vec<u64> = page.data.into_iter().map(|entry| entry.id).collect();
        if ids.is_empty() {
            break;
        }
        friends.extend(ids.iter().copied());
        match page.next_page_cursor {
            Some(next) if !next.is_empty() => cursor = Some(next),
            _ => break,
        }
    }
    tracing::debug!(user_id, count = friends.len(), "Fetched friend cache");
    Ok(friends)
}

#[derive(Deserialize)]
struct FollowingResponse {
    followings: Vec<FollowingEntry>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FollowingEntry {
    user_id: u64,
    is_following: bool,
}

fn following_relationship(
    response: FollowingResponse,
    target_user_id: u64,
) -> Result<bool, CoreError> {
    response
        .followings
        .into_iter()
        .find(|entry| entry.user_id == target_user_id)
        .map(|entry| entry.is_following)
        .ok_or_else(|| CoreError::RobloxApi {
            status: 502,
            message: "Roblox omitted the follow relationship".into(),
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FollowOutcome {
    success: bool,
    #[serde(default)]
    is_captcha_required: bool,
}

fn require_follow_success(outcome: FollowOutcome) -> Result<(), CoreError> {
    if !outcome.success || outcome.is_captcha_required {
        return Err(CoreError::RobloxApi {
            status: 403,
            message: "Roblox requires an interactive challenge or did not accept the follow".into(),
        });
    }
    Ok(())
}

/// Check the existing relationship before a temporary follow.
pub async fn is_following(
    client: &RobloxClient,
    cookie: &str,
    target_user_id: u64,
) -> Result<bool, CoreError> {
    let response: FollowingResponse = client
        .post_json(
            "https://friends.roblox.com/v1/user/following-exists",
            cookie,
            Some(&serde_json::json!({"targetUserIds": [target_user_id]})),
        )
        .await?;
    following_relationship(response, target_user_id)
}

/// Follow another user from the authenticated account.
pub async fn follow_user(
    client: &RobloxClient,
    cookie: &str,
    target_user_id: u64,
) -> Result<(), CoreError> {
    tracing::info!(target_user_id, "Following user");
    let outcome: FollowOutcome = client
        .post_json(
            &format!("https://friends.roblox.com/v1/users/{target_user_id}/follow"),
            cookie,
            None,
        )
        .await?;
    require_follow_success(outcome)
}

/// Unfollow another user from the authenticated account.
pub async fn unfollow_user(
    client: &RobloxClient,
    cookie: &str,
    target_user_id: u64,
) -> Result<(), CoreError> {
    tracing::info!(target_user_id, "Unfollowing user");
    perform_connection_action(
        client,
        cookie,
        Method::POST,
        &format!("https://friends.roblox.com/v1/users/{target_user_id}/unfollow"),
    )
    .await
}

/// Send a friend request to another user from the authenticated account.
pub async fn send_friend_request(
    client: &RobloxClient,
    cookie: &str,
    target_user_id: u64,
) -> Result<(), CoreError> {
    tracing::info!(target_user_id, "Sending friend request");
    perform_connection_action(
        client,
        cookie,
        Method::POST,
        &format!("https://friends.roblox.com/v1/users/{target_user_id}/request-friendship"),
    )
    .await
}

/// Block another user from the authenticated account.
pub async fn block_user(
    client: &RobloxClient,
    cookie: &str,
    target_user_id: u64,
) -> Result<(), CoreError> {
    tracing::info!(target_user_id, "Blocking user");
    perform_connection_action(
        client,
        cookie,
        Method::POST,
        &format!("https://apis.roblox.com/user-blocking-api/v1/users/{target_user_id}/block-user"),
    )
    .await
}

async fn perform_connection_action(
    client: &RobloxClient,
    cookie: &str,
    method: Method,
    url: &str,
) -> Result<(), CoreError> {
    let response = client.request(method, url, cookie, None).await?;
    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let message = response.text().await.unwrap_or_default();
        Err(CoreError::RobloxApi {
            status: status.as_u16(),
            message: if message.is_empty() {
                status.to_string()
            } else {
                message
            },
        })
    }
}

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
    #[serde(default)]
    state: Option<String>,
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
    tracing::debug!(
        count = user_ids.len(),
        "Fetching avatar thumbnails for user batch"
    );
    let ids: Vec<String> = user_ids.iter().map(|id| id.to_string()).collect();
    let ids_param = ids.join(",");
    let url = format!(
        "https://thumbnails.roblox.com/v1/users/avatar-headshot\
         ?userIds={ids_param}&size=150x150&format=Png&isCircular=false"
    );

    let resp: ThumbnailResponse = client.get_json(&url, "").await?;
    let paired = pair_thumbnails(user_ids, resp.data);
    tracing::debug!(resolved = paired.len(), "Resolved avatar thumbnail URLs");
    Ok(paired)
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
        .filter(|entry| {
            entry
                .state
                .as_deref()
                .is_none_or(|state| state == "Completed")
        })
        .filter_map(|entry| {
            entry
                .image_url
                .filter(|url| !url.is_empty())
                .map(|url| (entry.target_id, url))
        })
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
    // Keep CDN fan-out bounded.  A large account roster used to download
    // thumbnails serially, making refreshes unnecessarily slow; unbounded
    // spawning would instead create a burst of requests and sockets.
    const MAX_PARALLEL_AVATAR_DOWNLOADS: usize = 4;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_PARALLEL_AVATAR_DOWNLOADS));
    let mut tasks = tokio::task::JoinSet::new();
    for (id, url) in avatars {
        let permit = semaphore.clone().acquire_owned().await;
        let Ok(permit) = permit else {
            break;
        };
        let client = client.clone();
        let id = *id;
        let url = url.clone();
        tasks.spawn(async move {
            let result = client.get_bytes(&url, "").await;
            drop(permit);
            (id, result)
        });
    }

    let mut results = Vec::with_capacity(avatars.len());
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((id, Ok(bytes))) => results.push((id, bytes)),
            Ok((id, Err(e))) => tracing::warn!("Failed to download avatar for {id}: {e}"),
            Err(e) => tracing::warn!("Avatar download task failed: {e}"),
        }
    }
    tracing::debug!(
        downloaded = results.len(),
        requested = avatars.len(),
        "Avatar images download complete"
    );
    results
}

// ---------------------------------------------------------------------------
// Presence
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresenceResponse {
    user_presences: Option<Vec<PresenceEntry>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresenceEntry {
    user_id: u64,
    #[serde(default = "unknown_presence_type")]
    user_presence_type: u8,
    place_id: Option<u64>,
    game_id: Option<String>,
    last_location: Option<String>,
}

fn unknown_presence_type() -> u8 {
    u8::MAX
}

fn map_presences(resp: PresenceResponse, user_ids: &[u64]) -> Vec<(u64, Presence)> {
    let mut seen = std::collections::HashSet::new();
    resp.user_presences
        .unwrap_or_default()
        .into_iter()
        .filter(|p| user_ids.contains(&p.user_id) && seen.insert(p.user_id))
        .map(|p| {
            (
                p.user_id,
                Presence {
                    user_presence_type: p.user_presence_type,
                    place_id: p.place_id,
                    game_id: p.game_id,
                    last_location: p.last_location.unwrap_or_default(),
                },
            )
        })
        .collect()
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
    tracing::debug!(count = user_ids.len(), "Fetching presences for user batch");
    let body = serde_json::json!({ "userIds": user_ids });
    let resp: PresenceResponse = client
        .post_json(
            "https://presence.roblox.com/v1/presence/users",
            cookie,
            Some(&body),
        )
        .await?;

    let presences = map_presences(resp, user_ids);
    tracing::debug!(count = presences.len(), "Fetched presences successfully");
    Ok(presences)
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
    tracing::debug!(universe_id, "Resolving universe name");
    let url = format!("https://games.roblox.com/v1/games?universeIds={universe_id}");
    let resp: UniverseResponse = client.get_json(&url, "").await?;
    let name = resp
        .data
        .into_iter()
        .next()
        .map(|d| d.name)
        .ok_or_else(|| CoreError::RobloxApi {
            status: 404,
            message: format!("universe {universe_id} not found"),
        })?;
    tracing::debug!(universe_id, name = %name, "Resolved universe name");
    Ok(name)
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
    tracing::debug!(
        count = universe_ids.len(),
        "Fetching game icons for universe batch"
    );
    let ids: Vec<String> = universe_ids.iter().map(|id| id.to_string()).collect();
    let ids_param = ids.join(",");
    let url = format!(
        "https://thumbnails.roblox.com/v1/games/icons\
         ?universeIds={ids_param}&returnPolicy=PlaceHolder&size=150x150&format=Png&isCircular=false"
    );

    let resp: ThumbnailResponse = client.get_json(&url, "").await?;
    let paired = pair_thumbnails(universe_ids, resp.data);
    tracing::debug!(resolved = paired.len(), "Resolved game icons");
    Ok(paired)
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
    tracing::debug!(
        place_id,
        has_cursor = cursor.is_some(),
        "Fetching servers for place"
    );
    let mut url =
        format!("https://games.roblox.com/v1/games/{place_id}/servers/0?sortOrder=Asc&limit=25");
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={c}"));
    }
    let resp: ServerListResponse = client.get_json(&url, cookie).await?;
    tracing::debug!(
        place_id,
        count = resp.data.len(),
        has_next = resp.next_page_cursor.is_some(),
        "Fetched servers for place"
    );
    Ok((resp.data, resp.next_page_cursor))
}

// ---------------------------------------------------------------------------
// Share link resolution
// ---------------------------------------------------------------------------

/// Resolve a Roblox share link code (from `/share?code=CODE&type=Server`)
/// into `(place_id, universe_id, link_code, access_code)`.
///
/// 1. POST `apis.roblox.com/sharelinks/v1/resolve-link` for the invitation target.
/// 2. Resolve the universe's root place when no place ID is returned.
/// 3. GET `/games/{placeId}/game?privateServerLinkCode={linkCode}` and read
///    the UUID access code from the `joinPrivateGame(...)` JS call.
pub async fn resolve_share_link(
    client: &RobloxClient,
    cookie: &str,
    share_code: &str,
) -> Result<(u64, Option<u64>, String, String), CoreError> {
    let body = serde_json::json!({ "linkId": share_code, "linkType": "Server" });
    let response: serde_json::Value = client
        .post_json(
            "https://apis.roblox.com/sharelinks/v1/resolve-link",
            cookie,
            Some(&body),
        )
        .await?;
    let (place_id, universe_id, link_code) = private_share_target(&response)?;
    let place_id = match place_id {
        Some(place_id) => place_id,
        None => {
            let universe_id = universe_id.ok_or_else(private_server_lookup_error)?;
            let response: serde_json::Value = client
                .get_json(
                    &format!("https://games.roblox.com/v1/games?universeIds={universe_id}"),
                    "",
                )
                .await?;
            private_share_root_place(&response, universe_id)?
        }
    };

    tracing::info!("Share link resolved → placeId={place_id}");

    let game_url =
        format!("https://www.roblox.com/games/{place_id}/game?privateServerLinkCode={link_code}");
    let html = client.get_text(&game_url, cookie).await?;

    let access_code =
        private_access_code(&html, place_id, &link_code).ok_or_else(private_server_lookup_error)?;

    tracing::info!("Private server access code resolved");

    Ok((place_id, universe_id, link_code, access_code))
}

// ---------------------------------------------------------------------------
// GitHub update check
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

/// Check for a newer release on GitHub. Returns `Some((version, url))` if an
/// update is available, `None` if already on the latest.
pub async fn check_for_updates(
    current_version: &str,
) -> Result<Option<(String, String)>, CoreError> {
    let client = reqwest::Client::builder()
        .user_agent("RM-update-check")
        .build()?;

    let release: GitHubRelease = client
        .get("https://api.github.com/repos/Paryx-games/roblox-manager/releases/latest")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let remote = release.tag_name.trim_start_matches('v');
    let local = current_version.trim_start_matches('v');

    if remote != local {
        Ok(Some((remote.to_string(), release.html_url)))
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
    #[serde(default)]
    created: Option<DateTime<Utc>>,
}

/// Check whether a Roblox user is **permanently terminated** via the public
/// profile endpoint. Works without a cookie. Temporary moderations are NOT
/// reflected here — use [`fetch_moderation_message`] alongside this for those.
pub async fn fetch_public_ban_status(
    client: &RobloxClient,
    user_id: u64,
) -> Result<bool, CoreError> {
    tracing::debug!(user_id, "Checking public ban status");
    let url = format!("https://users.roblox.com/v1/users/{user_id}");
    let resp: PublicUserResponse = client.get_json(&url, "").await?;
    tracing::debug!(
        user_id,
        is_banned = resp.is_banned,
        "Public ban status received"
    );
    Ok(resp.is_banned)
}

/// Fetch the public Roblox account creation timestamp.
pub async fn fetch_public_created_at(
    client: &RobloxClient,
    user_id: u64,
) -> Result<Option<DateTime<Utc>>, CoreError> {
    tracing::debug!(user_id, "Fetching public account creation date");
    let url = format!("https://users.roblox.com/v1/users/{user_id}");
    let resp: PublicUserResponse = client.get_json(&url, "").await?;
    tracing::debug!(
        user_id,
        has_created = resp.created.is_some(),
        "Public creation date received"
    );
    Ok(resp.created)
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
    tracing::debug!(username, "Looking up user by username");
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
    let found = resp
        .data
        .into_iter()
        .next()
        .map(|e| (e.id, e.name, e.display_name));
    tracing::debug!(username, found = found.is_some(), "User lookup completed");
    Ok(found)
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
    tracing::debug!("Probing usermoderation endpoints for enforcement message");
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
    tracing::info!(
        has_expires = expires_at.is_some(),
        "Scraped active moderation message from Roblox"
    );
    Some((reason, expires_at))
}

/// Fetch the current moderation snapshot for the signed-in account, combining
/// the public `isBanned` check with the cookie-only moderation message.
pub async fn fetch_moderation_status(
    client: &RobloxClient,
    user_id: u64,
    cookie: &str,
) -> Result<Option<ModerationInfo>, CoreError> {
    tracing::debug!(user_id, "Fetching moderation status for user");
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

    tracing::info!(
        user_id,
        is_banned,
        has_reason = reason.is_some(),
        "Detected moderation enforcement for user"
    );

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

pub enum ParsedPrivateServerUrl {
    Direct { place_id: u64, link_code: String },
    Share { share_code: String },
}

fn private_server_parameter(url: &reqwest::Url, name: &str) -> Option<String> {
    let mut values = url
        .query_pairs()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name));
    let (_, value) = values.next()?;
    (values.next().is_none() && valid_private_server_code(&value)).then(|| value.into_owned())
}

fn valid_private_server_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 512
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
}

pub fn parse_private_server_url(input: &str) -> Result<ParsedPrivateServerUrl, &'static str> {
    let error = "Enter a valid Roblox private server URL";
    let url = reqwest::Url::parse(input.trim()).map_err(|_| error)?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("www.roblox.com" | "roblox.com"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err(error);
    }
    let segments: Vec<_> = url.path_segments().ok_or(error)?.collect();
    if segments.first() == Some(&"games") {
        let place_id = segments
            .get(1)
            .ok_or(error)?
            .parse::<u64>()
            .map_err(|_| error)?;
        if place_id == 0 {
            return Err(error);
        }
        let link_code = private_server_parameter(&url, "privateServerLinkCode")
            .ok_or("Enter a valid Roblox private server URL")?;
        return Ok(ParsedPrivateServerUrl::Direct {
            place_id,
            link_code,
        });
    }
    if url.path().trim_end_matches('/') == "/share" {
        if let Some(share_code) = private_server_parameter(&url, "code") {
            return Ok(ParsedPrivateServerUrl::Share { share_code });
        }
    }
    Err("Enter a valid Roblox private server URL")
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PrivateServerOwnerLookup {
    #[default]
    NotChecked,
    Found,
    NotListed,
    RateLimited,
    Unavailable,
    Incomplete,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrivateServerCheck {
    pub status: &'static str,
    pub owner_id: Option<u64>,
    pub owner_username: Option<String>,
    pub owner_lookup: PrivateServerOwnerLookup,
}

impl Default for PrivateServerCheck {
    fn default() -> Self {
        Self {
            status: "unknown",
            owner_id: None,
            owner_username: None,
            owner_lookup: PrivateServerOwnerLookup::NotChecked,
        }
    }
}

fn private_access_code(html: &str, place_id: u64, link_code: &str) -> Option<String> {
    let pattern = regex::Regex::new(
        r#"Roblox\.GameLauncher\.joinPrivateGame\s*\(\s*(\d+)\s*,\s*['"]([0-9a-fA-F-]{36})['"]\s*,\s*['"]([a-zA-Z0-9_-]{1,512})['"]"#,
    )
    .ok()?;
    let access = pattern.captures_iter(html).find_map(|capture| {
        if capture.get(1)?.as_str().parse::<u64>().ok()? != place_id
            || capture.get(3)?.as_str() != link_code
        {
            return None;
        }
        uuid::Uuid::parse_str(capture.get(2)?.as_str())
            .ok()
            .map(|id| id.to_string())
    });
    access
}

/// Resolve the invitation's UUID for Rust callers; never include it in UI summaries.
pub async fn resolve_private_server_access(
    client: &RobloxClient,
    cookie: &str,
    place_id: u64,
    link_code: &str,
) -> Result<String, CoreError> {
    if place_id == 0 || !valid_private_server_code(link_code) {
        return Err(private_server_lookup_error());
    }
    let url =
        format!("https://www.roblox.com/games/{place_id}/game?privateServerLinkCode={link_code}");
    let html = client.get_text(&url, cookie).await?;
    private_access_code(&html, place_id, link_code).ok_or_else(private_server_lookup_error)
}

fn private_server_owner(
    body: &serde_json::Value,
    access_code: &str,
) -> Option<(u64, Option<String>)> {
    body.get("data")?.as_array()?.iter().find_map(|server| {
        let code = server.get("accessCode")?.as_str()?;
        if !code.eq_ignore_ascii_case(access_code) {
            return None;
        }
        let owner = server.get("owner")?;
        let id = owner.get("id")?.as_u64().filter(|id| *id > 0)?;
        Some((
            id,
            owner
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        ))
    })
}

fn private_server_lookup_error() -> CoreError {
    CoreError::RobloxApi {
        status: 0,
        message: "Private server ownership could not be verified".into(),
    }
}

fn private_share_target(
    body: &serde_json::Value,
) -> Result<(Option<u64>, Option<u64>, String), CoreError> {
    let data = body
        .get("privateServerInviteData")
        .ok_or_else(private_server_lookup_error)?;
    if data.get("status").and_then(serde_json::Value::as_str) != Some("Valid") {
        return Err(private_server_lookup_error());
    }
    let id = |key| {
        data.get(key)
            .and_then(serde_json::Value::as_u64)
            .filter(|id| *id > 0)
    };
    let place_id = id("placeId");
    let universe_id = id("universeId");
    if place_id.is_none() && universe_id.is_none() {
        return Err(private_server_lookup_error());
    }
    let code = data
        .get("linkCode")
        .and_then(serde_json::Value::as_str)
        .filter(|value| valid_private_server_code(value))
        .ok_or_else(private_server_lookup_error)?;
    Ok((place_id, universe_id, code.into()))
}

fn private_share_root_place(body: &serde_json::Value, universe_id: u64) -> Result<u64, CoreError> {
    body.get("data")
        .and_then(serde_json::Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(serde_json::Value::as_u64) == Some(universe_id))
        })
        .and_then(|row| row.get("rootPlaceId"))
        .and_then(serde_json::Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or_else(private_server_lookup_error)
}

fn owned_server_candidates(
    body: &serde_json::Value,
    user_id: u64,
    place_id: u64,
    universe_id: Option<u64>,
) -> Result<Vec<(u64, Option<String>)>, CoreError> {
    let rows = body
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(private_server_lookup_error)?;
    let mut candidates = Vec::new();
    for row in rows {
        let owner = row
            .get("ownerId")
            .and_then(serde_json::Value::as_u64)
            .filter(|id| *id > 0)
            .ok_or_else(private_server_lookup_error)?;
        if owner != user_id {
            return Err(private_server_lookup_error());
        }
        let row_place = row
            .get("placeId")
            .and_then(serde_json::Value::as_u64)
            .filter(|id| *id > 0);
        let row_universe = row
            .get("universeId")
            .and_then(serde_json::Value::as_u64)
            .filter(|id| *id > 0);
        if row_place.is_some_and(|id| id != place_id)
            && universe_id
                .zip(row_universe)
                .is_some_and(|(expected, id)| expected != id)
        {
            continue;
        }
        if row_place.is_some_and(|id| id != place_id) && universe_id.is_none() {
            continue;
        }
        let id = row
            .get("privateServerId")
            .and_then(serde_json::Value::as_u64)
            .filter(|id| *id > 0)
            .ok_or_else(private_server_lookup_error)?;
        let name = row
            .get("ownerName")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        candidates.push((id, name));
    }
    Ok(candidates)
}

fn private_server_detail_link(
    link: &str,
    place_id: u64,
) -> Result<ParsedPrivateServerUrl, CoreError> {
    if valid_private_server_code(link) {
        return Ok(ParsedPrivateServerUrl::Direct {
            place_id,
            link_code: link.into(),
        });
    }
    parse_private_server_url(link).map_err(|_| private_server_lookup_error())
}

fn checked_private_server_cursor(body: &serde_json::Value) -> Result<Option<String>, CoreError> {
    match body.get("nextPageCursor") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) if value.is_empty() => Ok(None),
        Some(serde_json::Value::String(value)) if value.len() <= 4096 => Ok(Some(value.clone())),
        _ => Err(private_server_lookup_error()),
    }
}

/// Verify ownership against the authenticated account's complete owned-server list.
/// A negative result is returned only after every relevant server and page was checked.
pub async fn check_owned_private_server(
    client: &RobloxClient,
    cookie: &str,
    user_id: u64,
    place_id: u64,
    universe_id: Option<u64>,
    link_code: &str,
) -> Result<Option<PrivateServerCheck>, CoreError> {
    if user_id == 0 || place_id == 0 || !valid_private_server_code(link_code) {
        return Err(private_server_lookup_error());
    }
    let mut cursor: Option<String> = None;
    let mut seen = std::collections::HashSet::new();
    let mut details_checked = 0;
    for _ in 0..10 {
        let mut url =
            reqwest::Url::parse("https://games.roblox.com/v1/private-servers/my-private-servers")
                .map_err(|_| private_server_lookup_error())?;
        url.query_pairs_mut()
            .append_pair("privateServersTab", "MyPrivateServers")
            .append_pair("itemsPerPage", "25");
        if let Some(cursor) = &cursor {
            url.query_pairs_mut().append_pair("cursor", cursor);
        }
        let body: serde_json::Value = client.get_json(url.as_str(), cookie).await?;
        for (id, owner_username) in owned_server_candidates(&body, user_id, place_id, universe_id)?
        {
            details_checked += 1;
            if details_checked > 25 {
                return Err(private_server_lookup_error());
            }
            let detail: serde_json::Value = client
                .get_json(
                    &format!("https://games.roblox.com/v1/vip-servers/{id}"),
                    cookie,
                )
                .await?;
            if detail.get("id").and_then(serde_json::Value::as_u64) != Some(id) {
                return Err(private_server_lookup_error());
            }
            let link = detail
                .get("link")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(private_server_lookup_error)?;
            let code = match private_server_detail_link(link, place_id)? {
                ParsedPrivateServerUrl::Direct { link_code, .. } => link_code,
                ParsedPrivateServerUrl::Share { share_code } => {
                    resolve_share_link(client, cookie, &share_code).await?.2
                }
            };
            if code == link_code {
                return Ok(Some(PrivateServerCheck {
                    status: match detail.get("active").and_then(serde_json::Value::as_bool) {
                        Some(true) => "valid",
                        Some(false) => "invalid",
                        None => "unknown",
                    },
                    owner_id: Some(user_id),
                    owner_username,
                    owner_lookup: PrivateServerOwnerLookup::Found,
                }));
            }
        }
        cursor = checked_private_server_cursor(&body)?;
        let Some(next) = &cursor else {
            return Ok(None);
        };
        if !seen.insert(next.clone()) {
            return Err(private_server_lookup_error());
        }
    }
    Err(private_server_lookup_error())
}

/// Validate the existing join-page contract, then match the accessible-server API by access code.
/// Missing launcher markup or inaccessible owner data remains unknown, never proof of non-ownership.
pub async fn check_private_server(
    client: &RobloxClient,
    cookie: &str,
    place_id: u64,
    link_code: &str,
) -> Result<PrivateServerCheck, CoreError> {
    if place_id == 0 || !valid_private_server_code(link_code) {
        return Ok(PrivateServerCheck {
            status: "invalid",
            ..Default::default()
        });
    }
    let access_code = match resolve_private_server_access(client, cookie, place_id, link_code).await
    {
        Ok(access_code) => access_code,
        Err(CoreError::RobloxApi {
            status: 400 | 404, ..
        }) => {
            return Ok(PrivateServerCheck {
                status: "invalid",
                ..Default::default()
            })
        }
        Err(error) => return Err(error),
    };
    Ok(fetch_private_server_owner(client, cookie, place_id, &access_code).await)
}

fn owner_lookup_failure(error: &CoreError) -> PrivateServerOwnerLookup {
    match error {
        CoreError::RateLimited => PrivateServerOwnerLookup::RateLimited,
        _ => PrivateServerOwnerLookup::Unavailable,
    }
}

/// Owner data is optional enrichment and cannot invalidate an already resolved invitation.
pub async fn fetch_private_server_owner(
    client: &RobloxClient,
    cookie: &str,
    place_id: u64,
    access_code: &str,
) -> PrivateServerCheck {
    if place_id == 0 || uuid::Uuid::parse_str(access_code).is_err() {
        return PrivateServerCheck::default();
    }
    lookup_private_server_owner(access_code, |cursor: Option<String>| async move {
        let mut url = match reqwest::Url::parse(&format!(
            "https://games.roblox.com/v1/games/{place_id}/private-servers"
        )) {
            Ok(url) => url,
            Err(_) => {
                return Err(private_server_lookup_error());
            }
        };
        if let Some(cursor) = &cursor {
            url.query_pairs_mut()
                .append_pair("exclusiveStartKeyCursor", cursor);
        }
        client
            .get_json::<serde_json::Value>(url.as_str(), cookie)
            .await
    })
    .await
}

async fn lookup_private_server_owner<F, Fut>(access_code: &str, mut fetch: F) -> PrivateServerCheck
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value, CoreError>>,
{
    let mut result = PrivateServerCheck {
        status: "valid",
        ..Default::default()
    };
    let mut cursor: Option<String> = None;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..10 {
        let body = match fetch(cursor.clone()).await {
            Ok(body) => body,
            Err(error) => {
                result.owner_lookup = owner_lookup_failure(&error);
                return result;
            }
        };
        if let Some((id, name)) = private_server_owner(&body, access_code) {
            result.owner_id = Some(id);
            result.owner_username = name;
            result.owner_lookup = PrivateServerOwnerLookup::Found;
            return result;
        }
        if !body.get("data").is_some_and(serde_json::Value::is_array) {
            result.owner_lookup = PrivateServerOwnerLookup::Unavailable;
            return result;
        }
        cursor = match checked_private_server_cursor(&body) {
            Ok(cursor) => cursor,
            Err(_) => {
                result.owner_lookup = PrivateServerOwnerLookup::Incomplete;
                return result;
            }
        };
        let Some(next) = &cursor else {
            result.owner_lookup = PrivateServerOwnerLookup::NotListed;
            return result;
        };
        if !seen.insert(next.clone()) {
            result.owner_lookup = PrivateServerOwnerLookup::Incomplete;
            return result;
        }
    }
    result.owner_lookup = PrivateServerOwnerLookup::Incomplete;
    result
}

pub fn is_newer_stable_release(remote: &str, local: &str) -> bool {
    fn parse(version: &str) -> Option<[u64; 3]> {
        let mut values = version.trim_start_matches('v').split('.');
        let version = [
            values.next()?.parse().ok()?,
            values.next()?.parse().ok()?,
            values.next()?.parse().ok()?,
        ];
        values.next().is_none().then_some(version)
    }
    let (local_version, is_prerelease) = match local.split_once('-') {
        Some((version, suffix)) if !suffix.is_empty() => (version, true),
        Some(_) => return false,
        None => (local, false),
    };
    matches!((parse(remote), parse(local_version)), (Some(remote), Some(local)) if remote > local || (remote == local && is_prerelease))
}

#[cfg(test)]
mod tests {
    #[test]
    fn follow_contract_never_treats_missing_relationship_or_captcha_as_success() {
        let response = serde_json::from_str::<super::FollowingResponse>(
            r#"{"followings":[{"userId":2,"isFollowing":true},{"userId":1,"isFollowing":false}]}"#,
        )
        .unwrap();
        assert!(super::following_relationship(response, 2).unwrap());
        let response =
            serde_json::from_str::<super::FollowingResponse>(r#"{"followings":[]}"#).unwrap();
        assert!(super::following_relationship(response, 2).is_err());
        for body in [
            r#"{"success":false}"#,
            r#"{"success":true,"isCaptchaRequired":true}"#,
        ] {
            assert!(super::require_follow_success(serde_json::from_str(body).unwrap()).is_err());
        }
        assert!(super::require_follow_success(
            serde_json::from_str(r#"{"success":true}"#).unwrap()
        )
        .is_ok());
        for body in ["", "<html>proxy error</html>", "{}"] {
            assert!(serde_json::from_str::<super::FollowOutcome>(body).is_err());
        }
    }
    #[test]
    fn presence_maps_user_ids_with_reordered_partial_and_unknown_rows() {
        let response: super::PresenceResponse = serde_json::from_str(r#"{"userPresences":[{"userId":2,"userPresenceType":99},{"userId":9,"userPresenceType":0},{"userId":1,"userPresenceType":2,"placeId":42},{"userId":2,"userPresenceType":0}]}"#).unwrap();
        let rows = super::map_presences(response, &[1, 2, 3]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, 2);
        assert_eq!(rows[0].1.user_presence_type, 99);
        assert_eq!(rows[1].0, 1);
        assert_eq!(rows[1].1.place_id, Some(42));
        let empty: super::PresenceResponse =
            serde_json::from_str(r#"{"userPresences":null}"#).unwrap();
        assert!(super::map_presences(empty, &[1]).is_empty());
    }
    use super::*;

    #[test]
    fn private_server_links_preserve_direct_codes_and_share_codes() {
        match parse_private_server_url("  https://www.roblox.com/games/123/Synthetic?privateServerLinkCode=synthetic-code&other=value  ").unwrap() {
            ParsedPrivateServerUrl::Direct { place_id, link_code } => {
                assert_eq!(place_id, 123);
                assert_eq!(link_code, "synthetic-code");
            }
            _ => panic!("Expected a direct private server link"),
        }
        match parse_private_server_url(
            "https://www.roblox.com/share?CoDe=synthetic-share&type=Server",
        )
        .unwrap()
        {
            ParsedPrivateServerUrl::Share { share_code } => {
                assert_eq!(share_code, "synthetic-share")
            }
            _ => panic!("Expected a share link"),
        }
        for input in [
            "",
            "https://www.roblox.com/games/123",
            "https://www.roblox.com/games/no-id?privateServerLinkCode=synthetic",
            "https://www.roblox.com/share?code=",
            "http://www.roblox.com/games/123?privateServerLinkCode=synthetic",
            "https://evil.invalid/games/123?privateServerLinkCode=synthetic",
            "https://www.roblox.com@evil.invalid/games/123?privateServerLinkCode=synthetic",
            "https://www.roblox.com/games/0?privateServerLinkCode=synthetic",
            "https://www.roblox.com/games/123?privateServerLinkCode=one&privateServerLinkCode=two",
            "https://www.roblox.com/games/123?privateServerLinkCode=synthetic%26other%3Dvalue",
        ] {
            assert_eq!(
                parse_private_server_url(input).err(),
                Some("Enter a valid Roblox private server URL")
            );
        }
    }

    #[test]
    fn share_invites_can_resolve_from_universe_without_a_place_field() {
        let response = serde_json::json!({"privateServerInviteData": {"status": "Valid", "universeId": 20, "linkCode": "synthetic"}});
        assert_eq!(
            private_share_target(&response).unwrap(),
            (None, Some(20), "synthetic".into())
        );
        let roots = serde_json::json!({"data": [{"id": 19, "rootPlaceId": 999}, {"id": 20, "rootPlaceId": 100}]});
        assert_eq!(private_share_root_place(&roots, 20).unwrap(), 100);
        assert!(private_share_root_place(&roots, 21).is_err());
        for data in [
            serde_json::json!({"status": "Unknown", "placeId": 100, "linkCode": "synthetic"}),
            serde_json::json!({"status": "Valid", "linkCode": "synthetic"}),
            serde_json::json!({"status": "Valid", "placeId": 100, "linkCode": "bad&code"}),
        ] {
            assert!(
                private_share_target(&serde_json::json!({"privateServerInviteData": data}))
                    .is_err()
            );
        }
    }

    #[test]
    fn private_invite_markup_must_match_the_requested_place_and_invitation() {
        let html = "Roblox.GameLauncher.joinPrivateGame ( 123, \"00000000-0000-4000-8000-000000000001\", \"synthetic\" )";
        assert!(private_access_code(html, 123, "synthetic").is_some());
        assert!(private_access_code(html, 124, "synthetic").is_none());
        assert!(private_access_code(html, 123, "different").is_none());
    }

    #[test]
    fn private_server_owner_is_matched_by_access_code_only() {
        let code = "00000000-0000-4000-8000-000000000001";
        let body = serde_json::json!({"data": [
            {"name": "Same name", "owner": {"id": 10, "name": "WrongOwner"}},
            {"accessCode": "00000000-0000-4000-8000-000000000002", "owner": {"id": 11}},
            {"accessCode": code, "owner": {"id": 12, "name": "SyntheticOwner", "newField": true}}
        ]});
        assert_eq!(
            private_server_owner(&body, code),
            Some((12, Some("SyntheticOwner".into())))
        );
        for body in [
            serde_json::json!({}),
            serde_json::json!({"data": null}),
            serde_json::json!({"data": [{"accessCode": code, "owner": null}]}),
        ] {
            assert_eq!(private_server_owner(&body, code), None);
        }
    }

    #[tokio::test]
    async fn profile_failures_preserve_successful_invitation_resolution() {
        let result = lookup_private_server_owner("synthetic-access", |_| async {
            Err(CoreError::RateLimited)
        })
        .await;
        assert_eq!(result.status, "valid");
        assert_eq!(result.owner_lookup, PrivateServerOwnerLookup::RateLimited);
        let result = lookup_private_server_owner("synthetic-access", |_| async {
            Ok(serde_json::json!({"data": null}))
        })
        .await;
        assert_eq!(result.status, "valid");
        assert_eq!(result.owner_lookup, PrivateServerOwnerLookup::Unavailable);
    }

    #[tokio::test]
    async fn owner_lookup_checks_later_pages_and_distinguishes_an_unlisted_account() {
        let empty = lookup_private_server_owner("synthetic-access", |_| async {
            Ok(serde_json::json!({"data": []}))
        })
        .await;
        assert_eq!(empty.owner_lookup, PrivateServerOwnerLookup::NotListed);
        let calls = std::cell::Cell::new(0);
        let found = lookup_private_server_owner("synthetic-access", |cursor| {
            calls.set(calls.get() + 1);
            async move {
                Ok(if cursor.as_deref() == Some("next/page==") {
                    serde_json::json!({"data": [{"accessCode": "synthetic-access", "owner": {"id": 10, "name": "SyntheticOwner"}}]})
                } else { serde_json::json!({"data": [], "nextPageCursor": "next/page=="}) })
            }
        }).await;
        assert_eq!(calls.get(), 2);
        assert_eq!(found.owner_id, Some(10));
        assert_eq!(found.owner_lookup, PrivateServerOwnerLookup::Found);
    }

    #[tokio::test]
    async fn repeating_or_malformed_profile_cursors_remain_incomplete() {
        for body in [
            serde_json::json!({"data": [], "nextPageCursor": "cycle"}),
            serde_json::json!({"data": [], "nextPageCursor": 1}),
        ] {
            let result = lookup_private_server_owner("synthetic-access", |_| {
                let body = body.clone();
                async move { Ok(body) }
            })
            .await;
            assert_eq!(result.status, "valid");
            assert_eq!(result.owner_lookup, PrivateServerOwnerLookup::Incomplete);
            assert!(result.owner_id.is_none());
        }
    }

    #[test]
    fn owned_server_checks_accept_empty_lists_and_match_places_or_universes() {
        let body = serde_json::json!({"data": [
            {"privateServerId": 1, "ownerId": 10, "ownerName": "Synthetic", "placeId": 100, "universeId": 200},
            {"privateServerId": 2, "ownerId": 10, "placeId": 101, "universeId": 200},
            {"privateServerId": 3, "ownerId": 10, "placeId": 102, "universeId": 300}
        ]});
        assert_eq!(
            owned_server_candidates(&body, 10, 100, Some(200)).unwrap(),
            vec![(1, Some("Synthetic".into())), (2, None)]
        );
        assert_eq!(
            owned_server_candidates(&body, 10, 100, None).unwrap(),
            vec![(1, Some("Synthetic".into()))]
        );
        assert!(
            owned_server_candidates(&serde_json::json!({"data": []}), 10, 100, Some(200))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn incomplete_owned_server_data_cannot_prove_non_ownership() {
        for body in [
            serde_json::json!({}),
            serde_json::json!({"data": null}),
            serde_json::json!({"data": [{}]}),
            serde_json::json!({"data": [{"ownerId": 11, "privateServerId": 1, "placeId": 100}]}),
            serde_json::json!({"data": [{"ownerId": 10, "placeId": 100}]}),
        ] {
            assert!(owned_server_candidates(&body, 10, 100, Some(200)).is_err());
        }
        assert_eq!(
            checked_private_server_cursor(&serde_json::json!({"nextPageCursor": "a/b=="})).unwrap(),
            Some("a/b==".into())
        );
        assert!(checked_private_server_cursor(&serde_json::json!({"nextPageCursor": 12})).is_err());
        assert!(
            checked_private_server_cursor(&serde_json::json!({"nextPageCursor": null}))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn owned_server_links_support_codes_and_urls_but_never_treat_empty_as_a_match() {
        for link in [
            "synthetic-code",
            "https://www.roblox.com/games/100?privateServerLinkCode=synthetic-code",
        ] {
            assert!(
                matches!(private_server_detail_link(link, 100).unwrap(), ParsedPrivateServerUrl::Direct { place_id: 100, ref link_code } if link_code == "synthetic-code")
            );
        }
        for link in [
            "",
            "https://untrusted.invalid/games/100?privateServerLinkCode=synthetic",
            "code&other=value",
        ] {
            assert!(private_server_detail_link(link, 100).is_err());
        }
    }

    #[test]
    fn private_server_markup_requires_a_valid_uuid_and_missing_markup_stays_unknown() {
        assert_eq!(private_access_code("Roblox.GameLauncher.joinPrivateGame(123, '00000000-0000-4000-8000-000000000001', 'synthetic')", 123, "synthetic"), Some("00000000-0000-4000-8000-000000000001".into()));
        assert!(private_access_code("<html>Unknown page</html>", 123, "synthetic").is_none());
        assert!(private_access_code(
            "Roblox.GameLauncher.joinPrivateGame(123, '------------------------------------', 'synthetic')", 123, "synthetic"
        )
        .is_none());
        assert_eq!(PrivateServerCheck::default().status, "unknown");
        assert_eq!(PrivateServerCheck::default().owner_id, None);
    }

    #[test]
    fn pending_thumbnails_are_not_cached_as_real_images() {
        let response: ThumbnailResponse = serde_json::from_str(r#"{"data":[{"targetId":1,"state":"Pending","imageUrl":"https://example.invalid/placeholder"},{"targetId":2,"state":"Completed","imageUrl":"https://example.invalid/real"}]}"#).unwrap();
        assert_eq!(
            pair_thumbnails(&[1, 2], response.data),
            vec![(2, "https://example.invalid/real".into())]
        );
    }

    fn entry(target_id: u64, image_url: Option<&str>) -> ThumbnailEntry {
        ThumbnailEntry {
            state: Some("Completed".into()),
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

    #[test]
    fn search_keywords_shorter_than_three_chars_are_rejected() {
        assert!(normalize_search_keyword("a").is_err());
        assert!(normalize_search_keyword("  a ").is_err());
        assert!(normalize_search_keyword("ab").is_err());
        assert!(normalize_search_keyword("abc").is_ok());
    }

    #[test]
    fn search_targets_are_classified_for_exact_resolution() {
        assert_eq!(classify_search_target("12345"), SearchTargetKind::UserId);
        assert_eq!(classify_search_target("alice"), SearchTargetKind::Username);
        assert_eq!(
            classify_search_target("alice smith"),
            SearchTargetKind::BroadSearch
        );
        assert_eq!(classify_search_target("a"), SearchTargetKind::BroadSearch);
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;

    #[test]
    fn release_comparison_uses_numeric_components_and_rejects_invalid_versions() {
        assert!(is_newer_stable_release("v2.10.0", "2.9.0"));
        assert!(!is_newer_stable_release("2.9.0", "2.10.0"));
        for version in ["", "2.0", "2.0.0.1", "2.0.0-beta.1", "2.0.x", "2.0.0+build"] {
            assert!(!is_newer_stable_release(version, "1.16.0"));
        }
        for version in ["", "2.0", "2.0.0.1", "2.0.x", "2.0.0+build", "2.0.0-"] {
            assert!(!is_newer_stable_release("2.0.0", version));
        }
    }

    #[test]
    fn stable_update_checks_do_not_offer_downgrades_or_prereleases() {
        assert!(is_newer_stable_release("v2.0.0", "1.16.0"));
        assert!(!is_newer_stable_release("1.15.0", "1.16.0"));
        assert!(!is_newer_stable_release("2.0.0-rc.1", "1.16.0"));
        assert!(!is_newer_stable_release("1.16.0", "1.16.0"));
        assert!(is_newer_stable_release("2.0.0", "2.0.0-beta.1"));
        assert!(is_newer_stable_release("2.0.0", "2.0.0-rc.1"));
        assert!(!is_newer_stable_release("1.16.0", "2.0.0-beta.1"));
    }
}
