use ram_core::models::AccountStore;
use std::sync::Mutex;

pub struct AppState {
    pub accounts: Mutex<AccountStore>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            accounts: Mutex::new(AccountStore::default()),
        }
    }
}
