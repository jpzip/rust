use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The jpzip protocol version this SDK targets.
pub const SPEC_VERSION: &str = "1.0";

/// Production CDN origin.
pub const DEFAULT_BASE_URL: &str = "https://jpzip.nadai.dev";

/// One element of [`ZipcodeEntry::towns`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Town {
    pub town: String,
    pub kana: String,
    pub roma: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One logical entry as published by the CDN.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZipcodeEntry {
    pub prefecture: String,
    pub prefecture_kana: String,
    pub prefecture_roma: String,
    #[serde(default)]
    pub prefecture_code: String,
    pub city: String,
    pub city_kana: String,
    pub city_roma: String,
    pub city_code: String,
    pub towns: Vec<Town>,
}

/// On-the-wire shape of `/g/*.json` and `/p/*.json`.
pub type ZipcodeDict = HashMap<String, ZipcodeEntry>;

/// Endpoint template block of `/meta.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Endpoints {
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub prefix: String,
}

/// Contents of `/meta.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Meta {
    pub version: String,
    #[serde(default)]
    pub generated_at: String,
    pub spec_version: String,
    #[serde(default)]
    pub total_zipcodes: u64,
    #[serde(default)]
    pub prefix_count: u64,
    #[serde(default)]
    pub by_pref: HashMap<String, u64>,
    #[serde(default)]
    pub data_source: String,
    #[serde(default)]
    pub endpoints: Endpoints,
}
