use chrono::Utc;
use ram_core::assets::{
    self, AssetIndex, AssetRecord, AssetState, Creator, IndexLoad, ModerationStatus,
    OperationOutcome, StagedFile,
};
use ram_core::{assets_api, auth::RobloxClient, error::CoreError};
use serde::Serialize;
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;

pub struct AssetManager(Arc<Mutex<AssetRuntime>>);

struct AssetRuntime {
    index: AssetIndex,
    path: PathBuf,
    is_read_only: bool,
    notice: Option<String>,
    upload_users: HashSet<u64>,
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
            upload_users: HashSet::new(),
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
            is_uploading: !self.upload_users.is_empty()
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
            self.upload_users.clear();
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

fn read_file(path: &Path) -> Result<(Vec<u8>, assets::AssetKind, &'static str), String> {
    let mut file = std::fs::File::open(path).map_err(|_| "The file could not be opened.")?;
    let metadata = file
        .metadata()
        .map_err(|_| "The file could not be inspected.")?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err("Choose a regular file smaller than 128 MiB.".into());
    }
    let (kind, mime) = assets::validate_file(path, metadata.len())?;
    assets::reject_unuploadable(kind)?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The file could not be read.")?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("The file exceeds the 128 MiB import limit.".into());
    }
    assets::validate_file(path, bytes.len() as u64)?;
    Ok((bytes, kind, mime))
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
) -> Result<AssetWorkspace, String> {
    let cookie = credentials(&app, user_id).await?;
    if let Some(universe_id) = universe_id {
        let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
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
            let (file, invalid_reason) = match read_file(&path) {
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
                Creator::User(user_id),
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
    action: String,
    row_id: Option<String>,
) -> Result<AssetWorkspace, String> {
    let manager = app.state::<AssetManager>().0.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        let previous = runtime.index.clone();
        match action.as_str() {
            "clearFinished" => runtime.index.records.retain(|row| {
                row.state.is_active()
                    || matches!(row.state, AssetState::Queued | AssetState::Approved { .. })
            }),
            "remove" | "retry" => {
                let row_id = row_id.ok_or("Choose a queue row.")?;
                let row = runtime
                    .index
                    .get_mut(&row_id)
                    .ok_or("Queue row not found.")?;
                if action == "retry" {
                    if !matches!(
                        row.state,
                        AssetState::Failed {
                            retryable: true,
                            ..
                        }
                    ) {
                        return Err("This upload cannot safely be retried.".into());
                    }
                    row.state = AssetState::Queued;
                } else {
                    if row.state.is_active() {
                        return Err("Wait for this upload to finish before removing it.".into());
                    }
                    runtime.index.remove(&row_id);
                }
            }
            _ => return Err("Unknown queue action.".into()),
        }
        runtime.persist(previous)?;
        Ok::<_, String>(runtime.snapshot())
    })
    .await
    .map_err(|_| "Queue task unavailable")??;
    let _ = app.emit("assets-updated", &snapshot);
    Ok(snapshot)
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
pub async fn upload_assets(app: tauri::AppHandle, user_id: u64) -> Result<(), String> {
    drop(credentials(&app, user_id).await?);
    {
        let manager = app.state::<AssetManager>();
        let mut runtime = manager.0.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        runtime.upload_users.insert(user_id);
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
                    || (runtime.upload_users.is_empty() && !runtime.index.has_active())
                {
                    runtime.is_worker_running = false;
                    runtime.upload_users.clear();
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
                runtime.upload_users.contains(&row.uploaded_by)
                    && matches!(row.state, AssetState::Queued)
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
    for row in records.iter().filter(|row| is_poll_due(row)).take(20) {
        poll_row(app, manager.clone(), row).await?;
    }
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = manager.lock().map_err(|_| "Asset state unavailable")?;
        if runtime.is_read_only {
            return Err("The asset index is read-only.".into());
        }
        let previous = runtime.index.clone();
        let expired = assets::expire_stale_operations(&mut runtime.index, Utc::now());
        let queued_users: HashSet<u64> = runtime
            .index
            .records
            .iter()
            .filter(|row| matches!(row.state, AssetState::Queued | AssetState::Uploading))
            .map(|row| row.uploaded_by)
            .collect();
        runtime
            .upload_users
            .retain(|user_id| queued_users.contains(user_id));
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
                    if let Some(operation) = result.operation.filter(|operation| is_valid_operation(operation)) {
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
    if row.creator != Creator::User(row.uploaded_by) {
        return Err((
            "This queue row has an unsupported creator. Import it again for your account.".into(),
            false,
        ));
    }
    let path = row.file_path.clone();
    let (bytes, kind, mime) = tauri::async_runtime::spawn_blocking(move || read_file(&path))
        .await
        .map_err(|_| ("File task unavailable".into(), false))?
        .map_err(|message| (message, false))?;
    if assets::sha256_hex(&bytes) != row.file_sha256 {
        return Err((
            "The file changed after import. Remove this row and import it again.".into(),
            false,
        ));
    }
    let client = RobloxClient::new().map_err(|_| ("Roblox client unavailable".into(), false))?;
    let request = assets_api::UploadRequest {
        kind,
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

fn is_valid_operation(operation: &str) -> bool {
    !operation.is_empty()
        && operation.len() <= 256
        && operation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn is_poll_due(row: &AssetRecord) -> bool {
    let now = Utc::now();
    let (since, interval) = match row.state {
        AssetState::Pending { since, .. } => (
            since,
            assets::poll_interval_for_age(
                now.signed_duration_since(since)
                    .to_std()
                    .unwrap_or_default(),
            ),
        ),
        AssetState::InReview { since, .. } => (
            since,
            assets::review_poll_interval_for_age(
                now.signed_duration_since(since)
                    .to_std()
                    .unwrap_or_default(),
            ),
        ),
        _ => return false,
    };
    if now
        .signed_duration_since(row.updated_at.unwrap_or(since))
        .to_std()
        .unwrap_or_default()
        < interval
    {
        return false;
    }
    true
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
            let outcome = if is_valid_operation(operation) {
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
    fn operation_ids_cannot_escape_the_endpoint_path() {
        assert!(is_valid_operation("abc-123_def"));
        for operation in [
            "",
            "../assets",
            "abc?token=x",
            "https://example.com",
            "abc/def",
        ] {
            assert!(!is_valid_operation(operation));
        }
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
