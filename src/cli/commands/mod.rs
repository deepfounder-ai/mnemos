//! Subcommand dispatch. Each top-level `Cmd` variant is handled by a
//! function in one of the sibling modules:
//!
//! - [`user`] — `user register`, `user login`, `user whoami`
//! - [`keys`] — API key management
//! - [`pages`] — page CRUD
//! - [`sources`] — source management
//! - [`misc`] — `search`, `index`, `log`, `lint`, `serve`, `mcp`
//!
//! The [`run`] function is the entry point called from `main.rs`. It
//! resolves the [`CliConfig`], builds a [`Client`], and dispatches.

pub mod keys;
pub mod misc;
pub mod pages;
pub mod sources;
pub mod user;

use std::process::ExitCode;

use crate::cli::client::Client;
use crate::cli::config::CliConfig;
use crate::cli::output::{fail, CliResult};
use crate::cli::{Cli, Cmd};

/// Entry point. Returns an [`ExitCode`] suitable for `std::process::exit`.
pub async fn run(cli: Cli) -> ExitCode {
    match dispatch(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

async fn dispatch(cli: Cli) -> CliResult<()> {
    // serve / mcp / setup are special — they don't need a client.
    match &cli.command {
        Cmd::Serve { .. } => {
            return misc::run_serve(&cli).await;
        }
        Cmd::Mcp => {
            return misc::run_mcp().await;
        }
        Cmd::Completions { shell } => {
            misc::run_completions(*shell);
            return Ok(());
        }
        Cmd::Setup { data_dir, api_key, scope } => {
            return misc::run_setup(data_dir.as_deref(), api_key.as_deref(), scope);
        }
        _ => {}
    }

    // All other commands need a resolved config.
    let config = CliConfig::load(cli.api_url.clone(), cli.api_key.clone())?;
    let client = Client::new(&config)?;

    match cli.command {
        Cmd::Serve { .. } | Cmd::Mcp | Cmd::Completions { .. } | Cmd::Setup { .. } => unreachable!(),
        Cmd::User(u) => user::run(client, cli.json, u).await,
        Cmd::Keys(k) => keys::run(client, cli.json, k).await,
        Cmd::Pages(p) => pages::run(client, cli.json, p).await,
        Cmd::Sources(s) => sources::run(client, cli.json, s).await,
        Cmd::Search { query, limit } => misc::run_search(&client, cli.json, &query, limit).await,
        Cmd::Index => misc::run_index(&client, cli.json).await,
        Cmd::Log { since, limit } => {
            misc::run_log(&client, cli.json, since.as_deref(), limit).await
        }
        Cmd::Lint => misc::run_lint(&client, cli.json).await,
    }
}

/// Helper used by the per-family modules: convert a flag/enum into a
/// "use JSON output?" boolean, defaulting to false on human form.
pub fn json_mode(flag: bool) -> bool {
    flag
}

/// Helper: build a stable path under `/api/v1`. Centralised so the
/// MCP client can reuse the same paths later.
pub fn api_v1(suffix: &str) -> String {
    if suffix.starts_with('/') {
        format!("/api/v1{suffix}")
    } else {
        format!("/api/v1/{suffix}")
    }
}

/// Trivial helper for handlers that don't return data — just signals
/// success.
pub fn ok<T>(t: T) -> CliResult<T> {
    Ok(t)
}
