use base64::Engine;
use chrono::Utc;
use ram_core::assets::{
    self, AssetIndex, AssetRecord, AssetState, Creator, IndexLoad, ModerationStatus,
    OperationOutcome, StagedFile,
};
use ram_core::{assets_api, auth::RobloxClient, error::CoreError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

pub struct AssetManager(Arc<Mutex<AssetRuntime>>);

struct AssetRuntime {
    index: AssetIndex,
    path: PathBuf,
    is_read_only: bool,
    notice: Option<String>,
    upload_rows: HashSet<String>,
    is_worker_running: bool,
}

impl Default for AssetManager {
    fn default() -> Self {
        let directory = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RM");
        let path = assets::index_path(&directory);
        let (mut index, status) = AssetIndex::load(&path);
        for row in &mut index.records {
            if matches!(row.state, AssetState::Uploading) {
                row.state = AssetState::Failed {
                    message: "The app closed during upload. Check Creator Dashboard before importing again.".into(),
                    retryable: false,
                };
            }
        }
        let notice = match status {
            IndexLoad::Ok => None,
            IndexLoad::RecoveredFromBackup => {
                Some("The asset library was recovered from its backup.".into())
            }
            IndexLoad::Corrupt => Some(
                "The asset index is damaged. Uploads are disabled to protect your data.".into(),
            ),
            IndexLoad::NewerSchema => {
                Some("The asset index needs a newer version of RM. Uploads are disabled.".into())
            }
        };
        Self(Arc::new(Mutex::new(AssetRuntime {
            index,
            path,
            is_read_only: status.is_read_only(),
            notice,
            upload_rows: HashSet::new(),
            is_worker_running: false,
        })))
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRow {
    row_id: String,
    display_name: String,
    kind: String,
    file_bytes: u64,
    uploaded_by: u64,
    creator: Creator,
    state: &'static str,
    asset_id: Option<u64>,
    message: Option<String>,
    can_retry: bool,
    can_remove: bool,
    granted_universes: Vec<u64>,
}

fn summarize_row(row: &AssetRecord) -> AssetRow {
    let (state, message) = match &row.state {
        AssetState::Queued => ("queued", None),
        AssetState::Invalid { reason } => ("invalid", Some(reason.clone())),
        AssetState::Duplicate { .. } => (
            "duplicate",
            Some("These bytes were already uploaded for this creator.".into()),
        ),
        AssetState::Uploading => ("uploading", None),
        AssetState::Pending { .. } => ("pending", None),
        AssetState::InReview { .. } => ("inReview", None),
        AssetState::Approved { .. } => ("approved", None),
        AssetState::Rejected { .. } => (
            "rejected",
            Some("Roblox rejected this asset during moderation.".into()),
        ),
        AssetState::Failed { message, .. } => ("failed", Some(message.clone())),
        AssetState::Expired { .. } => (
            "expired",
            Some("The status check expired. The asset may still exist on Roblox.".into()),
        ),
        AssetState::Cancelled => ("cancelled", None),
    };
    AssetRow {
        row_id: row.row_id.clone(),
        display_name: row.display_name.clone(),
        kind: row.kind.as_api_str().into(),
        file_bytes: row.file_bytes,
        uploaded_by: row.uploaded_by,
        creator: row.creator,
        state,
        asset_id: row.state.asset_id(),
        message: message.map(|text| ram_core::redact::scrub(&text).into_owned()),
        can_retry: matches!(
            row.state,
            AssetState::Failed {
                retryable: true,
                ..
            }
        ),
        can_remove: !row.state.is_active(),
        granted_universes: row.granted_universes.clone(),
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetWorkspace {
    rows: Vec<AssetRow>,
    is_uploading: bool,
    is_read_only: bool,
    notice: Option<String>,
}

impl AssetRuntime {
    fn snapshot(&self) -> AssetWorkspace {
        AssetWorkspace {
            rows: self.index.records.iter().map(summarize_row).collect(),
            is_uploading: !self.upload_rows.is_empty()
                || self
                    .index
                    .records
                    .iter()
                    .any(|row| matches!(row.state, AssetState::Uploading)),
            is_read_only: self.is_read_only,
            notice: self.notice.clone(),
        }
    }

    fn persist(&mut self, previous: AssetIndex) -> Result<(), String> {
        if self.index.save(&self.path).is_err() {
            self.index = previous;
            self.is_read_only = true;
            self.upload_rows.clear();
            self.notice = Some(
                "The asset index could not be saved. Uploads have stopped to protect your data."
                    .into(),
            );
            return Err(self.notice.clone().unwrap_or_default());
        }
        Ok(())
    }
}

async fn credentials(app: &tauri::AppHandle, user_id: u64) -> Result<String, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || load_credentials(&app, user_id))
        .await
        .map_err(|_| "Credential task unavailable".to_string())?
}

fn load_credentials(app: &tauri::AppHandle, user_id: u64) -> Result<String, String> {
    let state = app.state::<crate::state::AppState>();
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?;
    if !runtime.unlocked {
        return Err("Unlock your account store before uploading.".into());
    }
    let account = runtime
        .accounts
        .find_by_id(user_id)
        .ok_or("Account not found")?;
    if account.cookie_expired {
        return Err("Re-add this account before uploading.".into());
    }
    crate::account_cookie(&runtime, user_id).map_err(|_| {
        "This account's credential is unavailable. Unlock or re-add the account.".into()
    })
}

fn describe_error(error: &CoreError) -> String {
    match error {
        CoreError::RobloxApi { status, .. } => assets_api::describe_status(*status),
        CoreError::RateLimited => assets_api::describe_status(429),
        CoreError::CookieRejected
        | CoreError::CookieRejectedWithReason(_)
        | CoreError::AuthFailed(_) => {
            "Roblox rejected this account's session. Re-add the account.".into()
        }
        _ => "The Roblox request did not complete. Check your connection and Creator Dashboard."
            .into(),
    }
}

async fn validate_creator(
    client: &RobloxClient,
    cookie: &str,
    user_id: u64,
    creator: Creator,
) -> Result<(), String> {
    assets_api::validate_creator(client, cookie, user_id, creator)
        .await
        .map_err(|error| match error {
            assets_api::CreatorValidationError::InvalidCreator => {
                "Choose this account or a group it can publish for.".into()
            }
            assets_api::CreatorValidationError::GroupUnavailable => {
                "This account cannot publish for the selected group.".into()
            }
            assets_api::CreatorValidationError::Request(error) => describe_error(&error),
        })
}

#[derive(Serialize)]
pub struct AssetCreator {
    id: u64,
    name: String,
}

#[tauri::command]
pub async fn list_asset_creators(
    app: tauri::AppHandle,
    user_id: u64,
) -> Result<Vec<AssetCreator>, String> {
    let cookie = credentials(&app, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    assets_api::list_publishable_groups(&client, &cookie)
        .await
        .map(|groups| {
            groups
                .into_iter()
                .map(|group| AssetCreator {
                    id: group.group_id,
                    name: group.name,
                })
                .collect()
        })
        .map_err(|error| describe_error(&error))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEdit {
    row_id: String,
    name: String,
    kind: String,
    creator: Creator,
}

#[tauri::command]
pub async fn update_asset_row(
    app: tauri::AppHandle,
    user_id: u64,
    edit: QueueEdit,
) -> Result<AssetWorkspace, String> {
    let kind = assets::AssetKind::from_api_str(&edit.kind)
        .filter(|kind| assets::AssetKind::selectable().contains(kind))
        .ok_or("Choose a supported asset type")?;
    if edit.name.trim().is_empty() || edit.name.chars().count() > 100 {
        return Err("Asset names must contain between one and 100 characters".into());
    }
    let cookie = credentials(&app, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    validate_creator(&client, &cookie, user_id, edit.creator).await?;
    let manager = app.state::<AssetManager>().0.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only".into());
        }
        if runtime.upload_rows.contains(&edit.row_id) {
            return Err("Wait for the selected upload to finish before editing it".into());
        }
        let original = runtime
            .index
            .get(&edit.row_id)
            .ok_or("Queue row not found")?;
        if original.uploaded_by != user_id
            || !matches!(
                original.state,
                AssetState::Queued | AssetState::Duplicate { .. }
            )
        {
            return Err("Only queued or duplicate rows for this account can be edited".into());
        }
        let duplicate = runtime
            .index
            .find_uploaded(&original.file_sha256, edit.creator)
            .and_then(|row| row.state.asset_id());
        if runtime.index.records.iter().any(|row| {
            row.row_id != edit.row_id
                && row.file_sha256 == original.file_sha256
                && row.creator == edit.creator
                && (row.state.is_active() || matches!(row.state, AssetState::Queued))
        }) {
            return Err("These bytes are already queued for this creator".into());
        }
        let previous = runtime.index.clone();
        let row = runtime
            .index
            .get_mut(&edit.row_id)
            .ok_or("Queue row not found")?;
        row.display_name = assets::sanitize_display_name(&edit.name);
        row.kind = kind;
        row.creator = edit.creator;
        row.state = duplicate.map_or(AssetState::Queued, |asset_id| AssetState::Duplicate {
            asset_id,
        });
        runtime.persist(previous)?;
        Ok::<_, String>(runtime.snapshot())
    })
    .await
    .map_err(|_| "Queue edit task failed")??;
    let _ = app.emit("assets-updated", &snapshot);
    Ok(snapshot)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationRow {
    asset_id: u64,
    name: String,
    kind: String,
    updated_at: Option<String>,
    thumbnail_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationPage {
    rows: Vec<CreationRow>,
    next_cursor: Option<String>,
}

#[tauri::command]
pub async fn list_asset_creations(
    app: tauri::AppHandle,
    user_id: u64,
    creator: Creator,
    kind: String,
    cursor: Option<String>,
) -> Result<CreationPage, String> {
    let kind = assets::AssetKind::from_api_str(&kind)
        .filter(|kind| assets::AssetKind::selectable().contains(kind))
        .ok_or("Choose a supported creation type")?;
    if cursor
        .as_ref()
        .is_some_and(|cursor| cursor.len() > 2048 || cursor.chars().any(char::is_control))
    {
        return Err("Invalid creation cursor".into());
    }
    let cookie = credentials(&app, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    validate_creator(&client, &cookie, user_id, creator).await?;
    let page = assets_api::list_creations(&client, &cookie, creator, kind, cursor.as_deref())
        .await
        .map_err(|error| describe_error(&error))?;
    let asset_ids = page
        .items
        .iter()
        .map(|row| row.asset_id)
        .take(100)
        .collect::<Vec<_>>();
    let thumbnails = ram_core::cached_api::fetch_asset_thumbnails(&client, &asset_ids)
        .await
        .unwrap_or_default();
    Ok(CreationPage {
        rows: page
            .items
            .into_iter()
            .take(100)
            .map(|row| CreationRow {
                asset_id: row.asset_id,
                name: row.name,
                kind: row.kind.as_api_str().into(),
                updated_at: row.updated.map(|date| date.to_rfc3339()),
                thumbnail_url: thumbnails
                    .iter()
                    .find(|(id, bytes)| *id == row.asset_id && bytes.len() <= 8 * 1024 * 1024)
                    .map(|(_, bytes)| {
                        format!(
                            "data:image/png;base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(bytes)
                        )
                    }),
            })
            .collect(),
        next_cursor: page.next_cursor,
    })
}

#[derive(Serialize)]
pub struct AssetGrant {
    granted: Vec<u64>,
    failures: Vec<Option<u64>>,
    notice: Option<String>,
}

#[tauri::command]
pub async fn grant_asset_access(
    app: tauri::AppHandle,
    user_id: u64,
    universe_id: u64,
    asset_ids: Vec<u64>,
) -> Result<AssetGrant, String> {
    if universe_id == 0 || asset_ids.is_empty() || asset_ids.len() > 100 || asset_ids.contains(&0) {
        return Err("Choose an experience and between one and 100 valid asset IDs".into());
    }
    let cookie = credentials(&app, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let universes = assets_api::list_manageable_universes(&client, &cookie)
        .await
        .map_err(|error| describe_error(&error))?;
    if !universes
        .iter()
        .any(|universe| universe.universe_id == universe_id)
    {
        return Err("This account cannot manage that experience".into());
    }
    let outcome = assets_api::grant_use_permission(&client, &cookie, universe_id, &asset_ids)
        .await
        .map_err(|error| describe_error(&error))?;
    let granted: Vec<u64> = outcome
        .granted
        .into_iter()
        .filter(|id| asset_ids.contains(id))
        .collect();
    let confirmed = granted.clone();
    let manager = app.state::<AssetManager>().0.clone();
    let saved = update_index(&app, manager, move |runtime| {
        for row in &mut runtime.index.records {
            if row.uploaded_by == user_id
                && row
                    .state
                    .asset_id()
                    .is_some_and(|id| confirmed.contains(&id))
                && !row.granted_universes.contains(&universe_id)
            {
                row.granted_universes.push(universe_id);
            }
        }
    })
    .await;
    Ok(AssetGrant { granted, failures: outcome.failures.into_iter().map(|(id, _)| id.filter(|id| asset_ids.contains(id))).collect(), notice: saved.err().map(|_| "Roblox permissions were checked, but the local access mirror could not be saved. Check Creator Dashboard.".into()) })
}

#[tauri::command]
pub async fn reveal_asset_file(app: tauri::AppHandle, row_id: String) -> Result<(), String> {
    let manager = app.state::<AssetManager>().0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = manager
            .lock()
            .map_err(|_| "Asset state unavailable")?
            .index
            .get(&row_id)
            .ok_or("Asset row not found")?
            .file_path
            .clone();
        let path = path
            .canonicalize()
            .map_err(|_| "The imported file no longer exists")?;
        if !path.is_file() {
            return Err("The imported file no longer exists".into());
        }
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|_| "The file could not be revealed".into())
    })
    .await
    .map_err(|_| "File reveal task failed")?
}

#[tauri::command]
pub async fn list_asset_workspace(app: tauri::AppHandle) -> Result<AssetWorkspace, String> {
    let manager = app.state::<AssetManager>().0.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        manager
            .lock()
            .map(|runtime| runtime.snapshot())
            .map_err(|_| "Asset state unavailable".to_string())
    })
    .await
    .map_err(|_| "Asset task unavailable")??;
    start_worker(&app);
    Ok(snapshot)
}

#[tauri::command]
pub async fn add_asset_files(
    app: tauri::AppHandle,
    user_id: u64,
    paths: Vec<String>,
    universe_id: Option<u64>,
    creator: Option<Creator>,
) -> Result<AssetWorkspace, String> {
    let cookie = credentials(&app, user_id).await?;
    let creator = creator.unwrap_or(Creator::User(user_id));
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    validate_creator(&client, &cookie, user_id, creator).await?;
    if let Some(universe_id) = universe_id {
        let universes = assets_api::list_manageable_universes(&client, &cookie)
            .await
            .map_err(|error| describe_error(&error))?;
        if !universes
            .iter()
            .any(|universe| universe.universe_id == universe_id && universe_id > 0)
        {
            return Err("Choose an experience this account can manage.".into());
        }
    }
    drop(cookie);
    if paths.len() > 100 {
        return Err("Import up to 100 files at a time.".into());
    }
    let manager = app.state::<AssetManager>().0.clone();
    let dialog_app = app.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<PathBuf> = if paths.is_empty() {
            dialog_app
                .dialog()
                .file()
                .set_title("Add asset files")
                .blocking_pick_files()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|path| path.into_path().ok())
                .collect()
        } else {
            paths.into_iter().map(PathBuf::from).collect()
        };
        if paths.len() > 100 {
            return Err("Import up to 100 files at a time.".into());
        }
        let mut records = Vec::new();
        for path in paths {
            if !path.is_absolute() {
                return Err("Choose an absolute file path.".into());
            }
            let (file, invalid_reason) = match assets::read_asset_file(&path) {
                Ok((bytes, kind, _)) => (
                    StagedFile {
                        path,
                        sha256: assets::sha256_hex(&bytes),
                        bytes: bytes.len() as u64,
                        kind,
                    },
                    None,
                ),
                Err(reason) => (
                    StagedFile {
                        path,
                        sha256: String::new(),
                        bytes: 0,
                        kind: assets::AssetKind::Other,
                    },
                    Some(reason),
                ),
            };
            let mut row = AssetRecord::staged(
                uuid::Uuid::new_v4().to_string(),
                file,
                creator,
                user_id,
                Utc::now(),
            );
            row.auto_grant_universe = universe_id;
            if let Some(reason) = invalid_reason {
                row.state = AssetState::Invalid { reason };
            }
            records.push(row);
        }
        let mut runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        let previous = runtime.index.clone();
        for mut row in records {
            if let Some(asset_id) = runtime
                .index
                .find_uploaded(&row.file_sha256, row.creator)
                .and_then(|found| found.state.asset_id())
            {
                row.state = AssetState::Duplicate { asset_id };
            }
            if runtime.index.records.iter().any(|existing| {
                !row.file_sha256.is_empty()
                    && existing.file_sha256 == row.file_sha256
                    && existing.creator == row.creator
                    && (existing.state.is_active() || matches!(existing.state, AssetState::Queued))
            }) {
                continue;
            }
            runtime.index.records.push(row);
        }
        runtime.persist(previous)?;
        Ok::<_, String>(runtime.snapshot())
    })
    .await
    .map_err(|_| "File import task unavailable")??;
    let _ = app.emit("assets-updated", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn change_asset_queue(
    app: tauri::AppHandle,
    user_id: u64,
    action: String,
    row_id: Option<String>,
) -> Result<AssetWorkspace, String> {
    let account_state = app.state::<crate::state::AppState>();
    {
        let runtime = account_state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked || runtime.accounts.find_by_id(user_id).is_none() {
            return Err("Select a saved account before changing its queue.".into());
        }
    }
    let manager = app.state::<AssetManager>().0.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        let previous = runtime.index.clone();
        let queue_action = match action.as_str() {
            "clearFinished" => assets::QueueAction::ClearFinished,
            "retry" => assets::QueueAction::Retry {
                row_id: row_id.as_deref().ok_or("Choose a queue row.")?,
            },
            "remove" => assets::QueueAction::Remove {
                row_id: row_id.as_deref().ok_or("Choose a queue row.")?,
            },
            _ => return Err("Unknown queue action.".into()),
        };
        change_account_queue(&mut runtime.index, user_id, queue_action)?;
        runtime.persist(previous)?;
        Ok::<_, String>(runtime.snapshot())
    })
    .await
    .map_err(|_| "Queue task unavailable")??;
    let _ = app.emit("assets-updated", &snapshot);
    Ok(snapshot)
}

fn change_account_queue(
    index: &mut AssetIndex,
    user_id: u64,
    action: assets::QueueAction<'_>,
) -> Result<(), String> {
    match action {
        assets::QueueAction::ClearFinished => {
            let mut owned = index.clone();
            owned.records.retain(|row| row.uploaded_by == user_id);
            assets::change_queue(&mut owned, assets::QueueAction::ClearFinished)?;
            let retained: HashSet<_> = owned
                .records
                .iter()
                .map(|row| row.row_id.as_str())
                .collect();
            index
                .records
                .retain(|row| row.uploaded_by != user_id || retained.contains(row.row_id.as_str()));
        }
        assets::QueueAction::Retry { row_id } | assets::QueueAction::Remove { row_id } => {
            let row = index.get(row_id).ok_or("Queue row not found.")?;
            if row.uploaded_by != user_id {
                return Err("Choose a queue row for the selected account.".into());
            }
            assets::change_queue(index, action)?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct AssetUniverse {
    id: u64,
    name: String,
}

#[tauri::command]
pub async fn list_asset_universes(
    app: tauri::AppHandle,
    user_id: u64,
) -> Result<Vec<AssetUniverse>, String> {
    let cookie = credentials(&app, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    assets_api::list_manageable_universes(&client, &cookie)
        .await
        .map(|universes| {
            universes
                .into_iter()
                .map(|universe| AssetUniverse {
                    id: universe.universe_id,
                    name: universe.name,
                })
                .collect()
        })
        .map_err(|error| describe_error(&error))
}

#[tauri::command]
pub async fn upload_assets(
    app: tauri::AppHandle,
    user_id: u64,
    row_ids: Vec<String>,
) -> Result<(), String> {
    drop(credentials(&app, user_id).await?);
    {
        let manager = app.state::<AssetManager>();
        let mut runtime = manager.0.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        let selected = assets::select_upload_rows(&runtime.index, user_id, &row_ids)?;
        runtime.upload_rows.extend(selected);
        let _ = app.emit("assets-updated", runtime.snapshot());
    }
    start_worker(&app);
    Ok(())
}

fn start_worker(app: &tauri::AppHandle) {
    let manager = app.state::<AssetManager>().0.clone();
    {
        let Ok(mut runtime) = manager.lock() else {
            return;
        };
        if runtime.is_read_only || runtime.is_worker_running {
            return;
        }
        runtime.is_worker_running = true;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let result = run_tick(&app, manager.clone()).await;
            let should_continue = {
                let Ok(mut runtime) = manager.lock() else {
                    return;
                };
                if result.is_err()
                    || runtime.is_read_only
                    || (runtime.upload_rows.is_empty() && !runtime.index.has_active())
                {
                    runtime.is_worker_running = false;
                    runtime.upload_rows.clear();
                    false
                } else {
                    true
                }
            };
            if !should_continue {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

async fn update_index(
    app: &tauri::AppHandle,
    manager: Arc<Mutex<AssetRuntime>>,
    update: impl FnOnce(&mut AssetRuntime) + Send + 'static,
) -> Result<(), String> {
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        let previous = runtime.index.clone();
        update(&mut runtime);
        runtime.persist(previous)?;
        Ok::<_, String>(runtime.snapshot())
    })
    .await
    .map_err(|_| "Asset task unavailable")?;
    match snapshot {
        Ok(snapshot) => {
            let _ = app.emit("assets-updated", snapshot);
            Ok(())
        }
        Err(error) => {
            if let Ok(runtime) = app.state::<AssetManager>().0.lock() {
                let _ = app.emit("assets-updated", runtime.snapshot());
            }
            Err(error)
        }
    }
}

fn apply_outcome(row: &mut AssetRecord, outcome: OperationOutcome) {
    row.updated_at = Some(Utc::now());
    row.state = match outcome {
        OperationOutcome::StillPending => return,
        OperationOutcome::Approved {
            asset_id,
            revision_id,
        } => AssetState::InReview {
            asset_id,
            revision_id,
            since: Utc::now(),
        },
        OperationOutcome::Rejected { .. } => AssetState::Rejected {
            reason: "Roblox rejected this asset during moderation.".into(),
        },
        OperationOutcome::Failed { retryable, .. } => AssetState::Failed {
            message: "Roblox could not finish this upload. Check Creator Dashboard for details."
                .into(),
            retryable,
        },
    };
}

async fn run_tick(app: &tauri::AppHandle, manager: Arc<Mutex<AssetRuntime>>) -> Result<(), String> {
    let queued = {
        let runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        runtime
            .index
            .records
            .iter()
            .find(|row| {
                runtime.upload_rows.contains(&row.row_id) && matches!(row.state, AssetState::Queued)
            })
            .cloned()
    };
    if let Some(row) = queued {
        send_upload(app, manager.clone(), row).await?;
    }
    let records = {
        let runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        runtime.index.records.clone()
    };
    for row in records
        .iter()
        .filter(|row| assets::is_poll_due(row))
        .take(20)
    {
        poll_row(app, manager.clone(), row).await?;
    }
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        let previous = runtime.index.clone();
        let expired = assets::expire_stale_operations(&mut runtime.index, Utc::now());
        let queued_rows: HashSet<String> = runtime
            .index
            .records
            .iter()
            .filter(|row| matches!(row.state, AssetState::Queued | AssetState::Uploading))
            .map(|row| row.row_id.clone())
            .collect();
        runtime
            .upload_rows
            .retain(|row_id| queued_rows.contains(row_id));
        if !expired.is_empty() {
            runtime.persist(previous)?;
        }
        Ok::<_, String>(runtime.snapshot())
    })
    .await
    .map_err(|_| "Asset task unavailable")??;
    let _ = app.emit("assets-updated", snapshot);
    Ok(())
}

async fn send_upload(
    app: &tauri::AppHandle,
    manager: Arc<Mutex<AssetRuntime>>,
    row: AssetRecord,
) -> Result<(), String> {
    let row_id = row.row_id.clone();
    let claim_manager = manager.clone();
    let claimed = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = claim_manager
            .lock()
            .map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        let previous = runtime.index.clone();
        let Some(row) = runtime.index.get_mut(&row_id) else {
            return Ok(false);
        };
        if !matches!(row.state, AssetState::Queued) {
            return Ok(false);
        }
        row.state = AssetState::Uploading;
        row.attempts = row.attempts.saturating_add(1);
        runtime.persist(previous)?;
        Ok::<_, String>(true)
    })
    .await
    .map_err(|_| "Upload task unavailable")??;
    if !claimed {
        return Ok(());
    }
    if let Ok(runtime) = manager.lock() {
        let _ = app.emit("assets-updated", runtime.snapshot());
    }
    let result = prepare_upload(app, &row).await;
    let row_id = row.row_id.clone();
    update_index(app, manager, move |runtime| {
        if let Some(row) = runtime.index.get_mut(&row_id) {
            match result {
                Ok(result) => {
                    if let Some(operation) = result.operation.filter(|operation| assets::is_valid_operation(operation)) {
                        row.state = AssetState::Pending { operation, since: Utc::now() };
                        apply_outcome(row, result.outcome);
                    } else if !matches!(result.outcome, OperationOutcome::StillPending) {
                        apply_outcome(row, result.outcome);
                    } else {
                        row.state = AssetState::Failed { message: "Roblox returned no valid upload operation. Check Creator Dashboard before importing again.".into(), retryable: false };
                    }
                }
                Err((message, retryable)) => row.state = AssetState::Failed { message, retryable },
            }
        }
    }).await
}

async fn prepare_upload(
    app: &tauri::AppHandle,
    row: &AssetRecord,
) -> Result<assets_api::CreateResult, (String, bool)> {
    let cookie = credentials(app, row.uploaded_by)
        .await
        .map_err(|message| (message, false))?;
    let client = RobloxClient::new().map_err(|_| ("Roblox client unavailable".into(), false))?;
    validate_creator(&client, &cookie, row.uploaded_by, row.creator)
        .await
        .map_err(|message| (message, false))?;
    let path = row.file_path.clone();
    let (bytes, _, mime) =
        tauri::async_runtime::spawn_blocking(move || assets::read_asset_file(&path))
            .await
            .map_err(|_| ("File task unavailable".into(), false))?
            .map_err(|message| (message, false))?;
    if assets::sha256_hex(&bytes) != row.file_sha256 {
        return Err((
            "The file changed after import. Remove this row and import it again.".into(),
            false,
        ));
    }
    let request = assets_api::UploadRequest {
        kind: row.kind,
        display_name: assets::sanitize_display_name(&row.display_name),
        description: row.description.chars().take(1000).collect(),
        creator: row.creator,
        file_name: row
            .file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        mime,
        bytes,
    };
    assets_api::create_asset(&client, &cookie, &request)
        .await
        .map_err(|error| {
            let retryable = matches!(
                error,
                CoreError::RateLimited | CoreError::RobloxApi { status: 429, .. }
            );
            (describe_error(&error), retryable)
        })
}

async fn poll_row(
    app: &tauri::AppHandle,
    manager: Arc<Mutex<AssetRuntime>>,
    row: &AssetRecord,
) -> Result<(), String> {
    let now = Utc::now();
    let Ok(cookie) = credentials(app, row.uploaded_by).await else {
        return Ok(());
    };
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let row_id = row.row_id.clone();
    match &row.state {
        AssetState::Pending { operation, .. } => {
            let outcome = if assets::is_valid_operation(operation) {
                assets_api::poll_operation(&client, &cookie, operation)
                    .await
                    .ok()
            } else {
                Some(OperationOutcome::Failed {
                    message: "Invalid upload operation".into(),
                    retryable: false,
                })
            };
            update_index(app, manager, move |runtime| {
                if let Some(row) = runtime.index.get_mut(&row_id) {
                    row.updated_at = Some(now);
                    if let Some(outcome) = outcome {
                        apply_outcome(row, outcome);
                    }
                }
            })
            .await
        }
        AssetState::InReview {
            asset_id,
            revision_id,
            ..
        } => {
            let verdicts = assets_api::fetch_moderation_statuses(&client, &cookie, &[*asset_id])
                .await
                .unwrap_or_default();
            let verdict = verdicts
                .into_iter()
                .find(|(id, _)| id == asset_id)
                .map(|(_, verdict)| verdict);
            let mut grant_confirmed = false;
            let mut grant_failed = false;
            if verdict == Some(ModerationStatus::Approved) {
                if let Some(universe_id) = row.auto_grant_universe {
                    match assets_api::grant_use_permission(
                        &client,
                        &cookie,
                        universe_id,
                        &[*asset_id],
                    )
                    .await
                    {
                        Ok(outcome) => {
                            grant_confirmed = outcome.granted.contains(asset_id);
                            grant_failed = !grant_confirmed;
                        }
                        Err(_) => grant_failed = true,
                    }
                }
            }
            let asset_id = *asset_id;
            let revision_id = *revision_id;
            let universe_id = row.auto_grant_universe;
            update_index(app, manager, move |runtime| {
                if let Some(row) = runtime.index.get_mut(&row_id) {
                    row.updated_at = Some(now);
                    match verdict {
                        Some(ModerationStatus::Approved) => row.state = AssetState::Approved { asset_id, revision_id },
                        Some(ModerationStatus::Rejected) => row.state = AssetState::Rejected { reason: "Roblox rejected this asset during moderation.".into() },
                        _ => {}
                    }
                    if grant_confirmed { if let Some(universe_id) = universe_id { row.granted_universes.push(universe_id); } }
                }
                if grant_failed { runtime.notice = Some("An asset was approved, but experience access could not be granted. Manage its permissions in Creator Dashboard.".into()); }
            }).await
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_actions_only_change_the_selected_accounts_rows() {
        let mut index = AssetIndex::default();
        for (row_id, user_id) in [("own", 1), ("other", 2)] {
            let mut row = AssetRecord::staged(
                row_id.into(),
                StagedFile {
                    path: PathBuf::from("synthetic.png"),
                    sha256: row_id.into(),
                    bytes: 1,
                    kind: assets::AssetKind::Decal,
                },
                Creator::User(user_id),
                user_id,
                Utc::now(),
            );
            row.state = AssetState::Failed {
                message: "Synthetic failure".into(),
                retryable: true,
            };
            index.records.push(row);
        }
        assert!(change_account_queue(
            &mut index,
            1,
            assets::QueueAction::Retry { row_id: "other" }
        )
        .is_err());
        assert!(change_account_queue(
            &mut index,
            1,
            assets::QueueAction::Remove { row_id: "other" }
        )
        .is_err());
        change_account_queue(&mut index, 1, assets::QueueAction::ClearFinished).unwrap();
        assert!(index.get("own").is_none());
        assert!(index.get("other").is_some());
    }

    #[tokio::test]
    async fn invalid_creators_are_rejected_before_any_request() {
        let client = RobloxClient::new().unwrap();
        assert!(
            validate_creator(&client, "synthetic-credential", 1, Creator::User(2))
                .await
                .is_err()
        );
        assert!(
            validate_creator(&client, "synthetic-credential", 1, Creator::Group(0))
                .await
                .is_err()
        );
    }

    #[test]
    fn ingestion_waits_for_a_moderation_verdict() {
        let mut row = AssetRecord::staged(
            "row".into(),
            StagedFile {
                path: PathBuf::from("test.png"),
                sha256: "hash".into(),
                bytes: 1,
                kind: assets::AssetKind::Decal,
            },
            Creator::User(1),
            1,
            Utc::now(),
        );
        apply_outcome(
            &mut row,
            OperationOutcome::Approved {
                asset_id: 42,
                revision_id: None,
            },
        );
        assert!(matches!(
            row.state,
            AssetState::InReview { asset_id: 42, .. }
        ));
        assert!(!summarize_row(&row).can_retry);
        assert!(!summarize_row(&row).can_remove);
    }

    #[test]
    fn external_error_details_never_cross_ipc() {
        let message = describe_error(&CoreError::RobloxApi {
            status: 403,
            message: "synthetic-secret".into(),
        });
        assert!(!message.contains("synthetic-secret"));
    }
}
