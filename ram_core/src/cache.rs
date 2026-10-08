//! Bounded, memory-only response caching. Clearing also rejects in-flight writes.
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

pub struct ResponseCache<K, V> {
    entries: Mutex<Entries<K, V>>,
    pub gate: tokio::sync::Mutex<()>,
    key_gates: Mutex<HashMap<K, Weak<tokio::sync::Mutex<()>>>>,
    ttl: Duration,
    capacity: usize,
}

struct Entries<K, V> {
    generation: u64,
    values: HashMap<K, (Instant, V)>,
}

impl<K: Eq + Hash + Clone, V: Clone> ResponseCache<K, V> {
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            entries: Mutex::new(Entries {
                generation: 0,
                values: HashMap::new(),
            }),
            gate: tokio::sync::Mutex::new(()),
            key_gates: Mutex::new(HashMap::new()),
            ttl,
            capacity,
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let mut entries = self.entries.lock().ok()?;
        entries
            .values
            .retain(|_, (time, _)| time.elapsed() < self.ttl);
        entries.values.get(key).map(|(_, value)| value.clone())
    }

    pub fn generation(&self) -> u64 {
        self.entries
            .lock()
            .map(|entries| entries.generation)
            .unwrap_or_default()
    }

    pub fn insert(&self, generation: u64, key: K, value: V) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.generation != generation || self.capacity == 0 {
            return;
        }
        entries
            .values
            .retain(|_, (time, _)| time.elapsed() < self.ttl);
        if entries.values.len() >= self.capacity && !entries.values.contains_key(&key) {
            if let Some(oldest) = entries
                .values
                .iter()
                .min_by_key(|(_, (time, _))| *time)
                .map(|(key, _)| key.clone())
            {
                entries.values.remove(&oldest);
            }
        }
        entries.values.insert(key, (Instant::now(), value));
    }

    /// Weak entries keep coordination alive only while requests are using it.
    pub fn key_gate(&self, key: &K) -> Arc<tokio::sync::Mutex<()>> {
        let mut gates = self
            .key_gates
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        gates.retain(|_, gate| gate.strong_count() > 0);
        if let Some(gate) = gates.get(key).and_then(Weak::upgrade) {
            return gate;
        }
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        gates.insert(key.clone(), Arc::downgrade(&gate));
        gate
    }

    pub async fn get_or_fetch<E, F, Fut>(&self, key: K, fetch: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<V, E>>,
    {
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let gate = self.key_gate(&key);
        let _gate = gate.lock().await;
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let generation = self.generation();
        let value = fetch().await?;
        self.insert(generation, key, value.clone());
        Ok(value)
    }

    /// Optional responses use the same per-key coordination without retaining omissions.
    pub async fn get_or_fetch_optional<E, F, Fut>(
        &self,
        key: K,
        fetch: F,
        cacheable: impl Fn(&V) -> bool,
    ) -> Result<Option<V>, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Option<V>, E>>,
    {
        if let Some(value) = self.get(&key) {
            return Ok(Some(value));
        }
        let gate = self.key_gate(&key);
        let _gate = gate.lock().await;
        if let Some(value) = self.get(&key) {
            return Ok(Some(value));
        }
        let generation = self.generation();
        let value = fetch().await?;
        if let Some(value) = value.as_ref().filter(|value| cacheable(value)) {
            self.insert(generation, key, value.clone());
        }
        Ok(value)
    }

    pub fn invalidate(&self, key: &K) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.values.remove(key);
            entries.generation = entries.generation.wrapping_add(1);
        }
    }

    pub fn clear(&self) -> usize {
        let Ok(mut entries) = self.entries.lock() else {
            return 0;
        };
        let count = entries.values.len();
        entries.values.clear();
        entries.generation = entries.generation.wrapping_add(1);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn optional_fetches_coordinate_per_key_and_recheck_after_waiting() {
        let cache = ResponseCache::new(Duration::from_secs(60), 4);
        let entered = tokio::sync::Notify::new();
        let release = tokio::sync::Notify::new();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let slow = cache.get_or_fetch_optional(
            1,
            || async {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                entered.notify_one();
                release.notified().await;
                Ok::<_, ()>(Some("one"))
            },
            |_| true,
        );
        let other = async {
            entered.notified().await;
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                cache.get_or_fetch_optional(2, || async { Ok::<_, ()>(Some("two")) }, |_| true),
            )
            .await;
            release.notify_one();
            assert_eq!(result.unwrap().unwrap(), Some("two"));
        };
        let same = cache.get_or_fetch_optional(
            1,
            || async {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok::<_, ()>(Some("unexpected duplicate"))
            },
            |_| true,
        );
        let (slow, _, same) = tokio::join!(slow, other, same);
        assert_eq!(slow.unwrap(), Some("one"));
        assert_eq!(same.unwrap(), Some("one"));
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn optional_cache_preserves_omissions_size_limits_errors_and_clear_generation() {
        let cache = ResponseCache::new(Duration::from_secs(60), 4);
        let small = vec![0_u8; 3];
        let large = vec![0_u8; 5];
        assert_eq!(
            cache
                .get_or_fetch_optional(
                    1,
                    || async { Ok::<_, ()>(None) },
                    |value: &Vec<u8>| value.len() <= 4
                )
                .await
                .unwrap(),
            None
        );
        assert_eq!(cache.get(&1), None);
        assert_eq!(
            cache
                .get_or_fetch_optional(
                    1,
                    || async { Ok::<_, ()>(Some(large.clone())) },
                    |value| value.len() <= 4
                )
                .await
                .unwrap(),
            Some(large)
        );
        assert_eq!(cache.get(&1), None);
        assert!(cache
            .get_or_fetch_optional(1, || async { Err::<Option<Vec<u8>>, _>(()) }, |_| true)
            .await
            .is_err());
        assert_eq!(cache.get(&1), None);
        let result = cache
            .get_or_fetch_optional(
                1,
                || async {
                    cache.clear();
                    Ok::<_, ()>(Some(small.clone()))
                },
                |_| true,
            )
            .await
            .unwrap();
        assert_eq!(result, Some(small));
        assert_eq!(cache.get(&1), None);
    }

    #[tokio::test]
    async fn unrelated_keys_do_not_wait_for_a_slow_fetch() {
        let cache = ResponseCache::new(Duration::from_secs(60), 4);
        let entered = tokio::sync::Notify::new();
        let release = tokio::sync::Notify::new();
        let slow = cache.get_or_fetch(1, || async {
            entered.notify_one();
            release.notified().await;
            Ok::<_, ()>("one")
        });
        let fast = async {
            entered.notified().await;
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                cache.get_or_fetch(2, || async { Ok::<_, ()>("two") }),
            )
            .await;
            release.notify_one();
            assert_eq!(result.unwrap().unwrap(), "two");
        };
        let (slow, _) = tokio::join!(slow, fast);
        assert_eq!(slow.unwrap(), "one");
    }

    #[test]
    fn authenticated_cache_keys_keep_accounts_and_revisions_separate() {
        let cache = ResponseCache::new(Duration::from_secs(60), 4);
        let generation = cache.generation();
        cache.insert(generation, (1, 0), "first account");
        assert_eq!(cache.get(&(2, 0)), None);
        assert_eq!(cache.get(&(1, 1)), None);
        cache.invalidate(&(1, 0));
        cache.insert(generation, (1, 0), "stale in-flight result");
        assert_eq!(cache.get(&(1, 0)), None);
    }

    #[test]
    fn expiry_capacity_and_clear_reject_old_results() {
        let cache = ResponseCache::new(Duration::from_secs(60), 2);
        let generation = cache.generation();
        cache.insert(generation, 1, "one");
        cache.insert(generation, 2, "two");
        cache.insert(generation, 3, "three");
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&3), Some("three"));
        cache.entries.lock().unwrap().values.get_mut(&3).unwrap().0 =
            Instant::now() - Duration::from_secs(60);
        assert_eq!(cache.get(&3), None);
        assert_eq!(cache.clear(), 1);
        cache.insert(generation, 1, "late response");
        assert_eq!(cache.get(&1), None);
    }
}
