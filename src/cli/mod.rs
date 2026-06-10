//! CLI commands (clap derive). Phase 1 exposes the full subcommand
//! surface; each command is a `unimplemented!()` stub except for `serve`
//! and `mcp` which are wired in `main.rs`.

pub mod cmd;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "mnemos",
    version,
    about = "Cloud memory for AI agents — persistent wiki + sources + index",
    long_about = None,
)]
pub struct Cli {
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
    /// Search pages.
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
}

#[derive(Debug, Subcommand)]
pub enum UserCmd {
    Register {
        username: String,
        #[arg(long)]
        password: Option<String>,
    },
    Login {
        username: String,
        #[arg(long)]
        password: Option<String>,
    },
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
        query: Option<String>,
    },
    Get { slug: String },
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
    Delete { slug: String },
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
    Get { id: String },
}
