//! Benchmark-only notification; normal windows never receive the enable flag.
//! The event is created and owned by the harness, contains no payload, and its
//! name comes exclusively from the process environment (never from IPC input).
use tauri::WebviewWindow;

pub fn event_name() -> Option<String> {
    if std::env::var("RM_BENCHMARK").ok().as_deref() != Some("1") {
        return None;
    }
    let name = std::env::var("RM_BENCHMARK_EVENT").ok()?;
    valid_event_name(&name).then_some(name)
}

fn valid_event_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix("Local\\RM_BENCHMARK_") else {
        return false;
    };
    name.len() < 128
        && !suffix.is_empty()
        && suffix.bytes().all(|c| c.is_ascii_digit() || c == b'_')
}

#[tauri::command]
pub fn benchmark_ready(window: WebviewWindow) -> Result<(), &'static str> {
    if window.label() != "main" {
        return Err("Benchmark notification is restricted to the main window");
    }
    let Some(name) = event_name() else {
        return Ok(());
    };
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
            fn SetEvent(event: *mut c_void) -> i32;
            fn CloseHandle(handle: *mut c_void) -> i32;
        }
        struct Event(*mut c_void);
        impl Drop for Event {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // EVENT_MODIFY_STATE only; the live, terminated UTF-16 buffer spans OpenEventW.
        let handle = unsafe { OpenEventW(2, 0, wide.as_ptr()) };
        if handle.is_null() {
            return Err("Benchmark event unavailable");
        }
        let event = Event(handle);
        if unsafe { SetEvent(event.0) } == 0 {
            return Err("Benchmark event notification failed");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::valid_event_name;
    #[test]
    fn accepts_only_bounded_benchmark_event_names() {
        assert!(valid_event_name("Local\\RM_BENCHMARK_123_1_456"));
        for name in [
            "",
            "Global\\RM_BENCHMARK_123",
            "Local\\other",
            "Local\\RM_BENCHMARK_",
            "Local\\RM_BENCHMARK_../x",
            "Local\\RM_BENCHMARK_123\0",
        ] {
            assert!(!valid_event_name(name));
        }
        assert!(!valid_event_name(&format!(
            "Local\\RM_BENCHMARK_{}",
            "1".repeat(128)
        )));
    }
}
