use crate::{account_cookie, state::AppState};
use ram_core::{api, auth::RobloxClient};
use serde::Serialize;

#[derive(Serialize)]
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

#[tauri::command]
pub async fn check_private_server(
    state: tauri::State<'_, AppState>,
    index: usize,
) -> Result<ServerCheck, String> {
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        check_private_server_inner(state.inner().clone(), index),
    )
    .await
    .map_err(|_| {
        "This server check timed out. Wait before refreshing status to retry".to_string()
    })?
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
