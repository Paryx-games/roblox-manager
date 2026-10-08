use ram_core::crypto::StoreSession;
use ram_core::models::{AccountStore, AppConfig};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct RuntimeState {
    pub accounts: AccountStore,
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub session: Option<StoreSession>,
    pub unlocked: bool,
    pub legacy_store: bool,
    pub is_first_install: bool,
    pub credential_revisions: std::collections::HashMap<u64, u64>,
}

#[derive(Clone)]
pub struct AppState {
    pub runtime: Arc<Mutex<RuntimeState>>,
    pub inventory_fetches: Arc<tokio::sync::Semaphore>,
    pub inventory_cache: Arc<ram_core::cache::ResponseCache<(u64, u64), Vec<crate::InventoryItem>>>,
    pub presence_cache:
        Arc<ram_core::cache::ResponseCache<(u64, u64, u64), ram_core::models::Presence>>,
    pub account_refresh: Arc<tokio::sync::Mutex<()>>,
    pub pending_additions:
        Arc<Mutex<std::collections::HashMap<String, crate::accounts::PendingAddition>>>,
    pub instances: Arc<Mutex<crate::instances::InstanceState>>,
    pub mac_rotated: Arc<std::sync::atomic::AtomicBool>,
    pub launch_queue: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    pub history: Arc<Mutex<ram_core::session_history::History>>,
    pub history_io: Arc<tokio::sync::Mutex<()>>,
    pub is_shutting_down: Arc<std::sync::atomic::AtomicBool>,
}

fn is_legacy_directory(directory: &Path) -> bool {
    directory.join("accounts.dat").is_file()
        || std::fs::read_to_string(directory.join("config.json"))
            .ok()
            .and_then(|content| serde_json::from_str::<AppConfig>(&content).ok())
            .is_some()
}

impl Default for AppState {
    fn default() -> Self {
        let data_dir = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RM");
        let modern_config = data_dir.join("config.json");
        let legacy_directory = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(PathBuf::from))
            .filter(|directory| is_legacy_directory(directory));
        let source_directory = if modern_config.exists() || data_dir.join("accounts.dat").exists() {
            data_dir.clone()
        } else {
            legacy_directory.unwrap_or_else(|| data_dir.clone())
        };
        let config_path = source_directory.join("config.json");
        let mut config = AppConfig::load(&config_path);
        if !config.accounts_path.is_absolute() {
            config.accounts_path = source_directory.join(&config.accounts_path);
        }

        Self {
            runtime: Arc::new(Mutex::new(RuntimeState {
                is_first_install: !config.accounts_path.is_file()
                    && config.last_seen_version.is_none(),
                accounts: AccountStore::default(),
                config,
                config_path,
                session: None,
                unlocked: false,
                legacy_store: false,
                credential_revisions: std::collections::HashMap::new(),
            })),
            inventory_fetches: Arc::new(tokio::sync::Semaphore::new(4)),
            inventory_cache: Arc::new(ram_core::cache::ResponseCache::new(
                std::time::Duration::from_secs(60),
                32,
            )),
            presence_cache: Arc::new(ram_core::cache::ResponseCache::new(
                std::time::Duration::from_secs(2),
                2048,
            )),
            account_refresh: Arc::new(tokio::sync::Mutex::new(())),
            pending_additions: Arc::new(Mutex::new(std::collections::HashMap::new())),
            instances: Arc::new(Mutex::new(crate::instances::InstanceState::default())),
            mac_rotated: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            launch_queue: Arc::new(tokio::sync::Mutex::new(None)),
            history: Arc::new(Mutex::new(Default::default())),
            history_io: Arc::new(tokio::sync::Mutex::new(())),
            is_shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_config_is_not_adopted_as_legacy_data() {
        let directory =
            std::env::temp_dir().join(format!("rm-legacy-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        ram_core::storage::atomic_write(
            &directory.join("config.json"),
            br#"{"otherApplication":true}"#,
        )
        .unwrap();
        assert!(!is_legacy_directory(&directory));
        AppConfig::default()
            .save(&directory.join("config.json"))
            .unwrap();
        assert!(is_legacy_directory(&directory));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
