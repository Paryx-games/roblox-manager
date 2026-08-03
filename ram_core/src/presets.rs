//! Per-file persistence for [`LaunchPreset`]s.
//!
//! Each preset is a small JSON file under `<presets_dir>/<slug>.json`. The
//! filename is derived from the preset name (slugified, with a numeric
//! disambiguator if needed) so users can hand-edit, copy, or share them via
//! the filesystem without going through the app.

use std::path::{Path, PathBuf};

use crate::models::LaunchPreset;
use crate::CoreError;

/// What [`load_all`] returns: every preset paired with the file it came from,
/// plus the paths of files that failed to parse and were skipped.
pub type LoadedPresets = (Vec<(PathBuf, LaunchPreset)>, Vec<PathBuf>);

/// Resolve and ensure the presets directory exists under `data_dir`.
pub fn presets_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("presets")
}

/// Slugify a name into a filesystem-safe filename stem (lowercase ASCII,
/// hyphens for whitespace, drops anything else). Empty result becomes `preset`.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_hyphen = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_hyphen = false;
        } else if (c.is_whitespace() || c == '-' || c == '_')
            && !last_hyphen
            && !out.is_empty()
        {
            out.push('-');
            last_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "preset".to_string()
    } else {
        out
    }
}

/// Pick a filename under `dir` for `name` that doesn't already exist on disk.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let stem = slugify(name);
    let first = dir.join(format!("{stem}.json"));
    if !first.exists() {
        return first;
    }
    for i in 2..1000 {
        let p = dir.join(format!("{stem}-{i}.json"));
        if !p.exists() {
            return p;
        }
    }
    // Pathological: fall back to a millisecond-suffixed name.
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    dir.join(format!("{stem}-{ms}.json"))
}

/// Load every `.json` preset file in `presets_dir(data_dir)`. Files that fail
/// to parse are skipped (with their path returned) rather than aborting the
/// whole load — one bad file shouldn't hide every other preset.
///
/// Returned tuple: `(presets_with_path, skipped_paths)`.
pub fn load_all(data_dir: &Path) -> Result<LoadedPresets, CoreError> {
    let dir = presets_dir(data_dir);
    if !dir.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut presets = Vec::new();
    let mut skipped = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&path)
            .map_err(CoreError::Io)
            .and_then(|s| serde_json::from_str::<LaunchPreset>(&s).map_err(CoreError::Json))
        {
            Ok(p) => presets.push((path, p)),
            Err(_) => skipped.push(path),
        }
    }
    // Sort by preset name (case-insensitive) so the UI order is stable.
    presets.sort_by_key(|(_, p)| p.name.to_lowercase());
    Ok((presets, skipped))
}

/// Persist `preset` to disk. If `path` is `Some`, overwrite that file
/// (rename if the preset's name changed enough to want a new slug);
/// if `None`, pick a fresh unique path under `presets_dir`. Returns the
/// final path the preset was written to.
pub fn save(
    data_dir: &Path,
    preset: &LaunchPreset,
    path: Option<&Path>,
) -> Result<PathBuf, CoreError> {
    let dir = presets_dir(data_dir);
    std::fs::create_dir_all(&dir)?;
    let target = match path {
        Some(p) => p.to_path_buf(),
        None => unique_path(&dir, &preset.name),
    };
    let json = serde_json::to_string_pretty(preset)?;
    crate::storage::atomic_write(&target, json.as_bytes())?;
    Ok(target)
}

/// Delete a preset file. No-op if the file is already gone.
pub fn delete(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(CoreError::Io(e)),
    }
}
