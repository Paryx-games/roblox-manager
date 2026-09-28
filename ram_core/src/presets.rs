//! Per-file persistence for [`LaunchPreset`]s.
//!
//! Each preset is a small JSON file under `<presets_dir>/<slug>.json`. The
//! filename is derived from the preset name (slugified, with a numeric
//! disambiguator if needed) so users can hand-edit, copy, or share them via
//! the filesystem without going through the app.

use std::path::{Path, PathBuf};

use crate::models::{AppConfig, LaunchPreset};
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
        } else if (c.is_whitespace() || c == '-' || c == '_') && !last_hyphen && !out.is_empty() {
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
        tracing::debug!(dir = %dir.display(), "presets directory does not exist; returning empty list");
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
            Ok(p) => {
                tracing::debug!(path = %path.display(), name = %p.name, place_id = p.place_id, "loaded preset");
                presets.push((path, p));
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipped corrupt or unreadable preset file");
                skipped.push(path);
            }
        }
    }
    // Sort by preset name (case-insensitive) so the UI order is stable.
    presets.sort_by_key(|(_, p)| p.name.to_lowercase());
    tracing::info!(
        loaded = presets.len(),
        skipped = skipped.len(),
        "finished loading presets"
    );
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
    tracing::info!(
        name = %preset.name,
        place_id = preset.place_id,
        path = %target.display(),
        "saved preset"
    );
    Ok(target)
}

/// Delete a preset file. No-op if the file is already gone.
pub fn delete(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => {
            tracing::info!(path = %path.display(), "deleted preset file");
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %path.display(), "preset file was already deleted");
            Ok(())
        }
        Err(e) => {
            tracing::error!(path = %path.display(), error = %e, "failed to delete preset file");
            Err(CoreError::Io(e))
        }
    }
}

pub fn migrate_favourites(
    config: &mut AppConfig,
    directory: &Path,
    config_path: &Path,
) -> Result<(), String> {
    if config.favorite_places.is_empty() {
        return Ok(());
    }
    let (mut existing, _) = load_all(directory).map_err(|_| "Saved presets could not be loaded")?;
    for favourite in &config.favorite_places {
        if favourite.place_id == 0 {
            return Err("A legacy favourite has an invalid Place ID. Correct it in the old configuration before retrying.".into());
        }
        let preset = LaunchPreset {
            name: favourite.name.clone(),
            place_id: favourite.place_id,
            job_id: None,
            data: None,
        };
        if !existing.iter().any(|(_, saved)| saved == &preset) {
            let path = save(directory, &preset, None)
                .map_err(|_| "A legacy favourite could not be saved as a preset")?;
            existing.push((path, preset));
        }
    }
    let mut candidate = config.clone();
    candidate.favorite_places.clear();
    candidate
        .save(config_path)
        .map_err(|_| "Favourite migration could not be recorded. Your favourites are retained.")?;
    *config = candidate;
    Ok(())
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn failed_config_save_retains_favourites_and_retry_reuses_saved_presets() {
        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&directory).unwrap();
        let blocked_parent = directory.join("blocked");
        std::fs::write(&blocked_parent, b"synthetic-file").unwrap();
        let mut config = AppConfig {
            favorite_places: vec![crate::models::FavoritePlace {
                name: "Synthetic favourite".into(),
                place_id: 123,
            }],
            ..AppConfig::default()
        };
        assert_eq!(
            migrate_favourites(&mut config, &directory, &blocked_parent.join("config.json")),
            Err("Favourite migration could not be recorded. Your favourites are retained.".into())
        );
        assert_eq!(config.favorite_places.len(), 1);
        assert_eq!(load_all(&directory).unwrap().0.len(), 1);
        let config_path = directory.join("config.json");
        migrate_favourites(&mut config, &directory, &config_path).unwrap();
        assert!(config.favorite_places.is_empty());
        assert_eq!(load_all(&directory).unwrap().0.len(), 1);
        assert!(AppConfig::load(&config_path).favorite_places.is_empty());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
