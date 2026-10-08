//! Observed account activity, independent of credentials and UI state.

use crate::{
    crypto::{self, StoreSession},
    error::CoreError,
    models::{ModerationInfo, Presence},
    redact, storage,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

pub const MAX_EVENTS: usize = 10_000;
type ModerationFingerprint = (bool, Option<String>, Option<DateTime<Utc>>);
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Observed,
    Joined,
    Left,
    ChangedGame,
    ChangedServer,
    Online,
    Offline,
    Studio,
    LeftStudio,
    Unknown,
    Moderated,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryEvent {
    pub id: u64,
    pub observed_at: DateTime<Utc>,
    pub user_id: u64,
    pub kind: EventKind,
    pub location: String,
    pub place_id: Option<u64>,
    pub job_id: Option<String>,
}

#[derive(Default)]
pub struct History {
    pub events: Vec<HistoryEvent>,
    pub generation: u64,
    pub saved_generation: u64,
    pub loaded: bool,
    pub error: Option<String>,
    presence: HashMap<u64, Presence>,
    moderation: HashMap<u64, ModerationFingerprint>,
    next_id: u64,
}

fn clean_text(text: &str) -> String {
    redact::scrub(text)
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect()
}

pub fn server(presence: &Presence) -> Option<(u64, String)> {
    if presence.user_presence_type != 2 {
        return None;
    }
    let place = presence.place_id.filter(|id| *id > 0)?;
    let job = presence.game_id.as_deref()?;
    let parsed = uuid::Uuid::parse_str(job).ok()?;
    if parsed.is_nil() {
        return None;
    }
    Some((place, parsed.hyphenated().to_string()))
}

impl History {
    fn push(&mut self, user_id: u64, kind: EventKind, presence: &Presence, now: DateTime<Utc>) {
        self.next_id += 1;
        self.events.push(HistoryEvent {
            id: self.next_id,
            observed_at: now,
            user_id,
            kind,
            location: clean_text(&presence.last_location),
            place_id: presence.place_id.filter(|id| *id > 0),
            job_id: server(presence).map(|(_, job)| job),
        });
        if self.events.len() > MAX_EVENTS {
            self.events.drain(..self.events.len() - MAX_EVENTS);
        }
        self.generation += 1;
    }

    pub fn observe_presence(&mut self, user_id: u64, current: &Presence, now: DateTime<Utc>) {
        let previous = self.presence.insert(user_id, current.clone());
        let Some(previous) = previous else {
            self.push(user_id, EventKind::Observed, current, now);
            return;
        };
        if previous.user_presence_type != current.user_presence_type {
            if previous.user_presence_type == 2 {
                self.push(user_id, EventKind::Left, &previous, now);
            }
            if previous.user_presence_type == 3 {
                self.push(user_id, EventKind::LeftStudio, &previous, now);
            }
            let kind = match current.user_presence_type {
                0 => EventKind::Offline,
                1 => EventKind::Online,
                2 => EventKind::Joined,
                3 => EventKind::Studio,
                _ => EventKind::Unknown,
            };
            self.push(user_id, kind, current, now);
        } else if current.user_presence_type == 2 {
            // missing identifiers mean hidden/partial presence, not a game transition.
            if previous
                .place_id
                .zip(current.place_id)
                .is_some_and(|(a, b)| a != b)
            {
                self.push(user_id, EventKind::ChangedGame, current, now);
            } else if previous
                .game_id
                .as_ref()
                .zip(current.game_id.as_ref())
                .is_some_and(|(a, b)| a != b)
            {
                self.push(user_id, EventKind::ChangedServer, current, now);
            }
        }
    }

    pub fn observe_moderation(
        &mut self,
        user_id: u64,
        info: Option<&ModerationInfo>,
        now: DateTime<Utc>,
    ) {
        let Some(info) = info else {
            return;
        };
        let fingerprint = (info.is_banned, info.reason.clone(), info.expires_at);
        let changed = self.moderation.get(&user_id) != Some(&fingerprint);
        self.moderation.insert(user_id, fingerprint);
        if changed && info.is_active() {
            self.push(user_id, EventKind::Moderated, &Presence::default(), now);
        }
    }

    pub fn restore(&mut self, events: Vec<HistoryEvent>) {
        self.next_id = events.iter().map(|e| e.id).max().unwrap_or(0);
        self.events = events;
        self.loaded = true;
        self.error = None;
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.presence.clear();
        self.moderation.clear();
        self.generation += 1;
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileHistory {
    schema: u8,
    events: Vec<HistoryEvent>,
}

pub fn save(path: &Path, events: &[HistoryEvent]) -> Result<(), CoreError> {
    let payload = serde_json::to_vec_pretty(&FileHistory {
        schema: 1,
        events: events.to_vec(),
    })?;
    storage::atomic_write(path, &payload)
}

fn read_payload(path: &Path) -> Result<String, CoreError> {
    if std::fs::metadata(path)?.len() > MAX_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "History file is too large",
        )
        .into());
    }
    Ok(std::fs::read_to_string(path)?)
}

fn parse(payload: &str) -> Result<Vec<HistoryEvent>, CoreError> {
    let file: FileHistory = serde_json::from_str(payload)?;
    if file.schema != 1
        || file.events.len() > MAX_EVENTS
        || file.events.iter().any(|e| {
            e.id == u64::MAX
                || e.user_id == 0
                || e.location != clean_text(&e.location)
                || e.job_id
                    .as_ref()
                    .is_some_and(|job| uuid::Uuid::parse_str(job).is_err())
        })
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "History format is unsupported",
        )
        .into());
    }
    Ok(file.events)
}

pub fn load(path: &Path) -> Result<Vec<HistoryEvent>, CoreError> {
    if !path.exists() && !storage::backup_path(path).exists() {
        return Ok(Vec::new());
    }
    read_payload(path)
        .and_then(|payload| parse(&payload))
        .or_else(|_| read_payload(&storage::backup_path(path)).and_then(|payload| parse(&payload)))
}

pub fn load_or_migrate(
    path: &Path,
    session: Option<&StoreSession>,
) -> Result<Vec<HistoryEvent>, CoreError> {
    if path.exists() || storage::backup_path(path).exists() {
        return load(path);
    }
    let legacy = path.with_extension("dat");
    if !legacy.exists() && !storage::backup_path(&legacy).exists() {
        return Ok(Vec::new());
    }
    let session = session.ok_or_else(|| {
        CoreError::Crypto("Unlock the account store to migrate legacy history".into())
    })?;
    let read = |file: &Path| parse(&crypto::decrypt_cookie(&read_payload(file)?, session)?);
    let events = read(&legacy).or_else(|_| read(&storage::backup_path(&legacy)))?;
    // keep legacy primary/backup intact until explicit clear; failed writes can retry.
    save(path, &events)?;
    Ok(events)
}

pub fn clear_saved(path: &Path) -> Result<(), CoreError> {
    let legacy = path.with_extension("dat");
    let legacy_backup = storage::backup_path(&legacy);
    if path.exists()
        || storage::backup_path(path).exists()
        || legacy.exists()
        || legacy_backup.exists()
    {
        save(path, &[])?;
        let empty = std::fs::read(path)?;
        storage::atomic_swap(&storage::backup_path(path), &empty)?;
        for file in [&legacy, &legacy_backup] {
            if file.exists() {
                std::fs::remove_file(file)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn presence(kind: u8, place: Option<u64>) -> Presence {
        Presence {
            user_presence_type: kind,
            place_id: place,
            ..Default::default()
        }
    }
    #[test]
    fn changes_and_duplicate_samples() {
        let mut history = History::default();
        let now = Utc::now();
        for p in [
            presence(0, None),
            presence(2, Some(1)),
            presence(2, Some(1)),
            presence(2, Some(2)),
            presence(3, None),
            presence(0, None),
        ] {
            history.observe_presence(1, &p, now);
        }
        assert_eq!(
            history.events.iter().map(|e| &e.kind).collect::<Vec<_>>(),
            vec![
                &EventKind::Observed,
                &EventKind::Joined,
                &EventKind::ChangedGame,
                &EventKind::Left,
                &EventKind::Studio,
                &EventKind::LeftStudio,
                &EventKind::Offline
            ]
        );
    }
    #[test]
    fn partial_and_unknown_presence_are_not_offline() {
        let mut history = History::default();
        let now = Utc::now();
        history.observe_presence(1, &presence(2, Some(1)), now);
        history.observe_presence(1, &presence(2, None), now);
        assert_eq!(history.events.len(), 1);
        history.observe_presence(1, &presence(99, None), now);
        assert_eq!(history.events.last().unwrap().kind, EventKind::Unknown);
    }
    #[test]
    fn moderation_ignores_check_time_and_unknown_results() {
        let mut history = History::default();
        let now = Utc::now();
        let mut info = ModerationInfo {
            is_banned: true,
            ..Default::default()
        };
        history.observe_moderation(1, Some(&info), now);
        info.last_checked = Some(now);
        history.observe_moderation(1, Some(&info), now);
        history.observe_moderation(1, None, now);
        assert_eq!(history.events.len(), 1);
    }

    #[test]
    fn clearing_reestablishes_presence_and_active_moderation_baselines() {
        let mut history = History::default();
        let now = Utc::now();
        let current = presence(2, Some(1));
        let moderation = ModerationInfo {
            is_banned: true,
            ..Default::default()
        };
        history.observe_presence(1, &current, now);
        history.observe_moderation(1, Some(&moderation), now);
        history.clear();
        assert!(history.events.is_empty());
        history.observe_presence(1, &current, now);
        history.observe_moderation(1, Some(&moderation), now);
        assert_eq!(
            history
                .events
                .iter()
                .map(|event| &event.kind)
                .collect::<Vec<_>>(),
            vec![&EventKind::Observed, &EventKind::Moderated]
        );
    }
    #[test]
    fn json_roundtrip_and_backup_recovery() {
        let dir = std::env::temp_dir().join(format!("rm-history-{}", uuid::Uuid::new_v4()));
        let path = dir.join("history.json");
        let mut history = History::default();
        history.observe_presence(7, &presence(2, Some(10)), Utc::now());
        save(&path, &history.events).unwrap();
        save(&path, &history.events).unwrap();
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("observedAt"));
        storage::atomic_swap(&path, b"corrupt").unwrap();
        assert_eq!(load(&path).unwrap().len(), 1);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(load(&path).unwrap().len(), 1);
        clear_saved(&path).unwrap();
        assert!(load(&path).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migration_recovers_legacy_backup_and_preserves_sources_on_failure() {
        let dir =
            std::env::temp_dir().join(format!("rm-history-migration-{}", uuid::Uuid::new_v4()));
        let path = dir.join("history.json");
        let legacy = path.with_extension("dat");
        let session = crypto::create_password_session(&uuid::Uuid::new_v4().to_string()).unwrap();
        let mut history = History::default();
        history.observe_presence(7, &presence(2, Some(10)), Utc::now());
        let payload = serde_json::to_string(&FileHistory {
            schema: 1,
            events: history.events,
        })
        .unwrap();
        let sealed = crypto::encrypt_cookie(&payload, &session).unwrap();
        storage::atomic_write(&legacy, sealed.as_bytes()).unwrap();
        storage::atomic_write(&legacy, b"corrupt").unwrap();
        assert!(load_or_migrate(&path, None).is_err());
        assert!(!path.exists());
        let blocking_parent = dir.join("blocked");
        storage::atomic_swap(&blocking_parent, b"not a directory").unwrap();
        let failed_path = blocking_parent.join("history.json");
        // make the same legacy source available beside a target that cannot be written.
        std::fs::create_dir(&path).unwrap();
        assert!(load_or_migrate(&path, Some(&session)).is_err());
        assert_eq!(
            std::fs::read(storage::backup_path(&legacy)).unwrap(),
            sealed.as_bytes()
        );
        assert!(save(&failed_path, &[]).is_err());
        std::fs::remove_dir(&path).unwrap();
        assert_eq!(load_or_migrate(&path, Some(&session)).unwrap().len(), 1);
        assert!(legacy.exists());
        assert_eq!(load_or_migrate(&path, None).unwrap().len(), 1);
        clear_saved(&path).unwrap();
        assert!(!legacy.exists());
        assert!(!storage::backup_path(&legacy).exists());
        assert!(load(&path).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_server_and_old_config() {
        let mut p = presence(2, Some(1));
        p.game_id = Some("bad+argument".into());
        assert!(server(&p).is_none());
        let mut json = serde_json::to_value(crate::models::AppConfig::default()).unwrap();
        json.as_object_mut()
            .unwrap()
            .remove("session_history_persist");
        assert!(
            !serde_json::from_value::<crate::models::AppConfig>(json)
                .unwrap()
                .session_history_persist
        );
    }
}
