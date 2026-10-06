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
pub async fn check_roblox_display_name(
    state: tauri::State<'_, AppState>,
    user_ids: Vec<u64>,
    name: String,
) -> Result<(), String> {
    validate_display_name(&name)?;
    validate_ids(&user_ids)?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let mut seen = std::collections::HashSet::new();
    for user_id in user_ids.into_iter().filter(|id| seen.insert(*id)) {
        let (cookie, _) = credential(state.inner().clone(), user_id).await?;
        let mut url = reqwest::Url::parse(&format!(
            "https://users.roblox.com/v1/users/{user_id}/display-names/validate"
        ))
        .map_err(|_| "Display-name validation unavailable")?;
        url.query_pairs_mut().append_pair("displayName", &name);
        let response = client
            .request(reqwest::Method::GET, url.as_str(), &cookie, None)
            .await
            .map_err(|_| "Name could not be checked. Edit the name to retry")?;
        match response.status().as_u16() {
            200 => {}
            400 => return Err("Roblox does not support this name. Choose another name".into()),
            429 => return Err("Roblox is limiting name checks or changes. Try again later".into()),
            401 | 403 => return Err(
                "Roblox could not check this name for every account. Refresh the selected accounts"
                    .into(),
            ),
            _ => return Err("Name could not be checked. Edit the name to retry".into()),
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn change_display_names(
    app: tauri::AppHandle,
    user_ids: Vec<u64>,
    name: String,
) -> Result<Vec<SettingResult>, String> {
    let name = name.as_str();
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

const PRIVACY_URL: &str = "https://apis.roblox.com/user-settings-api/v1/user-settings";
const PRIVACY_FIELDS: [&str; 2] = ["whoCanJoinMeInExperiences", "whoCanSeeMyOnlineStatus"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacySetting {
    field: String,
    current_value: String,
    options: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPrivacy {
    user_id: u64,
    settings: Vec<PrivacySetting>,
    error: Option<String>,
}

fn visibility_value(value: &serde_json::Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("value")?.as_str())
        .filter(|text| {
            !text.is_empty()
                && text.len() <= 100
                && text.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                })
        })
}

fn parse_privacy(value: &serde_json::Value) -> Result<Vec<PrivacySetting>, String> {
    PRIVACY_FIELDS.iter().map(|field| {
        let setting = &value[*field];
        let current = visibility_value(&setting["currentValue"]).ok_or("Roblox did not return these visibility settings. Use Roblox settings for this account")?;
        let entries = setting["options"].as_array().ok_or("Roblox did not return visibility choices")?;
        let mut options = Vec::new();
        let mut recognised = entries.is_empty();
        for entry in entries {
            let Some(option) = visibility_value(entry) else { continue; };
            recognised = true;
            if entry.get("isEnabled").and_then(serde_json::Value::as_bool) == Some(false)
                || entry.get("enabled").and_then(serde_json::Value::as_bool) == Some(false)
                || entry.get("isDisabled").and_then(serde_json::Value::as_bool) == Some(true)
                || entry.get("disabled").and_then(serde_json::Value::as_bool) == Some(true) {
                continue;
            }
            if !options.iter().any(|value| value == option) { options.push(option.to_owned()); }
        }
        if !recognised { return Err("Roblox did not provide usable visibility choices. Refresh the account or use Roblox settings".into()); }
        Ok(PrivacySetting { field: (*field).into(), current_value: current.into(), options })
    }).collect()
}

async fn fetch_privacy(client: &RobloxClient, cookie: &str) -> Result<Vec<PrivacySetting>, String> {
    let response = client
        .request(
            reqwest::Method::GET,
            &format!(
                "{PRIVACY_URL}/settings-and-options?requestedUserSettings={}",
                PRIVACY_FIELDS.join(",")
            ),
            cookie,
            None,
        )
        .await
        .map_err(|_| "Roblox visibility settings could not be loaded. Try again")?;
    if !response.status().is_success() {
        return Err("Roblox denied access to visibility settings. Refresh the account or use Roblox settings".into());
    }
    let value = response
        .json()
        .await
        .map_err(|_| "Roblox returned unreadable visibility settings")?;
    parse_privacy(&value)
}

fn validate_ids(ids: &[u64]) -> Result<(), String> {
    if ids.is_empty() || ids.len() > 500 || ids.contains(&0) {
        return Err("Select between 1 and 500 accounts".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn get_roblox_privacy(
    state: tauri::State<'_, AppState>,
    user_ids: Vec<u64>,
) -> Result<Vec<AccountPrivacy>, String> {
    validate_ids(&user_ids)?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for user_id in user_ids.into_iter().filter(|id| seen.insert(*id)) {
        let result = match credential(state.inner().clone(), user_id).await {
            Ok((cookie, _)) => fetch_privacy(&client, &cookie).await,
            Err(error) => Err(error),
        };
        let (settings, error) = match result {
            Ok(settings) => (settings, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        results.push(AccountPrivacy {
            user_id,
            settings,
            error,
        });
    }
    Ok(results)
}

async fn change_privacy(
    state: AppState,
    client: &RobloxClient,
    user_id: u64,
    field: &str,
    value: &str,
) -> Result<String, String> {
    let (cookie, _) = credential(state, user_id).await?;
    let settings = fetch_privacy(client, &cookie).await?;
    if !settings.iter().any(|setting| {
        setting.field == field && setting.options.iter().any(|option| option == value)
    }) {
        return Err(
            "This choice is unavailable for this account. Reload visibility settings".into(),
        );
    }
    let response = client
        .request(
            reqwest::Method::POST,
            PRIVACY_URL,
            &cookie,
            Some(&serde_json::json!({field: value})),
        )
        .await
        .map_err(|_| "Visibility update could not be confirmed. Reload settings before retrying")?;
    if !response.status().is_success() {
        return Err("Roblox rejected this visibility change. Check age, region or parental restrictions on Roblox".into());
    }
    let settings = fetch_privacy(client, &cookie).await.map_err(|_| {
        "Update sent, but verification failed. Reload visibility settings before retrying"
    })?;
    if !settings
        .iter()
        .any(|setting| setting.field == field && setting.current_value == value)
    {
        return Err("Roblox did not retain the requested choice. Reload settings to see the applied restrictions".into());
    }
    Ok("Visibility setting changed and verified on Roblox".into())
}

#[tauri::command]
pub async fn change_roblox_privacy(
    state: tauri::State<'_, AppState>,
    user_ids: Vec<u64>,
    field: String,
    value: String,
) -> Result<Vec<SettingResult>, String> {
    validate_ids(&user_ids)?;
    if !PRIVACY_FIELDS.contains(&field.as_str()) || value.is_empty() || value.len() > 100 {
        return Err("Choose a supported Roblox visibility setting".into());
    }
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for user_id in user_ids.into_iter().filter(|id| seen.insert(*id)) {
        let result = change_privacy(state.inner().clone(), &client, user_id, &field, &value).await;
        results.push(SettingResult {
            user_id,
            success: result.is_ok(),
            message: result.unwrap_or_else(|error| error),
        });
    }
    Ok(results)
}

#[cfg(test)]
mod privacy_tests {
    use super::*;
    #[test]
    fn missing_or_unrecognised_privacy_does_not_invent_choices() {
        assert!(parse_privacy(&serde_json::json!({})).is_err());
        let value = serde_json::json!({
            "whoCanJoinMeInExperiences": { "currentValue": "Friends", "options": ["Friends", "NoOne"] },
            "whoCanSeeMyOnlineStatus": { "currentValue": "NoOne", "options": ["Friends", "NoOne"] },
            "unrelatedSecret": "synthetic"
        });
        let settings = parse_privacy(&value).unwrap();
        assert_eq!(settings.len(), 2);
        assert_eq!(settings[0].options, ["Friends", "NoOne"]);
        assert_eq!(settings[1].current_value, "NoOne");
    }
    #[test]
    fn invalid_account_selections_are_rejected() {
        assert!(validate_ids(&[]).is_err());
        assert!(validate_ids(&[0]).is_err());
        assert!(validate_ids(&vec![1; 501]).is_err());
        assert!(validate_ids(&[1, 2]).is_ok());
    }
    #[test]
    fn structured_choices_preserve_values_and_exclude_disabled_options() {
        let value = serde_json::json!({
            "whoCanJoinMeInExperiences": { "currentValue": "Followers", "options": [
                { "value": "All", "label": "Everyone", "isEnabled": false },
                { "value": "Followers", "label": "Friends, followers and following" },
                { "value": "Following" }, "Following", { "value": "Friends" },
                { "value": "TrustedFriends", "disabled": true }, { "value": "NoOne" },
                { "label": "Invalid option without a value" }, null, 7
            ] },
            "whoCanSeeMyOnlineStatus": { "currentValue": "AllUsers", "options": [
                { "value": "AllUsers" }, { "value": "Friends" }, { "value": "NoOne" }
            ] }
        });
        let settings = parse_privacy(&value).unwrap();
        assert_eq!(settings[0].current_value, "Followers");
        assert_eq!(
            settings[0].options,
            ["Followers", "Following", "Friends", "NoOne"]
        );
        assert_eq!(settings[1].options, ["AllUsers", "Friends", "NoOne"]);
    }

    #[test]
    fn malformed_options_never_grant_permission_from_labels_or_current_values() {
        let mut value = serde_json::json!({
            "whoCanJoinMeInExperiences": { "currentValue": "All", "options": [{"label":"All"}, {"value":"../../All"}] },
            "whoCanSeeMyOnlineStatus": { "currentValue": "NoOne", "options": [] }
        });
        assert!(parse_privacy(&value).is_err());
        value["whoCanJoinMeInExperiences"]["options"] = serde_json::json!([]);
        let settings = parse_privacy(&value).unwrap();
        assert!(settings.iter().all(|setting| setting.options.is_empty()));
    }
}
