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
}

pub struct AppState {
    pub runtime: Arc<Mutex<RuntimeState>>,
}

impl Default for AppState {
    fn default() -> Self {
        let data_dir = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RM");
        let config_path = data_dir.join("config.json");
        let mut config = AppConfig::load(&config_path);
        if config.accounts_path == PathBuf::from("accounts.dat") {
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
            })),
        }
    }
}
