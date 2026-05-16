//! # jpzip
//!
//! Rust SDK for the [jpzip](https://jpzip.nadai.dev) Japanese postal-code
//! dataset. The SDK fetches normalized JSON from the CDN, keeps a per-prefix
//! in-memory LRU, and optionally backs that with a user-supplied persistent
//! cache.
//!
//! ```no_run
//! use jpzip::JpzipClient;
//!
//! # async fn run() -> Result<(), jpzip::Error> {
//! let client = JpzipClient::new();
//! if let Some(entry) = client.lookup("2310017").await? {
//!     println!("{} {}", entry.prefecture, entry.city);
//! }
//! # Ok(())
//! # }
//! ```

mod cache;
mod client;
mod error;
mod http;
mod types;

pub use cache::Cache;
pub use client::{is_valid_zipcode, JpzipClient, JpzipClientBuilder, SpecMismatchCallback};
pub use error::Error;
pub use types::{
    Endpoints, Meta, Town, ZipcodeDict, ZipcodeEntry, DEFAULT_BASE_URL, SPEC_VERSION,
};

use std::sync::OnceLock;

fn default_client() -> &'static JpzipClient {
    static DEFAULT: OnceLock<JpzipClient> = OnceLock::new();
    DEFAULT.get_or_init(JpzipClient::new)
}

/// Shortcut for [`JpzipClient::lookup`] on a lazily-initialized default client.
pub async fn lookup(zipcode: &str) -> Result<Option<ZipcodeEntry>, Error> {
    default_client().lookup(zipcode).await
}

/// Shortcut for [`JpzipClient::lookup_group`] on a lazily-initialized default client.
pub async fn lookup_group(prefix: &str) -> Result<ZipcodeDict, Error> {
    default_client().lookup_group(prefix).await
}

/// Shortcut for [`JpzipClient::lookup_all`] on a lazily-initialized default client.
pub async fn lookup_all() -> Result<ZipcodeDict, Error> {
    default_client().lookup_all().await
}

/// Shortcut for [`JpzipClient::preload`] on a lazily-initialized default client.
pub async fn preload(scope: &str) -> Result<(), Error> {
    default_client().preload(scope).await
}

/// Shortcut for [`JpzipClient::get_meta`] on a lazily-initialized default client.
pub async fn get_meta() -> Result<Option<Meta>, Error> {
    default_client().get_meta().await
}
