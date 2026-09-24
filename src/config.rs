//! Environment-driven configuration.
//!
//! All values are loaded once at process start. The `MNEMOS_*` env vars take
//! precedence over compiled defaults. Tests construct [`Config::for_test`]
//! directly with a temporary directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Top-level service configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// HTTP host to bind to.
    #[serde(default = "default_host")]
    pub host: String,

    /// HTTP port to listen on.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Directory for the SQLite database and on-disk user data.
    pub data_dir: PathBuf,

    /// SQLx connection string. If empty, defaults to
    /// `sqlite://<data_dir>/mnemos.db?mode=rwc`.
    #[serde(default)]
    pub db_url: String,

    /// Log filter (overridden by `RUST_LOG` if set in the environment).
    #[serde(default = "default_log_filter")]
    pub log_filter: String,

    /// Maximum size of a single fetched URL source, in bytes.
    #[serde(default = "default_max_source_bytes")]
    pub max_source_bytes: usize,

    /// Timeout for URL source fetches, in seconds.
    #[serde(default = "default_source_timeout_secs")]
    pub source_timeout_secs: u64,

    /// Optional shared secret that gates user registration. When set, every
    /// `POST /api/v1/auth/register` must present a matching `secret`. When
    /// empty, registration is open (the default). Set via `MNEMOS_SECRET` to
    /// keep a public instance from accumulating unwanted accounts.
    #[serde(default)]
    pub registration_secret: String,

    /// TypeSafe (Jev) knowledge-enrichment settings. When `typesafe_api_key`
    /// is empty the whole feature is off and the server behaves exactly as
    /// before — no Jev calls, no background work.
    #[serde(default)]
    pub enrich: EnrichConfig,
}

/// Configuration for the optional TypeSafe/Jev enrichment pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichConfig {
    /// TypeSafe API key (`MNEMOS_TYPESAFE_API_KEY`). Empty = feature disabled.
    #[serde(default)]
    pub api_key: String,
    /// Endpoint (`MNEMOS_TYPESAFE_URL`).
    #[serde(default = "default_typesafe_url")]
    pub url: String,
    /// Pinned model id (`MNEMOS_TYPESAFE_MODEL`). Pin a version, not `jev-latest`.
    #[serde(default = "default_typesafe_model")]
    pub model: String,
    /// noul threshold to add a `related` edge (`MNEMOS_ENRICH_RELATED_THRESHOLD`).
    #[serde(default = "default_related_threshold")]
    pub related_threshold: f64,
    /// noul threshold to add a tag (`MNEMOS_ENRICH_TAG_THRESHOLD`).
    #[serde(default = "default_tag_threshold")]
    pub tag_threshold: f64,
    /// choice confidence threshold to overwrite an existing page_type
    /// (`MNEMOS_ENRICH_TYPE_OVERRIDE`, default false = only fill when missing).
    #[serde(default)]
    pub type_override: bool,
    /// Max related edges + max tags added per page per pass, to bound cost.
    #[serde(default = "default_enrich_cap")]
    pub max_additions: usize,
}

impl Default for EnrichConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            url: default_typesafe_url(),
            model: default_typesafe_model(),
            related_threshold: default_related_threshold(),
            tag_threshold: default_tag_threshold(),
            type_override: false,
            max_additions: default_enrich_cap(),
        }
    }
}

impl EnrichConfig {
    /// Whether the enrichment feature is active.
    pub fn enabled(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

fn default_typesafe_url() -> String {
    "https://api.typesafe.ai/v1/systemone".to_string()
}
fn default_typesafe_model() -> String {
    "jev-1.13.0".to_string()
}
fn default_related_threshold() -> f64 {
    // Calibrated 2026-09-24 on a 58-page corpus (see docs/enrichment.md):
    // 0.6 gives ~2x the recall of 0.75 at similar measured precision.
    0.6
}
fn default_tag_threshold() -> f64 {
    0.8
}
fn default_enrich_cap() -> usize {
    8
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_log_filter() -> String {
    "mnemos=info,tower_http=info,axum=info".to_string()
}

fn default_max_source_bytes() -> usize {
    10 * 1024 * 1024 // 10 MB
}

fn default_source_timeout_secs() -> u64 {
    30
}

impl Config {
    /// Build a config from environment variables. Falls back to `./data` for
    /// `data_dir` and `8080` for `port` if nothing is set.
    pub fn from_env() -> Result<Self, ConfigError> {
        let data_dir: PathBuf = std::env::var("MNEMOS_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));

        let port: u16 = match std::env::var("MNEMOS_PORT") {
            Ok(s) => s.parse().map_err(|e: std::num::ParseIntError| {
                ConfigError::ParseInt(format!("MNEMOS_PORT: {e}"))
            })?,
            Err(_) => default_port(),
        };

        let host = std::env::var("MNEMOS_HOST").unwrap_or_else(|_| default_host());
        let db_url = std::env::var("MNEMOS_DB_URL").unwrap_or_default();
        let log_filter = std::env::var("MNEMOS_LOG")
            .or_else(|_| std::env::var("RUST_LOG"))
            .unwrap_or_else(|_| default_log_filter());

        let max_source_bytes = match std::env::var("MNEMOS_MAX_SOURCE_BYTES") {
            Ok(s) => s.parse().map_err(|e: std::num::ParseIntError| {
                ConfigError::ParseInt(format!("MNEMOS_MAX_SOURCE_BYTES: {e}"))
            })?,
            Err(_) => default_max_source_bytes(),
        };

        let source_timeout_secs = match std::env::var("MNEMOS_SOURCE_TIMEOUT_SECS") {
            Ok(s) => s.parse().map_err(|e: std::num::ParseIntError| {
                ConfigError::ParseInt(format!("MNEMOS_SOURCE_TIMEOUT_SECS: {e}"))
            })?,
            Err(_) => default_source_timeout_secs(),
        };

        let registration_secret = std::env::var("MNEMOS_SECRET").unwrap_or_default();

        let enrich = EnrichConfig {
            api_key: std::env::var("MNEMOS_TYPESAFE_API_KEY").unwrap_or_default(),
            url: std::env::var("MNEMOS_TYPESAFE_URL").unwrap_or_else(|_| default_typesafe_url()),
            model: std::env::var("MNEMOS_TYPESAFE_MODEL")
                .unwrap_or_else(|_| default_typesafe_model()),
            related_threshold: parse_env_f64(
                "MNEMOS_ENRICH_RELATED_THRESHOLD",
                default_related_threshold(),
            )?,
            tag_threshold: parse_env_f64("MNEMOS_ENRICH_TAG_THRESHOLD", default_tag_threshold())?,
            type_override: std::env::var("MNEMOS_ENRICH_TYPE_OVERRIDE")
                .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
                .unwrap_or(false),
            max_additions: match std::env::var("MNEMOS_ENRICH_MAX_ADDITIONS") {
                Ok(s) => s
                    .parse()
                    .map_err(|e: std::num::ParseIntError| {
                        ConfigError::ParseInt(format!("MNEMOS_ENRICH_MAX_ADDITIONS: {e}"))
                    })?,
                Err(_) => default_enrich_cap(),
            },
        };

        Ok(Self {
            host,
            port,
            data_dir,
            db_url,
            log_filter,
            max_source_bytes,
            source_timeout_secs,
            registration_secret,
            enrich,
        })
    }

    /// Build a deterministic config for tests. The data dir is the caller's
    /// responsibility — pass a `tempfile::TempDir` path so cleanup is easy.
    pub fn for_test(data_dir: PathBuf) -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 0,
            data_dir,
            db_url: String::new(),
            log_filter: "warn".to_string(),
            max_source_bytes: default_max_source_bytes(),
            source_timeout_secs: default_source_timeout_secs(),
            registration_secret: String::new(),
            enrich: EnrichConfig::default(),
        }
    }

    /// Resolve the final SQLite connection URL, taking `db_url` overrides
    /// into account.
    pub fn resolved_db_url(&self) -> String {
        if self.db_url.is_empty() {
            let path = self.data_dir.join("mnemos.db");
            format!("sqlite://{}?mode=rwc", path.display())
        } else {
            self.db_url.clone()
        }
    }
}

fn parse_env_f64(var: &str, default: f64) -> Result<f64, ConfigError> {
    match std::env::var(var) {
        Ok(s) => s
            .parse()
            .map_err(|e: std::num::ParseFloatError| ConfigError::ParseInt(format!("{var}: {e}"))),
        Err(_) => Ok(default),
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid number in env: {0}")]
    ParseInt(String),
}
