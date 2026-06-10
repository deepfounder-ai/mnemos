//! `mnemos` binary entry point.

use std::process::ExitCode;

use clap::Parser;

use mnemos::cli::commands::run as run_cli;
use mnemos::cli::Cli;
use mnemos::config::Config;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Initialise tracing once, before any subcommand runs. Logs go to
    // stderr so the MCP server's stdout stays a clean JSON-RPC stream.
    let log_filter = Config::from_env()
        .map(|c| c.log_filter)
        .unwrap_or_else(|_| "info".to_string());
    init_tracing(&log_filter);

    run_cli(cli).await
}

fn init_tracing(log_filter: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_new(log_filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}
