use std::time::Duration;

use reqwest::{Client as HttpClient, StatusCode};
use tokio::time::sleep;

use crate::error::Error;

/// Result of a raw GET: `Some(body_bytes)` on 2xx, `None` on 404.
pub(crate) async fn get_with_retry(
    http: &HttpClient,
    url: &str,
) -> Result<Option<Vec<u8>>, Error> {
    let mut last_err: Option<Error> = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            // Exponential backoff: 200ms * 2^attempt.
            let backoff_ms = 200u64.saturating_mul(1u64 << attempt);
            sleep(Duration::from_millis(backoff_ms)).await;
        }
        match http.get(url).header("Accept", "application/json").send().await {
            Ok(resp) => {
                let status = resp.status();
                if status == StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                if status.is_server_error() {
                    last_err = Some(Error::Status {
                        url: url.to_string(),
                        status: status.as_u16(),
                    });
                    continue;
                }
                if !status.is_success() {
                    return Err(Error::Status {
                        url: url.to_string(),
                        status: status.as_u16(),
                    });
                }
                match resp.bytes().await {
                    Ok(bytes) => return Ok(Some(bytes.to_vec())),
                    Err(e) => {
                        last_err = Some(Error::Http(e));
                        continue;
                    }
                }
            }
            Err(e) => {
                last_err = Some(Error::Http(e));
                continue;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| Error::Status {
        url: url.to_string(),
        status: 0,
    }))
}
