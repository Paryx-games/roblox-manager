//! Subprocess-based browser login to Roblox.
//!
//! `wry` + `tao` don't coexist with `eframe`'s main-thread `winit` event loop
//! in the same process — even with `EventLoopBuilderExtWindows::with_any_thread`,
//! `WindowBuilder::build` dies with a native Win32 exception when there's
//! already a winit-owned window pump elsewhere in the process. Workaround:
//! re-exec our own binary with a hidden flag so the child has a genuine main
//! thread to host the webview on. The child writes the captured cookie to a
//! file we hand it, then exits; the parent waits on the child and reads that
//! file to produce a [`LoginOutcome`].
//!
//! The wire format is trivially simple: the outfile only exists on success and
//! contains the raw `.ROBLOSECURITY` value. Anything else (missing file, empty
//! file, child exit != 0) is treated as cancel/failure.
//!
//! `main()` dispatches the child mode via [`FLAG`] before `eframe::run_native`
//! so the normal UI never initializes in the child process.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use tracing::{info, warn};
use wry::{WebContext, WebViewBuilder};

/// CLI flag that switches `main()` into child webview mode.
/// Invoked as: `ram_ui.exe --browser-login <profile_dir> <outfile>`.
pub const FLAG: &str = "--browser-login";

/// CLI flag for the "Open browser as <account>" child mode.
/// Invoked as: `ram_ui.exe --browse-as <profile_dir> <cookie_file>`.
pub const BROWSE_AS_FLAG: &str = "--browse-as";

const LOGIN_URL: &str = "https://www.roblox.com/login";
/// First page the browse-as child navigates to. Picked so it's small and
/// always loads — its only purpose is to give us a roblox.com origin from
/// which to install the auth cookie via `document.cookie`. The init script
/// then immediately redirects away before the login form renders.
const BROWSE_AS_BOOT_URL: &str = "https://www.roblox.com/login";
/// Path component of [`BROWSE_AS_BOOT_URL`] — used by the init script to
/// detect the bootstrap navigation and skip the redirect on subsequent ones.
const BROWSE_AS_BOOT_PATH: &str = "/login";
const BROWSE_AS_HOME_URL: &str = "https://www.roblox.com/home";
const POLL_INTERVAL: Duration = Duration::from_millis(400);

pub enum LoginOutcome {
    Success(String),
    Cancelled,
    Failed(String),
}

// ---------------------------------------------------------------------------
// Parent side — spawn the helper subprocess and deliver its result.
// ---------------------------------------------------------------------------

pub fn spawn(profile_dir: PathBuf, tx: Sender<LoginOutcome>) {
    std::thread::spawn(move || {
        let outcome = match spawn_and_wait(profile_dir) {
            Ok(o) => o,
            Err(e) => {
                warn!("browser_login parent: {e}");
                LoginOutcome::Failed(e)
            }
        };
        let _ = tx.send(outcome);
    });
}

fn spawn_and_wait(profile_dir: PathBuf) -> Result<LoginOutcome, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let outfile = profile_dir.join("cookie.out");
    // Clear any leftover from a prior attempt so `exists()` means "this run"
    let _ = std::fs::remove_file(&outfile);

    info!("browser_login parent: spawning child {}", exe.display());
    let status = std::process::Command::new(&exe)
        .arg(FLAG)
        .arg(&profile_dir)
        .arg(&outfile)
        .status()
        .map_err(|e| format!("spawn child: {e}"))?;

    if !status.success() {
        return Err(format!("child exited unsuccessfully: {status}"));
    }

    match std::fs::read_to_string(&outfile) {
        Ok(cookie) if !cookie.trim().is_empty() => {
            let cookie = cookie.trim().to_string();
            let _ = std::fs::remove_file(&outfile);
            Ok(LoginOutcome::Success(cookie))
        }
        _ => Ok(LoginOutcome::Cancelled),
    }
}

// ---------------------------------------------------------------------------
// Child side — runs on this process's main thread, hosts the webview, exits.
// ---------------------------------------------------------------------------

/// Entry point for the child process. Blocks until the user logs in or closes
/// the window, then returns an exit code for `std::process::exit`.
pub fn run_child(profile_dir: PathBuf, outfile: PathBuf) -> i32 {
    match run_child_inner(profile_dir, outfile) {
        Ok(()) => 0,
        Err(e) => {
            warn!("browser_login child: {e}");
            1
        }
    }
}

fn run_child_inner(profile_dir: PathBuf, outfile: PathBuf) -> Result<(), String> {
    info!(
        "browser_login child: start, profile={}, out={}",
        profile_dir.display(),
        outfile.display()
    );
    std::fs::create_dir_all(&profile_dir).map_err(|e| format!("create profile dir: {e}"))?;

    let event_loop = EventLoopBuilder::<()>::new().build();

    let window = WindowBuilder::new()
        .with_title("Log in to Roblox")
        .with_inner_size(tao::dpi::LogicalSize::new(500.0, 720.0))
        .build(&event_loop)
        .map_err(|e| format!("window build: {e}"))?;

    let mut web_context = WebContext::new(Some(profile_dir));

    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(LOGIN_URL)
        .build(&window)
        .map_err(|e| format!("webview build: {e}"))?;

    info!("browser_login child: entering event loop");
    let mut next_poll = Instant::now() + POLL_INTERVAL;
    let mut done = false;

    // `run` diverges (`-> !`) — the process exits when we request
    // ControlFlow::Exit, so there's no code after this point.
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(next_poll);

        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if !done {
                    if let Some(cookie) = try_extract_cookie(&webview) {
                        info!("browser_login child: cookie captured, writing outfile");
                        if let Err(e) = std::fs::write(&outfile, &cookie) {
                            warn!("failed to write cookie outfile: {e}");
                        }
                        done = true;
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
                next_poll = Instant::now() + POLL_INTERVAL;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}

fn try_extract_cookie(webview: &wry::WebView) -> Option<String> {
    let cookies = webview.cookies().ok()?;
    cookies.into_iter().find_map(|c| {
        if c.name() == ".ROBLOSECURITY" && !c.value().is_empty() {
            Some(c.value().to_string())
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// "Open browser as" — fire-and-forget child window pre-loaded with an
// account's cookie. Unlike the login flow there's no value to return: the
// parent spawns it detached and the user closes the window when done.
// ---------------------------------------------------------------------------

/// Parent-side: spawn a detached child window logged in as the given cookie.
/// The cookie is handed over via a one-shot file inside `profile_dir`; the
/// child deletes that file as its first action so the secret never lives on
/// disk longer than the spawn race. `label` is the username (or anon tag)
/// shown in the window title.
pub fn spawn_browse_as(profile_dir: PathBuf, cookie: String, label: String) -> Result<(), String> {
    spawn_browse_as_to(profile_dir, cookie, label, None)
}

/// Parent-side variant that opens the authenticated webview at a destination URL.
pub fn spawn_browse_as_to(
    profile_dir: PathBuf,
    cookie: String,
    label: String,
    destination_url: Option<String>,
) -> Result<(), String> {
    std::fs::create_dir_all(&profile_dir).map_err(|e| format!("create profile dir: {e}"))?;
    let cookie_in = profile_dir.join("cookie.in");
    // Clear any leftover from a previous failed spawn before writing fresh.
    let _ = std::fs::remove_file(&cookie_in);
    std::fs::write(&cookie_in, cookie.as_bytes()).map_err(|e| format!("write cookie file: {e}"))?;

    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    info!("browse_as parent: spawning child {}", exe.display());
    std::process::Command::new(&exe)
        .arg(BROWSE_AS_FLAG)
        .arg(&profile_dir)
        .arg(&cookie_in)
        .arg(&label)
        .args(destination_url.into_iter())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| {
            // Best-effort cleanup if the spawn itself failed
            let _ = std::fs::remove_file(&cookie_in);
            format!("spawn child: {e}")
        })?;
    Ok(())
}

/// Entry point for the browse-as child. Returns an exit code for `std::process::exit`.
pub fn run_browse_as_child(profile_dir: PathBuf, cookie_in: PathBuf, label: String) -> i32 {
    match run_browse_as_inner(profile_dir, cookie_in, label, None) {
        Ok(()) => 0,
        Err(e) => {
            warn!("browse_as child: {e}");
            1
        }
    }
}

pub fn run_browse_as_child_to(
    profile_dir: PathBuf,
    cookie_in: PathBuf,
    label: String,
    destination_url: String,
) -> i32 {
    match run_browse_as_inner(profile_dir, cookie_in, label, Some(destination_url)) {
        Ok(()) => 0,
        Err(e) => {
            warn!("browse_as child: {e}");
            1
        }
    }
}

fn run_browse_as_inner(
    profile_dir: PathBuf,
    cookie_in: PathBuf,
    label: String,
    destination_url: Option<String>,
) -> Result<(), String> {
    info!("browse_as child: start, profile={}", profile_dir.display());

    // Read and immediately delete the cookie hand-off file.
    let cookie_value =
        std::fs::read_to_string(&cookie_in).map_err(|e| format!("read cookie file: {e}"))?;
    let _ = std::fs::remove_file(&cookie_in);
    let cookie_value = cookie_value.trim().to_string();
    if cookie_value.is_empty() {
        return Err("empty cookie hand-off".into());
    }

    std::fs::create_dir_all(&profile_dir).map_err(|e| format!("create profile dir: {e}"))?;

    let event_loop = EventLoopBuilder::<()>::new().build();

    let title = if label.is_empty() {
        "Browsing as account".to_string()
    } else {
        format!("Browsing as {label}")
    };
    let window = WindowBuilder::new()
        .with_title(&title)
        .with_inner_size(tao::dpi::LogicalSize::new(1100.0, 800.0))
        .build(&event_loop)
        .map_err(|e| format!("window build: {e}"))?;

    let mut web_context = WebContext::new(Some(profile_dir));

    // WebView2's CookieManager.CreateCookie quietly stores any cookie whose
    // Domain attribute matches the host as host-only — Domain=roblox.com never
    // gets sent to www.roblox.com, and Domain=.roblox.com fares no better. So
    // we install the cookie from inside roblox.com's own origin via
    // document.cookie, which the underlying Chromium parser handles per
    // RFC 6265 (treating any Domain as covering subdomains).
    //
    // To avoid flashing the login form, we register the cookie+redirect as an
    // initialization script that runs BEFORE the page's own scripts. The first
    // load lands on /login (boot URL), the init script installs the cookie and
    // immediately `location.replace`s to /home, so the login UI never gets a
    // chance to render. The init script runs on subsequent navigations too,
    // but a path check prevents a redirect loop.
    let cookie_js_literal = serde_json::to_string(&cookie_value)
        .map_err(|e| format!("serialize cookie for JS: {e}"))?;
    let boot_path_js = serde_json::to_string(BROWSE_AS_BOOT_PATH).unwrap();
    let destination_url = destination_url
        .filter(|url| url.starts_with("https://www.roblox.com/"))
        .unwrap_or_else(|| BROWSE_AS_HOME_URL.to_string());
    let home_url_js = serde_json::to_string(&destination_url).unwrap();
    let init_script = format!(
        r#"(function(){{
            try {{
                document.cookie = ".ROBLOSECURITY=" + {cookie_js_literal} +
                    "; path=/; domain=.roblox.com; secure; samesite=lax";
            }} catch (e) {{}}
            try {{
                if (location.pathname.toLowerCase() === {boot_path_js}) {{
                    location.replace({home_url_js});
                }}
            }} catch (e) {{}}
        }})();"#,
    );

    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(BROWSE_AS_BOOT_URL)
        .with_initialization_script(&init_script)
        .build(&window)
        .map_err(|e| format!("webview build: {e}"))?;

    info!("browse_as child: entering event loop");
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        // `webview` is moved into this closure so it isn't dropped early —
        // dropping it would tear down the window.
        let _ = &webview;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    })
}
