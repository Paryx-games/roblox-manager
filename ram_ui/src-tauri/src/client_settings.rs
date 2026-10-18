use crate::state::AppState;
use ram_core::storage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Manager;

const SETTINGS_FILE: &str = "client-launch-settings.json";
const MAX_XML_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientLaunchSettings {
    pub enabled: bool,
    #[serde(default = "default_rm_only")]
    pub rm_only: bool,
    pub fps: u32,
    pub graphics: u8,
    pub fullscreen: bool,
    pub muted: bool,
}

fn default_rm_only() -> bool {
    true
}

impl Default for ClientLaunchSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            rm_only: true,
            fps: 60,
            graphics: 5,
            fullscreen: false,
            muted: false,
        }
    }
}

impl ClientLaunchSettings {
    fn validate(&self) -> Result<(), String> {
        if ![30, 60, 120, 144, 240].contains(&self.fps) || !(1..=10).contains(&self.graphics) {
            return Err("Choose a supported FPS limit and graphics quality from 1 to 10.".into());
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavedSettings {
    settings: ClientLaunchSettings,
    unmuted_volume: f32,
}

impl Default for SavedSettings {
    fn default() -> Self {
        Self {
            settings: ClientLaunchSettings::default(),
            unmuted_volume: 0.5,
        }
    }
}

fn preference_path(state: &AppState) -> Result<PathBuf, String> {
    state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable")?
        .config_path
        .parent()
        .map(|directory| directory.join(SETTINGS_FILE))
        .ok_or_else(|| "Client launch settings location unavailable.".into())
}

fn roblox_settings_path() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|directory| directory.join("Roblox").join("GlobalBasicSettings_13.xml"))
        .ok_or_else(|| "Roblox settings location unavailable.".into())
}

fn load(path: &Path) -> Result<SavedSettings, String> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SavedSettings::default())
        }
        Err(_) => return Err("Client launch settings could not be read.".into()),
    };
    let saved: SavedSettings = serde_json::from_slice(&data)
        .map_err(|_| "Client launch settings are invalid. Save them again to repair them.")?;
    saved.settings.validate()?;
    if !saved.unmuted_volume.is_finite()
        || !(0.0..=1.0).contains(&saved.unmuted_volume)
        || saved.unmuted_volume == 0.0
    {
        return Err("The saved client volume is invalid.".into());
    }
    Ok(saved)
}

fn read_xml(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|_| "Start Roblox once to create its settings, then try again.")?;
    if !metadata.is_file() || metadata.len() > MAX_XML_BYTES {
        return Err("Roblox settings have an unsupported size or format.".into());
    }
    std::fs::read_to_string(path)
        .map_err(|_| "Roblox settings could not be read.")
        .map_err(Into::into)
}

fn properties<'a>(
    document: &'a roxmltree::Document<'a>,
) -> Result<roxmltree::Node<'a, 'a>, String> {
    let mut items = document.descendants().filter(|node| {
        node.has_tag_name("Item") && node.attribute("class") == Some("UserGameSettings")
    });
    let item = items
        .next()
        .ok_or("Roblox settings format is not supported.")?;
    if items.next().is_some() {
        return Err("Roblox settings contain duplicate user settings.".into());
    }
    let mut properties = item
        .children()
        .filter(|node| node.has_tag_name("Properties"));
    let node = properties
        .next()
        .ok_or("Roblox settings properties are missing.")?;
    if properties.next().is_some() {
        return Err("Roblox settings properties are ambiguous.".into());
    }
    Ok(node)
}

fn xml_volume(xml: &str) -> Option<f32> {
    let document = roxmltree::Document::parse(xml).ok()?;
    let properties = properties(&document).ok()?;
    properties
        .children()
        .find(|node| node.has_tag_name("float") && node.attribute("name") == Some("MasterVolume"))?
        .text()?
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 1.0)
}

fn patch_xml(
    xml: &str,
    settings: &ClientLaunchSettings,
    unmuted_volume: f32,
) -> Result<String, String> {
    settings.validate()?;
    let document = roxmltree::Document::parse(xml)
        .map_err(|_| "Roblox settings XML is invalid; it was not changed.")?;
    let properties = properties(&document)?;
    let fields = [
        ("int", "FramerateCap", settings.fps.to_string()),
        ("int", "GraphicsQualityLevel", settings.graphics.to_string()),
        ("token", "SavedQualityLevel", settings.graphics.to_string()),
        ("int", "QualityResetLevel", settings.graphics.to_string()),
        ("bool", "MaxQualityEnabled", "false".into()),
        ("bool", "Fullscreen", settings.fullscreen.to_string()),
        (
            "float",
            "MasterVolume",
            if settings.muted {
                "0".into()
            } else {
                unmuted_volume.to_string()
            },
        ),
    ];
    let mut edits = Vec::new();
    let mut additions = String::new();
    for (tag, name, value) in fields {
        let mut matches = properties
            .children()
            .filter(|node| node.is_element() && node.attribute("name") == Some(name));
        let node = matches.next();
        if matches.next().is_some() {
            return Err(
                "Roblox settings contain duplicate control properties; nothing was changed.".into(),
            );
        }
        let replacement = format!("<{tag} name=\"{name}\">{value}</{tag}>");
        if let Some(node) = node {
            if !node.has_tag_name(tag) || node.children().any(|child| child.is_element()) {
                return Err(
                    "Roblox control settings have an unsupported format; nothing was changed."
                        .into(),
                );
            }
            edits.push((node.range(), replacement));
        } else {
            additions.push_str(&format!("\n\t\t{replacement}"));
        }
    }
    if !additions.is_empty() {
        let range = properties.range();
        let closing = xml[range.clone()]
            .rfind("</Properties>")
            .ok_or("Roblox settings properties cannot be safely extended.")?;
        let offset = range.start + closing;
        edits.push((offset..offset, additions));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut patched = xml.to_string();
    for (range, replacement) in edits {
        patched.replace_range(range, &replacement);
    }
    roxmltree::Document::parse(&patched)
        .map_err(|_| "Roblox settings could not be safely updated.")?;
    Ok(patched)
}

fn apply_to_file(path: &Path, saved: &SavedSettings) -> Result<(), String> {
    if !saved.settings.enabled {
        return Ok(());
    }
    let xml = read_xml(path)?;
    let patched = patch_xml(&xml, &saved.settings, saved.unmuted_volume)?;
    if patched == xml {
        return Ok(());
    }
    if read_xml(path)? != xml {
        return Err("Roblox settings changed during launch. Try again.".into());
    }
    write_preserving_readonly(path, patched.as_bytes())
}

fn write_preserving_readonly(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let permissions = std::fs::metadata(path)
        .map_err(|_| "Roblox settings could not be inspected. Nothing was launched.")?
        .permissions();
    let readonly = permissions.readonly();
    if readonly {
        let mut writable = permissions.clone();
        // RM is Windows-only; temporarily clear the attribute on this Roblox XML only.
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "Windows-only attribute change; the original permissions are restored after the atomic write"
        )]
        writable.set_readonly(false);
        std::fs::set_permissions(path, writable).map_err(|_| {
            "Roblox settings are read-only and could not be unlocked. Nothing was launched."
        })?;
    }
    let result = storage::atomic_write(path, bytes);
    if readonly {
        std::fs::set_permissions(path, permissions)
            .map_err(|_| "Roblox settings could not have their read-only attribute restored. Nothing was launched; check the file permissions before retrying.")?;
    }
    result.map_err(|_| "Roblox settings could not be saved. Another Roblox client or tool may be locking the file. Nothing was launched.".into())
}

fn prepare_launch_settings(
    source: &Path,
    isolated: &Path,
    saved: &SavedSettings,
) -> Result<Vec<String>, String> {
    if !saved.settings.enabled {
        return Ok(Vec::new());
    }
    if !saved.settings.rm_only {
        apply_to_file(source, saved)?;
        return Ok(Vec::new());
    }
    let xml = read_xml(source)?;
    let patched = patch_xml(&xml, &saved.settings, saved.unmuted_volume)?;
    let path = isolated
        .to_str()
        .ok_or("The isolated Roblox settings path is invalid.")?;
    storage::atomic_write(isolated, patched.as_bytes())
        .map_err(|_| "RM's isolated Roblox settings could not be saved. Nothing was launched.")?;
    Ok(vec!["--globalBasicSettingsPath".into(), path.into()])
}

pub fn apply_before_launch(state: &AppState) -> Result<Vec<String>, String> {
    let preference = preference_path(state)?;
    let saved = load(&preference)?;
    if !saved.settings.enabled {
        return Ok(Vec::new());
    }
    let isolated = preference.with_file_name("roblox-client-settings.xml");
    prepare_launch_settings(&roblox_settings_path()?, &isolated, &saved)
}

#[tauri::command]
pub async fn get_client_launch_settings(
    app: tauri::AppHandle,
) -> Result<ClientLaunchSettings, String> {
    let state = app.state::<AppState>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || Ok(load(&preference_path(&state)?)?.settings))
        .await
        .map_err(|_| "Client settings task failed")?
}

#[tauri::command]
pub async fn save_client_launch_settings(
    app: tauri::AppHandle,
    settings: ClientLaunchSettings,
) -> Result<ClientLaunchSettings, String> {
    settings.validate()?;
    let state = app.state::<AppState>().inner().clone();
    let _queue = state.launch_queue.lock().await;
    let save_state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = preference_path(&save_state)?;
        let mut saved = load(&path).unwrap_or_default();
        if settings.enabled {
            let xml = read_xml(&roblox_settings_path()?)?;
            patch_xml(&xml, &settings, saved.unmuted_volume)?;
            if let Some(volume) = xml_volume(&xml) {
                saved.unmuted_volume = volume;
            }
        }
        saved.settings = settings;
        let bytes = serde_json::to_vec_pretty(&saved)
            .map_err(|_| "Client settings could not be encoded.")?;
        storage::atomic_write(&path, &bytes)
            .map_err(|_| "Client launch settings could not be saved.")?;
        Ok(saved.settings)
    })
    .await
    .map_err(|_| "Client settings save task failed")?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_test_readonly(path: &Path) {
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "Windows-only cleanup of a synthetic test file"
        )]
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn readonly_roblox_xml_is_updated_atomically_and_relocked() {
        let directory =
            std::env::temp_dir().join(format!("rm-readonly-settings-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.xml");
        storage::atomic_write(&path, XML.as_bytes()).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        let saved = SavedSettings {
            settings: ClientLaunchSettings {
                enabled: true,
                fps: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        apply_to_file(&path, &saved).unwrap();
        assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("<int name=\"FramerateCap\">120</int>"));
        assert_eq!(
            std::fs::read(storage::backup_path(&path)).unwrap(),
            XML.as_bytes()
        );
        clear_test_readonly(&path);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn locked_roblox_xml_keeps_original_content_and_readonly_after_failure() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory =
            std::env::temp_dir().join(format!("rm-locked-settings-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.xml");
        storage::atomic_write(&path, XML.as_bytes()).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        let saved = SavedSettings {
            settings: ClientLaunchSettings {
                enabled: true,
                fps: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(apply_to_file(&path, &saved).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), XML.as_bytes());
        assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
        drop(lock);
        clear_test_readonly(&path);
        std::fs::remove_dir_all(directory).unwrap();
    }
    const XML: &str = "<roblox><Item class=\"Other\"><Properties><int name=\"FramerateCap\">1</int></Properties></Item><Item class=\"UserGameSettings\"><Properties><int name=\"FramerateCap\">60</int><float name=\"MasterVolume\">0.25</float><!--keep--><string name=\"Other\">unchanged</string></Properties></Item></roblox>";

    #[test]
    fn older_preferences_default_to_rm_only_and_explicit_choice_round_trips() {
        let old = r#"{"settings":{"enabled":true,"fps":120,"graphics":7,"fullscreen":true,"muted":true},"unmutedVolume":0.25}"#;
        let mut saved: SavedSettings = serde_json::from_str(old).unwrap();
        assert!(saved.settings.rm_only);
        assert_eq!(saved.settings.fps, 120);
        assert_eq!(saved.settings.graphics, 7);
        assert!(saved.settings.fullscreen && saved.settings.muted);
        assert_eq!(saved.unmuted_volume, 0.25);
        saved.settings.rm_only = false;
        let round_trip: SavedSettings =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert!(!round_trip.settings.rm_only);
        assert!(ClientLaunchSettings::default().rm_only);
    }

    #[test]
    fn rm_only_uses_separate_xml_and_leaves_shared_settings_and_backup_untouched() {
        let directory =
            std::env::temp_dir().join(format!("rm-isolated-settings-{}", uuid::Uuid::new_v4()));
        let source = directory.join("Roblox settings.xml");
        let isolated = directory.join("RM settings.xml");
        let saved = SavedSettings {
            settings: ClientLaunchSettings {
                enabled: true,
                fps: 144,
                muted: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            prepare_launch_settings(&source, &isolated, &SavedSettings::default())
                .unwrap()
                .is_empty()
        );
        assert!(!isolated.exists());
        assert!(prepare_launch_settings(&source, &isolated, &saved).is_err());
        storage::atomic_write(&source, XML.as_bytes()).unwrap();
        storage::atomic_write(&storage::backup_path(&source), b"original backup").unwrap();
        let mut permissions = std::fs::metadata(&source).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&source, permissions).unwrap();
        let args = prepare_launch_settings(&source, &isolated, &saved).unwrap();
        assert_eq!(
            args,
            vec!["--globalBasicSettingsPath", isolated.to_str().unwrap()]
        );
        assert_eq!(
            std::fs::read_to_string(&isolated).unwrap(),
            patch_xml(XML, &saved.settings, saved.unmuted_volume).unwrap()
        );
        assert_eq!(std::fs::read(&source).unwrap(), XML.as_bytes());
        assert_eq!(
            std::fs::read(storage::backup_path(&source)).unwrap(),
            b"original backup"
        );
        assert!(std::fs::metadata(&source).unwrap().permissions().readonly());
        clear_test_readonly(&source);
        storage::atomic_write(&source, b"invalid XML").unwrap();
        let previous = std::fs::read(&isolated).unwrap();
        assert!(prepare_launch_settings(&source, &isolated, &saved).is_err());
        assert_eq!(std::fs::read(&isolated).unwrap(), previous);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn shared_mode_updates_normal_settings_without_redirecting_the_player() {
        let directory =
            std::env::temp_dir().join(format!("rm-shared-settings-{}", uuid::Uuid::new_v4()));
        let source = directory.join("settings.xml");
        let isolated = directory.join("isolated.xml");
        storage::atomic_write(&source, XML.as_bytes()).unwrap();
        let saved = SavedSettings {
            settings: ClientLaunchSettings {
                enabled: true,
                rm_only: false,
                fps: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(prepare_launch_settings(&source, &isolated, &saved)
            .unwrap()
            .is_empty());
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            patch_xml(XML, &saved.settings, saved.unmuted_volume).unwrap()
        );
        assert!(!isolated.exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn overrides_change_only_user_settings_and_preserve_unknown_content() {
        let settings = ClientLaunchSettings {
            enabled: true,
            fps: 144,
            graphics: 7,
            fullscreen: true,
            muted: true,
            ..Default::default()
        };
        let result = patch_xml(XML, &settings, 0.25).unwrap();
        assert!(result.contains("<int name=\"FramerateCap\">1</int>"));
        for expected in [
            "<int name=\"FramerateCap\">144</int>",
            "<token name=\"SavedQualityLevel\">7</token>",
            "<bool name=\"Fullscreen\">true</bool>",
            "<float name=\"MasterVolume\">0</float>",
            "<!--keep--><string name=\"Other\">unchanged</string>",
        ] {
            assert!(result.contains(expected));
        }
        assert_eq!(patch_xml(&result, &settings, 0.25).unwrap(), result);
    }

    #[test]
    fn unmuting_restores_the_original_volume() {
        let settings = ClientLaunchSettings::default();
        assert_eq!(xml_volume(XML), Some(0.25));
        let result = patch_xml(XML, &settings, 0.25).unwrap();
        assert!(result.contains("<float name=\"MasterVolume\">0.25</float>"));
    }

    #[test]
    fn invalid_or_ambiguous_settings_are_rejected() {
        assert!(patch_xml("not XML", &ClientLaunchSettings::default(), 0.5).is_err());
        let duplicate = XML.replace("<!--keep-->", "<int name=\"FramerateCap\">30</int>");
        assert!(patch_xml(&duplicate, &ClientLaunchSettings::default(), 0.5).is_err());
        for (fps, graphics) in [(0, 5), (9999, 5), (60, 0), (60, 11)] {
            assert!(ClientLaunchSettings {
                fps,
                graphics,
                ..Default::default()
            }
            .validate()
            .is_err());
        }
    }

    #[test]
    fn disabled_overrides_do_not_require_roblox_settings_and_failed_patches_leave_files_intact() {
        let directory =
            std::env::temp_dir().join(format!("rm-client-settings-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.xml");
        assert!(apply_to_file(&path, &SavedSettings::default()).is_ok());
        storage::atomic_write(&path, b"invalid XML").unwrap();
        let saved = SavedSettings {
            settings: ClientLaunchSettings {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(apply_to_file(&path, &saved).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid XML");
        storage::atomic_write(&path, XML.as_bytes()).unwrap();
        apply_to_file(&path, &saved).unwrap();
        assert_eq!(
            std::fs::read(storage::backup_path(&path)).unwrap(),
            XML.as_bytes()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
