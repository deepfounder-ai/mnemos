//! CLI command dispatcher. Phase 1: only `serve` and `mcp` do real work;
//! every other subcommand is an explicit "not yet implemented" stub so
//! the surface is observable to users.

use anyhow::Context;

use crate::cli::{Cli, Cmd, KeysCmd, PagesCmd, SourcesCmd, UserCmd};
use crate::config::Config;

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    // Load config once. `serve` and `mcp` mutate a copy in-place.
    let config = Config::from_env().context("load config")?;
    match cli.command {
        Cmd::Serve { host, port } => cmd_serve(config, host, port).await,
        Cmd::Mcp => crate::mcp::run_stdio().await,
        cmd => cmd_stub(cmd).await,
    }
}

async fn cmd_serve(
    mut config: Config,
    host: Option<String>,
    port: Option<u16>,
) -> anyhow::Result<()> {
    if let Some(h) = host {
        config.host = h;
    }
    if let Some(p) = port {
        config.port = p;
    }
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("create data dir {}", config.data_dir.display()))?;

    let state = crate::storage::init_pool(config).await?;
    let config_ref = state.config.clone();
    crate::api::serve(state, &config_ref).await?;
    Ok(())
}

async fn cmd_stub(cmd: Cmd) -> anyhow::Result<()> {
    let summary = describe(&cmd);
    eprintln!("mnemos: '{summary}' is not yet implemented in phase 1 foundation");
    Err(anyhow::anyhow!("not implemented: {summary}"))
}

fn describe(cmd: &Cmd) -> String {
    match cmd {
        Cmd::User(u) => match u {
            UserCmd::Register { username, .. } => format!("user register {username}"),
            UserCmd::Login { username, .. } => format!("user login {username}"),
            UserCmd::Whoami => "user whoami".to_string(),
        },
        Cmd::Keys(k) => match k {
            KeysCmd::List => "keys list".to_string(),
            KeysCmd::Create { name } => format!("keys create {name}"),
            KeysCmd::Revoke { id } => format!("keys revoke {id}"),
        },
        Cmd::Pages(p) => match p {
            PagesCmd::List { .. } => "pages list".to_string(),
            PagesCmd::Get { slug } => format!("pages get {slug}"),
            PagesCmd::Create { slug, .. } => format!("pages create {slug}"),
            PagesCmd::Update { slug, .. } => format!("pages update {slug}"),
            PagesCmd::Delete { slug } => format!("pages delete {slug}"),
        },
        Cmd::Sources(s) => match s {
            SourcesCmd::List => "sources list".to_string(),
            SourcesCmd::AddUrl { url, .. } => format!("sources add-url {url}"),
            SourcesCmd::Upload { file, .. } => format!("sources upload {file}"),
            SourcesCmd::Get { id } => format!("sources get {id}"),
        },
        Cmd::Search { query, .. } => format!("search {query}"),
        Cmd::Index => "index".to_string(),
        Cmd::Log { .. } => "log".to_string(),
        Cmd::Lint => "lint".to_string(),
        Cmd::Serve { .. } | Cmd::Mcp => unreachable!("handled above"),
    }
}
