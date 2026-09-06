#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod state;

#[path = "../../src/browser_login.rs"]
#[allow(dead_code)]
mod browser_login;

use ram_core::crypto;
use ram_core::models::{Account, GroupMeta, LaunchPreset, Presence};
use ram_core::{api, assets_api, auth::RobloxClient, process};
use serde::Serialize;
use state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountSummary {
    user_id: u64,
    label: String,
    username: String,
    display_name: String,
    alias: String,
    group: String,
    avatar_url: String,
    is_pinned: bool,
    sort_order: u32,
    cookie_expired: bool,
    moderation_active: bool,
    moderation_banned: bool,
    moderation_reason: Option<String>,
    moderation_expires_at: Option<String>,
    player_path: Option<String>,
    created_at: Option<String>,
    presence: &'static str,
    presence_text: String,
    presence_location: String,
    can_launch: bool,
    last_activity: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreStatus {
    exists: bool,
    unlocked: bool,
    needs_password: bool,
    legacy: bool,
    account_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryItem {
    asset_id: u64,
    name: String,
    asset_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresenceUpdate {
    user_id: u64,
    presence: &'static str,
    presence_text: String,
    location: String,
}

fn account_cookie(runtime: &state::RuntimeState, user_id: u64) -> Result<String, String> {
    let account = runtime
        .accounts
        .find_by_id(user_id)
        .ok_or_else(|| "Account not found".to_string())?;
    if runtime.config.use_credential_manager {
        crypto::credential_load(user_id).map_err(|error| error.to_string())
    } else {
        let encrypted = account
            .encrypted_cookie
            .as_deref()
            .ok_or_else(|| "This account has no stored credential".to_string())?;
        let session = runtime
            .session
            .as_ref()
            .ok_or_else(|| "Account store is locked".to_string())?;
        crypto::decrypt_cookie(encrypted, session).map_err(|error| error.to_string())
    }
}

fn account_summary(account: &Account, player_path: Option<String>) -> AccountSummary {
    AccountSummary {
        user_id: account.user_id,
        label: account.label().to_string(),
        username: account.username.clone(),
        display_name: account.display_name.clone(),
        alias: account.alias.clone(),
        group: account.group.clone(),
        avatar_url: account.avatar_url.clone(),
        is_pinned: account.is_pinned,
        sort_order: account.sort_order,
        cookie_expired: account.cookie_expired,
        moderation_active: account
            .moderation
            .as_ref()
            .is_some_and(|info| info.is_active()),
        moderation_banned: account
            .moderation
            .as_ref()
            .is_some_and(|info| info.is_banned),
        moderation_reason: account
            .moderation
            .as_ref()
            .and_then(|info| info.reason.clone()),
        moderation_expires_at: account
            .moderation
            .as_ref()
            .and_then(|info| info.expires_at.map(|value| value.to_rfc3339())),
        player_path,
        created_at: account.created_at.map(|value| value.to_rfc3339()),
        presence: presence_kind(&account.last_presence),
        presence_text: account.last_presence.status_text().to_string(),
        presence_location: account.last_presence.last_location.clone(),
        can_launch: account.can_launch(),
        last_activity: account
            .last_used
            .or(account.last_validated)
            .map(|timestamp| timestamp.to_rfc3339()),
    }
}

fn add_account_with_cookie(
    runtime: &mut state::RuntimeState,
    cookie: &str,
    user_id: u64,
    username: String,
    display_name: String,
) -> Result<AccountSummary, String> {
    if runtime.accounts.find_by_id(user_id).is_some() {
        return Err("This account is already managed".to_string());
    }
    let mut account = Account::new(user_id, username, display_name);
    if runtime.config.use_credential_manager {
        crypto::credential_store(user_id, cookie).map_err(|error| error.to_string())?;
    } else {
        let session = runtime
            .session
            .as_ref()
            .ok_or_else(|| "Account store is locked".to_string())?;
        account.encrypted_cookie =
            Some(crypto::encrypt_cookie(cookie, session).map_err(|error| error.to_string())?);
    }
    runtime.accounts.accounts.push(account);
    let account = runtime
        .accounts
        .accounts
        .last()
        .expect("account was pushed");
    let summary = account_summary(
        account,
        runtime
            .config
            .custom_player_paths
            .get(&user_id)
            .map(|path| path.display().to_string()),
    );
    save_runtime(runtime)?;
    Ok(summary)
}

#[tauri::command]
fn store_status(state: tauri::State<'_, AppState>) -> Result<StoreStatus, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    let exists = runtime.config.accounts_path.is_file();
    let needs_password = if exists {
        crypto::peek_mode(&runtime.config.accounts_path)
            .map_err(|_| "Account store status unavailable".to_string())?
            .is_some_and(|mode| mode == crypto::StoreMode::Password)
    } else {
        false
    };
    Ok(StoreStatus {
        exists,
        unlocked: runtime.unlocked,
        needs_password,
        legacy: runtime.legacy_store,
        account_count: runtime.accounts.accounts.len(),
    })
}

#[tauri::command]
fn unlock_device(state: tauri::State<'_, AppState>) -> Result<StoreStatus, String> {
    let (path, runtime_state) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        (runtime.config.accounts_path.clone(), state.runtime.clone())
    };
    let (accounts, session) =
        crypto::unlock_with_device(&path).map_err(|error| error.to_string())?;
    let mut runtime = runtime_state
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    runtime.accounts = accounts;
    runtime.legacy_store = session.is_legacy();
    runtime.session = Some(session);
    runtime.unlocked = true;
    Ok(StoreStatus {
        exists: true,
        unlocked: true,
        needs_password: false,
        legacy: runtime.legacy_store,
        account_count: runtime.accounts.accounts.len(),
    })
}

#[tauri::command]
fn create_device_store(state: tauri::State<'_, AppState>) -> Result<StoreStatus, String> {
    let runtime_state = state.runtime.clone();
    let mut runtime = runtime_state
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    if runtime.config.accounts_path.exists() {
        return Err("Account store already exists".to_string());
    }
    if let Some(parent) = runtime.config.accounts_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let session = crypto::create_device_session().map_err(|error| error.to_string())?;
    crypto::save_store(&runtime.config.accounts_path, &runtime.accounts, &session)
        .map_err(|error| error.to_string())?;
    runtime.session = Some(session);
    runtime.unlocked = true;
    runtime.legacy_store = false;
    Ok(StoreStatus {
        exists: true,
        unlocked: true,
        needs_password: false,
        legacy: false,
        account_count: runtime.accounts.accounts.len(),
    })
}

#[tauri::command]
async fn unlock_password(
    state: tauri::State<'_, AppState>,
    password: String,
) -> Result<StoreStatus, String> {
    let (path, runtime_state) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        (runtime.config.accounts_path.clone(), state.runtime.clone())
    };
    let (accounts, session) = tauri::async_runtime::spawn_blocking(move || {
        crypto::unlock_with_password(&path, &password).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Account unlock task failed".to_string())??;
    let mut runtime = runtime_state
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    runtime.accounts = accounts;
    runtime.legacy_store = session.is_legacy();
    runtime.session = Some(session);
    runtime.unlocked = true;
    Ok(StoreStatus {
        exists: true,
        unlocked: true,
        needs_password: true,
        legacy: runtime.legacy_store,
        account_count: runtime.accounts.accounts.len(),
    })
}

#[tauri::command]
fn list_accounts(state: tauri::State<'_, AppState>) -> Result<Vec<AccountSummary>, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    if !runtime.unlocked {
        return Err("Account store is locked".to_string());
    }
    Ok(runtime
        .accounts
        .accounts
        .iter()
        .map(|account| {
            let player_path = runtime
                .config
                .custom_player_paths
                .get(&account.user_id)
                .map(|path| path.display().to_string());
            account_summary(account, player_path)
        })
        .collect())
}

fn save_runtime(runtime: &state::RuntimeState) -> Result<(), String> {
    let session = runtime
        .session
        .as_ref()
        .ok_or_else(|| "Account store is locked".to_string())?;
    crypto::save_store(&runtime.config.accounts_path, &runtime.accounts, session)
        .map_err(|error| error.to_string())
}

fn configured_player_path(runtime: &state::RuntimeState, user_id: u64) -> Option<String> {
    runtime
        .config
        .custom_player_paths
        .get(&user_id)
        .map(|path| path.display().to_string())
}

#[tauri::command]
fn update_account_alias(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    alias: String,
) -> Result<AccountSummary, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    if alias.chars().count() > 64 {
        return Err("Alias must be 64 characters or fewer".to_string());
    }
    let player_path = configured_player_path(&runtime, user_id);
    let summary = {
        let account = runtime
            .accounts
            .find_by_id_mut(user_id)
            .ok_or_else(|| "Account not found".to_string())?;
        account.alias = alias.trim().to_string();
        account_summary(account, player_path)
    };
    save_runtime(&runtime)?;
    Ok(summary)
}

#[tauri::command]
fn toggle_account_pin(
    state: tauri::State<'_, AppState>,
    user_id: u64,
) -> Result<AccountSummary, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    let player_path = configured_player_path(&runtime, user_id);
    let summary = {
        let account = runtime
            .accounts
            .find_by_id_mut(user_id)
            .ok_or_else(|| "Account not found".to_string())?;
        account.is_pinned = !account.is_pinned;
        account_summary(account, player_path)
    };
    save_runtime(&runtime)?;
    Ok(summary)
}

#[tauri::command]
fn update_account_group(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    group: String,
) -> Result<AccountSummary, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    let group = group.trim();
    if group.chars().count() > 64 {
        return Err("Group name must be 64 characters or fewer".to_string());
    }
    let player_path = configured_player_path(&runtime, user_id);
    let summary = {
        let account = runtime
            .accounts
            .find_by_id_mut(user_id)
            .ok_or_else(|| "Account not found".to_string())?;
        account.group = group.to_string();
        account_summary(account, player_path)
    };
    save_runtime(&runtime)?;
    Ok(summary)
}

#[tauri::command]
fn reorder_accounts(
    state: tauri::State<'_, AppState>,
    user_ids: Vec<u64>,
) -> Result<Vec<AccountSummary>, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    for (sort_order, user_id) in user_ids.iter().enumerate() {
        if let Some(account) = runtime.accounts.find_by_id_mut(*user_id) {
            account.sort_order = sort_order as u32;
        }
    }
    let summaries = runtime
        .accounts
        .accounts
        .iter()
        .map(|account| account_summary(account, configured_player_path(&runtime, account.user_id)))
        .collect();
    save_runtime(&runtime)?;
    Ok(summaries)
}

#[tauri::command]
fn update_player_path(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    path: Option<String>,
) -> Result<AccountSummary, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    let path = path.map(std::path::PathBuf::from);
    if let Some(candidate) = path.as_ref() {
        if !candidate.is_dir() && !candidate.is_file() {
            return Err("The Roblox player path does not exist".to_string());
        }
    }
    if let Some(candidate) = path {
        runtime
            .config
            .custom_player_paths
            .insert(user_id, candidate);
    } else {
        runtime.config.custom_player_paths.remove(&user_id);
    }
    runtime
        .config
        .save(&runtime.config_path)
        .map_err(|error| error.to_string())?;
    let account = runtime
        .accounts
        .find_by_id(user_id)
        .ok_or_else(|| "Account not found".to_string())?;
    let player_path = configured_player_path(&runtime, user_id);
    Ok(account_summary(account, player_path))
}

fn save_config(runtime: &state::RuntimeState) -> Result<(), String> {
    runtime
        .config
        .save(&runtime.config_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_account_group(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err("Group name must be between 1 and 64 characters".to_string());
    }
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    if runtime.config.groups.contains_key(name) {
        return Err("A group with that name already exists".to_string());
    }
    runtime.config.groups.insert(
        name.to_string(),
        GroupMeta {
            color: [59, 130, 246],
            description: String::new(),
            sort_order: u32::MAX,
        },
    );
    save_config(&runtime)
}

#[tauri::command]
fn delete_account_group(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    runtime.config.groups.remove(name.trim());
    for account in &mut runtime.accounts.accounts {
        if account.group == name.trim() {
            account.group.clear();
        }
    }
    save_config(&runtime)?;
    save_runtime(&runtime)
}

#[tauri::command]
fn open_account_url(user_id: u64, inventory: bool) -> Result<(), String> {
    let path = if inventory { "inventory" } else { "profile" };
    let url = format!("https://www.roblox.com/users/{user_id}/{path}");
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|error| format!("Could not open Roblox: {error}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err("Opening Roblox is only supported on Windows".to_string())
    }
}

#[tauri::command]
fn remove_account(state: tauri::State<'_, AppState>, user_id: u64) -> Result<(), String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    if !runtime.accounts.remove_by_id(user_id) {
        return Err("Account not found".to_string());
    }
    if runtime.config.use_credential_manager {
        crypto::credential_delete(user_id).map_err(|error| error.to_string())?;
    }
    save_runtime(&runtime)
}

#[tauri::command]
async fn launch_account(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    place_id: u64,
    job_id: Option<String>,
    data: Option<String>,
) -> Result<(), String> {
    let (
        encrypted_cookie,
        session,
        use_credential_manager,
        multi_instance,
        kill_background,
        privacy,
        player_path,
    ) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let account = runtime
            .accounts
            .find_by_id(user_id)
            .ok_or_else(|| "Account not found".to_string())?;
        if !account.can_launch() {
            return Err("This account is restricted and cannot launch".to_string());
        }
        (
            account.encrypted_cookie.clone(),
            runtime
                .session
                .clone()
                .ok_or_else(|| "Account store is locked".to_string())?,
            runtime.config.use_credential_manager,
            runtime.config.multi_instance_enabled,
            runtime.config.kill_background_roblox,
            runtime.config.privacy_cleanup_options(),
            runtime
                .config
                .custom_player_paths
                .get(&user_id)
                .cloned()
                .or_else(|| runtime.config.roblox_player_path.clone()),
        )
    };
    let cookie = if use_credential_manager {
        crypto::credential_load(user_id).map_err(|error| error.to_string())?
    } else {
        let encrypted =
            encrypted_cookie.ok_or_else(|| "This account has no stored credential".to_string())?;
        crypto::decrypt_cookie(&encrypted, &session).map_err(|error| error.to_string())?
    };
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    if multi_instance {
        tauri::async_runtime::spawn_blocking(process::enable_multi_instance)
            .await
            .map_err(|_| "Multi-instance setup failed".to_string())?
            .map_err(|error| error.to_string())?;
    }
    if kill_background || multi_instance {
        tauri::async_runtime::spawn_blocking(process::kill_tray_roblox)
            .await
            .map_err(|_| "Roblox tray cleanup failed".to_string())?;
    }
    let ticket = client
        .generate_auth_ticket(&cookie)
        .await
        .map_err(|error| error.to_string())?;
    let cleanup =
        tauri::async_runtime::spawn_blocking(move || process::prepare_privacy_cleanup(privacy))
            .await
            .map_err(|_| "Privacy cleanup failed".to_string())?
            .map_err(|error| error.to_string())?;
    let launch_result = tauri::async_runtime::spawn_blocking(move || {
        process::launch_game(
            &ticket,
            place_id,
            job_id.as_deref(),
            None,
            None,
            data.as_deref(),
            process::next_launchtime(),
            player_path.as_deref(),
        )
    })
    .await
    .map_err(|_| "Roblox launch task failed".to_string())?;
    launch_result.map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || cleanup.commit())
        .await
        .map_err(|_| "Privacy cleanup task failed".to_string())?
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn fetch_account_inventory(
    state: tauri::State<'_, AppState>,
    user_id: u64,
) -> Result<Vec<InventoryItem>, String> {
    let (cookie, client) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        (
            account_cookie(&runtime, user_id)?,
            RobloxClient::new().map_err(|error| error.to_string())?,
        )
    };
    let mut items = Vec::new();
    for asset_type in assets_api::USER_INVENTORY_ASSET_TYPES {
        let mut fetched = assets_api::list_user_inventory(&client, &cookie, user_id, asset_type)
            .await
            .map_err(|error| error.to_string())?;
        items.append(&mut fetched);
    }
    items.sort_by_key(|item| item.asset_id);
    items.dedup_by_key(|item| item.asset_id);
    Ok(items
        .into_iter()
        .map(|item| InventoryItem {
            asset_id: item.asset_id,
            name: item.name,
            asset_type: item.asset_type,
        })
        .collect())
}

#[tauri::command]
async fn search_connection_users(keyword: String) -> Result<Vec<api::UserSearchResult>, String> {
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    api::search_users(&client, &keyword)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn refresh_account_presence(
    state: tauri::State<'_, AppState>,
    user_ids: Vec<u64>,
) -> Result<Vec<PresenceUpdate>, String> {
    let (cookie, account_ids) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let first_id = user_ids
            .first()
            .copied()
            .or_else(|| {
                runtime
                    .accounts
                    .accounts
                    .first()
                    .map(|account| account.user_id)
            })
            .ok_or_else(|| "No accounts are available".to_string())?;
        (account_cookie(&runtime, first_id)?, user_ids)
    };
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let presences = api::fetch_presences(&client, &cookie, &account_ids)
        .await
        .map_err(|error| error.to_string())?;
    let updates = presences
        .iter()
        .map(|(user_id, presence)| PresenceUpdate {
            user_id: *user_id,
            presence: presence_kind(presence),
            presence_text: presence.status_text().to_string(),
            location: presence.last_location.clone(),
        })
        .collect::<Vec<_>>();
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    for (user_id, presence) in presences {
        if let Some(account) = runtime.accounts.find_by_id_mut(user_id) {
            account.last_presence = presence;
        }
    }
    save_runtime(&runtime)?;
    Ok(updates)
}

#[tauri::command]
async fn revalidate_accounts(
    state: tauri::State<'_, AppState>,
    user_ids: Vec<u64>,
) -> Result<Vec<AccountSummary>, String> {
    let ids = if user_ids.is_empty() {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        runtime
            .accounts
            .accounts
            .iter()
            .map(|account| account.user_id)
            .collect()
    } else {
        user_ids
    };
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let mut results = Vec::new();
    for user_id in ids {
        let cookie = {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable".to_string())?;
            account_cookie(&runtime, user_id)?
        };
        let validation = client.validate_cookie(&cookie).await;
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let player_path = configured_player_path(&runtime, user_id);
        if let Some(account) = runtime.accounts.find_by_id_mut(user_id) {
            account.cookie_expired = validation.is_err();
            if let Ok((validated_id, username, display_name)) = validation {
                if validated_id == user_id {
                    account.username = username;
                    account.display_name = display_name;
                    account.last_validated = Some(chrono::Utc::now());
                }
            }
            results.push(account_summary(account, player_path));
        }
    }
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    save_runtime(&runtime)?;
    Ok(results)
}

#[tauri::command]
async fn kill_all_accounts() -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(process::kill_all_roblox)
        .await
        .map_err(|_| "Roblox termination task failed".to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn arrange_account_windows(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let options = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        runtime.config.tiling_options()
    };
    tauri::async_runtime::spawn_blocking(move || process::arrange_roblox_windows(&options))
        .await
        .map_err(|_| "Window arrangement task failed".to_string())?;
    Ok(())
}

#[tauri::command]
async fn connection_action(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    target_user_id: u64,
    action: String,
) -> Result<(), String> {
    let cookie = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        account_cookie(&runtime, user_id)?
    };
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    match action.as_str() {
        "follow" => api::follow_user(&client, &cookie, target_user_id).await,
        "unfollow" => api::unfollow_user(&client, &cookie, target_user_id).await,
        "friend" => api::send_friend_request(&client, &cookie, target_user_id).await,
        "block" => api::block_user(&client, &cookie, target_user_id).await,
        _ => return Err("Unsupported connection action".to_string()),
    }
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn join_user_game(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    target_user_id: u64,
) -> Result<(), String> {
    let (cookie, client, options) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let cookie = account_cookie(&runtime, user_id)?;
        let options = (
            runtime.config.multi_instance_enabled,
            runtime.config.kill_background_roblox,
            runtime.config.privacy_cleanup_options(),
            runtime
                .config
                .custom_player_paths
                .get(&user_id)
                .cloned()
                .or_else(|| runtime.config.roblox_player_path.clone()),
        );
        (
            cookie,
            RobloxClient::new().map_err(|error| error.to_string())?,
            options,
        )
    };
    let presence = api::fetch_presences(&client, &cookie, &[target_user_id])
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .map(|(_, presence)| presence)
        .ok_or_else(|| "Target user is not reporting a live presence right now".to_string())?;
    let place_id = presence
        .place_id
        .ok_or_else(|| "Player is not in a game currently".to_string())?;
    let job_id = presence.game_id;
    if options.0 {
        tauri::async_runtime::spawn_blocking(process::enable_multi_instance)
            .await
            .map_err(|_| "Multi-instance setup failed".to_string())?
            .map_err(|error| error.to_string())?;
    }
    if options.1 || options.0 {
        tauri::async_runtime::spawn_blocking(process::kill_tray_roblox)
            .await
            .map_err(|_| "Roblox tray cleanup failed".to_string())?;
    }
    let ticket = client
        .generate_auth_ticket(&cookie)
        .await
        .map_err(|error| error.to_string())?;
    let cleanup =
        tauri::async_runtime::spawn_blocking(move || process::prepare_privacy_cleanup(options.2))
            .await
            .map_err(|_| "Privacy cleanup failed".to_string())?
            .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        process::launch_game(
            &ticket,
            place_id,
            job_id.as_deref(),
            None,
            None,
            None,
            process::next_launchtime(),
            options.3.as_deref(),
        )
    })
    .await
    .map_err(|_| "Join launch task failed".to_string())?
    .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || cleanup.commit())
        .await
        .map_err(|_| "Privacy cleanup task failed".to_string())?
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn add_account(
    state: tauri::State<'_, AppState>,
    cookie: String,
) -> Result<AccountSummary, String> {
    if cookie.trim().is_empty() {
        return Err("Enter a Roblox security cookie".to_string());
    }
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let (user_id, username, display_name) = client
        .validate_cookie(cookie.trim())
        .await
        .map_err(|_| "Roblox rejected this account credential".to_string())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    add_account_with_cookie(&mut runtime, cookie.trim(), user_id, username, display_name)
}

#[tauri::command]
async fn add_account_anyway(
    state: tauri::State<'_, AppState>,
    cookie: String,
    username: String,
) -> Result<AccountSummary, String> {
    if cookie.trim().is_empty() || username.trim().is_empty() {
        return Err("Enter both the cookie and Roblox username".to_string());
    }
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let candidates = api::search_users(&client, username.trim())
        .await
        .map_err(|_| "Roblox username lookup failed".to_string())?;
    let candidate = candidates
        .into_iter()
        .find(|user| user.username.eq_ignore_ascii_case(username.trim()))
        .ok_or_else(|| "That Roblox username could not be found".to_string())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    add_account_with_cookie(
        &mut runtime,
        cookie.trim(),
        candidate.user_id,
        candidate.username,
        candidate.display_name,
    )
}

#[tauri::command]
async fn login_and_add_account(
    state: tauri::State<'_, AppState>,
) -> Result<Option<AccountSummary>, String> {
    let profile_dir = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("RM")
        .join("webview_profile");
    let cookie = tauri::async_runtime::spawn_blocking(move || {
        let _ = std::fs::remove_dir_all(&profile_dir);
        browser_login::login_blocking(profile_dir)
    })
    .await
    .map_err(|_| "Browser login task failed".to_string())??;
    let Some(cookie) = cookie else {
        return Ok(None);
    };
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let (user_id, username, display_name) = client
        .validate_cookie(&cookie)
        .await
        .map_err(|_| "Roblox rejected this account credential".to_string())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    add_account_with_cookie(&mut runtime, &cookie, user_id, username, display_name).map(Some)
}

#[tauri::command]
async fn browse_as_account(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    inventory: bool,
) -> Result<(), String> {
    let (cookie, label) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let account = runtime
            .accounts
            .find_by_id(user_id)
            .ok_or_else(|| "Account not found".to_string())?;
        (
            account_cookie(&runtime, user_id)?,
            account.label().to_string(),
        )
    };
    let profile_dir = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("RM")
        .join("webview_browse_as")
        .join(user_id.to_string());
    let destination = if inventory {
        Some(format!("https://www.roblox.com/users/{user_id}/inventory"))
    } else {
        None
    };
    tauri::async_runtime::spawn_blocking(move || {
        browser_login::spawn_browse_as_to(profile_dir, cookie, label, destination)
    })
    .await
    .map_err(|_| "Browser launch task failed".to_string())??;
    Ok(())
}

#[tauri::command]
fn save_launch_preset(
    name: String,
    place_id: u64,
    job_id: Option<String>,
    data: Option<String>,
) -> Result<(), String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() || trimmed_name.chars().count() > 80 {
        return Err("Preset name must be between 1 and 80 characters".to_string());
    }
    let data_dir = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("RM");
    let preset = LaunchPreset {
        name: trimmed_name.to_string(),
        place_id,
        job_id: job_id.filter(|value| !value.trim().is_empty()),
        data: data.filter(|value| !value.trim().is_empty()),
    };
    ram_core::presets::save(&data_dir, &preset, None).map_err(|error| error.to_string())?;
    Ok(())
}

fn presence_kind(presence: &Presence) -> &'static str {
    match presence.user_presence_type {
        1..=3 => "online",
        _ => "neutral",
    }
}

#[tauri::command]
fn list_account_group_colors(
    state: tauri::State<'_, AppState>,
) -> Result<std::collections::HashMap<String, [u8; 3]>, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    Ok(runtime
        .config
        .groups
        .iter()
        .map(|(name, meta)| (name.clone(), meta.color))
        .collect())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == browser_login::FLAG {
        let code = browser_login::run_child(
            std::path::PathBuf::from(&args[2]),
            std::path::PathBuf::from(&args[3]),
        );
        std::process::exit(code);
    }
    if args.len() >= 4 && args[1] == browser_login::BROWSE_AS_FLAG {
        let profile_dir = std::path::PathBuf::from(&args[2]);
        let cookie_in = std::path::PathBuf::from(&args[3]);
        let label = args.get(4).cloned().unwrap_or_default();
        let code = if let Some(destination_url) = args.get(5) {
            browser_login::run_browse_as_child_to(
                profile_dir,
                cookie_in,
                label,
                destination_url.clone(),
            )
        } else {
            browser_login::run_browse_as_child(profile_dir, cookie_in, label)
        };
        std::process::exit(code);
    }
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_accounts,
            store_status,
            create_device_store,
            unlock_device,
            unlock_password,
            update_account_alias,
            toggle_account_pin,
            update_account_group,
            reorder_accounts,
            update_player_path,
            create_account_group,
            delete_account_group,
            open_account_url,
            remove_account,
            launch_account,
            fetch_account_inventory,
            search_connection_users,
            refresh_account_presence,
            revalidate_accounts,
            kill_all_accounts,
            arrange_account_windows,
            connection_action,
            join_user_game,
            add_account,
            add_account_anyway,
            login_and_add_account,
            browse_as_account,
            save_launch_preset,
            list_account_group_colors
        ])
        .run(tauri::generate_context!())
        .expect("error while running RM");
}
