//! MCP (Model Context Protocol) server for mnemos.
//!
//! Thin transport adapter: speaks JSON-RPC 2.0 on stdin/stdout and
//! translates `tools/call` and `resources/read` requests into HTTP
//! calls to the running REST API.

pub mod resources;
pub mod server;
pub mod tools;

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client as HttpClient, Method, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

/// Protocol version advertised in `initialize` responses.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Server name advertised in `initialize` responses.
pub const SERVER_NAME: &str = "mnemos";

/// Default REST API base URL when `MNEMOS_API_URL` is not set.
pub const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";

/// Errors that can come back from the REST API.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("upstream unreachable: {0}")]
    Unreachable(String),
    #[error("{code}: {message}")]
    Api { status: u16, code: String, message: String },
    #[error("upstream HTTP {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

/// HTTP client used by the MCP server to talk to the REST API.
#[derive(Debug, Clone)]
pub struct McpRestClient {
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) http: HttpClient,
}

impl McpRestClient {
    pub fn from_env() -> Result<Self, String> {
        let base_url = std::env::var("MNEMOS_API_URL")
            .unwrap_or_else(|_| DEFAULT_API_URL.to_string());
        let api_key = std::env::var("MNEMOS_API_KEY")
            .map_err(|_| "MNEMOS_API_KEY is required for the MCP server".to_string())?;
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err("MNEMOS_API_KEY must not be empty".to_string());
        }
        Self::new(base_url, api_key).map_err(|e| format!("build http client: {e}"))
    }

    pub fn new(base_url: String, api_key: String) -> Result<Self, reqwest::Error> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("mnemos-mcp/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { base_url, api_key, http })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn request<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, McpError>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let url = self.url_for(path);
        let mut req = self.http.request(method, &url);
        req = self.apply_auth(req);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(|e| McpError::Unreachable(e.to_string()))?;
        self.parse(resp).await
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, McpError> {
        self.request(Method::GET, path, None::<&()>).await
    }

    pub async fn get_value(&self, path: &str) -> Result<Value, McpError> {
        self.get(path).await
    }

    pub async fn get_text(&self, path: &str) -> Result<String, McpError> {
        let url = self.url_for(path);
        let req = self.apply_auth(self.http.get(&url));
        let resp = req.send().await.map_err(|e| McpError::Unreachable(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(self.build_error(resp, status).await);
        }
        resp.text().await.map_err(|e| McpError::Unreachable(e.to_string()))
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, McpError> {
        self.request(Method::POST, path, Some(body)).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, McpError> {
        self.request(Method::PUT, path, Some(body)).await
    }

    pub async fn post_value<B: Serialize>(&self, path: &str, body: &B) -> Result<Value, McpError> {
        self.post(path, body).await
    }

    pub async fn put_value<B: Serialize>(&self, path: &str, body: &B) -> Result<Value, McpError> {
        self.put(path, body).await
    }

    pub async fn delete(&self, path: &str) -> Result<(), McpError> {
        let url = self.url_for(path);
        let req = self.apply_auth(self.http.delete(&url));
        let resp = req.send().await.map_err(|e| McpError::Unreachable(e.to_string()))?;
        let status = resp.status();
        if status == StatusCode::NO_CONTENT || status.is_success() {
            return Ok(());
        }
        Err(self.build_error(resp, status).await)
    }

    fn apply_auth(&self, req: RequestBuilder) -> RequestBuilder {
        let mut headers = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(&format!("Bearer {}", self.api_key)) {
            headers.insert(AUTHORIZATION, v);
        }
        req.headers(headers)
    }

    pub(crate) fn url_for(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        if path.starts_with('/') {
            format!("{base}{path}")
        } else {
            format!("{base}/{path}")
        }
    }

    async fn parse<T: DeserializeOwned>(&self, resp: Response) -> Result<T, McpError> {
        let status = resp.status();
        if !status.is_success() {
            return Err(self.build_error(resp, status).await);
        }
        if status == StatusCode::NO_CONTENT {
            return serde_json::from_str("null")
                .map_err(|e| McpError::Unreachable(format!("decode null: {e}")));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| McpError::Unreachable(e.to_string()))?;
        if bytes.is_empty() {
            return serde_json::from_str("null")
                .map_err(|e| McpError::Unreachable(format!("decode null: {e}")));
        }
        serde_json::from_slice(&bytes).map_err(|e| McpError::Unreachable(format!("decode: {e}")))
    }

    async fn build_error(&self, resp: Response, status: StatusCode) -> McpError {
        let status = status.as_u16();
        let body = resp.text().await.unwrap_or_default();
        if let Ok(v) = serde_json::from_str::<Value>(&body) {
            let obj = v.get("error").unwrap_or(&v);
            let code = obj
                .get("code")
                .and_then(|c| c.as_str())
                .unwrap_or("unknown")
                .to_string();
            let message = obj
                .get("message")
                .and_then(|c| c.as_str())
                .unwrap_or("(no message)")
                .to_string();
            McpError::Api { status, code, message }
        } else if body.is_empty() {
            McpError::UnexpectedStatus {
                status,
                body: format!("HTTP {status} with empty body"),
            }
        } else {
            McpError::UnexpectedStatus {
                status,
                body: body.chars().take(200).collect(),
            }
        }
    }
}

/// Entry point. Reads env, builds the client, and runs the stdio loop
/// until EOF on stdin.
pub async fn run_stdio() -> anyhow::Result<()> {
    let client = McpRestClient::from_env()
        .map_err(|e| anyhow::anyhow!("init MCP client: {e}"))?;
    tracing::info!(
        base_url = %client.base_url(),
        "mnemos mcp server ready on stdio"
    );
    server::serve(client).await
}
