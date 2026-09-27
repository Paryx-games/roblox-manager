use crate::state::AppState;
use ram_core::{
    instances::{Attribution, InstanceRegistry, LiveClient, TrackedInstance},
    process,
};
use serde::Serialize;
use std::{
    collections::HashMap,
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
    arrange_after: Option<Instant>,
    snapshot: InstanceWorkspace,
}

#[derive(Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceWorkspace {
    instances: Vec<InstanceSummary>,
    running_count: usize,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    pid: u32,
    start_time: u64,
    user_id: Option<u64>,
    label: String,
    place_id: Option<u64>,
    attribution: &'static str,
    launched_at: Option<String>,
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

pub fn schedule_arrangement(state: &AppState, delay: Duration) {
    if let Ok(mut instances) = state.instances.lock() {
        instances.arrange_after = Some(Instant::now() + delay);
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
    let live = instances.tokens.scan();
    let outcome = instances.registry.sweep(&live, chrono::Utc::now());
    if !outcome.abandoned.is_empty() {
        let _ = app.emit(
            "background-notice",
            "A Roblox launch has not produced a client yet. Check Roblox before launching again.",
        );
    }
    let tracked = instances.registry.snapshot();
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
    if instances
        .arrange_after
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        if !config.auto_arrange_windows || live.len() >= 2 {
            instances.arrange_after = None;
            if config.auto_arrange_windows {
                process::arrange_roblox_windows(&config.tiling_options());
            }
        } else if instances
            .last_launch
            .is_none_or(|last| last.elapsed() >= Duration::from_secs(60))
        {
            instances.arrange_after = None;
        }
    }
    let snapshot = InstanceWorkspace {
        running_count: live.len(),
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
                    place_id: instance.map(|instance| instance.place_id),
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
    fn inferred_matches_cannot_be_killed() {
        assert!(exact_instance(&[instance(Attribution::Inferred)], 123, 5).is_err());
        assert!(exact_instance(&[instance(Attribution::Exact)], 123, 5).is_ok());
    }

    #[test]
    fn recycled_process_ids_cannot_use_old_attribution() {
        assert!(exact_instance(&[instance(Attribution::Exact)], 123, 6).is_err());
    }
}
