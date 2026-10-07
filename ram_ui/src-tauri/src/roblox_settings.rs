use crate::{account_cookie, accounts, save_runtime, state::AppState};
use ram_core::auth::RobloxClient;
use serde::Serialize;
use tauri::Manager;

pub async fn credential(state: AppState, user_id: u64) -> Result<(String, u64), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Couldn’t read this account. Try again.")?;
        let cookie = account_cookie(&runtime, user_id).map_err(|_| {
            "Unlock RM or log in to this account again before changing its settings."
        })?;
        Ok((
            cookie,
            *runtime.credential_revisions.get(&user_id).unwrap_or(&0),
        ))
    })
    .await
    .map_err(|_| "Couldn’t load this account’s login. Try again.")?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingResult {
    pub user_id: u64,
    pub success: bool,
    pub message: String,
}

fn validate_display_name(name: &str) -> Result<(), String> {
    if name.chars().count() < 3 {
        return Err("Display name is too short. Use at least 3 characters.".into());
    }
    if name.chars().count() > 20 {
        return Err("Display name is too long. Use no more than 20 characters.".into());
    }
    if name.chars().any(char::is_control) {
        return Err("Display name contains invalid characters. Try a different name.".into());
    }
    Ok(())
}

fn display_name_error(status: u16, codes: &[i64]) -> &'static str {
    match status {
        401 | 403 if codes.contains(&7) => "Roblox couldn’t find this account. Refresh it or log in again.",
        401 | 403 => "Roblox couldn’t log in to this account. Unlock RM, refresh the account or log in again.",
        429 if codes.contains(&5) => "This account can change its display name once every 7 days. Wait until its cooldown ends.",
        429 => "Roblox rate limited this request. Wait a few minutes before trying again.",
        400 => match codes.first() {
            Some(1) => "Display name is too short. Use at least 3 characters.",
            Some(2) => "Display name is too long. Use no more than 20 characters.",
            Some(3) => "Roblox rejected invalid characters in this name. Try a different name.",
            Some(4) => "Roblox moderated this name. Try a different one.",
            Some(8) => "This name mixes too many alphabets or writing systems. Try using fewer.",
            _ => "Roblox rejected this name without a clear reason. Try a different name or check the account on Roblox.",
        },
        _ => "Roblox is having trouble with display names. Try again later.",
    }
}

async fn check_display_name_response(response: reqwest::Response) -> Result<(), String> {
    let status = response.status().as_u16();
    if (200..=299).contains(&status) {
        return Ok(());
    }
    let body = response.json::<serde_json::Value>().await.ok();
    let codes: Vec<i64> = body
        .as_ref()
        .and_then(|body| body.get("errors"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|error| error.get("code").and_then(serde_json::Value::as_i64))
        .collect();
    Err(display_name_error(status, &codes).into())
}

async fn request_display_name(
    cookie: &str,
    user_id: u64,
    name: &str,
    verify: bool,
) -> Result<(), String> {
    // display-name 429 bodies distinguish the seven-day cooldown from request limits.
    // keep them intact instead of passing through the core's generic 429 retry loop.
    let client = reqwest::Client::builder()
        .user_agent("RM-display-name")
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Roblox is having trouble with display names. Try again later.")?;
    let mut cookie_header =
        reqwest::header::HeaderValue::from_str(&format!(".ROBLOSECURITY={cookie}"))
            .map_err(|_| "Roblox couldn’t log in to this account. Refresh it or log in again.")?;
    cookie_header.set_sensitive(true);
    let mut csrf = None;
    for attempt in 0..=2 {
        let suffix = if verify { "/validate" } else { "" };
        let mut url = reqwest::Url::parse(&format!(
            "https://users.roblox.com/v1/users/{user_id}/display-names{suffix}"
        ))
        .map_err(|_| "Roblox is having trouble with display names. Try again later.")?;
        let mut request = if verify {
            url.query_pairs_mut().append_pair("displayName", name);
            client.get(url)
        } else {
            client
                .patch(url)
                .json(&serde_json::json!({"newDisplayName": name}))
        }
        .header(reqwest::header::COOKIE, cookie_header.clone())
        .header(reqwest::header::REFERER, "https://www.roblox.com/")
        .header("x-bound-auth-token", "");
        if let Some(token) = &csrf {
            request = request.header("x-csrf-token", token);
        }
        let response = request.send().await.map_err(|_| {
            "Can’t reach Roblox. Check your connection and account profile before trying again."
        })?;
        if response.status() == reqwest::StatusCode::FORBIDDEN && attempt < 2 {
            if let Some(token) = response.headers().get("x-csrf-token") {
                let mut token = token.clone();
                token.set_sensitive(true);
                csrf = Some(token);
                continue;
            }
        }
        return check_display_name_response(response).await;
    }
    Err("Roblox couldn’t log in to this account. Refresh it or log in again.".into())
}

async fn set_display_name(
    app: &tauri::AppHandle,
    user_id: u64,
    name: &str,
) -> Result<String, String> {
    let state = app.state::<AppState>().inner().clone();
    let (cookie, revision) = credential(state.clone(), user_id).await?;
    request_display_name(&cookie, user_id, name, true).await?;
    request_display_name(&cookie, user_id, name, false).await?;
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
    if !matches!(saved, Ok(Ok(()))) {
        tracing::warn!(
            user_id,
            "Roblox display name changed but local persistence failed"
        );
        return Err("Your name changed on Roblox, but RM couldn’t save it. Refresh the account; don’t retry this change.".into());
    }
    accounts::publish(app);
    Ok("Display name verified".into())
}

#[tauri::command]
pub async fn check_roblox_display_name(
    state: tauri::State<'_, AppState>,
    user_ids: Vec<u64>,
    name: String,
) -> Result<(), String> {
    validate_display_name(&name)?;
    validate_ids(&user_ids)?;
    let user_id = user_ids[0];
    let (cookie, _) = credential(state.inner().clone(), user_id).await?;
    request_display_name(&cookie, user_id, &name, true).await
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

    #[test]
    fn display_name_failures_distinguish_api_codes_and_statuses() {
        let cases: &[(u16, &[i64], &str)] = &[
            (400, &[1], "too short"),
            (400, &[2], "too long"),
            (400, &[3], "invalid characters"),
            (400, &[4], "Roblox moderated this name"),
            (400, &[8], "writing systems"),
            (429, &[5], "cooldown"),
            (429, &[], "rate limited"),
            (401, &[0], "log in"),
            (403, &[0], "log in"),
            (403, &[7], "find this account"),
            (500, &[4], "trouble with display names"),
            (503, &[], "trouble with display names"),
            (400, &[999], "clear reason"),
            (400, &[], "clear reason"),
        ];
        for (status, codes, expected) in cases {
            assert!(display_name_error(*status, codes).contains(expected));
        }
        assert!(validate_display_name("ab")
            .unwrap_err()
            .contains("too short"));
        assert!(validate_display_name(&"a".repeat(21))
            .unwrap_err()
            .contains("too long"));
        assert!(validate_display_name("Name\n")
            .unwrap_err()
            .contains("invalid characters"));
    }
}

const PRIVACY_URL: &str = "https://apis.roblox.com/user-settings-api/v1/user-settings";
const PRIVACY_FIELDS: [&str; 2] = ["whoCanJoinMeInExperiences", "whoCanSeeMyOnlineStatus"];

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn option_shape(value: &serde_json::Value) -> String {
    option_shape_at(value, 0)
}

fn option_shape_at(value: &serde_json::Value, depth: usize) -> String {
    if depth >= 3 {
        return json_kind(value).into();
    }
    let Some(object) = value.as_object() else {
        return json_kind(value).into();
    };
    let known_keys = [
        "value",
        "option",
        "optionValue",
        "optionName",
        "name",
        "key",
        "id",
        "label",
        "displayName",
        "isEnabled",
        "enabled",
        "isDisabled",
        "disabled",
        "isAvailable",
        "text",
        "type",
        "optionType",
        "optionKey",
        "optionId",
        "displayValue",
        "settingValue",
        "valueToSet",
        "isAllowed",
        "isSelectable",
        "isRestricted",
        "canBeSelected",
        "available",
        "availability",
        "requirements",
        "restriction",
        "restrictions",
        "restrictionReason",
        "restrictionReasons",
        "disabledReason",
        "disabledReasons",
        "reason",
        "description",
        "translationKey",
        "errorCode",
        "error",
        "errors",
        "errorReason",
        "validationError",
        "validationErrors",
        "invalidReason",
        "invalidReasons",
        "isValid",
        "isOptionEnabled",
        "isOptionDisabled",
        "isOptionAvailable",
        "isOptionSelectable",
        "restrictionType",
        "optionRestriction",
        "optionRestrictions",
        "unavailabilityReason",
        "disabledReasonCode",
        "restrictionReasonCode",
        "dependencies",
        "dependentSettings",
        "requiredSettings",
        "requiresParentalConsent",
        "requiresAgeVerification",
    ];
    let mut members = Vec::new();
    for key in known_keys {
        if let Some(value) = object.get(key) {
            members.push(format!(
                "{key}:{}",
                if value.is_object() {
                    option_shape_at(value, depth + 1)
                } else {
                    json_kind(value).into()
                }
            ));
        }
    }
    members.push(format!(
        "otherKeys:{}",
        object
            .keys()
            .filter(|key| !known_keys.contains(&key.as_str()))
            .count()
    ));
    format!("object({})", members.join(","))
}

fn privacy_schema(value: &serde_json::Value) -> serde_json::Value {
    let fields = PRIVACY_FIELDS.into_iter().map(|field| {
        let setting = &value[field];
        let options = &setting["options"];
        (field.to_owned(), serde_json::json!({
            "settingType": json_kind(setting),
            "currentType": json_kind(&setting["currentValue"]),
            "optionsType": json_kind(options),
            "optionCount": options.as_array().map(Vec::len),
            "optionShapes": options.as_array().map(|options| options.iter().take(8).map(option_shape).collect::<std::collections::BTreeSet<_>>())
        }))
    }).collect::<serde_json::Map<_, _>>();
    serde_json::Value::Object(fields)
}

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
        .or_else(|| value.get("value").and_then(serde_json::Value::as_str))
        .or_else(|| value.get("optionValue").and_then(serde_json::Value::as_str))
        .or_else(|| {
            value
                .get("option")
                .and_then(|option| option.get("optionValue"))
                .and_then(serde_json::Value::as_str)
        })
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

async fn fetch_privacy(
    client: &RobloxClient,
    cookie: &str,
    user_id: u64,
    request_id: &str,
    phase: &'static str,
) -> Result<Vec<PrivacySetting>, String> {
    let started = std::time::Instant::now();
    tracing::debug!(
        event = "roblox_visibility_request",
        request_id,
        user_id,
        phase,
        "Loading Roblox visibility settings"
    );
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
        .map_err(|_| {
            tracing::warn!(
                event = "roblox_visibility_failure",
                request_id,
                user_id,
                phase,
                failure = "transport",
                "Roblox visibility request failed"
            );
            "Roblox visibility settings could not be loaded. Try again"
        })?;
    let status = response.status().as_u16();
    tracing::debug!(
        event = "roblox_visibility_response",
        request_id,
        user_id,
        phase,
        status,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Roblox visibility response received"
    );
    if !response.status().is_success() {
        tracing::warn!(
            event = "roblox_visibility_failure",
            request_id,
            user_id,
            phase,
            failure = "http",
            status,
            "Roblox denied the visibility request"
        );
        return Err("Roblox denied access to visibility settings. Refresh the account or use Roblox settings".into());
    }
    let value = response.json().await.map_err(|_| {
        tracing::warn!(
            event = "roblox_visibility_failure",
            request_id,
            user_id,
            phase,
            failure = "json",
            "Roblox visibility response was not valid JSON"
        );
        "Roblox returned unreadable visibility settings"
    })?;
    let result = parse_privacy(&value);
    if result.is_err() {
        tracing::warn!(
            event = "roblox_visibility_parse_failed",
            request_id,
            user_id,
            phase,
            "Roblox visibility response shape was not supported"
        );
        tracing::debug!(event = "roblox_visibility_schema", request_id, user_id, phase, schema = %privacy_schema(&value), "Roblox visibility response shape was not supported");
    } else {
        tracing::debug!(event = "roblox_visibility_schema", request_id, user_id, phase, schema = %privacy_schema(&value), "Roblox visibility response shape parsed");
        tracing::debug!(
            event = "roblox_visibility_loaded",
            request_id,
            user_id,
            phase,
            "Roblox visibility settings parsed"
        );
    }
    result.map_err(|error| format!("{error}. Diagnostic ID: {request_id}"))
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
            Ok((cookie, _)) => {
                fetch_privacy(
                    &client,
                    &cookie,
                    user_id,
                    &uuid::Uuid::new_v4().to_string(),
                    "load",
                )
                .await
            }
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

fn visibility_tracker(header: &str) -> Option<&str> {
    let (name, value) = header.split(';').next()?.split_once('=')?;
    if name.trim() != "RBXEventTrackerV2" {
        return None;
    }
    value.split('&').find_map(|part| {
        let (key, id) = part.split_once('=')?;
        (key.eq_ignore_ascii_case("browserid")
            && !id.is_empty()
            && id.len() <= 20
            && id.bytes().all(|byte| byte.is_ascii_digit())
            && id.parse::<u64>().is_ok_and(|id| id > 0))
        .then_some(id)
    })
}

async fn add_visibility_context(
    client: &RobloxClient,
    cookie: &mut String,
    user_id: u64,
    request_id: &str,
) -> Result<(), String> {
    // roblox requires its issued browser context for settings writes, in addition to csrf.
    // retain only the validated browser id, scoped to this account's save; never persist it.
    let response = client
        .request(
            reqwest::Method::GET,
            "https://www.roblox.com/home",
            cookie,
            None,
        )
        .await
        .map_err(|_| "Roblox save context could not be loaded. Try again")?;
    let tracker = response
        .status()
        .is_success()
        .then(|| {
            response
                .headers()
                .get_all(reqwest::header::SET_COOKIE)
                .iter()
                .find_map(|header| header.to_str().ok().and_then(visibility_tracker))
        })
        .flatten();
    let Some(tracker) = tracker else {
        tracing::warn!(
            event = "roblox_visibility_context_failed",
            request_id,
            user_id,
            status = response.status().as_u16(),
            "Roblox did not issue visibility save context"
        );
        return Err(format!("Roblox did not provide the context needed to save visibility. Refresh the account and retry. Diagnostic ID: {request_id}"));
    };
    cookie.push_str("; RBXEventTrackerV2=browserid=");
    cookie.push_str(tracker);
    tracing::debug!(
        event = "roblox_visibility_context_ready",
        request_id,
        user_id,
        "Roblox visibility save context acquired"
    );
    Ok(())
}

async fn change_privacy(
    state: AppState,
    client: &RobloxClient,
    user_id: u64,
    field: &str,
    value: &str,
) -> Result<String, String> {
    let (mut cookie, _) = credential(state, user_id).await?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let settings = fetch_privacy(client, &cookie, user_id, &request_id, "preflight").await?;
    if !settings.iter().any(|setting| {
        setting.field == field && setting.options.iter().any(|option| option == value)
    }) {
        tracing::warn!(
            event = "roblox_visibility_save_refused",
            request_id,
            user_id,
            field,
            "Requested visibility choice was not allowed by Roblox"
        );
        return Err(
            "This choice is unavailable for this account. Reload visibility settings".into(),
        );
    }
    tracing::debug!(
        event = "roblox_visibility_save",
        request_id,
        user_id,
        field,
        "Saving Roblox visibility choice"
    );
    add_visibility_context(client, &mut cookie, user_id, &request_id).await?;
    let response = client
        .request(
            reqwest::Method::POST,
            PRIVACY_URL,
            &cookie,
            Some(&serde_json::json!({field: value})),
        )
        .await
        .map_err(|_| {
            tracing::warn!(
                event = "roblox_visibility_failure",
                request_id,
                user_id,
                field,
                failure = "save_transport",
                "Roblox visibility save failed"
            );
            "Visibility update could not be confirmed. Reload settings before retrying"
        })?;
    tracing::debug!(
        event = "roblox_visibility_save_response",
        request_id,
        user_id,
        field,
        status = response.status().as_u16(),
        "Roblox visibility save response received"
    );
    if !response.status().is_success() {
        tracing::warn!(
            event = "roblox_visibility_save_failed",
            request_id,
            user_id,
            field,
            status = response.status().as_u16(),
            "Roblox rejected the visibility change"
        );
        return Err(format!("Roblox rejected this visibility change (HTTP {}). Reload settings and retry. Diagnostic ID: {request_id}", response.status().as_u16()));
    }
    let settings = fetch_privacy(client, &cookie, user_id, &request_id, "verify")
        .await
        .map_err(|_| {
            "Update sent, but verification failed. Reload visibility settings before retrying"
        })?;
    if !settings
        .iter()
        .any(|setting| setting.field == field && setting.current_value == value)
    {
        tracing::warn!(
            event = "roblox_visibility_verification_failed",
            request_id,
            user_id,
            field,
            "Roblox did not retain the requested visibility choice"
        );
        return Err("Roblox did not retain the requested choice. Reload settings to see the applied restrictions".into());
    }
    tracing::info!(
        event = "roblox_visibility_verified",
        request_id,
        user_id,
        field,
        "Roblox visibility change verified"
    );
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
    fn visibility_context_accepts_only_roblox_tracker_with_numeric_browser_id() {
        assert_eq!(visibility_tracker("RBXEventTrackerV2=CreateDate=synthetic&rbxid=42&browserid=123456; Domain=roblox.com; Secure"), Some("123456"));
        assert_eq!(
            visibility_tracker("RBXEventTrackerV2=BrowserID=42"),
            Some("42")
        );
        for header in [
            "OtherCookie=browserid=42",
            "RBXEventTrackerV2=rbxid=42",
            "RBXEventTrackerV2=browserid=",
            "RBXEventTrackerV2=browserid=0",
            "RBXEventTrackerV2=browserid=-1",
            "RBXEventTrackerV2=browserid=42\r\nInjected: value",
            "RBXEventTrackerV2=browserid=42%3bsecret=value",
            "RBXEventTrackerV2=browserid=18446744073709551616",
        ] {
            assert!(visibility_tracker(header).is_none());
        }
    }

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

    #[test]
    fn schema_diagnostics_never_include_remote_strings_or_unknown_keys() {
        let value = serde_json::json!({
            "whoCanJoinMeInExperiences": { "currentValue": "synthetic-cookie", "options": [{
                "value": "synthetic-token", "label": "synthetic-password", "synthetic-private-key": "synthetic-secret"
            }] },
            "whoCanSeeMyOnlineStatus": { "currentValue": "synthetic-ticket", "options": ["synthetic-csrf"] },
            "credentials": "synthetic-credential"
        });
        let diagnostic = privacy_schema(&value).to_string();
        assert!(!diagnostic.contains("synthetic"));
        assert!(diagnostic.contains("value:string"));
        assert!(diagnostic.contains("otherKeys:1"));
        assert!(diagnostic.contains("currentType"));
    }

    #[test]
    fn live_nested_option_value_shape_is_decoded_without_using_display_metadata() {
        let value = serde_json::json!({
            "whoCanJoinMeInExperiences": { "currentValue": "Following", "options": [
                {"option":{"optionValue":"All"}},
                {"option":{"optionValue":"Followers"}},
                {"option":{"optionValue":"Following"}},
                {"option":{"optionValue":"Friends"}},
                {"option":{"optionValue":"TrustedFriends"}},
                {"option":{"optionValue":"NoOne"}}
            ]},
            "whoCanSeeMyOnlineStatus": { "currentValue": "AllUsers", "options": [
                {"option":{"optionValue":"AllUsers"}},
                {"option":{"optionValue":"FriendsFollowingAndFollowers"}},
                {"option":{"optionValue":"FriendsAndFollowing"}},
                {"option":{"optionValue":"Friends"}},
                {"option":{"optionValue":"TrustedFriends"}},
                {"option":{"optionValue":"NoOne"},"disabled":true}
            ]}
        });
        let settings = parse_privacy(&value).unwrap();
        assert_eq!(
            settings[0].options,
            [
                "All",
                "Followers",
                "Following",
                "Friends",
                "TrustedFriends",
                "NoOne"
            ]
        );
        assert_eq!(settings[1].options.len(), 5);
        assert!(!settings[1].options.contains(&"NoOne".into()));
        assert_eq!(settings[1].current_value, "AllUsers");
    }
}
