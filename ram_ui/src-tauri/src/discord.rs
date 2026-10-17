use crate::state::AppState;
use ram_core::{crypto, models::ModerationInfo, storage};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};
use tauri::{Emitter, Manager};

static DELIVERY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static SETTINGS_IO: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    ModerationDetected,
    BatchLaunchFinished,
    SessionExpired,
    LaunchFailed,
    ClientExited,
    FriendRequestReceived,
    FriendRequestSent,
    FriendAdded,
    FriendRemoved,
    FriendRequestDeclined,
    UserBlocked,
}

impl Event {
    const ALL: [Self; 11] = [
        Self::ModerationDetected,
        Self::BatchLaunchFinished,
        Self::SessionExpired,
        Self::LaunchFailed,
        Self::ClientExited,
        Self::FriendRequestReceived,
        Self::FriendRequestSent,
        Self::FriendAdded,
        Self::FriendRemoved,
        Self::FriendRequestDeclined,
        Self::UserBlocked,
    ];
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    id: String,
    name: String,
    enabled: bool,
    events: Vec<Event>,
}

impl Destination {
    fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            enabled: true,
            events: Event::ALL.to_vec(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationSettings {
    enabled: bool,
    moderation_detected: bool,
    batch_launch_finished: bool,
    session_expired: bool,
    launch_failed: bool,
    client_exited: bool,
    friend_request_received: bool,
    friend_request_sent: bool,
    friend_added: bool,
    friend_removed: bool,
    friend_request_declined: bool,
    user_blocked: bool,
    destinations: Vec<Destination>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            moderation_detected: true,
            batch_launch_finished: true,
            session_expired: false,
            launch_failed: false,
            client_exited: false,
            friend_request_received: false,
            friend_request_sent: false,
            friend_added: false,
            friend_removed: false,
            friend_request_declined: false,
            user_blocked: false,
            destinations: Vec::new(),
        }
    }
}

impl NotificationSettings {
    fn friendship_sources(&self) -> (bool, bool) {
        (
            !self.routes(Event::FriendAdded).is_empty()
                || !self.routes(Event::FriendRemoved).is_empty(),
            !self.routes(Event::FriendRequestReceived).is_empty(),
        )
    }

    fn routes(&self, event: Event) -> Vec<&Destination> {
        let enabled = match event {
            Event::ModerationDetected => self.moderation_detected,
            Event::BatchLaunchFinished => self.batch_launch_finished,
            Event::SessionExpired => self.session_expired,
            Event::LaunchFailed => self.launch_failed,
            Event::ClientExited => self.client_exited,
            Event::FriendRequestReceived => self.friend_request_received,
            Event::FriendRequestSent => self.friend_request_sent,
            Event::FriendAdded => self.friend_added,
            Event::FriendRemoved => self.friend_removed,
            Event::FriendRequestDeclined => self.friend_request_declined,
            Event::UserBlocked => self.user_blocked,
        };
        self.destinations
            .iter()
            .filter(|destination| {
                self.enabled
                    && enabled
                    && destination.enabled
                    && destination.events.contains(&event)
            })
            .collect()
    }

    fn validate(&self) -> Result<(), String> {
        if self.destinations.len() > 16 {
            return Err("Save up to 16 Discord destinations".into());
        }
        let mut ids = std::collections::HashSet::new();
        for destination in &self.destinations {
            if !valid_destination_id(&destination.id)
                || !ids.insert(&destination.id)
                || destination.name.trim().is_empty()
                || destination.name.chars().count() > 40
                || destination.name.chars().any(char::is_control)
                || destination.events.len() > Event::ALL.len()
                || destination
                    .events
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != destination.events.len()
            {
                return Err("Discord destination settings are invalid".into());
            }
        }
        Ok(())
    }
}

fn valid_destination_id(id: &str) -> bool {
    id == "default" || uuid::Uuid::parse_str(id).is_ok_and(|value| value.to_string() == id)
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
            "Discord notification settings are invalid. Use Reset notification preferences to recover".into()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(NotificationSettings::default())
        }
        Err(_) => Err("Discord notification settings could not be read".into()),
    }
}

fn load_connected(path: &std::path::Path) -> Result<NotificationSettings, String> {
    let mut settings = load(path)?;
    settings.validate()?;
    if !settings
        .destinations
        .iter()
        .any(|destination| destination.id == "default")
        && crypto::discord_webhook()
            .map_err(|_| "Saved Discord webhook could not be read")?
            .is_some()
    {
        settings
            .destinations
            .push(Destination::new("default".into(), "Primary webhook".into()));
    }
    Ok(settings)
}

pub fn configured(config_path: &std::path::Path) -> Result<bool, String> {
    let legacy = crypto::discord_webhook()
        .map_err(|_| "Saved Discord webhook could not be read")?
        .is_some();
    // The integration reports unreadable preferences itself; other settings must remain accessible.
    Ok(legacy
        || load(&config_path.with_file_name("discord-notifications.json"))
            .is_ok_and(|settings| !settings.destinations.is_empty()))
}

fn save_preferences(path: &std::path::Path, settings: &NotificationSettings) -> Result<(), String> {
    settings.validate()?;
    let data = serde_json::to_vec_pretty(settings)
        .map_err(|_| "Notification settings could not be encoded")?;
    storage::atomic_write(path, &data)
        .map_err(|_| "Notification settings could not be saved".into())
}

fn reset_preferences(
    path: &std::path::Path,
    primary_connected: bool,
) -> Result<NotificationSettings, String> {
    let original = std::fs::read(path)
        .map_err(|_| "Saved notification preferences could not be backed up. Nothing was reset")?;
    if serde_json::from_slice::<NotificationSettings>(&original)
        .is_ok_and(|settings| settings.validate().is_ok())
    {
        return Err(
            "Notification preferences are valid now. Reload them instead of resetting".into(),
        );
    }
    let mut settings = NotificationSettings {
        enabled: false,
        ..Default::default()
    };
    if primary_connected {
        settings
            .destinations
            .push(Destination::new("default".into(), "Primary webhook".into()));
    }
    let recovery = path.with_file_name(format!(
        "discord-notifications.recovery-{}.json",
        uuid::Uuid::new_v4()
    ));
    let encoded = serde_json::to_vec_pretty(&settings)
        .map_err(|_| "Recovery preferences could not be encoded")?;
    storage::atomic_swap(&recovery, &original)
        .map_err(|_| "Saved notification preferences could not be backed up. Nothing was reset")?;
    // The separate recovery copy is durable; leave an existing .bak untouched.
    storage::atomic_swap(path, &encoded).map_err(|_| {
        "Notification preferences could not be reset. The original and recovery copy were kept"
    })?;
    Ok(settings)
}

#[tauri::command]
pub async fn reset_discord_notifications(
    state: tauri::State<'_, AppState>,
) -> Result<NotificationSettings, String> {
    let _settings = SETTINGS_IO.lock().await;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let primary_connected = crypto::discord_webhook()
            .map_err(|_| "Saved primary webhook could not be checked. Nothing was reset")?
            .is_some();
        let settings = reset_preferences(&preference_path(&state)?, primary_connected)?;
        state.friends.reset_notifications();
        Ok(settings)
    })
    .await
    .map_err(|_| "Notification recovery task failed")?
}

#[tauri::command]
pub async fn get_discord_notifications(
    state: tauri::State<'_, AppState>,
) -> Result<NotificationSettings, String> {
    let state = state.inner().clone();
    let _settings = SETTINGS_IO.lock().await;
    tauri::async_runtime::spawn_blocking(move || load_connected(&preference_path(&state)?))
        .await
        .map_err(|_| "Notification settings task failed")?
}

pub async fn friendship_watch(state: &AppState) -> Result<(bool, bool), String> {
    let _settings = SETTINGS_IO.lock().await;
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_connected(&preference_path(&state)?)?;
        Ok(settings.friendship_sources())
    })
    .await
    .map_err(|_| "Friendship notification preferences could not be read")?
}

#[tauri::command]
pub async fn save_discord_notifications(
    state: tauri::State<'_, AppState>,
    settings: NotificationSettings,
) -> Result<(), String> {
    let _settings = SETTINGS_IO.lock().await;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = preference_path(&state)?;
        let current = load_connected(&path)?;
        let ids: std::collections::HashSet<_> = current
            .destinations
            .iter()
            .map(|destination| &destination.id)
            .collect();
        let next_ids: std::collections::HashSet<_> = settings
            .destinations
            .iter()
            .map(|destination| &destination.id)
            .collect();
        if ids != next_ids {
            return Err("Discord destinations changed. Reload preferences before saving".into());
        }
        save_preferences(&path, &settings)?;
        if current.friendship_sources() != settings.friendship_sources() {
            state.friends.reset_notifications();
        }
        Ok(())
    })
    .await
    .map_err(|_| "Notification settings task failed")?
}

fn commit_change(
    update: impl FnOnce() -> Result<(), String>,
    persist: impl FnOnce() -> Result<(), String>,
    rollback: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    update()?;
    if let Err(error) = persist() {
        if rollback().is_err() {
            return Err("Webhook storage changed, but preferences and recovery could not be saved. Reopen Integrations and check each destination".into());
        }
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
pub async fn save_discord_destination(
    state: tauri::State<'_, AppState>,
    id: Option<String>,
    name: String,
    url: String,
) -> Result<NotificationSettings, String> {
    if !crate::valid_discord_webhook_url(url.trim()) {
        return Err("Enter a valid Discord webhook URL".into());
    }
    let _settings = SETTINGS_IO.lock().await;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = preference_path(&state)?;
        let mut settings = load_connected(&path)?;
        let previous_sources = settings.friendship_sources();
        let id = match id {
            Some(id)
                if settings
                    .destinations
                    .iter()
                    .any(|destination| destination.id == id) =>
            {
                id
            }
            Some(_) => return Err("Discord destination not found. Reload preferences".into()),
            None => uuid::Uuid::new_v4().to_string(),
        };
        if let Some(destination) = settings
            .destinations
            .iter_mut()
            .find(|destination| destination.id == id)
        {
            destination.name = name.trim().into();
        } else {
            settings
                .destinations
                .push(Destination::new(id.clone(), name.trim().into()));
        }
        settings.validate()?;
        let previous =
            crypto::discord_destination(&id).map_err(|_| "Saved webhook could not be read")?;
        commit_change(
            || {
                crypto::set_discord_destination(&id, url.trim())
                    .map_err(|_| "Webhook could not be saved".into())
            },
            || save_preferences(&path, &settings),
            || {
                match &previous {
                    Some(previous) => crypto::set_discord_destination(&id, previous),
                    None => crypto::delete_discord_destination(&id),
                }
                .map_err(|_| "Webhook recovery failed".into())
            },
        )?;
        if previous_sources != settings.friendship_sources() {
            state.friends.reset_notifications();
        }
        Ok(settings)
    })
    .await
    .map_err(|_| "Webhook save task failed")?
}

#[tauri::command]
pub async fn remove_discord_destination(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<NotificationSettings, String> {
    let _settings = SETTINGS_IO.lock().await;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = preference_path(&state)?;
        let mut settings = load_connected(&path)?;
        let previous_sources = settings.friendship_sources();
        if !settings
            .destinations
            .iter()
            .any(|destination| destination.id == id)
        {
            return Err("Discord destination not found".into());
        }
        settings
            .destinations
            .retain(|destination| destination.id != id);
        let previous =
            crypto::discord_destination(&id).map_err(|_| "Saved webhook could not be read")?;
        commit_change(
            || {
                crypto::delete_discord_destination(&id)
                    .map_err(|_| "Webhook could not be removed".into())
            },
            || save_preferences(&path, &settings),
            || {
                match &previous {
                    Some(previous) => crypto::set_discord_destination(&id, previous),
                    None => Ok(()),
                }
                .map_err(|_| "Webhook recovery failed".into())
            },
        )?;
        if previous_sources != settings.friendship_sources() {
            state.friends.reset_notifications();
        }
        Ok(settings)
    })
    .await
    .map_err(|_| "Webhook removal task failed")?
}

#[tauri::command]
pub async fn test_discord_destination(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let _delivery = DELIVERY.lock().await;
    let _settings = SETTINGS_IO.lock().await;
    let state = state.inner().clone();
    let url = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        if !load_connected(&preference_path(&state)?)?
            .destinations
            .iter()
            .any(|destination| destination.id == id)
        {
            return Err("Discord destination not found".into());
        }
        crypto::discord_destination(&id)
            .map_err(|_| "Saved webhook could not be read")?
            .ok_or_else(|| "Saved webhook is missing. Replace this destination's URL".into())
    })
    .await
    .map_err(|_| "Webhook test task failed")??;
    drop(_settings);
    send(&url, &payload("Webhook connected", "This saved Discord destination is working. Test messages can be sent while notifications are disabled.")).await
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
            { "type": 10, "content": format!("-# Roblox Manager {} - {}", env!("CARGO_PKG_VERSION"), chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")) }
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

#[derive(Default)]
pub struct NotificationQueue {
    messages: std::sync::Mutex<std::collections::VecDeque<(Event, serde_json::Value)>>,
    ready: tokio::sync::Notify,
}

fn dispatch(app: &tauri::AppHandle, event: Event, message: serde_json::Value) {
    let state = app.state::<AppState>();
    if state
        .is_shutting_down
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return;
    }
    if let Ok(mut messages) = state.discord.messages.lock() {
        if messages.len() >= 128 {
            let _ = app.emit(
                "background-notice",
                "Discord notification queue is full. Some events could not be queued",
            );
            return;
        }
        messages.push_back((event, message));
        state.discord.ready.notify_one();
    };
}

pub async fn run(app: tauri::AppHandle) {
    let state = app.state::<AppState>().inner().clone();
    loop {
        let notification = state.discord.ready.notified();
        let pending = state
            .discord
            .messages
            .lock()
            .ok()
            .and_then(|mut messages| messages.pop_front());
        let Some((event, message)) = pending else {
            notification.await;
            continue;
        };
        if state
            .is_shutting_down
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return;
        }
        let _delivery = DELIVERY.lock().await;
        let settings_guard = SETTINGS_IO.lock().await;
        let settings_state = state.clone();
        let routes = tauri::async_runtime::spawn_blocking(move || {
            let settings = load_connected(&preference_path(&settings_state)?)?;
            Ok::<_, String>(
                settings
                    .routes(event)
                    .iter()
                    .map(|destination| destination.id.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .await;
        drop(settings_guard);
        let routes = match routes {
            Ok(Ok(routes)) => routes,
            _ => {
                let _ = app.emit(
                    "background-notice",
                    "Discord preferences could not be read. Check Integrations in Settings",
                );
                continue;
            }
        };
        let mut delivered = std::collections::HashSet::new();
        for id in routes {
            if state
                .is_shutting_down
                .load(std::sync::atomic::Ordering::Acquire)
            {
                return;
            }
            let settings_guard = SETTINGS_IO.lock().await;
            let settings_state = state.clone();
            let destination = tauri::async_runtime::spawn_blocking(move || {
                let settings = load_connected(&preference_path(&settings_state)?)?;
                if !settings
                    .routes(event)
                    .iter()
                    .any(|destination| destination.id == id)
                {
                    return Ok(None);
                }
                crypto::discord_destination(&id)
                    .map_err(|_| "Saved Discord webhook could not be read".to_string())?
                    .map(Some)
                    .ok_or_else(|| {
                        "Saved Discord webhook is missing. Replace it in Integrations".to_string()
                    })
            })
            .await;
            drop(settings_guard);
            let result = match destination {
                Ok(Ok(Some(url))) if delivered.insert(url.clone()) => send(&url, &message).await,
                Ok(Ok(_)) => continue,
                Ok(Err(error)) => Err(error),
                Err(_) => Err("Discord destination lookup failed. Try again".into()),
            };
            if let Err(error) = result {
                let _ = app.emit("background-notice", error);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

fn job_detail(job_id: Option<&str>) -> String {
    ram_core::instances::normalize_job_id(job_id)
        .map(|job| format!("\n**Job ID:** `{job}`"))
        .unwrap_or_default()
}

fn friendship_payload(
    event: Event,
    account_id: u64,
    label: &str,
    target_id: u64,
    observed: bool,
) -> Option<serde_json::Value> {
    let title = match event {
        Event::FriendRequestReceived => "Friend request received",
        Event::FriendRequestSent => "Friend request sent",
        Event::FriendAdded => "Friend added",
        Event::FriendRemoved => "Friend removed",
        Event::FriendRequestDeclined => "Friend request declined",
        Event::UserBlocked => "User blocked",
        _ => return None,
    };
    Some(payload(title, &format!("**Account:** {}\n**Account ID:** {account_id}\n**Other user ID:** {target_id}\n**Profile:** https://www.roblox.com/users/{target_id}/profile\n\n{}", safe_label(label), if observed { "Observed in a complete background check. This does not identify who initiated a friendship change." } else { "Roblox accepted this action requested from RM." })))
}

pub fn friendship(
    app: &tauri::AppHandle,
    event: Event,
    account_id: u64,
    target_id: u64,
    observed: bool,
) {
    let state = app.state::<AppState>();
    let label = {
        let Ok(runtime) = state.runtime.lock() else {
            return;
        };
        let Some(account) = runtime.accounts.find_by_id(account_id) else {
            return;
        };
        if runtime.config.anonymize_names {
            format!("Account {account_id}")
        } else {
            account.label().to_string()
        }
    };
    if let Some(message) = friendship_payload(event, account_id, &label, target_id, observed) {
        dispatch(app, event, message);
    }
}

fn account_server_detail(app: &tauri::AppHandle, user_id: u64) -> String {
    crate::instances::account_server(&app.state::<AppState>(), user_id)
        .map(|(place, job)| format!("\n**Place ID:** {place}{}", job_detail(Some(&job))))
        .unwrap_or_default()
}

pub fn session_expired(app: &tauri::AppHandle, user_id: u64, label: &str) {
    dispatch(app, Event::SessionExpired, payload("Account session expired", &format!("**Account:** {}{}\n\nRoblox rejected this account's saved login during verification. Log in again from Accounts.", safe_label(label), account_server_detail(app, user_id))));
}

fn place_detail(place_id: u64) -> String {
    if place_id > 0 {
        format!("\n**Place ID:** {place_id}")
    } else {
        String::new()
    }
}

pub fn launch_failed(app: &tauri::AppHandle, user_id: u64, place_id: u64, job_id: Option<&str>) {
    dispatch(app, Event::LaunchFailed, payload("Launch request failed", &format!("**Account ID:** {user_id}{}{}\n\nRM could not complete this launch request. Check Accounts and Instances before retrying.", place_detail(place_id), job_detail(job_id))));
}

pub fn client_exited(app: &tauri::AppHandle, user_id: u64, place_id: u64, job_id: Option<&str>) {
    dispatch(app, Event::ClientExited, payload("Roblox client exited", &format!("**Account ID:** {user_id}{}{}\n\nAn exactly attributed Roblox client has exited. This includes clients closed manually; it does not confirm a crash.", place_detail(place_id), job_detail(job_id))));
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

pub fn moderation(app: &tauri::AppHandle, user_id: u64, label: &str) {
    dispatch(app, Event::ModerationDetected, payload("Moderation detected", &format!("**Account:** {}{}\n\nRoblox reported a new or changed account restriction. Open the account on Roblox to review the moderation notice.", safe_label(label), account_server_detail(app, user_id))));
}

pub fn batch_finished(
    app: &tauri::AppHandle,
    total: usize,
    completed: usize,
    place_id: u64,
    job_id: Option<&str>,
) {
    if total < 2 {
        return;
    }
    let stopped = completed < total;
    let detail = format!("**Place ID:** {place_id}{}\n**Launch requests completed:** {completed} / {total}\n**Failed:** {}\n**Not attempted:** {}\n\n{}", job_detail(job_id), usize::from(stopped), total.saturating_sub(completed + usize::from(stopped)),
        if stopped { "The batch stopped after a launch failure. Check RM before retrying." } else { "All launch requests finished. Roblox clients may still be starting or connecting; check Instances." });
    dispatch(
        app,
        Event::BatchLaunchFinished,
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
    #[test]
    fn home_notifications_omit_a_missing_game_destination() {
        assert!(super::place_detail(0).is_empty());
        assert_eq!(super::place_detail(123), "\n**Place ID:** 123");
    }

    use super::*;

    #[test]
    fn invalid_preferences_recover_paused_with_original_and_existing_backup_preserved() {
        let directory =
            std::env::temp_dir().join(format!("rm-discord-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let backup = serde_json::to_vec(&NotificationSettings::default()).unwrap();
        for (index, original) in [
            b"{incomplete".as_slice(),
            br#"{"destinations":[{"id":"default","name":"Synthetic","enabled":true,"events":["futureEvent"]}]}"#.as_slice(),
            br#"{"destinations":[{"id":"default","name":"","enabled":true,"events":[]}]}"#.as_slice(),
        ].into_iter().enumerate() {
            let path = directory.join(format!("preferences-{index}.json"));
            storage::atomic_swap(&path, original).unwrap();
            storage::atomic_swap(&storage::backup_path(&path), &backup).unwrap();
            let settings = reset_preferences(&path, true).unwrap();
            assert!(!settings.enabled);
            assert_eq!(settings.destinations.len(), 1);
            assert_eq!(settings.destinations[0].id, "default");
            assert!(settings.validate().is_ok());
            assert_eq!(load(&path).unwrap(), settings);
            assert_eq!(std::fs::read(storage::backup_path(&path)).unwrap(), backup);
            assert!(std::fs::read_dir(&directory).unwrap().filter_map(Result::ok).filter(|entry| entry.file_name().to_string_lossy().starts_with("discord-notifications.recovery-")).any(|entry| std::fs::read(entry.path()).unwrap() == original));
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_refuses_valid_or_unreadable_preferences_without_writing() {
        let directory =
            std::env::temp_dir().join(format!("rm-discord-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("discord-notifications.json");
        assert!(reset_preferences(&path, false).is_err());
        assert!(!path.exists());
        let original = serde_json::to_vec(&NotificationSettings::default()).unwrap();
        storage::atomic_swap(&path, &original).unwrap();
        assert!(reset_preferences(&path, false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn friendship_routes_are_opt_in_and_require_an_active_destination() {
        let mut settings: NotificationSettings = serde_json::from_str(r#"{"moderationDetected":false,"destinations":[{"id":"default","name":"Existing","enabled":true,"events":["moderationDetected","batchLaunchFinished"]}]}"#).unwrap();
        assert!(
            !settings.friend_request_received
                && !settings.friend_request_sent
                && !settings.friend_added
                && !settings.friend_removed
                && !settings.friend_request_declined
                && !settings.user_blocked
        );
        assert_eq!(settings.friendship_sources(), (false, false));
        settings.friend_added = true;
        settings.friend_request_received = true;
        assert_eq!(settings.friendship_sources(), (false, false));
        settings.destinations[0]
            .events
            .extend([Event::FriendAdded, Event::FriendRequestReceived]);
        assert_eq!(settings.friendship_sources(), (true, true));
        settings.destinations[0].enabled = false;
        assert_eq!(settings.friendship_sources(), (false, false));
        settings.destinations[0].enabled = true;
        settings.enabled = false;
        assert_eq!(settings.friendship_sources(), (false, false));
        settings.enabled = true;
        assert!(settings.validate().is_ok());
        let roundtrip: NotificationSettings =
            serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        assert_eq!(settings, roundtrip);
        assert!(!settings.routes(Event::FriendAdded).is_empty());
        assert!(settings.routes(Event::FriendRemoved).is_empty());
    }

    #[test]
    fn all_friendship_events_can_be_routed_and_duplicate_routes_are_rejected() {
        let mut settings = NotificationSettings {
            friend_request_received: true,
            friend_request_sent: true,
            friend_added: true,
            friend_removed: true,
            friend_request_declined: true,
            user_blocked: true,
            destinations: vec![Destination::new("default".into(), "Synthetic".into())],
            ..Default::default()
        };
        assert!(settings.validate().is_ok());
        for event in [
            Event::FriendRequestReceived,
            Event::FriendRequestSent,
            Event::FriendAdded,
            Event::FriendRemoved,
            Event::FriendRequestDeclined,
            Event::UserBlocked,
        ] {
            assert_eq!(settings.routes(event).len(), 1);
        }
        settings.destinations[0].events = vec![Event::FriendAdded, Event::FriendAdded];
        assert!(settings.validate().is_err());
    }

    #[test]
    fn friendship_messages_use_public_identifiers_and_do_not_ping() {
        for event in [
            Event::FriendRequestReceived,
            Event::FriendRequestSent,
            Event::FriendAdded,
            Event::FriendRemoved,
            Event::FriendRequestDeclined,
            Event::UserBlocked,
        ] {
            let message =
                friendship_payload(event, 10, "@everyone **Synthetic**", 20, false).unwrap();
            assert_eq!(message["allowed_mentions"]["parse"], serde_json::json!([]));
            let text = message.to_string();
            assert!(!text.contains("@everyone"));
            assert!(text.contains("Account ID:** 10") && text.contains("Other user ID:** 20"));
            assert!(text.contains("https://www.roblox.com/users/20/profile"));
            assert!(text.contains("Roblox accepted this action"));
        }
        assert!(
            friendship_payload(Event::FriendRemoved, 10, "Synthetic", 20, true)
                .unwrap()
                .to_string()
                .contains("does not identify who initiated")
        );
        assert!(
            friendship_payload(Event::ModerationDetected, 10, "Synthetic", 20, false).is_none()
        );
    }
    #[test]
    fn notification_job_details_include_only_normalized_public_server_ids() {
        let job = "12345678-ABCD-4234-8234-123456789ABC";
        let detail = job_detail(Some(job));
        assert_eq!(
            detail,
            "\n**Job ID:** `12345678-abcd-4234-8234-123456789abc`"
        );
        let notification = payload("Synthetic event", &format!("Place ID: 10{detail}"));
        assert!(notification
            .to_string()
            .contains("12345678-abcd-4234-8234-123456789abc"));
        for value in [
            None,
            Some(""),
            Some("synthetic-private-link"),
            Some("@everyone"),
            Some("00000000-0000-0000-0000-000000000000"),
        ] {
            assert!(job_detail(value).is_empty());
        }
    }

    #[test]
    fn legacy_preferences_preserve_events_and_disable_new_events() {
        let settings: NotificationSettings =
            serde_json::from_str(r#"{"moderationDetected":false,"batchLaunchFinished":true}"#)
                .unwrap();
        assert!(settings.enabled && settings.batch_launch_finished);
        assert!(
            !settings.moderation_detected
                && !settings.session_expired
                && !settings.launch_failed
                && !settings.client_exited
        );
        assert!(settings.destinations.is_empty());
    }

    #[test]
    fn routing_respects_global_event_and_destination_switches() {
        let mut settings = NotificationSettings {
            destinations: vec![
                Destination::new("default".into(), "Primary".into()),
                Destination::new(
                    "00000000-0000-4000-8000-000000000001".into(),
                    "Launches".into(),
                ),
            ],
            ..Default::default()
        };
        settings.destinations[1].events = vec![Event::BatchLaunchFinished];
        assert_eq!(settings.routes(Event::ModerationDetected).len(), 1);
        assert_eq!(settings.routes(Event::BatchLaunchFinished).len(), 2);
        assert!(settings.routes(Event::SessionExpired).is_empty());
        settings.destinations[0].enabled = false;
        assert!(settings.routes(Event::ModerationDetected).is_empty());
        assert_eq!(settings.routes(Event::BatchLaunchFinished).len(), 1);
        settings.batch_launch_finished = false;
        assert!(settings.routes(Event::BatchLaunchFinished).is_empty());
        settings.enabled = false;
        settings.batch_launch_finished = true;
        assert!(settings.routes(Event::BatchLaunchFinished).is_empty());
        assert_eq!(settings.destinations.len(), 2);
    }

    #[test]
    fn credential_changes_roll_back_on_preference_save_failure() {
        let operations = std::cell::RefCell::new(Vec::new());
        assert!(commit_change(
            || {
                operations.borrow_mut().push("update");
                Ok(())
            },
            || {
                operations.borrow_mut().push("save");
                Err("synthetic save failure".into())
            },
            || {
                operations.borrow_mut().push("restore");
                Ok(())
            },
        )
        .is_err());
        assert_eq!(*operations.borrow(), vec!["update", "save", "restore"]);
        assert!(commit_change(
            || Err("synthetic credential failure".into()),
            || panic!("must not save"),
            || panic!("must not restore")
        )
        .is_err());
        assert!(commit_change(|| Ok(()), || Ok(()), || panic!("must not restore")).is_ok());
        assert!(commit_change(
            || Ok(()),
            || Err("synthetic failure".into()),
            || Err("synthetic recovery failure".into())
        )
        .unwrap_err()
        .contains("recovery"));
    }

    #[test]
    fn destinations_reject_duplicates_and_other_credential_names() {
        let mut settings = NotificationSettings {
            destinations: vec![Destination::new("device-key".into(), "Invalid".into())],
            ..Default::default()
        };
        assert!(settings.validate().is_err());
        settings.destinations[0].id = "default".into();
        assert!(settings.validate().is_ok());
        settings.destinations.push(settings.destinations[0].clone());
        assert!(settings.validate().is_err());
    }
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
            ..Default::default()
        };
        storage::atomic_write(&path, &serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        storage::atomic_write(&path, b"invalid").unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
