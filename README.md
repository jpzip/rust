# jpzip — Rust SDK

> 日本の郵便番号を CDN 配信の JSON データから引く Rust SDK (tokio + reqwest)。

- 配信ドメイン: `https://jpzip.nadai.dev`
- プロトコル仕様: [`jpzip/spec`](https://github.com/jpzip/spec)
- データ ETL: [`jpzip/data`](https://github.com/jpzip/data)

```sh
cargo add jpzip
```

## 使い方

### 関数 API

```rust
use jpzip;

# async fn run() -> Result<(), jpzip::Error> {
let entry = jpzip::lookup("2310831").await?;
// None なら見つからなかった

let dict = jpzip::lookup_group("23").await?; // 2 桁は 10 並列 fetch
let all  = jpzip::lookup_all().await?;
let meta = jpzip::get_meta().await?;
# Ok(())
# }
```

### クライアント API (L2 キャッシュ・複数インスタンス用)

```rust
use std::sync::Arc;
use jpzip::{JpzipClient, Cache};

# async fn run(my_cache: Arc<dyn Cache>) -> Result<(), jpzip::Error> {
let client = JpzipClient::builder()
    .base_url("https://jpzip.nadai.dev")
    .memory_cache_size(200)
    .cache(my_cache) // Cache トレイトを実装
    .on_spec_mismatch(|expected, received| {
        eprintln!("spec mismatch: expected {} got {}", expected, received);
    })
    .build();

client.preload("all").await?;
let entry = client.lookup("2310831").await?;
# Ok(())
# }
```

## Cache トレイト

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

ファイル / KV / Redis 等の任意の実装を渡せる。`Arc<dyn Cache>` で `JpzipClient::builder().cache(...)` に渡す。

## 入力検証

`lookup()` は `^\d{7}$` にマッチしない入力には fetch せず `Ok(None)` を返す。`is_valid_zipcode()` で同じ判定を行える。

## バージョン整合性

`get_meta()` で `spec_version` が異なる場合、`on_spec_mismatch` で渡したコールバックが 1 度だけ呼ばれる。データバージョン (`version`) が変わったら L1/L2 を自動 invalidate する。

## HTTP

- `reqwest` の `rustls-tls` フィーチャーを使用 (OS ネイティブ TLS には依存しない)
- 5xx およびネットワークエラーは指数バックオフで最大 3 回リトライ (200ms × 2^attempt)
- 404 は `Ok(None)` / 空辞書として正常終了

## MSRV

Rust 1.75 以上。

## ライセンス

[MIT](./LICENSE)
