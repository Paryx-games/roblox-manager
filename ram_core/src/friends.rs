//! Authenticated friend browsing and explicit relationship actions.
use crate::{api, auth::RobloxClient, cached_api, models::Presence, CoreError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

fn null_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendUser {
    #[serde(rename = "userId", alias = "id")]
    pub user_id: u64,
    #[serde(default, alias = "name", deserialize_with = "null_string")]
    pub username: String,
    #[serde(default, deserialize_with = "null_string")]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FriendStatus {
    Playing,
    Studio,
    Online,
    Offline,
    Unknown,
}

pub fn classify(presence: Option<&Presence>) -> FriendStatus {
    match presence.map(|presence| presence.user_presence_type) {
        Some(0) => FriendStatus::Offline,
        Some(1) => FriendStatus::Online,
        Some(2) => FriendStatus::Playing,
        Some(3) => FriendStatus::Studio,
        _ => FriendStatus::Unknown,
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Friend {
    #[serde(flatten)]
    pub user: FriendUser,
    pub status: FriendStatus,
    pub activity: String,
    pub place_id: Option<u64>,
    pub job_id: Option<String>,
    pub game_name: Option<String>,
    pub place_name: Option<String>,
    pub game_icon: Option<String>,
}

pub fn map_friend(user: FriendUser, presence: Option<&Presence>) -> Friend {
    let status = classify(presence);
    let playing = status == FriendStatus::Playing;
    let in_place = playing || status == FriendStatus::Studio;
    Friend {
        user,
        status,
        activity: presence
            .filter(|_| !matches!(status, FriendStatus::Offline | FriendStatus::Unknown))
            .map(|value| value.last_location.clone())
            .unwrap_or_default(),
        place_id: presence
            .filter(|_| in_place)
            .and_then(|value| value.place_id)
            .filter(|id| *id > 0),
        job_id: presence
            .filter(|_| playing)
            .and_then(crate::session_history::server)
            .map(|(_, job)| job),
        game_name: None,
        place_name: None,
        game_icon: None,
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendsSnapshot {
    pub friends: Vec<Friend>,
    pub refreshed_at: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsersPage {
    data: Vec<FriendUser>,
    #[serde(default)]
    next_page_cursor: Option<String>,
}

pub fn error_message(error: &CoreError) -> String {
    match error {
        CoreError::RateLimited => "Roblox rate-limited this request. Wait before refreshing or trying again.".into(),
        CoreError::CookieRejected | CoreError::CookieRejectedWithReason(_) => "Roblox denied this request. Check the account login, privacy permissions or an interactive Roblox challenge.".into(),
        CoreError::RobloxApi { status: 401, .. } => "This account could not authenticate. Log in again from Accounts.".into(),
        CoreError::RobloxApi { status: 403, .. } => "Roblox did not allow this action. Check privacy settings and account restrictions.".into(),
        _ => "Roblox data could not be loaded or the action could not finish. Check your connection and retry.".into(),
    }
}

fn incomplete() -> CoreError {
    CoreError::RobloxApi {
        status: 502,
        message: "Roblox returned incomplete friend data".into(),
    }
}

fn next_url(base: &str, cursor: Option<&str>) -> Result<String, CoreError> {
    let mut url = reqwest::Url::parse(base).map_err(|_| incomplete())?;
    if let Some(cursor) = cursor {
        if cursor.len() > 4096 {
            return Err(incomplete());
        }
        url.query_pairs_mut().append_pair("cursor", cursor);
    }
    Ok(url.to_string())
}

fn unique_users(users: Vec<FriendUser>) -> Vec<FriendUser> {
    let mut seen = HashSet::new();
    users
        .into_iter()
        .filter(|user| user.user_id > 0 && seen.insert(user.user_id))
        .collect()
}

pub async fn fetch_users(
    client: &RobloxClient,
    cookie: &str,
    user_id: u64,
) -> Result<Vec<FriendUser>, CoreError> {
    let base = format!("https://friends.roblox.com/v1/users/{user_id}/friends");
    collect_users(client, cookie, &base).await
}

/// Complete request IDs for background change detection, without avatar or presence work.
pub async fn fetch_request_users(
    client: &RobloxClient,
    cookie: &str,
) -> Result<Vec<FriendUser>, CoreError> {
    collect_users(
        client,
        cookie,
        "https://friends.roblox.com/v1/my/friends/requests?limit=100",
    )
    .await
}

async fn collect_users(
    client: &RobloxClient,
    cookie: &str,
    base: &str,
) -> Result<Vec<FriendUser>, CoreError> {
    let mut cursor = None;
    let mut seen = HashSet::new();
    let mut users = Vec::new();
    for _ in 0..40 {
        let page: UsersPage = client
            .get_json(&next_url(base, cursor.as_deref())?, cookie)
            .await?;
        users.extend(page.data);
        if users.len() > 2000 {
            return Err(incomplete());
        }
        match page.next_page_cursor.filter(|cursor| !cursor.is_empty()) {
            None => return Ok(unique_users(users)),
            Some(next) if seen.insert(next.clone()) => cursor = Some(next),
            _ => return Err(incomplete()),
        }
    }
    Err(incomplete())
}

async fn fill_users(
    client: &RobloxClient,
    cookie: &str,
    users: &mut [FriendUser],
    warnings: &mut Vec<String>,
) {
    let missing: Vec<_> = users
        .iter()
        .filter(|user| user.username.is_empty())
        .map(|user| user.user_id)
        .collect();
    for batch in missing.chunks(100) {
        let body = serde_json::json!({"userIds": batch, "excludeBannedUsers": false});
        match client
            .post_json::<UsersPage>("https://users.roblox.com/v1/users", cookie, Some(&body))
            .await
        {
            Ok(page) => {
                for result in page.data {
                    if let Some(user) = users.iter_mut().find(|user| user.user_id == result.user_id)
                    {
                        user.username = result.username;
                        user.display_name = result.display_name;
                    }
                }
            }
            Err(_) => {
                warnings.push("Some usernames are unavailable. User IDs are still shown.".into());
                break;
            }
        }
    }
    let ids: Vec<_> = users.iter().map(|user| user.user_id).collect();
    match cached_api::fetch_avatars(client, &ids).await {
        Ok(avatars) => {
            let avatars: HashMap<_, _> = avatars.into_iter().collect();
            for user in users {
                user.avatar_url = avatars.get(&user.user_id).cloned().unwrap_or_default();
            }
        }
        Err(_) => warnings.push("Avatars could not be refreshed.".into()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceDetails {
    place_id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    source_name: Option<String>,
    #[serde(default)]
    universe_id: Option<u64>,
}

async fn fill_games(
    client: &RobloxClient,
    cookie: &str,
    friends: &mut [Friend],
    warnings: &mut Vec<String>,
) {
    let places: Vec<_> = friends
        .iter()
        .filter_map(|friend| friend.place_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    for batch in places.chunks(50) {
        let ids = batch
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let details: Vec<PlaceDetails> = match client
            .get_json(
                &format!("https://games.roblox.com/v1/games/multiget-place-details?placeIds={ids}"),
                cookie,
            )
            .await
        {
            Ok(details) => details,
            Err(_) => {
                warnings.push("Some game details are unavailable. Presence and visible server IDs are still shown.".into());
                break;
            }
        };
        let universes: Vec<_> = details
            .iter()
            .filter_map(|place| place.universe_id)
            .filter(|id| *id > 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let names: HashMap<_, _> = match cached_api::fetch_universe_names(client, &universes).await
        {
            Ok(names) => names.into_iter().collect(),
            Err(_) => {
                warnings.push("Some game names are unavailable.".into());
                HashMap::new()
            }
        };
        let icons = cached_api::fetch_game_icons(client, cookie, &universes).await;
        let icons: HashMap<_, _> = match icons {
            Ok(icons) => icons.into_iter().collect(),
            Err(_) => {
                warnings.push("Game thumbnails could not be refreshed.".into());
                HashMap::new()
            }
        };
        for friend in friends.iter_mut() {
            if let Some(place) = details
                .iter()
                .find(|place| Some(place.place_id) == friend.place_id)
            {
                friend.place_name = Some(
                    place
                        .source_name
                        .as_deref()
                        .filter(|name| !name.is_empty())
                        .unwrap_or(&place.name)
                        .to_string(),
                )
                .filter(|name| !name.is_empty());
                friend.game_name = place.universe_id.and_then(|id| names.get(&id)).cloned();
                friend.game_icon = place.universe_id.and_then(|id| icons.get(&id)).cloned();
            }
        }
    }
}

pub async fn fetch_snapshot(
    client: &RobloxClient,
    cookie: &str,
    user_id: u64,
) -> Result<FriendsSnapshot, CoreError> {
    let mut users = fetch_users(client, cookie, user_id).await?;
    let mut warnings = Vec::new();
    fill_users(client, cookie, &mut users, &mut warnings).await;
    let ids: Vec<_> = users.iter().map(|user| user.user_id).collect();
    let mut presences = HashMap::new();
    let mut presence_available = true;
    for batch in ids.chunks(50) {
        match api::fetch_presences(client, cookie, batch).await {
            Ok(batch) => presences.extend(batch),
            Err(error) => {
                warnings.push(format!("Presence is incomplete. {}", error_message(&error)));
                presence_available = false;
                break;
            }
        }
    }
    if presences.len() != ids.len() {
        presence_available = false;
        warnings.push("Roblox did not return every friend's presence. Missing states are shown as unavailable.".into());
    }
    let mut friends: Vec<_> = users
        .into_iter()
        .map(|user| {
            let presence = presences.get(&user.user_id);
            map_friend(user, presence)
        })
        .collect();
    fill_games(client, cookie, &mut friends, &mut warnings).await;
    Ok(FriendsSnapshot {
        friends,
        refreshed_at: presence_available.then(|| chrono::Utc::now().to_rfc3339()),
        warnings,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestsPage {
    pub users: Vec<FriendUser>,
    pub next_cursor: Option<String>,
    pub warnings: Vec<String>,
}

pub async fn fetch_requests(
    client: &RobloxClient,
    cookie: &str,
    cursor: Option<&str>,
) -> Result<RequestsPage, CoreError> {
    let page: UsersPage = client
        .get_json(
            &next_url(
                "https://friends.roblox.com/v1/my/friends/requests?limit=100",
                cursor,
            )?,
            cookie,
        )
        .await?;
    let mut users = unique_users(page.data);
    let mut warnings = Vec::new();
    fill_users(client, cookie, &mut users, &mut warnings).await;
    Ok(RequestsPage {
        users,
        next_cursor: page.next_page_cursor.filter(|cursor| !cursor.is_empty()),
        warnings,
    })
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FriendAction {
    Accept,
    Decline,
    Remove,
    Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipChange {
    RequestReceived(u64),
    Added(u64),
    Removed(u64),
}

#[derive(Default)]
pub struct RelationshipTracker {
    friends: Option<HashSet<u64>>,
    requests: Option<HashSet<u64>>,
}

impl RelationshipTracker {
    /// Only complete, successful snapshots may advance a baseline.
    pub fn observe(
        &mut self,
        friends: Option<&[FriendUser]>,
        requests: Option<&[FriendUser]>,
    ) -> Vec<RelationshipChange> {
        let mut changes = Vec::new();
        if let Some(users) = friends {
            let next: HashSet<_> = users
                .iter()
                .map(|user| user.user_id)
                .filter(|id| *id > 0)
                .collect();
            if let Some(previous) = &self.friends {
                changes.extend(
                    next.difference(previous)
                        .copied()
                        .map(RelationshipChange::Added),
                );
                changes.extend(
                    previous
                        .difference(&next)
                        .copied()
                        .map(RelationshipChange::Removed),
                );
            }
            self.friends = Some(next);
        }
        if let Some(users) = requests {
            let next: HashSet<_> = users
                .iter()
                .map(|user| user.user_id)
                .filter(|id| *id > 0)
                .collect();
            if let Some(previous) = &self.requests {
                changes.extend(
                    next.difference(previous)
                        .copied()
                        .map(RelationshipChange::RequestReceived),
                );
            }
            self.requests = Some(next);
        }
        changes.sort_by_key(|change| match change {
            RelationshipChange::RequestReceived(id) => (0, *id),
            RelationshipChange::Added(id) => (1, *id),
            RelationshipChange::Removed(id) => (2, *id),
        });
        changes
    }

    pub fn retain_sources(&mut self, friends: bool, requests: bool) {
        if !friends {
            self.friends = None;
        }
        if !requests {
            self.requests = None;
        }
    }

    pub fn record_action(&mut self, target: u64, action: FriendAction) {
        if let Some(requests) = &mut self.requests {
            requests.remove(&target);
        }
        if let Some(friends) = &mut self.friends {
            match action {
                FriendAction::Accept => {
                    friends.insert(target);
                }
                FriendAction::Remove | FriendAction::Block => {
                    friends.remove(&target);
                }
                FriendAction::Decline => {}
            }
        }
    }
}

pub async fn act(
    client: &RobloxClient,
    cookie: &str,
    target: u64,
    action: FriendAction,
) -> Result<(), CoreError> {
    if target == 0 {
        return Err(incomplete());
    }
    if matches!(action, FriendAction::Block) {
        return api::block_user(client, cookie, target).await;
    }
    let route = match action {
        FriendAction::Accept => "accept-friend-request",
        FriendAction::Decline => "decline-friend-request",
        FriendAction::Remove => "unfriend",
        FriendAction::Block => unreachable!(),
    };
    let response = client
        .request(
            reqwest::Method::POST,
            &format!("https://friends.roblox.com/v1/users/{target}/{route}"),
            cookie,
            Some(&serde_json::json!({})),
        )
        .await?;
    if !response.status().is_success() {
        return Err(CoreError::RobloxApi {
            status: response.status().as_u16(),
            message: "Friend action was rejected".into(),
        });
    }
    let body = response.bytes().await?;
    check_action_body(&body)
}

fn check_action_body(body: &[u8]) -> Result<(), CoreError> {
    if !body.is_empty() {
        let value: serde_json::Value = serde_json::from_slice(body)?;
        if !value.is_object()
            || value.get("success").and_then(|value| value.as_bool()) == Some(false)
            || value
                .get("errors")
                .and_then(|value| value.as_array())
                .is_some_and(|errors| !errors.is_empty())
        {
            return Err(incomplete());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendProfile {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created: Option<String>,
}

pub async fn fetch_profile(
    client: &RobloxClient,
    cookie: &str,
    target: u64,
) -> Result<FriendProfile, CoreError> {
    client
        .get_json(
            &format!("https://users.roblox.com/v1/users/{target}"),
            cookie,
        )
        .await
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendGroup {
    pub id: u64,
    pub name: String,
    pub role: String,
}

pub async fn fetch_groups(
    client: &RobloxClient,
    cookie: &str,
    target: u64,
) -> Result<Vec<FriendGroup>, CoreError> {
    #[derive(Deserialize)]
    struct Group {
        id: u64,
        name: String,
    }
    #[derive(Deserialize)]
    struct Role {
        name: String,
    }
    #[derive(Deserialize)]
    struct Membership {
        group: Group,
        role: Role,
    }
    #[derive(Deserialize)]
    struct Response {
        data: Vec<Membership>,
    }
    let response: Response = client
        .get_json(
            &format!("https://groups.roblox.com/v1/users/{target}/groups/roles"),
            cookie,
        )
        .await?;
    Ok(response
        .data
        .into_iter()
        .filter(|entry| entry.group.id > 0)
        .map(|entry| FriendGroup {
            id: entry.group.id,
            name: entry.group.name,
            role: entry.role.name,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn friend_pages_accept_id_only_users_and_encode_pagination_cursors() {
        let page: super::UsersPage = serde_json::from_str(r#"{"data":[{"id":1,"hasVerifiedBadge":false},{"id":2}],"nextPageCursor":"next&cursor"}"#).unwrap();
        assert_eq!(page.data.len(), 2);
        assert!(page.data[0].username.is_empty());
        let url = super::next_url(
            "https://friends.roblox.com/v1/users/9/friends?limit=50",
            page.next_page_cursor.as_deref(),
        )
        .unwrap();
        let url = reqwest::Url::parse(&url).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(query.get("limit").map(|value| value.as_ref()), Some("50"));
        assert_eq!(
            query.get("cursor").map(|value| value.as_ref()),
            Some("next&cursor")
        );
    }
    use super::*;

    fn users(ids: &[u64]) -> Vec<FriendUser> {
        ids.iter()
            .map(|id| FriendUser {
                user_id: *id,
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn friendship_observation_is_silent_at_start_and_emits_each_change_once() {
        let mut tracker = RelationshipTracker::default();
        assert!(tracker
            .observe(Some(&users(&[1, 2])), Some(&users(&[3])))
            .is_empty());
        assert_eq!(
            tracker.observe(Some(&users(&[2, 4, 4, 0])), Some(&users(&[3, 5, 5, 0]))),
            [
                RelationshipChange::RequestReceived(5),
                RelationshipChange::Added(4),
                RelationshipChange::Removed(1)
            ]
        );
        assert!(tracker
            .observe(Some(&users(&[2, 4])), Some(&users(&[3, 5])))
            .is_empty());
        assert!(tracker.observe(None, None).is_empty());
        assert!(tracker
            .observe(Some(&users(&[2, 4])), Some(&users(&[5])))
            .is_empty());
    }

    #[test]
    fn successful_actions_reconcile_baselines_without_duplicate_notifications() {
        let mut tracker = RelationshipTracker::default();
        tracker.observe(Some(&users(&[1, 2])), Some(&users(&[3, 4])));
        tracker.record_action(3, FriendAction::Accept);
        tracker.record_action(4, FriendAction::Decline);
        tracker.record_action(1, FriendAction::Remove);
        tracker.record_action(2, FriendAction::Block);
        assert!(tracker.observe(Some(&users(&[3])), Some(&[])).is_empty());
        assert_eq!(
            tracker.observe(Some(&users(&[3, 6])), None),
            [RelationshipChange::Added(6)]
        );
    }

    #[test]
    fn reenabled_sources_establish_a_fresh_baseline_without_replaying_changes() {
        let mut tracker = RelationshipTracker::default();
        tracker.observe(Some(&users(&[1])), Some(&users(&[2])));
        tracker.retain_sources(false, false);
        assert!(tracker
            .observe(Some(&users(&[3])), Some(&users(&[4])))
            .is_empty());
        tracker.retain_sources(true, false);
        assert_eq!(
            tracker.observe(Some(&users(&[3, 5])), Some(&users(&[6]))),
            [RelationshipChange::Added(5)]
        );
    }

    #[test]
    fn offline_and_unknown_presence_do_not_report_stale_activity() {
        let user = FriendUser {
            user_id: 1,
            ..Default::default()
        };
        let mut presence = Presence {
            last_location: "Website".into(),
            place_id: Some(10),
            game_id: Some("12345678-1234-4234-8234-123456789abc".into()),
            ..Default::default()
        };
        for status in [0, 99] {
            presence.user_presence_type = status;
            let friend = map_friend(user.clone(), Some(&presence));
            assert!(friend.activity.is_empty());
            assert!(friend.place_id.is_none());
            assert!(friend.job_id.is_none());
        }
        for status in [1, 2, 3] {
            presence.user_presence_type = status;
            assert_eq!(
                map_friend(user.clone(), Some(&presence)).activity,
                "Website"
            );
        }
    }

    #[test]
    fn relationship_actions_require_a_successful_acknowledgement() {
        for body in [
            b"".as_slice(),
            br#"{}"#.as_slice(),
            br#"{"success":true}"#.as_slice(),
        ] {
            assert!(check_action_body(body).is_ok());
        }
        for body in [
            br#"{"success":false}"#.as_slice(),
            br#"{"errors":[{"code":1}]}"#.as_slice(),
            b"<html>proxy error</html>".as_slice(),
            b"null".as_slice(),
        ] {
            assert!(check_action_body(body).is_err());
        }
        for (value, expected) in [
            (0, FriendStatus::Offline),
            (1, FriendStatus::Online),
            (2, FriendStatus::Playing),
            (3, FriendStatus::Studio),
            (4, FriendStatus::Unknown),
        ] {
            assert_eq!(
                classify(Some(&Presence {
                    user_presence_type: value,
                    ..Default::default()
                })),
                expected
            );
        }
        let user: FriendUser =
            serde_json::from_str(r#"{"id":10,"name":null,"displayName":null}"#).unwrap();
        assert!(user.username.is_empty() && user.display_name.is_empty());
    }

    #[test]
    fn unknown_and_studio_presence_never_invent_servers() {
        let user = FriendUser {
            user_id: 1,
            ..Default::default()
        };
        let mut presence = Presence {
            user_presence_type: 3,
            place_id: Some(1),
            game_id: Some("12345678-1234-4234-8234-123456789abc".into()),
            ..Default::default()
        };
        assert_eq!(classify(Some(&presence)), FriendStatus::Studio);
        assert!(map_friend(user.clone(), Some(&presence)).job_id.is_none());
        presence.user_presence_type = 2;
        assert!(map_friend(user.clone(), Some(&presence)).job_id.is_some());
        presence.game_id = None;
        assert!(map_friend(user.clone(), Some(&presence)).job_id.is_none());
        presence.user_presence_type = 99;
        assert_eq!(classify(Some(&presence)), FriendStatus::Unknown);
        assert_eq!(classify(None), FriendStatus::Unknown);
        assert!(map_friend(user, None).place_id.is_none());
    }
    #[test]
    fn partial_users_and_cursors_are_safe() {
        let page: UsersPage = serde_json::from_str(
            r#"{"data":[{"id":10},{"id":10},{"id":0}],"nextPageCursor":"a&b?c"}"#,
        )
        .unwrap();
        assert_eq!(unique_users(page.data).len(), 1);
        assert!(next_url(
            "https://friends.roblox.com/v1/my/friends/requests?limit=100",
            page.next_page_cursor.as_deref()
        )
        .unwrap()
        .contains("cursor=a%26b%3Fc"));
        assert!(serde_json::from_str::<UsersPage>(r#"{"errors":[]}"#).is_err());
    }
    #[test]
    fn safe_errors_do_not_expose_external_bodies() {
        let secret = CoreError::RobloxApi {
            status: 500,
            message: "synthetic-secret".into(),
        };
        assert!(!error_message(&secret).contains("synthetic-secret"));
        assert!(error_message(&CoreError::RateLimited).contains("rate-limited"));
        assert!(error_message(&CoreError::RobloxApi {
            status: 401,
            message: "synthetic".into()
        })
        .contains("authenticate"));
    }
}
