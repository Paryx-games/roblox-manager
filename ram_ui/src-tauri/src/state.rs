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
        let config_path = data_dir.join("config.json");
        let mut config = AppConfig::load(&config_path);
        if config.accounts_path == std::path::Path::new("accounts.dat") {
            config.accounts_path = data_dir.join("accounts.dat");
        }

        Self {
            runtime: Arc::new(Mutex::new(RuntimeState {
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
