use ram_core::{models::AppConfig, storage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PageVisibility {
    pub accounts: bool,
    pub instances: bool,
    pub groups: bool,
    pub private_servers: bool,
    pub presets: bool,
    pub inventories: bool,
    pub asset_manager: bool,
}

impl Default for PageVisibility {
    fn default() -> Self {
        Self {
            accounts: true,
            instances: true,
            groups: true,
            private_servers: true,
            presets: true,
            inventories: false,
            asset_manager: false,
        }
    }
}

fn path(config_path: &Path) -> PathBuf {
    config_path.with_file_name("page-visibility.json")
}

pub fn load(config_path: &Path) -> PageVisibility {
    match std::fs::read(path(config_path)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            tracing::warn!("Invalid page visibility preferences; using defaults");
            PageVisibility::default()
        }),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Page visibility preferences could not be read; using defaults");
            }
            PageVisibility::default()
        }
    }
}

pub fn save_settings(
    config_path: &Path,
    config: &AppConfig,
    pages: &PageVisibility,
) -> Result<(), String> {
    let original = match std::fs::read(config_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err("Existing settings could not be read. Nothing was saved.".into()),
    };
    let bytes =
        serde_json::to_vec_pretty(pages).map_err(|_| "Page preferences could not be encoded.")?;
    config
        .save(config_path)
        .map_err(|_| "Application settings could not be saved.")?;
    if storage::atomic_write(&path(config_path), &bytes).is_err() {
        let restored = match original {
            Some(bytes) => storage::atomic_swap(config_path, &bytes).is_ok(),
            None => std::fs::remove_file(config_path).is_ok(),
        };
        return Err(if restored { "Page visibility could not be saved. Application settings were restored; retry saving." } else { "Page visibility could not be saved and application settings could not be restored. Reload Settings before retrying." }.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_hide_only_optional_workspaces() {
        let pages = PageVisibility::default();
        assert!(
            pages.accounts
                && pages.instances
                && pages.groups
                && pages.private_servers
                && pages.presets
        );
        assert!(!pages.inventories && !pages.asset_manager);
        assert_eq!(serde_json::from_str::<PageVisibility>("{}").unwrap(), pages);
    }

    #[test]
    fn page_choices_survive_reload_and_config_saves() {
        let directory =
            std::env::temp_dir().join(format!("rm-page-visibility-{}", uuid::Uuid::new_v4()));
        let config_path = directory.join("config.json");
        let pages = PageVisibility {
            accounts: false,
            inventories: true,
            ..Default::default()
        };
        save_settings(&config_path, &AppConfig::default(), &pages).unwrap();
        AppConfig::default().save(&config_path).unwrap();
        assert_eq!(load(&config_path), pages);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_visibility_save_restores_application_settings() {
        let directory =
            std::env::temp_dir().join(format!("rm-page-rollback-{}", uuid::Uuid::new_v4()));
        let config_path = directory.join("config.json");
        let original = AppConfig::default();
        original.save(&config_path).unwrap();
        let bytes = std::fs::read(&config_path).unwrap();
        std::fs::create_dir(path(&config_path)).unwrap();
        let candidate = AppConfig {
            confirm_kill_all: !original.confirm_kill_all,
            ..original
        };
        assert!(save_settings(&config_path, &candidate, &PageVisibility::default()).is_err());
        assert_eq!(std::fs::read(&config_path).unwrap(), bytes);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
