#![cfg(windows)]

use std::collections::{HashMap, HashSet};
use std::mem::{size_of, zeroed};

use crate::job::Handle;
use windows_sys::Win32::Foundation::{FILETIME, HWND, LPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{
    GetProcessHandleCount, GetProcessIoCounters, GetProcessTimes, IO_COUNTERS, OpenProcess,
    PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_CLOSE, WM_NULL,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeProcess {
    pub working_set_bytes: u64,
    pub private_bytes: u64,
    pub virtual_bytes: u64,
    pub peak_working_set_bytes: u64,
    pub page_faults: u64,
    pub handles: u64,
    pub threads: u64,
    pub user_cpu_100ns: u64,
    pub kernel_cpu_100ns: u64,
    pub read_ops: u64,
    pub write_ops: u64,
    pub other_ops: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub other_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessEntry {
    pub webview: bool,
    pub threads: u32,
}

#[derive(Default)]
struct EnumContext {
    pids: HashSet<u32>,
    windows: Vec<HWND>,
    created: bool,
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    unsafe {
        let ctx = &mut *(lparam as *mut EnumContext);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        // callback context remains borrowed exclusively for EnumWindows.
        if !ctx.pids.contains(&pid) {
            return 1;
        }
        let mut title = [0u16; 128];
        let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        if ctx.pids.contains(&pid)
            && n > 0
            && String::from_utf16_lossy(&title[..n as usize]) == "Roblox Manager"
        {
            ctx.created = true;
            if IsWindowVisible(hwnd) != 0 {
                ctx.windows.push(hwnd);
            }
        }
    }
    1
}

pub fn visible_window_state(pids: &HashSet<u32>) -> (bool, bool, bool, Vec<HWND>) {
    let mut ctx = EnumContext {
        pids: pids.clone(),
        windows: Vec::new(),
        created: false,
    };
    unsafe {
        let _ = EnumWindows(Some(enum_window), &mut ctx as *mut _ as LPARAM);
    }
    let visible = !ctx.windows.is_empty();
    let responsive = ctx.windows.iter().any(|hwnd| unsafe {
        let mut result = 0;
        SendMessageTimeoutW(
            *hwnd,
            WM_NULL,
            0,
            0,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            10,
            &mut result,
        ) != 0
    });
    (ctx.created, visible, responsive, ctx.windows)
}

pub fn request_close(windows: &[HWND]) {
    for hwnd in windows {
        unsafe {
            let _ = PostMessageW(*hwnd, WM_CLOSE, 0, 0);
        }
    }
}

pub fn process_snapshot() -> anyhow::Result<HashMap<u32, ProcessEntry>> {
    let mut out = HashMap::new();
    unsafe {
        let snapshot = Handle::new(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0))?;

        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot.0, &mut entry) != 0 {
            loop {
                out.insert(
                    entry.th32ProcessID,
                    ProcessEntry {
                        webview: String::from_utf16_lossy(
                            &entry.szExeFile[..entry
                                .szExeFile
                                .iter()
                                .position(|c| *c == 0)
                                .unwrap_or(entry.szExeFile.len())],
                        )
                        .eq_ignore_ascii_case("msedgewebview2.exe"),
                        threads: entry.cntThreads,
                    },
                );
                if Process32NextW(snapshot.0, &mut entry) == 0 {
                    break;
                }
            }
        }
    }
    Ok(out)
}

fn filetime_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

pub fn native_process(pid: u32, thread_count: u32) -> Option<NativeProcess> {
    native_process_for_job(pid, thread_count, None)
}
pub fn native_process_for_job(
    pid: u32,
    thread_count: u32,
    job: Option<&crate::job::Job>,
) -> Option<NativeProcess> {
    unsafe {
        let handle = Handle::new(OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            0,
            pid,
        ))
        .ok()?;

        if job.is_some_and(|job| !job.contains(handle.0)) {
            return None;
        }
        let mut out = NativeProcess {
            threads: thread_count as u64,
            ..Default::default()
        };

        let mut mem: PROCESS_MEMORY_COUNTERS_EX = zeroed();
        mem.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        if K32GetProcessMemoryInfo(
            handle.0,
            &mut mem as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ) != 0
        {
            out.working_set_bytes = mem.WorkingSetSize as u64;
            out.private_bytes = mem.PrivateUsage as u64;
            out.peak_working_set_bytes = mem.PeakWorkingSetSize as u64;
            out.page_faults = mem.PageFaultCount as u64;
        } else {
            return None;
        }

        // ProcessVmCounters returns virtual address space, not PrivateUsage.
        // The native API is isolated here; failure excludes the run instead of a zero.
        let mut vm: ntapi::ntpsapi::VM_COUNTERS = zeroed();
        if ntapi::ntpsapi::NtQueryInformationProcess(
            handle.0 as _,
            ntapi::ntpsapi::ProcessVmCounters,
            &mut vm as *mut _ as _,
            size_of::<ntapi::ntpsapi::VM_COUNTERS>() as u32,
            std::ptr::null_mut(),
        ) < 0
        {
            return None;
        }
        out.virtual_bytes = vm.VirtualSize as u64;
        let mut handles = 0u32;
        if GetProcessHandleCount(handle.0, &mut handles) != 0 {
            out.handles = handles as u64;
        } else {
            return None;
        }

        let mut creation: FILETIME = zeroed();
        let mut exit: FILETIME = zeroed();
        let mut kernel: FILETIME = zeroed();
        let mut user: FILETIME = zeroed();
        if GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) != 0 {
            out.kernel_cpu_100ns = filetime_u64(kernel);
            out.user_cpu_100ns = filetime_u64(user);
        } else {
            return None;
        }

        let mut io: IO_COUNTERS = zeroed();
        if GetProcessIoCounters(handle.0, &mut io) != 0 {
            out.read_ops = io.ReadOperationCount;
            out.write_ops = io.WriteOperationCount;
            out.other_ops = io.OtherOperationCount;
            out.read_bytes = io.ReadTransferCount;
            out.write_bytes = io.WriteTransferCount;
            out.other_bytes = io.OtherTransferCount;
        } else {
            return None;
        }

        Some(out)
    }
}

/// A manual-reset, non-inherited event carries no app/account data.
pub struct Readiness {
    handle: Handle,
    pub name: String,
}
impl Readiness {
    pub fn new(run: usize) -> anyhow::Result<Self> {
        use windows_sys::Win32::System::Threading::CreateEventW;
        let name = format!(
            "Local\\RM_BENCHMARK_{}_{}_{}",
            std::process::id(),
            run,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // valid null security attributes give default access and no inheritance.
        let handle = unsafe { Handle::new(CreateEventW(std::ptr::null(), 1, 0, wide.as_ptr()))? };
        Ok(Self { handle, name })
    }
    pub fn signaled(&self) -> bool {
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(self.handle.0, 0)
                == windows_sys::Win32::Foundation::WAIT_OBJECT_0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_memory_and_process_times_are_available() {
        let before = native_process(std::process::id(), 0).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_millis(50);
        let mut value = 1u64;
        while std::time::Instant::now() < until {
            value = std::hint::black_box(value.wrapping_mul(13).wrapping_add(7));
        }
        let after = native_process(std::process::id(), 0).unwrap();
        assert!(
            after.user_cpu_100ns + after.kernel_cpu_100ns
                > before.user_cpu_100ns + before.kernel_cpu_100ns
        );
        assert!(after.virtual_bytes > after.private_bytes);
        assert!(after.working_set_bytes > 0 && after.handles > 0);
    }
    #[test]
    fn readiness_event_starts_unsignaled_and_is_owned() {
        let event = Readiness::new(999).unwrap();
        assert!(!event.signaled());
        assert_ne!(
            unsafe { windows_sys::Win32::System::Threading::SetEvent(event.handle.0) },
            0
        );
        assert!(event.signaled());
    }
}

/// Retain process handles until the run ends: GetProcessTimes remains readable
/// after exit, and creation time distinguishes a reused PID from its predecessor.
#[derive(Default)]
pub struct CpuGroup {
    processes: HashMap<(u32, u64), Handle>,
}
impl CpuGroup {
    pub fn totals(
        &mut self,
        pids: &HashSet<u32>,
        job: &crate::job::Job,
    ) -> anyhow::Result<(u64, u64, usize)> {
        for &pid in pids {
            // Query only; duplicate observations close their temporary handle.
            let handle =
                unsafe { Handle::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)) };
            if let Ok(handle) = handle
                && job.contains(handle.0)
            {
                let (creation, _, _) = cpu_times(&handle)?;
                self.processes.entry((pid, creation)).or_insert(handle);
            }
        }
        let mut user = 0u64;
        let mut kernel = 0u64;
        for handle in self.processes.values() {
            let (_, u, k) = cpu_times(handle)?;
            user += u;
            kernel += k;
        }
        Ok((user, kernel, self.processes.len()))
    }
}
fn cpu_times(handle: &Handle) -> anyhow::Result<(u64, u64, u64)> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // all FILETIME output buffers are valid; the owned handle pins process identity.
    if unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((
        filetime_u64(creation),
        filetime_u64(user),
        filetime_u64(kernel),
    ))
}
