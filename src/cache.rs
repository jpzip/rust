use async_trait::async_trait;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use crate::error::Error;
use crate::types::ZipcodeDict;

/// User-supplied L2 persistent cache. Implementations decide their own TTL,
/// eviction, and backend (file, KV, Redis, IndexedDB, …).
///
/// Keys are full URLs (e.g. `https://jpzip.nadai.dev/p/231.json`). Values
/// are the raw JSON bytes returned by the CDN.
#[async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error>;
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), Error>;
    async fn delete(&self, key: &str) -> Result<(), Error>;
    async fn clear(&self) -> Result<(), Error>;
}

/// L1 in-memory LRU keyed by prefix URL. Always on inside the client.
pub(crate) struct MemoryLru {
    inner: Mutex<LruCache<String, ZipcodeDict>>,
}

impl MemoryLru {
    pub(crate) fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).unwrap();
        Self {
            inner: Mutex::new(LruCache::new(cap)),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<ZipcodeDict> {
        let mut g = self.inner.lock().unwrap();
        g.get(key).cloned()
    }

    pub(crate) fn set(&self, key: String, value: ZipcodeDict) {
        let mut g = self.inner.lock().unwrap();
        g.put(key, value);
    }

    pub(crate) fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.clear();
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        let g = self.inner.lock().unwrap();
        g.len()
    }
}
