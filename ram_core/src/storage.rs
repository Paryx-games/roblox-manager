//! Crash- and race-safe file persistence.
//!
//! Every piece of on-disk state (the encrypted account store, `config.json`,
//! preset files) is written through [`atomic_write`]. A write is therefore
//! all-or-nothing: we stream the bytes into a sibling temp file, fsync it, then
//! atomically `rename` it over the target. `rename` within a directory is
//! atomic on both Windows and POSIX, so a concurrent reader — or a crash, or a
//! second process — can never observe a half-written file.
//!
//! This exists because v1.4.4 wrote the account store with a bare
//! `std::fs::write`, and two overlapping saves (bulk import + the background
//! refresh timers) could interleave into a torn AES-GCM blob that no longer
//! authenticated. Decryption then failed and was misreported as a wrong
//! password, locking users out of intact accounts. Going through this module
//! makes that class of corruption structurally impossible.
//!
//! Windows caveat: replacing a file that another handle is touching (a
//! concurrent swap, Defender, the search indexer) can fail transiently with
//! `ERROR_ACCESS_DENIED` / sharing violations instead of blocking. Writers in
//! this process are therefore serialized by [`WRITE_LOCK`], and filesystem
//! operations that can hit those transient errors go through [`retry_io`].

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crate::error::CoreError;

/// Serializes all writes in this process. Saves are infrequent, so one global
/// lock is simpler than a per-path map and costs nothing noticeable.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn lock_writes() -> MutexGuard<'static, ()> {
    // A panic in another writer must not wedge every future save.
    WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Windows reports access-denied / sharing / lock violations (5, 32, 33) when a
/// file is momentarily held by another handle (concurrent swap, Defender, the
/// indexer). These clear within milliseconds. Never transient elsewhere.
fn is_transient(e: &io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(5) | Some(32) | Some(33))
}

/// Run `op`, retrying a bounded number of times with short exponential backoff
/// when it fails with a transient Windows file-contention error. Also use this
/// around reads of files that writers may be swapping (e.g. `load_encrypted`).
pub(crate) fn retry_io<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    const ATTEMPTS: u32 = 10;
    let mut delay = Duration::from_millis(2);
    let mut attempt = 1;
    loop {
        match op() {
            Err(e) if attempt < ATTEMPTS && is_transient(&e) => {
                std::thread::sleep(delay);
                delay = (delay * 2).min(Duration::from_millis(50));
                attempt += 1;
            }
            other => return other,
        }
    }
}

/// Path of the last-known-good backup kept beside `path` (i.e. `path.bak`).
pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

/// A temp path beside `path`, unique per (process, call) so two concurrent
/// writers never share a temp file even across separate app instances.
fn temp_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // pid in the high bits guards multi-process collisions; the counter guards
    // collisions within this process. No clock/RNG needed.
    let tag = ((std::process::id() as u64) << 32) | (n & 0xFFFF_FFFF);
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".tmp-{tag}"));
    PathBuf::from(s)
}

/// The actual temp-file + fsync + rename. Callers must already hold
/// [`WRITE_LOCK`].
fn swap_inner(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let tmp = temp_path(path);
    tracing::debug!(
        path = %path.display(),
        tmp = %tmp.display(),
        bytes = bytes.len(),
        "atomic_swap: writing temp file"
    );
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        // Durably land the bytes before we swap them in, so a crash between the
        // write and the rename can't leave a zero-length/partial temp in play.
        f.sync_all()?;
    }

    // Atomic swap. On failure leave the original untouched and clean up.
    if let Err(e) = retry_io(|| std::fs::rename(&tmp, path)) {
        tracing::warn!(
            path = %path.display(),
            tmp = %tmp.display(),
            error = %e,
            "atomic_swap: rename failed, cleaning up temp file"
        );
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    tracing::debug!(path = %path.display(), "atomic_swap: write committed");
    Ok(())
}

/// Atomically replace the contents of `path` with `bytes`, writing through a
/// temp file + fsync + rename. Does **not** touch the backup — used both as the
/// building block for [`atomic_write`] and for restoring a good backup over a
/// corrupt primary without clobbering the backup itself.
pub fn atomic_swap(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let _guard = lock_writes();
    swap_inner(path, bytes)
}

/// Atomically write `bytes` to `path`, first preserving the current contents of
/// `path` (if any) as `path.bak`. The backup is the recovery source if a future
/// write or the file itself is ever damaged; see `crypto::load_encrypted`.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let _guard = lock_writes();

    // Preserve the previous good copy before swapping. Best-effort: a missing or
    // locked backup must never block the primary save. The backup is itself
    // written atomically so it can never be torn by an interleaved writer.
    if path.exists() {
        let bak = backup_path(path);
        match retry_io(|| std::fs::read(path)) {
            Ok(prev) => match swap_inner(&bak, &prev) {
                Ok(()) => tracing::debug!(
                    path = %path.display(),
                    backup = %bak.display(),
                    "atomic_write: created backup before write"
                ),
                Err(e) => tracing::warn!(
                    path = %path.display(),
                    backup = %bak.display(),
                    error = %e,
                    "atomic_write: failed to update backup copy"
                ),
            },
            Err(e) => tracing::warn!(
                path = %path.display(),
                error = %e,
                "atomic_write: failed to read current file for backup"
            ),
        }
    }
    swap_inner(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ram_storage_{}_{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("state.bin")
    }

    #[test]
    fn writes_then_reads_back() {
        let p = scratch("rw");
        atomic_write(&p, b"hello").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"hello");
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn second_write_preserves_previous_as_backup() {
        let p = scratch("bak");
        atomic_write(&p, b"v1").unwrap();
        assert!(
            !backup_path(&p).exists(),
            "no backup should exist after first write"
        );
        atomic_write(&p, b"v2").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"v2");
        assert_eq!(std::fs::read(backup_path(&p)).unwrap(), b"v1");
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn leaves_no_temp_files_behind() {
        let p = scratch("notmp");
        atomic_write(&p, b"a").unwrap();
        atomic_write(&p, b"bb").unwrap();
        let dir = p.parent().unwrap();
        let stray: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(stray.is_empty(), "temp files left behind: {stray:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn concurrent_writers_leave_a_complete_file_and_backup() {
        let p = scratch("concurrent");
        atomic_write(&p, &[0u8; 4096]).unwrap();

        let handles: Vec<_> = (1u8..=8)
            .map(|i| {
                let p = p.clone();
                std::thread::spawn(move || {
                    for _ in 0..25 {
                        atomic_write(&p, &[i; 4096]).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        // Every file must be one writer's complete payload, never a mix.
        for f in [p.clone(), backup_path(&p)] {
            let data = std::fs::read(&f).unwrap();
            assert_eq!(data.len(), 4096);
            assert!(data.iter().all(|b| *b == data[0]), "torn file: {f:?}");
        }
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn retry_io_gives_up_on_non_transient_errors() {
        let mut calls = 0;
        let r: io::Result<()> = retry_io(|| {
            calls += 1;
            Err(io::Error::new(io::ErrorKind::NotFound, "nope"))
        });
        assert!(r.is_err());
        assert_eq!(calls, 1, "non-transient errors must not be retried");
    }
}
