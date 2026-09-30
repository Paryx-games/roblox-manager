use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use webview2_com::ProcessFailedEventHandler;

#[derive(Default)]
pub struct RecoveryState {
    is_recovering: AtomicBool,
    attempts: AtomicUsize,
}

impl RecoveryState {
    pub fn is_recovering(&self) -> bool {
        self.is_recovering.load(Ordering::Relaxed)
    }
}

fn needs_recovery(kind: i32) -> bool {
    matches!(kind, 0..=2)
}

pub fn monitor(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
    let app = app.clone();
    window.with_webview(move |native| {
        let callback_app = app.clone();
        // webview2 process notifications require its native com interface
        let registered = (|| -> windows_core::Result<()> {
            unsafe {
                let webview = native.controller().CoreWebView2()?;
                let mut token = 0;
                webview.add_ProcessFailed(
                    &ProcessFailedEventHandler::create(Box::new(move |_, arguments| {
                        let mut kind = Default::default();
                        if let Some(arguments) = arguments {
                            arguments.ProcessFailedKind(&mut kind)?;
                        }
                        tracing::error!(
                            event = "main_webview_process_failed",
                            kind = kind.0,
                            "RM interface browser process failed"
                        );
                        if needs_recovery(kind.0) {
                            request_recovery(&callback_app);
                        }
                        Ok(())
                    })),
                    &mut token,
                )?;
            }
            Ok(())
        })();
        if let Err(error) = registered {
            tracing::error!(
                event = "main_webview_monitor_failed",
                code = error.code().0,
                "RM interface browser is unavailable"
            );
            request_recovery(&app);
        }
    })
}

fn request_recovery(app: &AppHandle) {
    let state = app.state::<RecoveryState>();
    if state
        .is_recovering
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let attempt = state.attempts.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tracing::warn!(
            event = "main_webview_recovery_started",
            attempt,
            "Restoring RM interface after a browser failure"
        );
        let result = restore_window(&app, attempt).await;
        app.state::<RecoveryState>()
            .is_recovering
            .store(false, Ordering::Relaxed);
        if result.is_err() {
            tracing::error!(
                event = "main_webview_recovery_failed",
                attempt,
                "RM interface could not be restored"
            );
            let dialog_app = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                dialog_app.dialog().message("RM's WebView2 browser failed and the interface could not be restored. Restart RM. If it happens again, repair Microsoft Edge WebView2 Runtime. Your encrypted account store has not been reset.").title("RM interface needs attention").kind(MessageDialogKind::Error).blocking_show();
            }).await;
            app.exit(1);
        } else {
            tracing::info!(
                event = "main_webview_recovery_completed",
                attempt,
                "RM interface restored; account store and background services retained"
            );
        }
    });
}

async fn restore_window(app: &AppHandle, attempt: usize) -> Result<(), String> {
    let old_window = app.get_webview_window("main");
    let size = old_window
        .as_ref()
        .and_then(|window| window.inner_size().ok());
    let position = old_window
        .as_ref()
        .and_then(|window| window.outer_position().ok());
    if let Some(window) = old_window {
        window
            .destroy()
            .map_err(|_| "Failed interface could not be closed")?;
    }
    if attempt > 2 {
        return Err("Browser repeatedly failed".into());
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        while app.get_webview_window("main").is_some() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "Failed interface did not close")?;
    let configuration = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .ok_or("Main window configuration unavailable")?;
    let profile = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Browser data directory unavailable")?
        .join("recovery-webviews")
        .join(uuid::Uuid::new_v4().to_string());
    let window = WebviewWindowBuilder::from_config(app, configuration)
        .map_err(|_| "Main window configuration invalid")?
        .data_directory(profile)
        .build()
        .map_err(|_| "Replacement browser unavailable")?;
    if let Some(size) = size {
        let _ = window.set_size(size);
    }
    if let Some(position) = position {
        let _ = window.set_position(position);
    }
    monitor(app, &window).map_err(|_| "Replacement browser monitoring unavailable")?;
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_main_browser_and_renderer_failures_recreate_the_interface() {
        for kind in [0, 1, 2] {
            assert!(needs_recovery(kind));
        }
        for kind in [3, 4, 5, 6, 7, 8, 9] {
            assert!(!needs_recovery(kind));
        }
    }
}
