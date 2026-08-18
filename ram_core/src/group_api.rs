use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Deserialize;

use crate::auth::RobloxClient;
use crate::error::CoreError;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub member_count: u64,
    #[serde(default)]
    pub public_entry_allowed: bool,
    #[serde(default)]
    pub has_verified_badge: bool,
    pub owner: Option<GroupOwner>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupOwner {
    pub id: u64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupShout {
    #[serde(default)]
    pub body: String,
    pub created: Option<DateTime<Utc>>,
    pub poster: Option<GroupPoster>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupPoster {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupAnnouncement {
    pub id: u64,
    #[serde(default)]
    pub body: String,
    pub created: Option<DateTime<Utc>>,
    pub poster: Option<GroupPoster>,
}

#[derive(Debug, Clone)]
pub struct GroupMembership {
    pub user_id: u64,
    pub joined: bool,
    pub role_name: Option<String>,
    pub role_rank: u16,
}

#[derive(Deserialize)]
struct GroupIconResponse {
    data: Vec<GroupIconEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupIconEntry {
    target_id: u64,
    image_url: Option<String>,
}

#[derive(Deserialize)]
struct AnnouncementResponse {
    #[serde(default)]
    data: Vec<GroupAnnouncement>,
}

#[derive(Deserialize)]
struct UserGroupsResponse {
    #[serde(default)]
    data: Vec<UserGroupRole>,
}

#[derive(Deserialize)]
struct UserGroupRole {
    group: UserGroup,
    role: UserRole,
}

#[derive(Deserialize)]
struct UserGroup {
    id: u64,
}

#[derive(Deserialize)]
struct UserRole {
    name: String,
    rank: u16,
}

pub async fn fetch_group(client: &RobloxClient, group_id: u64) -> Result<GroupInfo, CoreError> {
    let url = format!("https://groups.roblox.com/v1/groups/{group_id}");
    client.get_json(&url, "").await
}

pub async fn fetch_group_icon(
    client: &RobloxClient,
    group_id: u64,
) -> Result<Option<Vec<u8>>, CoreError> {
    let url = format!(
        "https://thumbnails.roblox.com/v1/groups/icons?groupIds={group_id}&size=150x150&format=Png&isCircular=false"
    );
    let response: GroupIconResponse = client.get_json(&url, "").await?;
    let Some(url) = response
        .data
        .into_iter()
        .find(|entry| entry.target_id == group_id)
        .and_then(|entry| entry.image_url)
    else {
        return Ok(None);
    };
    Ok(Some(client.get_bytes(&url, "").await?))
}

pub async fn fetch_group_shout(
    client: &RobloxClient,
    group_id: u64,
) -> Result<Option<GroupShout>, CoreError> {
    let url = format!("https://groups.roblox.com/v1/groups/{group_id}/status");
    let response: Option<GroupShout> = client.get_json(&url, "").await.ok();
    Ok(response.filter(|shout| !shout.body.trim().is_empty()))
}

pub async fn fetch_group_announcements(
    client: &RobloxClient,
    group_id: u64,
) -> Result<Vec<GroupAnnouncement>, CoreError> {
    let url = format!(
        "https://groups.roblox.com/v2/groups/{group_id}/wall/posts?sortOrder=Desc&limit=10"
    );
    let response: AnnouncementResponse = client.get_json(&url, "").await?;
    Ok(response.data)
}

pub async fn fetch_membership(
    client: &RobloxClient,
    group_id: u64,
    user_id: u64,
) -> Result<GroupMembership, CoreError> {
    let url = format!("https://groups.roblox.com/v2/users/{user_id}/groups/roles");
    let response: UserGroupsResponse = client.get_json(&url, "").await?;
    let role = response
        .data
        .into_iter()
        .find(|entry| entry.group.id == group_id)
        .map(|entry| entry.role);
    Ok(GroupMembership {
        user_id,
        joined: role.is_some(),
        role_name: role.as_ref().map(|value| value.name.clone()),
        role_rank: role.map(|value| value.rank).unwrap_or(0),
    })
}

pub async fn change_membership(
    client: &RobloxClient,
    cookie: &str,
    group_id: u64,
    user_id: u64,
    join: bool,
) -> Result<(), CoreError> {
    let method = if join { Method::POST } else { Method::DELETE };
    let url = format!("https://groups.roblox.com/v1/groups/{group_id}/users/{user_id}");
    let response = client.request(method, &url, cookie, None).await?;
    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let message = response.text().await.unwrap_or_default();
        Err(CoreError::RobloxApi {
            status: status.as_u16(),
            message,
        })
    }
}
