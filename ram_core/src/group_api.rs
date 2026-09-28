use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;

use crate::api::trusted_roblox_image_url;
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
    pub has_social_modules: bool,
    pub community_tier: Option<u8>,
    pub created: Option<DateTime<Utc>>,
    pub shout: Option<GroupShout>,
    pub owner: Option<GroupOwner>,
}

#[derive(Debug, Clone)]
pub struct GroupSearchResult {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub member_count: u64,
    pub has_verified_badge: bool,
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
    tracing::debug!(group_id, "Fetching Roblox group details");
    let url = format!("https://groups.roblox.com/v1/groups/{group_id}");
    let value = get_value(client, &url).await?;
    let owner = value.get("owner").and_then(parse_owner);
    let info = GroupInfo {
        id: value_u64(&value, "id").unwrap_or(group_id),
        name: value_string(&value, "name").unwrap_or_else(|| format!("Group {group_id}")),
        description: value_string(&value, "description").unwrap_or_default(),
        member_count: value_u64(&value, "memberCount").unwrap_or_default(),
        public_entry_allowed: value_bool(&value, "publicEntryAllowed").unwrap_or(false),
        has_verified_badge: value_bool(&value, "hasVerifiedBadge").unwrap_or(false),
        has_social_modules: value_bool(&value, "hasSocialModules").unwrap_or(false),
        community_tier: value
            .get("communityTier")
            .and_then(|tier| value_u64(tier, "currentTier"))
            .map(|tier| tier as u8),
        created: parse_date(value.get("created")),
        shout: value.get("shout").and_then(parse_shout),
        owner,
    };
    tracing::debug!(group_id, name = %info.name, members = info.member_count, "Fetched group info");
    Ok(info)
}

pub async fn search_groups(
    client: &RobloxClient,
    keyword: &str,
) -> Result<Vec<GroupSearchResult>, CoreError> {
    tracing::debug!(keyword, "Searching Roblox groups");
    let mut url =
        reqwest::Url::parse("https://groups.roblox.com/v1/groups/search").map_err(|error| {
            CoreError::RobloxApi {
                status: 0,
                message: error.to_string(),
            }
        })?;
    url.query_pairs_mut()
        .append_pair("keyword", keyword.trim())
        .append_pair("limit", "10");
    let value = get_value(client, url.as_str()).await?;
    let results: Vec<GroupSearchResult> = value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(GroupSearchResult {
                        id: value_u64(item, "id")?,
                        name: value_string(item, "name")?,
                        description: value_string(item, "description").unwrap_or_default(),
                        member_count: value_u64(item, "memberCount").unwrap_or_default(),
                        has_verified_badge: value_bool(item, "hasVerifiedBadge").unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    tracing::debug!(keyword, count = results.len(), "Group search completed");
    Ok(results)
}

pub async fn fetch_group_icon(
    client: &RobloxClient,
    group_id: u64,
) -> Result<Option<Vec<u8>>, CoreError> {
    tracing::debug!(group_id, "Fetching group icon bytes");
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
    let bytes = client.get_bytes(&url, "").await?;
    tracing::debug!(group_id, bytes = bytes.len(), "Downloaded group icon");
    Ok(Some(bytes))
}

pub async fn fetch_group_announcements(
    client: &RobloxClient,
    group_id: u64,
) -> Result<Vec<GroupAnnouncement>, CoreError> {
    tracing::debug!(group_id, "Fetching group announcements / wall posts");
    let url = format!(
        "https://groups.roblox.com/v2/groups/{group_id}/wall/posts?sortOrder=Desc&limit=10"
    );
    let value = get_value(client, &url).await?;
    let posts: Vec<GroupAnnouncement> = value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_announcement).collect())
        .unwrap_or_default();
    tracing::debug!(group_id, count = posts.len(), "Fetched group wall posts");
    Ok(posts)
}

pub async fn fetch_membership(
    client: &RobloxClient,
    group_id: u64,
    user_id: u64,
) -> Result<GroupMembership, CoreError> {
    tracing::debug!(group_id, user_id, "Fetching group membership role for user");
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
    let membership = GroupMembership {
        user_id,
        joined: role.is_some(),
        role_name: role.and_then(|value| value_string(value, "name")),
        role_rank: role.and_then(|value| value_u64(value, "rank")).unwrap_or(0) as u16,
    };
    tracing::debug!(
        group_id,
        user_id,
        joined = membership.joined,
        "Fetched group membership"
    );
    Ok(membership)
}

pub async fn change_membership(
    client: &RobloxClient,
    cookie: &str,
    group_id: u64,
    user_id: u64,
    join: bool,
) -> Result<(), CoreError> {
    tracing::info!(
        group_id,
        user_id,
        join,
        "Requesting group membership change"
    );
    let method = if join { Method::POST } else { Method::DELETE };
    let url = if join {
        format!("https://groups.roblox.com/v1/groups/{group_id}/users")
    } else {
        format!("https://groups.roblox.com/v1/groups/{group_id}/users/{user_id}")
    };
    let body = serde_json::json!({});
    let response = client.request(method, &url, cookie, Some(&body)).await?;
    if response.status().is_success() {
        tracing::info!(group_id, user_id, join, "Group membership change succeeded");
        Ok(())
    } else {
        let status = response.status();
        let message = response.text().await.unwrap_or_default();
        tracing::warn!(group_id, user_id, join, status = status.as_u16(), message = %message, "Group membership change rejected");
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
        id: value_u64(value, "id").or_else(|| value_u64(value, "userId"))?,
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

#[derive(Debug, Clone)]
pub struct LatestGroupAnnouncement {
    pub title: String,
    pub body: String,
    pub created: Option<String>,
    pub image_url: Option<String>,
    pub reactions: Vec<GroupReaction>,
}

#[derive(Debug, Clone)]
pub struct GroupReaction {
    pub label: String,
    pub count: u64,
}

#[derive(Debug, Clone)]
pub struct GroupForumCategory {
    pub id: String,
    pub name: String,
    pub posts: Vec<GroupForumPost>,
}

#[derive(Debug, Clone)]
pub struct GroupForumPost {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created: Option<String>,
    pub author: Option<String>,
    pub comment_count: u64,
}

fn parse_latest_group_announcement(
    value: &serde_json::Value,
) -> Option<(LatestGroupAnnouncement, Option<u64>)> {
    let announcement = value
        .get("announcement")
        .or_else(|| {
            value
                .get("data")
                .and_then(serde_json::Value::as_array)?
                .first()
        })
        .unwrap_or(value);
    let title = announcement
        .get("title")
        .or_else(|| announcement.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Announcement")
        .to_string();
    let body = announcement
        .pointer("/message/content/plainText")
        .or_else(|| announcement.pointer("/content/plainText"))
        .or_else(|| announcement.get("message"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let image_url = announcement
        .pointer("/media/0/imageUrl")
        .or_else(|| announcement.pointer("/message/media/0/imageUrl"))
        .or_else(|| announcement.pointer("/message/content/media/0/imageUrl"))
        .or_else(|| announcement.pointer("/media/0/url"))
        .and_then(serde_json::Value::as_str)
        .and_then(trusted_roblox_image_url);
    let reactions = announcement
        .pointer("/message/reactions")
        .or_else(|| announcement.get("reactions"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, reaction)| {
            Some(GroupReaction {
                label: reaction
                    .get("name")
                    .or_else(|| reaction.get("type"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("Reaction {}", index + 1)),
                count: reaction
                    .get("count")
                    .or_else(|| reaction.get("reactionCount"))?
                    .as_u64()?,
            })
        })
        .collect();
    Some((
        LatestGroupAnnouncement {
            title,
            body,
            created: announcement
                .get("createdAt")
                .or_else(|| announcement.get("created"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            image_url,
            reactions,
        },
        announcement
            .pointer("/message/media/assetId")
            .and_then(serde_json::Value::as_u64),
    ))
}

fn parse_forum_post(value: &serde_json::Value) -> Option<GroupForumPost> {
    let id = value
        .get("id")?
        .as_str()
        .map(str::to_string)
        .or_else(|| value.get("id")?.as_u64().map(|id| id.to_string()))?;
    let title = value
        .get("title")
        .or_else(|| value.get("subject"))?
        .as_str()?
        .to_string();
    Some(GroupForumPost {
        id,
        title,
        body: value
            .pointer("/content/plainText")
            .or_else(|| value.get("body"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        created: value
            .get("createdAt")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        author: value
            .pointer("/author/displayName")
            .or_else(|| value.pointer("/author/username"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        comment_count: value
            .get("commentCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default(),
    })
}

pub async fn fetch_group_forums(
    client: &RobloxClient,
    cookie: &str,
    group_id: u64,
) -> Option<Vec<GroupForumCategory>> {
    let url = format!("https://groups.roblox.com/v1/groups/{group_id}/forums");
    let response = client
        .get_json::<serde_json::Value>(&url, cookie)
        .await
        .ok()?;
    let categories = response.get("data").and_then(serde_json::Value::as_array)?;
    let mut forums = Vec::new();
    for category in categories.iter().take(8) {
        let Some(id) = category.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if id.len() != 36
            || !id
                .chars()
                .all(|character| character.is_ascii_hexdigit() || character == '-')
        {
            continue;
        }
        let Some(name) = category.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let posts_url = format!(
            "https://groups.roblox.com/v1/groups/{group_id}/forums/{id}/posts?limit=10&includeCommentCount=true"
        );
        let posts = client
            .get_json::<serde_json::Value>(&posts_url, cookie)
            .await
            .ok()
            .and_then(|response| {
                response
                    .get("data")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
            })
            .unwrap_or_default()
            .iter()
            .filter_map(parse_forum_post)
            .collect();
        forums.push(GroupForumCategory {
            id: id.to_string(),
            name: name.to_string(),
            posts,
        });
    }
    Some(forums)
}

pub async fn fetch_latest_group_announcement(
    client: &RobloxClient,
    group_id: u64,
) -> Option<LatestGroupAnnouncement> {
    let announcement_url =
        format!("https://groups.roblox.com/v1/groups/{group_id}/announcements/latest");
    let announcement = client
        .get_json::<serde_json::Value>(&announcement_url, "")
        .await
        .ok()
        .and_then(|value| parse_latest_group_announcement(&value));
    if let Some((mut announcement, image_asset_id)) = announcement {
        if announcement.image_url.is_none() {
            if let Some(asset_id) = image_asset_id {
                let thumbnail_url = format!(
                    "https://thumbnails.roblox.com/v1/assets?assetIds={asset_id}&size=420x420&format=Png&isCircular=false"
                );
                announcement.image_url = client
                    .get_json::<serde_json::Value>(&thumbnail_url, "")
                    .await
                    .ok()
                    .and_then(|response| response.get("data")?.as_array()?.first().cloned())
                    .and_then(|image| {
                        image
                            .get("imageUrl")?
                            .as_str()
                            .and_then(trusted_roblox_image_url)
                    });
            }
        }
        Some(announcement)
    } else {
        None
    }
}

#[cfg(test)]
mod group_content_tests {
    use super::*;

    #[test]
    fn announcement_fallbacks_preserve_text_and_filter_untrusted_media() {
        let response = serde_json::json!({
            "announcement": {
                "title": "Synthetic title",
                "message": "Synthetic message",
                "created": "synthetic-timestamp",
                "media": [{ "imageUrl": "https://rbxcdn.com.attacker.invalid/image.png" }],
                "reactions": [{ "name": "Useful", "count": 3 }, { "name": "Missing count" }]
            }
        });
        let (announcement, asset_id) = parse_latest_group_announcement(&response).unwrap();
        assert_eq!(announcement.title, "Synthetic title");
        assert_eq!(announcement.body, "Synthetic message");
        assert_eq!(announcement.created.as_deref(), Some("synthetic-timestamp"));
        assert!(announcement.image_url.is_none());
        assert!(asset_id.is_none());
        assert_eq!(announcement.reactions.len(), 1);
        assert_eq!(announcement.reactions[0].label, "Useful");
        let (announcement, _) = parse_latest_group_announcement(&serde_json::json!({})).unwrap();
        assert_eq!(announcement.title, "Announcement");
        assert!(announcement.body.is_empty());
    }

    #[test]
    fn forum_posts_preserve_identifier_formats_and_skip_malformed_entries() {
        let response = serde_json::json!({
            "id": 42,
            "subject": "Synthetic subject",
            "body": "Synthetic body",
            "author": { "username": "Synthetic user" },
            "commentCount": 7
        });
        let post = parse_forum_post(&response).unwrap();
        assert_eq!(post.id, "42");
        assert_eq!(post.title, "Synthetic subject");
        assert_eq!(post.body, "Synthetic body");
        assert_eq!(post.author.as_deref(), Some("Synthetic user"));
        assert_eq!(post.comment_count, 7);
        let post = parse_forum_post(&serde_json::json!({ "id": "synthetic-id", "title": "Title" }))
            .unwrap();
        assert_eq!(post.id, "synthetic-id");
        assert!(post.body.is_empty());
        assert_eq!(post.comment_count, 0);
        assert!(parse_forum_post(&serde_json::json!({ "title": "Missing ID" })).is_none());
        assert!(parse_forum_post(&serde_json::json!({ "id": 1 })).is_none());
    }

    #[test]
    fn roblox_images_require_https_and_an_exact_cdn_domain_suffix() {
        for url in [
            "https://rbxcdn.com/image.png",
            "https://subdomain.rbxcdn.com/image.png",
        ] {
            assert_eq!(trusted_roblox_image_url(url).as_deref(), Some(url));
        }
        for url in [
            "http://rbxcdn.com/image.png",
            "https://rbxcdn.com.attacker.invalid/image.png",
            "https://attacker.invalid/?url=rbxcdn.com",
            "not-a-url",
        ] {
            assert!(trusted_roblox_image_url(url).is_none());
        }
    }

    #[test]
    fn parses_current_announcement_content_and_media() {
        let response = serde_json::json!({
            "data": [{
                "name": "Update",
                "createdAt": "2026-06-12T01:02:18Z",
                "message": {
                    "content": { "plainText": "New content" },
                    "media": { "assetId": 123456789_u64 },
                    "reactions": [{ "emoteId": "synthetic-id", "reactionCount": 6 }]
                }
            }]
        });
        let (announcement, image_asset_id) = parse_latest_group_announcement(&response).unwrap();
        assert_eq!(announcement.title, "Update");
        assert_eq!(announcement.body, "New content");
        assert_eq!(image_asset_id, Some(123456789));
        assert_eq!(announcement.reactions[0].count, 6);
    }
}
