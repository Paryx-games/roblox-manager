use ram_core::crypto::StoreSession;
use ram_core::models::{AccountStore, AppConfig};
use std::path::PathBuf;
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
    pub account_refresh: Arc<tokio::sync::Mutex<()>>,
    pub pending_additions:
        Arc<Mutex<std::collections::HashMap<String, crate::accounts::PendingAddition>>>,
    pub instances: Arc<Mutex<crate::instances::InstanceState>>,
    pub launch_queue: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    pub is_shutting_down: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for AppState {
    fn default() -> Self {
        let data_dir = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RM");
        let modern_config = data_dir.join("config.json");
        let legacy_directory = [
            std::env::current_dir().ok(),
            std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(PathBuf::from)),
        ]
        .into_iter()
        .flatten()
        .find(|directory| {
            directory.join("config.json").is_file() || directory.join("accounts.dat").is_file()
        });
        let source_directory = if modern_config.exists() || data_dir.join("accounts.dat").exists() {
            data_dir.clone()
        } else {
            legacy_directory.unwrap_or_else(|| data_dir.clone())
        };
        let config_path = source_directory.join("config.json");
        let mut config = AppConfig::load(&config_path);
        if config.accounts_path == std::path::Path::new("accounts.dat") {
            config.accounts_path = source_directory.join("accounts.dat");
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
            account_refresh: Arc::new(tokio::sync::Mutex::new(())),
            pending_additions: Arc::new(Mutex::new(std::collections::HashMap::new())),
            instances: Arc::new(Mutex::new(crate::instances::InstanceState::default())),
            launch_queue: Arc::new(tokio::sync::Mutex::new(None)),
            is_shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}
