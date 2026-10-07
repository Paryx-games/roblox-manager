//! Bounded, memory-only response caching. Clearing also rejects in-flight writes.
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct ResponseCache<K, V> {
    entries: Mutex<Entries<K, V>>,
    pub gate: tokio::sync::Mutex<()>,
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

    pub async fn get_or_fetch<E, F, Fut>(&self, key: K, fetch: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<V, E>>,
    {
        let _gate = self.gate.lock().await;
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let generation = self.generation();
        let value = fetch().await?;
        self.insert(generation, key, value.clone());
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
