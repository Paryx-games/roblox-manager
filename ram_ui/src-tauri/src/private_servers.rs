use crate::{account_cookie, state::AppState};
use ram_core::{api, auth::RobloxClient};
use serde::Serialize;

#[derive(Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerCheck {
    #[serde(flatten)]
    check: api::PrivateServerCheck,
    ownership: &'static str,
    message: String,
}

fn ownership_status(
    check: &api::PrivateServerCheck,
    managed_owner: bool,
    complete: bool,
    checked: usize,
    saved: usize,
) -> &'static str {
    match check.owner_id {
        Some(_) if managed_owner => "owned",
        Some(_) => "not-owned",
        None if check.status == "valid" && complete && checked > 0 && checked == saved => {
            "not-owned"
        }
        None => "unknown",
    }
}

fn lookup_failure(error: &ram_core::CoreError) -> &'static str {
    match error {
        ram_core::CoreError::RateLimited => {
            "Roblox rate-limited the ownership check. Wait and refresh status"
        }
        ram_core::CoreError::CookieRejected
        | ram_core::CoreError::CookieRejectedWithReason(_)
        | ram_core::CoreError::RobloxApi { status: 401, .. } => {
            "An account could not authenticate for the ownership check. Refresh accounts and retry"
        }
        _ => "The ownership check could not finish. Refresh status to retry",
    }
}

fn merge_check(current: &mut api::PrivateServerCheck, result: api::PrivateServerCheck) {
    if result.owner_id.is_some() || result.status == "valid" || current.status != "valid" {
        *current = result;
    }
}

#[derive(Default)]
pub struct CheckCache {
    entries: std::sync::Mutex<std::collections::HashMap<usize, CachedCheck>>,
    requested: std::sync::Mutex<std::collections::HashSet<usize>>,
    notify: tokio::sync::Notify,
}

impl CheckCache {
    pub fn clear(&self) -> usize {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let count = entries.len();
        entries.clear();
        self.requested
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.notify.notify_one();
        count
    }
}

struct CachedCheck {
    identity: CheckIdentity,
    result: Option<ServerCheck>,
    checked_at: Option<std::time::Instant>,
    checking: bool,
    failed: bool,
}

#[derive(Clone, PartialEq)]
struct CheckIdentity {
    server: ram_core::models::PrivateServer,
    accounts: Vec<(u64, u64, bool)>,
}

fn identities(state: &AppState) -> Vec<CheckIdentity> {
    let Ok(runtime) = state.runtime.lock() else {
        return Vec::new();
    };
    if !runtime.unlocked {
        return Vec::new();
    }
    let accounts: Vec<_> = runtime
        .accounts
        .accounts
        .iter()
        .map(|account| {
            (
                account.user_id,
                *runtime
                    .credential_revisions
                    .get(&account.user_id)
                    .unwrap_or(&0),
                account.cookie_expired,
            )
        })
        .collect();
    runtime
        .config
        .private_servers
        .iter()
        .map(|server| CheckIdentity {
            server: server.clone(),
            accounts: accounts.clone(),
        })
        .collect()
}

#[derive(Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckSnapshot {
    failed: bool,
    checks: std::collections::HashMap<usize, ServerCheck>,
    checking: bool,
}

fn cache_snapshot(service: &CheckCache, current: &[CheckIdentity]) -> CheckSnapshot {
    let mut cache = service
        .entries
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    cache.retain(|index, entry| current.get(*index) == Some(&entry.identity));
    let queued = service
        .requested
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .any(|index| *index < current.len());
    CheckSnapshot {
        failed: cache.values().any(|entry| entry.failed),
        checks: cache
            .iter()
            .filter_map(|(index, entry)| entry.result.clone().map(|result| (*index, result)))
            .collect(),
        checking: queued
            || cache.values().any(|entry| entry.checking)
            || current
                .iter()
                .enumerate()
                .any(|(index, _)| !cache.contains_key(&index)),
    }
}

fn snapshot(state: &AppState) -> CheckSnapshot {
    cache_snapshot(&state.private_server_checks, &identities(state))
}

#[tauri::command]
pub fn cached_private_server_checks(state: tauri::State<'_, AppState>) -> CheckSnapshot {
    snapshot(state.inner())
}

#[tauri::command]
pub fn check_private_server(
    state: tauri::State<'_, AppState>,
    index: usize,
) -> Result<ServerCheck, String> {
    if identities(state.inner()).get(index).is_none() {
        return Err("Private server not found or accounts locked".into());
    }
    state
        .private_server_checks
        .requested
        .lock()
        .map_err(|_| "Server checks unavailable")?
        .insert(index);
    state.private_server_checks.notify.notify_one();
    Ok(snapshot(state.inner())
        .checks
        .remove(&index)
        .unwrap_or_else(|| ServerCheck {
            check: api::PrivateServerCheck::default(),
            ownership: "unknown",
            message: "Checking...".into(),
        }))
}

fn publish(app: &tauri::AppHandle, state: &AppState, previous: &mut Option<CheckSnapshot>) {
    use tauri::Emitter;
    let next = snapshot(state);
    if previous.as_ref() != Some(&next) {
        let _ = app.emit("private-server-checks-updated", &next);
        *previous = Some(next);
    }
}

pub async fn run_background(app: tauri::AppHandle) {
    use tauri::Manager;
    let state = app.state::<AppState>().inner().clone();
    let mut previous = None;
    loop {
        let current = identities(&state);
        for (index, identity) in current.iter().enumerate() {
            if identities(&state).get(index) != Some(identity) {
                break;
            }
            let requested = state
                .private_server_checks
                .requested
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&index);
            let due = {
                let mut cache = state
                    .private_server_checks
                    .entries
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                cache.retain(|key, entry| current.get(*key) == Some(&entry.identity));
                let entry = cache.entry(index).or_insert_with(|| CachedCheck {
                    identity: identity.clone(),
                    result: None,
                    checked_at: None,
                    checking: false,
                    failed: false,
                });
                let due = requested
                    || entry
                        .checked_at
                        .is_none_or(|time| time.elapsed() >= std::time::Duration::from_secs(300));
                entry.checking = due;
                due
            };
            if !due {
                continue;
            }
            publish(&app, &state, &mut previous);
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(60),
                check_private_server_inner(state.clone(), index),
            )
            .await;
            if identities(&state).get(index) != Some(identity) {
                continue;
            }
            {
                let mut cache = state
                    .private_server_checks
                    .entries
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if let Some(entry) = cache.get_mut(&index) {
                    entry.checking = false;
                    entry.checked_at = Some(std::time::Instant::now());
                    match result {
                        Ok(Ok(result)) => {
                            entry.result = Some(result);
                            entry.failed = false;
                        }
                        _ => {
                            entry.failed = true;
                            if entry.result.is_none() {
                                entry.result = Some(ServerCheck {
                                    check: api::PrivateServerCheck::default(),
                                    ownership: "unknown",
                                    message: "Check failed. Refresh to retry.".into(),
                                });
                            }
                        }
                    }
                }
            }
            publish(&app, &state, &mut previous);
        }
        // Prune removed bookmarks and locked stores even when there is no work.
        publish(&app, &state, &mut previous);
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {},
            _ = state.private_server_checks.notify.notified() => {},
        }
    }
}

async fn check_private_server_inner(state: AppState, index: usize) -> Result<ServerCheck, String> {
    let snapshot = state.clone();
    let (server, accounts) = tauri::async_runtime::spawn_blocking(move || {
        let runtime = snapshot
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked {
            return Err("Unlock accounts before checking server access");
        }
        let server = runtime
            .config
            .private_servers
            .get(index)
            .cloned()
            .ok_or("Private server not found")?;
        let accounts: Vec<_> = runtime
            .accounts
            .accounts
            .iter()
            .filter(|account| !account.cookie_expired)
            .map(|account| {
                (
                    account.user_id,
                    *runtime
                        .credential_revisions
                        .get(&account.user_id)
                        .unwrap_or(&0),
                )
            })
            .collect();
        Ok((server, accounts))
    })
    .await
    .map_err(|_| "Private server check task failed")??;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    tracing::debug!(
        event = "private_server_check",
        phase = "start",
        accounts = accounts.len()
    );
    let mut check = api::PrivateServerCheck::default();
    let mut failed = accounts.is_empty();
    let mut complete = !accounts.is_empty();
    let mut checked = 0;
    let mut failure_message = "No working account is available. Refresh accounts and retry";
    let mut resolved_access = None;
    let mut owner_problem = None;
    for (user_id, revision) in &accounts {
        let credential_state = state.clone();
        let user_id = *user_id;
        let revision = *revision;
        let credential = tauri::async_runtime::spawn_blocking(move || {
            let runtime = credential_state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            if *runtime.credential_revisions.get(&user_id).unwrap_or(&0) != revision {
                return Err("Account changed during server check".into());
            }
            account_cookie(&runtime, user_id)
        })
        .await
        .map_err(|_| "Private server credential task failed")?;
        let cookie = match credential {
            Ok(cookie) => cookie,
            Err(_) => {
                complete = false;
                failed = true;
                failure_message =
                    "An account credential is unavailable. Unlock or refresh accounts and retry";
                continue;
            }
        };
        let owned = api::check_owned_private_server(
            &client,
            &cookie,
            user_id,
            server.place_id,
            server.universe_id,
            &server.link_code,
        )
        .await;
        let owned_match = match owned {
            Ok(result) => {
                checked += 1;
                result
            }
            Err(error) => {
                complete = false;
                failed = true;
                failure_message = lookup_failure(&error);
                None
            }
        };
        let result: Result<_, ram_core::CoreError> = async {
            if let Some(result) = owned_match {
                return Ok(result);
            }
            if resolved_access.is_none() {
                resolved_access = Some(
                    api::resolve_private_server_access(
                        &client,
                        &cookie,
                        server.place_id,
                        &server.link_code,
                    )
                    .await?,
                );
            }
            Ok(api::fetch_private_server_owner(
                &client,
                &cookie,
                server.place_id,
                resolved_access.as_deref().unwrap_or_default(),
            )
            .await)
        }
        .await;
        drop(cookie);
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked
            || runtime.config.private_servers.get(index) != Some(&server)
            || runtime
                .accounts
                .find_by_id(user_id)
                .is_none_or(|account| account.cookie_expired)
            || *runtime.credential_revisions.get(&user_id).unwrap_or(&0) != revision
        {
            return Err(
                "Accounts or bookmarks changed. Refresh the list before checking again".into(),
            );
        }
        drop(runtime);
        match result {
            Ok(result) => {
                tracing::debug!(event = "private_server_check", phase = "profile", owner_lookup = ?result.owner_lookup, owner_found = result.owner_id.is_some(), link_status = result.status);
                if matches!(
                    result.owner_lookup,
                    api::PrivateServerOwnerLookup::RateLimited
                        | api::PrivateServerOwnerLookup::Unavailable
                        | api::PrivateServerOwnerLookup::Incomplete
                ) && owner_problem != Some(api::PrivateServerOwnerLookup::RateLimited)
                {
                    owner_problem = Some(result.owner_lookup);
                }
                merge_check(&mut check, result);
                if check.owner_id.is_some() {
                    break;
                }
            }
            Err(error) => {
                if matches!(
                    error,
                    ram_core::CoreError::RobloxApi {
                        status: 400 | 404,
                        ..
                    }
                ) && check.status != "valid"
                {
                    check.status = "invalid";
                }
                failed = true;
                failure_message = lookup_failure(&error);
            }
        }
    }
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?;
    let current_accounts: Vec<_> = runtime
        .accounts
        .accounts
        .iter()
        .filter(|account| !account.cookie_expired)
        .map(|account| {
            (
                account.user_id,
                *runtime
                    .credential_revisions
                    .get(&account.user_id)
                    .unwrap_or(&0),
            )
        })
        .collect();
    if !runtime.unlocked
        || current_accounts != accounts
        || runtime.config.private_servers.get(index) != Some(&server)
    {
        return Err("Accounts or bookmarks changed. Refresh the list before checking again".into());
    }
    let managed_owner = check
        .owner_id
        .is_some_and(|owner| runtime.accounts.find_by_id(owner).is_some());
    let all_saved_accounts_checked = complete && checked == runtime.accounts.accounts.len();
    let ownership = ownership_status(
        &check,
        managed_owner,
        complete,
        checked,
        runtime.accounts.accounts.len(),
    );
    if !all_saved_accounts_checked && !failed {
        failure_message = "Some saved accounts have expired or unavailable logins. Refresh them to finish checking ownership";
    }
    tracing::debug!(
        event = "private_server_check",
        phase = "complete",
        ownership,
        checked_accounts = checked,
        owned_checks_complete = complete,
        all_saved_accounts_checked
    );
    if check.owner_id.is_none() {
        if let Some(problem) = owner_problem {
            check.owner_lookup = problem;
        }
    }
    let profile_message = match check.owner_lookup {
        api::PrivateServerOwnerLookup::RateLimited => "Owner profile lookup was rate-limited. Wait and refresh status",
        api::PrivateServerOwnerLookup::Unavailable => "Owner profile lookup failed. Refresh status to retry",
        api::PrivateServerOwnerLookup::Incomplete => "Owner profile lookup was incomplete. Refresh status to retry",
        api::PrivateServerOwnerLookup::NotListed => "This shared server's owner is not listed in the checked accounts' accessible-server metadata",
        _ => "Owner profile unavailable",
    };
    let message = match check.status {
        "valid" if check.owner_id.is_none() && ownership == "not-owned" => format!("Link resolved. Not owned by any of the {checked} checked accounts. {profile_message}"),
        "valid" if check.owner_id.is_none() => format!("Link resolved. {failure_message}. {profile_message}"),
        "valid" => "Link resolved. Access still depends on the account joining".into(),
        "invalid" => "Roblox rejected this private-server link or it is inactive. Ask the owner for a fresh link".into(),
        _ if failed => failure_message.into(),
        _ => "Roblox did not return enough information to verify this server".into(),
    };
    Ok(ServerCheck {
        check,
        ownership,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn synthetic_identity() -> CheckIdentity {
        CheckIdentity {
            server: ram_core::models::PrivateServer {
                name: "Synthetic".into(),
                place_id: 1,
                universe_id: None,
                link_code: "synthetic-link".into(),
                access_code: String::new(),
                place_name: String::new(),
            },
            accounts: vec![(10, 1, false)],
        }
    }

    #[test]
    fn cached_badges_survive_refresh_and_invalidate_on_identity_changes() {
        let service = CheckCache::default();
        let identity = synthetic_identity();
        service.entries.lock().unwrap().insert(
            0,
            CachedCheck {
                identity: identity.clone(),
                checked_at: Some(std::time::Instant::now()),
                checking: false,
                failed: false,
                result: Some(ServerCheck {
                    check: api::PrivateServerCheck {
                        status: "valid",
                        ..Default::default()
                    },
                    ownership: "not-owned",
                    message: "Synthetic".into(),
                }),
            },
        );
        let cached = cache_snapshot(&service, std::slice::from_ref(&identity));
        assert!(!cached.checking);
        assert_eq!(cached.checks[&0].check.status, "valid");
        assert_eq!(cached.checks[&0].ownership, "not-owned");
        service.requested.lock().unwrap().insert(0);
        let refreshing = cache_snapshot(&service, std::slice::from_ref(&identity));
        assert!(refreshing.checking);
        assert_eq!(refreshing.checks.len(), 1);
        let mut changed = identity.clone();
        changed.accounts[0].1 += 1;
        assert!(cache_snapshot(&service, &[changed]).checks.is_empty());
        service.entries.lock().unwrap().insert(
            0,
            CachedCheck {
                identity: identity.clone(),
                result: None,
                checked_at: None,
                checking: false,
                failed: false,
            },
        );
        let mut edited = identity;
        edited.server.link_code = "another-synthetic-link".into();
        assert!(cache_snapshot(&service, &[edited]).checks.is_empty());
        assert!(!cache_snapshot(&service, &[]).checking);
        assert!(service.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn clearing_checks_drops_results_and_queued_refreshes() {
        let service = CheckCache::default();
        service.entries.lock().unwrap().insert(
            0,
            CachedCheck {
                identity: synthetic_identity(),
                result: None,
                checked_at: None,
                checking: true,
                failed: false,
            },
        );
        service.requested.lock().unwrap().insert(0);
        assert_eq!(service.clear(), 1);
        assert!(service.entries.lock().unwrap().is_empty());
        assert!(service.requested.lock().unwrap().is_empty());
    }

    #[test]
    fn later_owner_metadata_is_retained_even_for_an_inactive_owned_server() {
        let mut current = api::PrivateServerCheck {
            status: "valid",
            ..Default::default()
        };
        merge_check(
            &mut current,
            api::PrivateServerCheck {
                status: "invalid",
                owner_id: Some(10),
                owner_username: Some("SyntheticOwner".into()),
                owner_lookup: api::PrivateServerOwnerLookup::Found,
            },
        );
        assert_eq!(current.owner_id, Some(10));
        assert_eq!(current.owner_username.as_deref(), Some("SyntheticOwner"));
        assert_eq!(current.status, "invalid");
    }
    #[test]
    fn non_ownership_needs_a_resolved_link_and_complete_account_checks() {
        let valid = api::PrivateServerCheck {
            status: "valid",
            ..Default::default()
        };
        assert_eq!(ownership_status(&valid, false, true, 2, 2), "not-owned");
        assert_eq!(ownership_status(&valid, false, true, 2, 3), "unknown");
        assert_eq!(ownership_status(&valid, false, false, 2, 2), "unknown");
        assert_eq!(ownership_status(&valid, false, true, 0, 0), "unknown");
        assert_eq!(
            ownership_status(&api::PrivateServerCheck::default(), false, true, 2, 2),
            "unknown"
        );
        let known = api::PrivateServerCheck {
            owner_id: Some(1),
            ..valid
        };
        assert_eq!(ownership_status(&known, true, false, 0, 3), "owned");
        assert_eq!(ownership_status(&known, false, false, 0, 3), "not-owned");
    }
    #[test]
    fn failed_lookups_do_not_claim_roblox_omitted_owner_data() {
        assert!(lookup_failure(&ram_core::CoreError::RateLimited).contains("rate-limited"));
        assert!(lookup_failure(&ram_core::CoreError::CookieRejected).contains("authenticate"));
        let error = ram_core::CoreError::RobloxApi {
            status: 500,
            message: "synthetic-secret-response".into(),
        };
        assert!(!lookup_failure(&error).contains("synthetic-secret"));
    }
}
