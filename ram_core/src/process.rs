//! Windows process management — game launching, mutex patching, instance tracking.
//!
//! # Multi-instance strategy
//!
//! Roblox prevents multiple clients by creating a named mutex
//! `ROBLOX_singletonEvent`. To allow multi-instancing we:
//!
//! 1. Enumerate all processes named `RobloxPlayerBeta.exe`.
//! 2. For each, enumerate its handles looking for the singleton mutex.
//! 3. Duplicate the handle into our process, then close both the remote and
//!    local copies — effectively releasing the mutex so the next launch succeeds.
//!
//! **This technique interacts with Hyperion (Byfron) and carries ban risk.**
//! It is gated behind `AppConfig::multi_instance_enabled` (default: off).

use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::error::CoreError;

// ---------------------------------------------------------------------------
// Privacy — clear Roblox cookie tracking file
// ---------------------------------------------------------------------------

/// Clear `RobloxCookies.dat` so Roblox cannot associate accounts across launches.
///
/// The file lives at `%LOCALAPPDATA%\Roblox\LocalStorage\RobloxCookies.dat`.
/// We truncate it to an empty file; Roblox will recreate it on the next launch
/// with only the current session's cookie.
pub fn clear_roblox_cookies() {
    let Ok(local_app_data) = std::env::var("LOCALAPPDATA") else {
        warn!("LOCALAPPDATA not set — cannot clear RobloxCookies.dat");
        return;
    };
    let path = PathBuf::from(local_app_data)
        .join("Roblox")
        .join("LocalStorage")
        .join("RobloxCookies.dat");

    if !path.exists() {
        debug!("RobloxCookies.dat does not exist, nothing to clear");
        return;
    }

    match std::fs::write(&path, b"") {
        Ok(()) => info!("Cleared RobloxCookies.dat for privacy"),
        Err(e) => warn!("Failed to clear RobloxCookies.dat: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Game launch via URI scheme
// ---------------------------------------------------------------------------

/// Build the `roblox-player://` URI and open it via the default handler.
///
/// `ticket` — the rbx-authentication-ticket from [`crate::auth::RobloxClient`].
/// `place_id` — numeric Roblox place ID.
/// `job_id` — optional server Job ID for joining a specific server.
/// `link_code` — optional private server link code.
/// `access_code` — optional UUID access code for private servers.
pub fn launch_game(
    ticket: &str,
    place_id: u64,
    job_id: Option<&str>,
    link_code: Option<&str>,
    access_code: Option<&str>,
) -> Result<(), CoreError> {
    let browser_tracker_id: u64 = rand::random::<u64>() % 1_000_000_000;
    let timestamp = chrono::Utc::now().timestamp_millis();

    let request_type = if link_code.is_some() {
        "RequestPrivateGame"
    } else {
        "RequestGame"
    };

    let mut uri = format!(
        "roblox-player:1+launchmode:play\
         +gameinfo:{ticket}\
         +launchtime:{timestamp}\
         +placelauncherurl:https%3A%2F%2Fassetgame.roblox.com%2Fgame%2FPlaceLauncher.ashx\
         %3Frequest%3D{request_type}\
         %26browserTrackerId%3D{browser_tracker_id}\
         %26placeId%3D{place_id}\
         %26isPlayTogetherGame%3Dfalse"
    );
    if let Some(jid) = job_id {
        uri.push_str(&format!("%26gameId%3D{jid}"));
    }
    if let Some(ac) = access_code {
        uri.push_str(&format!("%26accessCode%3D{ac}"));
    } else if let Some(code) = link_code {
        // Fallback: use linkCode as accessCode for old-format URLs.
        uri.push_str(&format!("%26accessCode%3D{code}"));
    }
    if let Some(lc) = link_code {
        uri.push_str(&format!("%26linkCode%3D{lc}"));
    }

    info!("Launching game - place {place_id}");
    // Never the raw URI: `gameinfo:` carries a live Roblox authentication
    // ticket, and this line runs under `RUST_LOG=debug` straight into rm.log,
    // which users paste into bug reports. The redacted form still shows how the
    // URI was assembled, which is the only reason to log it.
    debug!("URI: {}", crate::redact::scrub(&uri));

    open_uri(&uri)?;
    Ok(())
}

/// Shell-execute a URI (delegates to `cmd /C start`).
fn open_uri(uri: &str) -> Result<(), CoreError> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", uri])
        .spawn()
        .map_err(|e| CoreError::Process(format!("failed to open URI: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Roblox player path discovery
// ---------------------------------------------------------------------------

/// Attempt to locate the Roblox player executable.
pub fn find_roblox_player() -> Option<PathBuf> {
    // Standard install location under LocalAppData
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let versions_dir = PathBuf::from(&local).join("Roblox").join("Versions");
        if versions_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&versions_dir) {
                for entry in entries.flatten() {
                    let candidate = entry.path().join("RobloxPlayerBeta.exe");
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Process tracking
// ---------------------------------------------------------------------------

/// Image name of the Roblox client process. The bootstrapper and the installer
/// have other names; only this one is an actual game client.
pub const ROBLOX_PLAYER_EXE: &str = "RobloxPlayerBeta.exe";

/// PIDs of every running Roblox client.
///
/// The single source of truth for "what is running": [`is_roblox_running`],
/// [`roblox_instance_count`] and [`crate::instances`] all read from this, so
/// they cannot disagree about the same moment.
pub fn roblox_pids() -> std::collections::HashSet<u32> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes()
        .values()
        .filter(|p| p.name().to_string_lossy() == ROBLOX_PLAYER_EXE)
        .map(|p| p.pid().as_u32())
        .collect()
}

/// Check if any `RobloxPlayerBeta.exe` is currently running.
pub fn is_roblox_running() -> bool {
    !roblox_pids().is_empty()
}

/// Count how many Roblox player instances are running.
pub fn roblox_instance_count() -> usize {
    roblox_pids().len()
}

/// Kill all running Roblox player instances.
pub fn kill_all_roblox() -> Result<usize, CoreError> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let pids: Vec<_> = sys
        .processes()
        .iter()
        .filter(|(_, p)| p.name().to_string_lossy() == "RobloxPlayerBeta.exe")
        .map(|(pid, _)| *pid)
        .collect();
    let count = pids.len();
    for pid in &pids {
        if let Some(process) = sys.process(*pid) {
            process.kill();
        }
    }
    info!("Killed {count} Roblox instance(s)");
    Ok(count)
}

/// Kill Roblox processes that were launched with `--launch-to-tray` (background
/// "always running" instances). These stack up with multi-instance and aren't
/// associated with an actual game session.
pub fn kill_tray_roblox() -> usize {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut killed = 0usize;
    let roblox: Vec<_> = sys
        .processes()
        .iter()
        .filter(|(_, p)| p.name().to_string_lossy() == "RobloxPlayerBeta.exe")
        .collect();
    info!("kill_tray_roblox: found {} Roblox process(es)", roblox.len());
    let targets: Vec<_> = roblox
        .iter()
        .filter(|(_, p)| {
            let cmd = p.cmd();
            let args: Vec<String> = cmd.iter().map(|a| a.to_string_lossy().to_string()).collect();
            info!("  PID {} — cmd len={}, args: {:?}", p.pid(), cmd.len(), args);
            if !args.is_empty() {
                // sysinfo could read the command line — check directly
                return args.iter().any(|a| a.contains("--launch-to-tray"));
            }
            // sysinfo returned empty cmd (protected/elevated process).
            // Fall back to reading the command line directly from the PEB.
            let raw_pid = p.pid().as_u32();
            match native_get_cmdline(raw_pid) {
                Some(cmdline) => {
                    info!("  PID {} — native cmdline: {:?}", p.pid(), cmdline);
                    cmdline.contains("--launch-to-tray")
                }
                None => {
                    info!("  PID {} — native cmdline query also failed", p.pid());
                    false
                }
            }
        })
        .map(|(pid, p)| (*pid, p.pid()))
        .collect();
    for (pid, sysinfo_pid) in &targets {
        if let Some(process) = sys.process(**pid) {
            if process.kill() {
                info!("  Killed PID {} via sysinfo", sysinfo_pid);
                killed += 1;
            } else {
                // sysinfo kill failed (protected / elevated process) — fall back
                // to taskkill which may succeed depending on UAC configuration.
                info!("  sysinfo kill failed for PID {}, trying taskkill /F", sysinfo_pid);
                let raw: u32 = sysinfo_pid.as_u32();
                let res = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &raw.to_string()])
                    .output();
                match res {
                    Ok(o) if o.status.success() => {
                        info!("  taskkill succeeded for PID {}", sysinfo_pid);
                        killed += 1;
                    }
                    Ok(o) => {
                        info!(
                            "  taskkill failed for PID {}: {}",
                            sysinfo_pid,
                            String::from_utf8_lossy(&o.stderr).trim()
                        );
                    }
                    Err(e) => {
                        info!("  taskkill spawn error for PID {}: {e}", sysinfo_pid);
                    }
                }
            }
        }
    }
    if killed > 0 {
        info!("Killed {killed} tray Roblox process(es)");
    }
    killed
}

/// Read a process's command line directly from its PEB via the Win32 API.
/// This is the same technique System Informer / Process Hacker uses:
///   OpenProcess → NtQueryInformationProcess(ProcessBasicInformation) → PEB
///   → RTL_USER_PROCESS_PARAMETERS → CommandLine (UNICODE_STRING)
///   all read via ReadProcessMemory.
///
/// Works without admin privileges for same-user processes.
#[cfg(windows)]
fn native_get_cmdline(pid: u32) -> Option<String> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;

    // NtQueryInformationProcess is not in windows-sys, so we load it from ntdll.
    #[repr(C)]
    struct ProcessBasicInformation {
        exit_status: usize,
        peb_base_address: usize,
        affinity_mask: usize,
        base_priority: i32,
        unique_process_id: usize,
        inherited_from_unique_process_id: usize,
    }

    type NtQueryInformationProcessFn = unsafe extern "system" fn(
        process_handle: HANDLE,
        process_information_class: u32,
        process_information: *mut std::ffi::c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> i32;

    // Locate NtQueryInformationProcess in ntdll.dll
    let ntdll = unsafe {
        windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(c"ntdll.dll".as_ptr().cast())
    };
    if ntdll.is_null() {
        return None;
    }
    let fn_ptr = unsafe {
        windows_sys::Win32::System::LibraryLoader::GetProcAddress(
            ntdll,
            c"NtQueryInformationProcess".as_ptr().cast(),
        )
    };
    let nt_query: NtQueryInformationProcessFn = unsafe { std::mem::transmute(fn_ptr?) };

    // Open the target process
    let handle = unsafe {
        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid)
    };
    if handle.is_null() {
        info!("  native_get_cmdline: OpenProcess failed for PID {pid}");
        return None;
    }

    let result = (|| -> Option<String> {
        // Step 1: Get the PEB address via NtQueryInformationProcess
        let mut pbi: ProcessBasicInformation = unsafe { zeroed() };
        let status = unsafe {
            nt_query(
                handle,
                0, // ProcessBasicInformation
                &mut pbi as *mut _ as *mut std::ffi::c_void,
                size_of::<ProcessBasicInformation>() as u32,
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            info!("  native_get_cmdline: NtQueryInformationProcess failed: 0x{status:08x}");
            return None;
        }

        // Step 2: Read the PEB to find ProcessParameters pointer.
        // PEB layout (64-bit): offset 0x20 = ProcessParameters pointer
        // PEB layout (32-bit): offset 0x10 = ProcessParameters pointer
        let params_ptr_offset = if size_of::<usize>() == 8 { 0x20usize } else { 0x10usize };
        let mut process_params_addr: usize = 0;
        let mut bytes_read: usize = 0;
        let ok = unsafe {
            ReadProcessMemory(
                handle,
                (pbi.peb_base_address + params_ptr_offset) as *const std::ffi::c_void,
                &mut process_params_addr as *mut _ as *mut std::ffi::c_void,
                size_of::<usize>(),
                &mut bytes_read,
            )
        };
        if ok == FALSE || bytes_read != size_of::<usize>() {
            info!("  native_get_cmdline: ReadProcessMemory (PEB) failed");
            return None;
        }

        // Step 3: Read the CommandLine UNICODE_STRING from RTL_USER_PROCESS_PARAMETERS.
        // Offset to CommandLine: 0x70 on 64-bit, 0x40 on 32-bit
        let cmdline_offset = if size_of::<usize>() == 8 { 0x70usize } else { 0x40usize };

        // UNICODE_STRING: { Length: u16, MaximumLength: u16, padding(on 64-bit): u32, Buffer: *mut u16 }
        #[repr(C)]
        struct UnicodeString {
            length: u16,        // in bytes
            maximum_length: u16,
            _padding: u32,      // alignment padding on 64-bit
            buffer: usize,      // pointer
        }

        let mut us: UnicodeString = unsafe { zeroed() };
        let us_size = if size_of::<usize>() == 8 {
            // On 64-bit, UNICODE_STRING is 16 bytes (2+2+4 padding + 8 ptr)
            16usize
        } else {
            // On 32-bit, UNICODE_STRING is 8 bytes (2+2+4 ptr)
            8usize
        };
        let ok = unsafe {
            ReadProcessMemory(
                handle,
                (process_params_addr + cmdline_offset) as *const std::ffi::c_void,
                &mut us as *mut _ as *mut std::ffi::c_void,
                us_size,
                &mut bytes_read,
            )
        };
        if ok == FALSE || bytes_read != us_size {
            info!("  native_get_cmdline: ReadProcessMemory (UNICODE_STRING) failed");
            return None;
        }

        let char_count = us.length as usize / 2;
        if char_count == 0 || us.buffer == 0 {
            return None;
        }

        // Step 4: Read the actual command line string
        let mut buf = vec![0u16; char_count];
        let ok = unsafe {
            ReadProcessMemory(
                handle,
                us.buffer as *const std::ffi::c_void,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                us.length as usize,
                &mut bytes_read,
            )
        };
        if ok == FALSE {
            info!("  native_get_cmdline: ReadProcessMemory (string data) failed");
            return None;
        }

        Some(String::from_utf16_lossy(&buf))
    })();

    unsafe { CloseHandle(handle) };
    result
}

// ---------------------------------------------------------------------------
// Multi-instance mutex patching (Windows-only, opt-in)
// ---------------------------------------------------------------------------

/// Hold the Roblox singleton mutex in RM's own process so that Roblox cannot
/// acquire it exclusively. This allows multiple Roblox clients to coexist.
///
/// The original Roblox Account Manager uses the same technique: it creates
/// `ROBLOX_singletonMutex` before any Roblox client launches, pre-empting the
/// exclusive lock.
///
/// **This technique interacts with Hyperion (Byfron) and carries ban risk.**
/// It is gated behind `AppConfig::multi_instance_enabled` (default: off).
#[cfg(windows)]
mod multi_instance {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use tracing::info;

    /// Hold the singleton mutex handle for the lifetime of the program.
    static HELD_MUTEX: OnceLock<MutexHandle> = OnceLock::new();

    /// Wrapper so we can store a HANDLE in a static (HANDLE is *mut c_void, not
    /// Send/Sync by default, but we never dereference it across threads).
    struct MutexHandle(HANDLE);
    unsafe impl Send for MutexHandle {}
    unsafe impl Sync for MutexHandle {}

    /// Acquire the `ROBLOX_singletonMutex` and hold it for the process lifetime.
    /// Subsequent calls are no-ops (already held). Returns `true` if successfully
    /// acquired (or already held).
    pub fn acquire_singleton_mutex() -> bool {
        HELD_MUTEX.get_or_init(|| {
            let name: Vec<u16> = "ROBLOX_singletonMutex\0"
                .encode_utf16()
                .collect();
            let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
            if handle.is_null() {
                info!("Failed to create ROBLOX_singletonMutex");
            } else {
                info!("Acquired ROBLOX_singletonMutex — multi-instance enabled");
            }
            MutexHandle(handle)
        });
        HELD_MUTEX.get().is_some_and(|h| !h.0.is_null())
    }
}

#[cfg(windows)]
pub fn enable_multi_instance() -> Result<(), CoreError> {
    if multi_instance::acquire_singleton_mutex() {
        Ok(())
    } else {
        Err(CoreError::Process(
            "failed to acquire ROBLOX_singletonMutex".into(),
        ))
    }
}

#[cfg(not(windows))]
pub fn enable_multi_instance() -> Result<(), CoreError> {
    Err(CoreError::Process(
        "multi-instance is only supported on Windows".into(),
    ))
}

// ---------------------------------------------------------------------------
// Window arrangement — tile Roblox windows in a grid
// ---------------------------------------------------------------------------

/// Find all visible Roblox player windows and arrange them in a grid that
/// fills the primary monitor.  Layout: 1 → full, 2 → side-by-side,
/// 3 → top-two + bottom-center, 4 → 2×2, etc.
#[cfg(windows)]
pub fn arrange_roblox_windows() {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetSystemMetrics, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, SetWindowPos, ShowWindow, SM_CXSCREEN, SM_CYSCREEN,
        SWP_NOZORDER, SW_RESTORE,
    };

    // Collect HWNDs belonging to RobloxPlayerBeta.exe
    let roblox_pids: std::collections::HashSet<u32> = {
        use sysinfo::System;
        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        sys.processes()
            .values()
            .filter(|p| p.name().to_string_lossy() == "RobloxPlayerBeta.exe")
            .map(|p| p.pid().as_u32())
            .collect()
    };

    if roblox_pids.is_empty() {
        info!("arrange_roblox_windows: no Roblox processes found");
        return;
    }

    // EnumWindows callback state — passed through LPARAM as a raw pointer
    struct EnumState {
        pids: std::collections::HashSet<u32>,
        hwnds: Vec<HWND>,
    }

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam as *mut EnumState);
        if IsWindowVisible(hwnd) == 0 {
            return TRUE;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if !state.pids.contains(&pid) {
            return TRUE;
        }
        // Only match windows with a title (skip child/helper windows)
        let mut title = [0u16; 256];
        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), 256);
        if len > 0 {
            state.hwnds.push(hwnd);
        }
        TRUE
    }

    let mut state = EnumState {
        pids: roblox_pids,
        hwnds: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(enum_callback), &mut state as *mut EnumState as LPARAM);
    }

    let count = state.hwnds.len();
    if count == 0 {
        info!("arrange_roblox_windows: no visible Roblox windows found");
        return;
    }

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    // Query the invisible border size from the first window.  On Windows 10/11,
    // windows have ~7 px invisible borders on left/right/bottom that are part
    // of the window rect but transparent.  We compensate by extending each
    // SetWindowPos call past those invisible edges so windows snap flush.
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let (border_left, border_right, border_top, border_bottom) = {
        let hwnd0 = state.hwnds[0];
        // Temporarily position the window so we can measure it
        unsafe {
            ShowWindow(hwnd0, SW_RESTORE);
            SetWindowPos(hwnd0, std::ptr::null_mut(), 0, 0, 800, 600, SWP_NOZORDER);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut window_rect: RECT = unsafe { std::mem::zeroed() };
        let mut frame_rect: RECT = unsafe { std::mem::zeroed() };
        let got_rects = unsafe {
            let wr = GetWindowRect(hwnd0, &mut window_rect);
            let fr = DwmGetWindowAttribute(
                hwnd0,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                &mut frame_rect as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<RECT>() as u32,
            );
            wr != 0 && fr == 0
        };
        if got_rects {
            let bl = frame_rect.left - window_rect.left;
            let br = window_rect.right - frame_rect.right;
            let bt = frame_rect.top - window_rect.top;
            let bb = window_rect.bottom - frame_rect.bottom;
            info!("arrange: invisible borders: left={bl} right={br} top={bt} bottom={bb}");
            (bl, br, bt, bb)
        } else {
            info!("arrange: could not query DWM frame bounds, using zero borders");
            (0, 0, 0, 0)
        }
    };

    // Calculate grid dimensions
    let cols = (count as f64).sqrt().ceil() as i32;
    let rows = ((count as f64) / cols as f64).ceil() as i32;
    let cell_w = screen_w / cols;
    let cell_h = screen_h / rows;

    info!("arrange_roblox_windows: tiling {count} window(s) in {cols}×{rows} grid ({cell_w}×{cell_h} each)");

    for (i, &hwnd) in state.hwnds.iter().enumerate() {
        let col = i as i32 % cols;
        let row = i as i32 / cols;
        let x = col * cell_w;
        let y = row * cell_h;

        // For the last row, if there are fewer windows than columns, center them
        let windows_in_last_row = count as i32 - (rows - 1) * cols;
        let (x, w) = if row == rows - 1 && windows_in_last_row < cols {
            let last_col = i as i32 - (rows - 1) * cols;
            let total_width = windows_in_last_row * cell_w;
            let offset = (screen_w - total_width) / 2;
            (offset + last_col * cell_w, cell_w)
        } else {
            (x, cell_w)
        };

        // Expand placement to compensate for invisible borders so windows snap flush.
        // Left edge: move left by border_left  (except if at screen left edge)
        // Right edge: expand width by border_left + border_right
        // Top/bottom: same logic vertically.
        let adj_x = x - border_left;
        let adj_y = y - border_top;
        let adj_w = w + border_left + border_right;
        let adj_h = cell_h + border_top + border_bottom;

        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetWindowPos(hwnd, std::ptr::null_mut(), adj_x, adj_y, adj_w, adj_h, SWP_NOZORDER);
        }
    }

    info!("arrange_roblox_windows: done");
}

#[cfg(not(windows))]
pub fn arrange_roblox_windows() {
    info!("Window arrangement is only supported on Windows");
}
