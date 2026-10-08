use crate::state::AppState;
use ram_core::session_history::{self, HistoryEvent};
use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySnapshot {
    events: Vec<HistoryEvent>,
    persistent: bool,
    error: Option<String>,
}

fn path(state: &AppState) -> Result<std::path::PathBuf, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?;
    Ok(runtime.config_path.with_file_name("session-history.json"))
}

fn snapshot(state: &AppState) -> Result<HistorySnapshot, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?;
    let history = state.history.lock().map_err(|_| "History unavailable")?;
    Ok(HistorySnapshot {
        events: if runtime.unlocked {
            history.events.clone()
        } else {
            Vec::new()
        },
        persistent: runtime.config.session_history_persist,
        error: history.error.clone(),
    })
}

pub fn publish(app: &tauri::AppHandle) {
    if let Ok(snapshot) = snapshot(&app.state::<AppState>()) {
        let _ = app.emit("session-history-updated", snapshot);
    }
}

pub async fn initialize(state: &AppState) -> Result<(), String> {
    let gate = state.history_io.clone().lock_owned().await;
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _gate = gate;
        if state
            .history
            .lock()
            .map_err(|_| "History unavailable")?
            .loaded
        {
            return Ok(());
        }
        let (persistent, session) = {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            if !runtime.unlocked {
                return Ok(());
            }
            (
                runtime.config.session_history_persist,
                runtime.session.clone(),
            )
        };
        let events = if persistent {
            session_history::load_or_migrate(&path(&state)?, session.as_ref())
            .map_err(|_| "Saved history could not be read. Retry after checking the file, or clear history to start again.")
        } else {
            Ok(Vec::new())
        };
        let mut history = state.history.lock().map_err(|_| "History unavailable")?;
        match events {
            Ok(events) => history.restore(events),
            Err(message) => {
                history.loaded = false;
                history.error = Some(message.into());
            }
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "History load task failed")?
}

pub fn observe_moderation(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let Ok(runtime) = state.runtime.lock() else {
        return;
    };
    if !runtime.unlocked {
        return;
    }
    let Ok(mut history) = state.history.lock() else {
        return;
    };
    if !history.loaded {
        return;
    }
    let before = history.generation;
    for account in &runtime.accounts.accounts {
        history.observe_moderation(
            account.user_id,
            account.moderation.as_ref(),
            chrono::Utc::now(),
        );
    }
    let changed = history.generation != before;
    drop(history);
    drop(runtime);
    if changed {
        publish(app);
    }
}

pub async fn flush(state: &AppState) -> Result<(), String> {
    let gate = state.history_io.clone().lock_owned().await;
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _gate = gate;
        let persistent = {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            runtime.config.session_history_persist && runtime.unlocked
        };
        if !persistent {
            return Ok(());
        }
        let (events, generation) = {
            let history = state.history.lock().map_err(|_| "History unavailable")?;
            if !history.loaded
                || history.generation == history.saved_generation
                || history
                    .error
                    .as_deref()
                    .is_some_and(|e| e.starts_with("Saved history could not be read"))
            {
                return Ok(());
            }
            (history.events.clone(), history.generation)
        };
        let result = session_history::save(&path(&state)?, &events)
            .map_err(|_| "History could not be saved. Activity is still available in memory.");
        let mut history = state.history.lock().map_err(|_| "History unavailable")?;
        match result {
            Ok(()) => {
                history.saved_generation = generation;
                history.error = None;
                Ok(())
            }
            Err(error) => {
                history.error = Some(error.into());
                Err(error.into())
            }
        }
    })
    .await
    .map_err(|_| "History save task failed")?
}

#[tauri::command]
pub async fn get_session_history(
    state: tauri::State<'_, AppState>,
) -> Result<HistorySnapshot, String> {
    initialize(&state).await?;
    snapshot(&state)
}

#[tauri::command]
pub async fn set_history_persistence(
    app: tauri::AppHandle,
    persistent: bool,
) -> Result<HistorySnapshot, String> {
    let state = app.state::<AppState>().inner().clone();
    initialize(&state).await?;
    let gate = state.history_io.clone().lock_owned().await;
    let worker = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _gate = gate;
        let (events, config_path) = {
            let runtime = worker
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            if !runtime.unlocked {
                return Err("Unlock the account store to change history storage".into());
            }
            let history = worker.history.lock().map_err(|_| "History unavailable")?;
            if persistent
                && history
                    .error
                    .as_deref()
                    .is_some_and(|e| e.starts_with("Saved history could not be read"))
            {
                return Err("Clear unreadable history before enabling file storage".into());
            }
            (history.events.clone(), runtime.config_path.clone())
        };
        if persistent {
            session_history::save(&path(&worker)?, &events)
                .map_err(|_| "History could not be saved. Storage mode was not changed.")?;
        }
        // config writes are serialized with other settings; preserve unrelated newer settings.
        let mut runtime = worker
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked {
            return Err("The account store was locked. Try again.".into());
        }
        let mut config = runtime.config.clone();
        config.session_history_persist = persistent;
        config
            .save(&config_path)
            .map_err(|_| "History preference could not be saved")?;
        runtime.config = config;
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "History preference task failed")??;
    publish(&app);
    snapshot(&state)
}

#[tauri::command]
pub async fn clear_session_history(app: tauri::AppHandle) -> Result<HistorySnapshot, String> {
    let state = app.state::<AppState>().inner().clone();
    initialize(&state).await?;
    let gate = state.history_io.clone().lock_owned().await;
    let worker = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _gate = gate;
        let unlocked = {
            let runtime = worker
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            runtime.unlocked
        };
        if !unlocked {
            return Err("Unlock the account store to clear history".into());
        }
        let file = path(&worker)?;
        session_history::clear_saved(&file)
            .map_err(|_| "History files or backups could not be cleared. Try again.")?;
        let mut history = worker.history.lock().map_err(|_| "History unavailable")?;
        history.clear();
        history.loaded = true;
        history.error = None;
        history.saved_generation = history.generation;
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "History clear task failed")??;
    publish(&app);
    snapshot(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ram_core::{
        crypto,
        models::{AppConfig, Presence},
    };

    fn synthetic_state() -> (AppState, std::path::PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("rm-history-io-{}", uuid::Uuid::new_v4()));
        let state = AppState::default();
        {
            let mut runtime = state.runtime.lock().unwrap();
            runtime.config = AppConfig {
                session_history_persist: true,
                ..Default::default()
            };
            runtime.config_path = directory.join("config.json");
            runtime.session =
                Some(crypto::create_password_session(&uuid::Uuid::new_v4().to_string()).unwrap());
            runtime.unlocked = true;
        }
        (state, directory)
    }

    #[tokio::test]
    async fn flush_load_and_failed_write_preserve_memory() {
        let (state, directory) = synthetic_state();
        initialize(&state).await.unwrap();
        state
            .history
            .lock()
            .unwrap()
            .observe_presence(1, &Presence::default(), chrono::Utc::now());
        flush(&state).await.unwrap();
        *state.history.lock().unwrap() = Default::default();
        initialize(&state).await.unwrap();
        assert_eq!(snapshot(&state).unwrap().events.len(), 1);
        let file = path(&state).unwrap();
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        state
            .history
            .lock()
            .unwrap()
            .observe_presence(1, &Presence::default(), chrono::Utc::now());
        assert!(flush(&state).await.is_err());
        let history = state.history.lock().unwrap();
        assert_eq!(history.events.len(), 2);
        assert!(history.error.is_some());
        drop(history);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn memory_mode_does_not_load_or_write_saved_history() {
        let (state, directory) = synthetic_state();
        state.runtime.lock().unwrap().config.session_history_persist = false;
        initialize(&state).await.unwrap();
        state
            .history
            .lock()
            .unwrap()
            .observe_presence(1, &Presence::default(), chrono::Utc::now());
        flush(&state).await.unwrap();
        assert!(!path(&state).unwrap().exists());
        *state.history.lock().unwrap() = Default::default();
        initialize(&state).await.unwrap();
        assert!(snapshot(&state).unwrap().events.is_empty());
        assert!(!directory.exists());
    }

    #[tokio::test]
    async fn unreadable_history_can_be_retried_without_clearing_it() {
        let (state, directory) = synthetic_state();
        let file = path(&state).unwrap();
        ram_core::storage::atomic_swap(&file, b"broken").unwrap();
        initialize(&state).await.unwrap();
        assert!(!state.history.lock().unwrap().loaded);
        assert!(snapshot(&state).unwrap().error.is_some());
        session_history::save(&file, &[]).unwrap();
        initialize(&state).await.unwrap();
        assert!(state.history.lock().unwrap().loaded);
        assert!(snapshot(&state).unwrap().error.is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
