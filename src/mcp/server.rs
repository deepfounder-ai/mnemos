//! JSON-RPC 2.0 transport for the MCP server.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::mcp::resources;
use crate::mcp::tools;
use crate::mcp::{McpError, McpRestClient, PROTOCOL_VERSION, SERVER_NAME};

pub async fn serve(client: McpRestClient) -> anyhow::Result<()> {
    let client = Arc::new(client);
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
                let resp = error_response(Value::Null, -32700, &format!("parse error: {e}"));
                write_message(&mut stdout, &resp).await?;
                continue;
            }
        };

        if let Some(resp) = handle_message(&request, client.as_ref()).await {
            write_message(&mut stdout, &resp).await?;
        }
    }

    Ok(())
}

/// Handle a single JSON-RPC message. Returns `Some(response)` for requests and
/// `None` for notifications (messages without an `id`). Shared by the stdio
/// loop and the HTTP (`/mcp`) transport so both speak an identical protocol.
pub async fn handle_message(request: &Value, client: &McpRestClient) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let params = request.get("params").cloned().unwrap_or(Value::Null);

    let outcome = dispatch(&method, params, client).await;

    if id.is_none() {
        if let Err((code, msg)) = outcome {
            tracing::warn!(method = %method, code, %msg, "notification handler error");
        }
        return None;
    }

    let id = id.unwrap_or(Value::Null);
    Some(match outcome {
        Ok(result) => success_response(id, result),
        Err((code, msg)) => error_response(id, code, &msg),
    })
}

async fn dispatch(
    method: &str,
    params: Value,
    client: &McpRestClient,
) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(initialize_result()),
        "notifications/initialized" | "initialized" => Ok(Value::Null),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::list_tools() })),
        "tools/call" => tools_call(params, client).await,
        "resources/list" => Ok(json!({ "resources": resources::list_resources() })),
        "resources/list_templates" => Ok(json!({ "resourceTemplates": [] })),
        "resources/read" => resources_read(params, client).await,
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
            "name": SERVER_NAME,
            "version": crate::VERSION
        }
    })
}

async fn tools_call(
    params: Value,
    client: &McpRestClient,
) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| (-32602, "tools/call requires a string `name`".to_string()))?
        .to_string();
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

    match tools::call_tool(&name, arguments, client).await {
        Ok(content) => Ok(json!({ "content": content, "isError": false })),
        Err(e) => {
            let msg = format_tool_error(&e);
            let text = crate::mcp::tools::TextContent::text(msg);
            Ok(json!({
                "content": [ text ],
                "isError": true
            }))
        }
    }
}

async fn resources_read(
    params: Value,
    client: &McpRestClient,
) -> Result<Value, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(|u| u.as_str())
        .ok_or_else(|| (-32602, "resources/read requires a string `uri`".to_string()))?
        .to_string();

    match resources::read(&uri, client).await {
        Ok(contents) => Ok(json!({ "contents": contents })),
        Err(e) => Err(mcp_error_to_rpc(&e)),
    }
}

fn format_tool_error(e: &McpError) -> String {
    match e {
        McpError::InvalidArgument(m) => m.clone(),
        McpError::Unreachable(m) => format!("upstream unreachable: {m}"),
        McpError::Api { status, code, message } => format!("upstream {status} ({code}): {message}"),
        McpError::UnexpectedStatus { status, body } => format!("upstream HTTP {status}: {body}"),
    }
}

fn mcp_error_to_rpc(e: &McpError) -> (i64, String) {
    let code = match e {
        McpError::InvalidArgument(_) => -32602,
        _ => -32603,
    };
    (code, format_tool_error(e))
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

async fn write_message(out: &mut tokio::io::Stdout, message: &Value) -> anyhow::Result<()> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}
