use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub async fn capture_cookie(app: &tauri::AppHandle) -> Result<Option<String>, String> {
    if let Some(window) = app.get_webview_window("account-login") {
        let _ = window.set_focus();
        return Err("An account login is already open. Finish or close it first.".into());
    }
    let login_url = "https://www.roblox.com/login"
        .parse::<tauri::Url>()
        .map_err(|_| "The Roblox login address is unavailable")?;
    let window = WebviewWindowBuilder::new(
        app,
        "account-login",
        WebviewUrl::External(login_url.clone()),
    )
    .title("Log in to Roblox")
    .inner_size(500.0, 720.0)
    .data_directory(crate::lifecycle::data_directory().join("tauri_login_profile"))
    .incognito(true)
    .on_navigation(|url| url.scheme() == "https")
    .build()
    .map_err(|_| "The Roblox login window could not be opened")?;
    let is_closed = Arc::new(AtomicBool::new(false));
    let closed = Arc::clone(&is_closed);
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
        ) {
            closed.store(true, Ordering::Relaxed);
        }
    });
    let mut interval = tokio::time::interval(Duration::from_millis(400));
    let result = loop {
        interval.tick().await;
        if is_closed.load(Ordering::Relaxed) {
            break Ok(None);
        }
        let cookie_window = window.clone();
        let cookie_url = login_url.clone();
        let cookies =
            tauri::async_runtime::spawn_blocking(move || cookie_window.cookies_for_url(cookie_url))
                .await;
        if is_closed.load(Ordering::Relaxed) {
            break Ok(None);
        }
        match cookies {
            Ok(Ok(cookies)) => {
                if let Some(cookie) = cookies
                    .into_iter()
                    .find(|cookie| cookie.name() == ".ROBLOSECURITY" && !cookie.value().is_empty())
                {
                    break Ok(Some(cookie.value().to_owned()));
                }
            }
            _ => {
                break Err("The Roblox login session could not be read. Close it and retry.".into())
            }
        }
    };
    let _ = window.close();
    result
}
