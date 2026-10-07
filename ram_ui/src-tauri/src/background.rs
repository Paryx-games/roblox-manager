use crate::{accounts, state::AppState, PresenceUpdate};
use ram_core::{api, auth::RobloxClient};
use std::time::Duration;
use tauri::{Emitter, Manager};

pub struct BackgroundTasks(Vec<tauri::async_runtime::JoinHandle<()>>);

impl BackgroundTasks {
    pub fn stop(&self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

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
    let _gate = state.presence_cache.gate.lock().await;
    let credential_state = state.clone();
    let (cookie, viewer, revision) = tauri::async_runtime::spawn_blocking(move || {
        let runtime = credential_state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked {
            return Err("Account store is locked".to_string());
        }
        runtime
            .accounts
            .accounts
            .iter()
            .filter(|account| !account.cookie_expired)
            .find_map(|account| {
                crate::account_cookie(&runtime, account.user_id)
                    .ok()
                    .map(|cookie| {
                        (
                            cookie,
                            account.user_id,
                            *runtime
                                .credential_revisions
                                .get(&account.user_id)
                                .unwrap_or(&0),
                        )
                    })
            })
            .ok_or_else(|| "No authenticated account is available to refresh presence".to_string())
    })
    .await
    .map_err(|_| "Presence credential task failed")??;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let generation = state.presence_cache.generation();
    let mut presences = Vec::new();
    let mut missing = Vec::new();
    for id in ids {
        if let Some(presence) = state.presence_cache.get(&(viewer, revision, id)) {
            presences.push((id, presence));
        } else if !missing.contains(&id) {
            missing.push(id);
        }
    }
    for batch in missing.chunks(50) {
        presences.extend(
            api::fetch_presences(&client, &cookie, batch)
                .await
                .map_err(|_| "Presence could not be refreshed. Check your connection.")?,
        );
    }
    drop(cookie);
    let updates = presences
        .iter()
        .map(|(user_id, presence)| PresenceUpdate {
            user_id: *user_id,
            presence: crate::presence_kind(presence),
            presence_text: presence.status_text().to_string(),
            location: presence.last_location.clone(),
        })
        .collect();
    {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked
            || runtime.accounts.find_by_id(viewer).is_none()
            || *runtime.credential_revisions.get(&viewer).unwrap_or(&0) != revision
        {
            return Err("Presence account changed. Refresh again.".into());
        }
        for (user_id, presence) in presences {
            if missing.contains(&user_id) {
                state.presence_cache.insert(
                    generation,
                    (viewer, revision, user_id),
                    presence.clone(),
                );
            }
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
            ram_core::cached_api::fetch_avatars(&client, batch)
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
    let instance_app = app.clone();
    let process_app = app.clone();
    BackgroundTasks(vec![
        tauri::async_runtime::spawn(async move {
            loop {
                let _ = crate::instances::list_instances(instance_app.clone()).await;
                let state = instance_app.state::<AppState>();
                tokio::time::sleep(crate::instances::sweep_interval(&state)).await;
            }
        }),
        tauri::async_runtime::spawn(async move {
            let is_enabled = process_app
                .state::<AppState>()
                .runtime
                .lock()
                .ok()
                .is_some_and(|runtime| runtime.config.multi_instance_enabled);
            if is_enabled
                && !matches!(
                    tauri::async_runtime::spawn_blocking(ram_core::process::enable_multi_instance)
                        .await,
                    Ok(Ok(()))
                )
            {
                let _ = process_app.emit(
                    "background-notice",
                    "Multi-instance setup failed at startup. Check Settings before launching.",
                );
            }
            loop {
                if let Some(place_id) = crate::browser_login::take_browse_as_launch_request(
                    &crate::lifecycle::data_directory(),
                )
                .filter(|place_id| *place_id > 0)
                {
                    let _ = process_app.emit("browser-play-request", place_id);
                }
                let needs_cleanup = process_app
                    .state::<AppState>()
                    .runtime
                    .lock()
                    .ok()
                    .is_some_and(|runtime| {
                        runtime.config.kill_background_roblox
                            || runtime.config.multi_instance_enabled
                    });
                if needs_cleanup {
                    let state = process_app.state::<AppState>().inner().clone();
                    if let Ok(_launch) = state.launch_queue.try_lock() {
                        let _ = tauri::async_runtime::spawn_blocking(
                            ram_core::process::kill_tray_roblox,
                        )
                        .await;
                    };
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }),
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
