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
) -> &'static str {
    match check.owner_id {
        Some(_) if managed_owner => "owned",
        Some(_) => "not-owned",
        None if check.status == "valid" && complete && checked > 0 => "not-owned",
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

#[tauri::command]
pub async fn check_private_server(
    state: tauri::State<'_, AppState>,
    index: usize,
) -> Result<ServerCheck, String> {
    let state = state.inner().clone();
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
    let mut check = api::PrivateServerCheck::default();
    let mut failed = accounts.is_empty();
    let mut complete = !accounts.is_empty();
    let mut checked = 0;
    let mut failure_message = "No working account is available. Refresh accounts and retry";
    for (user_id, revision) in &accounts {
        let credential_state = state.clone();
        let user_id = *user_id;
        let revision = *revision;
        let cookie = tauri::async_runtime::spawn_blocking(move || {
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
        .map_err(|_| "Private server credential task failed")??;
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
        let result = if let Some(result) = owned_match {
            Ok(result)
        } else if check.status == "valid" {
            Ok(check.clone())
        } else {
            api::check_private_server(&client, &cookie, server.place_id, &server.link_code).await
        };
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
                if result.status == "valid" || check.status != "valid" {
                    check = result;
                }
                if check.owner_id.is_some() {
                    break;
                }
            }
            Err(error) => {
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
    let ownership = ownership_status(&check, managed_owner, complete, checked);
    let message = match check.status {
        "valid" if check.owner_id.is_none() && ownership == "not-owned" => format!("Link resolved. Not owned by any of the {checked} checked accounts. Owner profile unavailable for this shared or unlisted server"),
        "valid" if check.owner_id.is_none() => format!("Link resolved. {failure_message}"),
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
    fn non_ownership_needs_a_resolved_link_and_complete_account_checks() {
        let valid = api::PrivateServerCheck {
            status: "valid",
            ..Default::default()
        };
        assert_eq!(ownership_status(&valid, false, true, 2), "not-owned");
        assert_eq!(ownership_status(&valid, false, false, 2), "unknown");
        assert_eq!(ownership_status(&valid, false, true, 0), "unknown");
        assert_eq!(
            ownership_status(&api::PrivateServerCheck::default(), false, true, 2),
            "unknown"
        );
        let known = api::PrivateServerCheck {
            owner_id: Some(1),
            ..valid
        };
        assert_eq!(ownership_status(&known, true, false, 0), "owned");
        assert_eq!(ownership_status(&known, false, false, 0), "not-owned");
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
