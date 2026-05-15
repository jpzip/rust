use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::future::try_join_all;
use reqwest::Client as HttpClient;
use tokio::sync::Mutex as AsyncMutex;

use crate::cache::{Cache, MemoryLru};
use crate::error::Error;
use crate::http::get_with_retry;
use crate::types::{Meta, ZipcodeDict, ZipcodeEntry, DEFAULT_BASE_URL, SPEC_VERSION};

/// Callback invoked once when the CDN's `spec_version` does not match
/// [`SPEC_VERSION`].
pub type SpecMismatchCallback = Arc<dyn Fn(&str, &str) + Send + Sync>;

fn is_valid_prefix(prefix: &str) -> bool {
    !prefix.is_empty()
        && prefix.len() <= 3
        && prefix.chars().all(|c| c.is_ascii_digit())
}

/// Returns true iff `s` matches `^\d{7}$`.
pub fn is_valid_zipcode(s: &str) -> bool {
    s.len() == 7 && s.chars().all(|c| c.is_ascii_digit())
}

/// Builder for [`JpzipClient`].
pub struct JpzipClientBuilder {
    base_url: String,
    http: Option<HttpClient>,
    memory_cache_size: usize,
    cache: Option<Arc<dyn Cache>>,
    on_spec_mismatch: Option<SpecMismatchCallback>,
}

impl Default for JpzipClientBuilder {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            http: None,
            memory_cache_size: 100,
            cache: None,
            on_spec_mismatch: None,
        }
    }
}

impl JpzipClientBuilder {
    /// Override the CDN origin (without trailing slash).
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        let mut u = url.into();
        while u.ends_with('/') {
            u.pop();
        }
        self.base_url = u;
        self
    }

    /// Swap the underlying `reqwest::Client` (useful for custom timeouts /
    /// proxies / tests).
    pub fn http_client(mut self, http: HttpClient) -> Self {
        self.http = Some(http);
        self
    }

    /// L1 capacity in prefix entries. Default 100.
    pub fn memory_cache_size(mut self, n: usize) -> Self {
        self.memory_cache_size = n;
        self
    }

    /// Enable an L2 persistent cache.
    pub fn cache(mut self, cache: Arc<dyn Cache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Hook invoked once if `/meta.json`'s `spec_version` differs from
    /// [`SPEC_VERSION`].
    pub fn on_spec_mismatch<F>(mut self, f: F) -> Self
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        self.on_spec_mismatch = Some(Arc::new(f));
        self
    }

    /// Build the client.
    pub fn build(self) -> JpzipClient {
        let http = self.http.unwrap_or_else(|| {
            HttpClient::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client build")
        });
        JpzipClient {
            inner: Arc::new(ClientInner {
                base_url: self.base_url,
                http,
                memory: MemoryLru::new(self.memory_cache_size),
                cache: self.cache,
                on_spec_mismatch: self.on_spec_mismatch,
                meta: AsyncMutex::new(MetaState::default()),
            }),
        }
    }
}

#[derive(Default)]
struct MetaState {
    cached: Option<Meta>,
    resolved: bool,
    known_version: Option<String>,
}

struct ClientInner {
    base_url: String,
    http: HttpClient,
    memory: MemoryLru,
    cache: Option<Arc<dyn Cache>>,
    on_spec_mismatch: Option<SpecMismatchCallback>,
    meta: AsyncMutex<MetaState>,
}

/// The jpzip SDK entry point.
#[derive(Clone)]
pub struct JpzipClient {
    inner: Arc<ClientInner>,
}

impl Default for JpzipClient {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl JpzipClient {
    /// Construct a default client (default base URL, default HTTP client,
    /// L1 of 100 entries, no L2).
    pub fn new() -> Self {
        Self::default()
    }

    /// Start building a configured client.
    pub fn builder() -> JpzipClientBuilder {
        JpzipClientBuilder::default()
    }

    fn prefix_url(&self, prefix3: &str) -> String {
        format!("{}/p/{}.json", self.inner.base_url, prefix3)
    }

    fn group_url(&self, prefix1: &str) -> String {
        format!("{}/g/{}.json", self.inner.base_url, prefix1)
    }

    /// Returns the entry for `zipcode`, or `Ok(None)` if not found.
    /// Malformed input returns `Ok(None)` without contacting the network.
    pub async fn lookup(&self, zipcode: &str) -> Result<Option<ZipcodeEntry>, Error> {
        if !is_valid_zipcode(zipcode) {
            return Ok(None);
        }
        let prefix3 = &zipcode[..3];
        let dict = self.fetch_prefix_dict(prefix3).await?;
        match dict {
            Some(d) => Ok(d.get(zipcode).cloned()),
            None => Ok(None),
        }
    }

    /// Fetch all entries under a 1-, 2-, or 3-digit prefix. A 2-digit
    /// prefix fans out into 10 prefix-3 fetches in parallel.
    pub async fn lookup_group(&self, prefix: &str) -> Result<ZipcodeDict, Error> {
        if !is_valid_prefix(prefix) {
            return Err(Error::InvalidPrefix(prefix.to_string()));
        }
        match prefix.len() {
            3 => {
                let d = self.fetch_prefix_dict(prefix).await?;
                Ok(d.unwrap_or_default())
            }
            1 => {
                let url = self.group_url(prefix);
                let d = self.fetch_url(&url).await?;
                Ok(d.unwrap_or_default())
            }
            2 => {
                let futs: Vec<_> = (0..10u8)
                    .map(|i| {
                        let p3 = format!("{}{}", prefix, i);
                        async move { self.fetch_prefix_dict(&p3).await }
                    })
                    .collect();
                let results = try_join_all(futs).await?;
                let mut out = HashMap::new();
                for d in results.into_iter().flatten() {
                    out.extend(d);
                }
                Ok(out)
            }
            _ => Err(Error::InvalidPrefix(prefix.to_string())),
        }
    }

    /// Fetch the full dataset by fanning out across `/g/0..9.json` in
    /// parallel and merging the results. The CDN does not publish a single
    /// `/all.json`.
    pub async fn lookup_all(&self) -> Result<ZipcodeDict, Error> {
        let futs: Vec<_> = (0..10u8)
            .map(|i| {
                let url = self.group_url(&i.to_string());
                async move { self.fetch_url(&url).await }
            })
            .collect();
        let results = try_join_all(futs).await?;
        let mut out = HashMap::new();
        for d in results.into_iter().flatten() {
            out.extend(d);
        }
        Ok(out)
    }

    /// Return the cached `/meta.json`. The first call hits the network;
    /// subsequent calls reuse the result until [`refresh`](Self::refresh).
    pub async fn get_meta(&self) -> Result<Option<Meta>, Error> {
        {
            let g = self.inner.meta.lock().await;
            if g.resolved {
                return Ok(g.cached.clone());
            }
        }
        let url = format!("{}/meta.json", self.inner.base_url);
        let body = get_with_retry(&self.inner.http, &url).await?;
        let mut g = self.inner.meta.lock().await;
        let body = match body {
            None => {
                g.resolved = true;
                return Ok(None);
            }
            Some(b) => b,
        };
        let meta: Meta = serde_json::from_slice(&body)?;
        if meta.spec_version != SPEC_VERSION {
            if let Some(cb) = &self.inner.on_spec_mismatch {
                cb(SPEC_VERSION, &meta.spec_version);
            }
        }
        if let Some(known) = &g.known_version {
            if known != &meta.version {
                self.inner.memory.clear();
                if let Some(c) = &self.inner.cache {
                    c.clear().await?;
                }
            }
        }
        g.known_version = Some(meta.version.clone());
        g.cached = Some(meta.clone());
        g.resolved = true;
        Ok(Some(meta))
    }

    /// Pull the requested scope into L1 (and L2 when configured).
    /// `scope == "all"` downloads `/g/0..9.json`; otherwise it must be a
    /// 1-3 digit prefix.
    pub async fn preload(&self, scope: &str) -> Result<(), Error> {
        if scope == "all" {
            let dict = self.lookup_all().await?;
            let mut buckets: HashMap<String, ZipcodeDict> = HashMap::new();
            for (zip, entry) in dict {
                if zip.len() < 3 {
                    continue;
                }
                let p = zip[..3].to_string();
                buckets.entry(p).or_default().insert(zip, entry);
            }
            for (p, b) in buckets {
                let url = self.prefix_url(&p);
                self.inner.memory.set(url.clone(), b.clone());
                self.write_l2(&url, &b).await?;
            }
            return Ok(());
        }
        if !is_valid_prefix(scope) {
            return Err(Error::InvalidPrefix(scope.to_string()));
        }
        let _ = self.lookup_group(scope).await?;
        Ok(())
    }

    /// Wipe L1 (and L2 when configured) and forget cached meta.
    pub async fn refresh(&self) -> Result<(), Error> {
        self.inner.memory.clear();
        {
            let mut g = self.inner.meta.lock().await;
            g.cached = None;
            g.resolved = false;
            g.known_version = None;
        }
        if let Some(c) = &self.inner.cache {
            c.clear().await?;
        }
        Ok(())
    }

    /* ----------------------------- internals ----------------------------- */

    async fn fetch_prefix_dict(&self, prefix3: &str) -> Result<Option<ZipcodeDict>, Error> {
        let url = self.prefix_url(prefix3);
        if let Some(d) = self.inner.memory.get(&url) {
            return Ok(Some(d));
        }
        if let Some(d) = self.read_l2(&url).await? {
            self.inner.memory.set(url.clone(), d.clone());
            return Ok(Some(d));
        }
        let d = self.fetch_url(&url).await?;
        if let Some(ref dict) = d {
            self.inner.memory.set(url.clone(), dict.clone());
            self.write_l2(&url, dict).await?;
        }
        Ok(d)
    }

    async fn fetch_url(&self, url: &str) -> Result<Option<ZipcodeDict>, Error> {
        let body = get_with_retry(&self.inner.http, url).await?;
        match body {
            None => Ok(None),
            Some(bytes) => {
                let d: ZipcodeDict = serde_json::from_slice(&bytes)?;
                Ok(Some(d))
            }
        }
    }

    async fn read_l2(&self, url: &str) -> Result<Option<ZipcodeDict>, Error> {
        let Some(cache) = &self.inner.cache else {
            return Ok(None);
        };
        let Some(bytes) = cache.get(url).await? else {
            return Ok(None);
        };
        match serde_json::from_slice::<ZipcodeDict>(&bytes) {
            Ok(d) => Ok(Some(d)),
            Err(_) => {
                // Corrupt cache — drop it.
                let _ = cache.delete(url).await;
                Ok(None)
            }
        }
    }

    async fn write_l2(&self, url: &str, dict: &ZipcodeDict) -> Result<(), Error> {
        let Some(cache) = &self.inner.cache else {
            return Ok(());
        };
        let b = serde_json::to_vec(dict)?;
        cache.set(url, b).await
    }
}
