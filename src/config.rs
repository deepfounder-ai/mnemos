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

        Ok(Self {
            host,
            port,
            data_dir,
            db_url,
            log_filter,
            max_source_bytes,
            source_timeout_secs,
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

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid integer in env: {0}")]
    ParseInt(String),
}
