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
use tauri::Manager;

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
}
impl Default for FriendsState {
    fn default() -> Self {
        Self {
            snapshots: ResponseCache::new(Duration::from_secs(30), 32),
            requests: ResponseCache::new(Duration::from_secs(30), 64),
            details: ResponseCache::new(Duration::from_secs(300), 128),
        }
    }
}
impl FriendsState {
    pub fn clear(&self) -> usize {
        self.snapshots.clear() + self.requests.clear() + self.details.clear()
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

fn current(state: &AppState, viewer: u64, revision: u64) -> bool {
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

#[tauri::command]
pub async fn friend_action(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    target_id: u64,
    action: FriendAction,
) -> Result<(), String> {
    validate_target(user_id, target_id)?;
    let _guard = state.joins.queue.lock().await;
    let (cookie, revision) = credential(&state, user_id).await?;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    state.friends.clear();
    let result = friends::act(&client, &cookie, target_id, action).await;
    state.friends.clear();
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
