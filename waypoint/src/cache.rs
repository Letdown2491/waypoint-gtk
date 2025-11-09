//! Simple TTL-based cache for expensive filesystem operations
//!
//! This module provides a thread-safe cache with time-to-live (TTL) support
//! to reduce redundant filesystem queries for snapshot sizes and disk space.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A cache entry with a value and expiration time
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    value: V,
    expires_at: Instant,
}

impl<V> CacheEntry<V> {
    /// Check if this entry has expired
    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

/// A thread-safe TTL cache
///
/// This cache automatically expires entries after their TTL has elapsed.
/// It's designed to be cloned cheaply (uses Arc internally) and shared
/// across threads.
#[derive(Debug, Clone)]
pub struct TtlCache<K, V> {
    store: Arc<Mutex<HashMap<K, CacheEntry<V>>>>,
    default_ttl: Duration,
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create a new cache with the specified default TTL
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Insert a value into the cache with the default TTL
    pub fn insert(&self, key: K, value: V) {
        self.insert_with_ttl(key, value, self.default_ttl);
    }

    /// Insert a value into the cache with a custom TTL
    pub fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) {
        let entry = CacheEntry {
            value,
            expires_at: Instant::now() + ttl,
        };

        if let Ok(mut store) = self.store.lock() {
            store.insert(key, entry);
        }
    }

    /// Get a value from the cache if it exists and hasn't expired
    pub fn get(&self, key: &K) -> Option<V> {
        if let Ok(mut store) = self.store.lock() {
            if let Some(entry) = store.get(key) {
                if entry.is_expired() {
                    // Remove expired entry
                    store.remove(key);
                    None
                } else {
                    Some(entry.value.clone())
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Remove a specific key from the cache
    #[allow(dead_code)]
    pub fn remove(&self, key: &K) {
        if let Ok(mut store) = self.store.lock() {
            store.remove(key);
        }
    }

    /// Clear all entries from the cache
    #[allow(dead_code)]
    pub fn clear(&self) {
        if let Ok(mut store) = self.store.lock() {
            store.clear();
        }
    }

    /// Remove all expired entries from the cache
    #[allow(dead_code)]
    pub fn cleanup_expired(&self) {
        if let Ok(mut store) = self.store.lock() {
            store.retain(|_, entry| !entry.is_expired());
        }
    }

    /// Get the number of entries in the cache (including expired ones)
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        if let Ok(store) = self.store.lock() {
            store.len()
        } else {
            0
        }
    }

    /// Check if the cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_cache_basic_operations() {
        let cache = TtlCache::new(Duration::from_secs(60));

        // Insert and retrieve
        cache.insert("key1".to_string(), 42);
        assert_eq!(cache.get(&"key1".to_string()), Some(42));

        // Non-existent key
        assert_eq!(cache.get(&"key2".to_string()), None);

        // Clear
        cache.clear();
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_cache_expiration() {
        let cache = TtlCache::new(Duration::from_millis(100));

        cache.insert("key".to_string(), "value".to_string());
        assert_eq!(cache.get(&"key".to_string()), Some("value".to_string()));

        // Wait for expiration
        thread::sleep(Duration::from_millis(150));

        // Should be expired now
        assert_eq!(cache.get(&"key".to_string()), None);
    }

    #[test]
    fn test_cache_custom_ttl() {
        let cache = TtlCache::new(Duration::from_secs(60));

        // Insert with short TTL
        cache.insert_with_ttl("short".to_string(), 1, Duration::from_millis(50));
        // Insert with longer TTL
        cache.insert_with_ttl("long".to_string(), 2, Duration::from_millis(200));

        // Both should be present initially
        assert_eq!(cache.get(&"short".to_string()), Some(1));
        assert_eq!(cache.get(&"long".to_string()), Some(2));

        // Wait for short TTL to expire
        thread::sleep(Duration::from_millis(100));

        // Short should be expired, long should still be valid
        assert_eq!(cache.get(&"short".to_string()), None);
        assert_eq!(cache.get(&"long".to_string()), Some(2));
    }

    #[test]
    fn test_cache_remove() {
        let cache = TtlCache::new(Duration::from_secs(60));

        cache.insert("key".to_string(), 42);
        assert_eq!(cache.get(&"key".to_string()), Some(42));

        cache.remove(&"key".to_string());
        assert_eq!(cache.get(&"key".to_string()), None);
    }

    #[test]
    fn test_cache_cleanup() {
        let cache = TtlCache::new(Duration::from_millis(50));

        cache.insert("key1".to_string(), 1);
        cache.insert("key2".to_string(), 2);

        assert_eq!(cache.len(), 2);

        // Wait for expiration
        thread::sleep(Duration::from_millis(100));

        // Cleanup should remove expired entries
        cache.cleanup_expired();
        assert_eq!(cache.len(), 0);
    }
}
