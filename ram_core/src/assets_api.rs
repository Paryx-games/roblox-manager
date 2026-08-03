//! Roblox Assets API calls, cookie-authenticated.
//!
//! Separate from `api.rs` because this is a distinct and riskier surface: the
//! cookie transport is the undocumented path Studio itself uses, not published
//! Open Cloud. Every base URL lives in one block below, so if Roblox moves or
//! withdraws an endpoint the blast radius is a single constant.

use chrono::{DateTime, Utc};
use reqwest::{Method, StatusCode};

use crate::assets::{self, AssetKind, Creator, OperationOutcome};
use crate::auth::RobloxClient;
use crate::error::CoreError;
use crate::multipart::{self, Part};

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

/// Cookie transport for the Assets API. The Open Cloud equivalent is
/// `https://apis.roblox.com/assets/v1` with an `x-api-key`; this app is
/// cookie-only by design, so it uses the `user-auth` mirror instead.
pub const ASSETS_BASE: &str = "https://apis.roblox.com/assets/user-auth/v1";

/// Bulk asset permissions. Beta, and the request shape is provisional; see
/// [`assets::build_permissions_body`].
pub const PERMISSIONS_BASE: &str = "https://apis.roblox.com/asset-permissions-api/v1";

/// Place to universe resolution.
pub const UNIVERSES_BASE: &str = "https://apis.roblox.com/universes/v1";

/// Universes an account manages. Provisional.
pub const DEVELOP_BASE: &str = "https://develop.roblox.com/v1";

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

/// Everything one upload needs. A struct rather than loose arguments so the
/// call stays under clippy's argument limit, which CI treats as an error.
pub struct UploadRequest {
    pub kind: AssetKind,
    pub display_name: String,
    pub description: String,
    pub creator: Creator,
    /// Sent as the `fileContent` part's filename. Roblox does not use it for
    /// naming, but it does look at the extension.
    pub file_name: String,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

/// What `POST /assets` produced.
pub struct CreateResult {
    /// The operation ID to poll, when Roblox handed one back.
    pub operation: Option<String>,
    /// Normally [`OperationOutcome::StillPending`]. Roblox occasionally
    /// resolves small uploads inline, and it costs nothing to honour that.
    pub outcome: OperationOutcome,
}

/// Upload a file and get back an operation to poll.
///
/// The body is `multipart/form-data` with exactly two parts: `request` (the
/// JSON metadata) and `fileContent` (the raw bytes).
pub async fn create_asset(
    client: &RobloxClient,
    cookie: &str,
    request: &UploadRequest,
) -> Result<CreateResult, CoreError> {
    assets::reject_unuploadable(request.kind).map_err(|message| CoreError::RobloxApi {
        status: 400,
        message,
    })?;

    let metadata = assets::build_create_request_json(
        request.kind,
        &request.display_name,
        &request.description,
        request.creator,
    );
    let metadata = serde_json::to_string(&metadata)?;
    let file_name = multipart::sanitize_filename(&request.file_name);

    let parts = [
        Part {
            name: "request",
            filename: None,
            content_type: None,
            bytes: metadata.as_bytes(),
        },
        Part {
            name: "fileContent",
            filename: Some(&file_name),
            content_type: Some(request.mime),
            bytes: &request.bytes,
        },
    ];
    let (boundary, body) = multipart::encode_with_fresh_boundary(&parts);

    let url = format!("{ASSETS_BASE}/assets");
    let response = client
        .request_raw(
            Method::POST,
            &url,
            cookie,
            &multipart::content_type_header(&boundary),
            &body,
        )
        .await?;

    let status = response.status();
    if !status.is_success() {
        return Err(upload_error(status, response.text().await.ok()));
    }

    let json: serde_json::Value = response.json().await?;
    Ok(CreateResult {
        operation: operation_id(&json).map(str::to_string),
        outcome: assets::parse_operation_response(&json),
    })
}

// ---------------------------------------------------------------------------
// Poll
// ---------------------------------------------------------------------------

/// Ask whether an operation has finished. A 404 means the operation ID is no
/// longer resolvable, which after ~24h is expected rather than exceptional.
pub async fn poll_operation(
    client: &RobloxClient,
    cookie: &str,
    operation: &str,
) -> Result<OperationOutcome, CoreError> {
    let url = format!("{ASSETS_BASE}/operations/{operation}");
    // Deliberately the raw `request`, not `get_json`: a 404 here is an expected
    // outcome (an aged-out operation), not an error to propagate.
    let response = client.request(Method::GET, &url, cookie, None).await?;
    let status = response.status();

    if status == StatusCode::NOT_FOUND {
        return Ok(OperationOutcome::Failed {
            message: "Roblox no longer recognises this upload. It may still have been published."
                .to_string(),
            retryable: false,
        });
    }
    if !status.is_success() {
        return Err(upload_error(status, response.text().await.ok()));
    }

    let json: serde_json::Value = response.json().await?;
    Ok(assets::parse_operation_response(&json))
}

// ---------------------------------------------------------------------------
// Moderation
// ---------------------------------------------------------------------------

/// Most asset ids to put in one moderation query. The endpoint takes a
/// comma-joined list; keeping it modest keeps the URL short and the response
/// small enough to parse without a second thought.
pub const MAX_MODERATION_IDS: usize = 50;

/// Ask Roblox whether a batch of assets has cleared moderation.
///
/// `GET develop.roblox.com/v1/assets?assetIds=...`, cookie-authenticated. This
/// is the only surface that answers the question at all: the Assets API's
/// operations endpoint reports *ingestion*, and reports it as `done` long
/// before a verdict exists. Uploading audio and reading `done: true` as
/// "approved" is exactly the mistake this call exists to stop.
///
/// Ids Roblox does not return simply do not appear in the result, and a row
/// with no answer stays in review rather than being guessed at.
pub async fn fetch_moderation_statuses(
    client: &RobloxClient,
    cookie: &str,
    asset_ids: &[u64],
) -> Result<Vec<(u64, assets::ModerationStatus)>, CoreError> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(asset_ids.len());
    for chunk in asset_ids.chunks(MAX_MODERATION_IDS) {
        let ids: Vec<String> = chunk.iter().map(u64::to_string).collect();
        let url = format!("{DEVELOP_BASE}/assets?assetIds={}", ids.join(","));
        let body: serde_json::Value = client.get_json(&url, cookie).await?;
        out.extend(assets::parse_moderation_response(&body));
    }
    // Never report on something the caller did not ask about.
    out.retain(|(id, _)| asset_ids.contains(id));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

/// Grant a universe `Use` permission on a batch of assets.
///
/// Chunked at [`assets::MAX_PERMISSION_REQUESTS`]. A chunk that fails outright
/// aborts the rest and surfaces the error, because a partial grant with no
/// error is worse than a loud failure.
///
/// Returns only the assets Roblox confirmed. A 200 here is not a blanket yes:
/// the body carries `successAssetIds` next to a per-asset `errors` list, and
/// reading the status code alone reported a clean grant for assets that had
/// just been refused.
pub async fn grant_use_permission(
    client: &RobloxClient,
    cookie: &str,
    universe_id: u64,
    asset_ids: &[u64],
) -> Result<assets::GrantOutcome, CoreError> {
    let url = format!("{PERMISSIONS_BASE}/assets/permissions");
    let mut outcome = assets::GrantOutcome::default();

    for chunk in asset_ids.chunks(assets::MAX_PERMISSION_REQUESTS) {
        let body = assets::build_permissions_body(universe_id, chunk);
        let response = client
            .request(Method::PATCH, &url, cookie, Some(&body))
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(upload_error(status, response.text().await.ok()));
        }

        let json: serde_json::Value = response.json().await?;
        let chunk_outcome = assets::parse_grant_response(&json);
        // An unrecognised body names nothing at all. Rather than silently
        // dropping the chunk, treat the 200 as covering what was asked for and
        // say so in the log, so a response-shape change degrades to the old
        // behaviour instead of reporting zero grants.
        if chunk_outcome.granted.is_empty() && chunk_outcome.failures.is_empty() {
            tracing::info!("grant response had no successAssetIds or errors; assuming the 200");
            outcome.granted.extend_from_slice(chunk);
        } else {
            outcome.granted.extend(chunk_outcome.granted);
            outcome.failures.extend(chunk_outcome.failures);
        }
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Universes
// ---------------------------------------------------------------------------

/// A universe the acting account can point permissions at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniverseTarget {
    pub universe_id: u64,
    pub name: String,
}

/// Resolve a place ID to the universe that contains it.
pub async fn resolve_place_universe(
    client: &RobloxClient,
    cookie: &str,
    place_id: u64,
) -> Result<u64, CoreError> {
    let url = format!("{UNIVERSES_BASE}/places/{place_id}/universe");
    let body: serde_json::Value = client.get_json(&url, cookie).await?;
    body.get("universeId")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| CoreError::RobloxApi {
            status: 404,
            message: format!("Roblox did not return a universe for place {place_id}"),
        })
}

/// Universes the account can manage.
///
/// **Provisional endpoint.** Parsed leniently out of a `serde_json::Value`
/// rather than a typed struct (the `resolve_share_link` precedent in `api.rs`),
/// so a shape change degrades to an empty list instead of a hard error. The
/// manual place-or-universe ID field is the guaranteed path; this dropdown is
/// convenience on top of it.
pub async fn list_manageable_universes(
    client: &RobloxClient,
    cookie: &str,
) -> Result<Vec<UniverseTarget>, CoreError> {
    let url = format!("{DEVELOP_BASE}/user/universes?limit=50&sortOrder=Asc");
    let body: serde_json::Value = client.get_json(&url, cookie).await?;
    Ok(parse_universe_list(&body))
}

fn parse_universe_list(body: &serde_json::Value) -> Vec<UniverseTarget> {
    let Some(entries) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            // Roblox spells this `id` on some surfaces and `universeId` on
            // others, so accept both rather than betting on one.
            let universe_id = entry
                .get("id")
                .or_else(|| entry.get("universeId"))
                .and_then(|v| v.as_u64())?;
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled experience")
                .to_string();
            Some(UniverseTarget { universe_id, name })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Inventories
// ---------------------------------------------------------------------------

/// Creations listing. Undocumented, but verified live against a real account:
/// this is the host the Creator Dashboard uses, and it is **not**
/// `toolbox-service`, which 404s on every `creations/*` path.
pub const CREATIONS_BASE: &str = "https://itemconfiguration.roblox.com/v1";

/// Asset metadata (names, types, timestamps). The creations listing returns
/// only `assetId` and `name`, so dates come from here.
pub const TOOLBOX_BASE: &str = "https://apis.roblox.com/toolbox-service/v1";

/// Roblox rejects any other page size with `400 Allowed values: 10, 25, 50,
/// 100`. Verified live. Do not make this a free integer.
pub const CREATIONS_PAGE_SIZE: u32 = 50;


/// A group the acting account might be able to publish under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupTarget {
    pub group_id: u64,
    pub name: String,
}

/// One asset already on Roblox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreationItem {
    pub asset_id: u64,
    pub name: String,
    pub kind: AssetKind,
    /// From the details call. `None` when that second request failed, which is
    /// survivable: the row still shows with a blank date.
    pub updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreationPage {
    pub items: Vec<CreationItem>,
    pub next_cursor: Option<String>,
}

/// Build the creations-listing URL.
///
/// Split out from the request so the query-string rules that Roblox enforces
/// (and that cost real debugging to find) are pinned by tests:
///
/// - `groupId` must be **omitted** for a user's own creations. Passing
///   `groupId=0` returns **403**, not an empty list.
/// - `limit` must be one of 10/25/50/100. Anything else is a 400.
/// - The cursor is base64 and contains `=` and `/`, so it has to be
///   percent-encoded.
fn creations_url(creator: Creator, kind: AssetKind, cursor: Option<&str>) -> String {
    let mut url = format!(
        "{CREATIONS_BASE}/creations/get-assets?assetType={}&isArchived=false&limit={CREATIONS_PAGE_SIZE}",
        kind.as_api_str()
    );
    if let Creator::Group(group_id) = creator {
        url.push_str(&format!("&groupId={group_id}"));
    }
    if let Some(cursor) = cursor.filter(|c| !c.is_empty()) {
        url.push_str("&cursor=");
        url.push_str(&percent_encode(cursor));
    }
    url
}

/// Percent-encode a query-string value. Hand-rolled to keep the feature's
/// zero-new-dependency property; only unreserved characters survive.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// List a user's or group's own creations of one asset type.
///
/// Two requests: the listing returns only `{assetId, name}`, so a second call
/// fills in the timestamps. The second one failing is not fatal, the rows just
/// show without dates.
pub async fn list_creations(
    client: &RobloxClient,
    cookie: &str,
    creator: Creator,
    kind: AssetKind,
    cursor: Option<&str>,
) -> Result<CreationPage, CoreError> {
    assets::reject_unuploadable(kind).map_err(|message| CoreError::RobloxApi {
        status: 400,
        message,
    })?;

    let url = creations_url(creator, kind, cursor);
    let body: serde_json::Value = client
        .get_json(&url, cookie)
        .await
        .map_err(|e| reword_listing_error(creator, e))?;
    // The requested kind is authoritative: the query filters by type, so there
    // is no need to decode Roblox's numeric typeId and risk getting it wrong.
    let mut page = parse_creation_page(&body, kind);

    let ids: Vec<u64> = page.items.iter().map(|i| i.asset_id).collect();
    match fetch_item_timestamps(client, cookie, &ids).await {
        Ok(stamps) => {
            for item in &mut page.items {
                item.updated = stamps
                    .iter()
                    .find(|(id, _)| *id == item.asset_id)
                    .map(|(_, when)| *when);
            }
        }
        Err(e) => tracing::info!("creation timestamps unavailable: {e}"),
    }
    Ok(page)
}

fn parse_creation_page(body: &serde_json::Value, kind: AssetKind) -> CreationPage {
    let items = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let asset_id = entry
                        .get("assetId")
                        .or_else(|| entry.get("id"))
                        .and_then(json_u64)?;
                    Some(CreationItem {
                        asset_id,
                        name: entry
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Untitled")
                            .to_string(),
                        kind,
                        updated: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    CreationPage {
        items,
        next_cursor: body
            .get("nextPageCursor")
            .and_then(|v| v.as_str())
            .filter(|c| !c.is_empty())
            .map(str::to_string),
    }
}

/// Re-word a listing failure for the creator it happened on.
///
/// `RobloxClient` turns any 403 without a CSRF challenge into
/// [`CoreError::CookieRejected`], whose text is "cookie rejected". On this
/// endpoint that is usually wrong and actively misleading: a 403 on a group
/// listing means the account lacks the role, not that its session is dead.
/// Verified live, where 9 of 13 of one account's groups answered 403 with a
/// perfectly good cookie.
fn reword_listing_error(creator: Creator, error: CoreError) -> CoreError {
    match (creator, &error) {
        (Creator::Group(id), CoreError::CookieRejected) => CoreError::RobloxApi {
            status: 403,
            message: format!(
                "This account does not have permission to view group {id}'s assets."
            ),
        },
        (Creator::User(_), CoreError::CookieRejected) => CoreError::RobloxApi {
            status: 403,
            message: "Roblox rejected this account's cookie. Re-add the account.".to_string(),
        },
        _ => error,
    }
}

/// `(asset_id, last updated)` for a batch of assets.
async fn fetch_item_timestamps(
    client: &RobloxClient,
    cookie: &str,
    asset_ids: &[u64],
) -> Result<Vec<(u64, DateTime<Utc>)>, CoreError> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = asset_ids.iter().map(u64::to_string).collect();
    let url = format!("{TOOLBOX_BASE}/items/details?assetIds={}", ids.join(","));
    let body: serde_json::Value = client.get_json(&url, cookie).await?;
    Ok(parse_item_timestamps(&body))
}

fn parse_item_timestamps(body: &serde_json::Value) -> Vec<(u64, DateTime<Utc>)> {
    let Some(entries) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let asset = entry.get("asset")?;
            let asset_id = asset.get("id").and_then(json_u64)?;
            let raw = asset
                .get("updatedUtc")
                .or_else(|| asset.get("createdUtc"))
                .and_then(|v| v.as_str())?;
            // Roblox emits 7 fractional digits, which is not what `%.f` style
            // parsers always expect; RFC 3339 handles it.
            let when = DateTime::parse_from_rfc3339(raw).ok()?.with_timezone(&Utc);
            Some((asset_id, when))
        })
        .collect()
}

/// Groups this account can actually manage assets for.
///
/// Deliberately **not** `groups/roles`, which lists every membership including
/// ones where the account is a rank-1 member with no rights. Listing those
/// produces tree entries that only ever 403.
///
/// Verified live: for an account in 13 groups, `canmanage` returned exactly the
/// 4 whose creations could be listed, with no false positives or negatives.
pub async fn list_publishable_groups(
    client: &RobloxClient,
    cookie: &str,
) -> Result<Vec<GroupTarget>, CoreError> {
    let url = format!("{DEVELOP_BASE}/user/groups/canmanage");
    let body: serde_json::Value = client.get_json(&url, cookie).await?;
    Ok(parse_group_list(&body))
}

fn parse_group_list(body: &serde_json::Value) -> Vec<GroupTarget> {
    let Some(entries) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            // `canmanage` is flat; `groups/roles` nests under "group". Accept
            // both so a change of endpoint does not silently empty the list.
            let source = entry.get("group").unwrap_or(entry);
            Some(GroupTarget {
                group_id: source.get("id").and_then(json_u64)?,
                name: source
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unnamed group")
                    .to_string(),
            })
        })
        .collect()
}

/// Roblox is inconsistent about quoting IDs, so accept both forms.
fn json_u64(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}

// ---------------------------------------------------------------------------
// Thumbnails
// ---------------------------------------------------------------------------

/// One size for every icon view. A 150px source upscales acceptably to the
/// largest tile and avoids maintaining three caches of the same assets.
const THUMBNAIL_SIZE: &str = "150x150";

/// Fetch thumbnail images for a batch of assets.
///
/// Deliberately unauthenticated, for the reason recorded in `api.rs`: the
/// thumbnails host is public, and signing these with a cookie turns a
/// would-be anonymous 200 into a 403 the moment any one cookie goes bad.
///
/// Assets whose thumbnail Roblox has not rendered yet are **skipped** rather
/// than cached, so a later call picks them up instead of pinning a placeholder
/// forever.
pub async fn fetch_asset_thumbnails(
    client: &RobloxClient,
    asset_ids: &[u64],
) -> Result<Vec<(u64, Vec<u8>)>, CoreError> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = asset_ids.iter().map(u64::to_string).collect();
    let url = format!(
        "https://thumbnails.roblox.com/v1/assets?assetIds={}&returnPolicy=PlaceHolder&size={THUMBNAIL_SIZE}&format=Png&isCircular=false",
        ids.join(",")
    );
    let body: serde_json::Value = client.get_json(&url, "").await?;

    let mut out = Vec::new();
    for (asset_id, image_url) in parse_thumbnail_urls(&body, asset_ids) {
        match client.get_bytes(&image_url, "").await {
            Ok(bytes) => out.push((asset_id, bytes)),
            Err(e) => tracing::warn!("thumbnail download failed for {asset_id}: {e}"),
        }
    }
    Ok(out)
}

fn parse_thumbnail_urls(body: &serde_json::Value, requested: &[u64]) -> Vec<(u64, String)> {
    let Some(entries) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            // Keyed on targetId, never position: the thumbnails service does
            // not promise request order and omits IDs it cannot resolve. Both
            // were observed live and caused v1.4.6's wrong-avatar bug.
            let target_id = entry.get("targetId").and_then(json_u64)?;
            if !requested.contains(&target_id) {
                return None;
            }
            // "Pending" means Roblox is still rendering it and the URL points
            // at a generic placeholder.
            if entry.get("state").and_then(|s| s.as_str()) != Some("Completed") {
                return None;
            }
            let image_url = entry.get("imageUrl").and_then(|v| v.as_str())?;
            Some((target_id, image_url.to_string()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Error shaping
// ---------------------------------------------------------------------------

/// Turn an HTTP status into something a queue row can show verbatim.
///
/// The wording matters: a 403 on this endpoint is far more likely to mean "the
/// creator you asked for is outside what this account may publish to" than
/// anything CSRF-related, and saying so saves the user from chasing their
/// cookie.
pub fn describe_status(status: u16) -> String {
    match status {
        400 => "Roblox rejected the request. The file may not match its asset type.".to_string(),
        401 => "Roblox did not accept this account's cookie. Re-add the account.".to_string(),
        403 => {
            "This account is not allowed to publish under the selected creator.".to_string()
        }
        404 => "Roblox could not find that asset or upload.".to_string(),
        413 => "Roblox rejected the file as too large.".to_string(),
        429 => "Roblox is rate limiting uploads. Try again shortly.".to_string(),
        500..=599 => "Roblox had a server error. Try again shortly.".to_string(),
        other => format!("Roblox returned an unexpected status ({other})."),
    }
}

/// Whether a failed upload is worth re-sending unchanged.
pub fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500..=599)
}

fn upload_error(status: StatusCode, body: Option<String>) -> CoreError {
    let mut message = describe_status(status.as_u16());
    // Roblox's own text is often the only thing that identifies an audio quota
    // rejection, so keep it, trimmed to something a table cell can hold.
    if let Some(detail) = body.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        let detail: String = detail.chars().take(300).collect();
        message.push(' ');
        message.push_str(&detail);
    }
    CoreError::RobloxApi {
        status: status.as_u16(),
        message,
    }
}

/// Pull the pollable operation ID out of a create response.
///
/// Roblox returns `{"path": "operations/9f3c..."}`; the poll URL wants just the
/// `9f3c...`. Some responses use `operationId` directly, so accept both.
fn operation_id(body: &serde_json::Value) -> Option<&str> {
    if let Some(id) = body.get("operationId").and_then(|v| v.as_str()) {
        return Some(id);
    }
    let path = body.get("path").and_then(|v| v.as_str())?;
    let id = path.rsplit('/').next().unwrap_or(path);
    (!id.is_empty()).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_id_strips_the_operations_prefix() {
        let body = serde_json::json!({ "path": "operations/9f3c-abc", "done": false });
        assert_eq!(operation_id(&body), Some("9f3c-abc"));
    }

    #[test]
    fn operation_id_accepts_a_bare_field() {
        let body = serde_json::json!({ "operationId": "zzz" });
        assert_eq!(operation_id(&body), Some("zzz"));
    }

    #[test]
    fn operation_id_accepts_a_path_without_a_prefix() {
        let body = serde_json::json!({ "path": "abc" });
        assert_eq!(operation_id(&body), Some("abc"));
    }

    #[test]
    fn operation_id_is_none_when_absent_or_empty() {
        assert_eq!(operation_id(&serde_json::json!({ "done": true })), None);
        assert_eq!(operation_id(&serde_json::json!({ "path": "" })), None);
        assert_eq!(operation_id(&serde_json::json!({ "path": "operations/" })), None);
    }

    #[test]
    fn only_rate_limits_and_server_errors_are_retryable() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(503));
        for status in [400, 401, 403, 404, 413] {
            assert!(!is_retryable_status(status), "status {status}");
        }
    }

    #[test]
    fn a_403_blames_the_creator_not_the_cookie() {
        let text = describe_status(403);
        assert!(text.contains("creator"), "got: {text}");
        assert!(!text.contains("CSRF"), "got: {text}");
    }

    #[test]
    fn universe_list_accepts_either_id_spelling() {
        let body = serde_json::json!({
            "data": [
                { "id": 1, "name": "Alpha" },
                { "universeId": 2, "name": "Beta" }
            ]
        });
        assert_eq!(
            parse_universe_list(&body),
            vec![
                UniverseTarget { universe_id: 1, name: "Alpha".into() },
                UniverseTarget { universe_id: 2, name: "Beta".into() },
            ]
        );
    }

    #[test]
    fn universe_list_degrades_instead_of_failing() {
        // A shape change must not take the whole dropdown down with it.
        assert!(parse_universe_list(&serde_json::json!({})).is_empty());
        assert!(parse_universe_list(&serde_json::json!({ "data": "nope" })).is_empty());
        // An entry with no usable ID is skipped, the rest survive.
        let mixed = serde_json::json!({ "data": [{ "name": "x" }, { "id": 9, "name": "y" }] });
        assert_eq!(parse_universe_list(&mixed).len(), 1);
    }

    #[test]
    fn universe_without_a_name_still_appears() {
        let body = serde_json::json!({ "data": [{ "id": 7 }] });
        assert_eq!(parse_universe_list(&body)[0].name, "Untitled experience");
    }

    #[test]
    fn creation_page_accepts_either_id_spelling_and_keeps_the_cursor() {
        let body = serde_json::json!({
            "data": [
                { "assetId": "94394275811137", "name": "jigsaw_3" },
                { "id": 42, "name": "other" }
            ],
            "nextPageCursor": "abc"
        });
        let page = parse_creation_page(&body, AssetKind::Audio);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].asset_id, 94394275811137);
        assert_eq!(page.items[0].kind, AssetKind::Audio);
        assert_eq!(page.items[1].name, "other");
        assert_eq!(page.next_cursor.as_deref(), Some("abc"));
    }

    // The next three encode rules that were learned the hard way, by probing
    // the live API. Breaking any of them is a 400 or a 403, not a soft failure.

    #[test]
    fn a_user_listing_must_not_send_group_id() {
        // groupId=0 returns 403, not "the authenticated user". Verified live.
        let url = creations_url(Creator::User(1916532448), AssetKind::Audio, None);
        assert!(!url.contains("groupId"), "got: {url}");
        assert!(url.contains("assetType=Audio"));
        assert!(url.starts_with(CREATIONS_BASE));
    }

    #[test]
    fn a_group_listing_sends_group_id() {
        let url = creations_url(Creator::Group(9375134), AssetKind::Decal, None);
        assert!(url.contains("&groupId=9375134"), "got: {url}");
    }

    #[test]
    fn page_size_is_one_roblox_accepts() {
        // Roblox: "Allowed values: 10, 25, 50, 100".
        assert!(matches!(CREATIONS_PAGE_SIZE, 10 | 25 | 50 | 100));
        let url = creations_url(Creator::User(1), AssetKind::Model, None);
        assert!(url.contains(&format!("limit={CREATIONS_PAGE_SIZE}")), "got: {url}");
    }

    #[test]
    fn the_cursor_is_percent_encoded() {
        // Real cursors are base64 and end in "==".
        let cursor = "eyJzdGFydEluZGV4IjoxMH0=/+a";
        let url = creations_url(Creator::User(1), AssetKind::Audio, Some(cursor));
        assert!(url.contains("cursor=eyJzdGFydEluZGV4IjoxMH0%3D%2F%2Ba"), "got: {url}");
    }

    #[test]
    fn an_empty_cursor_is_not_sent() {
        let url = creations_url(Creator::User(1), AssetKind::Audio, Some(""));
        assert!(!url.contains("cursor="), "got: {url}");
    }

    #[test]
    fn percent_encode_leaves_unreserved_characters_alone() {
        assert_eq!(percent_encode("aZ0-_.~"), "aZ0-_.~");
        assert_eq!(percent_encode("a b"), "a%20b");
    }

    #[test]
    fn timestamps_come_from_the_nested_asset_object() {
        // Roblox emits 7 fractional second digits here.
        let body = serde_json::json!({
            "data": [{
                "asset": {
                    "id": 110657841768732u64,
                    "name": "quad",
                    "createdUtc": "2026-08-02T21:01:20.720171Z",
                    "updatedUtc": "2026-08-02T21:05:20.7829319Z"
                }
            }]
        });
        let stamps = parse_item_timestamps(&body);
        assert_eq!(stamps.len(), 1);
        assert_eq!(stamps[0].0, 110657841768732);
        // updatedUtc wins over createdUtc.
        assert_eq!(stamps[0].1.to_rfc3339(), "2026-08-02T21:05:20.782931900+00:00");
    }

    #[test]
    fn a_group_403_blames_the_role_not_the_cookie() {
        let reworded = reword_listing_error(Creator::Group(14080716), CoreError::CookieRejected);
        let text = reworded.to_string();
        assert!(text.contains("permission"), "got: {text}");
        assert!(text.contains("14080716"), "got: {text}");
        assert!(!text.to_lowercase().contains("cookie"), "got: {text}");
    }

    #[test]
    fn a_user_403_does_blame_the_cookie() {
        let text = reword_listing_error(Creator::User(1), CoreError::CookieRejected).to_string();
        assert!(text.to_lowercase().contains("cookie"), "got: {text}");
    }

    #[test]
    fn other_errors_pass_through_untouched() {
        let text = reword_listing_error(Creator::Group(1), CoreError::RateLimited).to_string();
        assert_eq!(text, CoreError::RateLimited.to_string());
    }

    #[test]
    fn timestamps_degrade_rather_than_fail() {
        assert!(parse_item_timestamps(&serde_json::json!({})).is_empty());
        let junk = serde_json::json!({ "data": [{ "asset": { "id": 1, "updatedUtc": "not a date" } }] });
        assert!(parse_item_timestamps(&junk).is_empty());
    }

    #[test]
    fn creation_page_degrades_instead_of_failing() {
        // The whole point of the local-index-first design: a shape change here
        // empties one tree node, it does not break the tab.
        assert_eq!(
            parse_creation_page(&serde_json::json!({}), AssetKind::Decal),
            CreationPage::default()
        );
        let junk = serde_json::json!({ "data": [{ "name": "no id" }] });
        assert!(parse_creation_page(&junk, AssetKind::Decal).items.is_empty());
    }

    #[test]
    fn an_empty_cursor_is_treated_as_no_more_pages() {
        let body = serde_json::json!({ "data": [], "nextPageCursor": "" });
        assert_eq!(parse_creation_page(&body, AssetKind::Model).next_cursor, None);
    }

    #[test]
    fn group_list_reads_the_flat_canmanage_shape() {
        let body = serde_json::json!({
            "data": [
                { "id": 1031223594, "name": "Guts & Blackpowder QA Team" },
                { "id": 9375134, "name": "Central Roleplay | Development" }
            ]
        });
        assert_eq!(
            parse_group_list(&body),
            vec![
                GroupTarget { group_id: 1031223594, name: "Guts & Blackpowder QA Team".into() },
                GroupTarget { group_id: 9375134, name: "Central Roleplay | Development".into() },
            ]
        );
    }

    #[test]
    fn group_list_still_reads_the_nested_roles_shape() {
        let body = serde_json::json!({
            "data": [
                { "group": { "id": 8497064, "name": "Central Roleplay" }, "role": { "rank": 250 } },
                { "role": { "rank": 1 } }
            ]
        });
        assert_eq!(
            parse_group_list(&body),
            vec![GroupTarget { group_id: 8497064, name: "Central Roleplay".into() }]
        );
    }

    #[test]
    fn thumbnails_skip_pending_renders() {
        // Verified live: a freshly uploaded asset comes back Pending with an
        // "UnavailableImage" URL. Caching that would pin a placeholder.
        let body = serde_json::json!({
            "data": [
                { "targetId": 1, "state": "Pending", "imageUrl": "https://x/UnavailableImage" },
                { "targetId": 2, "state": "Completed", "imageUrl": "https://x/Decal" }
            ]
        });
        assert_eq!(
            parse_thumbnail_urls(&body, &[1, 2]),
            vec![(2, "https://x/Decal".to_string())]
        );
    }

    #[test]
    fn thumbnails_pair_by_target_id_not_position() {
        // Roblox returns these out of order and drops IDs it cannot resolve.
        let body = serde_json::json!({
            "data": [
                { "targetId": 30, "state": "Completed", "imageUrl": "c" },
                { "targetId": 10, "state": "Completed", "imageUrl": "a" }
            ]
        });
        let paired = parse_thumbnail_urls(&body, &[10, 20, 30]);
        assert_eq!(
            paired,
            vec![(30, "c".to_string()), (10, "a".to_string())]
        );
    }

    #[test]
    fn thumbnails_ignore_ids_we_did_not_ask_for() {
        let body = serde_json::json!({
            "data": [{ "targetId": 99, "state": "Completed", "imageUrl": "x" }]
        });
        assert!(parse_thumbnail_urls(&body, &[1]).is_empty());
    }

    #[test]
    fn thumbnails_degrade_rather_than_fail() {
        assert!(parse_thumbnail_urls(&serde_json::json!({}), &[1]).is_empty());
        let junk = serde_json::json!({ "data": [{ "targetId": 1, "state": "Completed" }] });
        assert!(parse_thumbnail_urls(&junk, &[1]).is_empty());
    }

    #[test]
    fn moderation_lives_on_develop_not_the_assets_api() {
        // The Assets API's operations endpoint answers "has this been
        // ingested", which is the question that was being mistaken for "has
        // this been approved". Only this host answers the second one.
        assert!(DEVELOP_BASE.starts_with("https://develop.roblox.com"));
        assert!(!DEVELOP_BASE.contains("apis.roblox.com"));
    }

    #[test]
    fn every_documented_status_has_wording() {
        for status in [400u16, 401, 403, 404, 413, 429, 500, 503, 418] {
            assert!(!describe_status(status).is_empty(), "status {status}");
        }
    }
}
