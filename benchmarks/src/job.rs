//! Own one run's job and root handle. All FFI pointers refer to sized, live buffers;
//! handles are non-inheritable and closed exactly once by Handle::drop.
use crate::measurement::Counters;
use anyhow::{Context, Result, bail};
use std::{collections::HashSet, mem::size_of, os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_MORE_DATA, GetLastError, HANDLE},
    System::{JobObjects::*, Threading::*},
};

pub(crate) struct Handle(pub HANDLE);
impl Handle {
    pub fn new(raw: HANDLE) -> Result<Self> {
        if raw.is_null() || raw == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            Err(std::io::Error::last_os_error().into())
        } else {
            Ok(Self(raw))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub struct Job {
    job: Handle,
    root: Handle,
    pub pid: u32,
}
impl Job {
    pub fn launch(exe: &Path, ready_event: &str) -> Result<Self> {
        Self::launch_command(exe, ready_event, "")
    }
    fn launch_command(exe: &Path, ready_event: &str, arguments: &str) -> Result<Self> {
        // Suspension prevents any WebView2 child from starting before assignment.
        unsafe {
            let job = Handle::new(CreateJobObjectW(ptr::null(), ptr::null()))?;
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let path: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
            let mut command: Vec<u16> = format!("\"{}\" {}", exe.display(), arguments)
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let cwd: Vec<u16> = exe
                .parent()
                .context("executable has no directory")?
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            let mut environment: Vec<(std::ffi::OsString, std::ffi::OsString)> =
                std::env::vars_os()
                    .filter(|(k, _)| {
                        !k.to_string_lossy().eq_ignore_ascii_case("RM_BENCHMARK")
                            && !k
                                .to_string_lossy()
                                .eq_ignore_ascii_case("RM_BENCHMARK_EVENT")
                    })
                    .collect();
            environment.push(("RM_BENCHMARK".into(), "1".into()));
            environment.push(("RM_BENCHMARK_EVENT".into(), ready_event.into()));
            environment.sort_by_key(|(k, _)| k.to_string_lossy().to_lowercase());
            let mut env_block = Vec::<u16>::new();
            for (key, value) in environment {
                env_block.extend(key.encode_wide());
                env_block.push('=' as u16);
                env_block.extend(value.encode_wide());
                env_block.push(0);
            }
            env_block.push(0);
            let startup = STARTUPINFOW {
                cb: size_of::<STARTUPINFOW>() as u32,
                ..Default::default()
            };
            let mut info = PROCESS_INFORMATION::default();
            if CreateProcessW(
                path.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
                env_block.as_ptr() as _,
                cwd.as_ptr(),
                &startup,
                &mut info,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let root = Handle::new(info.hProcess)?;
            let thread = Handle::new(info.hThread)?;
            if AssignProcessToJobObject(job.0, root.0) == 0 {
                let error = std::io::Error::last_os_error();
                TerminateProcess(root.0, 1);
                WaitForSingleObject(root.0, 3000);
                return Err(error)
                    .context("job assignment failed; run cancelled before RM resumed");
            }
            let run = Self {
                job,
                root,
                pid: info.dwProcessId,
            };
            if ResumeThread(thread.0) == u32::MAX {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(run)
        }
    }
    pub fn contains(&self, process: HANDLE) -> bool {
        let mut member = 0;
        // membership is checked on the open handle, preventing PID reuse contamination.
        unsafe { IsProcessInJob(process, self.job.0, &mut member) != 0 && member != 0 }
    }
    pub fn exit_code(&self) -> Result<Option<u32>> {
        unsafe {
            if WaitForSingleObject(self.root.0, 0) == windows_sys::Win32::Foundation::WAIT_TIMEOUT {
                return Ok(None);
            }
            let mut code = 0;
            if GetExitCodeProcess(self.root.0, &mut code) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(Some(code))
        }
    }
    pub fn pids(&self) -> Result<HashSet<u32>> {
        let mut capacity = 64;
        loop {
            // usize allocation provides alignment for the two DWORD header fields
            // and ULONG_PTR array. Windows reports the number of valid entries.
            let offset = std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList);
            let mut buffer = vec![0usize; capacity + offset / size_of::<usize>()];
            unsafe {
                if QueryInformationJobObject(
                    self.job.0,
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr() as _,
                    (buffer.len() * size_of::<usize>()) as u32,
                    ptr::null_mut(),
                ) != 0
                {
                    let header = buffer.as_ptr() as *const u32;
                    let count = *header.add(1) as usize;
                    if count > capacity {
                        bail!("invalid job PID count");
                    }

                    let ids = std::slice::from_raw_parts(
                        (buffer.as_ptr() as *const u8).add(offset) as *const usize,
                        count,
                    );
                    return Ok(ids.iter().map(|p| *p as u32).collect());
                }
                if GetLastError() != ERROR_MORE_DATA {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            capacity *= 2;
            if capacity > 65536 {
                bail!("job process list exceeds safety bound");
            }
        }
    }
    pub fn counters(&self) -> Result<(Counters, u64, u32)> {
        let mut info = JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION::default();
        unsafe {
            if QueryInformationJobObject(
                self.job.0,
                JobObjectBasicAndIoAccountingInformation,
                &mut info as *mut _ as _,
                size_of::<JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION>() as u32,
                ptr::null_mut(),
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        let io = info.IoInfo;
        Ok((
            Counters {
                user_100ns: info.BasicInfo.TotalUserTime as u64,
                kernel_100ns: info.BasicInfo.TotalKernelTime as u64,
                read_ops: io.ReadOperationCount,
                write_ops: io.WriteOperationCount,
                other_ops: io.OtherOperationCount,
                read_bytes: io.ReadTransferCount,
                write_bytes: io.WriteTransferCount,
                other_bytes: io.OtherTransferCount,
            },
            info.BasicInfo.TotalPageFaultCount as u64,
            info.BasicInfo.TotalProcesses,
        ))
    }
    pub fn terminate(&self) -> Result<()> {
        unsafe {
            if TerminateJobObject(self.job.0, 1) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn job_retains_exited_child_accounting_and_contains_cleanup() {
        let exe = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let run=Job::launch_command(&exe,"",r#"-NoProfile -NonInteractive -Command "$p=Start-Process $env:ComSpec -ArgumentList '/c ping -n 2 127.0.0.1 >nul' -PassThru -WindowStyle Hidden; $p.WaitForExit(); Start-Sleep -Seconds 10"#).unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut before = Counters::default();
        let mut native = crate::win::CpuGroup::default();
        let mut native_before = 0u64;
        let mut observed_count = 0;
        loop {
            let (counters, _, total) = run.counters().unwrap();
            assert!(counters.user_100ns >= before.user_100ns);
            assert!(counters.kernel_100ns >= before.kernel_100ns);
            assert!(counters.read_bytes >= before.read_bytes);
            before = counters;
            let pids = run.pids().unwrap();
            let (u, k, n) = native.totals(&pids, &run).unwrap();
            assert!(u + k >= native_before);
            native_before = u + k;
            observed_count = observed_count.max(n);
            if total >= 2 && run.pids().unwrap().len() == 1 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "child was not accounted in the job"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(before.user_100ns + before.kernel_100ns > 0);
        assert!(
            observed_count >= 2,
            "native sampler did not observe the child"
        );
        let unrelated = unsafe {
            Handle::new(OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                std::process::id(),
            ))
        }
        .unwrap();
        assert!(!run.contains(unrelated.0));
        run.terminate().unwrap();
        while !run.pids().unwrap().is_empty() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        let after = run.counters().unwrap().0;
        let (u, k, n) = native.totals(&HashSet::new(), &run).unwrap();
        assert!(u + k >= native_before);
        assert!(n >= observed_count);
        assert!(after.user_100ns >= before.user_100ns);
        assert!(after.read_bytes >= before.read_bytes);
        while run.exit_code().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "root handle did not signal after termination"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
