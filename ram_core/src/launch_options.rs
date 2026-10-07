//! Local launch preparation; never interpret custom arguments through a shell.
use crate::{error::CoreError, storage};
use serde_json::{Map, Value};
use std::{collections::HashMap, path::Path};

/// Parse Windows quoting and backslash rules, with no shell expansion.
pub fn parse_arguments(input: &str) -> Result<Vec<String>, CoreError> {
    let invalid = || {
        CoreError::Process(
            "Custom arguments are invalid, unclosed, or contain reserved launch parameters".into(),
        )
    };
    if input.len() > 4096 || input.chars().any(char::is_control) {
        return Err(invalid());
    }
    let chars: Vec<_> = input.chars().collect();
    let mut args = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index == chars.len() {
            break;
        }
        let mut arg = String::new();
        let mut quoted = false;
        while index < chars.len() && (quoted || !chars[index].is_whitespace()) {
            let mut slashes = 0;
            while index < chars.len() && chars[index] == '\\' {
                slashes += 1;
                index += 1;
            }
            if index < chars.len() && chars[index] == '"' {
                arg.extend(std::iter::repeat_n('\\', slashes / 2));
                if slashes % 2 != 0 {
                    arg.push('"');
                } else if quoted && chars.get(index + 1) == Some(&'"') {
                    arg.push('"');
                    index += 1;
                } else {
                    quoted = !quoted;
                }
                index += 1;
            } else {
                arg.extend(std::iter::repeat_n('\\', slashes));
                if index == chars.len() || (!quoted && chars[index].is_whitespace()) {
                    break;
                }
                arg.push(chars[index]);
                index += 1;
            }
        }
        if quoted {
            return Err(invalid());
        }
        let lower = arg.to_ascii_lowercase();
        if [
            "roblox-player:",
            "gameinfo:",
            "launchtime:",
            "placelauncherurl:",
            "launchmode:",
            ".roblosecurity",
            "x-csrf-token",
            "rbx-authentication-ticket",
        ]
        .iter()
        .any(|reserved| lower.contains(reserved))
        {
            return Err(invalid());
        }
        args.push(arg);
        if args.len() > 64 {
            return Err(invalid());
        }
    }
    Ok(args)
}

pub fn validate_fast_flags(flags: &HashMap<String, String>) -> Result<(), CoreError> {
    if flags.len() > 100
        || flags.iter().any(|(key, value)| {
            key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || value.len() > 4096
                || value.chars().any(char::is_control)
        })
    {
        return Err(CoreError::Process(
            "Fast flags must have valid names and values".into(),
        ));
    }
    Ok(())
}

fn read_object(path: &Path) -> Result<Map<String, Value>, CoreError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(_) => {
            return Err(CoreError::Process(
                "Fast flag settings could not be read".into(),
            ))
        }
    };
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(CoreError::Process(
            "Fast flag settings have an unsupported size".into(),
        ));
    }
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|_| {
        CoreError::Process(
            "Existing fast flag settings are invalid; they were not overwritten".into(),
        )
    })
}

/// Preserve unrelated flags and remember original values for RM-owned keys.
/// The journal is durable before editing Roblox's file; interrupted writes can be retried.
pub fn apply_fast_flags(
    executable: &Path,
    flags: &HashMap<String, String>,
) -> Result<(), CoreError> {
    validate_fast_flags(flags)?;
    let directory = executable
        .parent()
        .ok_or_else(|| CoreError::Process("Roblox installation unavailable".into()))?
        .join("ClientSettings");
    let path = directory.join("ClientAppSettings.json");
    let journal_path = directory.join("rm-original-fast-flags.json");
    if flags.is_empty() && !journal_path.exists() {
        return Ok(());
    }
    let mut current = read_object(&path)?;
    let mut originals = read_object(&journal_path)?;
    for (key, original) in &originals {
        if !flags.contains_key(key) {
            if original.is_null() {
                current.remove(key);
            } else {
                current.insert(key.clone(), original.clone());
            }
        }
    }
    for (key, value) in flags {
        originals
            .entry(key.clone())
            .or_insert_with(|| current.get(key).cloned().unwrap_or(Value::Null));
        current.insert(key.clone(), Value::String(value.clone()));
    }
    std::fs::create_dir_all(&directory)?;
    storage::atomic_write(&journal_path, &serde_json::to_vec_pretty(&originals)?)?;
    storage::atomic_write(&path, &serde_json::to_vec_pretty(&current)?)?;
    originals.retain(|key, _| flags.contains_key(key));
    storage::atomic_write(&journal_path, &serde_json::to_vec_pretty(&originals)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_arguments_preserve_quotes_backslashes_and_literal_shell_text() {
        assert_eq!(
            parse_arguments(r#"--flag "two words" "C:\folder\\" a\"b "" "$PATH;$(foo)""#).unwrap(),
            vec![
                "--flag",
                "two words",
                "C:\\folder\\",
                "a\"b",
                "",
                "$PATH;$(foo)"
            ]
        );
        assert_eq!(parse_arguments("   ").unwrap(), Vec::<String>::new());
        assert_eq!(parse_arguments(r#""a""b""#).unwrap(), vec!["a\"b"]);
    }
    #[test]
    fn invalid_and_reserved_arguments_fail_without_echoing_input() {
        for input in [
            "--flag\nsecret",
            "\"unfinished",
            "roblox-player:secret",
            "+gameinfo:secret",
            "+launchtime:123",
            "+launchmode:play",
            ".ROBLOSECURITY=secret",
        ] {
            let error = parse_arguments(input).unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
    }
    #[test]
    fn fast_flags_restore_originals_and_preserve_other_tools_settings() {
        let directory = std::env::temp_dir().join(format!("rm-flags-{}", uuid::Uuid::new_v4()));
        let executable = directory.join("RobloxPlayerBeta.exe");
        let path = directory.join("ClientSettings/ClientAppSettings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        storage::atomic_write(&path, br#"{"FFlagOriginal":true,"Other":17}"#).unwrap();
        let desired = HashMap::from([
            ("FFlagOriginal".into(), "False".into()),
            ("FFlagNew".into(), "True".into()),
        ]);
        apply_fast_flags(&executable, &desired).unwrap();
        assert_eq!(read_object(&path).unwrap()["Other"], 17);
        assert_eq!(read_object(&path).unwrap()["FFlagNew"], "True");
        apply_fast_flags(&executable, &desired).unwrap();
        apply_fast_flags(&executable, &HashMap::new()).unwrap();
        assert_eq!(
            read_object(&path).unwrap(),
            serde_json::from_str::<Map<String, Value>>(r#"{"FFlagOriginal":true,"Other":17}"#)
                .unwrap()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn corrupt_flags_are_preserved_and_interrupted_writes_recover() {
        let directory = std::env::temp_dir().join(format!("rm-flags-{}", uuid::Uuid::new_v4()));
        let executable = directory.join("RobloxPlayerBeta.exe");
        let path = directory.join("ClientSettings/ClientAppSettings.json");
        let journal = directory.join("ClientSettings/rm-original-fast-flags.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        storage::atomic_write(&path, b"broken JSON").unwrap();
        let flags = HashMap::from([("FFlagNew".into(), "True".into())]);
        assert!(apply_fast_flags(&executable, &flags).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken JSON");
        // an interrupted apply recorded ownership before changing Roblox's file
        storage::atomic_write(&path, br#"{"Other":1}"#).unwrap();
        storage::atomic_write(&journal, br#"{"FFlagNew":null}"#).unwrap();
        apply_fast_flags(&executable, &flags).unwrap();
        assert_eq!(read_object(&path).unwrap()["FFlagNew"], "True");
        apply_fast_flags(&executable, &HashMap::new()).unwrap();
        assert!(!read_object(&path).unwrap().contains_key("FFlagNew"));
        assert!(
            validate_fast_flags(&HashMap::from([("../escape".into(), "True".into())])).is_err()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
