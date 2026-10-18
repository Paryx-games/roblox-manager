use crate::state::AppState;
use ram_core::{
    account_export::{self, ExportAccount, ExportOptions},
    crypto, storage,
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri_plugin_dialog::DialogExt;

fn resolved_path(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        path.parent()
            .and_then(|parent| std::fs::canonicalize(parent).ok())
            .and_then(|parent| path.file_name().map(|name| parent.join(name)))
            .unwrap_or_else(|| path.to_path_buf())
    })
}

fn safe_destination(
    path: &std::path::Path,
    config_path: &std::path::Path,
    accounts_path: &std::path::Path,
) -> bool {
    let destination = resolved_path(path).to_string_lossy().to_lowercase();
    let accounts = resolved_path(accounts_path)
        .to_string_lossy()
        .to_lowercase();
    let config_dir = config_path.parent().map(resolved_path);
    destination != accounts
        && destination != format!("{accounts}.bak")
        && !config_dir.is_some_and(|directory| {
            let directory = directory.to_string_lossy().to_lowercase();
            destination == directory
                || destination.starts_with(&format!("{}\\", directory.trim_end_matches('\\')))
        })
}

struct ExportGuard(Arc<AtomicBool>);
impl Drop for ExportGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    count: usize,
    encrypted: bool,
}

#[tauri::command]
pub async fn export_accounts(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    options: ExportOptions,
) -> Result<Option<ExportResult>, String> {
    options.validate().map_err(|error| error.to_string())?;
    let state = state.inner().clone();
    state
        .export_in_progress
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "An account export is already open.")?;
    let guard = ExportGuard(state.export_in_progress.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let extension = if options.encryption.recommended() {
            "rmexport"
        } else {
            options.format.extension()
        };
        let path = app
            .dialog()
            .file()
            .set_title("Export accounts to")
            .set_file_name(options.filename())
            .add_filter("Account export", &[extension])
            .blocking_save_file();
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path
            .into_path()
            .map_err(|_| "Choose a local file for the account export.")?;
        let (accounts, revisions, session, credential_manager) = {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable")?;
            if !runtime.unlocked || state.is_shutting_down.load(Ordering::Acquire) {
                return Err("Unlock the account store before exporting.".into());
            }
            if !safe_destination(&path, &runtime.config_path, &runtime.config.accounts_path) {
                return Err(
                    "Choose an export destination outside RM's data directory and account store."
                        .into(),
                );
            }
            let accounts = options
                .account_ids
                .iter()
                .map(|id| {
                    runtime
                        .accounts
                        .find_by_id(*id)
                        .cloned()
                        .ok_or("An export account was removed. Select the accounts again.")
                })
                .collect::<Result<Vec<_>, _>>()?;
            let revisions = options
                .account_ids
                .iter()
                .map(|id| *runtime.credential_revisions.get(id).unwrap_or(&0))
                .collect::<Vec<_>>();
            (
                accounts,
                revisions,
                if options.includes_cookies() {
                    runtime.session.clone()
                } else {
                    None
                },
                runtime.config.use_credential_manager,
            )
        };
        let mut rows = Vec::with_capacity(accounts.len());
        for account in accounts {
            let cookie = if options.includes_cookies() {
                let cookie = if credential_manager {
                    crypto::credential_load(account.user_id)
                } else {
                    let encrypted = account
                        .encrypted_cookie
                        .as_deref()
                        .ok_or("An account credential is unavailable. No export was written.")?;
                    let session = session
                        .as_ref()
                        .ok_or("The account store is locked. No export was written.")?;
                    crypto::decrypt_cookie(encrypted, session)
                }
                .map_err(|_| "An account credential could not be read. No export was written.")?;
                Some(cookie)
            } else {
                None
            };
            rows.push(ExportAccount { account, cookie });
        }
        let bytes = account_export::encode(&options, &rows).map_err(|error| error.to_string())?;
        drop(options.password);
        drop(rows);
        drop(session);
        // Keep account removal, locking and credential changes out of the final commit window.
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if !runtime.unlocked
            || state.is_shutting_down.load(Ordering::Acquire)
            || !safe_destination(&path, &runtime.config_path, &runtime.config.accounts_path)
            || options
                .account_ids
                .iter()
                .zip(&revisions)
                .any(|(id, revision)| {
                    runtime.accounts.find_by_id(*id).is_none()
                        || runtime.credential_revisions.get(id).unwrap_or(&0) != revision
                })
        {
            return Err(
                "The selected accounts changed. No export was written; select them again.".into(),
            );
        }
        storage::atomic_swap(&path, &bytes)
            .map_err(|_| "The export could not be saved. Check the destination and try again.")?;
        Ok(Some(ExportResult {
            count: options.account_ids.len(),
            encrypted: options.encryption.recommended(),
        }))
    })
    .await
    .map_err(|_| "The account export task failed.")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exports_cannot_replace_live_state_or_its_backup() {
        let base = std::env::temp_dir().join(format!("rm-export-paths-{}", uuid::Uuid::new_v4()));
        let data = base.join("RM");
        std::fs::create_dir_all(&data).unwrap();
        let config = data.join("config.json");
        let accounts = base.join("accounts.dat");
        assert!(!safe_destination(&config, &config, &accounts));
        assert!(!safe_destination(
            &data.join("export.json"),
            &config,
            &accounts
        ));
        assert!(!safe_destination(&accounts, &config, &accounts));
        assert!(!safe_destination(
            &base.join("accounts.dat.bak"),
            &config,
            &accounts
        ));
        assert!(safe_destination(
            &base.join("export.json.rmexport"),
            &config,
            &accounts
        ));
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn export_ownership_is_released_when_the_operation_ends() {
        let busy = Arc::new(AtomicBool::new(true));
        {
            let _guard = ExportGuard(busy.clone());
            assert!(busy.load(Ordering::Acquire));
        }
        assert!(!busy.load(Ordering::Acquire));
    }
}
