# jpzip

[![crates.io](https://img.shields.io/crates/v/jpzip.svg)](https://crates.io/crates/jpzip)
[![docs.rs](https://docs.rs/jpzip/badge.svg)](https://docs.rs/jpzip)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Publish](https://github.com/jpzip/rust/actions/workflows/publish.yml/badge.svg)](https://github.com/jpzip/rust/actions/workflows/publish.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.75-blue.svg)](#必要環境)

> **jpzip** の Rust SDK — 無料・無制限の日本郵便番号 API。
> 日本郵便の `KEN_ALL.csv` / `KEN_ALL_ROME.csv` を JSON 正規化し CDN 配信。

[English](./README.md) | **日本語**

`jpzip` は `jpzip.nadai.dev` から日本の郵便番号 120,677 件を引く Rust SDK です。
登録不要、レート制限なし、API キー不要。

- 🇯🇵 **全件収録** — 漢字・カナ・ローマ字・自治体コード(JIS X 0401 / 総務省地方公共団体コード)
- ⚡️ **非同期 + キャッシュ** — `tokio` + `reqwest`、L1 LRU + 任意の L2 永続キャッシュ。`preload` でネットワーク往復なしのルックアップが可能
- 🛡️ **堅牢** — 5xx / ネットワーク失敗時は指数バックオフで最大 3 回リトライ
- 🪶 **軽量** — pure-Rust TLS (`rustls`) のみ使用。`openssl-sys` 不要・ビルド時 C ツールチェーン不要
- 🆓 **永久無料** — Cloudflare Pages 無料枠で運用(課金軸が存在しない)
- 🔌 **同一 API** — [全 jpzip SDK](#他言語版) で API が揃う

## 必要環境

Rust 1.75 以上(edition 2021)、tokio ランタイム。

## インストール

```bash
cargo add jpzip
```

または `Cargo.toml` に:

```toml
[dependencies]
jpzip = "0.1"
tokio = { version = "1", features = ["full"] }
```

## クイックスタート

```rust
#[tokio::main]
async fn main() -> Result<(), jpzip::Error> {
    let entry = jpzip::lookup("2310017").await?;
    match entry {
        Some(e) => {
            println!("{} {} {}", e.prefecture, e.city, e.towns[0].town);
            // 出力: 神奈川県 横浜市中区 港町
        }
        None => println!("見つかりません"),
    }
    Ok(())
}
```

ローマ字・自治体コードも同じエントリに含まれます:

```rust
let e = jpzip::lookup("2310017").await?.unwrap();
println!("{} {} {}", e.prefecture_roma, e.city_roma, e.towns[0].roma);
// 出力: Kanagawa Ken Yokohama Shi Naka Ku Minatocho

println!("{} {}", e.prefecture_code, e.city_code);
// 出力: 14 14104
```

## ユースケース

### 郵便番号ルックアップ HTTP エンドポイント (axum)

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

### 郵便番号ルックアップ HTTP エンドポイント (actix-web)

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

### CSV のバッチ検証

```rust
let all = jpzip::lookup_all().await?; // 全件をメモリに展開(JSON 約 37 MiB)
for zip in csv_zipcodes {
    if !all.contains_key(&zip) {
        eprintln!("不正な郵便番号: {zip}");
    }
}
```

### キャッシュからの提供(任意の L2 バックエンド)

データは 948 個の 3 桁 prefix バケットに分割されています。デフォルト L1 (100 件)
はホットなバケットを保持しますが、全件を常駐させるには L2 を併用するか
`memory_cache_size` を 948 超に設定してください。

```rust
use std::sync::Arc;
use jpzip::{Cache, JpzipClient};

# async fn run(my_file_cache: Arc<dyn Cache>) -> Result<(), jpzip::Error> {
let client = JpzipClient::builder()
    .memory_cache_size(1024)
    .cache(my_file_cache) // Cache トレイト実装を渡す
    .build();

client.preload("all").await?;
// 以降の lookup は L1/L2 で完結し、ネットワークにアクセスしない
let entry = client.lookup("2310017").await?;
# Ok(())
# }
```

## API リファレンス

完全版は [docs.rs/jpzip](https://docs.rs/jpzip) を参照。

### 関数(クレートレベル、内部の default `JpzipClient` を共有)

| 関数 | 説明 |
|---|---|
| `lookup(zipcode)` | 7 桁の郵便番号で 1 件引く。見つからない / 不正な入力は `Ok(None)`(不正入力時はネットワーク不使用)。 |
| `lookup_group(prefix)` | 1〜3 桁の prefix で引く。1 桁は `/g/{d}.json` を 1 回、3 桁は `/p/{ddd}.json` を 1 回、2 桁は 10 並列 fetch して結合。 |
| `lookup_all()` | `/g/0..9.json` を並列取得して全件(120k 件、約 37 MiB)を返す。 |
| `get_meta()` | データバージョン・生成日時・都道府県別件数・spec version。`refresh` までは結果をキャッシュ。 |
| `preload(scope)` | `"all"` または特定 prefix で L1(L2 設定時は L2 も)を温める。 |
| `is_valid_zipcode(s)` | 純粋な書式チェック(`^\d{7}$`)。ネットワーク不使用。 |

すべて async、戻り値は `Result<_, jpzip::Error>`。

### `JpzipClient`(高度な用途)

`JpzipClient::builder()` で設定可能なインスタンスを取得。L2 キャッシュ、HTTP クライアント差し替え、配信元変更、複数の独立キャッシュが必要な場合に使用:

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
    .memory_cache_size(200)         // L1 容量(prefix バケット数)、デフォルト 100
    .cache(my_cache)                // L2(任意、Arc<dyn Cache>)
    .on_spec_mismatch(|expected, received| {
        eprintln!("jpzip spec 不一致: SDK={expected} server={received}");
    })
    .build();
```

`JpzipClient` は `lookup` / `lookup_group` / `lookup_all` / `get_meta` / `preload` に加えて:

| メソッド | 説明 |
|---|---|
| `client.refresh()` | L1(L2 設定時は L2 も)を消し、キャッシュ済み meta を破棄。 |

`get_meta` が `/meta.json` の `version` 変更を検知すると L1/L2 が自動クリアされます。データ切り替えに追従するには `get_meta` を定期的に呼んでください。

### エラー

`jpzip::Error` は `thiserror` 派生の単一 enum:

| バリアント | 発生条件 |
|---|---|
| `Error::Http(reqwest::Error)` | リトライを使い切ったトランスポート層失敗。 |
| `Error::Status { url, status }` | 2xx 以外の HTTP ステータス(404 は `Ok(None)` に変換)。 |
| `Error::Parse(serde_json::Error)` | JSON 形状がデータスキーマと一致しない。 |
| `Error::InvalidPrefix(String)` | `lookup_group` / `preload` に 1〜3 桁以外の prefix が渡された。 |
| `Error::Cache(Box<dyn Error + Send + Sync>)` | ユーザー実装の `Cache` から返ったエラー。 |

ネットワーク失敗と 5xx は最大 3 回試行(初回 + リトライ 2 回)、指数バックオフのスリープは 400ms / 800ms。404 以外の 4xx は即座にエラー返却。

### `Cache` トレイト

任意の L2 バックエンド(ファイル / sled / Redis / KV など)を渡せます:

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

`Arc<dyn Cache>` を `JpzipClient::builder().cache(...)` に渡してください。キーは prefix バケットの完全 URL(例: `https://jpzip.nadai.dev/p/231.json`)、値は生 JSON バイト列。

## なぜ jpzip か

| | **jpzip** | [jpostcode_rs][jpostcode_rs] | [kenall-rs][kenall_rs] | [zipcloud API][zipcloud] |
|---|---|---|---|---|
| ローマ字(`Yokohama Shi`) | ✅ | ❌ | ⚠️ 有料プラン | ❌ |
| 自治体コード(JIS / 総務省) | ✅ | ⚠️ JIS のみ | ✅ | ❌ |
| CSV を手動 DL 不要 | ✅ | ✅ 埋め込み | ✅ | ✅ |
| 月次更新 | ✅ 自動 | ❌ クレート再公開時 | ✅ | ✅ |
| Preload 後オフライン | ✅ | ✅(常時) | ❌ | ❌ |
| API キー不要 | ✅ | ✅ | ❌ 必須 | ✅ |
| レート制限なし | ✅ | ✅ | ⚠️ プラン依存 | ⚠️ 大量アクセス非推奨 |
| 非同期(`tokio`) | ✅ | ❌ 同期 | ✅ | n/a |
| L1 + 差し替え可能な L2 | ✅ | n/a(バイナリ内蔵) | ❌ | ❌ |

[jpostcode_rs]: https://github.com/nwiizo/jpostcode_rs
[kenall_rs]: https://github.com/chansuke/kenall-rs
[zipcloud]: http://zipcloud.ibsnet.co.jp/doc/api

`jpostcode_rs` は「ネットワーク不要・設定不要で引きたい、ローマ字は不要」というケース向きで、バイナリサイズはデータ量だけ膨らみます。`kenall-rs` は商用 KENALL サービスのラッパーです。`jpzip` はその中間に位置付けで、HTTP で取得するためクレート再公開なしにデータが最新化され、`preload` で本番トラフィックを CDN 往復から切り離せて、しかも無料です。

## 他言語版

全 SDK で同一の API を提供しています:

[Go](https://github.com/jpzip/go) · [TypeScript](https://github.com/jpzip/js) · [Python](https://github.com/jpzip/python) · [Ruby](https://github.com/jpzip/ruby) · [PHP](https://github.com/jpzip/php) · [Swift](https://github.com/jpzip/swift) · [Dart](https://github.com/jpzip/dart)

## 関連リソース

- **Web サイト** — https://jpzip.nadai.dev
- **プロトコル仕様** — [jpzip/spec](https://github.com/jpzip/spec)
- **データ ETL** — [jpzip/data](https://github.com/jpzip/data)
- **MCP サーバー** — [jpzip/mcp](https://github.com/jpzip/mcp) — Claude / ChatGPT / Cursor から jpzip を呼ぶ

## キーワード

日本郵便番号, 郵便番号, KEN_ALL, KEN_ALL_ROME, 住所検索, 住所バリデーション, japanese postal code, japan zipcode, postal code lookup rust, rust japanese address, async zipcode crate, JIS X 0401, 総務省地方公共団体コード

## ライセンス

[MIT](./LICENSE)
