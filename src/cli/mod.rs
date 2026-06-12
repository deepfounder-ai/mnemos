//! CLI surface (clap derive). The CLI is a thin HTTP client over the REST
//! API (`docs/cli.md`); `serve`, `mcp`, and `completions` are the only
//! subcommands that run in-process.

pub mod client;
pub mod commands;
pub mod config;
pub mod output;

use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "mnemos",
    version,
    about = "Cloud memory for AI agents — persistent wiki + sources + index",
    long_about = None,
)]
pub struct Cli {
    /// Base URL of the mnemos server (overrides MNEMOS_API_URL).
    #[arg(long, global = true)]
    pub api_url: Option<String>,

    /// API key (overrides MNEMOS_API_KEY).
    #[arg(long, global = true)]
    pub api_key: Option<String>,

    /// Emit raw JSON instead of human-readable output.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Run the HTTP API server.
    Serve {
        /// Override host (defaults to MNEMOS_HOST or 0.0.0.0).
        #[arg(long)]
        host: Option<String>,
        /// Override port (defaults to MNEMOS_PORT or 8080).
        #[arg(long)]
        port: Option<u16>,
    },
    /// Run the MCP server on stdio.
    Mcp,
    /// Generate shell completions.
    Completions {
        /// Target shell.
        shell: Shell,
    },
    /// User management.
    #[command(subcommand)]
    User(UserCmd),
    /// API key management.
    #[command(subcommand)]
    Keys(KeysCmd),
    /// Wiki page operations.
    #[command(subcommand)]
    Pages(PagesCmd),
    /// Source management.
    #[command(subcommand)]
    Sources(SourcesCmd),
    /// Search pages (FTS5 ranked).
    Search {
        /// Search query.
        query: String,
        /// Max number of hits.
        #[arg(long, default_value_t = 10)]
        limit: i64,
    },
    /// Print the auto-generated index.
    Index,
    /// Print the append-only event log.
    Log {
        /// Only events after this RFC3339 timestamp.
        #[arg(long)]
        since: Option<String>,
        /// Max number of events.
        #[arg(long, default_value_t = 100)]
        limit: i64,
    },
    /// Run linter over the user's wiki.
    Lint,
    /// Register the mnemos MCP server with Claude Code (runs `claude mcp add`).
    Setup {
        /// Override the data directory passed to the MCP server.
        #[arg(long)]
        data_dir: Option<String>,
        /// Override the API key passed to the MCP server.
        #[arg(long)]
        api_key: Option<String>,
        /// Claude MCP scope: local | user | project.
        #[arg(long, default_value = "user")]
        scope: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum UserCmd {
    /// Register a new user; prints the initial API key once.
    Register {
        username: String,
        #[arg(long)]
        password: Option<String>,
        /// Read the password from stdin (trailing newline trimmed).
        #[arg(long)]
        password_stdin: bool,
        /// Registration secret (required if the server sets MNEMOS_SECRET).
        /// Falls back to the MNEMOS_SECRET env var.
        #[arg(long)]
        secret: Option<String>,
    },
    /// Exchange username + password for a fresh API key.
    Login {
        username: String,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        password_stdin: bool,
    },
    /// Print the user_id + username of the configured API key.
    Whoami,
}

#[derive(Debug, Subcommand)]
pub enum KeysCmd {
    List,
    Create { name: String },
    Revoke { id: String },
}

#[derive(Debug, Subcommand)]
pub enum PagesCmd {
    List {
        #[arg(long)]
        tag: Option<String>,
        #[arg(long = "type")]
        page_type: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        query: Option<String>,
    },
    Get {
        slug: String,
    },
    Create {
        slug: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        from_file: Option<String>,
        #[arg(long)]
        from_stdin: bool,
    },
    Update {
        slug: String,
        #[arg(long)]
        from_file: Option<String>,
        #[arg(long)]
        from_stdin: bool,
    },
    Delete {
        slug: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum SourcesCmd {
    List,
    AddUrl {
        url: String,
        #[arg(long)]
        slug: Option<String>,
    },
    Upload {
        file: String,
        #[arg(long)]
        slug: Option<String>,
    },
    Get {
        id: String,
    },
}
