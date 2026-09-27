use crate::{accounts, state::AppState, PresenceUpdate};
use ram_core::{api, auth::RobloxClient};
use std::time::Duration;
use tauri::{Emitter, Manager};

pub struct BackgroundTasks(Vec<tauri::async_runtime::JoinHandle<()>>);

impl Drop for BackgroundTasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

fn unlocked_ids(app: &tauri::AppHandle) -> Vec<u64> {
    let state = app.state::<AppState>();
    let Ok(runtime) = state.runtime.lock() else {
        return Vec::new();
    };
    if !runtime.unlocked {
        return Vec::new();
    }
    runtime
        .accounts
        .accounts
        .iter()
        .map(|account| account.user_id)
        .collect()
}

pub async fn refresh_presence(
    app: &tauri::AppHandle,
    requested_ids: Vec<u64>,
) -> Result<Vec<PresenceUpdate>, String> {
    let state = app.state::<AppState>().inner().clone();
    let ids = if requested_ids.is_empty() {
        unlocked_ids(app)
    } else {
        requested_ids
    };
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let cookie = tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        runtime
            .accounts
            .accounts
            .iter()
            .filter(|account| !account.cookie_expired)
            .find_map(|account| crate::account_cookie(&runtime, account.user_id).ok())
            .ok_or_else(|| "No authenticated account is available to refresh presence".to_string())
    })
    .await
    .map_err(|_| "Presence credential task failed")??;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let mut presences = Vec::new();
    for batch in ids.chunks(50) {
        presences.extend(
            api::fetch_presences(&client, &cookie, batch)
                .await
                .map_err(|_| "Presence could not be refreshed. Check your connection.")?,
        );
    }
    let updates = presences
        .iter()
        .map(|(user_id, presence)| PresenceUpdate {
            user_id: *user_id,
            presence: crate::presence_kind(presence),
            presence_text: presence.status_text().to_string(),
            location: presence.last_location.clone(),
        })
        .collect();
    let state = app.state::<AppState>();
    {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        for (user_id, presence) in presences {
            if let Some(account) = runtime.accounts.find_by_id_mut(user_id) {
                account.last_presence = presence;
            }
        }
    }
    accounts::publish(app);
    Ok(updates)
}

async fn refresh_avatars(app: &tauri::AppHandle, ids: &[u64]) -> Result<(), String> {
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let mut avatars = Vec::new();
    for batch in ids.chunks(100) {
        avatars.extend(
            api::fetch_avatars(&client, batch)
                .await
                .map_err(|_| "Account avatars could not be refreshed")?,
        );
    }
    let state = app.state::<AppState>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        let previous = runtime.accounts.clone();
        let mut has_changes = false;
        for (user_id, url) in avatars {
            if let Some(account) = runtime.accounts.find_by_id_mut(user_id) {
                if account.avatar_url != url {
                    account.avatar_url = url;
                    has_changes = true;
                }
            }
        }
        if has_changes && crate::save_runtime(&runtime).is_err() {
            runtime.accounts = previous;
            return Err("Account avatars could not be saved".into());
        }
        Ok(())
    })
    .await
    .map_err(|_| "Avatar refresh task failed")??;
    accounts::publish(app);
    Ok(())
}

pub fn start(app: &tauri::AppHandle) -> BackgroundTasks {
    let validation_app = app.clone();
    let presence_app = app.clone();
    let avatar_app = app.clone();
    BackgroundTasks(vec![
        tauri::async_runtime::spawn(async move {
            let mut last_refresh = None::<std::time::Instant>;
            loop {
                let ids = unlocked_ids(&validation_app);
                if !ids.is_empty()
                    && last_refresh.is_none_or(|last| last.elapsed() >= Duration::from_secs(300))
                {
                    last_refresh = Some(std::time::Instant::now());
                    if accounts::refresh(&validation_app, Vec::new())
                        .await
                        .is_err()
                    {
                        let _ = validation_app.emit(
                            "background-notice",
                            "Automatic account refresh failed. Use Refresh accounts to retry.",
                        );
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }),
        tauri::async_runtime::spawn(async move {
            loop {
                let ids = unlocked_ids(&presence_app);
                if !ids.is_empty() {
                    let _ = refresh_presence(&presence_app, ids).await;
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }),
        tauri::async_runtime::spawn(async move {
            loop {
                let ids = unlocked_ids(&avatar_app);
                if !ids.is_empty() {
                    let _ = refresh_avatars(&avatar_app, &ids).await;
                }
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }),
    ])
}
