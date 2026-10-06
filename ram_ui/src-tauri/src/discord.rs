use crate::state::AppState;
use ram_core::{crypto, models::ModerationInfo, storage};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};
use tauri::{Emitter, Manager};

static DELIVERY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationSettings {
    moderation_detected: bool,
    batch_launch_finished: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            moderation_detected: true,
            batch_launch_finished: true,
        }
    }
}

fn preference_path(state: &AppState) -> Result<PathBuf, String> {
    state
        .runtime
        .lock()
        .map_err(|_| "Application state unavailable")?
        .config_path
        .parent()
        .map(|directory| directory.join("discord-notifications.json"))
        .ok_or_else(|| "Discord notification settings location unavailable".into())
}

fn load(path: &std::path::Path) -> Result<NotificationSettings, String> {
    match std::fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).map_err(|_| {
            "Discord notification settings are invalid. Save them again to repair them".into()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(NotificationSettings::default())
        }
        Err(_) => Err("Discord notification settings could not be read".into()),
    }
}

#[tauri::command]
pub async fn get_discord_notifications(
    state: tauri::State<'_, AppState>,
) -> Result<NotificationSettings, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || load(&preference_path(&state)?))
        .await
        .map_err(|_| "Notification settings task failed")?
}

#[tauri::command]
pub async fn save_discord_notifications(
    state: tauri::State<'_, AppState>,
    settings: NotificationSettings,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let data = serde_json::to_vec_pretty(&settings)
            .map_err(|_| "Notification settings could not be encoded")?;
        storage::atomic_write(&preference_path(&state)?, &data)
            .map_err(|_| "Notification settings could not be saved".into())
    })
    .await
    .map_err(|_| "Notification settings task failed")?
}

fn safe_label(label: &str) -> String {
    ram_core::redact::scrub(label)
        .chars()
        .filter(|character| !character.is_control())
        .take(100)
        .map(|character| {
            if "\\`*_~<>[]()#@|".contains(character) {
                ' '
            } else {
                character
            }
        })
        .collect()
}

pub fn payload(title: &str, detail: &str) -> serde_json::Value {
    serde_json::json!({
        "username": "Roblox Manager",
        "flags": 32768,
        "allowed_mentions": { "parse": [] },
        "components": [{ "type": 17, "accent_color": 4088822, "components": [
            { "type": 10, "content": format!("## {title}") },
            { "type": 14, "divider": true, "spacing": 1 },
            { "type": 10, "content": detail },
            { "type": 14, "divider": true, "spacing": 1 },
            { "type": 10, "content": format!("-# Roblox Manager {} · {}", env!("CARGO_PKG_VERSION"), chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")) }
        ]}]
    })
}

pub async fn send(url: &str, message: &serde_json::Value) -> Result<(), String> {
    if !crate::valid_discord_webhook_url(url) {
        return Err("Enter a valid Discord webhook URL".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Discord client unavailable")?;
    let response = client.post(format!("{url}?wait=true&with_components=true")).json(message)
        .send().await.map_err(|_| "Discord notification could not be delivered. Check your connection and test the webhook")?;
    if !response.status().is_success() {
        return Err(format!(
            "Discord rejected the notification (HTTP {}). Test or replace the webhook",
            response.status().as_u16()
        ));
    }
    Ok(())
}

pub async fn test(url: String) -> Result<(), String> {
    let url = if url.is_empty() {
        tauri::async_runtime::spawn_blocking(|| {
            crypto::discord_webhook().map_err(|_| "Saved Discord webhook could not be read")
        })
        .await
        .map_err(|_| "Webhook lookup task failed")??
        .ok_or("Configure a Discord webhook first")?
    } else {
        url
    };
    let _delivery = DELIVERY.lock().await;
    send(&url, &payload("Webhook connected", "Discord Components V2 notifications are working.\n\nEnabled events will be sent here while Roblox Manager is running.")).await
}

enum Event {
    Moderation,
    Batch,
}

fn dispatch(app: &tauri::AppHandle, event: Event, message: serde_json::Value) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _delivery = DELIVERY.lock().await;
        let state = app.state::<AppState>().inner().clone();
        let destination = tauri::async_runtime::spawn_blocking(move || {
            let settings = load(&preference_path(&state)?)?;
            let enabled = match event {
                Event::Moderation => settings.moderation_detected,
                Event::Batch => settings.batch_launch_finished,
            };
            if !enabled {
                return Ok(None);
            }
            crypto::discord_webhook()
                .map_err(|_| "Saved Discord webhook could not be read".to_string())
        })
        .await;
        let result = match destination {
            Ok(Ok(Some(url))) => send(&url, &message).await,
            Ok(Ok(None)) => return,
            Ok(Err(error)) => Err(format!("{error}. Check Integrations in Settings")),
            Err(_) => Err("Discord notification task failed. Try again".into()),
        };
        if let Err(error) = result {
            let _ = app.emit("background-notice", error);
        }
        // space independent event deliveries instead of flooding the channel
        tokio::time::sleep(Duration::from_millis(500)).await;
    });
}

pub fn moderation_changed(
    previous: Option<&ModerationInfo>,
    current: Option<&ModerationInfo>,
) -> bool {
    current.is_some_and(|current| {
        current.is_active()
            && previous.is_none_or(|previous| {
                !previous.is_active()
                    || previous.is_banned != current.is_banned
                    || previous.expires_at != current.expires_at
            })
    })
}

pub fn moderation(app: &tauri::AppHandle, label: &str) {
    dispatch(app, Event::Moderation, payload("Moderation detected", &format!("**Account:** {}\n\nRoblox reported a new or changed account restriction. Open the account on Roblox to review the moderation notice.", safe_label(label))));
}

pub fn batch_finished(app: &tauri::AppHandle, total: usize, completed: usize, place_id: u64) {
    if total < 2 {
        return;
    }
    let stopped = completed < total;
    let detail = format!("**Place ID:** {place_id}\n**Launch requests completed:** {completed} / {total}\n**Failed:** {}\n**Not attempted:** {}\n\n{}", usize::from(stopped), total.saturating_sub(completed + usize::from(stopped)),
        if stopped { "The batch stopped after a launch failure. Check RM before retrying." } else { "All launch requests finished. Roblox clients may still be starting or connecting; check Instances." });
    dispatch(
        app,
        Event::Batch,
        payload(
            if stopped {
                "Batch launch stopped"
            } else {
                "Batch launch finished"
            },
            &detail,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_messages_suppress_mentions_and_do_not_mix_legacy_fields() {
        let message = payload("Test", "Synthetic notification");
        assert_eq!(message["flags"], 32768);
        assert_eq!(message["components"][0]["type"], 17);
        assert_eq!(message["allowed_mentions"]["parse"], serde_json::json!([]));
        assert!(message.get("content").is_none());
        assert!(message.get("embeds").is_none());
        assert!(!safe_label("@everyone **name**\n<@123>").contains('@'));
    }
    #[test]
    fn webhook_destinations_reject_redirects_and_untrusted_hosts() {
        assert!(crate::valid_discord_webhook_url(
            "https://discord.com/api/webhooks/123456789012345678/synthetic_token"
        ));
        for url in [
            "http://discord.com/api/webhooks/123456789012345678/synthetic",
            "https://example.com/api/webhooks/123456789012345678/synthetic",
            "https://discord.com/api/webhooks/123456789012345678/synthetic?redirect=example",
            "https://discord.com@evil.example/api/webhooks/123456789012345678/synthetic",
        ] {
            assert!(!crate::valid_discord_webhook_url(url));
        }
    }
    #[test]
    fn repeated_moderation_checks_do_not_repeat_notifications() {
        let active = ModerationInfo {
            is_banned: true,
            ..Default::default()
        };
        assert!(moderation_changed(None, Some(&active)));
        assert!(!moderation_changed(Some(&active), Some(&active)));
        assert!(!moderation_changed(Some(&active), None));
        let checked = ModerationInfo {
            last_checked: Some(chrono::Utc::now()),
            ..active.clone()
        };
        assert!(!moderation_changed(Some(&active), Some(&checked)));
    }
    #[test]
    fn notification_preferences_round_trip_without_secrets() {
        let directory =
            std::env::temp_dir().join(format!("rm-discord-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("discord-notifications.json");
        assert_eq!(load(&path).unwrap(), NotificationSettings::default());
        let settings = NotificationSettings {
            moderation_detected: false,
            batch_launch_finished: true,
        };
        storage::atomic_write(&path, &serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        storage::atomic_write(&path, b"invalid").unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
