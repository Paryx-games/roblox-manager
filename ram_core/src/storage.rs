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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::CoreError;

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

/// Atomically replace the contents of `path` with `bytes`, writing through a
/// temp file + fsync + rename. Does **not** touch the backup — used both as the
/// building block for [`atomic_write`] and for restoring a good backup over a
/// corrupt primary without clobbering the backup itself.
pub fn atomic_swap(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
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
    if let Err(e) = std::fs::rename(&tmp, path) {
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

/// Atomically write `bytes` to `path`, first preserving the current contents of
/// `path` (if any) as `path.bak`. The backup is the recovery source if a future
/// write or the file itself is ever damaged; see `crypto::load_encrypted`.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    // Preserve the previous good copy before swapping. Best-effort: a missing or
    // locked backup must never block the primary save.
    if path.exists() {
        let bak = backup_path(path);
        if let Err(e) = std::fs::copy(path, &bak) {
            tracing::warn!(
                path = %path.display(),
                backup = %bak.display(),
                error = %e,
                "atomic_write: failed to update backup copy"
            );
        } else {
            tracing::debug!(
                path = %path.display(),
                backup = %bak.display(),
                "atomic_write: created backup before write"
            );
        }
    }
    atomic_swap(path, bytes)
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
}
