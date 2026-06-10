//! HTTP request / response DTOs.
//!
//! These types are the on-the-wire shape of the REST API. They mirror the
//! domain types in `core::page`, `core::source`, and `auth::user`, but
//! stay decoupled so the wire schema can evolve without churning the
//! internal types.
//!
//! Validation: prefer the explicit `validate_*` helpers below to the
//! `validator` crate — the surface is small enough that ad-hoc checks
//! stay readable, and it keeps the dependency graph light.

use serde::{Deserialize, Serialize};

use crate::core::frontmatter::{Frontmatter, PageType, Scope, SourceRef};
use crate::core::page::Page;
use crate::core::source::Source;

// ---------------------------------------------------------------------------
// Common helpers
// ---------------------------------------------------------------------------

pub const MIN_USERNAME_LEN: usize = 3;
pub const MAX_USERNAME_LEN: usize = 32;
pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_KEY_NAME_LEN: usize = 64;
pub const MAX_BODY_BYTES: usize = 10 * 1024 * 1024; // 10 MB

/// Pagination defaults. A single page of `?limit=` results.
pub const DEFAULT_PAGE_LIMIT: i64 = 50;
pub const MAX_PAGE_LIMIT: i64 = 200;

// ---------------------------------------------------------------------------
// Error envelope
// ---------------------------------------------------------------------------

/// JSON error envelope. Wrapped under the top-level `error` key in the
/// final response, see [`ApiError::into_response`].
#[derive(Debug, Serialize)]
pub struct ErrorEnvelope<'a> {
    pub code: &'a str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user_id: String,
    pub username: String,
    pub api_key: String,
    /// The id of the freshly-issued key. Useful for clients that want
    /// to refer to it later (e.g. revoke).
    pub key_id: String,
}

impl AuthResponse {
    pub fn new(
        user_id: impl Into<String>,
        username: impl Into<String>,
        key_id: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            username: username.into(),
            api_key: api_key.into(),
            key_id: key_id.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct KeyView {
    pub id: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<crate::storage::api_key_repo::ApiKeyRow> for KeyView {
    fn from(r: crate::storage::api_key_repo::ApiKeyRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            created_at: r.created_at,
            last_used_at: r.last_used_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CreatedKeyView {
    #[serde(flatten)]
    pub view: KeyView,
    /// Plaintext key, shown exactly once.
    pub api_key: String,
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreatePageRequest {
    /// Optional. If absent, the slug is auto-generated from the title.
    pub slug: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub frontmatter: Option<Frontmatter>,
    /// Convenience: callers that only have a single `body` field can send
    /// the frontmatter inline as a YAML string. We parse it on the server.
    pub frontmatter_yaml: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePageRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    pub frontmatter: Option<Frontmatter>,
    /// Convenience: replace the full frontmatter with the contents of this
    /// YAML string. Useful for clients that already maintain the page on
    /// disk.
    pub frontmatter_yaml: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListPagesQuery {
    pub tag: Option<String>,
    #[serde(rename = "type")]
    pub page_type: Option<String>,
    pub project: Option<String>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PageSummary {
    pub slug: String,
    pub title: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub page_type: Option<PageType>,
    pub tags: Vec<String>,
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

impl From<&Page> for PageSummary {
    fn from(p: &Page) -> Self {
        Self {
            slug: p.slug.clone(),
            title: p.title.clone(),
            updated_at: p.updated_at,
            created_at: p.created_at,
            page_type: p.frontmatter.page_type,
            tags: p.frontmatter.tags.clone(),
            scope: p.frontmatter.scope,
            project: p.frontmatter.project.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PageListResponse {
    pub items: Vec<PageSummary>,
    pub total: usize,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct PageView {
    pub slug: String,
    pub title: String,
    pub body: String,
    pub frontmatter: Frontmatter,
    pub sources: Vec<SourceRef>,
    pub related: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<Page> for PageView {
    fn from(p: Page) -> Self {
        Self {
            slug: p.slug,
            title: p.title,
            body: p.body,
            frontmatter: p.frontmatter.clone(),
            sources: p.frontmatter.sources.clone(),
            related: p.frontmatter.related.clone(),
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AddSourceUrlRequest {
    pub url: String,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SourceView {
    pub id: String,
    pub source_id: String,
    pub slug: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub origin: Option<String>,
    pub path: String,
    pub content_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Source> for SourceView {
    fn from(s: Source) -> Self {
        Self {
            id: s.id,
            source_id: s.source_id,
            slug: s.slug,
            kind: s.r#type,
            origin: s.origin,
            path: s.path,
            content_hash: s.content_hash,
            created_at: s.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Log
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    pub since: Option<String>,
    pub limit: Option<i64>,
    pub format: Option<String>, // "markdown" (default) or "json"
}

#[derive(Debug, Serialize)]
pub struct LogEvent {
    pub id: i64,
    pub user_id: String,
    pub kind: String,
    #[serde(rename = "ref")]
    pub ref_: Option<String>,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub payload: Option<serde_json::Value>,
}

impl From<crate::storage::event_repo::EventRow> for LogEvent {
    fn from(r: crate::storage::event_repo::EventRow) -> Self {
        let payload = r
            .payload_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Self {
            id: r.id,
            user_id: r.user_id,
            kind: r.kind,
            ref_: r.ref_,
            ts: r.ts,
            payload,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LogJson {
    pub items: Vec<LogEvent>,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Lint
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct LintView {
    pub findings: Vec<crate::core::lint::LintFinding>,
    pub summary: LintSummary,
}

#[derive(Debug, Serialize, Default)]
pub struct LintSummary {
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

impl LintSummary {
    pub fn from_findings(fs: &[crate::core::lint::LintFinding]) -> Self {
        let mut s = Self::default();
        for f in fs {
            match f.severity {
                crate::core::lint::Severity::Error => s.errors += 1,
                crate::core::lint::Severity::Warning => s.warnings += 1,
                crate::core::lint::Severity::Info => s.info += 1,
            }
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub build_rev: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Returns the first validation error message, or `Ok(())` if `name` is
/// acceptable as a username. API-specific rules: 3-32 chars, `[a-z0-9_-]`.
pub fn validate_api_username(name: &str) -> Result<(), String> {
    if name.len() < MIN_USERNAME_LEN {
        return Err(format!(
            "username must be at least {MIN_USERNAME_LEN} characters"
        ));
    }
    if name.len() > MAX_USERNAME_LEN {
        return Err(format!(
            "username must be at most {MAX_USERNAME_LEN} characters"
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err("username may only contain [a-z0-9_-]".to_string());
    }
    Ok(())
}

pub fn validate_api_password(pw: &str) -> Result<(), String> {
    if pw.len() < MIN_PASSWORD_LEN {
        return Err(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        ));
    }
    Ok(())
}

pub fn validate_key_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("key name must not be empty".into());
    }
    if trimmed.len() > MAX_KEY_NAME_LEN {
        return Err(format!(
            "key name must be at most {MAX_KEY_NAME_LEN} characters"
        ));
    }
    Ok(())
}

pub fn validate_page_type(t: &str) -> Result<(), String> {
    match t {
        "concept" | "recipe" | "reference" | "decision" => Ok(()),
        other => Err(format!(
            "invalid page type '{other}': expected concept|recipe|reference|decision"
        )),
    }
}

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use crate::core::frontmatter::{
    Frontmatter as FrontmatterDto, PageType as PageTypeDto, Scope as ScopeDto, SourceKind,
    SourceRef as SourceRefDto,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_validation() {
        assert!(validate_api_username("ab").is_err());
        assert!(validate_api_username("abc").is_ok());
        assert!(validate_api_username("a".repeat(33).as_str()).is_err());
        assert!(validate_api_username("abc_def-99").is_ok());
        assert!(validate_api_username("AB").is_err()); // uppercase rejected
        assert!(validate_api_username("ab c").is_err()); // space rejected
    }

    #[test]
    fn password_validation() {
        assert!(validate_api_password("short").is_err());
        assert!(validate_api_password("longenough").is_ok());
    }

    #[test]
    fn key_name_validation() {
        assert!(validate_key_name("").is_err());
        assert!(validate_key_name("   ").is_err());
        assert!(validate_key_name("laptop").is_ok());
    }
}
