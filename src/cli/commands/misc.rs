//! `serve`, `mcp`, `completions`, `search`, `index`, `log`, `lint`.

use clap::CommandFactory;
use serde::Deserialize;
use serde_json::json;

use crate::cli::client::Client;
use crate::cli::output::{print_json, print_line, CliError, CliResult};
use crate::cli::{Cli, Cmd};
use crate::config::Config;

// ---------------------------------------------------------------------------
// In-process commands (no client)
// ---------------------------------------------------------------------------

/// `serve` — boot the HTTP API in-process.
pub async fn run_serve(cli: &Cli) -> CliResult<()> {
    let (host, port) = match &cli.command {
        Cmd::Serve { host, port } => (host.clone(), *port),
        _ => (None, None),
    };

    let mut config = Config::from_env().map_err(|e| CliError::Other(format!("config: {e}")))?;
    if let Some(h) = host {
        config.host = h;
    }
    if let Some(p) = port {
        config.port = p;
    }
    std::fs::create_dir_all(&config.data_dir)?;

    let state = crate::storage::init_pool(config)
        .await
        .map_err(|e| CliError::Other(format!("init store: {e}")))?;
    let cfg = state.config.clone();
    crate::api::serve(state, &cfg)
        .await
        .map_err(|e| CliError::Other(format!("server: {e}")))?;
    Ok(())
}

/// `setup` — register the mnemos MCP server with Claude Code.
pub fn run_setup(data_dir: Option<&str>, api_key: Option<&str>, scope: &str) -> CliResult<()> {
    let bin = std::env::current_exe()
        .map_err(|e| CliError::Other(format!("find binary: {e}")))?;
    let bin_str = bin.to_string_lossy().to_string();

    let data_dir_val = data_dir
        .map(|s| s.to_string())
        .or_else(|| std::env::var("MNEMOS_DATA_DIR").ok())
        .unwrap_or_else(|| "./data".to_string());

    let api_key_val = api_key
        .map(|s| s.to_string())
        .or_else(|| std::env::var("MNEMOS_API_KEY").ok());

    let mut cmd = std::process::Command::new("claude");
    cmd.args(["mcp", "add", "mnemos", &bin_str, "mcp"]);
    cmd.args(["-e", &format!("MNEMOS_DATA_DIR={data_dir_val}")]);
    if let Some(ref key) = api_key_val {
        cmd.args(["-e", &format!("MNEMOS_API_KEY={key}")]);
    }
    cmd.args(["--scope", scope]);

    eprintln!(
        "Running: claude mcp add mnemos {bin_str} mcp -e MNEMOS_DATA_DIR={data_dir_val} --scope {scope}"
    );

    let status = cmd
        .status()
        .map_err(|e| CliError::Other(format!("exec claude: {e}")))?;

    if status.success() {
        println!("mnemos MCP server registered (scope={scope}).");
        if api_key_val.is_none() {
            println!("Tip: set MNEMOS_API_KEY before starting Claude Code, or re-run with --api-key.");
        }
        Ok(())
    } else {
        Err(CliError::Other(format!(
            "claude mcp add exited with status {status}"
        )))
    }
}

/// `mcp` — run the stdio MCP server.
pub async fn run_mcp() -> CliResult<()> {
    crate::mcp::run_stdio()
        .await
        .map_err(|e| CliError::Other(e.to_string()))
}

/// `completions <shell>` — print a completion script to stdout.
pub fn run_completions(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "mnemos", &mut std::io::stdout());
}

// ---------------------------------------------------------------------------
// Client-backed commands
// ---------------------------------------------------------------------------

pub async fn run_search(client: &Client, json: bool, query: &str, limit: i64) -> CliResult<()> {
    let path = format!("/api/v1/search?q={}&limit={limit}", encode(query));
    let v: serde_json::Value = client.get(&path).await?;
    if json {
        return print_json(&v);
    }
    if let Some(hits) = v.get("hits").and_then(|h| h.as_array()) {
        for h in hits {
            let slug = h.get("slug").and_then(|x| x.as_str()).unwrap_or("?");
            let title = h.get("title").and_then(|x| x.as_str()).unwrap_or("");
            print_line(format!("{slug}  — {title}"));
        }
    }
    Ok(())
}

pub async fn run_index(client: &Client, json: bool) -> CliResult<()> {
    let md = client.get_text("/api/v1/index").await?;
    if json {
        return print_json(&json!({ "index": md }));
    }
    print!("{md}");
    Ok(())
}

pub async fn run_log(
    client: &Client,
    json: bool,
    since: Option<&str>,
    limit: i64,
) -> CliResult<()> {
    if json {
        let mut path = format!("/api/v1/log?limit={limit}&format=json");
        if let Some(s) = since {
            path.push_str(&format!("&since={}", encode(s)));
        }
        let v: serde_json::Value = client.get(&path).await?;
        return print_json(&v);
    }
    let mut path = format!("/api/v1/log?limit={limit}&format=md");
    if let Some(s) = since {
        path.push_str(&format!("&since={}", encode(s)));
    }
    let text = client.get_text(&path).await?;
    print!("{text}");
    Ok(())
}

#[derive(Debug, Deserialize)]
struct LintFinding {
    severity: String,
    kind: String,
    #[serde(rename = "ref_", alias = "ref")]
    ref_: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct LintSummary {
    #[serde(default)]
    errors: usize,
    #[serde(default)]
    warnings: usize,
    #[serde(default)]
    info: usize,
}

#[derive(Debug, Deserialize)]
struct LintView {
    findings: Vec<LintFinding>,
    summary: LintSummary,
}

pub async fn run_lint(client: &Client, json: bool) -> CliResult<()> {
    let report: LintView = client.get("/api/v1/lint").await?;
    if json {
        print_json(&json!({
            "findings": report.findings.iter().map(|f| json!({
                "severity": f.severity, "kind": f.kind, "ref": f.ref_, "message": f.message
            })).collect::<Vec<_>>(),
            "summary": {
                "errors": report.summary.errors,
                "warnings": report.summary.warnings,
                "info": report.summary.info,
            }
        }))?;
    } else {
        for f in &report.findings {
            print_line(format!(
                "[{}] {} {}: {}",
                f.severity, f.kind, f.ref_, f.message
            ));
        }
        print_line(format!(
            "{} error(s), {} warning(s), {} info",
            report.summary.errors, report.summary.warnings, report.summary.info
        ));
    }
    // Exit 1 if any error-level findings (see docs/cli.md).
    if report.summary.errors > 0 {
        return Err(CliError::Other(format!(
            "{} error-level finding(s)",
            report.summary.errors
        )));
    }
    Ok(())
}

fn encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".to_string(),
            other => other
                .to_string()
                .bytes()
                .map(|b| format!("%{b:02X}"))
                .collect(),
        })
        .collect()
}
