use crate::{
    account_cookie, account_summary, configured_player_path, save_runtime, state::AppState,
    AccountSummary,
};
use ram_core::{
    api,
    auth::RobloxClient,
    crypto,
    error::CoreError,
    models::{Account, ModerationInfo},
};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

pub struct PendingAddition {
    account: Account,
    created_at: Instant,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionOutcome {
    account: AccountSummary,
    confirmation_id: Option<String>,
    is_replacement: bool,
}

fn credential_was_rejected(error: &CoreError) -> bool {
    // forbidden responses can request a challenge without invalidating the session
    matches!(error, CoreError::RobloxApi { status: 401, .. })
}

fn apply_validation(
    account: &mut Account,
    validation: Result<(u64, String, String), CoreError>,
) -> bool {
    match validation {
        Ok((user_id, username, display_name)) if user_id == account.user_id => {
            account.username = username;
            account.display_name = display_name;
            account.cookie_expired = false;
            account.last_validated = Some(chrono::Utc::now());
        }
        Ok(_) => account.cookie_expired = true,
        Err(ref error) if credential_was_rejected(error) => account.cookie_expired = true,
        Err(_) => return false,
    }
    true
}

fn merge_moderation(
    previous: Option<ModerationInfo>,
    current: Option<ModerationInfo>,
) -> Option<ModerationInfo> {
    current.map(|mut current| {
        if let Some(previous) = previous {
            if current.reason.is_none() {
                current.reason = previous.reason;
            }
            if current.expires_at.is_none() {
                current.expires_at = previous.expires_at;
            }
        }
        current
    })
}

pub fn summaries(state: &AppState) -> Result<Vec<AccountSummary>, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?;
    Ok(runtime
        .accounts
        .accounts
        .iter()
        .map(|account| {
            account_summary(
                account,
                configured_player_path(&runtime, account.user_id),
                &runtime.config,
            )
        })
        .collect())
}

pub fn publish(app: &tauri::AppHandle) {
    if let Ok(accounts) = summaries(&app.state::<AppState>()) {
        let _ = app.emit("accounts-updated", &accounts);
    }
}

pub async fn stage_addition(
    app: &tauri::AppHandle,
    cookie: &str,
    mut account: Account,
) -> Result<AdditionOutcome, String> {
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let user_ids = [account.user_id];
    let (created, moderation, avatars) = tokio::join!(
        api::fetch_public_created_at(&client, account.user_id),
        api::fetch_moderation_status(&client, account.user_id, cookie),
        api::fetch_avatars(&client, &user_ids),
    );
    account.created_at = created.ok().flatten();
    account.moderation = moderation.ok().flatten();
    account.avatar_url = avatars
        .ok()
        .and_then(|values| {
            values
                .into_iter()
                .find(|(user_id, _)| *user_id == account.user_id)
                .map(|(_, url)| url)
        })
        .unwrap_or_default();
    let state = app.state::<AppState>().inner().clone();
    let cookie = cookie.to_owned();
    let outcome =
        tauri::async_runtime::spawn_blocking(move || -> Result<AdditionOutcome, String> {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            if !runtime.unlocked {
                return Err("Unlock the account store before adding accounts".into());
            }
            let session = runtime.session.as_ref().ok_or("Account store is locked")?;
            account.encrypted_cookie = Some(
                crypto::encrypt_cookie(&cookie, session)
                    .map_err(|_| "Account credential could not be secured")?,
            );
            let existing = runtime.accounts.find_by_id(account.user_id);
            let is_replacement = existing.is_some();
            if let Some(existing) = existing {
                account = merge_replacement(existing, account);
            }
            let requires_confirmation = account
                .moderation
                .as_ref()
                .is_some_and(ModerationInfo::is_active);
            let summary = account_summary(
                &account,
                configured_player_path(&runtime, account.user_id),
                &runtime.config,
            );
            drop(runtime);
            let confirmation_id = uuid::Uuid::new_v4().to_string();
            {
                let mut additions = state
                    .pending_additions
                    .lock()
                    .map_err(|_| "Account addition unavailable")?;
                additions
                    .retain(|_, addition| addition.created_at.elapsed() < Duration::from_secs(600));
                additions.insert(
                    confirmation_id.clone(),
                    PendingAddition {
                        account,
                        created_at: Instant::now(),
                    },
                );
            }
            if requires_confirmation {
                Ok(AdditionOutcome {
                    account: summary,
                    confirmation_id: Some(confirmation_id),
                    is_replacement,
                })
            } else {
                let account = commit_addition(&state, &confirmation_id)?;
                Ok(AdditionOutcome {
                    account,
                    confirmation_id: None,
                    is_replacement,
                })
            }
        })
        .await
        .map_err(|_| "Account addition task failed")??;
    publish(app);
    Ok(outcome)
}

fn merge_replacement(existing: &Account, account: Account) -> Account {
    let mut updated = existing.clone();
    updated.username = account.username;
    updated.display_name = account.display_name;
    updated.encrypted_cookie = account.encrypted_cookie;
    updated.cookie_expired = account.cookie_expired;
    updated.last_validated = account.last_validated;
    updated.moderation = account.moderation;
    if account.created_at.is_some() {
        updated.created_at = account.created_at;
    }
    if !account.avatar_url.is_empty() {
        updated.avatar_url = account.avatar_url;
    }
    updated
}

fn commit_addition(state: &AppState, confirmation_id: &str) -> Result<AccountSummary, String> {
    let mut additions = state
        .pending_additions
        .lock()
        .map_err(|_| "Account addition unavailable")?;
    let pending = additions
        .get(confirmation_id)
        .ok_or("Account addition expired. Try adding it again.")?;
    if pending.created_at.elapsed() >= Duration::from_secs(600) {
        additions.remove(confirmation_id);
        return Err("Account addition expired. Try adding it again.".into());
    }
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?;
    let session = runtime.session.as_ref().ok_or("Account store is locked")?;
    let cookie = crypto::decrypt_cookie(
        pending
            .account
            .encrypted_cookie
            .as_deref()
            .ok_or("Account credential unavailable")?,
        session,
    )
    .map_err(|_| "Account credential could not be opened")?;
    let mut account = pending.account.clone();
    if let Some(existing) = runtime.accounts.find_by_id(account.user_id) {
        account = merge_replacement(existing, account);
    }
    let user_id = account.user_id;
    let previous = runtime.accounts.clone();
    let previous_credential = if runtime.config.use_credential_manager {
        let value = if runtime.accounts.find_by_id(user_id).is_some() {
            Some(
                crypto::credential_load(user_id)
                    .map_err(|_| "Existing credential could not be backed up")?,
            )
        } else {
            None
        };
        crypto::credential_store(user_id, &cookie)
            .map_err(|_| "Account credential could not be saved")?;
        account.encrypted_cookie = None;
        value
    } else {
        None
    };
    if let Some(existing) = runtime.accounts.find_by_id_mut(user_id) {
        *existing = account;
    } else {
        runtime.accounts.accounts.push(account);
    }
    if save_runtime(&runtime).is_err() {
        runtime.accounts = previous;
        if runtime.config.use_credential_manager {
            let restored = if let Some(cookie) = previous_credential {
                crypto::credential_store(user_id, &cookie)
            } else {
                crypto::credential_delete(user_id)
            };
            if restored.is_err() {
                return Err("Account save failed and credential recovery failed. Re-add this account before launching.".into());
            }
        }
        return Err("The account store could not be saved. Your previous account was kept.".into());
    }
    *runtime.credential_revisions.entry(user_id).or_default() += 1;
    additions.remove(confirmation_id);
    let account = runtime
        .accounts
        .find_by_id(user_id)
        .ok_or("Account unavailable")?;
    Ok(account_summary(
        account,
        configured_player_path(&runtime, user_id),
        &runtime.config,
    ))
}

#[tauri::command]
pub async fn confirm_account_addition(
    app: tauri::AppHandle,
    confirmation_id: String,
) -> Result<AccountSummary, String> {
    let state = app.state::<AppState>().inner().clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || commit_addition(&state, &confirmation_id))
            .await
            .map_err(|_| "Account addition task failed")??;
    publish(&app);
    Ok(result)
}

#[tauri::command]
pub fn cancel_account_addition(
    state: tauri::State<'_, AppState>,
    confirmation_id: String,
) -> Result<(), String> {
    state
        .pending_additions
        .lock()
        .map_err(|_| "Account addition unavailable")?
        .remove(&confirmation_id);
    Ok(())
}

pub async fn refresh(
    app: &tauri::AppHandle,
    user_ids: Vec<u64>,
) -> Result<Vec<AccountSummary>, String> {
    let state = app.state::<AppState>().inner().clone();
    let _refresh = state.account_refresh.lock().await;
    let ids = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked {
            return Err("Account store is locked".into());
        }
        runtime
            .accounts
            .accounts
            .iter()
            .filter(|account| user_ids.is_empty() || user_ids.contains(&account.user_id))
            .map(|account| account.user_id)
            .collect::<Vec<_>>()
    };
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let mut completed = 0;
    for user_id in &ids {
        let user_id = *user_id;
        let credential_state = state.clone();
        let credential = tauri::async_runtime::spawn_blocking(move || {
            let runtime = credential_state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            let cookie = account_cookie(&runtime, user_id)?;
            Ok::<_, String>((
                cookie,
                *runtime.credential_revisions.get(&user_id).unwrap_or(&0),
            ))
        })
        .await
        .map_err(|_| "Account credential task failed")?;
        let Ok((cookie, revision)) = credential else {
            continue;
        };
        let validation = client.validate_cookie(&cookie).await;
        let user_ids = [user_id];
        let (created, moderation, avatars) = tokio::join!(
            api::fetch_public_created_at(&client, user_id),
            api::fetch_moderation_status(&client, user_id, &cookie),
            api::fetch_avatars(&client, &user_ids)
        );
        let state = state.clone();
        let did_complete = tauri::async_runtime::spawn_blocking(move || {
            let mut runtime = state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            if *runtime.credential_revisions.get(&user_id).unwrap_or(&0) != revision {
                return Ok(false);
            }
            let previous = runtime.accounts.clone();
            let Some(account) = runtime.accounts.find_by_id_mut(user_id) else {
                return Ok(false);
            };
            if !apply_validation(account, validation) {
                return Ok(false);
            }
            if let Ok(Some(created)) = created {
                account.created_at = Some(created);
            }
            if let Ok(moderation) = moderation {
                account.moderation = merge_moderation(account.moderation.take(), moderation);
            }
            if let Ok(avatars) = avatars {
                if let Some((_, url)) = avatars.into_iter().find(|(id, _)| *id == user_id) {
                    account.avatar_url = url;
                }
            }
            if save_runtime(&runtime).is_err() {
                runtime.accounts = previous;
                return Err("Account refresh could not be saved".to_string());
            }
            Ok(true)
        })
        .await
        .map_err(|_| "Account refresh task failed")??;
        if did_complete {
            completed += 1;
            publish(app);
        }
    }
    if completed == 0 {
        return Err(
            "Accounts could not be verified. Your previous credential status was kept. Retry later, or open the account browser to check for a Roblox challenge.".into(),
        );
    }
    let _ = crate::background::refresh_presence(app, ids.clone()).await;
    if completed < ids.len() {
        return Err("Some accounts could not be refreshed. Completed updates were kept; check your connection and retry.".into());
    }
    let mut accounts = summaries(&state)?;
    accounts.retain(|account| ids.contains(&account.user_id));
    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committing_readdition_preserves_organisation_after_reopening_store() {
        use crate::state::RuntimeState;
        use ram_core::models::{AccountStore, AppConfig};
        use std::sync::{Arc, Mutex};

        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("accounts.dat");
        let session = crypto::create_password_session("synthetic-test-password").unwrap();
        let mut existing = Account::new(1, "old".into(), "Old".into());
        existing.alias = "Main".into();
        existing.group = "Farm".into();
        existing.is_pinned = true;
        existing.sort_order = 3;
        existing.friends_cache.insert(2);
        let mut replacement = Account::new(1, "new".into(), "New".into());
        replacement.encrypted_cookie =
            Some(crypto::encrypt_cookie("synthetic-replacement", &session).unwrap());
        let state = AppState {
            runtime: Arc::new(Mutex::new(RuntimeState {
                accounts: AccountStore {
                    accounts: vec![existing],
                },
                config: AppConfig {
                    accounts_path: path.clone(),
                    use_credential_manager: false,
                    ..AppConfig::default()
                },
                config_path: directory.join("config.json"),
                session: Some(session),
                unlocked: true,
                legacy_store: false,
                is_first_install: false,
                credential_revisions: Default::default(),
            })),
            account_refresh: Arc::new(tokio::sync::Mutex::new(())),
            pending_additions: Arc::new(Mutex::new(Default::default())),
            instances: Arc::new(Mutex::new(Default::default())),
            launch_queue: Arc::new(tokio::sync::Mutex::new(None)),
            is_shutting_down: Arc::new(Default::default()),
        };
        for identifier in ["first", "retry"] {
            state.pending_additions.lock().unwrap().insert(
                identifier.into(),
                PendingAddition {
                    account: replacement.clone(),
                    created_at: Instant::now(),
                },
            );
            let summary = commit_addition(&state, identifier).unwrap();
            assert_eq!(summary.alias, "Main");
            assert_eq!(summary.group, "Farm");
            assert!(summary.is_pinned);
            assert_eq!(summary.sort_order, 3);
        }
        let (store, session) =
            crypto::unlock_with_password(&path, "synthetic-test-password").unwrap();
        assert_eq!(store.accounts.len(), 1);
        let account = &store.accounts[0];
        assert_eq!(account.alias, "Main");
        assert_eq!(account.group, "Farm");
        assert!(account.is_pinned);
        assert_eq!(account.sort_order, 3);
        assert!(account.friends_cache.contains(&2));
        assert_eq!(account.username, "new");
        assert_eq!(
            crypto::decrypt_cookie(account.encrypted_cookie.as_deref().unwrap(), &session).unwrap(),
            "synthetic-replacement"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn replacing_credentials_keeps_account_organisation() {
        let mut existing = Account::new(1, "old".into(), "Old".into());
        existing.alias = "Main".into();
        existing.group = "Farm".into();
        existing.is_pinned = true;
        existing.sort_order = 3;
        existing.last_used = Some(chrono::Utc::now());
        let updated = merge_replacement(&existing, Account::new(1, "new".into(), "New".into()));
        assert_eq!(updated.username, "new");
        assert_eq!(updated.alias, existing.alias);
        assert_eq!(updated.group, existing.group);
        assert!(updated.is_pinned);
        assert_eq!(updated.sort_order, 3);
        assert_eq!(updated.last_used, existing.last_used);
    }

    #[test]
    fn transient_errors_do_not_expire_credentials() {
        assert!(!credential_was_rejected(&CoreError::RateLimited));
        assert!(!credential_was_rejected(&CoreError::RobloxApi {
            status: 503,
            message: String::new()
        }));
        assert!(credential_was_rejected(&CoreError::RobloxApi {
            status: 401,
            message: String::new()
        }));
    }

    #[test]
    fn forbidden_and_csrf_failures_do_not_expire_credentials() {
        let failures = [
            CoreError::CookieRejected,
            CoreError::CookieRejectedWithReason("Synthetic challenge required".into()),
            CoreError::AuthFailed("403 Forbidden after CSRF retries".into()),
            CoreError::RobloxApi {
                status: 403,
                message: String::new(),
            },
        ];
        for failure in failures {
            assert!(!credential_was_rejected(&failure));
        }
    }

    #[test]
    fn validation_changes_credential_status_only_with_an_authentication_verdict() {
        let mut account = Account::new(1, "Original".into(), "Original".into());
        assert!(!apply_validation(
            &mut account,
            Err(CoreError::CookieRejected)
        ));
        assert!(!account.cookie_expired);
        assert!(account.last_validated.is_none());
        assert!(apply_validation(
            &mut account,
            Err(CoreError::RobloxApi {
                status: 401,
                message: String::new(),
            })
        ));
        assert!(account.cookie_expired);
        assert!(!apply_validation(
            &mut account,
            Err(CoreError::CookieRejected)
        ));
        assert!(account.cookie_expired);
        assert!(apply_validation(
            &mut account,
            Ok((1, "Updated".into(), "Updated name".into()))
        ));
        assert!(!account.cookie_expired);
        assert_eq!(account.username, "Updated");
        assert!(account.last_validated.is_some());
        assert!(apply_validation(
            &mut account,
            Ok((2, "Other user".into(), "Other".into()))
        ));
        assert!(account.cookie_expired);
        assert_eq!(account.user_id, 1);
        assert_eq!(account.username, "Updated");
    }
}
