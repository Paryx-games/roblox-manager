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
    crate::history::initialize(&state).await?;
    let ids = if requested_ids.is_empty() {
        unlocked_ids(app)
    } else {
        requested_ids
    };
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let viewers = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked {
            return Err("Account store is locked".into());
        }
        runtime
            .accounts
            .accounts
            .iter()
            .filter(|account| !account.cookie_expired)
            .map(|account| account.user_id)
            .collect::<Vec<_>>()
    };
    let groups = presence_groups(&ids, &viewers);
    if groups.is_empty() {
        return Err("No authenticated account is available to refresh presence".into());
    }
    let mut updates = Vec::new();
    let mut failure = None;
    for (viewer, ids) in groups {
        match refresh_presence_as(app, ids, viewer).await {
            Ok(batch) => updates.extend(batch),
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    accounts::publish(app);
    crate::history::publish(app);
    match failure {
        Some(error) => Err(error),
        None => Ok(updates),
    }
}

pub(crate) fn presence_viewer(target: u64, viewers: &[u64], fallback: u64) -> u64 {
    // Another account's view may hide the saved target's game and server.
    if viewers.contains(&target) {
        target
    } else {
        fallback
    }
}

fn presence_groups(ids: &[u64], viewers: &[u64]) -> Vec<(u64, Vec<u64>)> {
    let Some(&fallback) = viewers.first() else {
        return Vec::new();
    };
    let mut groups: Vec<(u64, Vec<u64>)> = Vec::new();
    for &id in ids {
        let viewer = presence_viewer(id, viewers, fallback);
        if let Some((_, targets)) = groups.iter_mut().find(|(owner, _)| *owner == viewer) {
            if !targets.contains(&id) {
                targets.push(id);
            }
        } else {
            groups.push((viewer, vec![id]));
        }
    }
    groups
}

async fn refresh_presence_as(
    app: &tauri::AppHandle,
    ids: Vec<u64>,
    viewer: u64,
) -> Result<Vec<PresenceUpdate>, String> {
    let state = app.state::<AppState>().inner().clone();
    let _gate = state.presence_cache.gate.lock().await;
    let credential_state = state.clone();
    let (cookie, revision) = tauri::async_runtime::spawn_blocking(move || {
        let runtime = credential_state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked {
            return Err("Account store is locked".to_string());
        }
        let account = runtime
            .accounts
            .find_by_id(viewer)
            .filter(|account| !account.cookie_expired)
            .ok_or_else(|| "Presence account changed. Refresh again.".to_string())?;
        let cookie = crate::account_cookie(&runtime, account.user_id)?;
        Ok((
            cookie,
            *runtime.credential_revisions.get(&viewer).unwrap_or(&0),
        ))
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
    let checked_at = chrono::Utc::now();
    for batch in missing.chunks(50) {
        presences.extend(
            api::fetch_presences(&client, &cookie, batch)
                .await
                .map_err(|_| "Presence could not be refreshed. Check your connection.")?,
        );
    }
    drop(cookie);
    for (target, presence) in &presences {
        tracing::debug!(
            viewer_id = viewer,
            target_id = target,
            presence_type = presence.user_presence_type,
            has_place = presence.place_id.is_some_and(|id| id > 0),
            has_job = ram_core::instances::normalize_job_id(presence.game_id.as_deref()).is_some(),
            "Presence visibility checked"
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
    let mut checked_presences = Vec::new();
    {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked
            || runtime
                .accounts
                .find_by_id(viewer)
                .is_none_or(|account| account.cookie_expired)
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
                if missing.contains(&user_id) {
                    checked_presences.push((user_id, presence.clone()));
                }
                if let Ok(mut history) = state.history.lock() {
                    if history.loaded {
                        history.observe_presence(user_id, &presence, chrono::Utc::now());
                    }
                }
                account.last_presence = presence;
            }
        }
    }
    crate::instances::record_presence_check(&state, &checked_presences, checked_at)?;
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
    let friends_app = app.clone();
    let private_servers_app = app.clone();
    let discord_app = app.clone();
    let validation_app = app.clone();
    let presence_app = app.clone();
    let avatar_app = app.clone();
    let instance_app = app.clone();
    let process_app = app.clone();
    let startup_app = app.clone();
    let history_app = app.clone();
    BackgroundTasks(vec![
        tauri::async_runtime::spawn(crate::friends::run_notifications(friends_app)),
        tauri::async_runtime::spawn(crate::private_servers::run_background(private_servers_app)),
        tauri::async_runtime::spawn(crate::discord::run(discord_app)),
        tauri::async_runtime::spawn(async move {
            loop {
                let state = history_app.state::<AppState>().inner().clone();
                flush_history(&history_app, &state).await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }),
        tauri::async_runtime::spawn(crate::startup_actions::run(startup_app)),
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
                if !ids.is_empty() && last_refresh.is_none() {
                    last_refresh = Some(std::time::Instant::now());
                }
                if !ids.is_empty()
                    && last_refresh.is_some_and(|last| last.elapsed() >= Duration::from_secs(300))
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

async fn flush_history(app: &tauri::AppHandle, state: &AppState) {
    if crate::history::flush(state).await.is_err() {
        crate::history::publish(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_targets_use_their_own_presence_view_even_when_another_viewer_is_first() {
        assert_eq!(presence_viewer(2, &[1, 2, 3], 1), 2);
        assert_eq!(
            presence_groups(&[3, 2, 1], &[1, 2, 3]),
            vec![(3, vec![3]), (2, vec![2]), (1, vec![1])]
        );
    }

    #[test]
    fn external_or_unavailable_targets_keep_the_fallback_view_and_are_batched_once() {
        assert_eq!(presence_viewer(9, &[1, 2], 2), 2);
        assert_eq!(
            presence_groups(&[2, 9, 1, 9, 8, 2], &[1, 2]),
            vec![(2, vec![2]), (1, vec![9, 1, 8])]
        );
        assert!(presence_groups(&[1, 2], &[]).is_empty());
    }
}
