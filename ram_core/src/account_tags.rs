use crate::{
    models::{AccountStore, AccountTag},
    CoreError,
};
use std::collections::HashSet;

pub const MAX_TAGS: usize = 128;

pub fn commit_candidate(
    store: &mut AccountStore,
    update: impl FnOnce(&mut AccountStore) -> Result<(), CoreError>,
    persist: impl FnOnce(&AccountStore) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    let mut candidate = store.clone();
    update(&mut candidate)?;
    persist(&candidate)?;
    *store = candidate;
    Ok(())
}

pub fn validate_name(name: &str) -> Result<String, CoreError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err(CoreError::AccountTags(
            "Tag names must contain 1 to 40 characters, without control characters".into(),
        ));
    }
    Ok(name.to_owned())
}

pub fn validate_ids(store: &AccountStore, ids: &[String]) -> Result<(), CoreError> {
    if ids.len() > MAX_TAGS || ids.iter().collect::<HashSet<_>>().len() != ids.len() {
        return Err(CoreError::AccountTags("Choose each tag only once".into()));
    }
    if ids
        .iter()
        .any(|id| !store.tags.iter().any(|tag| &tag.id == id))
    {
        return Err(CoreError::AccountTags(
            "A selected tag no longer exists. Update your tag selection".into(),
        ));
    }
    Ok(())
}

pub fn save_tag(store: &mut AccountStore, id: Option<&str>, name: &str) -> Result<(), CoreError> {
    save_tag_with_color(store, id, name, None)
}

pub fn save_tag_with_color(
    store: &mut AccountStore,
    id: Option<&str>,
    name: &str,
    color: Option<[u8; 3]>,
) -> Result<(), CoreError> {
    let name = validate_name(name)?;
    if store
        .tags
        .iter()
        .any(|tag| Some(tag.id.as_str()) != id && tag.name.to_lowercase() == name.to_lowercase())
    {
        return Err(CoreError::AccountTags(
            "A tag with this name already exists".into(),
        ));
    }
    if let Some(id) = id {
        let tag = store
            .tags
            .iter_mut()
            .find(|tag| tag.id == id)
            .ok_or_else(|| CoreError::AccountTags("Tag no longer exists".into()))?;
        tag.name = name;
        if let Some(color) = color {
            tag.color = color;
        }
    } else {
        if store.tags.len() >= MAX_TAGS {
            return Err(CoreError::AccountTags(
                "You can create up to 128 tags".into(),
            ));
        }
        store.tags.push(AccountTag {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            color: color.unwrap_or_else(crate::models::default_tag_color),
        });
    }
    Ok(())
}

pub fn delete_tag(store: &mut AccountStore, id: &str) -> Result<(), CoreError> {
    if !store.tags.iter().any(|tag| tag.id == id) {
        return Err(CoreError::AccountTags("Tag no longer exists".into()));
    }
    store.tags.retain(|tag| tag.id != id);
    for account in &mut store.accounts {
        account.tag_ids.retain(|tag_id| tag_id != id);
    }
    Ok(())
}

pub fn assign(
    store: &mut AccountStore,
    user_ids: &[u64],
    tag_ids: &[String],
    remove: bool,
) -> Result<(), CoreError> {
    validate_ids(store, tag_ids)?;
    if user_ids.is_empty() || tag_ids.is_empty() {
        return Err(CoreError::AccountTags(
            "Select at least one account and tag".into(),
        ));
    }
    if user_ids.len() > store.accounts.len()
        || user_ids.iter().collect::<HashSet<_>>().len() != user_ids.len()
        || user_ids.iter().any(|id| store.find_by_id(*id).is_none())
    {
        return Err(CoreError::AccountTags(
            "Account selection changed. Select the accounts again".into(),
        ));
    }
    for account in &mut store.accounts {
        if !user_ids.contains(&account.user_id) {
            continue;
        }
        if remove {
            account.tag_ids.retain(|id| !tag_ids.contains(id));
        } else {
            for id in tag_ids {
                if !account.tag_ids.contains(id) {
                    account.tag_ids.push(id.clone());
                }
            }
        }
    }
    Ok(())
}

pub fn matching_accounts(store: &AccountStore, ids: &[String]) -> Result<Vec<u64>, CoreError> {
    validate_ids(store, ids)?;
    if ids.is_empty() {
        return Err(CoreError::AccountTags("Choose at least one tag".into()));
    }
    Ok(store
        .accounts
        .iter()
        .filter(|account| account.tag_ids.iter().any(|id| ids.contains(id)))
        .map(|account| account.user_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Account, LaunchPreset};

    #[test]
    fn old_tags_load_with_a_neutral_color_and_renames_preserve_new_colors() {
        let legacy: AccountTag = serde_json::from_str(r#"{"id":"legacy","name":"Old"}"#).unwrap();
        assert_eq!(legacy.color, crate::models::default_tag_color());
        let mut store = AccountStore {
            tags: vec![legacy],
            ..Default::default()
        };
        save_tag_with_color(&mut store, Some("legacy"), "Colored", Some([60, 180, 60])).unwrap();
        save_tag(&mut store, Some("legacy"), "Renamed").unwrap();
        assert_eq!(store.tags[0].color, [60, 180, 60]);
        assert_eq!(store.tags[0].id, "legacy");
        let decoded: AccountStore =
            serde_json::from_value(serde_json::to_value(&store).unwrap()).unwrap();
        assert_eq!(decoded.tags, store.tags);
    }

    fn sample() -> AccountStore {
        AccountStore {
            accounts: vec![
                Account::new(1, "one".into(), "One".into()),
                Account::new(2, "two".into(), "Two".into()),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn old_accounts_and_presets_load_without_tags() {
        let mut account =
            serde_json::to_value(Account::new(1, "one".into(), "One".into())).unwrap();
        account.as_object_mut().unwrap().remove("tag_ids");
        let store: AccountStore =
            serde_json::from_value(serde_json::json!({"accounts":[account]})).unwrap();
        assert!(store.tags.is_empty());
        assert!(store.accounts[0].tag_ids.is_empty());
        let preset: LaunchPreset = serde_json::from_str(r#"{"name":"Old","place_id":1}"#).unwrap();
        assert!(preset.tag_ids.is_empty());
    }

    #[test]
    fn tags_stack_and_renames_keep_preset_references() {
        let mut store = sample();
        save_tag(&mut store, None, "Farm").unwrap();
        save_tag(&mut store, None, "Main").unwrap();
        let ids: Vec<_> = store.tags.iter().map(|tag| tag.id.clone()).collect();
        assign(&mut store, &[1], &ids, false).unwrap();
        assign(&mut store, &[1], &ids, false).unwrap();
        assert_eq!(store.accounts[0].tag_ids.len(), 2);
        save_tag(&mut store, Some(&ids[0]), "Renamed").unwrap();
        assert_eq!(matching_accounts(&store, &ids).unwrap(), vec![1]);
        assign(&mut store, &[1], &[ids[0].clone()], true).unwrap();
        assert_eq!(store.accounts[0].tag_ids, vec![ids[1].clone()]);
        delete_tag(&mut store, &ids[1]).unwrap();
        assert!(store.accounts[0].tag_ids.is_empty());
        assert!(matching_accounts(&store, &[ids[1].clone()]).is_err());
    }

    #[test]
    fn validation_and_bulk_failure_do_not_change_assignments() {
        let mut store = sample();
        save_tag(&mut store, None, " Farm ").unwrap();
        let ids = vec![store.tags[0].id.clone()];
        assert!(save_tag(&mut store, None, "farm").is_err());
        assert!(save_tag(&mut store, None, "bad\nname").is_err());
        assert!(assign(&mut store, &[1, 999], &ids, false).is_err());
        assert!(store
            .accounts
            .iter()
            .all(|account| account.tag_ids.is_empty()));
        assert!(assign(&mut store, &[1], &["unknown".into()], false).is_err());
        assert!(validate_ids(&store, &[ids[0].clone(), ids[0].clone()]).is_err());
    }

    #[test]
    fn failed_save_keeps_catalog_and_assignments_unchanged() {
        let mut store = sample();
        save_tag(&mut store, None, "Farm").unwrap();
        let ids = vec![store.tags[0].id.clone()];
        assign(&mut store, &[1], &ids, false).unwrap();
        let before = serde_json::to_value(&store).unwrap();
        assert!(commit_candidate(
            &mut store,
            |candidate| delete_tag(candidate, &ids[0]),
            |_| Err(CoreError::AccountTags("synthetic save failure".into()))
        )
        .is_err());
        assert_eq!(serde_json::to_value(store).unwrap(), before);
    }

    #[test]
    fn encrypted_store_and_backup_preserve_tag_metadata() {
        let directory = std::env::temp_dir().join(format!("rm-tags-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("accounts.dat");
        let session = crate::crypto::create_password_session("synthetic-password").unwrap();
        let mut store = sample();
        save_tag_with_color(&mut store, None, "Synthetic tag", Some([60, 180, 60])).unwrap();
        crate::crypto::save_store(&path, &store, &session).unwrap();
        let ids = vec![store.tags[0].id.clone()];
        commit_candidate(
            &mut store,
            |candidate| assign(candidate, &[1], &ids, false),
            |candidate| {
                crate::crypto::save_store(&path, candidate, &session)
                    .map_err(|_| CoreError::AccountTags("save failed".into()))
            },
        )
        .unwrap();
        let (loaded, _) = crate::crypto::unlock_with_password(&path, "synthetic-password").unwrap();
        assert_eq!(loaded.tags, store.tags);
        assert_eq!(loaded.accounts[0].tag_ids, ids);
        let (backup, _) = crate::crypto::unlock_with_password(
            &crate::storage::backup_path(&path),
            "synthetic-password",
        )
        .unwrap();
        assert_eq!(backup.tags, store.tags);
        assert!(backup.accounts[0].tag_ids.is_empty());
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes
            .windows(b"Synthetic tag".len())
            .any(|window| window == b"Synthetic tag"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
