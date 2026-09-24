//! Minimal client for the TypeSafe "System One" (Jev) decision model.
//!
//! Jev is a *decision* model: it never emits text. Given a `state` and a set of
//! named `questions`, it returns a typed answer per question (a `choice`, a
//! `score`, or a `noul` probability). We use it for knowledge enrichment —
//! classifying `page_type`, matching tags against a fixed vocabulary, and
//! deciding whether two pages should be cross-linked.
//!
//! See `docs/enrichment.md` and the Jev API reference. This is NOT an
//! OpenAI-compatible endpoint; it is a plain JSON POST to `/v1/systemone`.

use std::time::Duration;

use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::EnrichConfig;

/// Errors from a Jev call.
#[derive(Debug, thiserror::Error)]
pub enum TypeSafeError {
    #[error("typesafe unreachable: {0}")]
    Unreachable(String),
    #[error("typesafe HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("typesafe decode: {0}")]
    Decode(String),
}

/// A thin HTTP client bound to one TypeSafe endpoint + key + model.
#[derive(Debug, Clone)]
pub struct TypeSafeClient {
    http: HttpClient,
    url: String,
    api_key: String,
    model: String,
}

/// The `answers` map from a decision response, plus typed accessors.
#[derive(Debug, Clone, Deserialize)]
pub struct Answers {
    #[serde(default)]
    answers: std::collections::BTreeMap<String, Value>,
}

impl Answers {
    /// `noul` probability (0..1) for a question id, if present.
    pub fn noul(&self, id: &str) -> Option<f64> {
        self.answers.get(id)?.get("noul")?.as_f64()
    }

    /// `choice` (winning option, confidence) for a question id, if present.
    pub fn choice(&self, id: &str) -> Option<(String, f64)> {
        let a = self.answers.get(id)?;
        let choice = a.get("choice")?.as_str()?.to_string();
        let confidence = a.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0);
        Some((choice, confidence))
    }
}

impl TypeSafeClient {
    /// Build a client from enrichment config. Returns `None` when the feature
    /// is disabled (no API key) or the HTTP client cannot be built.
    pub fn from_config(cfg: &EnrichConfig) -> Option<Self> {
        if !cfg.enabled() {
            return None;
        }
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("mnemos/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok()?;
        Some(Self {
            http,
            url: cfg.url.clone(),
            api_key: cfg.api_key.trim().to_string(),
            model: cfg.model.clone(),
        })
    }

    /// Send one decision request. `state` is the content to judge; `questions`
    /// is the id→question map. Every question is evaluated in parallel.
    pub async fn decide(&self, state: Value, questions: Value) -> Result<Answers, TypeSafeError> {
        let body = json!({ "model": self.model, "state": state, "questions": questions });
        let resp = self
            .http
            .post(&self.url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| TypeSafeError::Unreachable(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TypeSafeError::Http {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }
        resp.json::<Answers>()
            .await
            .map_err(|e| TypeSafeError::Decode(e.to_string()))
    }
}

/// Build a `choice` question object.
pub fn q_choice(instructions: &str, criteria: Value) -> Value {
    json!({ "type": "choice", "instructions": instructions, "criteria": criteria })
}

/// Build a `noul` (yes/no proposition) question object.
pub fn q_noul(instructions: &str) -> Value {
    json!({ "type": "noul", "instructions": instructions })
}
