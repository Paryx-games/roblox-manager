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
    moderation: HashMap<u64, (bool, Option<String>, Option<DateTime<Utc>>)>,
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
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.generation += 1;
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileHistory {
    schema: u8,
    events: Vec<HistoryEvent>,
}

pub fn save(path: &Path, events: &[HistoryEvent], session: &StoreSession) -> Result<(), CoreError> {
    let payload = serde_json::to_string(&FileHistory {
        schema: 1,
        events: events.to_vec(),
    })?;
    // reuse the existing authenticated envelope primitive; no new keys or crypto.
    let sealed = crypto::encrypt_cookie(&payload, session)?;
    storage::atomic_write(path, sealed.as_bytes())
}

fn read(path: &Path, session: &StoreSession) -> Result<Vec<HistoryEvent>, CoreError> {
    if std::fs::metadata(path)?.len() > MAX_FILE_BYTES {
        return Err(CoreError::Crypto("History file is too large".into()));
    }
    let sealed = std::fs::read_to_string(path)?;
    let payload = crypto::decrypt_cookie(&sealed, session)?;
    let file: FileHistory = serde_json::from_str(&payload)?;
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
        return Err(CoreError::Crypto("History format is unsupported".into()));
    }
    Ok(file.events)
}

pub fn load(path: &Path, session: &StoreSession) -> Result<Vec<HistoryEvent>, CoreError> {
    if !path.exists() && !storage::backup_path(path).exists() {
        return Ok(Vec::new());
    }
    read(path, session).or_else(|_| read(&storage::backup_path(path), session))
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
    fn encrypted_roundtrip_and_backup_recovery() {
        let dir = std::env::temp_dir().join(format!("rm-history-{}", uuid::Uuid::new_v4()));
        let path = dir.join("history.dat");
        let session = crypto::create_password_session("synthetic-history-password").unwrap();
        let mut history = History::default();
        history.observe_presence(7, &presence(2, Some(10)), Utc::now());
        save(&path, &history.events, &session).unwrap();
        save(&path, &history.events, &session).unwrap();
        assert!(!std::fs::read_to_string(&path)
            .unwrap()
            .contains("observedAt"));
        storage::atomic_swap(&path, b"corrupt").unwrap();
        assert_eq!(load(&path, &session).unwrap().len(), 1);
        let wrong = crypto::create_password_session("other-synthetic-password").unwrap();
        assert!(load(&path, &wrong).is_err());
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
