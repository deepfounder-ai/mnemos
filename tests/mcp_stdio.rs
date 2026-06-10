//! Black-box test of the MCP stdio server: spawn the real `mnemos mcp`
//! binary, speak JSON-RPC over its stdin/stdout, and assert the responses.
//!
//! The store is seeded in-process (a user + API key) before the binary is
//! launched against the same data directory.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

#[tokio::test]
async fn mcp_stdio_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Seed a user + key in the same store the binary will open.
    let cfg = mnemos::config::Config::for_test(dir.path().to_path_buf());
    let key = {
        let state = mnemos::storage::init_pool(cfg).await.expect("init pool");
        let svc = mnemos::auth::UserService::new(state.clone());
        let (_user, key) = svc
            .register("agent", "hunter2hunter2")
            .await
            .expect("register");
        state.db.close().await; // release the WAL writer before spawning
        key.plaintext
    };

    let bin = env!("CARGO_BIN_EXE_mnemos");
    let mut child = Command::new(bin)
        .arg("mcp")
        .env("MNEMOS_DATA_DIR", dir.path())
        .env("MNEMOS_API_KEY", &key)
        .env("MNEMOS_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn mnemos mcp");

    // Drive the protocol, then close stdin so the server loop hits EOF.
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let requests = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                    "params": { "name": "create_page",
                                "arguments": { "title": "Hello", "body": "world" } } }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/read",
                    "params": { "uri": "mnemos://page/hello" } }),
        ];
        for r in requests {
            writeln!(stdin, "{r}").unwrap();
        }
    }
    child.stdin.take(); // drop -> EOF

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
    assert!(status.success(), "mcp server exited non-zero");

    let by_id = |id: i64| responses.iter().find(|r| r["id"] == id).cloned();

    // initialize
    let init = by_id(1).expect("initialize response");
    assert_eq!(init["result"]["serverInfo"]["name"], "mnemos");
    assert!(init["result"]["protocolVersion"].is_string());

    // notifications/initialized must NOT produce a response
    assert!(
        responses
            .iter()
            .all(|r| r["id"] != Value::Null || r.get("error").is_some()),
        "notification produced a spurious response"
    );

    // tools/list — 13 tools
    let tools = by_id(2).expect("tools/list response");
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 13);

    // tools/call create_page
    let call = by_id(3).expect("tools/call response");
    assert_eq!(call["result"]["isError"], false);
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    let page: Value = serde_json::from_str(text).unwrap();
    assert_eq!(page["slug"], "hello");

    // resources/read the page we just created
    let read = by_id(4).expect("resources/read response");
    let body = read["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(body.contains("title: Hello"));
    assert!(body.contains("world"));
}

#[tokio::test]
async fn mcp_unknown_method_returns_jsonrpc_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = mnemos::config::Config::for_test(dir.path().to_path_buf());
    let key = {
        let state = mnemos::storage::init_pool(cfg).await.expect("init pool");
        let svc = mnemos::auth::UserService::new(state.clone());
        let (_u, key) = svc
            .register("agent", "hunter2hunter2")
            .await
            .expect("register");
        state.db.close().await;
        key.plaintext
    };

    let bin = env!("CARGO_BIN_EXE_mnemos");
    let mut child = Command::new(bin)
        .arg("mcp")
        .env("MNEMOS_DATA_DIR", dir.path())
        .env("MNEMOS_API_KEY", &key)
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
            json!({ "jsonrpc": "2.0", "id": 9, "method": "does/not/exist" })
        )
        .unwrap();
    }
    child.stdin.take();

    let stdout = child.stdout.take().unwrap();
    let mut last = Value::Null;
    for line in BufReader::new(stdout).lines() {
        let line = line.unwrap();
        if !line.trim().is_empty() {
            last = serde_json::from_str(&line).unwrap();
        }
    }
    child.wait().unwrap();

    assert_eq!(last["id"], 9);
    assert_eq!(last["error"]["code"], -32601, "method not found code");
}
