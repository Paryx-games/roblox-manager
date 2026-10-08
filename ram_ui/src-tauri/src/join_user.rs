use crate::{
    launcher::{self, LaunchRequest},
    state::AppState,
};
use ram_core::{
    api,
    auth::RobloxClient,
    crypto::StoreSession,
    join_user::{self, JoinMode, PendingFollow, Target},
    session_history,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct JoinState {
    pub queue: Arc<tokio::sync::Mutex<()>>,
    operations: Mutex<HashMap<String, Arc<Cancellation>>>,
    pending: Mutex<Option<Vec<PendingFollow>>>,
}

#[derive(Default)]
struct Cancellation {
    flag: AtomicBool,
    notify: tokio::sync::Notify,
}

struct Operation {
    state: Arc<JoinState>,
    id: String,
    cancel: Arc<Cancellation>,
}
impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut operations) = self.state.operations.lock() {
            operations.remove(&self.id);
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinProgress {
    operation_id: String,
    user_id: u64,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinResult {
    user_id: u64,
    requested: bool,
    message: String,
    cleanup_pending: bool,
}

fn progress(app: &tauri::AppHandle, id: &str, user_id: u64, message: &str) {
    let _ = app.emit(
        "join-user-progress",
        JoinProgress {
            operation_id: id.into(),
            user_id,
            message: message.into(),
        },
    );
}

async fn credential(state: &AppState, user_id: u64) -> Result<(String, u64), String> {
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        let account = runtime
            .accounts
            .find_by_id(user_id)
            .ok_or("Account no longer exists")?;
        if !runtime.unlocked || account.cookie_expired || !account.can_launch() {
            return Err("Account is locked, restricted, or its credential has expired".into());
        }
        let cookie = crate::account_cookie(&runtime, user_id)
            .map_err(|_| "Account credential unavailable")?;
        Ok((
            cookie,
            *runtime.credential_revisions.get(&user_id).unwrap_or(&0),
        ))
    })
    .await
    .map_err(|_| "Join credential task failed")?
}

fn current(state: &AppState, user_id: u64, revision: u64) -> bool {
    !state.is_shutting_down.load(Ordering::Acquire)
        && state.runtime.lock().ok().is_some_and(|runtime| {
            runtime.unlocked
                && runtime
                    .accounts
                    .find_by_id(user_id)
                    .is_some_and(|a| !a.cookie_expired && a.can_launch())
                && *runtime.credential_revisions.get(&user_id).unwrap_or(&0) == revision
        })
}

fn journal_context(state: &AppState) -> Result<(std::path::PathBuf, StoreSession), String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?;
    if !runtime.unlocked {
        return Err("Unlock the account store first".into());
    }
    Ok((
        runtime
            .config_path
            .with_file_name("join-follow-cleanup.dat"),
        runtime
            .session
            .clone()
            .ok_or("Unlock the account store first")?,
    ))
}

async fn load_pending(state: &AppState) -> Result<Vec<PendingFollow>, String> {
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(entries)=state.joins.pending.lock().map_err(|_|"Join cleanup unavailable")?.as_ref() { return Ok(entries.clone()); }
        let (path,session)=journal_context(&state)?;
        let entries=join_user::load_pending(&path,&session).map_err(|_|"Saved follow cleanup could not be read. Check account relationships on Roblox before joining.")?;
        *state.joins.pending.lock().map_err(|_|"Join cleanup unavailable")?=Some(entries.clone());
        Ok(entries)
    }).await.map_err(|_|"Follow cleanup load failed")?
}

async fn journal(
    state: &AppState,
    user_id: u64,
    target_user_id: u64,
    add: bool,
) -> Result<(), String> {
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (path, session) = journal_context(&state)?;
        let mut entries = state
            .joins
            .pending
            .lock()
            .map_err(|_| "Join cleanup unavailable")?
            .clone()
            .ok_or("Follow cleanup has not loaded")?;
        entries.retain(|entry| entry.user_id != user_id || entry.target_user_id != target_user_id);
        if add {
            if entries.len() >= 1000 {
                return Err("Resolve pending follow cleanup before joining more accounts".into());
            }
            entries.push(PendingFollow {
                user_id,
                target_user_id,
                recorded_at: chrono::Utc::now(),
            });
        }
        join_user::save_pending(&path, &entries, &session)
            .map_err(|_| "Follow cleanup journal could not be saved")?;
        *state
            .joins
            .pending
            .lock()
            .map_err(|_| "Join cleanup unavailable")? = Some(entries);
        Ok(())
    })
    .await
    .map_err(|_| "Follow cleanup save failed")?
}

fn safe_error(error: &ram_core::CoreError) -> String {
    match error {
        ram_core::CoreError::RateLimited=>"Roblox rate limit was reached after its retry window. The queue stopped; try later.".into(),
        ram_core::CoreError::CookieRejected|ram_core::CoreError::CookieRejectedWithReason(_)|ram_core::CoreError::AuthFailed(_)=>"Roblox rejected this account or requires an interactive challenge.".into(),
        _=>"Roblox could not complete this step. Check the account, connection, and join permissions.".into(),
    }
}

async fn visible(
    client: &RobloxClient,
    cookie: &str,
    target: u64,
) -> Result<Option<(u64, String)>, ram_core::CoreError> {
    Ok(api::fetch_presences(client, cookie, &[target])
        .await?
        .into_iter()
        .find(|(id, _)| *id == target)
        .and_then(|(_, p)| session_history::server(&p)))
}

#[tauri::command]
pub async fn join_user_accounts(
    app: tauri::AppHandle,
    operation_id: String,
    user_ids: Vec<u64>,
    target: String,
    mode: JoinMode,
) -> Result<Vec<JoinResult>, String> {
    let user_ids = join_user::account_ids(user_ids).map_err(str::to_string)?;
    let target = join_user::parse_target(&target).map_err(str::to_string)?;
    uuid::Uuid::parse_str(&operation_id).map_err(|_| "Invalid join operation ID")?;
    let state = app.state::<AppState>().inner().clone();
    let cancel = Arc::new(Cancellation::default());
    {
        let mut operations = state
            .joins
            .operations
            .lock()
            .map_err(|_| "Join queue unavailable")?;
        if operations.contains_key(&operation_id) || operations.len() >= 16 {
            return Err("This join is already queued or the queue is full".into());
        }
        operations.insert(operation_id.clone(), cancel.clone());
    }
    let operation = Operation {
        state: state.joins.clone(),
        id: operation_id.clone(),
        cancel,
    };
    progress(
        &app,
        &operation_id,
        0,
        "Waiting for earlier join operations",
    );
    let _queue = tokio::select! {
        guard = state.joins.queue.lock() => guard,
        _ = operation.cancel.notify.notified() => return Err("Join queue cancelled".into()),
    };
    let cancelled = || {
        operation.cancel.flag.load(Ordering::Acquire)
            || state.is_shutting_down.load(Ordering::Acquire)
    };
    if cancelled() {
        return Err("Join queue cancelled".into());
    }
    let pending = load_pending(&state).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let target = match target {
        Target::UserId(id) => id,
        Target::Username(name) => api::lookup_username(&client, &name)
            .await
            .map_err(|e| safe_error(&e))?
            .map(|(id, _, _)| id)
            .ok_or("No exact Roblox username match")?,
    };
    let mut results = Vec::new();
    let mut shared_server = None;
    let mut terminal = false;
    if mode == JoinMode::VisibleServer {
        for &user_id in &user_ids {
            if cancelled() {
                break;
            }
            progress(
                &app,
                &operation_id,
                user_id,
                "Checking target presence from this account",
            );
            let Ok((cookie, revision)) = credential(&state, user_id).await else {
                continue;
            };
            match visible(&client, &cookie, target).await {
                Ok(server) if current(&state, user_id, revision) => {
                    shared_server = server;
                    if shared_server.is_some() {
                        break;
                    }
                }
                Err(ram_core::CoreError::RateLimited) => {
                    terminal = true;
                    break;
                }
                _ => {}
            }
        }
    }
    for user_id in user_ids {
        let mut row = JoinResult {
            user_id,
            requested: false,
            message: String::new(),
            cleanup_pending: false,
        };
        if cancelled() {
            row.message = "Cancelled before launch".into();
        } else if terminal {
            row.message = "Queue stopped after rate limiting or follow cleanup failure. Resolve it before retrying.".into();
        } else if user_id == target {
            row.message = "An account cannot join itself".into();
        } else if pending
            .iter()
            .any(|p| p.user_id == user_id && p.target_user_id == target)
        {
            row.cleanup_pending = true;
            row.message =
                "Resolve the earlier temporary follow before joining this target again".into();
        } else if mode == JoinMode::VisibleServer {
            if let Some((place, job)) = &shared_server {
                progress(
                    &app,
                    &operation_id,
                    user_id,
                    "Launching into the discovered server",
                );
                match launcher::launch(
                    &app,
                    LaunchRequest {
                        user_id,
                        place_id: *place,
                        job_id: Some(job.clone()),
                        data: None,
                        link_code: None,
                        access_code: None,
                    },
                )
                .await
                {
                    Ok(()) => {
                        row.requested = true;
                        row.message = "Launch requested. Check Instances for progress.".into();
                    }
                    Err(e) => row.message = e,
                }
            } else {
                row.message="No selected account can see a live server for this target. Check join privacy and whether the user is in a game.".into();
            }
        } else {
            match temporary_join(
                &app,
                &state,
                &client,
                &operation_id,
                user_id,
                target,
                &operation.cancel.flag,
            )
            .await
            {
                Ok(result) => {
                    terminal = result.cleanup_pending;
                    row = result;
                }
                Err(error) => {
                    terminal = matches!(error, ram_core::CoreError::RateLimited);
                    row.message = safe_error(&error);
                    row.cleanup_pending = load_pending(&state)
                        .await?
                        .iter()
                        .any(|p| p.user_id == user_id && p.target_user_id == target);
                    if row.cleanup_pending {
                        terminal = true;
                        row.message.push_str(
                            " Temporary follow cleanup needs attention; use Retry cleanup.",
                        );
                    }
                }
            }
        }
        progress(&app, &operation_id, user_id, &row.message);
        results.push(row);
    }
    Ok(results)
}

async fn temporary_join(
    app: &tauri::AppHandle,
    state: &AppState,
    client: &RobloxClient,
    operation_id: &str,
    user_id: u64,
    target: u64,
    cancel: &AtomicBool,
) -> Result<JoinResult, ram_core::CoreError> {
    use join_user::TemporaryJoinActions;
    let (cookie, revision) = credential(state, user_id)
        .await
        .map_err(ram_core::CoreError::Process)?;
    struct Actions<'a> {
        app: &'a tauri::AppHandle,
        state: &'a AppState,
        client: &'a RobloxClient,
        operation_id: &'a str,
        cookie: &'a str,
        user_id: u64,
        target: u64,
        revision: u64,
        cancel: &'a AtomicBool,
    }
    impl TemporaryJoinActions for Actions<'_> {
        fn stopped(&self) -> bool {
            self.cancel.load(Ordering::Acquire) || !current(self.state, self.user_id, self.revision)
        }
        fn progress(&self, message: &'static str) {
            progress(self.app, self.operation_id, self.user_id, message);
        }
        async fn visible(&self) -> Result<Option<(u64, String)>, ram_core::CoreError> {
            visible(self.client, self.cookie, self.target).await
        }
        async fn following(&self) -> Result<bool, ram_core::CoreError> {
            api::is_following(self.client, self.cookie, self.target).await
        }
        async fn journal(&self, add: bool) -> Result<(), ram_core::CoreError> {
            journal(self.state, self.user_id, self.target, add)
                .await
                .map_err(ram_core::CoreError::Process)
        }
        async fn follow(&self) -> Result<(), ram_core::CoreError> {
            api::follow_user(self.client, self.cookie, self.target).await
        }
        async fn unfollow(&self) -> Result<(), ram_core::CoreError> {
            api::unfollow_user(self.client, self.cookie, self.target).await
        }
        async fn launch(&self, server: (u64, String)) -> Result<(), ram_core::CoreError> {
            launcher::launch(
                self.app,
                LaunchRequest {
                    user_id: self.user_id,
                    place_id: server.0,
                    job_id: Some(server.1),
                    data: None,
                    link_code: None,
                    access_code: None,
                },
            )
            .await
            .map_err(ram_core::CoreError::Process)
        }
    }
    let actions = Actions {
        app,
        state,
        client,
        operation_id,
        cookie: &cookie,
        user_id,
        target,
        revision,
        cancel,
    };
    let result = join_user::temporary_join(&actions).await?;
    let mut message = result.message.to_string();
    if result.cleanup_pending {
        message.push_str(" Temporary follow cleanup needs attention; use Retry cleanup.");
    }
    Ok(JoinResult {
        user_id,
        requested: result.requested,
        message,
        cleanup_pending: result.cleanup_pending,
    })
}
#[tauri::command]
pub fn cancel_user_join(
    state: tauri::State<'_, AppState>,
    operation_id: String,
) -> Result<(), String> {
    if let Some(cancel) = state
        .joins
        .operations
        .lock()
        .map_err(|_| "Join queue unavailable")?
        .get(&operation_id)
    {
        cancel.flag.store(true, Ordering::Release);
        cancel.notify.notify_one();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_join_cleanups(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PendingFollow>, String> {
    let _queue = state.joins.queue.lock().await;
    load_pending(&state).await
}

#[tauri::command]
pub async fn retry_join_cleanup(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    target_user_id: u64,
) -> Result<Vec<PendingFollow>, String> {
    let _queue = state.joins.queue.lock().await;
    let pending = load_pending(&state).await?;
    if !pending
        .iter()
        .any(|p| p.user_id == user_id && p.target_user_id == target_user_id)
    {
        return Err("No pending temporary follow for this account and target".into());
    }
    let (cookie, _) = credential(&state, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    api::unfollow_user(&client, &cookie, target_user_id)
        .await
        .map_err(|e| safe_error(&e))?;
    journal(&state, user_id, target_user_id, false).await?;
    load_pending(&state).await
}

pub async fn check_manual_follow(
    state: &AppState,
    user_id: u64,
    target: u64,
) -> Result<bool, String> {
    Ok(load_pending(state)
        .await?
        .iter()
        .any(|entry| entry.user_id == user_id && entry.target_user_id == target))
}

pub async fn finish_manual_unfollow(
    state: &AppState,
    user_id: u64,
    target: u64,
) -> Result<(), String> {
    journal(state, user_id, target, false).await
}

#[tauri::command]
pub async fn reset_join_cleanup_journal(
    state: tauri::State<'_, AppState>,
    confirmation: String,
) -> Result<(), String> {
    if confirmation != "REVIEWED FOLLOWS" {
        return Err(
            "Review the pending relationships on Roblox before resetting this journal".into(),
        );
    }
    let gate = state.joins.queue.clone().lock_owned().await;
    let worker = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _gate = gate;
        let (path, session) = journal_context(&worker)?;
        if join_user::load_pending(&path, &session).is_ok() {
            return Err(
                "This cleanup journal is readable. Use its per-account cleanup actions instead."
                    .into(),
            );
        }
        join_user::save_pending(&path, &[], &session)
            .map_err(|_| "The cleanup journal could not be reset")?;
        *worker
            .joins
            .pending
            .lock()
            .map_err(|_| "Join cleanup unavailable")? = Some(Vec::new());
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "Cleanup journal reset failed")?
}
