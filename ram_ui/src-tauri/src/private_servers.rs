use crate::{account_cookie, state::AppState};
use ram_core::{api, auth::RobloxClient};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCheck {
    #[serde(flatten)]
    check: api::PrivateServerCheck,
    ownership: &'static str,
    message: &'static str,
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
        let result =
            api::check_private_server(&client, &cookie, server.place_id, &server.link_code).await;
        drop(cookie);
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked
            || runtime.config.private_servers.get(index) != Some(&server)
            || runtime.accounts.find_by_id(user_id).is_none()
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
            Err(_) => failed = true,
        }
    }
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?;
    let ownership = match check.owner_id {
        Some(owner) if runtime.accounts.find_by_id(owner).is_some() => "owned",
        Some(_) => "not-owned",
        None => "unknown",
    };
    let message = match check.status {
        "valid" if check.owner_id.is_none() => {
            "Link resolved; Roblox did not expose the owner to the checked accounts"
        }
        "valid" => "Link resolved. Access still depends on the account joining",
        "invalid" => "Roblox rejected this private-server link. Ask the owner for a fresh link",
        _ if failed => "Could not check this server. Refresh accounts and retry",
        _ => "Roblox did not return enough information to verify this server",
    };
    Ok(ServerCheck {
        check,
        ownership,
        message,
    })
}
