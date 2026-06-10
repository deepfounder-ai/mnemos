//! `mnemos` binary entry point.

use anyhow::Context;
use clap::Parser;

use mnemos::cli::cmd::run as run_cli;
use mnemos::cli::Cli;
use mnemos::config::Config;
use mnemos::storage::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialise tracing once. We want logs from startup (config parsing,
    // DB migration, etc.) to appear too.
    let config = Config::from_env().context("load config")?;
    init_tracing(&config);

    run_cli(cli).await
}

async fn serve_direct(host: Option<String>, port: Option<u16>) -> anyhow::Result<()> {
    // Kept for potential future use; the canonical entrypoint is now
    // `cli::run(Cli { command: Cmd::Serve { host, port } })`.
    let mut config = Config::from_env().context("load config")?;
    if let Some(h) = host {
        config.host = h;
    }
    if let Some(p) = port {
        config.port = p;
    }
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("create data dir {}", config.data_dir.display()))?;

    let state: AppState = mnemos::storage::init_pool(config).await?;
    let cfg = state.config.clone();
    mnemos::api::serve(state, &cfg).await?;
    Ok(())
}

#[allow(dead_code)]
fn _unused_serve_direct_silencer() {
    let _ = serve_direct;
}

fn init_tracing(config: &Config) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_new(&config.log_filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
