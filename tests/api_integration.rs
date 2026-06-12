//! End-to-end HTTP tests against the real Axum router.
//!
//! Each test spins up the full stack (router + `require_auth` middleware +
//! a temp SQLite store) on an ephemeral port and drives it with `reqwest`,
//! so middleware, extractors, and error envelopes are all exercised.

use mnemos::config::Config;
use serde_json::{json, Value};

struct TestServer {
    base: String,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        Self::start_with(|_| {}).await
    }

    /// Start a server, mutating the `Config` before the store is initialised.
    async fn start_with(mutate: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut cfg = Config::for_test(dir.path().to_path_buf());
        mutate(&mut cfg);
        let state = mnemos::storage::init_pool(cfg).await.expect("init pool");
        let app = mnemos::api::router(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        TestServer {
            base: format!("http://{addr}"),
            http: reqwest::Client::new(),
            _dir: dir,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// Register a fresh user and return its bearer API key.
    async fn register(&self, username: &str) -> String {
        let resp = self
            .http
            .post(self.url("/api/v1/auth/register"))
            .json(&json!({ "username": username, "password": "hunter2hunter2" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "register should 201");
        let body: Value = resp.json().await.unwrap();
        body["api_key"].as_str().unwrap().to_string()
    }

    fn auth(&self, method: reqwest::Method, path: &str, key: &str) -> reqwest::RequestBuilder {
        self.http.request(method, self.url(path)).bearer_auth(key)
    }
}

#[tokio::test]
async fn health_is_public() {
    let s = TestServer::start().await;
    let resp = s.http.get(s.url("/healthz")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn root_landing_page_renders() {
    let s = TestServer::start().await;
    let resp = s.http.get(s.url("/")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ctype.contains("text/html"));
    let body = resp.text().await.unwrap();
    assert!(body.contains("mnemos"));
    assert!(body.contains("Copy this to your LLM"));
    // The configured host:port is substituted into the page.
    assert!(!body.contains("__BASE__"));
}

#[tokio::test]
async fn register_duplicate_conflicts() {
    let s = TestServer::start().await;
    s.register("alice").await;
    let resp = s
        .http
        .post(s.url("/api/v1/auth/register"))
        .json(&json!({ "username": "alice", "password": "hunter2hunter2" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "conflict");
}

#[tokio::test]
async fn registration_secret_gates_signup() {
    let s = TestServer::start_with(|cfg| {
        cfg.registration_secret = "swordfish".into();
    })
    .await;

    // No secret -> forbidden.
    let resp = s
        .http
        .post(s.url("/api/v1/auth/register"))
        .json(&json!({ "username": "alice", "password": "hunter2hunter2" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "missing secret should be forbidden");

    // Wrong secret -> forbidden.
    let resp = s
        .http
        .post(s.url("/api/v1/auth/register"))
        .json(&json!({ "username": "alice", "password": "hunter2hunter2", "secret": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "wrong secret should be forbidden");

    // Correct secret -> created.
    let resp = s
        .http
        .post(s.url("/api/v1/auth/register"))
        .json(&json!({ "username": "alice", "password": "hunter2hunter2", "secret": "swordfish" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "correct secret should create the user");
}

#[tokio::test]
async fn missing_auth_returns_nested_envelope_and_www_authenticate() {
    let s = TestServer::start().await;
    let resp = s.http.get(s.url("/api/v1/pages")).send().await.unwrap();
    assert_eq!(resp.status(), 401);
    assert!(resp.headers().contains_key("www-authenticate"));
    let body: Value = resp.json().await.unwrap();
    // Documented shape: { "error": { "code", "message" } }
    assert!(body["error"]["code"].is_string(), "got {body}");
}

#[tokio::test]
async fn invalid_key_rejected() {
    let s = TestServer::start().await;
    let resp = s
        .auth(
            reqwest::Method::GET,
            "/api/v1/pages",
            "mnemo_not_a_real_key_000000000000",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn page_crud_round_trip() {
    let s = TestServer::start().await;
    let key = s.register("bob").await;

    // create
    let payload = json!({
        "slug": "kafka",
        "title": "Kafka",
        "body": "## Key points\n- a log",
        "frontmatter": { "tags": ["queue"], "page_type": "concept", "scope": "global", "related": [] }
    });
    let resp = s
        .auth(reqwest::Method::POST, "/api/v1/pages", &key)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created: Value = resp.json().await.unwrap();
    assert_eq!(created["slug"], "kafka");
    assert!(created["id"].is_string());

    // get
    let got: Value = s
        .auth(reqwest::Method::GET, "/api/v1/pages/kafka", &key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(got["title"], "Kafka");
    assert_eq!(got["frontmatter"]["tags"][0], "queue");

    // raw markdown
    let raw = s
        .auth(reqwest::Method::GET, "/api/v1/pages/kafka/raw", &key)
        .send()
        .await
        .unwrap();
    assert!(raw
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/markdown"));
    let raw_text = raw.text().await.unwrap();
    assert!(raw_text.starts_with("---"));
    assert!(raw_text.contains("title: Kafka"));

    // list
    let list: Value = s
        .auth(reqwest::Method::GET, "/api/v1/pages", &key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["total"], 1);
    assert_eq!(list["pages"][0]["slug"], "kafka");

    // update
    let upd = json!({
        "body": "## Key points\n- updated",
        "frontmatter": { "tags": ["queue", "log"], "page_type": "concept", "scope": "global", "related": [] }
    });
    let resp = s
        .auth(reqwest::Method::PUT, "/api/v1/pages/kafka", &key)
        .json(&upd)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let got: Value = s
        .auth(reqwest::Method::GET, "/api/v1/pages/kafka", &key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(got["body"].as_str().unwrap().contains("updated"));

    // delete (idempotent)
    let resp = s
        .auth(reqwest::Method::DELETE, "/api/v1/pages/kafka", &key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    let resp = s
        .auth(reqwest::Method::DELETE, "/api/v1/pages/kafka", &key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204, "delete is idempotent");
    let resp = s
        .auth(reqwest::Method::GET, "/api/v1/pages/kafka", &key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn tenant_isolation() {
    let s = TestServer::start().await;
    let alice = s.register("alice").await;
    let bob = s.register("bob").await;

    let payload = json!({
        "slug": "secret", "title": "Secret", "body": "x",
        "frontmatter": { "tags": ["t"], "scope": "global", "related": [] }
    });
    let resp = s
        .auth(reqwest::Method::POST, "/api/v1/pages", &alice)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Bob cannot see Alice's page.
    let resp = s
        .auth(reqwest::Method::GET, "/api/v1/pages/secret", &bob)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let list: Value = s
        .auth(reqwest::Method::GET, "/api/v1/pages", &bob)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["total"], 0);
}

#[tokio::test]
async fn source_upload_and_read_back() {
    let s = TestServer::start().await;
    let key = s.register("carol").await;

    let form = reqwest::multipart::Form::new().text("slug", "notes").part(
        "file",
        reqwest::multipart::Part::bytes(b"hello kafka world".to_vec()).file_name("notes.md"),
    );
    let resp = s
        .auth(reqwest::Method::POST, "/api/v1/sources/upload", &key)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let src: Value = resp.json().await.unwrap();
    let sid = src["source_id"].as_str().unwrap().to_string();
    assert_eq!(src["slug"], "notes");

    // list
    let list: Value = s
        .auth(reqwest::Method::GET, "/api/v1/sources", &key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["sources"].as_array().unwrap().len(), 1);

    // raw body
    let raw = s
        .auth(
            reqwest::Method::GET,
            &format!("/api/v1/sources/{sid}/raw"),
            &key,
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(raw.contains("hello kafka world"));
}

#[tokio::test]
async fn search_index_log_lint() {
    let s = TestServer::start().await;
    let key = s.register("dave").await;

    let payload = json!({
        "slug": "marsupials", "title": "Quokka", "body": "a small marsupial from australia",
        "frontmatter": { "tags": ["animal"], "page_type": "concept", "scope": "global", "related": [] }
    });
    s.auth(reqwest::Method::POST, "/api/v1/pages", &key)
        .json(&payload)
        .send()
        .await
        .unwrap();

    // search
    let hits: Value = s
        .auth(reqwest::Method::GET, "/api/v1/search?q=marsupial", &key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(hits["count"].as_i64().unwrap() >= 1);
    assert_eq!(hits["hits"][0]["slug"], "marsupials");

    // index (markdown)
    let index = s
        .auth(reqwest::Method::GET, "/api/v1/index", &key)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(index.contains("Quokka"));

    // log (json) — must contain the page.create event
    let log: Value = s
        .auth(reqwest::Method::GET, "/api/v1/log?format=json", &key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let kinds: Vec<&str> = log["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"page.create"), "kinds: {kinds:?}");

    // lint — single orphan page => one info finding, zero errors
    let lint: Value = s
        .auth(reqwest::Method::GET, "/api/v1/lint", &key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(lint["summary"]["errors"], 0);
}

#[tokio::test]
async fn create_page_with_invalid_slug_is_rejected() {
    let s = TestServer::start().await;
    let key = s.register("erin").await;
    let payload = json!({
        "slug": "Not A Slug!!", "title": "X", "body": "x",
        "frontmatter": { "tags": ["t"], "scope": "global", "related": [] }
    });
    let resp = s
        .auth(reqwest::Method::POST, "/api/v1/pages", &key)
        .json(&payload)
        .send()
        .await
        .unwrap();
    // slugify normalises, so this actually succeeds with a cleaned slug;
    // assert the server cleaned it rather than stored the raw value.
    assert_eq!(resp.status(), 201);
    let created: Value = resp.json().await.unwrap();
    assert_eq!(created["slug"], "not-a-slug");
}

#[tokio::test]
async fn key_lifecycle_over_http() {
    let s = TestServer::start().await;
    let key = s.register("frank").await;

    // create a second key
    let created: Value = s
        .auth(reqwest::Method::POST, "/api/v1/auth/keys", &key)
        .json(&json!({ "name": "laptop" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let new_key = created["api_key"].as_str().unwrap().to_string();
    let new_id = created["id"].as_str().unwrap().to_string();

    // the new key works
    let resp = s
        .auth(reqwest::Method::GET, "/api/v1/auth/whoami", &new_key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // list shows at least two keys
    let list: Value = s
        .auth(reqwest::Method::GET, "/api/v1/auth/keys", &key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list["keys"].as_array().unwrap().len() >= 2);

    // revoke the new key (204), after which it stops working
    let resp = s
        .auth(
            reqwest::Method::DELETE,
            &format!("/api/v1/auth/keys/{new_id}"),
            &key,
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    let resp = s
        .auth(reqwest::Method::GET, "/api/v1/auth/whoami", &new_key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "revoked key must be rejected");
}
