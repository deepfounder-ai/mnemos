//! End-to-end CLI tests: boot the API in-process, then drive the real
//! `mnemos` binary against it over HTTP and assert stdout + exit codes.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use mnemos::config::Config;
use serde_json::Value;

struct Harness {
    base: String,
    bin: &'static str,
    _dir: tempfile::TempDir,
}

impl Harness {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::for_test(dir.path().to_path_buf());
        let state = mnemos::storage::init_pool(cfg).await.unwrap();
        let app = mnemos::api::router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Harness {
            base: format!("http://{addr}"),
            bin: env!("CARGO_BIN_EXE_mnemos"),
            _dir: dir,
        }
    }

    fn cmd(&self, key: Option<&str>) -> Command {
        let mut c = Command::new(self.bin);
        c.env("MNEMOS_API_URL", &self.base);
        c.env("MNEMOS_LOG", "error");
        // Avoid inheriting a stray key from the test environment.
        c.env_remove("MNEMOS_API_KEY");
        if let Some(k) = key {
            c.env("MNEMOS_API_KEY", k);
        }
        c
    }

    fn run(&self, key: Option<&str>, args: &[&str]) -> Output {
        self.cmd(key).args(args).output().expect("run mnemos")
    }

    fn run_stdin(&self, key: Option<&str>, args: &[&str], input: &str) -> Output {
        let mut child = self
            .cmd(key)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.stdin.take();
        child.wait_with_output().unwrap()
    }

    async fn register(&self, username: &str) -> String {
        let out = self.run(
            None,
            &[
                "user",
                "register",
                username,
                "--password",
                "hunter2hunter2",
                "--json",
            ],
        );
        assert!(out.status.success(), "register failed: {}", stderr(&out));
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        v["api_key"].as_str().unwrap().to_string()
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

const PAGE_DOC: &str = "---\n\
title: Kafka intro\n\
tags: [kafka, queue]\n\
created: 2026-06-09\n\
updated: 2026-06-09\n\
scope: global\n\
page_type: concept\n\
related: []\n\
---\n\
\n\
## Key points\n\
\n\
- Kafka is a distributed log.\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_cli_flow() {
    let h = Harness::start().await;
    let key = h.register("cliuser").await;

    // whoami
    let out = h.run(Some(&key), &["user", "whoami"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("cliuser"));

    // create page from stdin
    let out = h.run_stdin(
        Some(&key),
        &["pages", "create", "kafka-intro", "--from-stdin"],
        PAGE_DOC,
    );
    assert!(out.status.success(), "create failed: {}", stderr(&out));
    assert!(stdout(&out).contains("kafka-intro"));

    // list
    let out = h.run(Some(&key), &["pages", "list"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("kafka-intro"));

    // get (raw markdown)
    let out = h.run(Some(&key), &["pages", "get", "kafka-intro"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("title: Kafka intro"));

    // get --json
    let out = h.run(Some(&key), &["--json", "pages", "get", "kafka-intro"]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["slug"], "kafka-intro");

    // search
    let out = h.run(Some(&key), &["search", "distributed log"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("kafka-intro"));

    // index
    let out = h.run(Some(&key), &["index"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Kafka intro"));

    // lint — orphan page is info only, exit 0
    let out = h.run(Some(&key), &["lint"]);
    assert!(out.status.success(), "lint exit: {:?}", out.status.code());

    // delete
    let out = h.run(Some(&key), &["pages", "delete", "kafka-intro"]);
    assert!(out.status.success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_key_exits_one() {
    let h = Harness::start().await;
    let out = h.run(None, &["pages", "list"]);
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(1), "no key => usage error, exit 1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_password_exits_four() {
    let h = Harness::start().await;
    h.register("loginuser").await;
    let out = h.run(
        None,
        &["user", "login", "loginuser", "--password", "wrongpass"],
    );
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(4), "401 maps to exit 4");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completions_generate() {
    let h = Harness::start().await;
    let out = h.run(None, &["completions", "bash"]);
    assert!(out.status.success());
    let body = stdout(&out);
    assert!(body.contains("mnemos") && body.contains("complete"));
}
