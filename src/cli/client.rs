//! HTTP client used by the CLI to talk to a running `mnemos serve`.
//!
//! All subcommands go through [`Client::request`] so that auth header
//! injection, JSON error parsing, and exit-code mapping happen in one
//! place. The client is cheap to clone (an `Arc<reqwest::Client>`) and
//! can be created per command — there's no need to share state.
//!
//! ## Auth
//!
//! The bearer token is attached as
//! `Authorization: Bearer mnemo_…`. If [`Client::api_key`] is `None`
//! at request time, the client refuses to make the call and returns
//! `CliError::Usage("api key required")` — this is what `user register`
//! / `user login` rely on to fail fast when no key has been set up yet.
//!
//! ## Error parsing
//!
//! The server's error envelope is `{ "code": "...", "message": "..." }`.
//! Non-2xx responses are turned into `CliError::Http`, which [`output`]
//! maps to a stable exit code.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use reqwest::{Client as HttpClient, Method, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::cli::config::CliConfig;
use crate::cli::output::{CliError, CliResult};

/// CLI-side HTTP client.
#[derive(Debug, Clone)]
pub struct Client {
    base_url: String,
    api_key: Option<String>,
    http: HttpClient,
}

impl Client {
    /// Build a client from a resolved [`CliConfig`].
    pub fn new(config: &CliConfig) -> CliResult<Self> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("mnemos-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| CliError::Network(e.to_string()))?;
        Ok(Self {
            base_url: config.api_url.clone(),
            api_key: config.api_key.clone(),
            http,
        })
    }

    /// Replace the API key. Used by `user register` / `user login`
    /// after a successful auth so the in-process client can make the
    /// follow-up call without re-reading the file.
    pub fn with_api_key(mut self, key: String) -> Self {
        self.api_key = Some(key);
        self
    }

    /// The base URL the client is targeting. Useful for diagnostics.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The currently configured API key, if any.
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Issue a typed request. The body is serialised to JSON if
    /// `Some`. Returns the decoded response on 2xx, or `CliError::Http`
    /// on anything else.
    pub async fn request<T, B>(&self, method: Method, path: &str, body: Option<&B>) -> CliResult<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let url = self.url_for(path);
        let mut req = self.http.request(method.clone(), &url);
        req = self.apply_auth(req)?;
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await?;
        self.handle(resp).await
    }

    /// Issue a request without a request body and returning a raw
    /// `serde_json::Value`. Used by `lint`, `index`, `log`, and
    /// similar endpoints where the schema is simple.
    pub async fn request_value(&self, method: Method, path: &str) -> CliResult<Value> {
        self.request(method, path, None::<&()>).await
    }

    /// Send a raw `reqwest::RequestBuilder`. Used for special-cased
    /// endpoints (multipart upload, raw text bodies).
    pub async fn send_raw(&self, builder: RequestBuilder) -> CliResult<Response> {
        let builder = self.apply_auth(builder)?;
        let resp = builder.send().await?;
        if !resp.status().is_success() {
            // Parse to a typed error and propagate.
            return Err(self.parse_error(resp).await);
        }
        Ok(resp)
    }

    /// Convenience: `GET <path>` and return the JSON body.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> CliResult<T> {
        self.request(Method::GET, path, None::<&()>).await
    }

    /// Convenience: `POST <path>` with a JSON body.
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> CliResult<T> {
        self.request(Method::POST, path, Some(body)).await
    }

    /// `POST <path>` with a JSON body and **no** auth header. Used by
    /// `user register` / `user login`, which mint the first key.
    pub async fn post_noauth<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> CliResult<T> {
        let url = self.url_for(path);
        let resp = self.http.post(&url).json(body).send().await?;
        self.handle(resp).await
    }

    /// Convenience: `PUT <path>` with a JSON body.
    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> CliResult<T> {
        self.request(Method::PUT, path, Some(body)).await
    }

    /// Convenience: `DELETE <path>` returning the JSON body (or unit
    /// when the server returns an empty body).
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> CliResult<T> {
        self.request(Method::DELETE, path, None::<&()>).await
    }

    /// `POST <path>` as `multipart/form-data`. Used by `sources upload`.
    pub async fn post_multipart<T: DeserializeOwned>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> CliResult<T> {
        let url = self.url_for(path);
        let req = self.http.post(&url).multipart(form);
        let req = self.apply_auth(req)?;
        let resp = req.send().await?;
        self.handle(resp).await
    }

    /// `GET <path>` returning the raw text body. Used by `pages get
    /// --raw` and `sources get --raw`.
    pub async fn get_text(&self, path: &str) -> CliResult<String> {
        let url = self.url_for(path);
        let req = self.http.get(&url);
        let req = self.apply_auth(req)?;
        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(self.parse_error(resp).await);
        }
        Ok(resp.text().await?)
    }

    /// `GET <path>` returning the raw bytes. Used by `sources upload
    /// <file>` flows that want to pipe bytes back to stdout without
    /// going through JSON.
    pub async fn get_bytes(&self, path: &str) -> CliResult<Vec<u8>> {
        let url = self.url_for(path);
        let req = self.http.get(&url);
        let req = self.apply_auth(req)?;
        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(self.parse_error(resp).await);
        }
        Ok(resp.bytes().await?.to_vec())
    }

    /// Apply the bearer auth header to any `RequestBuilder` we send.
    fn apply_auth(&self, req: RequestBuilder) -> CliResult<RequestBuilder> {
        if let Some(key) = &self.api_key {
            let mut headers = HeaderMap::new();
            let header_name = HeaderName::from_static("authorization");
            let header_value = format!("Bearer {key}");
            let value = HeaderValue::from_str(&header_value)
                .map_err(|e| CliError::Usage(format!("invalid api key: {e}")))?;
            headers.insert(header_name, value);
            // We use the typed AUTHORIZATION constant for safety, but
            // the from_static route above is a belt-and-braces for
            // cases where the constant isn't picked up.
            let mut req = req;
            if let Ok(v) = HeaderValue::from_str(&format!("Bearer {key}")) {
                req = req.header(AUTHORIZATION, v);
            }
            Ok(req)
        } else {
            // No key. The caller can still use this for `user register`
            // / `user login` / `serve` / `mcp` — but those don't go
            // through `Client`. The expectation is that auth-required
            // commands fail before they get here.
            Err(CliError::Usage(
                "api key is required (set MNEMOS_API_KEY or run `mnemos user login`)".into(),
            ))
        }
    }

    fn url_for(&self, path: &str) -> String {
        // Both base and path may or may not have a slash; we don't
        // care which.
        let base = self.base_url.trim_end_matches('/');
        if path.starts_with('/') {
            format!("{base}{path}")
        } else {
            format!("{base}/{path}")
        }
    }

    async fn handle<T: DeserializeOwned>(&self, resp: Response) -> CliResult<T> {
        if !resp.status().is_success() {
            return Err(self.parse_error(resp).await);
        }
        // Be lenient about empty bodies for things like DELETE.
        if resp.status() == StatusCode::NO_CONTENT {
            // Caller asked for a concrete type. There's no reasonable
            // default for arbitrary T, so we special-case `()` and
            // empty JSON for everything else.
            return serde_json::from_str("null").map_err(CliError::from);
        }
        let bytes = resp.bytes().await?;
        if bytes.is_empty() {
            return serde_json::from_str("null").map_err(CliError::from);
        }
        serde_json::from_slice(&bytes).map_err(CliError::from)
    }

    async fn parse_error(&self, resp: Response) -> CliError {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        // Try to parse the structured error body. If it doesn't match
        // the shape, surface a generic message.
        if let Ok(v) = serde_json::from_str::<Value>(&body) {
            // Two shapes in the wild: the `ApiError` envelope nests under
            // `error`, while the auth middleware returns a flat object.
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
            CliError::Http {
                status,
                code,
                message,
            }
        } else if body.is_empty() {
            CliError::Http {
                status,
                code: "unknown".into(),
                message: format!("HTTP {status} with empty body"),
            }
        } else {
            CliError::Http {
                status,
                code: "unknown".into(),
                message: body.chars().take(200).collect(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(url: &str, key: Option<&str>) -> CliConfig {
        CliConfig {
            api_url: url.to_string(),
            api_key: key.map(|s| s.to_string()),
            credentials_path: std::path::PathBuf::from("/tmp/none"),
            config_path: None,
        }
    }

    #[test]
    fn url_for_joins_correctly() {
        let c = Client::new(&cfg("http://h:8080", None)).unwrap();
        assert_eq!(c.url_for("/api/v1/pages"), "http://h:8080/api/v1/pages");
        assert_eq!(c.url_for("api/v1/pages"), "http://h:8080/api/v1/pages");
        // Trailing slash on base, leading slash on path: no double slash.
        let c2 = Client::new(&cfg("http://h:8080/", None)).unwrap();
        assert_eq!(c2.url_for("/api/v1/pages"), "http://h:8080/api/v1/pages");
    }

    #[test]
    fn auth_required_for_get() {
        let c = Client::new(&cfg("http://h:8080", None)).unwrap();
        // Should fail with the usage error before any HTTP happens.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt.block_on(async { c.get::<serde_json::Value>("/api/v1/pages").await });
        assert!(matches!(err, Err(CliError::Usage(_))));
    }
}
