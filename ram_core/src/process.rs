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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use tracing::{debug, info, warn};

use crate::error::CoreError;
use crate::instances::{classify_cmdline, LiveClient};

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

/// The last `launchtime` this process handed out.
///
/// `launchtime` is the whole basis of [`crate::instances`] attribution: it is
/// stamped into the launch URI, reaches the spawned client's command line, and
/// is looked up there. That only works if it is unique, and the obvious source
/// (`Utc::now().timestamp_millis()`) is not: two launches in the same
/// millisecond collide, and a bulk launch fires them back to back.
static LAST_LAUNCHTIME: AtomicI64 = AtomicI64::new(0);

/// Mint a `launchtime` that no other launch in this process will ever reuse.
///
/// Normally the wall clock in milliseconds. When the clock has not advanced
/// since the last call (or has gone backwards over an NTP correction) it
/// returns one past the previous value instead of a duplicate. Uniqueness is
/// guaranteed outright rather than merely likely, because a collision would
/// silently attribute one account's client to another.
///
/// Call this immediately before [`launch_game`] and register the mapping with
/// [`crate::instances::InstanceRegistry::note_launch`] before the launch goes
/// out, so a sweep landing in between cannot see the client first.
pub fn next_launchtime() -> i64 {
    let now = chrono::Utc::now().timestamp_millis();
    // `fetch_update` retries its compare-exchange internally, so two threads
    // racing here still come away with different numbers. The closure never
    // returns `None`, so the `Result` cannot actually be `Err`.
    let previous = LAST_LAUNCHTIME
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last| {
            Some(if now > last { now } else { last + 1 })
        })
        .unwrap_or(now);
    if now > previous {
        now
    } else {
        previous + 1
    }
}

/// Assemble the `placelauncherurl` query for one launch.
///
/// Three shapes, each mirroring one that has been observed working rather than
/// assembled from first principles:
///
/// * **`RequestGameJob`** when a job ID is given. This is what the Roblox web
///   client itself emits when you join a specific server, down to the
///   `joinAttemptId` and the `www.roblox.com` host. RM used to send
///   `RequestGame` with a `gameId` bolted on, a form that appears nowhere in
///   any captured launch and was never verified to place the client in the
///   requested server at all.
/// * **`RequestPrivateGame`** for private servers, unchanged.
/// * **`RequestGame`** for a plain place launch, unchanged. This is the form
///   RM has always sent and the one every captured RM launch used.
fn place_launcher_query(
    place_id: u64,
    job_id: Option<&str>,
    link_code: Option<&str>,
    access_code: Option<&str>,
) -> String {
    let browser_tracker_id: u64 = rand::random::<u64>() % 1_000_000_000;

    // A specific server, and not a private one: the web client's job-join form.
    if let (Some(jid), None) = (job_id, link_code) {
        let join_attempt = uuid::Uuid::new_v4();
        return format!(
            "https%3A%2F%2Fwww.roblox.com%2Fgame%2FPlaceLauncher.ashx\
             %3Frequest%3DRequestGameJob\
             %26browserTrackerId%3D{browser_tracker_id}\
             %26placeId%3D{place_id}\
             %26gameId%3D{jid}\
             %26joinAttemptId%3D{join_attempt}"
        );
    }

    let request_type = if link_code.is_some() {
        "RequestPrivateGame"
    } else {
        "RequestGame"
    };
    let mut query = format!(
        "https%3A%2F%2Fassetgame.roblox.com%2Fgame%2FPlaceLauncher.ashx\
         %3Frequest%3D{request_type}\
         %26browserTrackerId%3D{browser_tracker_id}\
         %26placeId%3D{place_id}\
         %26isPlayTogetherGame%3Dfalse"
    );
    if let Some(jid) = job_id {
        // Only reachable alongside a link code, where the private-server
        // request already names the server and the job ID is supplementary.
        query.push_str(&format!("%26gameId%3D{jid}"));
    }
    if let Some(ac) = access_code {
        query.push_str(&format!("%26accessCode%3D{ac}"));
    } else if let Some(code) = link_code {
        // Fallback: use linkCode as accessCode for old-format URLs.
        query.push_str(&format!("%26accessCode%3D{code}"));
    }
    if let Some(lc) = link_code {
        query.push_str(&format!("%26linkCode%3D{lc}"));
    }
    query
}

/// Build the `roblox-player://` URI and open it via the default handler.
///
/// `ticket` — the rbx-authentication-ticket from [`crate::auth::RobloxClient`].
/// `place_id` — numeric Roblox place ID.
/// `job_id` — optional server Job ID for joining a specific server.
/// `link_code` — optional private server link code.
/// `access_code` — optional UUID access code for private servers.
/// `launchtime` — the attribution token, from [`next_launchtime`].
///
/// `launchtime` is taken rather than minted here on purpose. The caller has to
/// register it against the account *before* the client can exist, and the only
/// way to guarantee that ordering is for the caller to hold the number first.
/// Returning it from this function would leave a window between the spawn and
/// the registration in which a sweep could see the client and fall back to
/// guessing at a process RM already knows the answer for.
pub fn launch_game(
    ticket: &str,
    place_id: u64,
    job_id: Option<&str>,
    link_code: Option<&str>,
    access_code: Option<&str>,
    launchtime: i64,
    player_path: Option<&std::path::Path>,
) -> Result<(), CoreError> {
    let query = place_launcher_query(place_id, job_id, link_code, access_code);
    let uri = format!(
        "roblox-player:1+launchmode:play\
         +gameinfo:{ticket}\
         +launchtime:{launchtime}\
         +placelauncherurl:{query}"
    );

    info!("Launching game - place {place_id} (launchtime {launchtime})");
    // Never the raw URI: `gameinfo:` carries a live Roblox authentication
    // ticket, and this line runs under `RUST_LOG=debug` straight into rm.log,
    // which users paste into bug reports. The redacted form still shows how the
    // URI was assembled, which is the only reason to log it.
    debug!("URI: {}", crate::redact::scrub(&uri));

    open_uri(&uri, player_path)?;
    Ok(())
}

/// Shell-execute a URI (delegates to `cmd /C start`).
fn open_uri(uri: &str, player_path: Option<&std::path::Path>) -> Result<(), CoreError> {
    if let Some(player_path) = player_path {
        std::process::Command::new(player_path)
            .arg(uri)
            .spawn()
            .map_err(|e| CoreError::Process(format!("failed to launch Roblox player: {e}")))?;
    } else {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", uri])
            .spawn()
            .map_err(|e| CoreError::Process(format!("failed to open URI: {e}")))?;
    }
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

/// Enumerates live Roblox clients and reads each one's `launchtime`, keeping
/// what it learns so the same process is not read twice.
///
/// Reading a command line means `OpenProcess` + `NtQueryInformationProcess` +
/// three `ReadProcessMemory` calls per client. Cheap once, wasteful every two
/// seconds forever, so a successful answer is cached.
///
/// # Keying
///
/// Entries are keyed on `(pid, start_time)`, never the PID alone. Windows
/// recycles PIDs, and a cache hit on a recycled PID would report the *previous*
/// process's account. A changed start time is therefore a different process and
/// gets read afresh.
///
/// `start_time` comes from `sysinfo`, which on Windows reads it via
/// `GetProcessTimes`. It was checked against a live Hyperion-protected client
/// on the target machine and reports a correct creation time even though the
/// same `sysinfo` process entry returns an *empty* command line, which is why
/// the command line itself goes through the native PEB walk instead. There was
/// no need to call `GetProcessTimes` directly.
///
/// Its resolution is one second, so in principle a process that started, died,
/// and had its PID reissued all inside the same second could hit a stale entry.
/// The consequence would be one wrong `launchtime` for one client, corrected on
/// the next sweep after that process exits.
#[derive(Debug, Default)]
pub struct LaunchTokenCache {
    /// Only *successful* reads live here. A read that failed is retried on the
    /// next sweep: a client's PEB is not always readable the instant it
    /// appears, and caching that failure would strand the client on the
    /// inferred path for its whole lifetime.
    resolved: HashMap<(u32, u64), crate::instances::LaunchToken>,
}

impl LaunchTokenCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every running Roblox client, with whatever is known about who launched
    /// it.
    pub fn scan(&mut self) -> Vec<LiveClient> {
        use sysinfo::System;
        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let mut clients = Vec::new();
        for process in sys.processes().values() {
            if process.name().to_string_lossy() != ROBLOX_PLAYER_EXE {
                continue;
            }
            let pid = process.pid().as_u32();
            let key = (pid, process.start_time());
            let token = match self.resolved.get(&key) {
                Some(known) => *known,
                None => {
                    // Never `sysinfo`'s own `cmd()`: it comes back empty for
                    // every Roblox client (Hyperion), verified on the target
                    // machine. Straight to the PEB walk.
                    let token = classify_cmdline(native_get_cmdline(pid).as_deref());
                    if token != crate::instances::LaunchToken::Unreadable {
                        self.resolved.insert(key, token);
                    }
                    token
                }
            };
            clients.push(LiveClient {
                pid,
                start_time: key.1,
                token,
            });
        }

        // Drop anything that is no longer running, or the map grows for the
        // life of the session.
        let live: std::collections::HashSet<(u32, u64)> =
            clients.iter().map(|c| (c.pid, c.start_time)).collect();
        self.resolved.retain(|key, _| live.contains(key));

        clients
    }
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
    // Every 10 seconds for the whole session when the feature is on, so DEBUG.
    // At INFO this alone was a meaningful share of a 3.7 MB log file.
    debug!("kill_tray_roblox: found {} Roblox process(es)", roblox.len());
    let targets: Vec<_> = roblox
        .iter()
        .filter(|(_, p)| {
            // Log the *answer*, never the command line. A Roblox client's
            // command line contains the full launch URI, and that carries a
            // live `gameinfo:` authentication ticket. This runs at INFO for
            // anyone with multi-instance or kill-background enabled, so the
            // old `info!("native cmdline: {cmdline:?}")` here put 1,593
            // tickets into one user's log file before it was caught. The
            // scrubbing writer in `ram_ui` does redact them on the way to
            // disk, but the only line worth writing is whether the flag was
            // there. Anything that genuinely needs a command line in a log
            // must pass it through `crate::redact::scrub` first.
            let args: Vec<String> = p
                .cmd()
                .iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect();
            if !args.is_empty() {
                // sysinfo could read the command line — check directly.
                let tray = args.iter().any(|a| a.contains("--launch-to-tray"));
                debug!("  PID {} — launch-to-tray: {tray} (via sysinfo)", p.pid());
                return tray;
            }
            // sysinfo returned an empty cmd, which is what it does for every
            // Roblox client. Fall back to reading the PEB directly.
            match native_get_cmdline(p.pid().as_u32()) {
                Some(cmdline) => {
                    let tray = cmdline.contains("--launch-to-tray");
                    debug!("  PID {} — launch-to-tray: {tray}", p.pid());
                    tray
                }
                None => {
                    debug!("  PID {} — command line unreadable", p.pid());
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
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    let handle =
        unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid) };
    if handle.is_null() {
        debug!("  native_get_cmdline: OpenProcess failed for PID {pid}");
        return None;
    }
    let result = unsafe { read_cmdline_from_handle(handle) };
    unsafe { CloseHandle(handle) };
    result
}

/// The PEB walk itself, against a handle the caller already holds.
///
/// Split out from [`native_get_cmdline`] so that a caller about to *act* on a
/// process can check and act through one handle. A handle names one specific
/// process object for as long as it is open, so nothing can slip a recycled PID
/// in between the check and the kill. Re-opening by PID would leave exactly
/// that gap, which is the difference between a hint and a guarantee.
///
/// # Safety
///
/// `handle` must be a live process handle opened with at least
/// `PROCESS_QUERY_INFORMATION | PROCESS_VM_READ`.
#[cfg(windows)]
unsafe fn read_cmdline_from_handle(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Option<String> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{FALSE, HANDLE};
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

    (|| -> Option<String> {
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
            debug!("  native_get_cmdline: NtQueryInformationProcess failed: 0x{status:08x}");
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
            debug!("  native_get_cmdline: ReadProcessMemory (PEB) failed");
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
            debug!("  native_get_cmdline: ReadProcessMemory (UNICODE_STRING) failed");
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
            debug!("  native_get_cmdline: ReadProcessMemory (string data) failed");
            return None;
        }

        Some(String::from_utf16_lossy(&buf))
    })()
}

// ---------------------------------------------------------------------------
// Per-account actions — kill and focus one attributed client
// ---------------------------------------------------------------------------

/// Terminate one Roblox client, but only after proving it is still the client
/// RM attributed to this account.
///
/// The PID-to-account map is a hint. It is refreshed every couple of seconds,
/// which means there is always a window in which the process it names has
/// exited and Windows has handed the number to something else. Killing on the
/// strength of the map alone would eventually kill the wrong process, and the
/// wrong process could be anything on the machine.
///
/// So the map is not what authorises the kill. This is:
///
/// 1. Open the process once, for query, read, **and** terminate. Everything
///    below happens through that one handle, which pins the process object, so
///    there is no gap between deciding and acting.
/// 2. Confirm the image is really `RobloxPlayerBeta.exe`.
/// 3. Read the command line and confirm it carries `expected_launchtime`, the
///    token RM minted for this account's launch.
/// 4. Only then terminate.
///
/// Any failure returns an error naming what did not line up, and kills nothing.
/// Note that step 3 cannot pass for an [`crate::instances::Attribution::Inferred`]
/// mapping whose command line was never readable in the first place, which is
/// the intended outcome: RM does not kill on a guess.
#[cfg(windows)]
pub fn kill_verified_instance(pid: u32, expected_launchtime: i64) -> Result<(), CoreError> {
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_QUERY_INFORMATION,
        PROCESS_TERMINATE, PROCESS_VM_READ,
    };

    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_TERMINATE,
            FALSE,
            pid,
        )
    };
    if handle.is_null() {
        return Err(CoreError::Process(format!(
            "Could not open PID {pid}. It has probably already exited."
        )));
    }

    // Everything from here runs through `handle`, so bail out via this closure
    // rather than returning directly: the handle has to be closed either way.
    let verdict = (|| -> Result<(), CoreError> {
        let mut buf = [0u16; 260];
        let mut len: u32 = buf.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len) };
        if ok == 0 {
            return Err(CoreError::Process(format!(
                "Could not identify PID {pid}. Nothing was killed."
            )));
        }
        let image = String::from_utf16_lossy(&buf[..len as usize]);
        let is_client = image
            .rsplit(['\\', '/'])
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(ROBLOX_PLAYER_EXE));
        if !is_client {
            return Err(CoreError::Process(format!(
                "PID {pid} is not a Roblox client any more. Nothing was killed."
            )));
        }

        let Some(cmdline) = (unsafe { read_cmdline_from_handle(handle) }) else {
            return Err(CoreError::Process(format!(
                "Could not read PID {pid} to confirm which account it belongs to. \
                 Nothing was killed."
            )));
        };
        match crate::instances::parse_launchtime(&cmdline) {
            Some(found) if found == expected_launchtime => {}
            Some(_) => {
                return Err(CoreError::Process(format!(
                    "PID {pid} belongs to a different launch than the one recorded \
                     for this account. Nothing was killed."
                )))
            }
            None => {
                return Err(CoreError::Process(format!(
                    "PID {pid} carries no RM launch token, so it was not started by \
                     this account. Nothing was killed."
                )))
            }
        }

        if unsafe { TerminateProcess(handle, 1) } == 0 {
            return Err(CoreError::Process(format!(
                "Windows refused to terminate PID {pid}."
            )));
        }
        Ok(())
    })();

    unsafe { CloseHandle(handle) };
    if verdict.is_ok() {
        info!("Killed verified Roblox client PID {pid}");
    }
    verdict
}

#[cfg(not(windows))]
pub fn kill_verified_instance(_pid: u32, _expected_launchtime: i64) -> Result<(), CoreError> {
    Err(CoreError::Process(
        "per-account kill is only supported on Windows".into(),
    ))
}

/// Every visible, titled top-level window belonging to one of `pids`.
///
/// Shared by focus and retitling so both agree on what "the client's window"
/// means. The visible-and-titled filter is the same one `arrange_roblox_windows`
/// uses, and it is what skips Roblox's invisible helper windows.
#[cfg(windows)]
fn windows_for_pids(
    pids: &std::collections::HashSet<u32>,
) -> Vec<(windows_sys::Win32::Foundation::HWND, u32)> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct EnumState<'a> {
        pids: &'a std::collections::HashSet<u32>,
        found: Vec<(HWND, u32)>,
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam as *mut EnumState);
        if IsWindowVisible(hwnd) == 0 {
            return TRUE;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if !state.pids.contains(&pid) {
            return TRUE;
        }
        if GetWindowTextLengthW(hwnd) > 0 {
            state.found.push((hwnd, pid));
        }
        TRUE
    }

    let mut state = EnumState {
        pids,
        found: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(callback), &mut state as *mut EnumState as LPARAM);
    }
    state.found
}

/// Bring one client's window to the front.
///
/// `SetForegroundWindow` is not a request Windows always honours. It succeeds
/// when the calling process already owns the foreground window, which is the
/// case here because the user just clicked a menu item in RM, and that is why
/// this is called straight from the UI thread rather than queued to the backend.
/// If it is refused anyway, the fallback attaches RM's input queue to the
/// target's for the duration of the call, which is the standard way to borrow
/// the foreground right.
///
/// Returns false when the client has no visible window, or when Windows refused
/// both attempts. Callers should say so rather than pretending it worked.
#[cfg(windows)]
pub fn focus_instance(pid: u32) -> bool {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
        SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let pids: std::collections::HashSet<u32> = std::iter::once(pid).collect();
    let Some(&(hwnd, _)) = windows_for_pids(&pids).first() else {
        debug!("focus_instance: PID {pid} has no visible window");
        return false;
    };

    unsafe {
        // A minimized window will not come forward on its own.
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd) != 0 {
            return true;
        }

        // Refused. Borrow the foreground thread's input state and try again.
        let foreground = GetForegroundWindow();
        if foreground.is_null() {
            return false;
        }
        let target_thread = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
        let our_thread = GetCurrentThreadId();
        if target_thread == 0 || target_thread == our_thread {
            return false;
        }
        AttachThreadInput(our_thread, target_thread, 1);
        BringWindowToTop(hwnd);
        let ok = SetForegroundWindow(hwnd) != 0;
        SetFocus(hwnd);
        AttachThreadInput(our_thread, target_thread, 0);
        ok
    }
}

#[cfg(not(windows))]
pub fn focus_instance(_pid: u32) -> bool {
    false
}

/// Retitle each client's window so tiled clients are tellable apart.
///
/// Roblox rewrites its own title as it loads (it starts as "Roblox" and becomes
/// the place name), so this is called again on every sweep. It is cheap to
/// repeat because it reads the current title first and only calls
/// `SetWindowTextW` when it actually differs, so the steady state is one
/// `GetWindowTextW` per client every couple of seconds and no writes at all.
/// That is what keeps "reapply on sweep" from being a busy loop.
///
/// Returns `(pid, title_that_was_there)` for every window actually rewritten,
/// so the caller can put the original back when the feature is turned off.
///
/// That return value is also how the caller keeps its idea of the original
/// *fresh*. Roblox rewrites its own title as it loads, so the first thing we
/// displace is usually the placeholder "Roblox" rather than the place name the
/// user would expect to see restored. Every later rewrite by Roblox shows up
/// here as another prior title, so a caller that keeps the most recent one ends
/// up holding Roblox's latest intent rather than whatever happened to be there
/// the first time we looked.
///
/// The caller decides what the titles say, which is deliberate: honouring
/// `anonymize_names` needs the account list, and a window title is readable by
/// every process on the machine and shows up in screenshots and screen shares.
#[cfg(windows)]
pub fn apply_instance_titles(titles: &[(u32, String)]) -> Vec<(u32, String)> {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;

    if titles.is_empty() {
        return Vec::new();
    }
    let wanted: HashMap<u32, &str> = titles
        .iter()
        .map(|(pid, title)| (*pid, title.as_str()))
        .collect();
    let pids: std::collections::HashSet<u32> = wanted.keys().copied().collect();

    let mut displaced = Vec::new();
    for (hwnd, pid) in windows_for_pids(&pids) {
        let Some(&want) = wanted.get(&pid) else {
            continue;
        };
        let current = window_title(hwnd);
        if current == want {
            continue;
        }
        let wide: Vec<u16> = want.encode_utf16().chain(std::iter::once(0)).collect();
        if unsafe { SetWindowTextW(hwnd, wide.as_ptr()) } != 0 {
            displaced.push((pid, current));
        }
    }
    displaced
}

#[cfg(not(windows))]
pub fn apply_instance_titles(_titles: &[(u32, String)]) -> Vec<(u32, String)> {
    Vec::new()
}

/// Put back the titles RM displaced, for when the feature is switched off or
/// RM is closing. Best effort: a client that has since exited is skipped, and
/// so is one whose title no longer matches anything we know about.
///
/// Returns how many were restored.
#[cfg(windows)]
pub fn restore_instance_titles(originals: &[(u32, String)]) -> usize {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;

    if originals.is_empty() {
        return 0;
    }
    let wanted: HashMap<u32, &str> = originals
        .iter()
        .map(|(pid, title)| (*pid, title.as_str()))
        .collect();
    let pids: std::collections::HashSet<u32> = wanted.keys().copied().collect();

    let mut restored = 0usize;
    for (hwnd, pid) in windows_for_pids(&pids) {
        let Some(&want) = wanted.get(&pid) else {
            continue;
        };
        if window_title(hwnd) == want {
            continue;
        }
        let wide: Vec<u16> = want.encode_utf16().chain(std::iter::once(0)).collect();
        if unsafe { SetWindowTextW(hwnd, wide.as_ptr()) } != 0 {
            restored += 1;
        }
    }
    restored
}

#[cfg(not(windows))]
pub fn restore_instance_titles(_originals: &[(u32, String)]) -> usize {
    0
}

/// Read one window's title.
#[cfg(windows)]
fn window_title(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buf = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
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


#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Attribution token
    // -----------------------------------------------------------------------

    /// The property the whole of `crate::instances` rests on. A bulk launch
    /// issues these back to back, well inside one millisecond, and two launches
    /// sharing a token would attribute one account's client to another with
    /// full confidence.
    #[test]
    fn launch_tokens_are_unique_and_increasing() {
        let tokens: Vec<i64> = (0..1_000).map(|_| next_launchtime()).collect();
        assert!(
            tokens.windows(2).all(|w| w[0] < w[1]),
            "tokens must strictly increase"
        );
        let mut deduped = tokens.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(deduped.len(), tokens.len(), "duplicate token issued");
    }

    /// It is still a timestamp, not a counter from zero: the value has to be
    /// plausible as `launchtime` to Roblox and readable as a time by a human
    /// reading a log.
    #[test]
    fn a_launch_token_is_a_millisecond_timestamp() {
        let now = chrono::Utc::now().timestamp_millis();
        let token = next_launchtime();
        // Within a second either way of the wall clock. Other tests in this
        // module may have nudged the counter past `now` by a few units.
        assert!((token - now).abs() < 1_000, "token {token} vs now {now}");
    }

    // -----------------------------------------------------------------------
    // Launch URI
    // -----------------------------------------------------------------------

    /// A plain place launch is byte-for-byte the shape RM has always sent, and
    /// the only shape every captured RM launch used. Pinned so the job-ID work
    /// cannot quietly change it.
    #[test]
    fn a_plain_launch_still_uses_the_request_game_form() {
        let query = place_launcher_query(606, None, None, None);
        assert!(query.contains("assetgame.roblox.com"), "{query}");
        assert!(query.contains("%3Frequest%3DRequestGame%26"), "{query}");
        assert!(query.contains("%26placeId%3D606"), "{query}");
        assert!(query.contains("%26isPlayTogetherGame%3Dfalse"), "{query}");
        assert!(!query.contains("gameId"), "{query}");
        assert!(!query.contains("joinAttemptId"), "{query}");
    }

    /// Joining a specific server uses `RequestGameJob`, which is what the
    /// Roblox web client itself emits. The previous `RequestGame` + `gameId`
    /// form appears in no captured launch from any client, RM's included.
    #[test]
    fn a_job_launch_uses_the_request_game_job_form() {
        let job = "f196c5f0-f601-4244-a620-39dc62807f1c";
        let query = place_launcher_query(4_282_985_734, Some(job), None, None);

        assert!(query.contains("www.roblox.com"), "{query}");
        assert!(query.contains("%3Frequest%3DRequestGameJob%26"), "{query}");
        assert!(query.contains("%26placeId%3D4282985734"), "{query}");
        assert!(query.contains(&format!("%26gameId%3D{job}")), "{query}");
        assert!(query.contains("%26joinAttemptId%3D"), "{query}");
        // The job form does not carry this, and adding it would be inventing
        // a shape rather than copying an observed one.
        assert!(!query.contains("isPlayTogetherGame"), "{query}");
    }

    /// Every job launch gets its own attempt ID, the way a fresh page load
    /// would.
    #[test]
    fn each_job_launch_gets_a_fresh_join_attempt_id() {
        let job = Some("f196c5f0-f601-4244-a620-39dc62807f1c");
        assert_ne!(
            place_launcher_query(1, job, None, None),
            place_launcher_query(1, job, None, None)
        );
    }

    /// Private servers are unchanged, including the case where a job ID rides
    /// along with the link code.
    #[test]
    fn a_private_server_launch_is_unchanged() {
        let query = place_launcher_query(606, None, Some("LINK"), Some("ACCESS"));
        assert!(query.contains("assetgame.roblox.com"), "{query}");
        assert!(query.contains("%3Frequest%3DRequestPrivateGame%26"), "{query}");
        assert!(query.contains("%26accessCode%3DACCESS"), "{query}");
        assert!(query.contains("%26linkCode%3DLINK"), "{query}");
        assert!(!query.contains("RequestGameJob"), "{query}");
    }

    #[test]
    fn a_private_server_falls_back_to_the_link_code_as_access_code() {
        let query = place_launcher_query(606, None, Some("LINK"), None);
        assert!(query.contains("%26accessCode%3DLINK"), "{query}");
    }

    /// A job ID alongside a link code must stay on the private-server request,
    /// not get promoted to the job form.
    #[test]
    fn a_private_server_with_a_job_id_stays_a_private_request() {
        let query = place_launcher_query(606, Some("JOB"), Some("LINK"), None);
        assert!(query.contains("%3Frequest%3DRequestPrivateGame%26"), "{query}");
        assert!(query.contains("%26gameId%3DJOB"), "{query}");
    }

    // -----------------------------------------------------------------------
    // Attribution round trip
    // -----------------------------------------------------------------------

    /// The end-to-end property, minus the actual spawn: the token RM stamps
    /// into the URI is the token a reader pulls back out of the command line
    /// the URI becomes. If this breaks, every attribution silently degrades to
    /// the fallback.
    #[test]
    fn the_stamped_token_is_the_one_read_back() {
        let launchtime = next_launchtime();
        let query = place_launcher_query(606, None, None, None);
        let uri = format!(
            "roblox-player:1+launchmode:play\
             +gameinfo:A-TICKET\
             +launchtime:{launchtime}\
             +placelauncherurl:{query}"
        );
        // Roughly how Windows presents it on the client's command line.
        let cmdline = format!(r#""C:\Roblox\RobloxPlayerBeta.exe" {uri}"#);

        assert_eq!(
            crate::instances::parse_launchtime(&cmdline),
            Some(launchtime)
        );
        assert_eq!(
            crate::instances::classify_cmdline(Some(&cmdline)),
            crate::instances::LaunchToken::Found(launchtime)
        );
    }

    /// The launch URI carries a live authentication ticket, and it reaches the
    /// log through `debug!`. The scrubber has to catch it in the exact shape
    /// this module builds, not just in the shape its own tests use.
    #[test]
    fn the_ticket_is_scrubbed_out_of_the_uri_this_module_builds() {
        let query = place_launcher_query(606, None, None, None);
        let uri = format!(
            "roblox-player:1+launchmode:play+gameinfo:LIVE-TICKET-VALUE\
             +launchtime:1700000000000+placelauncherurl:{query}"
        );
        let scrubbed = crate::redact::scrub(&uri);
        assert!(!scrubbed.contains("LIVE-TICKET-VALUE"), "{scrubbed}");
        assert!(scrubbed.contains("gameinfo:<redacted>"), "{scrubbed}");
        // The rest still has to be readable or there is no point logging it.
        assert!(scrubbed.contains("launchtime:1700000000000"), "{scrubbed}");
        assert!(scrubbed.contains("placeId%3D606"), "{scrubbed}");
    }
}
