//! Public metadata caches shared by all workspaces. Credentials never enter these keys.
use crate::{api, assets_api, auth::RobloxClient, cache::ResponseCache, group_api, CoreError};
use std::{collections::HashMap, sync::OnceLock, time::Duration};

macro_rules! cache {
    ($name:ident, $value:ty, $ttl:expr, $capacity:expr) => {
        fn $name() -> &'static ResponseCache<u64, $value> {
            static CACHE: OnceLock<ResponseCache<u64, $value>> = OnceLock::new();
            CACHE.get_or_init(|| ResponseCache::new(Duration::from_secs($ttl), $capacity))
        }
    };
}
cache!(avatars, String, 300, 2048);
cache!(game_icons, String, 1800, 1024);
cache!(asset_thumbnails, Vec<u8>, 1800, 64);
cache!(groups, group_api::GroupInfo, 60, 256);
cache!(group_icons, Vec<u8>, 1800, 64);
cache!(announcements, group_api::LatestGroupAnnouncement, 30, 256);
cache!(universe_names, String, 1800, 1024);
cache!(place_universes, u64, 1800, 1024);
cache!(creation_dates, chrono::DateTime<chrono::Utc>, 86400, 2048);

pub fn invalidate_group(id: u64) {
    groups().invalidate(&id);
    announcements().invalidate(&id);
}

pub fn clear() -> usize {
    avatars().clear()
        + game_icons().clear()
        + asset_thumbnails().clear()
        + groups().clear()
        + group_icons().clear()
        + announcements().clear()
        + universe_names().clear()
        + place_universes().clear()
        + creation_dates().clear()
}

async fn batch<V: Clone, F, Fut>(
    cache: &ResponseCache<u64, V>,
    ids: &[u64],
    fetch: F,
    cacheable: impl Fn(&V) -> bool,
) -> Result<Vec<(u64, V)>, CoreError>
where
    F: FnOnce(Vec<u64>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<(u64, V)>, CoreError>>,
{
    let mut values = HashMap::new();
    let mut missing = Vec::new();
    for id in ids {
        if let Some(value) = cache.get(id) {
            values.insert(*id, value);
        } else if !missing.contains(id) {
            missing.push(*id);
        }
    }
    // acquire each missing key once in a stable order, so overlapping batches cannot deadlock
    missing.sort_unstable();
    let mut guards = Vec::with_capacity(missing.len());
    for id in &missing {
        guards.push(cache.key_gate(id).lock_owned().await);
    }
    missing.retain(|id| {
        if let Some(value) = cache.get(id) {
            values.insert(*id, value);
            false
        } else {
            true
        }
    });
    let generation = cache.generation();
    if !missing.is_empty() {
        for (id, value) in fetch(missing.clone()).await? {
            if !missing.contains(&id) {
                continue;
            }
            if cacheable(&value) {
                cache.insert(generation, id, value.clone());
            }
            values.insert(id, value);
        }
    }
    Ok(ids
        .iter()
        .filter_map(|id| values.get(id).cloned().map(|value| (*id, value)))
        .collect())
}

pub async fn fetch_avatars(
    client: &RobloxClient,
    ids: &[u64],
) -> Result<Vec<(u64, String)>, CoreError> {
    batch(
        avatars(),
        ids,
        |missing| async move {
            let mut values = Vec::new();
            for chunk in missing.chunks(100) {
                values.extend(api::fetch_avatars(client, chunk).await?);
            }
            Ok(values)
        },
        |url| !url.is_empty(),
    )
    .await
}

pub async fn fetch_game_icons(
    client: &RobloxClient,
    _cookie: &str,
    ids: &[u64],
) -> Result<Vec<(u64, String)>, CoreError> {
    batch(
        game_icons(),
        ids,
        |missing| async move { api::fetch_game_icons(client, "", &missing).await },
        |url| !url.is_empty(),
    )
    .await
}

pub async fn fetch_asset_thumbnails(
    client: &RobloxClient,
    ids: &[u64],
) -> Result<Vec<(u64, Vec<u8>)>, CoreError> {
    batch(
        asset_thumbnails(),
        ids,
        |missing| async move { assets_api::fetch_asset_thumbnails(client, &missing).await },
        |bytes| bytes.len() <= 1024 * 1024,
    )
    .await
}

pub async fn fetch_group(
    client: &RobloxClient,
    id: u64,
) -> Result<group_api::GroupInfo, CoreError> {
    groups()
        .get_or_fetch(id, || group_api::fetch_group(client, id))
        .await
}

pub async fn fetch_group_icon(
    client: &RobloxClient,
    id: u64,
) -> Result<Option<Vec<u8>>, CoreError> {
    let cache = group_icons();
    let _gate = cache.gate.lock().await;
    if let Some(bytes) = cache.get(&id) {
        return Ok(Some(bytes));
    }
    let generation = cache.generation();
    let result = group_api::fetch_group_icon(client, id).await?;
    if let Some(bytes) = result.as_ref().filter(|bytes| bytes.len() <= 1024 * 1024) {
        cache.insert(generation, id, bytes.clone());
    }
    Ok(result)
}

pub async fn fetch_latest_group_announcement(
    client: &RobloxClient,
    id: u64,
) -> Option<group_api::LatestGroupAnnouncement> {
    let cache = announcements();
    let _gate = cache.gate.lock().await;
    if let Some(value) = cache.get(&id) {
        return Some(value);
    }
    let generation = cache.generation();
    let value = group_api::fetch_latest_group_announcement(client, id).await?;
    cache.insert(generation, id, value.clone());
    Some(value)
}

pub async fn resolve_universe_name(client: &RobloxClient, id: u64) -> Result<String, CoreError> {
    universe_names()
        .get_or_fetch(id, || api::resolve_universe_name(client, id))
        .await
}

pub async fn resolve_place_universe(
    client: &RobloxClient,
    _cookie: &str,
    id: u64,
) -> Result<u64, CoreError> {
    place_universes()
        .get_or_fetch(id, || assets_api::resolve_place_universe(client, "", id))
        .await
}

pub async fn fetch_public_created_at(
    client: &RobloxClient,
    id: u64,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, CoreError> {
    let cache = creation_dates();
    let _gate = cache.gate.lock().await;
    if let Some(value) = cache.get(&id) {
        return Ok(Some(value));
    }
    let generation = cache.generation();
    let value = api::fetch_public_created_at(client, id).await?;
    if let Some(value) = value {
        cache.insert(generation, id, value);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn disjoint_batches_and_cache_hits_do_not_wait_for_slow_fetches() {
        let cache = ResponseCache::new(Duration::from_secs(60), 8);
        cache.insert(cache.generation(), 4, "cached");
        let entered = tokio::sync::Notify::new();
        let release = tokio::sync::Notify::new();
        let slow = batch(
            &cache,
            &[2, 1],
            |missing| {
                assert_eq!(missing, vec![1, 2]);
                async {
                    entered.notify_one();
                    release.notified().await;
                    Ok(vec![(1, "one"), (2, "two")])
                }
            },
            |_| true,
        );
        let fast = async {
            entered.notified().await;
            let result = tokio::time::timeout(Duration::from_secs(2), async {
                let hits = batch(
                    &cache,
                    &[4, 4],
                    |_| async { panic!("cached IDs must not fetch") },
                    |_| true,
                )
                .await
                .unwrap();
                assert_eq!(hits, vec![(4, "cached"), (4, "cached")]);
                batch(
                    &cache,
                    &[3],
                    |missing| async move {
                        assert_eq!(missing, vec![3]);
                        Ok(vec![(3, "three")])
                    },
                    |_| true,
                )
                .await
            })
            .await;
            release.notify_one();
            assert_eq!(result.unwrap().unwrap(), vec![(3, "three")]);
        };
        let (slow, _) = tokio::join!(slow, fast);
        assert_eq!(slow.unwrap(), vec![(2, "two"), (1, "one")]);
    }

    #[tokio::test]
    async fn overlapping_batches_share_fetches_even_with_reversed_and_duplicate_ids() {
        let cache = ResponseCache::new(Duration::from_secs(60), 8);
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let fetch = |missing| {
            assert_eq!(missing, vec![1, 2]);
            async {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tokio::task::yield_now().await;
                Ok(vec![(1, "one"), (2, "two")])
            }
        };
        let (a, b) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                batch(&cache, &[2, 1, 2], fetch, |_| true),
                batch(&cache, &[1, 2], fetch, |_| true),
            )
        })
        .await
        .unwrap();
        assert_eq!(a.unwrap(), vec![(2, "two"), (1, "one"), (2, "two")]);
        assert_eq!(b.unwrap(), vec![(1, "one"), (2, "two")]);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn batches_reuse_hits_and_retry_omitted_thumbnails() {
        let cache = ResponseCache::new(Duration::from_secs(60), 4);
        cache.insert(cache.generation(), 2, "two".to_string());
        let result = batch(
            &cache,
            &[2, 1, 3, 1],
            |missing| async move {
                assert_eq!(missing, vec![1, 3]);
                Ok(vec![
                    (1, "one".to_string()),
                    (99, "unrequested".to_string()),
                ])
            },
            |_| true,
        )
        .await
        .unwrap();
        assert_eq!(
            result.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![2, 1, 1]
        );
        assert_eq!(cache.get(&3), None);
        assert_eq!(cache.get(&99), None);
    }
    #[tokio::test]
    async fn overlapping_requests_share_work_and_errors_are_not_cached() {
        let cache = ResponseCache::new(Duration::from_secs(60), 4);
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let fetch = || async {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tokio::task::yield_now().await;
            Ok::<_, ()>("value")
        };
        let (a, b) = tokio::join!(cache.get_or_fetch(1, fetch), cache.get_or_fetch(1, fetch));
        assert_eq!(a, b);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(cache
            .get_or_fetch(2, || async { Err::<&str, _>(()) })
            .await
            .is_err());
        assert_eq!(cache.get(&2), None);
    }
}
