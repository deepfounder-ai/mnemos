//! MCP server (stdio transport).
//!
//! The server speaks newline-delimited JSON-RPC 2.0 on stdin/stdout and
//! logs to stderr so the protocol stream stays clean. It runs **in
//! process** against the same SQLite + filesystem store the REST API uses —
//! there is no upstream HTTP hop. Authentication is a single `MNEMOS_API_KEY`
//! resolved once at startup; every tool/resource call is scoped to the
//! resulting user.
//!
//! Surface (see `docs/mcp.md`):
//!
//! - `initialize` / `notifications/initialized` / `ping`
//! - `tools/list`, `tools/call`
//! - `resources/list`, `resources/read`

pub mod resources;
pub mod tools;

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::config::Config;
use crate::error::AppError;
use crate::storage::{init_pool, AppState};

/// Protocol version we advertise in `initialize`.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Shared, request-independent server context. Cheap to clone behind an
/// `Arc`; holds the live [`AppState`].
#[derive(Debug)]
pub struct McpContext {
    pub state: AppState,
}

impl McpContext {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

/// An authenticated principal for a single MCP session. Tool and resource
/// handlers receive this and use `ctx.state` for service calls, scoped by
/// `user_id`.
#[derive(Debug, Clone)]
pub struct AuthedContext {
    pub ctx: Arc<McpContext>,
    pub user_id: String,
    pub api_key_id: String,
}

/// Run the MCP server on stdio until EOF on stdin.
///
/// Resolves `MNEMOS_API_KEY` against the configured store, then serves
/// JSON-RPC requests line by line.
pub async fn run_stdio() -> anyhow::Result<()> {
    let config = Config::from_env().map_err(|e| anyhow::anyhow!("load config: {e}"))?;
    // The MCP process must not pollute stdout with anything but JSON-RPC.
    // The data dir still needs to exist so we can open / migrate the DB.
    std::fs::create_dir_all(&config.data_dir)
        .map_err(|e| anyhow::anyhow!("create data dir {}: {e}", config.data_dir.display()))?;

    let api_key = std::env::var("MNEMOS_API_KEY")
        .map_err(|_| anyhow::anyhow!("MNEMOS_API_KEY is required for the MCP server"))?;

    let state: AppState = init_pool(config).await?;
    let user_service = crate::auth::UserService::new(state.clone());
    let resolved = user_service
        .resolve_api_key(api_key.trim())
        .await?
        .ok_or_else(|| anyhow::anyhow!("MNEMOS_API_KEY did not resolve to a known user"))?;

    let authed = AuthedContext {
        ctx: Arc::new(McpContext::new(state)),
        user_id: resolved.user_id,
        api_key_id: resolved.id,
    };

    tracing::info!(user_id = %authed.user_id, "mnemos mcp server ready on stdio");
    serve(authed).await
}

/// The JSON-RPC read/dispatch/write loop.
async fn serve(authed: AuthedContext) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = reader.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                // Parse error — id unknown, so respond with null id per spec.
                let resp = error_response(Value::Null, -32700, &format!("parse error: {e}"));
                write_message(&mut stdout, &resp).await?;
                continue;
            }
        };

        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        // Notifications have no `id` and expect no response.
        let is_notification = id.is_none();

        let outcome = dispatch(method, params, &authed).await;

        if is_notification {
            // We still log dispatch errors, but never write a response.
            if let Err((code, msg)) = outcome {
                tracing::warn!(method, code, %msg, "notification handler error");
            }
            continue;
        }

        let id = id.unwrap_or(Value::Null);
        let resp = match outcome {
            Ok(result) => success_response(id, result),
            Err((code, msg)) => error_response(id, code, &msg),
        };
        write_message(&mut stdout, &resp).await?;
    }

    Ok(())
}

/// Dispatch a single method. Returns the `result` value on success, or a
/// `(code, message)` JSON-RPC error pair.
async fn dispatch(
    method: &str,
    params: Value,
    authed: &AuthedContext,
) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(initialize_result()),
        // Lifecycle notifications — accept and ignore.
        "notifications/initialized" | "initialized" => Ok(Value::Null),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::list_tools() })),
        "tools/call" => tools_call(params, authed).await,
        "resources/list" => Ok(json!({ "resources": resources::list_resources() })),
        "resources/read" => resources_read(params, authed).await,
        other => Err((-32601, format!("method not found: {other}"))),
    }
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "listChanged": false, "subscribe": false }
        },
        "serverInfo": {
            "name": "mnemos",
            "version": crate::VERSION
        }
    })
}

async fn tools_call(params: Value, authed: &AuthedContext) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| (-32602, "tools/call requires a `name`".to_string()))?
        .to_string();
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

    match tools::call_tool(&name, arguments, authed).await {
        Ok(content) => Ok(json!({ "content": content, "isError": false })),
        // Tool-level errors are reported as a successful response with
        // `isError: true`, per the MCP convention, so the agent sees the
        // message instead of a transport failure.
        Err(e) => Ok(json!({
            "content": [ tools::TextContent::text(e.to_string()) ],
            "isError": true
        })),
    }
}

async fn resources_read(params: Value, authed: &AuthedContext) -> Result<Value, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(|u| u.as_str())
        .ok_or_else(|| (-32602, "resources/read requires a `uri`".to_string()))?
        .to_string();

    match resources::read(&uri, authed).await {
        Ok(contents) => Ok(json!({ "contents": contents })),
        Err(e) => Err(app_error_to_rpc(&e)),
    }
}

/// Map an [`AppError`] to a JSON-RPC error code. We keep this coarse: -32602
/// for caller mistakes, -32603 for everything else.
fn app_error_to_rpc(err: &AppError) -> (i64, String) {
    let code = match err {
        AppError::NotFound(_) | AppError::Validation(_) | AppError::BadRequest(_) => -32602,
        _ => -32603,
    };
    (code, err.to_string())
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

async fn write_message(out: &mut tokio::io::Stdout, message: &Value) -> anyhow::Result<()> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}
