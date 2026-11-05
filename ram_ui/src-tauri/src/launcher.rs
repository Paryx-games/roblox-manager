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
    pub overrides: LaunchOverrides,
}

#[derive(Clone, Copy, Default)]
pub struct LaunchOverrides {
    pub privacy_mode: Option<bool>,
    pub multi_instance_enabled: Option<bool>,
    pub auto_arrange_windows: Option<bool>,
}

#[derive(Clone, Copy, PartialEq)]
enum LaunchMode {
    Game,
    Home,
}

#[tauri::command]
pub async fn open_account_home(app: tauri::AppHandle, user_id: u64) -> Result<(), String> {
    launch_impl(
        &app,
        LaunchRequest {
            user_id,
            place_id: 0,
            job_id: None,
            data: None,
            link_code: None,
            access_code: None,
            overrides: LaunchOverrides::default(),
        },
        false,
        false,
        false,
        LaunchMode::Home,
        None,
    )
    .await
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchProgress {
    request_id: String,
    user_id: u64,
    friend_target_id: Option<u64>,
    phase: &'static str,
    reason_code: Option<&'static str>,
}

fn progress(
    app: &tauri::AppHandle,
    request_id: &str,
    user_id: u64,
    friend_target_id: Option<u64>,
    phase: &'static str,
    reason_code: Option<&'static str>,
) {
    tracing::info!(
        request_id,
        user_id,
        phase,
        reason_code = reason_code.unwrap_or("none"),
        "launch status"
    );
    let _ = app.emit(
        "launch-progress",
        LaunchProgress {
            request_id: request_id.into(),
            user_id,
            friend_target_id,
            phase,
            reason_code,
        },
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaunchFailureStage {
    Queued,
    Preflight,
    Authentication,
    RateLimited,
    Transport,
    PrivacyPreparation,
    ProcessStart,
}

fn launch_failure(stage: LaunchFailureStage, error: &str) -> (&'static str, &'static str) {
    let code = if error.contains("Choose an account") || error.contains("valid account") {
        "invalid_account_or_target"
    } else if error.contains("Launch input contains") {
        "invalid_launch_input"
    } else if error.contains("Select between") {
        "invalid_batch_size"
    } else if error.contains("Account not found") {
        "account_missing"
    } else if error.contains("restricted") || error.contains("credential has expired") {
        "account_restricted_or_expired"
    } else if error.contains("credential unavailable") {
        "account_credential_unavailable"
    } else if error.contains("Multi-instance was disabled") {
        "multi_instance_disabled"
    } else if error.contains("Startup launch was disabled") {
        "startup_settings_changed"
    } else if error.contains("RobloxPlayerBeta.exe was not found") {
        "player_executable_missing"
    } else if error.contains("Custom Roblox arguments") {
        "custom_arguments_invalid"
    } else if error.contains("Fast flags") {
        "fast_flags_failed"
    } else if error.contains("Privacy cleanup failed") {
        "privacy_cleanup_failed"
    } else if error.contains("Close Roblox before") {
        "mac_rotation_blocked"
    } else if error.contains("MAC rotation failed or was cancelled") {
        "mac_rotation_failed"
    } else if error.contains("account changed or RM is closing") {
        "launch_cancelled"
    } else if error.contains("private-server invitation") {
        "private_server_access_failed"
    } else if error.contains("Roblox rejected the launch request") {
        "roblox_auth_rejected"
    } else if error.contains("could not be launched") {
        "process_launch_failed"
    } else {
        match stage {
            LaunchFailureStage::Queued => "launch_queue_failed",
            LaunchFailureStage::Preflight => "launch_preflight_failed",
            LaunchFailureStage::Authentication => "roblox_request_failed",
            LaunchFailureStage::RateLimited => "roblox_rate_limited",
            LaunchFailureStage::Transport => "roblox_connection_failed",
            LaunchFailureStage::PrivacyPreparation => "launch_preparation_failed",
            LaunchFailureStage::ProcessStart => "process_launch_failed",
        }
    };
    let status = match code {
        "invalid_account_or_target"
        | "invalid_launch_input"
        | "invalid_batch_size"
        | "account_missing"
        | "account_restricted_or_expired"
        | "account_credential_unavailable"
        | "multi_instance_disabled"
        | "startup_settings_changed"
        | "player_executable_missing"
        | "custom_arguments_invalid"
        | "fast_flags_failed"
        | "privacy_cleanup_failed"
        | "mac_rotation_blocked"
        | "mac_rotation_failed"
        | "launch_cancelled" => "blocked",
        _ => match stage {
            LaunchFailureStage::Preflight | LaunchFailureStage::PrivacyPreparation => "blocked",
            LaunchFailureStage::Queued
            | LaunchFailureStage::Authentication
            | LaunchFailureStage::RateLimited
            | LaunchFailureStage::Transport
            | LaunchFailureStage::ProcessStart => "failed",
        },
    };
    (status, code)
}

fn auth_ticket_failure(error: ram_core::CoreError, stage: &mut LaunchFailureStage) -> String {
    match error {
        ram_core::CoreError::RateLimited => {
            *stage = LaunchFailureStage::RateLimited;
            "Roblox rate-limited the launch request. Wait before trying again.".into()
        }
        ram_core::CoreError::Http(_) => {
            *stage = LaunchFailureStage::Transport;
            "Roblox could not be reached. Check the connection and retry.".into()
        }
        ram_core::CoreError::CookieRejected
        | ram_core::CoreError::CookieRejectedWithReason(_)
        | ram_core::CoreError::RobloxApi {
            status: 401 | 403, ..
        } => "Roblox rejected the launch request. Refresh the account and try again.".into(),
        _ => "Roblox could not complete the launch request. Check the account and connection, then retry."
            .into(),
    }
}

fn batch_outcome(total: usize, launched: usize) -> Result<(), String> {
    let failed = total.saturating_sub(launched);
    if failed == 0 {
        Ok(())
    } else {
        Err(format!(
            "{launched} of {total} accounts launched; {failed} failed or were blocked. See the RM log for per-account reasons."
        ))
    }
}

struct LaunchSpacing {
    configured_delay: u32,
    needs_tray_cleanup: bool,
}

struct LaunchPacing<'a> {
    deadline: &'a mut Option<Instant>,
    failure_delay: Duration,
    armed: bool,
}

impl LaunchPacing<'_> {
    fn arm(&mut self, delay: Duration) {
        self.failure_delay = delay;
        self.armed = true;
    }

    fn schedule(&mut self, delay: Duration) {
        *self.deadline = Some(Instant::now() + delay);
        self.armed = false;
    }
}

impl Drop for LaunchPacing<'_> {
    fn drop(&mut self) {
        if self.armed {
            *self.deadline = Some(Instant::now() + self.failure_delay);
        }
    }
}

fn launch_spacing(options: LaunchSpacing) -> Duration {
    Duration::from_secs(
        u64::from(options.configured_delay).max(if options.needs_tray_cleanup { 3 } else { 0 }),
    )
}

fn apply_launch_overrides(config: &mut ram_core::models::AppConfig, overrides: LaunchOverrides) {
    if let Some(value) = overrides.privacy_mode {
        config.privacy_mode = value;
    }
    if let Some(value) = overrides.multi_instance_enabled {
        config.multi_instance_enabled = value;
    }
    if let Some(value) = overrides.auto_arrange_windows {
        config.auto_arrange_windows = value;
    }
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
    launch_impl(app, request, false, false, false, LaunchMode::Game, None).await
}

pub async fn launch_startup(app: &tauri::AppHandle, request: LaunchRequest) -> Result<(), String> {
    launch_impl(app, request, true, false, false, LaunchMode::Game, None).await
}

pub async fn launch_friend(
    app: &tauri::AppHandle,
    request: LaunchRequest,
    friend_target_id: u64,
    reuse_confirmed: bool,
    require_new_instance: bool,
) -> Result<(), String> {
    let (multi_instance, check_account_session) = if require_new_instance || !reuse_confirmed {
        (true, !reuse_confirmed)
    } else {
        (false, false)
    };
    launch_impl(
        app,
        request,
        false,
        multi_instance,
        check_account_session,
        LaunchMode::Game,
        Some(friend_target_id),
    )
    .await
}

fn validate_instance_policy(required: bool, enabled: bool) -> Result<(), String> {
    if required && !enabled {
        return Err("Multi-instance was disabled before this launch. Enable it in Settings or confirm the saved launch policy before retrying.".into());
    }
    Ok(())
}

async fn launch_impl(
    app: &tauri::AppHandle,
    request: LaunchRequest,
    startup: bool,
    require_multi_instance: bool,
    check_account_session: bool,
    mode: LaunchMode,
    friend_target_id: Option<u64>,
) -> Result<(), String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut stage = LaunchFailureStage::Queued;
    progress(
        app,
        &request_id,
        request.user_id,
        friend_target_id,
        "queued",
        None,
    );
    if let Err(error) = validate_launch_mode(&request, mode) {
        let (status, reason_code) = launch_failure(LaunchFailureStage::Preflight, &error);
        progress(
            app,
            &request_id,
            request.user_id,
            friend_target_id,
            status,
            Some(reason_code),
        );
        return Err(error);
    }
    let result = launch_queued(
        app,
        &request_id,
        &request,
        &mut stage,
        startup,
        require_multi_instance,
        check_account_session,
        mode,
        friend_target_id,
    )
    .await;
    if result.is_ok() && instances::list_instances(app.clone()).await.is_err() {
        let _ = app.emit("background-notice", "Roblox launch was requested, but its starting status could not be refreshed. Check Instances before launching again.");
    }
    if result.is_err() {
        crate::discord::launch_failed(
            app,
            request.user_id,
            request.place_id,
            request.job_id.as_deref(),
        );
    }
    if let Err(error) = &result {
        let (status, reason_code) = launch_failure(stage, error);
        progress(
            app,
            &request_id,
            request.user_id,
            friend_target_id,
            status,
            Some(reason_code),
        );
        tracing::warn!(
            request_id,
            user_id = request.user_id,
            reason_code,
            "launch attempt ended"
        );
    } else {
        progress(
            app,
            &request_id,
            request.user_id,
            friend_target_id,
            "started",
            None,
        );
    }
    result
}

fn validate_launch_mode(request: &LaunchRequest, mode: LaunchMode) -> Result<(), String> {
    if mode == LaunchMode::Game {
        return validate_request(request);
    }
    if request.user_id == 0
        || request.place_id != 0
        || request.job_id.is_some()
        || request.data.is_some()
        || request.link_code.is_some()
        || request.access_code.is_some()
    {
        return Err("Choose a valid account to open Roblox home".into());
    }
    Ok(())
}

fn common_job_id(requests: &[LaunchRequest]) -> Option<String> {
    let first = requests.first()?;
    let job = ram_core::instances::normalize_job_id(first.job_id.as_deref())?;
    requests
        .iter()
        .all(|request| {
            request.place_id == first.place_id
                && ram_core::instances::normalize_job_id(request.job_id.as_deref()).as_ref()
                    == Some(&job)
        })
        .then_some(job)
}

pub async fn batch(app: &tauri::AppHandle, requests: Vec<LaunchRequest>) -> Result<(), String> {
    let total = requests.len();
    let place_id = requests
        .first()
        .map(|request| request.place_id)
        .unwrap_or(0);
    let job_id = common_job_id(&requests);
    let mut launched = 0;
    for request in requests {
        if launch(app, request).await.is_ok() {
            launched += 1;
        }
    }
    crate::discord::batch_finished(app, total, launched, place_id, job_id.as_deref());
    batch_outcome(total, launched)
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
                overrides: LaunchOverrides::default(),
            })
            .collect(),
    )
    .await
}

async fn launch_queued(
    app: &tauri::AppHandle,
    request_id: &str,
    request: &LaunchRequest,
    stage: &mut LaunchFailureStage,
    startup: bool,
    require_multi_instance: bool,
    check_account_session: bool,
    mode: LaunchMode,
    friend_target_id: Option<u64>,
) -> Result<(), String> {
    let state = app.state::<AppState>().inner().clone();
    let mut next_launch = state.launch_queue.lock().await;
    if let Some(deadline) = *next_launch {
        tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
    }
    let mut pacing = LaunchPacing {
        deadline: &mut next_launch,
        failure_delay: Duration::ZERO,
        armed: false,
    };
    *stage = LaunchFailureStage::Preflight;
    progress(
        app,
        request_id,
        request.user_id,
        friend_target_id,
        "starting",
        None,
    );
    if check_account_session {
        instances::require_no_account_session(&state, request.user_id)?;
    }
    let credential_state = state.clone();
    let user_id = request.user_id;
    let startup_place = request.place_id;
    let (cookie, mut config, player_path, revision) = tauri::async_runtime::spawn_blocking(move || {
        let runtime = credential_state.runtime.lock().map_err(|_| "Account state unavailable")?;
        if startup && (!runtime.config.auto_launch_on_startup || runtime.config.auto_launch_account_id != Some(user_id)
            || runtime.config.auto_launch_place_id != Some(startup_place)) { return Err("Startup launch was disabled or changed while waiting.".to_string()); }
        let account = runtime.accounts.find_by_id(user_id).ok_or("Account not found")?;
        if account.cookie_expired || !account.can_launch() { return Err("This account is restricted or its credential has expired. Refresh or re-add it before launching.".to_string()); }
        let cookie = crate::account_cookie(&runtime, user_id).map_err(|_| "Account credential unavailable. Unlock or re-add the account.")?;
        let player_path = runtime.config.custom_player_paths.get(&user_id).cloned().or_else(|| runtime.config.roblox_player_path.clone());
        Ok((cookie, runtime.config.clone(), player_path, *runtime.credential_revisions.get(&user_id).unwrap_or(&0)))
    }).await.map_err(|_| "Launch credential task failed")??;
    apply_launch_overrides(&mut config, request.overrides);
    validate_instance_policy(require_multi_instance, config.multi_instance_enabled)?;
    let options_state = state.clone();
    let options_config = config.clone();
    let (executable, custom_args) = tauri::async_runtime::spawn_blocking(move || {
        let executable = process::resolve_player_executable(player_path.as_deref())
            .map_err(|_| "RobloxPlayerBeta.exe was not found. Check the player path in Settings.")?;
        let mut args = ram_core::launch_options::parse_arguments(&options_config.custom_game_args)
            .map_err(|_| "Custom Roblox arguments are invalid. Check quoting and reserved launch parameters in Settings.")?;
        ram_core::launch_options::apply_fast_flags(&executable, &options_config.roblox_fast_flags)
            .map_err(|_| "Fast flags could not be applied. Check the selected installation and its ClientSettings files.")?;
        args.extend(crate::client_settings::apply_before_launch(&options_state)?);
        Ok::<_, String>((executable, args))
    }).await.map_err(|_| "Launch options task failed")??;
    if config.mac_rotation_enabled && !state.mac_rotated.load(std::sync::atomic::Ordering::Acquire)
    {
        progress(app, request_id, user_id, friend_target_id, "starting", None);
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
    pacing.arm(launch_spacing(LaunchSpacing {
        configured_delay: config.launch_delay_secs,
        needs_tray_cleanup,
    }));
    *stage = LaunchFailureStage::Authentication;
    progress(
        app,
        request_id,
        request.user_id,
        friend_target_id,
        "starting",
        None,
    );
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let resolved_access_code = match request.link_code.as_deref() {
        Some(link_code) => Some(ram_core::api::resolve_private_server_access(&client, &cookie, request.place_id, link_code).await
            .map_err(|_| "This private-server invitation could not be resolved. Check its link and this account's access before trying again.")?),
        None => request.access_code.clone(),
    };
    let ticket = client
        .generate_auth_ticket_with_retry_callback(&cookie, |_| {
            progress(
                app,
                request_id,
                request.user_id,
                friend_target_id,
                "retrying",
                None,
            );
        })
        .await
        .map_err(|error| auth_ticket_failure(error, stage))?;
    drop(cookie);
    let cleanup_options = config.privacy_cleanup_options();
    *stage = LaunchFailureStage::PrivacyPreparation;
    let cleanup = tauri::async_runtime::spawn_blocking(move || {
        if needs_tray_cleanup {
            process::kill_tray_roblox();
        }
        if multi_instance {
            std::thread::sleep(Duration::from_millis(500));
            process::enable_multi_instance().map_err(|error| error.to_string())?;
        }
        process::prepare_privacy_cleanup(cleanup_options)
            .map_err(|_| "Privacy cleanup failed. Roblox was not launched.".to_string())
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
    let access_code = resolved_access_code;
    *stage = LaunchFailureStage::ProcessStart;
    progress(
        app,
        request_id,
        request.user_id,
        friend_target_id,
        "starting",
        None,
    );
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
        match mode {
            LaunchMode::Home => {
                process::launch_home_with_args(&ticket, token, Some(&executable), &custom_args)
            }
            LaunchMode::Game => process::launch_game_with_args(
                &ticket,
                place_id,
                job_id.as_deref(),
                link_code.as_deref(),
                access_code.as_deref(),
                data.as_deref(),
                token,
                Some(&executable),
                &custom_args,
            ),
        }
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
    pacing.schedule(launch_spacing(LaunchSpacing {
        configured_delay: config.launch_delay_secs,
        needs_tray_cleanup,
    }));
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
    instances::schedule_arrangement(
        &state,
        Duration::from_secs(u64::from(config.launch_delay_secs).saturating_add(2).max(5)),
        config.auto_arrange_windows,
    );
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
    fn home_mode_needs_an_account_but_rejects_game_destinations() {
        let mut request = LaunchRequest {
            user_id: 1,
            place_id: 0,
            job_id: None,
            data: None,
            link_code: None,
            access_code: None,
            overrides: LaunchOverrides::default(),
        };
        assert!(validate_launch_mode(&request, LaunchMode::Home).is_ok());
        assert!(validate_launch_mode(&request, LaunchMode::Game).is_err());
        request.user_id = 0;
        assert!(validate_launch_mode(&request, LaunchMode::Home).is_err());
        request.user_id = 1;
        request.place_id = 2;
        assert!(validate_launch_mode(&request, LaunchMode::Home).is_err());
        assert!(validate_launch_mode(&request, LaunchMode::Game).is_ok());
        request.place_id = 0;
        request.job_id = Some("synthetic-job".into());
        assert!(validate_launch_mode(&request, LaunchMode::Home).is_err());
        request.job_id = None;
        request.data = Some("synthetic-data".into());
        assert!(validate_launch_mode(&request, LaunchMode::Home).is_err());
        request.data = None;
        request.link_code = Some("synthetic-link".into());
        assert!(validate_launch_mode(&request, LaunchMode::Home).is_err());
        request.link_code = None;
        request.access_code = Some("synthetic-access".into());
        assert!(validate_launch_mode(&request, LaunchMode::Home).is_err());
    }

    #[test]
    fn explicit_new_instance_requests_cannot_reuse_a_client_after_policy_changes() {
        assert!(validate_instance_policy(true, false).is_err());
        assert!(validate_instance_policy(true, true).is_ok());
        assert!(validate_instance_policy(false, false).is_ok());
    }

    #[test]
    fn launch_failures_have_safe_statuses_and_reason_codes() {
        assert_eq!(
            launch_failure(
                LaunchFailureStage::Preflight,
                "This account is restricted or its credential has expired."
            ),
            ("blocked", "account_restricted_or_expired")
        );
        assert_eq!(
            launch_failure(
                LaunchFailureStage::Authentication,
                "Roblox rejected the launch request."
            ),
            ("failed", "roblox_auth_rejected")
        );
        assert_eq!(
            launch_failure(
                LaunchFailureStage::PrivacyPreparation,
                "Privacy cleanup failed. Roblox was not launched."
            ),
            ("blocked", "privacy_cleanup_failed")
        );
        assert_eq!(
            launch_failure(
                LaunchFailureStage::Preflight,
                "Automatic MAC rotation failed or was cancelled."
            ),
            ("blocked", "mac_rotation_failed")
        );
        assert_eq!(
            launch_failure(LaunchFailureStage::RateLimited, "safe rate limit message"),
            ("failed", "roblox_rate_limited")
        );
        assert_eq!(
            launch_failure(LaunchFailureStage::Transport, "safe transport message"),
            ("failed", "roblox_connection_failed")
        );
    }

    #[test]
    fn auth_ticket_errors_keep_safe_reason_categories() {
        let mut stage = LaunchFailureStage::Authentication;
        let rate_limit = auth_ticket_failure(ram_core::CoreError::RateLimited, &mut stage);
        assert_eq!(stage, LaunchFailureStage::RateLimited);
        assert_eq!(
            launch_failure(stage, &rate_limit),
            ("failed", "roblox_rate_limited")
        );

        let mut stage = LaunchFailureStage::Authentication;
        let rejected = auth_ticket_failure(
            ram_core::CoreError::CookieRejectedWithReason("synthetic-secret".into()),
            &mut stage,
        );
        assert_eq!(stage, LaunchFailureStage::Authentication);
        assert!(!rejected.contains("synthetic-secret"));
        assert_eq!(
            launch_failure(stage, &rejected),
            ("failed", "roblox_auth_rejected")
        );

        let mut stage = LaunchFailureStage::Authentication;
        let api_error = auth_ticket_failure(
            ram_core::CoreError::RobloxApi {
                status: 503,
                message: "synthetic-secret".into(),
            },
            &mut stage,
        );
        assert!(!api_error.contains("synthetic-secret"));
        assert_eq!(
            launch_failure(stage, &api_error),
            ("failed", "roblox_request_failed")
        );

        let transport = reqwest::Client::new().get("not-a-url").build().unwrap_err();
        let mut stage = LaunchFailureStage::Authentication;
        let transport_error = auth_ticket_failure(ram_core::CoreError::Http(transport), &mut stage);
        assert_eq!(stage, LaunchFailureStage::Transport);
        assert_eq!(
            launch_failure(stage, &transport_error),
            ("failed", "roblox_connection_failed")
        );
    }

    #[test]
    fn launch_progress_serializes_friend_target_context() {
        let payload = serde_json::to_value(LaunchProgress {
            request_id: "synthetic-request".into(),
            user_id: 1,
            friend_target_id: Some(2),
            phase: "failed",
            reason_code: Some("roblox_rate_limited"),
        })
        .unwrap();
        assert_eq!(payload["userId"], 1);
        assert_eq!(payload["friendTargetId"], 2);
        assert_eq!(payload["reasonCode"], "roblox_rate_limited");
    }

    #[test]
    fn preset_overrides_change_only_the_selected_global_launch_options() {
        let mut config = ram_core::models::AppConfig {
            privacy_mode: true,
            multi_instance_enabled: false,
            auto_arrange_windows: false,
            ..ram_core::models::AppConfig::default()
        };
        apply_launch_overrides(
            &mut config,
            LaunchOverrides {
                privacy_mode: Some(false),
                multi_instance_enabled: Some(true),
                auto_arrange_windows: None,
            },
        );
        assert!(!config.privacy_mode);
        assert!(config.multi_instance_enabled);
        assert!(!config.auto_arrange_windows);
        apply_launch_overrides(&mut config, LaunchOverrides::default());
        assert!(!config.privacy_mode);
        assert!(config.multi_instance_enabled);
    }

    #[test]
    fn batch_outcome_reports_partial_and_complete_launches() {
        assert_eq!(batch_outcome(3, 3), Ok(()));
        assert_eq!(
            batch_outcome(3, 1),
            Err("1 of 3 accounts launched; 2 failed or were blocked. See the RM log for per-account reasons.".into())
        );
        assert_eq!(
            batch_outcome(2, 0),
            Err("0 of 2 accounts launched; 2 failed or were blocked. See the RM log for per-account reasons.".into())
        );
    }

    #[test]
    fn batch_job_ids_are_included_only_for_a_shared_explicit_destination() {
        let request = |job: Option<&str>| LaunchRequest {
            user_id: 1,
            place_id: 2,
            job_id: job.map(str::to_owned),
            data: None,
            link_code: None,
            access_code: None,
            overrides: LaunchOverrides::default(),
        };
        let job = "12345678-1234-4234-8234-123456789abc";
        assert_eq!(
            common_job_id(&[request(Some(job)), request(Some(job))]).as_deref(),
            Some(job)
        );
        assert!(common_job_id(&[]).is_none());
        assert!(common_job_id(&[request(Some(job)), request(None)]).is_none());
        assert!(common_job_id(&[
            request(Some(job)),
            request(Some("87654321-1234-4234-8234-123456789abc"))
        ])
        .is_none());
        let mut private = request(None);
        private.access_code = Some(job.into());
        private.link_code = Some("synthetic-invitation".into());
        assert!(common_job_id(&[private]).is_none());
    }

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
            overrides: LaunchOverrides::default(),
        };
        assert!(validate_request(&request).is_err());
    }
}
