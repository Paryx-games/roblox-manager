#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod state;

use ram_core::models::Presence;
use serde::Serialize;
use state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountSummary {
    user_id: u64,
    label: String,
    username: String,
    display_name: String,
    presence: &'static str,
    presence_text: String,
    can_launch: bool,
    last_activity: Option<String>,
}

#[tauri::command]
fn list_accounts(state: tauri::State<'_, AppState>) -> Result<Vec<AccountSummary>, String> {
    let accounts = state
        .accounts
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    Ok(accounts
        .accounts
        .iter()
        .map(|account| AccountSummary {
            user_id: account.user_id,
            label: account.label().to_string(),
            username: account.username.clone(),
            display_name: account.display_name.clone(),
            presence: presence_kind(&account.last_presence),
            presence_text: account.last_presence.status_text().to_string(),
            can_launch: account.can_launch(),
            last_activity: account
                .last_used
                .or(account.last_validated)
                .map(|timestamp| timestamp.to_rfc3339()),
        })
        .collect())
}

fn presence_kind(presence: &Presence) -> &'static str {
    match presence.user_presence_type {
        1..=3 => "online",
        _ => "neutral",
    }
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![list_accounts])
        .run(tauri::generate_context!())
        .expect("error while running RM");
}
