//! End-to-end test of TypeSafe/Jev enrichment against a mock Jev server.
//!
//! The mock answers deterministically based on the page being judged (the
//! `state.title` it receives), so background enrichment passes triggered by
//! page creation are either idempotent or no-ops and the final state is fixed.

use axum::{routing::post, Json, Router};
use mnemos::config::Config;
use serde_json::{json, Value};

/// Mock Jev: high confidence only when judging the page titled "Alpha".
async fn mock_jev(Json(req): Json<Value>) -> Json<Value> {
    let is_alpha = req["state"]["title"] == "Alpha";
    let mut answers = serde_json::Map::new();
    if let Some(qs) = req["questions"].as_object() {
        for id in qs.keys() {
            let ans = if id == "__page_type" {
                json!({ "type": "choice", "choice": if is_alpha { "recipe" } else { "none" },
                        "confidence": 0.9, "probabilities": {} })
            } else if let Some(tag) = id.strip_prefix("tag::") {
                json!({ "type": "noul", "noul": if is_alpha && tag == "secrets" { 0.95 } else { 0.05 } })
            } else if let Some(slug) = id.strip_prefix("rel::") {
                json!({ "type": "noul", "noul": if is_alpha && slug == "beta" { 0.92 } else { 0.05 } })
            } else {
                json!({ "type": "noul", "noul": 0.0 })
            };
            answers.insert(id.clone(), ans);
        }
    }
    Json(json!({ "model": "jev-mock", "answers": answers }))
}

async fn spawn(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn enrich_sets_type_tags_and_bidirectional_related() {
    let jev = spawn(Router::new().route("/v1/systemone", post(mock_jev))).await;

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::for_test(dir.path().to_path_buf());
    cfg.enrich.api_key = "test-key".into();
    cfg.enrich.url = format!("{jev}/v1/systemone");
    let state = mnemos::storage::init_pool(cfg).await.unwrap();
    let base = spawn(mnemos::api::router(state)).await;
    let http = reqwest::Client::new();

    let reg: Value = http
        .post(format!("{base}/api/v1/auth/register"))
        .json(&json!({ "username": "enricher", "password": "hunter2hunter2" }))
        .send().await.unwrap().json().await.unwrap();
    let key = reg["api_key"].as_str().unwrap().to_string();

    let create = |slug: &'static str, title: &'static str, tags: Value, page_type: Option<&str>| {
        let mut fm = json!({ "tags": tags, "scope": "global", "related": [] });
        if let Some(pt) = page_type {
            fm["page_type"] = json!(pt);
        }
        http.post(format!("{base}/api/v1/pages"))
            .bearer_auth(&key)
            .json(&json!({ "slug": slug, "title": title, "body": "body text", "frontmatter": fm }))
            .send()
    };
    assert_eq!(create("alpha", "Alpha", json!(["supabase", "cron"]), None).await.unwrap().status(), 201);
    assert_eq!(create("beta", "Beta", json!(["supabase", "secrets"]), Some("concept")).await.unwrap().status(), 201);
    assert_eq!(create("gamma", "Gamma", json!(["supabase"]), Some("concept")).await.unwrap().status(), 201);

    // Synchronous enrichment of alpha.
    let resp = http
        .post(format!("{base}/api/v1/enrich?slug=alpha"))
        .bearer_auth(&key)
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let report: Value = resp.json().await.unwrap();
    assert_eq!(report["processed"], 1);

    let get = |slug: &'static str| {
        let (http, base, key) = (http.clone(), base.clone(), key.clone());
        async move {
            http.get(format!("{base}/api/v1/pages/{slug}"))
                .bearer_auth(&key)
                .send().await.unwrap().json::<Value>().await.unwrap()
        }
    };

    let alpha = get("alpha").await;
    assert_eq!(alpha["frontmatter"]["page_type"], "recipe", "page_type filled");
    let tags: Vec<&str> = alpha["frontmatter"]["tags"].as_array().unwrap()
        .iter().filter_map(|t| t.as_str()).collect();
    assert!(tags.contains(&"secrets"), "vocabulary tag added: {tags:?}");
    let rel: Vec<&str> = alpha["related"].as_array().unwrap()
        .iter().filter_map(|t| t.as_str()).collect();
    assert_eq!(rel, vec!["beta"], "only the high-confidence edge is added");

    let beta = get("beta").await;
    let beta_rel: Vec<&str> = beta["related"].as_array().unwrap()
        .iter().filter_map(|t| t.as_str()).collect();
    assert!(beta_rel.contains(&"alpha"), "reverse edge added: {beta_rel:?}");
    assert_eq!(beta["frontmatter"]["page_type"], "concept", "existing type not overwritten");

    let gamma = get("gamma").await;
    assert!(gamma["related"].as_array().unwrap().is_empty(), "low-confidence pair not linked");
}

#[tokio::test]
async fn enrich_endpoint_conflicts_when_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let state = mnemos::storage::init_pool(Config::for_test(dir.path().to_path_buf())).await.unwrap();
    let base = spawn(mnemos::api::router(state)).await;
    let http = reqwest::Client::new();
    let reg: Value = http
        .post(format!("{base}/api/v1/auth/register"))
        .json(&json!({ "username": "noenrich", "password": "hunter2hunter2" }))
        .send().await.unwrap().json().await.unwrap();
    let resp = http
        .post(format!("{base}/api/v1/enrich"))
        .bearer_auth(reg["api_key"].as_str().unwrap())
        .send().await.unwrap();
    assert_eq!(resp.status(), 409);
}
