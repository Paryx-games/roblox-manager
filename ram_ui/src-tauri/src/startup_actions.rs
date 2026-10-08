//! One startup attempt after unlock; owned and cancelled by BackgroundTasks.
use crate::{
    accounts,
    launcher::{self, LaunchRequest},
    state::AppState,
};
use ram_core::models::AppConfig;
use std::{sync::atomic::Ordering, time::Duration};
use tauri::{Emitter, Manager};

fn startup_target(config: &AppConfig) -> Result<Option<(u64, u64)>, &'static str> {
    if !config.auto_launch_on_startup {
        return Ok(None);
    }
    match (config.auto_launch_account_id, config.auto_launch_place_id) {
        (Some(account), Some(place)) if account > 0 && place > 0 => Ok(Some((account, place))),
        _ => Err(
            "Startup launch needs an Account ID and Place ID. Set them in Settings and restart RM.",
        ),
    }
}

pub async fn run(app: tauri::AppHandle) {
    let state = app.state::<AppState>().inner().clone();
    let config = loop {
        if state.is_shutting_down.load(Ordering::Acquire) {
            return;
        }
        let config = state
            .runtime
            .lock()
            .ok()
            .filter(|runtime| runtime.unlocked)
            .map(|runtime| runtime.config.clone());
        if let Some(config) = config {
            break config;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    let target = startup_target(&config);
    let mut target_checked = false;
    if config.refresh_on_startup {
        match accounts::refresh(&app, Vec::new()).await {
            Ok(accounts) => {
                target_checked = target
                    .as_ref()
                    .ok()
                    .and_then(|value| *value)
                    .is_some_and(|(id, _)| accounts.iter().any(|account| account.user_id == id))
            }
            Err(_) => {
                let _ = app.emit(
                    "background-notice",
                    "Startup account checks failed. Refresh Accounts to retry.",
                );
            }
        }
    }
    let target = match target {
        Ok(Some(target)) => target,
        Ok(None) => return,
        Err(message) => {
            let _ = app.emit("background-notice", message);
            return;
        }
    };
    // always validate the launch account, even when general startup checks are disabled
    if !target_checked && accounts::refresh(&app, vec![target.0]).await.is_err() {
        let _ = app.emit("background-notice", "Startup launch stopped because the account could not be checked. Refresh Accounts and launch manually.");
        return;
    }
    let still_enabled = state.runtime.lock().ok().is_some_and(|runtime| {
        runtime.unlocked && startup_target(&runtime.config).ok().flatten() == Some(target)
    });
    if !still_enabled || state.is_shutting_down.load(Ordering::Acquire) {
        return;
    }
    if let Err(error) = launcher::launch_startup(
        &app,
        LaunchRequest {
            user_id: target.0,
            place_id: target.1,
            job_id: None,
            data: None,
            link_code: None,
            access_code: None,
        },
    )
    .await
    {
        let _ = app.emit(
            "background-notice",
            format!("Startup launch stopped: {error}"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_never_choose_an_arbitrary_startup_game() {
        let config = AppConfig {
            auto_launch_on_startup: true,
            auto_launch_account_id: Some(1),
            ..Default::default()
        };
        assert!(startup_target(&config).is_err());
        assert_eq!(startup_target(&AppConfig::default()).unwrap(), None);
        assert_eq!(
            startup_target(&AppConfig {
                auto_launch_place_id: Some(2),
                ..config
            })
            .unwrap(),
            Some((1, 2))
        );
    }
    #[test]
    fn startup_place_is_a_compatible_optional_config_addition() {
        let mut value = serde_json::to_value(AppConfig::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("auto_launch_place_id");
        value["auto_launch_on_startup"] = serde_json::json!(true);
        value["auto_launch_account_id"] = serde_json::json!(7);
        let old: AppConfig = serde_json::from_value(value).unwrap();
        assert_eq!(old.auto_launch_account_id, Some(7));
        assert_eq!(old.auto_launch_place_id, None);
        assert!(startup_target(&old).is_err());
    }
}
