# jpzip

[![crates.io](https://img.shields.io/crates/v/jpzip.svg)](https://crates.io/crates/jpzip)
[![docs.rs](https://docs.rs/jpzip/badge.svg)](https://docs.rs/jpzip)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Publish](https://github.com/jpzip/rust/actions/workflows/publish.yml/badge.svg)](https://github.com/jpzip/rust/actions/workflows/publish.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.75-blue.svg)](#requirements)

> Rust SDK for **jpzip** — a free, unlimited Japanese postal code (郵便番号) API.
> 日本の全郵便番号 120,677 件を CDN 配信 JSON から引く Rust SDK。

**English** | [日本語](./README.ja.md)

`jpzip` looks up Japanese postal codes (郵便番号) from `jpzip.nadai.dev`,
a CDN-hosted dataset built from Japan Post's `KEN_ALL.csv` and `KEN_ALL_ROME.csv`
normalized to JSON. No registration, no rate limits, no API key.

- 🇯🇵 **Complete dataset** — 120,677 entries with kanji, kana, romaji, and government codes (JIS X 0401 / 総務省地方公共団体コード)
- ⚡️ **Async + cached** — `tokio` + `reqwest`, L1 LRU + optional L2 persistent cache; `preload` to serve lookups without per-request network round-trips
- 🛡️ **Resilient** — 3-attempt retry with exponential backoff on 5xx / network failures
- 🪶 **Lightweight** — pure-Rust TLS (`rustls`), no `openssl-sys`, no C toolchain at build time
- 🆓 **Free forever** — backed by Cloudflare Pages' free tier (no billing axis exists)
- 🔌 **Drop-in** — same API surface across [every jpzip SDK](#other-languages)

## Requirements

Rust 1.75+ (edition 2021), tokio runtime.

## Install

```bash
cargo add jpzip
```

Or in `Cargo.toml`:

```toml
[dependencies]
jpzip = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
#[tokio::main]
async fn main() -> Result<(), jpzip::Error> {
    let entry = jpzip::lookup("2310017").await?;
    match entry {
        Some(e) => {
            println!("{} {} {}", e.prefecture, e.city, e.towns[0].town);
            // Output: 神奈川県 横浜市中区 港町
        }
        None => println!("not found"),
    }
    Ok(())
}
```

Romaji and government codes are included on the same entry:

```rust
let e = jpzip::lookup("2310017").await?.unwrap();
println!("{} {} {}", e.prefecture_roma, e.city_roma, e.towns[0].roma);
// Output: Kanagawa Ken Yokohama Shi Naka Ku Minatocho

println!("{} {}", e.prefecture_code, e.city_code);
// Output: 14 14104
```

## Use Cases

### Zipcode lookup HTTP endpoint (axum)

```rust
use axum::{extract::Path, http::StatusCode, response::Json, routing::get, Router};
use jpzip::ZipcodeEntry;

async fn zipcode(Path(code): Path<String>)
    -> Result<Json<ZipcodeEntry>, StatusCode>
{
    match jpzip::lookup(&code).await {
        Ok(Some(entry)) => Ok(Json(entry)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/api/zipcode/:code", get(zipcode));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### Zipcode lookup HTTP endpoint (actix-web)

```rust
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};

#[get("/api/zipcode/{code}")]
async fn zipcode(code: web::Path<String>) -> impl Responder {
    match jpzip::lookup(&code).await {
        Ok(Some(entry)) => HttpResponse::Ok().json(entry),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(zipcode))
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}
```

### Batch validation

```rust
let all = jpzip::lookup_all().await?; // entire dataset in memory (~37 MiB JSON)
for zip in csv_zipcodes {
    if !all.contains_key(&zip) {
        eprintln!("invalid zipcode: {zip}");
    }
}
```

### Serve lookups from cache (BYO L2 backend)

The dataset is partitioned into 948 three-digit prefix buckets. The default
L1 (100 entries) keeps the hottest buckets; to cache the whole dataset, pair
`preload("all")` with an L2 cache or raise `memory_cache_size` above 948.

```rust
use std::sync::Arc;
use jpzip::{Cache, JpzipClient};

# async fn run(my_file_cache: Arc<dyn Cache>) -> Result<(), jpzip::Error> {
let client = JpzipClient::builder()
    .memory_cache_size(1024)
    .cache(my_file_cache) // any Cache trait implementation
    .build();

client.preload("all").await?;
// Subsequent lookups are served from L1/L2 without hitting the network.
let entry = client.lookup("2310017").await?;
# Ok(())
# }
```

## API Reference

Full docs on [docs.rs/jpzip](https://docs.rs/jpzip).

### Functions (crate-level, share a default `JpzipClient`)

| Function | Description |
|---|---|
| `lookup(zipcode)` | Look up a single 7-digit zipcode. Returns `Ok(None)` if not found or malformed (no network call for malformed input). |
| `lookup_group(prefix)` | Look up by 1-, 2-, or 3-digit prefix. 1-digit fetches `/g/{d}.json`; 3-digit fetches `/p/{ddd}.json`; 2-digit fans out into 10 parallel 3-digit fetches and merges. |
| `lookup_all()` | Fetch entire dataset (120k entries, ~37 MiB) in parallel across `/g/0..9.json`. |
| `get_meta()` | Dataset version, generated-at, per-prefecture counts, spec version. Cached until `refresh`. |
| `preload(scope)` | Warm L1 (and L2 when configured) for `"all"` or a specific prefix. |
| `is_valid_zipcode(s)` | Pure syntax check (`^\d{7}$`) — no network. |

All async; all return `Result<_, jpzip::Error>`.

### `JpzipClient` (advanced)

`JpzipClient::builder()` returns a configurable instance; required for L2 caching, custom HTTP client, alternate base URL, or multiple isolated caches:

```rust
use std::time::Duration;
use jpzip::JpzipClient;

let client = JpzipClient::builder()
    .base_url("https://jpzip.nadai.dev")
    .http_client(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
    )
    .memory_cache_size(200)         // L1 capacity in prefix buckets, default 100
    .cache(my_cache)                // optional L2 (Arc<dyn Cache>)
    .on_spec_mismatch(|expected, received| {
        eprintln!("jpzip spec mismatch: SDK={expected} server={received}");
    })
    .build();
```

`JpzipClient` exposes `lookup` / `lookup_group` / `lookup_all` / `get_meta` / `preload` plus:

| Method | Description |
|---|---|
| `client.refresh()` | Wipe L1 (and L2 when configured) and forget the cached meta. |

When `get_meta` observes that `/meta.json`'s `version` has changed since the last successful fetch, L1 and L2 are cleared automatically — call `get_meta` periodically to pick up dataset rollovers.

### Errors

`jpzip::Error` is a single `thiserror`-derived enum:

| Variant | When |
|---|---|
| `Error::Http(reqwest::Error)` | Transport-level failure after retries are exhausted. |
| `Error::Status { url, status }` | Non-2xx HTTP (404 is mapped to `Ok(None)` instead). |
| `Error::Parse(serde_json::Error)` | JSON shape did not match the dataset schema. |
| `Error::InvalidPrefix(String)` | `lookup_group` / `preload` received a prefix that is not 1-3 digits. |
| `Error::Cache(Box<dyn Error + Send + Sync>)` | Bubbled up from your `Cache` implementation. |

Transient network failures and 5xx responses are retried up to 3 attempts (initial + 2 retries) with exponential backoff sleeps of 400 ms and 800 ms. 4xx responses other than 404 are returned immediately.

### `Cache` trait

Bring your own L2 backend (file, sled, Redis, KV, etc.):

```rust
use async_trait::async_trait;
use jpzip::{Cache, Error};

#[async_trait]
pub trait CacheLike: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error>;
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), Error>;
    async fn delete(&self, key: &str) -> Result<(), Error>;
    async fn clear(&self) -> Result<(), Error>;
}
```

Pass `Arc<dyn Cache>` to `JpzipClient::builder().cache(...)`. Keys are the full prefix-bucket URLs (e.g. `https://jpzip.nadai.dev/p/231.json`); values are raw JSON bytes.

## Why jpzip?

| | **jpzip** | [jpostcode_rs][jpostcode_rs] | [kenall-rs][kenall_rs] | [zipcloud API][zipcloud] |
|---|---|---|---|---|
| Romaji (`Yokohama Shi`) | ✅ | ❌ | ⚠️ via paid plan | ❌ |
| Government codes (JIS / 総務省) | ✅ | ⚠️ JIS only | ✅ | ❌ |
| No manual CSV download | ✅ | ✅ Embedded | ✅ | ✅ |
| Monthly updates | ✅ Auto | ❌ Bumped on crate release | ✅ | ✅ |
| Offline after preload | ✅ | ✅ (always) | ❌ | ❌ |
| No API key | ✅ | ✅ | ❌ Required | ✅ |
| Rate-limit-free | ✅ | ✅ | ⚠️ Plan-gated | ⚠️ Discouraged |
| Async (`tokio`) | ✅ | ❌ Sync | ✅ | n/a |
| L1 + pluggable L2 cache | ✅ | n/a (in-binary) | ❌ | ❌ |

[jpostcode_rs]: https://github.com/nwiizo/jpostcode_rs
[kenall_rs]: https://github.com/chansuke/kenall-rs
[zipcloud]: http://zipcloud.ibsnet.co.jp/doc/api

`jpostcode_rs` is the right choice if you want zero-network, zero-config lookups and don't need romaji; binary size grows with the dataset. `kenall-rs` wraps the commercial KENALL service. `jpzip` sits between them: HTTP-fetched (so the dataset stays current without re-publishing the crate), preloadable (so production traffic doesn't depend on the CDN per-request), and free.

## Other Languages

Same API surface across all SDKs:

[Go](https://github.com/jpzip/go) · [TypeScript](https://github.com/jpzip/js) · [Python](https://github.com/jpzip/python) · [Ruby](https://github.com/jpzip/ruby) · [PHP](https://github.com/jpzip/php) · [Swift](https://github.com/jpzip/swift) · [Dart](https://github.com/jpzip/dart)

## Resources

- **Website** — https://jpzip.nadai.dev
- **Protocol spec** — [jpzip/spec](https://github.com/jpzip/spec)
- **Data ETL** — [jpzip/data](https://github.com/jpzip/data)
- **MCP server** — [jpzip/mcp](https://github.com/jpzip/mcp) — use jpzip from Claude / ChatGPT / Cursor

## Keywords

japanese postal code, japan zipcode, 郵便番号, KEN_ALL, KEN_ALL_ROME, address validation, japan address api, postal code lookup rust, rust japanese address, async zipcode crate, JIS X 0401, 総務省地方公共団体コード

## License

[MIT](./LICENSE)
