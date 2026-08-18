use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;

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

pub async fn fetch_group(client: &RobloxClient, group_id: u64) -> Result<GroupInfo, CoreError> {
    let url = format!("https://groups.roblox.com/v1/groups/{group_id}");
    let value = get_value(client, &url).await?;
    let owner = value.get("owner").and_then(parse_owner);
    Ok(GroupInfo {
        id: value_u64(&value, "id").unwrap_or(group_id),
        name: value_string(&value, "name").unwrap_or_else(|| format!("Group {group_id}")),
        description: value_string(&value, "description").unwrap_or_default(),
        member_count: value_u64(&value, "memberCount").unwrap_or_default(),
        public_entry_allowed: value_bool(&value, "publicEntryAllowed").unwrap_or(false),
        has_verified_badge: value_bool(&value, "hasVerifiedBadge").unwrap_or(false),
        owner,
    })
}

pub async fn fetch_group_icon(
    client: &RobloxClient,
    group_id: u64,
) -> Result<Option<Vec<u8>>, CoreError> {
    let url = format!(
        "https://thumbnails.roblox.com/v1/groups/icons?groupIds={group_id}&size=150x150&format=Png&isCircular=false"
    );
    let value = get_value(client, &url).await?;
    let Some(url) = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|entry| {
                (value_u64(entry, "targetId") == Some(group_id))
                    .then(|| value_string(entry, "imageUrl"))
                    .flatten()
            })
        })
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
    let value = get_value(client, &url).await?;
    let shout = parse_shout(&value);
    Ok(shout.filter(|shout| !shout.body.trim().is_empty()))
}

pub async fn fetch_group_announcements(
    client: &RobloxClient,
    group_id: u64,
) -> Result<Vec<GroupAnnouncement>, CoreError> {
    let url = format!(
        "https://groups.roblox.com/v2/groups/{group_id}/wall/posts?sortOrder=Desc&limit=10"
    );
    let value = get_value(client, &url).await?;
    Ok(value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_announcement).collect())
        .unwrap_or_default())
}

pub async fn fetch_membership(
    client: &RobloxClient,
    group_id: u64,
    user_id: u64,
) -> Result<GroupMembership, CoreError> {
    let url = format!("https://groups.roblox.com/v2/users/{user_id}/groups/roles");
    let value = get_value(client, &url).await?;
    let role = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|entry| {
                let group = entry.get("group")?;
                (value_u64(group, "id")? == group_id).then(|| entry.get("role"))
            })
        })
        .flatten();
    Ok(GroupMembership {
        user_id,
        joined: role.is_some(),
        role_name: role.and_then(|value| value_string(value, "name")),
        role_rank: role.and_then(|value| value_u64(value, "rank")).unwrap_or(0) as u16,
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
    let url = if join {
        format!("https://groups.roblox.com/v1/groups/{group_id}/users")
    } else {
        format!("https://groups.roblox.com/v1/groups/{group_id}/users/{user_id}")
    };
    let body = serde_json::json!({});
    let response = client.request(method, &url, cookie, Some(&body)).await?;
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

async fn get_value(client: &RobloxClient, url: &str) -> Result<Value, CoreError> {
    let text = client.get_text(url, "").await?;
    serde_json::from_str(&text).map_err(|error| CoreError::RobloxApi {
        status: 200,
        message: format!("invalid group response: {error}"),
    })
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn value_u64(value: &Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
}

fn value_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn parse_owner(value: &Value) -> Option<GroupOwner> {
    Some(GroupOwner {
        id: value_u64(value, "id")?,
        username: value_string(value, "username").unwrap_or_default(),
        display_name: value_string(value, "displayName").unwrap_or_default(),
    })
}

fn parse_poster(value: Option<&Value>) -> Option<GroupPoster> {
    let value = value?;
    let user = value.get("user").unwrap_or(value);
    Some(GroupPoster {
        username: value_string(user, "username").unwrap_or_default(),
        display_name: value_string(user, "displayName").unwrap_or_default(),
    })
}

fn parse_date(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value?.as_str()?.parse().ok()
}

fn parse_shout(value: &Value) -> Option<GroupShout> {
    Some(GroupShout {
        body: value_string(value, "body").unwrap_or_default(),
        created: parse_date(value.get("created")),
        poster: parse_poster(value.get("poster")),
    })
}

fn parse_announcement(value: &Value) -> Option<GroupAnnouncement> {
    Some(GroupAnnouncement {
        id: value_u64(value, "id")?,
        body: value_string(value, "body").unwrap_or_default(),
        created: parse_date(value.get("created")),
        poster: parse_poster(value.get("poster")),
    })
}
