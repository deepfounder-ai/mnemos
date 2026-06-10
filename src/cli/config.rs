//! CLI configuration resolution.
//!
//! Precedence: explicit `--api-url` / `--api-key` flags, then the
//! `MNEMOS_API_URL` / `MNEMOS_API_KEY` env vars, then compiled defaults.
//! There is no config file in v0.1 (see `docs/cli.md`).

use std::path::PathBuf;

use crate::cli::output::CliResult;

const DEFAULT_API_URL: &str = "http://localhost:8080";

/// Resolved CLI configuration.
#[derive(Debug, Clone)]
pub struct CliConfig {
    pub api_url: String,
    pub api_key: Option<String>,
    /// Where a future version would persist the key after `user login`.
    /// Unused in v0.1 beyond diagnostics.
    pub credentials_path: PathBuf,
    /// Path to a TOML config file (reserved for v0.2).
    pub config_path: Option<PathBuf>,
}

impl CliConfig {
    /// Resolve config from flags + environment.
    pub fn load(api_url: Option<String>, api_key: Option<String>) -> CliResult<Self> {
        let api_url = api_url
            .or_else(|| std::env::var("MNEMOS_API_URL").ok())
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());
        let api_key = api_key.or_else(|| std::env::var("MNEMOS_API_KEY").ok());
        let config_path = std::env::var("MNEMOS_CONFIG").ok().map(PathBuf::from);

        Ok(Self {
            api_url,
            api_key,
            credentials_path: default_credentials_path(),
            config_path,
        })
    }
}

fn default_credentials_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("mnemos")
        .join("credentials")
}
