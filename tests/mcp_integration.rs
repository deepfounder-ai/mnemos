//! End-to-end test of the MCP stdio server.
//!
//! Spins up the real REST API on an ephemeral port, registers a user,
//! and spawns the `mnemos mcp` subprocess against it. Sends JSON-RPC
//! requests over stdin and asserts the responses. The MCP server is
//! exercised as a black box — no library calls, just the wire.
//!
//! This mirrors the live smoke test in the verifier: a running REST
//! API + a separate MCP process communicating via stdio.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

struct TestServer {
    base: String,
    _dir: tempfile::TempDir,
    api_key: String,
    http: reqwest::Client,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = mnemos::config::Config::for_test(dir.path().to_path_buf());
        let state = mnemos::storage::init_pool(cfg).await.expect("init pool");
        let app = mnemos::api::router(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let base = format!("http://{addr}");
        let http = reqwest::Client::new();

        // Register a user and capture the bearer key for the MCP process.
        let resp = http
            .post(format!("{base}/api/v1/auth/register"))
            .json(&json!({ "username": "mcpagent", "password": "hunter2hunter2" }))
            .send()
            .await
            .expect("register request");
        assert_eq!(resp.status(), 201, "register should 201");
        let body: Value = resp.json().await.expect("register json");
        let api_key = body["api_key"].as_str().expect("api_key").to_string();

        TestServer {
            base,
            _dir: dir,
            api_key,
            http,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }
}

/// Drive a single JSON-RPC request against the running MCP server and
/// return the parsed response. The MCP process is spawned fresh per
/// call — it's a thin stdio loop and we're not optimising for cost.
async fn mcp_call(
    base_url: &str,
    api_key: &str,
    requests: Vec<Value>,
) -> (Vec<Value>, std::process::ExitStatus) {
    let bin = env!("CARGO_BIN_EXE_mnemos");
    let mut child = Command::new(bin)
        .arg("mcp")
        .env("MNEMOS_API_URL", base_url)
        .env("MNEMOS_API_KEY", api_key)
        .env("MNEMOS_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn mnemos mcp");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for r in &requests {
            writeln!(stdin, "{}", r).unwrap();
        }
    }
    child.stdin.take(); // close → EOF

    let stdout = child.stdout.take().expect("stdout");
    let mut responses: Vec<Value> = Vec::new();
    for line in BufReader::new(stdout).lines() {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        responses.push(serde_json::from_str(&line).expect("valid json-rpc line"));
    }
    let status = child.wait().expect("wait");
    (responses, status)
}

fn by_id(responses: &[Value], id: i64) -> Option<Value> {
    responses.iter().find(|r| r["id"] == id).cloned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `initialize` + `tools/list` round-trip. Asserts all 13 contract
/// tools are present.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_initialize_and_tools_list() {
    let s = TestServer::start().await;

    let (responses, status) = mcp_call(
        &s.base,
        &s.api_key,
        vec![
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": { "protocolVersion": "2024-11-05",
                                "capabilities": {},
                                "clientInfo": { "name": "test", "version": "0.1" } } }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        ],
    )
    .await;
    assert!(status.success(), "mcp exited non-zero");

    let init = by_id(&responses, 1).expect("initialize response");
    assert_eq!(init["result"]["serverInfo"]["name"], "mnemos");
    assert!(init["result"]["protocolVersion"].is_string());
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(init["result"]["capabilities"]["resources"].is_object());

    let tools = by_id(&responses, 2).expect("tools/list response");
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    let expected = [
        "list_pages",
        "get_page",
        "create_page",
        "update_page",
        "delete_page",
        "search_pages",
        "list_sources",
        "get_source",
        "add_source_url",
        "upload_source",
        "get_index",
        "get_log",
        "lint",
    ];
    let missing: Vec<&&str> = expected
        .iter()
        .filter(|n| !names.contains(n))
        .collect();
    assert!(
        missing.is_empty(),
        "missing tools: {:?}; got: {:?}",
        missing,
        names
    );
    assert_eq!(names.len(), 13, "expected exactly 13 tools");
}

/// End-to-end tool call: `create_page` → `get_page` → `list_pages`.
/// All three land in the REST API store, not in any in-process cache.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_create_get_list_page() {
    let s = TestServer::start().await;

    let (responses, status) = mcp_call(
        &s.base,
        &s.api_key,
        vec![
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {} }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                    "params": { "name": "create_page",
                                "arguments": { "title": "Hello", "body": "world",
                                               "frontmatter": { "tags": ["greeting"],
                                                                "page_type": "concept" } } } }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                    "params": { "name": "get_page",
                                "arguments": { "slug": "hello" } } }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                    "params": { "name": "list_pages",
                                "arguments": { "limit": 10 } } }),
        ],
    )
    .await;
    assert!(status.success());

    // create_page returned a successful JSON payload.
    let create = by_id(&responses, 2).expect("create_page response");
    assert_eq!(create["result"]["isError"], false);
    let create_body: Value =
        serde_json::from_str(create["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(create_body["slug"], "hello");

    // get_page round-trips the same slug.
    let get = by_id(&responses, 3).expect("get_page response");
    assert_eq!(get["result"]["isError"], false);
    let get_body: Value =
        serde_json::from_str(get["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(get_body["slug"], "hello");
    assert_eq!(get_body["body"], "world");
    assert_eq!(get_body["frontmatter"]["tags"][0], "greeting");

    // list_pages sees the new page.
    let list = by_id(&responses, 4).expect("list_pages response");
    let list_body: Value =
        serde_json::from_str(list["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let slugs: Vec<&str> = list_body["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap())
        .collect();
    assert!(slugs.contains(&"hello"), "expected 'hello' in list, got {:?}", slugs);
}

/// `mnemos://index` resource is readable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_resource_index_is_readable() {
    let s = TestServer::start().await;

    // Create a page so index.md has something to list.
    let _ = s
        .http
        .post(s.url("/api/v1/pages"))
        .bearer_auth(&s.api_key)
        .json(&json!({ "title": "ResTest", "body": "x" }))
        .send()
        .await
        .unwrap();

    let (responses, _status) = mcp_call(
        &s.base,
        &s.api_key,
        vec![
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {} }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/read",
                    "params": { "uri": "mnemos://index" } }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/read",
                    "params": { "uri": "mnemos://page/ResTest" } }),
        ],
    )
    .await;

    // resources/list advertises the four contract URIs.
    let list = by_id(&responses, 2).expect("resources/list response");
    let uris: Vec<&str> = list["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"mnemos://index"));
    assert!(uris.contains(&"mnemos://log"));
    assert!(uris.contains(&"mnemos://page/{slug}"));
    assert!(uris.contains(&"mnemos://source/{id}"));

    // mnemos://index returns markdown containing the page we just made.
    let index = by_id(&responses, 3).expect("index read");
    let text = index["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("ResTest"),
        "index.md should contain ResTest; got: {}",
        text
    );
    let mime = index["result"]["contents"][0]["mimeType"].as_str().unwrap();
    assert_eq!(mime, "text/markdown");

    // mnemos://page/{slug} returns the rendered page markdown.
    let page = by_id(&responses, 4).expect("page read");
    let ptext = page["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(ptext.contains("title: ResTest") || ptext.contains("ResTest"));
}

/// Unknown methods return JSON-RPC -32601.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_unknown_method_returns_jsonrpc_error() {
    let s = TestServer::start().await;
    let (responses, _status) = mcp_call(
        &s.base,
        &s.api_key,
        vec![json!({ "jsonrpc": "2.0", "id": 9, "method": "does/not/exist" })],
    )
    .await;
    let last = responses.last().expect("at least one response");
    assert_eq!(last["id"], 9);
    assert_eq!(last["error"]["code"], -32601);
}

/// Tool error (unknown tool name) is reported as a successful
/// JSON-RPC response with `isError: true`, not a transport error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_unknown_tool_is_tool_error_not_rpc_error() {
    let s = TestServer::start().await;
    let (responses, _status) = mcp_call(
        &s.base,
        &s.api_key,
        vec![json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                     "params": { "name": "no_such_tool", "arguments": {} } })],
    )
    .await;
    let resp = by_id(&responses, 1).expect("response");
    assert!(resp["error"].is_null());
    assert_eq!(resp["result"]["isError"], true);
    let msg = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(msg.contains("no_such_tool"));
}

/// `initialize` then EOF → process exits 0 after producing the one
/// response.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_exits_cleanly_on_eof() {
    let s = TestServer::start().await;

    let bin = env!("CARGO_BIN_EXE_mnemos");
    let mut child = Command::new(bin)
        .arg("mcp")
        .env("MNEMOS_API_URL", &s.base)
        .env("MNEMOS_API_KEY", &s.api_key)
        .env("MNEMOS_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} })
        )
        .unwrap();
    }
    child.stdin.take();

    let stdout = child.stdout.take().unwrap();
    let mut responses: Vec<Value> = Vec::new();
    for line in BufReader::new(stdout).lines() {
        let line = line.unwrap();
        if !line.trim().is_empty() {
            responses.push(serde_json::from_str(&line).unwrap());
        }
    }
    let status = child.wait().unwrap();
    assert!(status.success(), "MCP server should exit 0 on EOF");
    assert_eq!(responses.len(), 1);
    assert!(responses[0]["result"]["serverInfo"].is_object());
}

/// Auth failure: invalid API key gets a 401 from the REST API, which
/// the MCP server surfaces as a tool-level `isError: true`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_bad_api_key_surfaces_as_tool_error() {
    let s = TestServer::start().await;
    let (responses, _status) = mcp_call(
        &s.base,
        "mnemo_definitely_not_a_real_key",
        vec![json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                     "params": { "name": "list_pages", "arguments": {} } })],
    )
    .await;
    let resp = by_id(&responses, 1).expect("response");
    assert!(resp["error"].is_null(), "expected transport OK, got {:?}", resp);
    assert_eq!(resp["result"]["isError"], true);
    let msg = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        msg.contains("401") || msg.to_lowercase().contains("unauthorized") || msg.contains("upstream"),
        "tool error should mention the upstream 401, got: {msg}"
    );
}
