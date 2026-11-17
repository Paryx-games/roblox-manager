use crate::state::AppState;
use ram_core::{
    instances::{Attribution, InstanceRegistry, LiveClient, TrackedInstance},
    models::{AppConfig, CpuLimitAction, MemoryLimitAction},
    process,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct InstanceState {
    registry: InstanceRegistry,
    tokens: process::LaunchTokenCache,
    original_titles: HashMap<(u32, u64), String>,
    applied_titles: HashMap<(u32, u64), String>,
    last_launch: Option<Instant>,
    arrange_after: Option<(Instant, bool)>,
    snapshot: InstanceWorkspace,
    presence_checks: HashMap<u64, (ram_core::models::Presence, chrono::DateTime<chrono::Utc>)>,
    resource_triggers: HashSet<(u32, u64, ResourceKind)>,
    invalid_threshold_notices: HashSet<ResourceKind>,
    #[cfg(windows)]
    resource_monitor: process::RobloxResourceMonitor,
    #[cfg(windows)]
    cpu_throttle_jobs: HashMap<(u32, u64), process::CpuThrottleJob>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ResourceKind {
    Cpu,
    Memory,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMonitorSettings {
    cpu_limit_percent: Option<u32>,
    memory_limit_mb: Option<u64>,
    cpu_action: CpuLimitAction,
    memory_action: MemoryLimitAction,
    system_memory_limit_mb: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMonitorSettingsUpdate {
    cpu_limit_percent: Option<u32>,
    memory_limit_mb: Option<u64>,
    cpu_action: CpuLimitAction,
    memory_action: MemoryLimitAction,
}

impl ResourceMonitorSettings {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            cpu_limit_percent: config.roblox_cpu_limit_percent,
            memory_limit_mb: config.roblox_memory_limit_mb,
            cpu_action: config.roblox_cpu_limit_action,
            memory_action: config.roblox_memory_limit_action,
            system_memory_limit_mb: process::system_memory_limit_mb(),
        }
    }
}

fn validate_resource_monitor_settings(
    settings: &ResourceMonitorSettingsUpdate,
    system_memory_limit_mb: Option<u64>,
    existing_memory_limit_mb: Option<u64>,
) -> Result<(), &'static str> {
    if settings
        .cpu_limit_percent
        .is_some_and(|percent| !(1..=99).contains(&percent))
    {
        return Err("The CPU threshold must be between 1 and 99 percent");
    }
    if let Some(megabytes) = settings.memory_limit_mb {
        if Some(megabytes) == existing_memory_limit_mb {
            if megabytes < 1024 {
                return Err("The RAM threshold must be at least 1024 MB");
            }
        } else {
            let maximum = system_memory_limit_mb
                .ok_or("The installed system memory could not be determined")?;
            if !(1024..=maximum).contains(&megabytes) {
                return Err(
                    "The RAM threshold must be between 1024 MB and installed physical memory",
                );
            }
        }
    }
    Ok(())
}

#[derive(Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceWorkspace {
    instances: Vec<InstanceSummary>,
    running_count: usize,
    starting_count: usize,
}

fn instance_is_starting(
    instance: &TrackedInstance,
    presence: Option<&(ram_core::models::Presence, chrono::DateTime<chrono::Utc>)>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    if now
        .signed_duration_since(instance.launched_at)
        .num_seconds()
        >= 90
    {
        return false;
    }
    let Some((presence, checked_at)) = presence else {
        return true;
    };
    if *checked_at < instance.launched_at {
        return true;
    }
    if instance.place_id == 0 {
        !matches!(presence.user_presence_type, 1 | 2)
    } else {
        presence.user_presence_type != 2 || presence.place_id != Some(instance.place_id)
    }
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    pid: u32,
    start_time: u64,
    user_id: Option<u64>,
    label: String,
    place_id: Option<u64>,
    job_id: Option<String>,
    attribution: &'static str,
    launched_at: Option<String>,
}

fn instance_job_id(
    instance: &TrackedInstance,
    tracked: &[TrackedInstance],
    presence: Option<&ram_core::models::Presence>,
    checked_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    if !instance.attribution.is_exact()
        || tracked
            .iter()
            .filter(|other| other.user_id == instance.user_id)
            .count()
            != 1
    {
        return None;
    }
    let checked_at = checked_at?;
    if checked_at < instance.launched_at || now.signed_duration_since(checked_at).num_seconds() > 30
    {
        return None;
    }
    let presence = presence?;
    if presence.user_presence_type != 2 || presence.place_id != Some(instance.place_id) {
        return None;
    }
    ram_core::instances::normalize_job_id(presence.game_id.as_deref())
}

#[cfg(windows)]
fn notify_resource_threshold(
    app: &tauri::AppHandle,
    pid: u32,
    resource: crate::discord::ResourceLimit,
    usage: u64,
    threshold: u64,
) {
    use tauri_plugin_notification::NotificationExt;

    let (label, units) = match resource {
        crate::discord::ResourceLimit::Cpu => ("CPU", "%"),
        crate::discord::ResourceLimit::Ram => ("RAM", " MB"),
    };
    let body = format!(
        "{label} use for Roblox client PID {pid} reached {usage}{units} (threshold {threshold}{units})."
    );
    if app
        .notification()
        .builder()
        .title("Roblox resource threshold reached")
        .body(&body)
        .show()
        .is_err()
    {
        let _ = app.emit(
            "background-notice",
            "A Roblox resource threshold was reached, but Windows could not show its notification.",
        );
    }
    crate::discord::resource_limit_exceeded(app, pid, resource, usage, threshold);
}

#[cfg(windows)]
fn enforce_resource_limits(
    app: &tauri::AppHandle,
    instances: &mut InstanceState,
    live: &[LiveClient],
    tracked: &[TrackedInstance],
    config: &AppConfig,
) {
    let live_keys = live
        .iter()
        .map(|client| (client.pid, client.start_time))
        .collect::<HashSet<_>>();
    instances
        .resource_triggers
        .retain(|(pid, start_time, _)| live_keys.contains(&(*pid, *start_time)));
    instances
        .cpu_throttle_jobs
        .retain(|key, _| live_keys.contains(key));

    let cpu_threshold = config
        .roblox_cpu_limit_percent
        .filter(|value| (1..=99).contains(value));
    let memory_threshold = config.roblox_memory_limit_mb.filter(|value| *value >= 1024);
    for (kind, configured, valid) in [
        (
            ResourceKind::Cpu,
            config.roblox_cpu_limit_percent.is_some(),
            cpu_threshold.is_some(),
        ),
        (
            ResourceKind::Memory,
            config.roblox_memory_limit_mb.is_some(),
            memory_threshold.is_some(),
        ),
    ] {
        if configured && !valid && instances.invalid_threshold_notices.insert(kind) {
            let resource = if kind == ResourceKind::Cpu {
                "CPU"
            } else {
                "RAM"
            };
            let _ = app.emit(
                "background-notice",
                format!(
                    "The saved {resource} threshold is invalid. Review Resource monitoring in Instances; no automatic action was taken."
                ),
            );
        } else if !configured || valid {
            instances.invalid_threshold_notices.remove(&kind);
        }
    }

    if cpu_threshold.is_none() && memory_threshold.is_none() {
        instances.resource_monitor.sample(&[]);
        return;
    }
    let targets = tracked
        .iter()
        .filter(|instance| {
            instance.attribution.is_exact()
                && live_keys.contains(&(instance.pid, instance.start_time))
        })
        .collect::<Vec<_>>();
    let pids = targets
        .iter()
        .map(|instance| instance.pid)
        .collect::<Vec<_>>();
    let usage_by_pid = instances.resource_monitor.sample(&pids);

    for instance in targets {
        let Some(usage) = usage_by_pid.get(&instance.pid) else {
            continue;
        };
        let identity = (instance.pid, instance.start_time);
        if let Some(threshold) = cpu_threshold {
            let key = (identity.0, identity.1, ResourceKind::Cpu);
            if usage.cpu_percent >= threshold && instances.resource_triggers.insert(key) {
                match config.roblox_cpu_limit_action {
                    CpuLimitAction::Warn => notify_resource_threshold(
                        app,
                        instance.pid,
                        crate::discord::ResourceLimit::Cpu,
                        u64::from(usage.cpu_percent),
                        u64::from(threshold),
                    ),
                    CpuLimitAction::Throttle => {
                        notify_resource_threshold(
                            app,
                            instance.pid,
                            crate::discord::ResourceLimit::Cpu,
                            u64::from(usage.cpu_percent),
                            u64::from(threshold),
                        );
                        let throttle_percent = (threshold / 2).max(1);
                        match process::throttle_verified_instance(
                            instance.pid,
                            instance.launchtime,
                            throttle_percent,
                        ) {
                            Ok(job) => {
                                instances.cpu_throttle_jobs.insert(identity, job);
                            }
                            Err(_) => {
                                let _ = app.emit(
                                    "background-notice",
                                    format!(
                                        "CPU throttling could not be applied to Roblox client PID {}. It was left running.",
                                        instance.pid
                                    ),
                                );
                            }
                        }
                    }
                    CpuLimitAction::Kill => {
                        match process::kill_verified_instance(instance.pid, instance.launchtime) {
                            Ok(()) => break,
                            Err(_) => {
                                let _ = app.emit(
                                    "background-notice",
                                    format!(
                                        "Roblox client PID {} exceeded its CPU threshold but could not be safely closed.",
                                        instance.pid
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }

        if let Some(threshold) = memory_threshold {
            let key = (identity.0, identity.1, ResourceKind::Memory);
            if usage.memory_mb >= threshold && instances.resource_triggers.insert(key) {
                match config.roblox_memory_limit_action {
                    MemoryLimitAction::Warn => notify_resource_threshold(
                        app,
                        instance.pid,
                        crate::discord::ResourceLimit::Ram,
                        usage.memory_mb,
                        threshold,
                    ),
                    MemoryLimitAction::Kill => {
                        match process::kill_verified_instance(instance.pid, instance.launchtime) {
                            Ok(()) => break,
                            Err(_) => {
                                let _ = app.emit(
                                    "background-notice",
                                    format!(
                                        "Roblox client PID {} exceeded its RAM threshold but could not be safely closed.",
                                        instance.pid
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn record_presence_check(
    state: &AppState,
    presences: &[(u64, ram_core::models::Presence)],
    checked_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let mut instances = state
        .instances
        .lock()
        .map_err(|_| "Instance presence tracking unavailable")?;
    for (id, presence) in presences {
        instances
            .presence_checks
            .insert(*id, (presence.clone(), checked_at));
    }
    Ok(())
}

pub fn require_no_account_session(state: &AppState, user_id: u64) -> Result<(), String> {
    let instances = state
        .instances
        .lock()
        .map_err(|_| "Instance tracking unavailable")?;
    if instances.registry.has_session(user_id) {
        return Err("An instance for this account is running or still starting. Confirm possible session replacement before joining.".into());
    }
    Ok(())
}

pub fn account_server(state: &AppState, user_id: u64) -> Option<(u64, String)> {
    let instances = state.instances.lock().ok()?;
    let mut matches = instances
        .snapshot
        .instances
        .iter()
        .filter(|instance| instance.user_id == Some(user_id));
    let instance = matches.next()?;
    if matches.next().is_some() || instance.attribution != "exact" {
        return None;
    }
    Some((instance.place_id?, instance.job_id.clone()?))
}

fn exact_instance(
    instances: &[TrackedInstance],
    pid: u32,
    start_time: u64,
) -> Result<TrackedInstance, String> {
    let instance = instances
        .iter()
        .find(|instance| instance.pid == pid && instance.start_time == start_time)
        .ok_or("This instance is no longer tracked. Refresh the list.")?;
    if !instance.attribution.is_exact() {
        return Err(
            "This account match is inferred. RM cannot safely kill this instance individually."
                .into(),
        );
    }
    Ok(instance.clone())
}

pub fn note_launch(state: &AppState, user_id: u64, place_id: u64) -> Result<i64, String> {
    let mut instances = state
        .instances
        .lock()
        .map_err(|_| "Instance tracking unavailable")?;
    if state
        .is_shutting_down
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err("RM is closing. This launch was cancelled.".into());
    }
    let live = instances.tokens.scan();
    let token = process::next_launchtime();
    instances
        .registry
        .note_launch(user_id, place_id, token, &live, chrono::Utc::now());
    instances.last_launch = Some(Instant::now());
    Ok(token)
}

pub fn schedule_arrangement(state: &AppState, delay: Duration, enabled: bool) {
    if let Ok(mut instances) = state.instances.lock() {
        instances.arrange_after = Some((Instant::now() + delay, enabled));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArrangementAction {
    Wait,
    Arrange,
    Cancel,
}

fn arrangement_action(
    enabled: bool,
    live_count: usize,
    launch_is_stale: bool,
) -> ArrangementAction {
    if !enabled {
        ArrangementAction::Cancel
    } else if live_count >= 2 {
        ArrangementAction::Arrange
    } else if launch_is_stale {
        ArrangementAction::Cancel
    } else {
        ArrangementAction::Wait
    }
}

pub fn sweep_interval(state: &AppState) -> Duration {
    if state
        .instances
        .lock()
        .ok()
        .and_then(|instances| instances.last_launch)
        .is_some_and(|last| last.elapsed() < Duration::from_secs(30))
    {
        Duration::from_millis(400)
    } else {
        Duration::from_secs(2)
    }
}

fn restore_titles(instances: &mut InstanceState, live: &[LiveClient]) {
    instances.applied_titles.clear();
    let titles = instances
        .original_titles
        .drain()
        .filter(|((pid, start_time), _)| {
            live.iter()
                .any(|client| client.pid == *pid && client.start_time == *start_time)
        })
        .map(|((pid, _), title)| (pid, title))
        .collect::<Vec<_>>();
    process::restore_instance_titles(&titles);
}

fn sweep(app: &tauri::AppHandle) -> Result<InstanceWorkspace, String> {
    let state = app.state::<AppState>();
    let (config, labels) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        (
            runtime.config.clone(),
            runtime
                .accounts
                .accounts
                .iter()
                .map(|account| {
                    (
                        account.user_id,
                        if runtime.config.anonymize_names {
                            format!("Account {}", account.user_id)
                        } else {
                            account.label().to_string()
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
        )
    };
    let mut instances = state
        .instances
        .lock()
        .map_err(|_| "Instance tracking unavailable")?;
    if state
        .is_shutting_down
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Ok(instances.snapshot.clone());
    }
    instances
        .presence_checks
        .retain(|id, _| labels.contains_key(id));
    let live = instances.tokens.scan();
    let outcome = instances.registry.sweep(&live, chrono::Utc::now());
    for instance in &outcome.exited {
        if instance.attribution.is_exact() {
            let job_id = instances
                .snapshot
                .instances
                .iter()
                .find(|previous| {
                    previous.pid == instance.pid && previous.start_time == instance.start_time
                })
                .and_then(|previous| previous.job_id.as_deref());
            crate::discord::client_exited(app, instance.user_id, instance.place_id, job_id);
        }
    }
    if !outcome.abandoned.is_empty() {
        let _ = app.emit(
            "background-notice",
            "A Roblox launch has not produced a client yet. Check Roblox before launching again.",
        );
    }
    let tracked = instances.registry.snapshot();
    #[cfg(windows)]
    enforce_resource_limits(app, &mut instances, &live, &tracked, &config);
    instances.original_titles.retain(|(pid, start_time), _| {
        live.iter()
            .any(|client| client.pid == *pid && client.start_time == *start_time)
    });
    instances.applied_titles.retain(|(pid, start_time), _| {
        live.iter()
            .any(|client| client.pid == *pid && client.start_time == *start_time)
    });
    if config.rename_roblox_windows {
        let titles = tracked
            .iter()
            .filter(|instance| instance.attribution.is_exact())
            .map(|instance| {
                (
                    instance.pid,
                    format!(
                        "{} | Roblox",
                        labels
                            .get(&instance.user_id)
                            .cloned()
                            .unwrap_or_else(|| "Removed account".into())
                    ),
                )
            })
            .collect::<Vec<_>>();
        for (pid, original) in process::apply_instance_titles(&titles) {
            if let Some(client) = live.iter().find(|client| client.pid == pid) {
                let identity = (pid, client.start_time);
                if instances.applied_titles.get(&identity) != Some(&original) {
                    instances.original_titles.insert(identity, original);
                }
                if let Some((_, title)) = titles.iter().find(|(target, _)| *target == pid) {
                    instances.applied_titles.insert(identity, title.clone());
                }
            }
        }
    } else {
        restore_titles(&mut instances, &live);
    }
    if let Some((deadline, should_arrange)) = instances.arrange_after {
        if Instant::now() >= deadline {
            match arrangement_action(
                should_arrange,
                live.len(),
                instances
                    .last_launch
                    .is_none_or(|last| last.elapsed() >= Duration::from_secs(60)),
            ) {
                ArrangementAction::Arrange => {
                    instances.arrange_after = None;
                    process::arrange_roblox_windows(&config.tiling_options());
                }
                ArrangementAction::Cancel => instances.arrange_after = None,
                ArrangementAction::Wait => {}
            }
        }
    }
    let snapshot = InstanceWorkspace {
        running_count: live.len(),
        starting_count: instances.registry.pending_count()
            + tracked
                .iter()
                .filter(|instance| {
                    instance_is_starting(
                        instance,
                        instances.presence_checks.get(&instance.user_id),
                        chrono::Utc::now(),
                    )
                })
                .count(),
        instances: live
            .iter()
            .map(|client| {
                let instance = tracked.iter().find(|instance| {
                    instance.pid == client.pid && instance.start_time == client.start_time
                });
                InstanceSummary {
                    pid: client.pid,
                    start_time: client.start_time,
                    user_id: instance.map(|instance| instance.user_id),
                    label: instance
                        .and_then(|instance| labels.get(&instance.user_id))
                        .cloned()
                        .unwrap_or_else(|| "Unmatched client".into()),
                    place_id: instance
                        .map(|instance| instance.place_id)
                        .filter(|id| *id > 0),
                    job_id: instance.and_then(|instance| {
                        instances.presence_checks.get(&instance.user_id).and_then(
                            |(presence, checked_at)| {
                                instance_job_id(
                                    instance,
                                    &tracked,
                                    Some(presence),
                                    Some(*checked_at),
                                    chrono::Utc::now(),
                                )
                            },
                        )
                    }),
                    attribution: match instance.map(|instance| instance.attribution) {
                        Some(Attribution::Exact) => "exact",
                        Some(Attribution::Inferred) => "inferred",
                        None => "unmatched",
                    },
                    launched_at: instance.map(|instance| instance.launched_at.to_rfc3339()),
                }
            })
            .collect(),
    };
    if instances.snapshot != snapshot {
        instances.snapshot = snapshot.clone();
        let _ = app.emit("instances-updated", &snapshot);
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn list_instances(app: tauri::AppHandle) -> Result<InstanceWorkspace, String> {
    tauri::async_runtime::spawn_blocking(move || sweep(&app))
        .await
        .map_err(|_| "Instance refresh task failed")?
}

#[tauri::command]
pub async fn get_resource_monitor_settings(
    state: tauri::State<'_, AppState>,
) -> Result<ResourceMonitorSettings, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable")?;
        Ok(ResourceMonitorSettings::from_config(&runtime.config))
    })
    .await
    .map_err(|_| "Resource monitor settings could not be loaded")?
}

#[tauri::command]
pub async fn save_resource_monitor_settings(
    state: tauri::State<'_, AppState>,
    settings: ResourceMonitorSettingsUpdate,
) -> Result<ResourceMonitorSettings, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let memory_limit = process::system_memory_limit_mb();
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable")?;
        validate_resource_monitor_settings(
            &settings,
            memory_limit,
            runtime.config.roblox_memory_limit_mb,
        )
        .map_err(str::to_string)?;
        let mut candidate = runtime.config.clone();
        candidate.roblox_cpu_limit_percent = settings.cpu_limit_percent;
        candidate.roblox_memory_limit_mb = settings.memory_limit_mb;
        candidate.roblox_cpu_limit_action = settings.cpu_action;
        candidate.roblox_memory_limit_action = settings.memory_action;
        candidate
            .save(&runtime.config_path)
            .map_err(|_| "Resource monitor settings could not be saved")?;
        runtime.config = candidate;
        Ok(ResourceMonitorSettings::from_config(&runtime.config))
    })
    .await
    .map_err(|_| "Resource monitor settings save task failed")?
}

#[tauri::command]
pub async fn focus_instance(
    state: tauri::State<'_, AppState>,
    pid: u32,
    start_time: u64,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut instances = state
            .instances
            .lock()
            .map_err(|_| "Instance tracking unavailable")?;
        let live = instances.tokens.scan();
        if !live
            .iter()
            .any(|client| client.pid == pid && client.start_time == start_time)
        {
            return Err("This instance has exited. Refresh the list.".into());
        }
        if process::focus_instance(pid) {
            Ok(())
        } else {
            Err("The Roblox window is not available yet.".into())
        }
    })
    .await
    .map_err(|_| "Instance focus task failed")?
}

#[tauri::command]
pub async fn kill_instance(app: tauri::AppHandle, pid: u32, start_time: u64) -> Result<(), String> {
    let state = app.state::<AppState>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut instances = state
            .instances
            .lock()
            .map_err(|_| "Instance tracking unavailable")?;
        let live = instances.tokens.scan();
        instances.registry.sweep(&live, chrono::Utc::now());
        let instance = exact_instance(&instances.registry.snapshot(), pid, start_time)?;
        process::kill_verified_instance(pid, instance.launchtime).map_err(|_| {
            "The instance could not be verified or closed. Nothing else was killed.".to_string()
        })
    })
    .await
    .map_err(|_| "Instance close task failed")??;
    list_instances(app).await?;
    Ok(())
}

pub fn shutdown(state: &AppState) {
    state
        .is_shutting_down
        .store(true, std::sync::atomic::Ordering::Release);
    if let Ok(mut instances) = state.instances.lock() {
        let live = instances.tokens.scan();
        restore_titles(&mut instances, &live);
    }
    let privacy = state.runtime.lock().ok().and_then(|runtime| {
        runtime
            .config
            .privacy_clean_on_exit
            .then(|| runtime.config.privacy_cleanup_options())
    });
    if let Some(privacy) = privacy {
        if process::prepare_privacy_cleanup(privacy)
            .and_then(|cleanup| cleanup.commit())
            .is_err()
        {
            tracing::warn!("Privacy cleanup on exit did not complete");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_threshold_validation_accepts_bounds_and_rejects_invalid_values() {
        let valid = ResourceMonitorSettingsUpdate {
            cpu_limit_percent: Some(1),
            memory_limit_mb: Some(8192),
            cpu_action: CpuLimitAction::Throttle,
            memory_action: MemoryLimitAction::Warn,
        };
        assert!(validate_resource_monitor_settings(&valid, Some(8192), None).is_ok());

        for invalid in [
            ResourceMonitorSettingsUpdate {
                cpu_limit_percent: Some(0),
                ..valid_update()
            },
            ResourceMonitorSettingsUpdate {
                cpu_limit_percent: Some(100),
                ..valid_update()
            },
            ResourceMonitorSettingsUpdate {
                memory_limit_mb: Some(1023),
                ..valid_update()
            },
            ResourceMonitorSettingsUpdate {
                memory_limit_mb: Some(8193),
                ..valid_update()
            },
        ] {
            assert!(validate_resource_monitor_settings(&invalid, Some(8192), None).is_err());
        }
        assert!(validate_resource_monitor_settings(&valid_update(), None, None).is_ok());
        assert!(validate_resource_monitor_settings(
            &ResourceMonitorSettingsUpdate {
                memory_limit_mb: Some(1024),
                ..valid_update()
            },
            None,
            None,
        )
        .is_err());
        let unchanged_ram = ResourceMonitorSettingsUpdate {
            memory_limit_mb: Some(8192),
            ..valid_update()
        };
        assert!(validate_resource_monitor_settings(&unchanged_ram, None, Some(8192)).is_ok());
        let changed_ram = ResourceMonitorSettingsUpdate {
            memory_limit_mb: Some(4096),
            ..valid_update()
        };
        assert!(validate_resource_monitor_settings(&changed_ram, None, Some(8192)).is_err());
    }

    fn valid_update() -> ResourceMonitorSettingsUpdate {
        ResourceMonitorSettingsUpdate {
            cpu_limit_percent: None,
            memory_limit_mb: None,
            cpu_action: CpuLimitAction::Warn,
            memory_action: MemoryLimitAction::Warn,
        }
    }

    #[test]
    fn scheduled_arrangement_uses_effective_launch_setting() {
        assert_eq!(
            arrangement_action(true, 2, false),
            ArrangementAction::Arrange
        );
        assert_eq!(
            arrangement_action(true, 2, true),
            ArrangementAction::Arrange
        );
        assert_eq!(
            arrangement_action(false, 2, false),
            ArrangementAction::Cancel
        );
        assert_eq!(arrangement_action(true, 1, false), ArrangementAction::Wait);
        assert_eq!(arrangement_action(true, 1, true), ArrangementAction::Cancel);
    }

    #[test]
    fn loading_waits_for_post_launch_presence_and_expires_without_confirmation() {
        let mut client = instance(Attribution::Exact);
        let now = client.launched_at + chrono::Duration::seconds(10);
        assert!(instance_is_starting(&client, None, now));
        let mut presence = (
            ram_core::models::Presence {
                user_presence_type: 2,
                place_id: Some(client.place_id),
                ..Default::default()
            },
            client.launched_at - chrono::Duration::seconds(1),
        );
        assert!(instance_is_starting(&client, Some(&presence), now));
        presence.1 = now;
        assert!(!instance_is_starting(&client, Some(&presence), now));
        presence.0.place_id = Some(999);
        assert!(instance_is_starting(&client, Some(&presence), now));
        assert!(!instance_is_starting(
            &client,
            None,
            client.launched_at + chrono::Duration::seconds(90)
        ));
        client.place_id = 0;
        presence.0.user_presence_type = 0;
        assert!(instance_is_starting(&client, Some(&presence), now));
        presence.0.user_presence_type = 1;
        assert!(!instance_is_starting(&client, Some(&presence), now));
        presence.0.user_presence_type = 2;
        assert!(!instance_is_starting(&client, Some(&presence), now));
        presence.0.user_presence_type = 99;
        assert!(instance_is_starting(&client, Some(&presence), now));
    }

    fn instance(attribution: Attribution) -> TrackedInstance {
        TrackedInstance {
            pid: 123,
            start_time: 5,
            user_id: 1,
            place_id: 2,
            launchtime: 3,
            launched_at: chrono::Utc::now(),
            attribution,
        }
    }

    #[test]
    fn job_ids_require_fresh_unambiguous_exact_presence_for_the_same_place() {
        let exact = instance(Attribution::Exact);
        let job = "12345678-1234-4234-8234-123456789abc";
        let mut presence = ram_core::models::Presence {
            user_presence_type: 2,
            place_id: Some(2),
            game_id: Some(job.into()),
            ..Default::default()
        };
        let now = exact.launched_at + chrono::Duration::seconds(10);
        let checked = Some(exact.launched_at + chrono::Duration::seconds(1));
        let tracked = vec![exact.clone()];
        assert_eq!(
            instance_job_id(&exact, &tracked, Some(&presence), checked, now).as_deref(),
            Some(job)
        );
        assert!(instance_job_id(&exact, &tracked, Some(&presence), None, now).is_none());
        assert!(instance_job_id(
            &exact,
            &tracked,
            Some(&presence),
            Some(exact.launched_at - chrono::Duration::seconds(1)),
            now
        )
        .is_none());
        assert!(instance_job_id(
            &exact,
            &tracked,
            Some(&presence),
            checked,
            now + chrono::Duration::seconds(31)
        )
        .is_none());
        let inferred = instance(Attribution::Inferred);
        assert!(instance_job_id(
            &inferred,
            std::slice::from_ref(&inferred),
            Some(&presence),
            checked,
            now
        )
        .is_none());
        assert!(instance_job_id(
            &exact,
            &[exact.clone(), exact.clone()],
            Some(&presence),
            checked,
            now
        )
        .is_none());
        presence.place_id = Some(3);
        assert!(instance_job_id(&exact, &tracked, Some(&presence), checked, now).is_none());
        presence.place_id = Some(2);
        presence.game_id = Some("synthetic-invalid-or-invitation".into());
        assert!(instance_job_id(&exact, &tracked, Some(&presence), checked, now).is_none());
        presence.game_id = Some(job.into());
        presence.user_presence_type = 0;
        assert!(instance_job_id(&exact, &tracked, Some(&presence), checked, now).is_none());
    }

    #[test]
    fn inferred_matches_cannot_be_killed() {
        assert!(exact_instance(&[instance(Attribution::Inferred)], 123, 5).is_err());
        assert!(exact_instance(&[instance(Attribution::Exact)], 123, 5).is_ok());
    }

    #[test]
    fn recycled_process_ids_cannot_use_old_attribution() {
        assert!(exact_instance(&[instance(Attribution::Exact)], 123, 6).is_err());
    }
}
