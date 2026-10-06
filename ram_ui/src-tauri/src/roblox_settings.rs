use crate::{account_cookie, accounts, save_runtime, state::AppState};
use ram_core::auth::RobloxClient;
use serde::Serialize;
use tauri::Manager;

pub async fn credential(state: AppState, user_id: u64) -> Result<(String, u64), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        let cookie = account_cookie(&runtime, user_id)
            .map_err(|_| "Unlock or re-add this account before changing Roblox settings")?;
        Ok((
            cookie,
            *runtime.credential_revisions.get(&user_id).unwrap_or(&0),
        ))
    })
    .await
    .map_err(|_| "Account credential task failed")?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingResult {
    pub user_id: u64,
    pub success: bool,
    pub message: String,
}

fn validate_display_name(name: &str) -> Result<(), String> {
    if !(3..=20).contains(&name.chars().count()) || name.chars().any(char::is_control) {
        return Err("Enter a display name with 3–20 characters and no control characters".into());
    }
    Ok(())
}

async fn set_display_name(
    app: &tauri::AppHandle,
    user_id: u64,
    name: &str,
) -> Result<String, String> {
    let state = app.state::<AppState>().inner().clone();
    let (cookie, revision) = credential(state.clone(), user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let response = client
        .request(
            reqwest::Method::PATCH,
            &format!("https://users.roblox.com/v1/users/{user_id}/display-names"),
            &cookie,
            Some(&serde_json::json!({"newDisplayName": name})),
        )
        .await
        .map_err(|_| "Roblox could not be reached. Check your profile before retrying")?;
    match response.status().as_u16() {
        200..=299 => {}
        400 => return Err("Roblox rejected this display name. Choose another name".into()),
        429 => {
            return Err("Roblox limits display-name changes. Wait until your cooldown ends".into())
        }
        401 | 403 => {
            return Err(
                "Roblox denied this change. Refresh the account or change the name on Roblox"
                    .into(),
            )
        }
        _ => return Err("Roblox could not change this display name. Try again later".into()),
    }
    let name = name.to_owned();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state.runtime.lock().map_err(|_| ())?;
        if !runtime.unlocked
            || *runtime.credential_revisions.get(&user_id).unwrap_or(&0) != revision
        {
            return Err(());
        }
        let previous = runtime.accounts.clone();
        runtime
            .accounts
            .find_by_id_mut(user_id)
            .ok_or(())?
            .display_name = name;
        if save_runtime(&runtime).is_err() {
            runtime.accounts = previous;
            return Err(());
        }
        Ok(())
    })
    .await;
    accounts::publish(app);
    Ok(if matches!(saved, Ok(Ok(()))) {
        "Display name changed"
    } else {
        "Display name changed on Roblox, but RM could not save it locally. Refresh this account"
    }
    .into())
}

#[tauri::command]
pub async fn change_display_names(
    app: tauri::AppHandle,
    user_ids: Vec<u64>,
    name: String,
) -> Result<Vec<SettingResult>, String> {
    let name = name.trim();
    validate_display_name(name)?;
    if user_ids.is_empty() || user_ids.len() > 500 || user_ids.contains(&0) {
        return Err("Select between 1 and 500 accounts".into());
    }
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for user_id in user_ids.into_iter().filter(|id| seen.insert(*id)) {
        let result = set_display_name(&app, user_id, name).await;
        results.push(SettingResult {
            user_id,
            success: result.is_ok(),
            message: result.unwrap_or_else(|error| error),
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_names_validate_unicode_length_and_control_characters() {
        assert!(validate_display_name("Name").is_ok());
        assert!(validate_display_name("名前名前").is_ok());
        assert!(validate_display_name("ab").is_err());
        assert!(validate_display_name(&"a".repeat(21)).is_err());
        assert!(validate_display_name("Name\n").is_err());
    }
}
