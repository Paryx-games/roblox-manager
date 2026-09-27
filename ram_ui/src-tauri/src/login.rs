use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use webview2_com::{
    take_pwstr, GetCookiesCompletedHandler,
    Microsoft::Web::WebView2::Win32::{ICoreWebView2CookieList, ICoreWebView2_2},
    ProcessFailedEventHandler,
};
use windows_core::{Interface, HSTRING, PCWSTR, PWSTR};

fn read_login_cookie(
    cookies: Option<ICoreWebView2CookieList>,
) -> windows_core::Result<Option<String>> {
    let Some(cookies) = cookies else {
        return Ok(None);
    };
    // webview2 exposes cookie access only through its native com interface
    unsafe {
        let mut count = 0;
        cookies.Count(&mut count)?;
        for index in 0..count {
            let cookie = cookies.GetValueAtIndex(index)?;
            let mut name = PWSTR::null();
            cookie.Name(&mut name)?;
            if take_pwstr(name) == ".ROBLOSECURITY" {
                let mut value = PWSTR::null();
                cookie.Value(&mut value)?;
                let value = take_pwstr(value);
                return Ok((!value.is_empty()).then_some(value));
            }
        }
    }
    Ok(None)
}

async fn request_cookie(window: &WebviewWindow) -> Result<Option<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    window.with_webview(move |native| {
        // return to the window loop immediately; waiting here can freeze webview2
        let requested = (|| -> windows_core::Result<()> {
            unsafe {
                let webview = native.controller().CoreWebView2()?.cast::<ICoreWebView2_2>()?;
                let address = HSTRING::from("https://www.roblox.com/");
                webview.CookieManager()?.GetCookies(
                    PCWSTR::from_raw(address.as_ptr()),
                    &GetCookiesCompletedHandler::create(Box::new(move |status, cookies| {
                        let result = status.and_then(|()| read_login_cookie(cookies));
                        let _ = sender.send(result.map_err(|_| "The Roblox login session could not be read. Close it and retry.".to_string()));
                        Ok(())
                    })),
                )?;
            }
            Ok(())
        })();
        if requested.is_err() {
            tracing::warn!(event = "login_cookie_request_failed", "Roblox login cookie request failed");
        }
    }).map_err(|_| "The Roblox login window is unavailable. Retry.".to_string())?;
    match tokio::time::timeout(Duration::from_secs(5), receiver).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("The Roblox login session could not be read. Retry.".into()),
        Err(_) => {
            tracing::warn!(
                event = "login_cookie_timeout",
                "Roblox login stopped responding"
            );
            Err(
                "The Roblox login window stopped responding. It was closed; please try again."
                    .into(),
            )
        }
    }
}

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
    let has_browser_failed = Arc::new(AtomicBool::new(false));
    let failed = Arc::clone(&has_browser_failed);
    window
        .with_webview(move |native| {
            // process notifications are native com callbacks and must never wait on the window
            let registered = (|| -> windows_core::Result<()> {
                unsafe {
                    let webview = native.controller().CoreWebView2()?;
                    let mut token = 0;
                    webview.add_ProcessFailed(
                        &ProcessFailedEventHandler::create(Box::new(move |_, _| {
                            failed.store(true, Ordering::Relaxed);
                            tracing::warn!(
                                event = "login_browser_failed",
                                "Roblox login browser process failed"
                            );
                            Ok(())
                        })),
                        &mut token,
                    )?;
                }
                Ok(())
            })();
            if registered.is_err() {
                tracing::warn!(
                    event = "login_monitor_failed",
                    "Roblox login process monitoring unavailable"
                );
            }
        })
        .map_err(|_| "The Roblox login window could not be monitored".to_string())?;
    tracing::info!(event = "login_opened", "Roblox login window opened");
    let mut interval = tokio::time::interval(Duration::from_millis(400));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = loop {
        interval.tick().await;
        if is_closed.load(Ordering::Relaxed) {
            break Ok(None);
        }
        if has_browser_failed.load(Ordering::Relaxed) {
            break Err("The Roblox login browser failed. It was closed; please try again.".into());
        }
        let cookies = request_cookie(&window).await;
        if is_closed.load(Ordering::Relaxed) {
            break Ok(None);
        }
        match cookies {
            Ok(Some(cookie)) => break Ok(Some(cookie)),
            Ok(None) => {}
            Err(error) => break Err(error),
        }
    };
    let _ = window.close();
    tracing::info!(
        event = "login_finished",
        outcome = match &result {
            Ok(Some(_)) => "authenticated",
            Ok(None) => "cancelled",
            Err(_) => "failed",
        },
        "Roblox login window finished"
    );
    result
}
