//! MCP tool surface — mirrors the REST API, with one tool per row in the
//! scratchpad contract.
//!
//! Each tool has a [`Tool`] definition (name, description, JSON Schema for
//! `inputSchema`) and a handler in [`call_tool`]. Handlers resolve to
//! [`crate::core`] service calls; all errors are mapped to MCP error
//! codes by the server.

use base64::Engine;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::frontmatter::{Frontmatter, PageType, Scope, SourceKind, SourceRef};
use crate::core::index::IndexBuilder;
use crate::core::lint::{Linter, Severity};
use crate::core::page::{PageFilter, PageService};
use crate::core::search::{PageHit, SearchService};
use crate::core::source::SourceService;
use crate::error::{AppError, Result};
use crate::mcp::AuthedContext;
use crate::storage::event_repo;

/// A `tools/list` entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// A single text content item returned by a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    /// Optional MIME hint. `text/markdown` for rendered wiki pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

impl TextContent {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            kind: "text".into(),
            text: s.into(),
            mime_type: None,
        }
    }
    pub fn markdown(s: impl Into<String>) -> Self {
        Self {
            kind: "text".into(),
            text: s.into(),
            mime_type: Some("text/markdown".into()),
        }
    }
}

/// Parameters to a `tools/call` request.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Return the canonical list of tools.
pub fn list_tools() -> Vec<Tool> {
    vec![
        tool(
            "list_pages",
            "List pages in the wiki. Supports optional substring match on title/slug, \
             a single tag filter, a `page_type` filter, and a `project` filter.",
            json!({
                "type": "object",
                "properties": {
                    "query":     { "type": "string",  "description": "Substring filter on title or slug." },
                    "tag":       { "type": "string",  "description": "Filter to pages carrying this tag." },
                    "page_type": { "type": "string",  "enum": ["concept", "recipe", "reference", "decision"], "description": "Filter to a specific page_type." },
                    "project":   { "type": "string",  "description": "Filter to pages belonging to a specific project." },
                    "limit":     { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Maximum number of pages to return." }
                },
                "additionalProperties": false,
            }),
        ),
        tool(
            "get_page",
            "Fetch a single page by slug, including frontmatter and body.",
            json!({
                "type": "object",
                "properties": {
                    "slug": { "type": "string", "description": "Page slug (kebab-case, max 80 chars)." }
                },
                "required": ["slug"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "create_page",
            "Create a new wiki page. Returns the inserted page (frontmatter + body). \
             If `slug` is empty, it is derived from the title. `frontmatter` accepts the \
             same shape as the on-disk frontmatter (title, tags, page_type, scope, related, sources).",
            json!({
                "type": "object",
                "properties": {
                    "slug":         { "type": "string", "description": "Optional page slug. Auto-derived from title if empty." },
                    "title":        { "type": "string", "description": "Page title. Required." },
                    "body":         { "type": "string", "description": "Page body in markdown." },
                    "frontmatter":  { "$ref": "#/$defs/frontmatter" }
                },
                "required": ["title", "body"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "update_page",
            "Update an existing page. Any of `body`, `title`, `frontmatter` may be omitted \
             (omit = unchanged). The `slug` is always preserved — to rename, delete and re-create.",
            json!({
                "type": "object",
                "properties": {
                    "slug":         { "type": "string", "description": "Slug of the page to update." },
                    "title":        { "type": "string" },
                    "body":         { "type": "string" },
                    "frontmatter":  { "$ref": "#/$defs/frontmatter" }
                },
                "required": ["slug"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "delete_page",
            "Delete a page by slug. Idempotent (returns ok even if the page didn't exist).",
            json!({
                "type": "object",
                "properties": {
                    "slug": { "type": "string" }
                },
                "required": ["slug"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "search_pages",
            "Full-text search over the wiki using FTS5 BM25 ranking.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query (FTS5 syntax; multi-word is OR-joined by default)." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 10 }
                },
                "required": ["query"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "list_sources",
            "List all raw sources registered for the user (URL fetches and uploads).",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
        tool(
            "get_source",
            "Fetch source metadata by `source_id`. Use `mnemos://source/{id}` for the raw body.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Opaque source id." }
                },
                "required": ["id"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "add_source_url",
            "Fetch a URL and store the response body as an immutable source.",
            json!({
                "type": "object",
                "properties": {
                    "url":  { "type": "string", "description": "http(s) URL to fetch." },
                    "slug": { "type": "string", "description": "Optional slug for the source (auto-derived from the URL path otherwise)." }
                },
                "required": ["url"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "upload_source",
            "Upload raw content as a new source. `content` is base64-encoded.",
            json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "Base64-encoded file content." },
                    "slug":    { "type": "string", "description": "Slug for the source (defaults to 'upload')." }
                },
                "required": ["content"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "get_index",
            "Return the auto-generated catalog (index.md) for the user.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
        tool(
            "get_log",
            "Return the append-only event log (log.md).",
            json!({
                "type": "object",
                "properties": {
                    "since": { "type": "string", "description": "RFC3339 timestamp; only events strictly after this are returned." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 100 }
                },
                "additionalProperties": false,
            }),
        ),
        tool(
            "lint",
            "Run the wiki linter. Returns orphan pages, broken `related:` links, and missing source refs.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
    ]
}

fn tool(name: &str, description: &str, mut input_schema: Value) -> Tool {
    // Add the shared $defs.frontmatter schema to any tool that needs it.
    if let Some(obj) = input_schema.as_object_mut() {
        let needs_fm_def = obj
            .values()
            .any(|v| v.get("$ref").and_then(|r| r.as_str()) == Some("#/$defs/frontmatter"));
        if needs_fm_def {
            let mut defs = serde_json::Map::new();
            defs.insert("frontmatter".into(), frontmatter_schema());
            obj.insert("$defs".into(), Value::Object(defs));
        }
    }
    Tool {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
    }
}

fn frontmatter_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title":     { "type": "string" },
            "tags":      { "type": "array", "items": { "type": "string" }, "maxItems": 8 },
            "created":   { "type": "string", "format": "date", "description": "YYYY-MM-DD. Set on first create; never changes." },
            "updated":   { "type": "string", "format": "date", "description": "YYYY-MM-DD. Bumped on every update." },
            "scope":     { "type": "string", "enum": ["global", "local"] },
            "page_type": { "type": "string", "enum": ["concept", "recipe", "reference", "decision"] },
            "related":   { "type": "array", "items": { "type": "string" } },
            "sources": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["type", "ref"],
                    "properties": {
                        "type":   { "type": "string", "enum": ["url", "upload", "session"] },
                        "ref":    { "type": "string", "description": "Source ref id (e.g. sources/abc123-slug.md)." },
                        "origin": { "type": "string", "description": "Original URL, for type=url." }
                    },
                    "additionalProperties": false,
                }
            }
        },
        "additionalProperties": false,
    })
}

/// Dispatch a single tool call to the appropriate handler.
pub async fn call_tool(name: &str, args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    match name {
        "list_pages" => tool_list_pages(args, user).await,
        "get_page" => tool_get_page(args, user).await,
        "create_page" => tool_create_page(args, user).await,
        "update_page" => tool_update_page(args, user).await,
        "delete_page" => tool_delete_page(args, user).await,
        "search_pages" => tool_search_pages(args, user).await,
        "list_sources" => tool_list_sources(user).await,
        "get_source" => tool_get_source(args, user).await,
        "add_source_url" => tool_add_source_url(args, user).await,
        "upload_source" => tool_upload_source(args, user).await,
        "get_index" => tool_get_index(user).await,
        "get_log" => tool_get_log(args, user).await,
        "lint" => tool_lint(user).await,
        other => Err(AppError::Validation(format!("unknown tool '{other}'"))),
    }
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(other) => Err(AppError::Validation(format!(
            "{key} must be a string, got {other}"
        ))),
    }
}

fn arg_str_required<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    arg_str(args, key)?.ok_or_else(|| AppError::Validation(format!("{key} is required")))
}

fn arg_i64_opt(args: &Value, key: &str) -> Result<Option<i64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| AppError::Validation(format!("{key} must be an integer"))),
        Some(other) => Err(AppError::Validation(format!(
            "{key} must be an integer, got {other}"
        ))),
    }
}

fn arg_tags(args: &Value) -> Result<Option<Vec<String>>> {
    match args.get("tags") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                match v {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(AppError::Validation(format!(
                            "tags must be an array of strings, got {other}"
                        )))
                    }
                }
            }
            Ok(Some(out))
        }
        Some(other) => Err(AppError::Validation(format!(
            "tags must be an array, got {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

async fn tool_list_pages(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let page_type = arg_str(&args, "page_type")?.map(str::to_string);
    let tag = arg_str(&args, "tag")?.map(str::to_string);
    let query = arg_str(&args, "query")?.map(str::to_string);
    let project = arg_str(&args, "project")?.map(str::to_string);
    let limit = arg_i64_opt(&args, "limit")?;
    // The REST surface uses a single `tag`; we accept the array form for
    // agent convenience and OR-filter the result.
    let tags = arg_tags(&args)?;
    let limit = limit.unwrap_or(100).clamp(1, 1000);

    let svc = PageService::new(user.ctx.state.clone());
    let mut filter = PageFilter {
        query: query.clone(),
        tag: tag.clone(),
        page_type: page_type.clone(),
        project: project.clone(),
        limit: Some(limit),
    };
    let mut pages = svc.list(&user.user_id, &filter).await?;
    if let Some(want) = &tags {
        pages.retain(|p| {
            want.iter()
                .all(|t| p.frontmatter.tags.iter().any(|x| x == t))
        });
    }
    let _ = &mut filter;
    let summary = json!({
        "filter": { "query": query, "tag": tag, "tags": tags, "page_type": page_type, "project": project, "limit": limit },
        "count": pages.len(),
        "pages": pages,
    });
    Ok(vec![TextContent::text(
        serde_json::to_string_pretty(&summary).unwrap_or_default(),
    )])
}

async fn tool_get_page(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let slug = arg_str_required(&args, "slug")?.to_string();
    let svc = PageService::new(user.ctx.state.clone());
    let page = svc.get(&user.user_id, &slug).await?;
    Ok(vec![TextContent::json(page)])
}

async fn tool_create_page(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let title = arg_str_required(&args, "title")?.to_string();
    let body = arg_str(&args, "body")?.unwrap_or("").to_string();
    let slug = arg_str(&args, "slug")?.map(str::to_string);
    let fm = parse_frontmatter(
        args.get("frontmatter").cloned().unwrap_or(Value::Null),
        title.clone(),
    )?;

    let svc = PageService::new(user.ctx.state.clone());
    let page = svc
        .create(&user.user_id, slug.as_deref().unwrap_or(""), fm, &body)
        .await?;
    Ok(vec![TextContent::json(page)])
}

async fn tool_update_page(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let slug = arg_str_required(&args, "slug")?.to_string();
    let title = arg_str(&args, "title")?.map(str::to_string);
    let body = arg_str(&args, "body")?.map(str::to_string);

    let svc = PageService::new(user.ctx.state.clone());
    let existing = svc.get(&user.user_id, &slug).await?;

    // Merge frontmatter: start from the existing one and overlay any
    // explicitly provided fields. `None` in the incoming payload means
    // "leave alone", while `Some(v)` means "set to v".
    let mut fm = existing.frontmatter.clone();
    if let Some(value) = args.get("frontmatter") {
        if !value.is_null() {
            let incoming = parse_frontmatter(value.clone(), fm.title.clone().unwrap_or_default())?;
            merge_frontmatter(&mut fm, incoming);
        }
    }
    if let Some(t) = title {
        fm.title = Some(t);
    }

    let body = body.unwrap_or(existing.body);
    let page = svc.update(&user.user_id, &slug, fm, &body).await?;
    Ok(vec![TextContent::json(page)])
}

async fn tool_delete_page(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let slug = arg_str_required(&args, "slug")?.to_string();
    let svc = PageService::new(user.ctx.state.clone());
    // Idempotent: silently succeed on missing pages so retries are safe.
    match svc.delete(&user.user_id, &slug).await {
        Ok(()) => Ok(vec![TextContent::json(json!({
            "ok": true,
            "slug": slug,
            "deleted": true,
        }))]),
        Err(AppError::NotFound(_)) => Ok(vec![TextContent::json(json!({
            "ok": true,
            "slug": slug,
            "deleted": false,
        }))]),
        Err(e) => Err(e),
    }
}

async fn tool_search_pages(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let query = arg_str_required(&args, "query")?.to_string();
    let limit = arg_i64_opt(&args, "limit")?.unwrap_or(10).clamp(1, 1000);
    let svc = SearchService::new(&user.ctx.state);
    let hits: Vec<PageHit> = svc.search(&user.user_id, &query, limit).await?;
    let summary = json!({
        "query": query,
        "limit": limit,
        "count": hits.len(),
        "hits": hits,
    });
    Ok(vec![TextContent::text(
        serde_json::to_string_pretty(&summary).unwrap_or_default(),
    )])
}

async fn tool_list_sources(user: &AuthedContext) -> Result<Vec<TextContent>> {
    let svc = SourceService::new(user.ctx.state.clone())?;
    let list = svc.list(&user.user_id).await?;
    Ok(vec![TextContent::json(json!({
        "count": list.len(),
        "sources": list,
    }))])
}

async fn tool_get_source(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let id = arg_str_required(&args, "id")?.to_string();
    let svc = SourceService::new(user.ctx.state.clone())?;
    let src = svc.get(&user.user_id, &id).await?;
    Ok(vec![TextContent::json(src)])
}

async fn tool_add_source_url(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let url = arg_str_required(&args, "url")?.to_string();
    let slug = arg_str(&args, "slug")?.unwrap_or("").to_string();
    let svc = SourceService::new(user.ctx.state.clone())?;
    let src = svc.register_url(&user.user_id, &url, &slug).await?;
    Ok(vec![TextContent::json(src)])
}

async fn tool_upload_source(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let content_b64 = arg_str_required(&args, "content")?.to_string();
    let slug = arg_str(&args, "slug")?.unwrap_or("").to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content_b64.as_bytes())
        .map_err(|e| AppError::Validation(format!("content must be base64: {e}")))?;
    let svc = SourceService::new(user.ctx.state.clone())?;
    let src = svc.upload(&user.user_id, &slug, &bytes).await?;
    Ok(vec![TextContent::json(src)])
}

async fn tool_get_index(user: &AuthedContext) -> Result<Vec<TextContent>> {
    // Ensure the on-disk index.md exists for the user. The build is
    // idempotent and cheap; the file may legitimately be missing for a
    // brand-new user.
    let state = user.ctx.state.clone();
    let builder = IndexBuilder::new(state.clone());
    builder.rebuild_for_user(&user.user_id).await?;
    let path = crate::storage::fs_layout::index_path(&state.config.data_dir, &user.user_id);
    let body = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    Ok(vec![TextContent::markdown(body)])
}

async fn tool_get_log(args: Value, user: &AuthedContext) -> Result<Vec<TextContent>> {
    let since = arg_str(&args, "since")?.map(str::to_string);
    let limit = arg_i64_opt(&args, "limit")?.unwrap_or(100).clamp(1, 1000);
    let state = user.ctx.state.clone();

    // Reading the log is side-effect free: parse `since` first, then push the
    // filter down to SQL so we never load more than `limit` rows.
    let since_dt = match &since {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(s)
                .map_err(|_| {
                    AppError::Validation(format!(
                        "since must be RFC3339 (e.g. 2026-04-17T12:00:00Z); got '{s}'"
                    ))
                })?
                .with_timezone(&Utc),
        ),
        None => None,
    };

    let filter = event_repo::EventFilter {
        since: since_dt,
        limit: Some(limit),
    };
    let events = event_repo::list_for_user(&state.db, &user.user_id, &filter).await?;
    let summary = json!({
        "since": since,
        "limit": limit,
        "count": events.len(),
        "events": events,
    });
    Ok(vec![TextContent::text(
        serde_json::to_string_pretty(&summary).unwrap_or_default(),
    )])
}

async fn tool_lint(user: &AuthedContext) -> Result<Vec<TextContent>> {
    let linter = Linter::new(user.ctx.state.clone());
    let report = linter.run(&user.user_id).await?;
    // Group counts by severity for quick eyeballing.
    let mut by_severity = serde_json::Map::new();
    for f in &report.findings {
        let key = match f.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        let n = by_severity
            .entry(key.to_string())
            .or_insert_with(|| json!(0));
        if let Value::Number(num) = n {
            *n = json!(num.as_i64().unwrap_or(0) + 1);
        }
    }
    Ok(vec![TextContent::json(json!({
        "user_id": report.user_id,
        "summary": by_severity,
        "findings": report.findings,
    }))])
}

// ---------------------------------------------------------------------------
// Frontmatter parsing/merging helpers
// ---------------------------------------------------------------------------

fn parse_frontmatter(value: Value, default_title: String) -> Result<Frontmatter> {
    if value.is_null() {
        let mut fm = Frontmatter::default();
        if !default_title.is_empty() {
            fm.title = Some(default_title);
        }
        return Ok(fm);
    }
    // Some MCP hosts (e.g. Claude Code) serialise an object-typed argument as a
    // JSON *string*. Accept either: if we got a string, parse it as JSON first.
    let value = match value {
        Value::String(s) => serde_json::from_str(&s)
            .map_err(|e| AppError::Validation(format!("frontmatter (string): {e}")))?,
        other => other,
    };
    let raw: FrontmatterRaw = serde_json::from_value(value)
        .map_err(|e| AppError::Validation(format!("frontmatter: {e}")))?;
    let mut fm = Frontmatter::default();
    if let Some(t) = raw.title {
        fm.title = Some(t);
    } else if !default_title.is_empty() {
        fm.title = Some(default_title);
    }
    fm.tags = raw.tags.unwrap_or_default();
    fm.created = raw.created;
    fm.updated = raw.updated;
    fm.scope = raw.scope.unwrap_or(Scope::Global);
    fm.page_type = raw.page_type;
    fm.related = raw.related.unwrap_or_default();
    fm.sources = raw
        .sources
        .unwrap_or_default()
        .into_iter()
        .map(|s| SourceRef {
            kind: s.kind,
            ref_: s.ref_,
            origin: s.origin,
        })
        .collect();
    Ok(fm)
}

fn merge_frontmatter(target: &mut Frontmatter, overlay: Frontmatter) {
    if overlay.title.is_some() {
        target.title = overlay.title;
    }
    if !overlay.tags.is_empty() {
        target.tags = overlay.tags;
    }
    if overlay.created.is_some() {
        target.created = overlay.created;
    }
    if overlay.updated.is_some() {
        target.updated = overlay.updated;
    }
    if let Some(s) = overlay.page_type {
        target.page_type = Some(s);
    }
    // `scope` always has a value, so the overlay always wins.
    target.scope = overlay.scope;
    if !overlay.related.is_empty() {
        target.related = overlay.related;
    }
    if !overlay.sources.is_empty() {
        target.sources = overlay.sources;
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
struct FrontmatterRaw {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    created: Option<NaiveDate>,
    #[serde(default)]
    updated: Option<NaiveDate>,
    #[serde(default)]
    scope: Option<Scope>,
    #[serde(default)]
    page_type: Option<PageType>,
    #[serde(default)]
    related: Option<Vec<String>>,
    #[serde(default)]
    sources: Option<Vec<SourceRefRaw>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SourceRefRaw {
    #[serde(rename = "type")]
    kind: SourceKind,
    #[serde(rename = "ref")]
    ref_: String,
    #[serde(default)]
    origin: Option<String>,
}

impl TextContent {
    fn json(value: impl Serialize) -> Self {
        Self {
            kind: "text".into(),
            text: serde_json::to_string_pretty(&value).unwrap_or_default(),
            mime_type: Some("application/json".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::mcp::McpContext;
    use crate::storage::init_pool;
    use serde_json::json;

    #[test]
    fn frontmatter_accepts_object_or_json_string() {
        // Object form.
        let fm = parse_frontmatter(
            json!({ "tags": ["a", "b"], "page_type": "recipe", "related": ["x"] }),
            "T".into(),
        )
        .unwrap();
        assert_eq!(fm.tags, vec!["a", "b"]);
        assert_eq!(fm.page_type, Some(PageType::Recipe));
        assert_eq!(fm.related, vec!["x"]);

        // Stringified form (some MCP hosts double-encode object args).
        let fm = parse_frontmatter(
            json!(r#"{"tags":["a","b"],"page_type":"recipe","related":["x"]}"#),
            "T".into(),
        )
        .unwrap();
        assert_eq!(fm.tags, vec!["a", "b"]);
        assert_eq!(fm.page_type, Some(PageType::Recipe));
        assert_eq!(fm.related, vec!["x"]);
    }

    async fn ctx() -> (AuthedContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::for_test(dir.path().to_path_buf());
        let state = init_pool(cfg).await.unwrap();
        let password_hash = crate::auth::hash_password("pw").unwrap();
        let user = crate::storage::user_repo::insert(&state.db, "u", &password_hash)
            .await
            .unwrap();
        let auth = AuthedContext {
            ctx: std::sync::Arc::new(McpContext::new(state)),
            user_id: user.id,
            api_key_id: "k".into(),
        };
        (auth, dir)
    }

    #[tokio::test]
    async fn create_then_get_page() {
        let (u, _d) = ctx().await;
        let created = call_tool(
            "create_page",
            json!({
                "title": "Hello",
                "body":  "world",
                "frontmatter": { "tags": ["greeting"] }
            }),
            &u,
        )
        .await
        .unwrap();
        assert_eq!(created.len(), 1);
        let body: serde_json::Value = serde_json::from_str(&created[0].text).unwrap();
        assert_eq!(body["slug"], "hello");
        assert_eq!(body["title"], "Hello");
        assert_eq!(body["body"], "world");

        let fetched = call_tool("get_page", json!({ "slug": "hello" }), &u)
            .await
            .unwrap();
        let fbody: serde_json::Value = serde_json::from_str(&fetched[0].text).unwrap();
        assert_eq!(fbody["slug"], "hello");
        assert_eq!(fbody["frontmatter"]["tags"][0], "greeting");
    }

    #[tokio::test]
    async fn list_pages_filter() {
        let (u, _d) = ctx().await;
        call_tool(
            "create_page",
            json!({ "title": "Alpha", "body": "1", "frontmatter": { "tags": ["k"] } }),
            &u,
        )
        .await
        .unwrap();
        call_tool(
            "create_page",
            json!({ "title": "Beta", "body": "2", "frontmatter": { "tags": ["k"] } }),
            &u,
        )
        .await
        .unwrap();
        call_tool(
            "create_page",
            json!({ "title": "Gamma", "body": "3", "frontmatter": { "tags": ["x"] } }),
            &u,
        )
        .await
        .unwrap();

        let res = call_tool("list_pages", json!({ "tag": "k", "limit": 100 }), &u)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&res[0].text).unwrap();
        assert_eq!(v["count"], 2);
    }

    #[tokio::test]
    async fn search_finds_match() {
        let (u, _d) = ctx().await;
        call_tool(
            "create_page",
            json!({ "title": "Quokka", "body": "a small marsupial" }),
            &u,
        )
        .await
        .unwrap();
        let res = call_tool("search_pages", json!({ "query": "marsupial" }), &u)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&res[0].text).unwrap();
        assert!(v["count"].as_i64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn get_index_after_create() {
        let (u, _d) = ctx().await;
        call_tool(
            "create_page",
            json!({ "title": "Alpha", "body": "x", "frontmatter": { "page_type": "concept" } }),
            &u,
        )
        .await
        .unwrap();
        let res = call_tool("get_index", json!({}), &u).await.unwrap();
        assert_eq!(res[0].mime_type.as_deref(), Some("text/markdown"));
        assert!(res[0].text.contains("Alpha"));
    }

    #[tokio::test]
    async fn upload_source_via_base64() {
        let (u, _d) = ctx().await;
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"hello world");
        let res = call_tool(
            "upload_source",
            json!({ "content": encoded, "slug": "greeting" }),
            &u,
        )
        .await
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&res[0].text).unwrap();
        assert_eq!(v["slug"], "greeting");
        assert_eq!(v["type"], "upload");
    }

    #[tokio::test]
    async fn lint_clean_when_no_pages() {
        let (u, _d) = ctx().await;
        let res = call_tool("lint", json!({}), &u).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&res[0].text).unwrap();
        assert_eq!(v["findings"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let (u, _d) = ctx().await;
        let err = call_tool("nope", json!({}), &u).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn create_page_validates_required_title() {
        let (u, _d) = ctx().await;
        let err = call_tool("create_page", json!({ "body": "x" }), &u)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
