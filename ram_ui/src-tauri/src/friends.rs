use crate::state::AppState;
use ram_core::{
    auth::RobloxClient,
    cache::ResponseCache,
    friends::{
        self, FriendAction, FriendGroup, FriendProfile, FriendUser, FriendsSnapshot, RequestsPage,
    },
};
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Duration};
use tauri::{Emitter, Manager};

type AccountKey = (u64, u64);
type DetailKey = (u64, u64, u64, String);
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    profile: Option<FriendProfile>,
    mutual: Vec<FriendUser>,
    groups: Vec<FriendGroup>,
}

pub struct FriendsState {
    snapshots: ResponseCache<AccountKey, FriendsSnapshot>,
    requests: ResponseCache<(u64, u64, String), RequestsPage>,
    details: ResponseCache<DetailKey, Details>,
    observers:
        std::sync::Mutex<std::collections::HashMap<AccountKey, friends::RelationshipTracker>>,
}
impl Default for FriendsState {
    fn default() -> Self {
        Self {
            snapshots: ResponseCache::new(Duration::from_secs(30), 32),
            requests: ResponseCache::new(Duration::from_secs(30), 64),
            details: ResponseCache::new(Duration::from_secs(300), 128),
            observers: Default::default(),
        }
    }
}
impl FriendsState {
    pub fn clear(&self) -> usize {
        if let Ok(mut observers) = self.observers.lock() {
            observers.clear();
        }
        self.invalidate()
    }

    pub(crate) fn invalidate(&self) -> usize {
        self.snapshots.clear() + self.requests.clear() + self.details.clear()
    }

    pub(crate) fn reset_notifications(&self) {
        if let Ok(mut observers) = self.observers.lock() {
            observers.clear();
        }
        self.snapshots.clear();
    }

    pub(crate) fn record_action(&self, account: AccountKey, target: u64, action: FriendAction) {
        if let Ok(mut observers) = self.observers.lock() {
            if let Some(observer) = observers.get_mut(&account) {
                observer.record_action(target, action);
            }
        }
    }
}

async fn credential(state: &AppState, user_id: u64) -> Result<(String, u64), String> {
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        let account = runtime
            .accounts
            .find_by_id(user_id)
            .ok_or("This account no longer exists")?;
        if !runtime.unlocked || account.cookie_expired {
            return Err(
                "Unlock this account and check its login from Accounts before continuing".into(),
            );
        }
        let revision = *runtime.credential_revisions.get(&user_id).unwrap_or(&0);
        let cookie = crate::account_cookie(&runtime, user_id)
            .map_err(|_| "Account credential unavailable")?;
        Ok((cookie, revision))
    })
    .await
    .map_err(|_| "Account credential task failed")?
}

fn current_revision(runtime: &crate::state::RuntimeState, viewer: u64, revision: u64) -> bool {
    runtime.unlocked
        && runtime
            .accounts
            .find_by_id(viewer)
            .is_some_and(|account| !account.cookie_expired)
        && *runtime.credential_revisions.get(&viewer).unwrap_or(&0) == revision
}

pub(crate) fn current(state: &AppState, viewer: u64, revision: u64) -> bool {
    !state
        .is_shutting_down
        .load(std::sync::atomic::Ordering::Acquire)
        && state
            .runtime
            .lock()
            .ok()
            .is_some_and(|runtime| current_revision(&runtime, viewer, revision))
}

async fn cached<K, V, F, Fut>(
    cache: &ResponseCache<K, V>,
    key: K,
    refresh: bool,
    state: &AppState,
    account: AccountKey,
    fetch: F,
) -> Result<V, String>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<V, ram_core::CoreError>>,
{
    let gate = cache.key_gate(&key);
    let _guard = gate.lock().await;
    if !current(state, account.0, account.1) {
        return Err("Account changed. Select it again to refresh.".into());
    }
    if !refresh {
        if let Some(value) = cache.get(&key) {
            return Ok(value);
        }
    }
    let generation = cache.generation();
    let value = tokio::time::timeout(Duration::from_secs(120), fetch())
        .await
        .map_err(|_| "Roblox took too long to respond. Cached data was kept; refresh to retry.")?
        .map_err(|error| friends::error_message(&error))?;
    if !current(state, account.0, account.1) {
        return Err("Account changed while loading. Refresh before continuing.".into());
    }
    cache.insert(generation, key, value.clone());
    Ok(value)
}

#[tauri::command]
pub async fn get_friends(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    refresh: bool,
) -> Result<FriendsSnapshot, String> {
    let (cookie, revision) = credential(&state, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    cached(
        &state.friends.snapshots,
        (user_id, revision),
        refresh,
        &state,
        (user_id, revision),
        || friends::fetch_snapshot(&client, &cookie, user_id),
    )
    .await
}

#[tauri::command]
pub async fn get_friend_requests(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    cursor: Option<String>,
    refresh: bool,
) -> Result<RequestsPage, String> {
    if cursor.as_ref().is_some_and(|cursor| cursor.len() > 4096) {
        return Err("Invalid request cursor".into());
    }
    let (cookie, revision) = credential(&state, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    cached(
        &state.friends.requests,
        (user_id, revision, cursor.clone().unwrap_or_default()),
        refresh,
        &state,
        (user_id, revision),
        || friends::fetch_requests(&client, &cookie, cursor.as_deref()),
    )
    .await
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DetailSection {
    Details,
    Mutual,
    Groups,
}

#[tauri::command]
pub async fn get_friend_details(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    target_id: u64,
    section: DetailSection,
) -> Result<Details, String> {
    validate_target(user_id, target_id)?;
    let (cookie, revision) = credential(&state, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let name = match section {
        DetailSection::Details => "details",
        DetailSection::Mutual => "mutual",
        DetailSection::Groups => "groups",
    };
    cached(
        &state.friends.details,
        (user_id, revision, target_id, name.into()),
        false,
        &state,
        (user_id, revision),
        || async {
            let mut result = Details {
                profile: None,
                mutual: Vec::new(),
                groups: Vec::new(),
            };
            match section {
                DetailSection::Details => {
                    result.profile =
                        Some(friends::fetch_profile(&client, &cookie, target_id).await?)
                }
                DetailSection::Groups => {
                    result.groups = friends::fetch_groups(&client, &cookie, target_id).await?
                }
                DetailSection::Mutual => {
                    let own =
                        if let Some(snapshot) = state.friends.snapshots.get(&(user_id, revision)) {
                            snapshot
                                .friends
                                .into_iter()
                                .map(|friend| friend.user)
                                .collect()
                        } else {
                            friends::fetch_users(&client, &cookie, user_id).await?
                        };
                    let theirs = friends::fetch_users(&client, &cookie, target_id).await?;
                    let ids: std::collections::HashSet<_> =
                        theirs.into_iter().map(|user| user.user_id).collect();
                    result.mutual = own
                        .into_iter()
                        .filter(|user| ids.contains(&user.user_id))
                        .collect();
                }
            }
            Ok(result)
        },
    )
    .await
}

fn validate_target(viewer: u64, target: u64) -> Result<(), String> {
    if viewer == 0 || target == 0 || viewer == target {
        return Err("Choose a different, valid Roblox user".into());
    }
    Ok(())
}

async fn notification_sweep(
    app: &tauri::AppHandle,
    failures: &mut std::collections::HashSet<AccountKey>,
) -> Result<(), String> {
    let state = app.state::<AppState>().inner().clone();
    let (watch_friends, watch_requests) = crate::discord::friendship_watch(&state).await?;
    let accounts: std::collections::HashSet<_> = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Friendship account state unavailable")?;
        runtime
            .accounts
            .accounts
            .iter()
            .filter(|account| runtime.unlocked && !account.cookie_expired)
            .map(|account| {
                (
                    account.user_id,
                    *runtime
                        .credential_revisions
                        .get(&account.user_id)
                        .unwrap_or(&0),
                )
            })
            .collect()
    };
    failures.retain(|key| accounts.contains(key));
    {
        let mut observers = state
            .friends
            .observers
            .lock()
            .map_err(|_| "Friendship notification state unavailable")?;
        observers.retain(|key, _| accounts.contains(key) && (watch_friends || watch_requests));
        for observer in observers.values_mut() {
            observer.retain_sources(watch_friends, watch_requests);
        }
    }
    if !watch_friends && !watch_requests {
        failures.clear();
        return Ok(());
    }
    let client = RobloxClient::new().map_err(|_| "Friendship notification client unavailable")?;
    let mut accounts: Vec<_> = accounts.into_iter().collect();
    accounts.sort_unstable();
    for (user_id, revision) in accounts {
        if state
            .is_shutting_down
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Ok(());
        }
        let generation = state.friends.snapshots.generation();
        let result = tokio::time::timeout(Duration::from_secs(60), async {
            let (cookie, observed_revision) = credential(&state, user_id).await?;
            if observed_revision != revision {
                return Err("Account changed before friendship check".into());
            }
            let friends = if watch_friends {
                Some(
                    friends::fetch_users(&client, &cookie, user_id)
                        .await
                        .map_err(|error| friends::error_message(&error))?,
                )
            } else {
                None
            };
            let requests = if watch_requests {
                Some(
                    friends::fetch_request_users(&client, &cookie)
                        .await
                        .map_err(|error| friends::error_message(&error))?,
                )
            } else {
                None
            };
            Ok::<_, String>((friends, requests))
        })
        .await
        .map_err(|_| "Roblox took too long to check friendships".to_string())
        .and_then(|result| result);
        if !current(&state, user_id, revision) {
            continue;
        }
        let (friends, requests) = match result {
            Ok(value) => {
                failures.remove(&(user_id, revision));
                value
            }
            Err(error) => {
                if failures.insert((user_id, revision)) {
                    let _ = app.emit("background-notice", format!("Friendship notifications could not check account {user_id}. {error} Previous data was kept; RM will check again in the background."));
                }
                continue;
            }
        };
        // A social action can change either list while the network request is in flight.
        let Ok(_actions) = state.joins.queue.try_lock() else {
            continue;
        };
        if generation != state.friends.snapshots.generation() || !current(&state, user_id, revision)
        {
            continue;
        }
        let changes = {
            let mut observers = state
                .friends
                .observers
                .lock()
                .map_err(|_| "Friendship notification state unavailable")?;
            observers
                .entry((user_id, revision))
                .or_default()
                .observe(friends.as_deref(), requests.as_deref())
        };
        for change in changes {
            let (event, target) = match change {
                friends::RelationshipChange::RequestReceived(id) => {
                    (crate::discord::Event::FriendRequestReceived, id)
                }
                friends::RelationshipChange::Added(id) => (crate::discord::Event::FriendAdded, id),
                friends::RelationshipChange::Removed(id) => {
                    (crate::discord::Event::FriendRemoved, id)
                }
            };
            crate::discord::friendship(app, event, user_id, target, true);
        }
    }
    Ok(())
}

/// App-owned monitoring continues without loading the Friends workspace.
pub async fn run_notifications(app: tauri::AppHandle) {
    let mut failures = std::collections::HashSet::new();
    let mut preferences_failed = false;
    loop {
        match notification_sweep(&app, &mut failures).await {
            Ok(()) => preferences_failed = false,
            Err(error) if !preferences_failed => {
                preferences_failed = true;
                let _ = app.emit("background-notice", error);
            }
            Err(_) => {}
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

#[tauri::command]
pub async fn friend_action(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    user_id: u64,
    target_id: u64,
    action: FriendAction,
) -> Result<(), String> {
    validate_target(user_id, target_id)?;
    let _guard = state.joins.queue.lock().await;
    let (cookie, revision) = credential(&state, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    state.friends.invalidate();
    let result = friends::act(&client, &cookie, target_id, action).await;
    state.friends.invalidate();
    result.map_err(|error| {
        format!(
            "{} Refresh before retrying; the action may have completed.",
            friends::error_message(&error)
        )
    })?;
    if !current(&state, user_id, revision) {
        return Err(
            "The action may have completed, but this account changed. Refresh before retrying."
                .into(),
        );
    }
    if let Ok(mut runtime) = state.runtime.lock() {
        if let Some(account) = runtime.accounts.find_by_id_mut(user_id) {
            account.friends_cache.clear();
        }
    }
    state
        .friends
        .record_action((user_id, revision), target_id, action);
    let event = match action {
        FriendAction::Accept => crate::discord::Event::FriendAdded,
        FriendAction::Decline => crate::discord::Event::FriendRequestDeclined,
        FriendAction::Remove => crate::discord::Event::FriendRemoved,
        FriendAction::Block => crate::discord::Event::UserBlocked,
    };
    crate::discord::friendship(&app, event, user_id, target_id, false);
    Ok(())
}

fn join_destination(
    presence: &ram_core::models::Presence,
    expected: Option<&str>,
) -> Result<(u64, String), String> {
    let (place, job) = ram_core::session_history::server(presence).ok_or("This friend's server is unavailable to the selected account. They may have left, hidden their session, or restricted joining.")?;
    if let Some(expected) = expected {
        if ram_core::instances::normalize_job_id(Some(expected)).as_ref() != Some(&job) {
            return Err("This friend's server changed. Refresh and choose Join again.".into());
        }
    }
    Ok((place, job))
}

#[tauri::command]
pub async fn join_friend_game(
    app: tauri::AppHandle,
    user_id: u64,
    target_id: u64,
    expected_job_id: Option<String>,
    reuse_confirmed: bool,
    require_new_instance: bool,
) -> Result<(), String> {
    validate_target(user_id, target_id)?;
    let state = app.state::<AppState>().inner().clone();
    {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        if require_new_instance && !runtime.config.multi_instance_enabled {
            return Err(
                "Enable Multi-instance in Settings before requesting a new instance".into(),
            );
        }
        if !runtime.config.multi_instance_enabled && !reuse_confirmed {
            return Err("Confirm Roblox's existing-client behavior before launching with Multi-instance disabled".into());
        }
    }
    let _guard = state.joins.queue.lock().await;
    let (cookie, revision) = credential(&state, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let presence = ram_core::api::fetch_presences(&client, &cookie, &[target_id])
        .await
        .map_err(|error| friends::error_message(&error))?
        .into_iter()
        .find(|(id, _)| *id == target_id)
        .map(|(_, presence)| presence)
        .ok_or("Roblox did not expose this friend's session. Refresh or check join permissions.")?;
    let (place_id, job_id) = join_destination(&presence, expected_job_id.as_deref())?;
    drop(cookie);
    if !current(&state, user_id, revision) {
        return Err("Account changed. Refresh before joining.".into());
    }
    let request = crate::launcher::LaunchRequest {
        user_id,
        place_id,
        job_id: Some(job_id),
        data: None,
        link_code: None,
        access_code: None,
    };
    if require_new_instance || !reuse_confirmed {
        crate::launcher::launch_new_instance(&app, request, reuse_confirmed).await
    } else {
        crate::launcher::launch(&app, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_invalidation_preserves_notification_baselines_but_reset_is_silent() {
        let state = FriendsState::default();
        let users = |ids: &[u64]| {
            ids.iter()
                .map(|id| FriendUser {
                    user_id: *id,
                    ..Default::default()
                })
                .collect::<Vec<_>>()
        };
        state
            .observers
            .lock()
            .unwrap()
            .entry((1, 3))
            .or_default()
            .observe(Some(&users(&[10])), None);
        state.invalidate();
        assert_eq!(
            state
                .observers
                .lock()
                .unwrap()
                .get_mut(&(1, 3))
                .unwrap()
                .observe(Some(&users(&[10, 20])), None),
            [friends::RelationshipChange::Added(20)]
        );
        let old_generation = state.snapshots.generation();
        state.reset_notifications();
        assert_ne!(state.snapshots.generation(), old_generation);
        assert!(state
            .observers
            .lock()
            .unwrap()
            .entry((1, 3))
            .or_default()
            .observe(Some(&users(&[10, 20, 30])), None)
            .is_empty());
    }
    #[test]
    fn cached_results_are_isolated_by_account_and_credential_revision() {
        let state = FriendsState::default();
        let snapshot = FriendsSnapshot {
            friends: Vec::new(),
            refreshed_at: None,
            warnings: Vec::new(),
        };
        let generation = state.snapshots.generation();
        state.snapshots.insert(generation, (1, 3), snapshot.clone());
        assert!(state.snapshots.get(&(1, 3)).is_some());
        assert!(state.snapshots.get(&(2, 3)).is_none());
        assert!(state.snapshots.get(&(1, 4)).is_none());
        assert_eq!(state.clear(), 1);
        state.snapshots.insert(generation, (1, 3), snapshot);
        assert!(state.snapshots.get(&(1, 3)).is_none());
    }

    #[test]
    fn async_results_require_the_same_unlocked_current_credential() {
        let mut store = ram_core::models::AccountStore::default();
        store.accounts.push(ram_core::models::Account::new(
            1,
            "Synthetic".into(),
            "Synthetic".into(),
        ));
        let mut runtime = crate::state::RuntimeState {
            accounts: store,
            config: ram_core::models::AppConfig::default(),
            config_path: std::path::PathBuf::new(),
            session: None,
            unlocked: true,
            legacy_store: false,
            is_first_install: false,
            credential_revisions: [(1, 3)].into_iter().collect(),
        };
        assert!(current_revision(&runtime, 1, 3));
        assert!(!current_revision(&runtime, 1, 2));
        assert!(!current_revision(&runtime, 2, 3));
        runtime.unlocked = false;
        assert!(!current_revision(&runtime, 1, 3));
        runtime.unlocked = true;
        runtime.accounts.accounts[0].cookie_expired = true;
        assert!(!current_revision(&runtime, 1, 3));
    }

    #[test]
    fn joins_recheck_a_real_playing_session_and_exact_server_when_requested() {
        let mut presence = ram_core::models::Presence {
            user_presence_type: 2,
            place_id: Some(10),
            game_id: Some("12345678-1234-4234-8234-123456789abc".into()),
            ..Default::default()
        };
        assert!(join_destination(&presence, None).is_ok());
        assert!(join_destination(&presence, presence.game_id.as_deref()).is_ok());
        assert!(join_destination(&presence, Some("87654321-1234-4234-8234-123456789abc")).is_err());
        presence.user_presence_type = 3;
        assert!(join_destination(&presence, None).is_err());
        presence.user_presence_type = 2;
        presence.game_id = None;
        assert!(join_destination(&presence, None).is_err());
    }

    #[test]
    fn mutation_targets_require_two_distinct_nonzero_users() {
        assert!(validate_target(1, 2).is_ok());
        for (viewer, target) in [(0, 2), (1, 0), (1, 1)] {
            assert!(validate_target(viewer, target).is_err());
        }
    }
}
