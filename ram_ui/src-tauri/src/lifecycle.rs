use crate::{state::AppState, StoreStatus};
use ram_core::{crypto, models::AccountStore, presets, storage};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager};

pub fn data_directory() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RM")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    needs_tutorial: bool,
    changelog: Option<String>,
    passwordless_offer: bool,
    legacy_migration_available: bool,
}

fn release_notes(changelog: &str, version: &str) -> Option<String> {
    let sections: Vec<_> = changelog.split("\n## ").skip(1).collect();
    let matching = sections.iter().find(|section| {
        section
            .lines()
            .next()
            .is_some_and(|heading| heading.trim().trim_start_matches('v') == version)
    });
    let section = matching.or_else(|| {
        version
            .contains('-')
            .then(|| {
                sections.iter().find(|section| {
                    section
                        .lines()
                        .next()
                        .is_some_and(|heading| heading.trim() == "Unreleased")
                })
            })
            .flatten()
    })?;
    let content = section.split_once('\n')?.1;
    let content = if matching.is_none() {
        &content[content.find("### ")?..]
    } else {
        content.trim()
    };
    Some(format!("## v{version}\n\n{}", content.trim()))
}

fn migrate_favourites(
    runtime: &mut crate::state::RuntimeState,
    directory: &Path,
) -> Result<(), String> {
    presets::migrate_favourites(&mut runtime.config, directory, &runtime.config_path)
}

#[tauri::command]
pub async fn startup_status(state: tauri::State<'_, AppState>) -> Result<StartupStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable")?;
        migrate_favourites(&mut runtime, &data_directory())?;
        if runtime.unlocked
            && !runtime.config.offered_passwordless
            && runtime
                .session
                .as_ref()
                .is_some_and(|session| !session.needs_password())
        {
            let mut candidate = runtime.config.clone();
            candidate.offered_passwordless = true;
            candidate
                .save(&runtime.config_path)
                .map_err(|_| "The device-encryption preference could not be recorded")?;
            runtime.config = candidate;
        }
        let version = env!("CARGO_PKG_VERSION");
        let changed_version = runtime
            .config
            .last_seen_version
            .as_deref()
            .is_some_and(|previous| previous != version);
        Ok(StartupStatus {
            needs_tutorial: runtime.is_first_install && runtime.config.last_seen_version.is_none(),
            changelog: changed_version
                .then(|| release_notes(include_str!("../../../CHANGELOG.md"), version))
                .flatten(),
            passwordless_offer: runtime.unlocked
                && !runtime.config.offered_passwordless
                && runtime
                    .session
                    .as_ref()
                    .is_some_and(|session| session.needs_password()),
            legacy_migration_available: runtime.config_path != data_directory().join("config.json")
                && !data_directory().join("config.json").exists()
                && !data_directory().join("accounts.dat").exists(),
        })
    })
    .await
    .map_err(|_| "Startup status task failed")?
}

#[tauri::command]
pub async fn acknowledge_startup(
    state: tauri::State<'_, AppState>,
    section: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable")?;
        let mut candidate = runtime.config.clone();
        match section.as_str() {
            "version" => {
                if candidate.last_seen_version.as_deref() == Some(env!("CARGO_PKG_VERSION")) {
                    return Ok(());
                }
                candidate.last_seen_version = Some(env!("CARGO_PKG_VERSION").into());
            }
            "passwordless" => candidate.offered_passwordless = true,
            _ => return Err("Unknown startup section".into()),
        }
        candidate
            .save(&runtime.config_path)
            .map_err(|_| "The startup choice could not be saved")?;
        runtime.config = candidate;
        Ok(())
    })
    .await
    .map_err(|_| "Startup choice task failed")?
}

#[tauri::command]
pub async fn migrate_legacy_data(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable")?;
        let directory = data_directory();
        let target_config = directory.join("config.json");
        let target_accounts = directory.join("accounts.dat");
        if target_config.exists() || target_accounts.exists() {
            return Err("Modern data already exists. Nothing was overwritten.".into());
        }
        std::fs::create_dir_all(&directory)
            .map_err(|_| "The data directory could not be created")?;
        let mut candidate = runtime.config.clone();
        if runtime.config.accounts_path.is_file() {
            let encrypted = std::fs::read(&runtime.config.accounts_path)
                .map_err(|_| "The old encrypted store could not be read")?;
            storage::atomic_write(&target_accounts, &encrypted)
                .map_err(|_| "The encrypted store could not be copied")?;
            candidate.accounts_path = target_accounts;
        } else {
            candidate.accounts_path = target_accounts;
        }
        if candidate.save(&target_config).is_err() {
            if candidate.accounts_path.is_file() && runtime.config.accounts_path.is_file() {
                let _ = std::fs::remove_file(&candidate.accounts_path);
            }
            return Err(
                "Migration could not save the new configuration. Original files remain in place."
                    .into(),
            );
        }
        runtime.config = candidate;
        runtime.config_path = target_config;
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "Migration task failed")??;
    crate::accounts::publish(&app);
    let _ = app.emit(
        "settings-updated",
        crate::SettingsConfig::from_config(
            &app.state::<AppState>()
                .runtime
                .lock()
                .map_err(|_| "Application state unavailable")?
                .config,
        ),
    );
    Ok(())
}

#[tauri::command]
pub async fn reset_account_store(
    app: tauri::AppHandle,
    confirmation: String,
) -> Result<StoreStatus, String> {
    if confirmation != "START OVER" {
        return Err("Type START OVER to confirm creating a new store".into());
    }
    let state = app.state::<AppState>().inner().clone();
    let status = tauri::async_runtime::spawn_blocking(move || -> Result<StoreStatus, String> {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if runtime.unlocked {
            return Err(
                "The account store is unlocked. Use account removal instead of recovery.".into(),
            );
        }
        let path = runtime.config.accounts_path.clone();
        if path.is_file() {
            let encrypted = std::fs::read(&path)
                .map_err(|_| "The existing encrypted store could not be backed up")?;
            let backup = path.with_extension(format!("recovery-{}.dat", uuid::Uuid::new_v4()));
            storage::atomic_write(&backup, &encrypted)
                .map_err(|_| "Recovery backup failed. The account store was not changed.")?;
            let previous_backup = path.with_extension("dat.bak");
            if previous_backup.is_file() {
                let bytes = std::fs::read(previous_backup)
                    .map_err(|_| "The previous backup could not be preserved")?;
                storage::atomic_write(&backup.with_extension("previous.dat"), &bytes)
                    .map_err(|_| "The previous backup could not be preserved")?;
            }
        }
        let session = crypto::create_device_session()
            .map_err(|_| "Device encryption could not be created")?;
        let accounts = AccountStore::default();
        crypto::save_store(&path, &accounts, &session)
            .map_err(|_| "The new encrypted store could not be saved")?;
        runtime.accounts = accounts;
        runtime.session = Some(session);
        runtime.unlocked = true;
        runtime.legacy_store = false;
        runtime
            .credential_revisions
            .values_mut()
            .for_each(|revision| *revision += 1);
        Ok(StoreStatus {
            exists: true,
            unlocked: true,
            needs_password: false,
            legacy: false,
            account_count: 0,
        })
    })
    .await
    .map_err(|_| "Recovery task failed")??;
    app.state::<AppState>()
        .pending_additions
        .lock()
        .map_err(|_| "Account state unavailable")?
        .clear();
    crate::accounts::publish(&app);
    Ok(status)
}

#[tauri::command]
pub async fn check_release_update(
    state: tauri::State<'_, AppState>,
) -> Result<Option<(String, String)>, String> {
    if !state
        .runtime
        .lock()
        .map_err(|_| "Application state unavailable")?
        .config
        .show_update_notifications
    {
        return Ok(None);
    }
    let update = ram_core::api::check_for_updates(env!("CARGO_PKG_VERSION"))
        .await
        .map_err(|_| "The update check could not be completed")?;
    Ok(update.filter(|(version, url)| {
        ram_core::api::is_newer_stable_release(version, env!("CARGO_PKG_VERSION"))
            && url.starts_with("https://github.com/Paryx-games/roblox-manager/releases/")
    }))
}

#[tauri::command]
pub async fn open_release_page(url: String) -> Result<(), String> {
    if !url.starts_with("https://github.com/Paryx-games/roblox-manager/releases/")
        || url.len() > 2048
        || url.chars().any(char::is_control)
    {
        return Err("Invalid release URL".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new("explorer.exe")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|_| "The release page could not be opened".into())
    })
    .await
    .map_err(|_| "Release page task failed")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_notes_select_only_the_installed_version() {
        let changelog = "# Changelog\n\n## Unreleased\n\nDevelopment links\n\n### Fixed\n\n- Upcoming\n\n## v2.0.0-beta.1\n\n### Added\n\n- Current\n\n## v1.0.0\n\n- Older\n";
        assert_eq!(
            release_notes(changelog, "2.0.0-beta.1").unwrap(),
            "## v2.0.0-beta.1\n\n### Added\n\n- Current"
        );
        assert!(release_notes(changelog, "2.0.0").is_none());
    }

    #[test]
    fn prerelease_notes_use_unreleased_without_the_development_intro() {
        let changelog = "# Changelog\r\n\r\n## Unreleased\r\n\r\nDevelopment links\r\n\r\n### Fixed\r\n\r\n- Current\r\n\r\n## v1.0.0\r\n\r\n- Older";
        assert_eq!(
            release_notes(changelog, "2.0.0-beta.1").unwrap(),
            "## v2.0.0-beta.1\n\n### Fixed\r\n\r\n- Current"
        );
    }

    #[test]
    fn favourites_migrate_without_duplicates_and_invalid_entries_are_retained() {
        let directory =
            std::env::temp_dir().join(format!("rm-migration-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut runtime = crate::state::RuntimeState {
            accounts: AccountStore::default(),
            config: ram_core::models::AppConfig::default(),
            config_path: directory.join("config.json"),
            session: None,
            unlocked: false,
            legacy_store: false,
            is_first_install: false,
            credential_revisions: Default::default(),
        };
        let favourite = ram_core::models::FavoritePlace {
            name: "Synthetic favourite".into(),
            place_id: 123,
        };
        runtime.config.favorite_places.push(favourite.clone());
        migrate_favourites(&mut runtime, &directory).unwrap();
        assert!(runtime.config.favorite_places.is_empty());
        runtime.config.favorite_places.push(favourite);
        migrate_favourites(&mut runtime, &directory).unwrap();
        assert_eq!(presets::load_all(&directory).unwrap().0.len(), 1);
        runtime
            .config
            .favorite_places
            .push(ram_core::models::FavoritePlace {
                name: "Invalid".into(),
                place_id: 0,
            });
        assert!(migrate_favourites(&mut runtime, &directory).is_err());
        assert_eq!(runtime.config.favorite_places.len(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
