use thiserror::Error;

/// Errors returned by the jpzip SDK.
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP transport error (network failure, exhausted retries on 5xx, etc).
    #[error("jpzip: http error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON parse error.
    #[error("jpzip: parse error: {0}")]
    Parse(#[from] serde_json::Error),

    /// The prefix passed to `lookup_group` / `preload` was not 1-3 digits.
    #[error("jpzip: prefix must be 1-3 digits: {0:?}")]
    InvalidPrefix(String),

    /// Error coming out of a user-supplied L2 [`Cache`](crate::cache::Cache).
    #[error("jpzip: cache error: {0}")]
    Cache(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Non-2xx HTTP status (other than 404, which is mapped to `None`).
    #[error("jpzip: {url} returned HTTP {status}")]
    Status { url: String, status: u16 },
}
