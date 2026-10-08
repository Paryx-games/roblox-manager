use crate::{accounts, instances, state::AppState};
use ram_core::{auth::RobloxClient, process};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

pub struct LaunchRequest {
    pub user_id: u64,
    pub place_id: u64,
    pub job_id: Option<String>,
    pub data: Option<String>,
    pub link_code: Option<String>,
    pub access_code: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchProgress {
    request_id: String,
    user_id: u64,
    phase: &'static str,
}

fn progress(app: &tauri::AppHandle, request_id: &str, user_id: u64, phase: &'static str) {
    let _ = app.emit(
        "launch-progress",
        LaunchProgress {
            request_id: request_id.into(),
            user_id,
            phase,
        },
    );
}

struct LaunchSpacing {
    configured_delay: u32,
    needs_tray_cleanup: bool,
}

fn launch_spacing(options: LaunchSpacing) -> Duration {
    Duration::from_secs(
        u64::from(options.configured_delay).max(if options.needs_tray_cleanup { 3 } else { 0 }),
    )
}

fn validate_request(request: &LaunchRequest) -> Result<(), String> {
    if request.user_id == 0 || request.place_id == 0 {
        return Err("Choose an account and a nonzero Place ID".into());
    }
    for value in [
        &request.job_id,
        &request.data,
        &request.link_code,
        &request.access_code,
    ]
    .into_iter()
    .flatten()
    {
        if value.len() > 4096
            || value.contains('\0')
            || value.contains('\r')
            || value.contains('\n')
        {
            return Err("Launch input contains invalid characters or is too long".into());
        }
    }
    Ok(())
}

pub async fn launch(app: &tauri::AppHandle, request: LaunchRequest) -> Result<(), String> {
    launch_impl(app, request, false).await
}

pub async fn launch_startup(app: &tauri::AppHandle, request: LaunchRequest) -> Result<(), String> {
    launch_impl(app, request, true).await
}

async fn launch_impl(
    app: &tauri::AppHandle,
    request: LaunchRequest,
    startup: bool,
) -> Result<(), String> {
    validate_request(&request)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    progress(app, &request_id, request.user_id, "waiting");
    let result = launch_queued(app, &request_id, &request, startup).await;
    progress(
        app,
        &request_id,
        request.user_id,
        if result.is_ok() {
            "requested"
        } else {
            "failed"
        },
    );
    result
}

pub async fn batch(app: &tauri::AppHandle, requests: Vec<LaunchRequest>) -> Result<(), String> {
    let total = requests.len();
    let place_id = requests
        .first()
        .map(|request| request.place_id)
        .unwrap_or(0);
    for (completed, request) in requests.into_iter().enumerate() {
        if let Err(error) = launch(app, request).await {
            crate::discord::batch_finished(app, total, completed, place_id);
            return Err(error);
        }
    }
    crate::discord::batch_finished(app, total, total, place_id);
    Ok(())
}

#[tauri::command]
pub async fn launch_accounts(
    app: tauri::AppHandle,
    user_ids: Vec<u64>,
    place_id: u64,
    job_id: Option<String>,
    data: Option<String>,
) -> Result<(), String> {
    if user_ids.is_empty() || user_ids.len() > 500 {
        return Err("Select between 1 and 500 accounts".into());
    }
    batch(
        &app,
        user_ids
            .into_iter()
            .map(|user_id| LaunchRequest {
                user_id,
                place_id,
                job_id: job_id.clone(),
                data: data.clone(),
                link_code: None,
                access_code: None,
            })
            .collect(),
    )
    .await
}

async fn launch_queued(
    app: &tauri::AppHandle,
    request_id: &str,
    request: &LaunchRequest,
    startup: bool,
) -> Result<(), String> {
    let state = app.state::<AppState>().inner().clone();
    let mut next_launch = state.launch_queue.lock().await;
    if let Some(deadline) = *next_launch {
        tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
    }
    let credential_state = state.clone();
    let user_id = request.user_id;
    let startup_place = request.place_id;
    let (cookie, config, player_path, revision) = tauri::async_runtime::spawn_blocking(move || {
        let runtime = credential_state.runtime.lock().map_err(|_| "Account state unavailable")?;
        if startup && (!runtime.config.auto_launch_on_startup || runtime.config.auto_launch_account_id != Some(user_id)
            || runtime.config.auto_launch_place_id != Some(startup_place)) { return Err("Startup launch was disabled or changed while waiting.".to_string()); }
        let account = runtime.accounts.find_by_id(user_id).ok_or("Account not found")?;
        if account.cookie_expired || !account.can_launch() { return Err("This account is restricted or its credential has expired. Refresh or re-add it before launching.".to_string()); }
        let cookie = crate::account_cookie(&runtime, user_id).map_err(|_| "Account credential unavailable. Unlock or re-add the account.")?;
        let player_path = runtime.config.custom_player_paths.get(&user_id).cloned().or_else(|| runtime.config.roblox_player_path.clone());
        Ok((cookie, runtime.config.clone(), player_path, *runtime.credential_revisions.get(&user_id).unwrap_or(&0)))
    }).await.map_err(|_| "Launch credential task failed")??;
    let options_state = state.clone();
    let options_config = config.clone();
    let (executable, custom_args) = tauri::async_runtime::spawn_blocking(move || {
        let executable = process::resolve_player_executable(player_path.as_deref())
            .map_err(|_| "RobloxPlayerBeta.exe was not found. Check the player path in Settings.")?;
        let args = ram_core::launch_options::parse_arguments(&options_config.custom_game_args)
            .map_err(|_| "Custom Roblox arguments are invalid. Check quoting and reserved launch parameters in Settings.")?;
        ram_core::launch_options::apply_fast_flags(&executable, &options_config.roblox_fast_flags)
            .map_err(|_| "Fast flags could not be applied. Check the selected installation and its ClientSettings files.")?;
        crate::client_settings::apply_before_launch(&options_state)?;
        Ok::<_, String>((executable, args))
    }).await.map_err(|_| "Launch options task failed")??;
    if config.mac_rotation_enabled && !state.mac_rotated.load(std::sync::atomic::Ordering::Acquire)
    {
        progress(app, request_id, user_id, "preparing");
        let mac_config = config.clone();
        let mac_state = state.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if mac_state.is_shutting_down.load(std::sync::atomic::Ordering::Acquire) { return Err("RM is closing. Launch cancelled."); }
            if process::is_roblox_running() { return Err("Close Roblox before the first launch with automatic MAC rotation enabled."); }
            process::rotate_mac_address(mac_config.mac_preserve_oui, &mac_config.mac_alternate_oui)
                .map_err(|_| "Automatic MAC rotation failed or was cancelled. Check adapter support and administrator permission, or turn it off in Settings.")?;
            mac_state.mac_rotated.store(true, std::sync::atomic::Ordering::Release);
            Ok(())
        }).await.map_err(|_| "Automatic MAC rotation task failed")??;
    }
    let multi_instance = config.multi_instance_enabled;
    let needs_tray_cleanup = config.kill_background_roblox || multi_instance;
    progress(app, request_id, request.user_id, "authenticating");
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let ticket = client
        .generate_auth_ticket(&cookie)
        .await
        .map_err(|_| "Roblox rejected the launch request. Refresh the account and try again.")?;
    drop(cookie);
    let cleanup_options = config.privacy_cleanup_options();
    let cleanup = tauri::async_runtime::spawn_blocking(move || {
        if multi_instance {
            process::enable_multi_instance().map_err(|_| "Multi-instance setup failed")?;
        }
        if needs_tray_cleanup {
            process::kill_tray_roblox();
        }
        process::prepare_privacy_cleanup(cleanup_options)
            .map_err(|_| "Privacy cleanup failed. Roblox was not launched.")
    })
    .await
    .map_err(|_| "Launch preparation task failed")??;
    let tracking_state = state.clone();
    let place_id = request.place_id;
    let token = tauri::async_runtime::spawn_blocking(move || {
        instances::note_launch(&tracking_state, user_id, place_id)
    })
    .await
    .map_err(|_| "Instance tracking task failed")??;
    let job_id = request.job_id.clone();
    let data = request.data.clone();
    let link_code = request.link_code.clone();
    let access_code = request.access_code.clone();
    progress(app, request_id, request.user_id, "launching");
    let clear_clipboard = config.privacy_mode && config.privacy_clear_clipboard;
    let launch_state = state.clone();
    let launch_result = tauri::async_runtime::spawn_blocking(move || {
        {
            let runtime = launch_state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            if launch_state
                .is_shutting_down
                .load(std::sync::atomic::Ordering::Acquire)
                || !runtime.unlocked
                || runtime
                    .accounts
                    .find_by_id(user_id)
                    .is_none_or(|account| account.cookie_expired || !account.can_launch())
                || *runtime.credential_revisions.get(&user_id).unwrap_or(&0) != revision
            {
                return Err("The account changed or RM is closing. Launch cancelled.");
            }
        }
        process::launch_game_with_args(
            &ticket,
            place_id,
            job_id.as_deref(),
            link_code.as_deref(),
            access_code.as_deref(),
            data.as_deref(),
            token,
            Some(&executable),
            &custom_args,
        )
        .map_err(|_| "Roblox could not be launched. Check the configured player path.")?;
        let privacy_backup_cleaned = cleanup.commit().is_ok();
        let mut clipboard_cleared = true;
        #[cfg(windows)]
        if clear_clipboard {
            clipboard_cleared = process::clear_clipboard().is_ok();
        }
        Ok::<_, &str>((privacy_backup_cleaned, clipboard_cleared))
    })
    .await
    .map_err(|_| "Roblox launch task failed")?;
    *next_launch = Some(
        Instant::now()
            + launch_spacing(LaunchSpacing {
                configured_delay: config.launch_delay_secs,
                needs_tray_cleanup,
            }),
    );
    let (privacy_backup_cleaned, clipboard_cleared) = launch_result?;
    if !privacy_backup_cleaned {
        let _ = app.emit(
            "background-notice",
            "Roblox launched, but privacy backup cleanup did not complete.",
        );
    }
    if !clipboard_cleared {
        let _ = app.emit(
            "background-notice",
            "Roblox launched, but the clipboard could not be cleared. Another app may be using it.",
        );
    }
    if config.auto_arrange_windows {
        instances::schedule_arrangement(
            &state,
            Duration::from_secs(u64::from(config.launch_delay_secs).saturating_add(2).max(5)),
        );
    }
    let save_state = state.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = save_state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        let previous = runtime.accounts.clone();
        if let Some(account) = runtime.accounts.find_by_id_mut(user_id) {
            account.last_used = Some(chrono::Utc::now());
        }
        if crate::save_runtime(&runtime).is_err() {
            runtime.accounts = previous;
            return Err("Launch timestamp could not be saved");
        }
        Ok(())
    })
    .await;
    if !matches!(saved, Ok(Ok(()))) {
        let _ = app.emit(
            "background-notice",
            "Roblox launched, but its last-used timestamp could not be saved.",
        );
    }
    accounts::publish(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_preserves_the_tray_cleanup_window() {
        assert_eq!(
            launch_spacing(LaunchSpacing {
                configured_delay: 0,
                needs_tray_cleanup: true
            }),
            Duration::from_secs(3)
        );
        assert_eq!(
            launch_spacing(LaunchSpacing {
                configured_delay: 8,
                needs_tray_cleanup: true
            }),
            Duration::from_secs(8)
        );
        assert_eq!(
            launch_spacing(LaunchSpacing {
                configured_delay: 0,
                needs_tray_cleanup: false
            }),
            Duration::ZERO
        );
    }

    #[test]
    fn launch_inputs_cannot_contain_control_characters() {
        let request = LaunchRequest {
            user_id: 1,
            place_id: 2,
            job_id: Some("server\nargument".into()),
            data: None,
            link_code: None,
            access_code: None,
        };
        assert!(validate_request(&request).is_err());
    }
}
