//! Join target validation and crash-recoverable temporary-follow obligations.
use crate::{
    crypto::{self, StoreSession},
    error::CoreError,
    storage,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum JoinMode {
    VisibleServer,
    TemporaryFollow,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    UserId(u64),
    Username(String),
}

pub fn parse_target(value: &str) -> Result<Target, &'static str> {
    let value = value.trim().trim_start_matches('@');
    if value.bytes().all(|b| b.is_ascii_digit()) && !value.is_empty() {
        return value
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(Target::UserId)
            .ok_or("Enter a valid nonzero user ID");
    }
    if !(3..=20).contains(&value.len())
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("Enter an exact Roblox username or a numeric user ID");
    }
    Ok(Target::Username(value.to_string()))
}

pub fn account_ids(ids: Vec<u64>) -> Result<Vec<u64>, &'static str> {
    if ids.is_empty() || ids.len() > 500 || ids.contains(&0) {
        return Err("Select between 1 and 500 valid accounts");
    }
    let mut seen = std::collections::HashSet::new();
    Ok(ids.into_iter().filter(|id| seen.insert(*id)).collect())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingFollow {
    pub user_id: u64,
    pub target_user_id: u64,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

pub struct TemporaryJoinOutcome {
    pub requested: bool,
    pub message: &'static str,
    pub cleanup_pending: bool,
}

pub trait TemporaryJoinActions {
    fn stopped(&self) -> bool;
    fn progress(&self, message: &'static str);
    fn visible(
        &self,
    ) -> impl std::future::Future<Output = Result<Option<(u64, String)>, CoreError>> + Send;
    fn following(&self) -> impl std::future::Future<Output = Result<bool, CoreError>> + Send;
    fn journal(&self, add: bool)
        -> impl std::future::Future<Output = Result<(), CoreError>> + Send;
    fn follow(&self) -> impl std::future::Future<Output = Result<(), CoreError>> + Send;
    fn unfollow(&self) -> impl std::future::Future<Output = Result<(), CoreError>> + Send;
    fn launch(
        &self,
        server: (u64, String),
    ) -> impl std::future::Future<Output = Result<(), CoreError>> + Send;
}

pub async fn temporary_join(
    actions: &impl TemporaryJoinActions,
) -> Result<TemporaryJoinOutcome, CoreError> {
    let cancelled = || TemporaryJoinOutcome {
        requested: false,
        message: "Cancelled or account changed before launch",
        cleanup_pending: false,
    };
    let hidden = || TemporaryJoinOutcome {
        requested: false,
        message: "Roblox hides the server or the target is not in a game",
        cleanup_pending: false,
    };
    actions.progress("Checking visible server and existing follow state");
    let server = actions.visible().await?;
    if actions.stopped() {
        return Ok(cancelled());
    }
    if let Some(server) = server {
        actions.progress("Launching into the target server");
        actions.launch(server).await?;
        return Ok(TemporaryJoinOutcome {
            requested: true,
            message: "Launch requested. Check Instances for progress.",
            cleanup_pending: false,
        });
    }
    if actions.following().await? {
        return Ok(hidden());
    }
    if actions.stopped() {
        return Ok(cancelled());
    }
    // persist before a request: network failure can conceal successful follow creation.
    actions.journal(true).await?;
    let mut attempted = false;
    let result = async {
        if actions.stopped() {
            return Ok(cancelled());
        }
        actions.progress("Temporarily following the target");
        attempted = true;
        actions.follow().await?;
        if actions.stopped() {
            return Ok(cancelled());
        }
        actions.progress("Checking target server after follow");
        let server = actions.visible().await?;
        if actions.stopped() {
            return Ok(cancelled());
        }
        if let Some(server) = server {
            actions.progress("Launching into the target server");
            actions.launch(server).await?;
            Ok(TemporaryJoinOutcome {
                requested: true,
                message: "Launch requested. Check Instances for progress.",
                cleanup_pending: false,
            })
        } else {
            Ok(hidden())
        }
    }
    .await;
    actions.progress("Restoring the previous follow state");
    let cleanup_pending = if attempted && actions.unfollow().await.is_err() {
        true
    } else {
        actions.journal(false).await.is_err()
    };
    result.map(|mut outcome| {
        outcome.cleanup_pending = cleanup_pending;
        outcome
    })
}

pub fn save_pending(
    path: &Path,
    entries: &[PendingFollow],
    session: &StoreSession,
) -> Result<(), CoreError> {
    let json = serde_json::to_string(entries)?;
    let encrypted = crypto::encrypt_cookie(&json, session)?;
    // avoid retaining obsolete obligations in a backup after cleanup succeeded.
    storage::atomic_swap(path, encrypted.as_bytes())
}

pub fn load_pending(path: &Path, session: &StoreSession) -> Result<Vec<PendingFollow>, CoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    if std::fs::metadata(path)?.len() > 1024 * 1024 {
        return Err(CoreError::Crypto("Follow cleanup file is too large".into()));
    }
    let payload = crypto::decrypt_cookie(&std::fs::read_to_string(path)?, session)?;
    let entries: Vec<PendingFollow> = serde_json::from_str(&payload)?;
    if entries.len() > 1000
        || entries
            .iter()
            .any(|e| e.user_id == 0 || e.target_user_id == 0 || e.user_id == e.target_user_id)
    {
        return Err(CoreError::Crypto("Invalid follow cleanup file".into()));
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn targets_are_exact_and_ids_are_deduplicated() {
        assert_eq!(parse_target("1").unwrap(), Target::UserId(1));
        assert_eq!(
            parse_target(" @Example_1 ").unwrap(),
            Target::Username("Example_1".into())
        );
        for bad in [
            "0",
            "",
            "18446744073709551616",
            "abc/xyz",
            "abc\nxyz",
            "longusernameover20letters",
        ] {
            assert!(parse_target(bad).is_err());
        }
        assert_eq!(account_ids(vec![2, 1, 2]).unwrap(), vec![2, 1]);
        assert!(account_ids(vec![0]).is_err());
        assert!(account_ids(vec![1; 501]).is_err());
    }
    #[test]
    fn cleanup_journal_is_encrypted_and_removal_is_durable() {
        let dir = std::env::temp_dir().join(format!("rm-join-journal-{}", uuid::Uuid::new_v4()));
        let file = dir.join("pending.dat");
        let session = crypto::create_password_session("synthetic-join-password").unwrap();
        let entry = PendingFollow {
            user_id: 1,
            target_user_id: 2,
            recorded_at: chrono::Utc::now(),
        };
        save_pending(&file, &[entry], &session).unwrap();
        assert_eq!(load_pending(&file, &session).unwrap().len(), 1);
        assert!(!std::fs::read_to_string(&file)
            .unwrap()
            .contains("targetUserId"));
        save_pending(&file, &[], &session).unwrap();
        assert!(load_pending(&file, &session).unwrap().is_empty());
        assert!(!storage::backup_path(&file).exists());
        storage::atomic_swap(&file, b"corrupt").unwrap();
        assert!(load_pending(&file, &session).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[derive(Default)]
    struct Fake {
        calls: std::sync::Mutex<Vec<&'static str>>,
        fail: &'static str,
        already_visible: bool,
        already_following: bool,
        cancel_after_follow: bool,
        cancelled: std::sync::atomic::AtomicBool,
    }
    impl Fake {
        fn step(&self, name: &'static str) -> Result<(), CoreError> {
            self.calls.lock().unwrap().push(name);
            if self.fail == name {
                Err(CoreError::RateLimited)
            } else {
                Ok(())
            }
        }
    }
    impl TemporaryJoinActions for Fake {
        fn stopped(&self) -> bool {
            self.cancelled.load(std::sync::atomic::Ordering::Acquire)
        }
        fn progress(&self, _: &'static str) {}
        async fn visible(&self) -> Result<Option<(u64, String)>, CoreError> {
            self.step("visible")?;
            Ok(
                (self.already_visible || self.calls.lock().unwrap().contains(&"follow"))
                    .then(|| (1, "11111111-2222-3333-4444-555555555555".into())),
            )
        }
        async fn following(&self) -> Result<bool, CoreError> {
            self.step("following")?;
            Ok(self.already_following)
        }
        async fn journal(&self, add: bool) -> Result<(), CoreError> {
            self.step(if add { "journal-add" } else { "journal-remove" })
        }
        async fn follow(&self) -> Result<(), CoreError> {
            self.step("follow")?;
            if self.cancel_after_follow {
                self.cancelled
                    .store(true, std::sync::atomic::Ordering::Release);
            }
            Ok(())
        }
        async fn unfollow(&self) -> Result<(), CoreError> {
            self.step("unfollow")
        }
        async fn launch(&self, _: (u64, String)) -> Result<(), CoreError> {
            self.step("launch")
        }
    }
    #[tokio::test]
    async fn visible_and_existing_follows_are_preserved() {
        let fake = Fake {
            already_visible: true,
            ..Default::default()
        };
        assert!(temporary_join(&fake).await.unwrap().requested);
        assert_eq!(*fake.calls.lock().unwrap(), vec!["visible", "launch"]);
        let fake = Fake {
            already_following: true,
            ..Default::default()
        };
        assert!(!temporary_join(&fake).await.unwrap().requested);
        assert_eq!(*fake.calls.lock().unwrap(), vec!["visible", "following"]);
    }
    #[tokio::test]
    async fn cancellation_and_failures_always_restore_temporary_follows() {
        let fake = Fake {
            cancel_after_follow: true,
            ..Default::default()
        };
        assert!(!temporary_join(&fake).await.unwrap().requested);
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![
                "visible",
                "following",
                "journal-add",
                "follow",
                "unfollow",
                "journal-remove"
            ]
        );
        for step in ["follow", "launch"] {
            let fake = Fake {
                fail: step,
                ..Default::default()
            };
            assert!(temporary_join(&fake).await.is_err());
            let calls = fake.calls.lock().unwrap();
            assert!(calls.contains(&"unfollow"));
            assert_eq!(calls.last(), Some(&"journal-remove"));
        }
        let fake = Fake {
            fail: "journal-add",
            ..Default::default()
        };
        assert!(temporary_join(&fake).await.is_err());
        assert!(!fake.calls.lock().unwrap().contains(&"follow"));
    }
    #[tokio::test]
    async fn cleanup_failures_remain_journalled() {
        let fake = Fake {
            fail: "unfollow",
            ..Default::default()
        };
        let outcome = temporary_join(&fake).await.unwrap();
        assert!(outcome.requested);
        assert!(outcome.cleanup_pending);
        assert!(!fake.calls.lock().unwrap().contains(&"journal-remove"));
        let fake = Fake {
            fail: "journal-remove",
            ..Default::default()
        };
        assert!(temporary_join(&fake).await.unwrap().cleanup_pending);
    }
}
