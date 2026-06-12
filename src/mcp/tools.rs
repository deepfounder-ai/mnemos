//! MCP tool surface — mirrors the REST API one-to-one.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::form_urlencoded;

use crate::mcp::{McpError, McpRestClient};

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
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

impl TextContent {
    pub fn text(s: impl Into<String>) -> Self {
        Self { kind: "text".into(), text: s.into(), mime_type: None }
    }
    pub fn markdown(s: impl Into<String>) -> Self {
        Self { kind: "text".into(), text: s.into(), mime_type: Some("text/markdown".into()) }
    }
    pub fn json(value: impl Serialize) -> Self {
        Self {
            kind: "text".into(),
            text: serde_json::to_string_pretty(&value).unwrap_or_default(),
            mime_type: Some("application/json".into()),
        }
    }
}

pub fn list_tools() -> Vec<Tool> {
    vec![
        tool("list_pages", "List pages in the wiki. Supports optional substring search, a single tag, multiple tags (all must match), a page_type filter, and a result limit.", json!({
            "type": "object",
            "properties": {
                "query":     { "type": "string", "description": "Substring filter on title, slug, or body." },
                "tag":       { "type": "string", "description": "Filter to pages carrying this tag." },
                "tags":      { "type": "array",  "items": { "type": "string" }, "description": "Filter to pages carrying ALL of these tags." },
                "page_type": { "type": "string", "enum": ["concept", "recipe", "reference", "decision"], "description": "Filter to a specific page_type." },
                "limit":     { "type": "integer", "minimum": 1, "maximum": 200, "description": "Maximum number of pages to return (default 50)." }
            },
            "additionalProperties": false,
        })),
        tool("get_page", "Fetch a single page by slug, including body, frontmatter, related links, and source refs.", json!({
            "type": "object",
            "properties": { "slug": { "type": "string", "description": "Page slug (kebab-case, max 80 chars)." } },
            "required": ["slug"],
            "additionalProperties": false,
        })),
        tool("create_page", "Create a new wiki page. If `slug` is empty, the server derives one from the title. `frontmatter` is the same shape as the on-disk frontmatter (title, tags, page_type, scope, related, sources).", json!({
            "type": "object",
            "properties": {
                "slug":         { "type": "string", "description": "Optional page slug. Auto-derived from title if empty." },
                "title":        { "type": "string", "description": "Page title. Required." },
                "body":         { "type": "string", "description": "Page body in markdown." },
                "frontmatter":  frontmatter_schema_ref()
            },
            "required": ["title"],
            "additionalProperties": false,
        })),
        tool("update_page", "Update an existing page. Any of `body`, `title`, `frontmatter` may be omitted (omit = unchanged). The `slug` is always preserved — to rename, delete and re-create.", json!({
            "type": "object",
            "properties": {
                "slug":         { "type": "string", "description": "Slug of the page to update." },
                "title":        { "type": "string" },
                "body":         { "type": "string" },
                "frontmatter":  frontmatter_schema_ref()
            },
            "required": ["slug"],
            "additionalProperties": false,
        })),
        tool("delete_page", "Delete a page by slug. Idempotent — returns ok even if the page didn't exist.", json!({
            "type": "object",
            "properties": { "slug": { "type": "string" } },
            "required": ["slug"],
            "additionalProperties": false,
        })),
        tool("search_pages", "Full-text search over the wiki using FTS5 BM25 ranking. Returns ranked hits with slug, title, and snippet.", json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query (FTS5 syntax; multi-word is OR-joined by default)." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 10 }
            },
            "required": ["query"],
            "additionalProperties": false,
        })),
        tool("list_sources", "List all raw sources registered for the user (URL fetches and uploads).", json!({
            "type": "object", "properties": {}, "additionalProperties": false,
        })),
        tool("get_source", "Fetch source metadata by `id` (the opaque source id, not the slug). Use `mnemos://source/{id}` for the raw body.", json!({
            "type": "object",
            "properties": { "id": { "type": "string", "description": "Opaque source id." } },
            "required": ["id"],
            "additionalProperties": false,
        })),
        tool("add_source_url", "Fetch a URL and store the response body as an immutable source. Returns the new source's metadata.", json!({
            "type": "object",
            "properties": {
                "url":  { "type": "string", "description": "http(s) URL to fetch." },
                "slug": { "type": "string", "description": "Optional slug for the source (auto-derived from the URL path otherwise)." }
            },
            "required": ["url"],
            "additionalProperties": false,
        })),
        tool("upload_source", "Upload raw content as a new source. `content` is base64-encoded. Returns the new source's metadata.", json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "Base64-encoded file content." },
                "slug":    { "type": "string", "description": "Optional slug for the source (defaults to 'upload')." }
            },
            "required": ["content"],
            "additionalProperties": false,
        })),
        tool("get_index", "Return the auto-generated catalog (index.md) for the user as markdown.", json!({
            "type": "object", "properties": {}, "additionalProperties": false,
        })),
        tool("get_log", "Return the append-only event log as markdown. Optional `since` (RFC3339) and `limit` filter.", json!({
            "type": "object",
            "properties": {
                "since": { "type": "string", "description": "RFC3339 timestamp; only events strictly after this are returned." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 100 }
            },
            "additionalProperties": false,
        })),
        tool("lint", "Run the wiki linter. Returns findings (orphan pages, broken `related:` links, missing source refs) and a summary by severity.", json!({
            "type": "object", "properties": {}, "additionalProperties": false,
        })),
    ]
}

fn tool(name: &str, description: &str, mut input_schema: Value) -> Tool {
    if let Some(obj) = input_schema.as_object_mut() {
        let needs_fm_def = obj
            .values()
            .any(|v| v.get("$ref").and_then(|r| r.as_str()) == Some("#/$defs/frontmatter"));
        if needs_fm_def {
            let mut defs = serde_json::Map::new();
            defs.insert("frontmatter".into(), frontmatter_schema_def());
            obj.insert("$defs".into(), Value::Object(defs));
        }
    }
    Tool {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
    }
}

fn frontmatter_schema_ref() -> Value {
    json!({ "$ref": "#/$defs/frontmatter" })
}

fn frontmatter_schema_def() -> Value {
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

pub async fn call_tool(
    name: &str,
    args: Value,
    client: &McpRestClient,
) -> Result<Vec<TextContent>, McpError> {
    match name {
        "list_pages" => tool_list_pages(args, client).await,
        "get_page" => tool_get_page(args, client).await,
        "create_page" => tool_create_page(args, client).await,
        "update_page" => tool_update_page(args, client).await,
        "delete_page" => tool_delete_page(args, client).await,
        "search_pages" => tool_search_pages(args, client).await,
        "list_sources" => tool_list_sources(client).await,
        "get_source" => tool_get_source(args, client).await,
        "add_source_url" => tool_add_source_url(args, client).await,
        "upload_source" => tool_upload_source(args, client).await,
        "get_index" => tool_get_index(client).await,
        "get_log" => tool_get_log(args, client).await,
        "lint" => tool_lint(client).await,
        other => Err(McpError::InvalidArgument(format!("unknown tool '{other}'"))),
    }
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, McpError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(other) => Err(McpError::InvalidArgument(format!(
            "{key} must be a string, got {other}"
        ))),
    }
}

fn arg_str_required<'a>(args: &'a Value, key: &str) -> Result<&'a str, McpError> {
    arg_str(args, key)?
        .ok_or_else(|| McpError::InvalidArgument(format!("{key} is required")))
}

fn arg_i64_opt(args: &Value, key: &str) -> Result<Option<i64>, McpError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_i64().map(Some).ok_or_else(|| {
            McpError::InvalidArgument(format!("{key} must be an integer, got {n}"))
        }),
        Some(other) => Err(McpError::InvalidArgument(format!(
            "{key} must be an integer, got {other}"
        ))),
    }
}

fn arg_tags(args: &Value) -> Result<Option<Vec<String>>, McpError> {
    match args.get("tags") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) => {
            let mut out: Vec<String> = Vec::with_capacity(arr.len());
            for v in arr {
                match v {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(McpError::InvalidArgument(format!(
                            "tags must be an array of strings, got {other}"
                        )))
                    }
                }
            }
            Ok(Some(out))
        }
        Some(other) => Err(McpError::InvalidArgument(format!(
            "tags must be an array, got {other}"
        ))),
    }
}

fn arg_frontmatter(args: &Value) -> Result<Option<Value>, McpError> {
    match args.get("frontmatter") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let v: Value = serde_json::from_str(s).map_err(|e| {
                McpError::InvalidArgument(format!("frontmatter (string) is not valid JSON: {e}"))
            })?;
            Ok(Some(v))
        }
        Some(v @ Value::Object(_)) => Ok(Some(v.clone())),
        Some(other) => Err(McpError::InvalidArgument(format!(
            "frontmatter must be an object, got {other}"
        ))),
    }
}

fn build_query<'a, I>(parts: I) -> String
where
    I: IntoIterator<Item = (&'a str, Option<&'a str>)>,
{
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (k, v) in parts {
        if let Some(val) = v {
            if !val.is_empty() {
                serializer.append_pair(k, val);
            }
        }
    }
    serializer.finish()
}

async fn tool_list_pages(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let query = arg_str(&args, "query")?;
    let tag = arg_str(&args, "tag")?;
    let tags = arg_tags(&args)?;
    let page_type = arg_str(&args, "page_type")?;
    let limit: Option<String> = arg_i64_opt(&args, "limit")?.map(|n: i64| n.to_string());

    let primary_tag: Option<String> = match tag.map(str::to_string) {
        Some(t) => Some(t),
        None => tags.as_ref().and_then(|v: &Vec<String>| v.first().cloned()),
    };

    let query_str = build_query([
        ("q", query),
        ("tag", primary_tag.as_deref()),
        ("type", page_type),
        ("limit", limit.as_deref()),
    ]);
    let path = if query_str.is_empty() {
        "/api/v1/pages".to_string()
    } else {
        format!("/api/v1/pages?{query_str}")
    };
    let v: Value = client.get(&path).await?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_get_page(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let slug = arg_str_required(&args, "slug")?;
    let path = format!("/api/v1/pages/{}", urlencoded(slug));
    let v: Value = client.get(&path).await?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_create_page(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let title = arg_str(&args, "title")?.map(str::to_string);
    let body = arg_str(&args, "body")?.map(str::to_string);
    let slug = arg_str(&args, "slug")?.map(str::to_string);
    let frontmatter = arg_frontmatter(&args)?;

    let mut payload = serde_json::Map::new();
    if let Some(t) = title {
        payload.insert("title".into(), Value::String(t));
    } else {
        return Err(McpError::InvalidArgument(
            "create_page requires a `title`".into(),
        ));
    }
    if let Some(b) = body {
        payload.insert("body".into(), Value::String(b));
    }
    if let Some(s) = slug {
        payload.insert("slug".into(), Value::String(s));
    }
    if let Some(fm) = frontmatter {
        payload.insert("frontmatter".into(), fm);
    }
    let v: Value = client
        .post_value("/api/v1/pages", &Value::Object(payload))
        .await?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_update_page(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let slug = arg_str_required(&args, "slug")?.to_string();
    let title = arg_str(&args, "title")?.map(str::to_string);
    let body = arg_str(&args, "body")?.map(str::to_string);
    let frontmatter = arg_frontmatter(&args)?;

    let mut payload = serde_json::Map::new();
    if let Some(t) = title {
        payload.insert("title".into(), Value::String(t));
    }
    if let Some(b) = body {
        payload.insert("body".into(), Value::String(b));
    }
    if let Some(fm) = frontmatter {
        payload.insert("frontmatter".into(), fm);
    }
    let path = format!("/api/v1/pages/{}", urlencoded(&slug));
    let v: Value = client
        .put_value(&path, &Value::Object(payload))
        .await?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_delete_page(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let slug = arg_str_required(&args, "slug")?.to_string();
    let path = format!("/api/v1/pages/{}", urlencoded(&slug));
    client.delete(&path).await?;
    Ok(vec![TextContent::json(json!({
        "ok": true,
        "slug": slug,
    }))])
}

async fn tool_search_pages(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let query = arg_str_required(&args, "query")?;
    let limit: Option<String> = arg_i64_opt(&args, "limit")?.map(|n: i64| n.to_string());
    let query_str = build_query([
        ("q", Some(query)),
        ("limit", limit.as_deref()),
    ]);
    let v: Value = client
        .get(&format!("/api/v1/search?{query_str}"))
        .await?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_list_sources(client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let v: Value = client.get("/api/v1/sources").await?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_get_source(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let id = arg_str_required(&args, "id")?;
    let path = format!("/api/v1/sources/{}", urlencoded(id));
    let v: Value = client.get(&path).await?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_add_source_url(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let url = arg_str_required(&args, "url")?.to_string();
    let slug = arg_str(&args, "slug")?.map(str::to_string);
    let payload = match slug {
        Some(s) => json!({ "url": url, "slug": s }),
        None => json!({ "url": url }),
    };
    let v: Value = client.post_value("/api/v1/sources/url", &payload).await?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_upload_source(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    use base64::Engine;

    let content_b64 = arg_str_required(&args, "content")?.to_string();
    let slug = arg_str(&args, "slug")?.map(str::to_string);

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content_b64.as_bytes())
        .map_err(|e| McpError::InvalidArgument(format!("content must be base64: {e}")))?;

    let part = reqwest::multipart::Part::bytes(bytes).file_name("upload".to_string());
    let mut form = reqwest::multipart::Form::new().part("file", part);
    if let Some(s) = slug.clone() {
        form = form.text("slug", s);
    }
    let url = format!(
        "{}/api/v1/sources/upload",
        client.base_url().trim_end_matches('/')
    );
    let resp: reqwest::Response = client.http_post_multipart(url, form).await?;
    if !resp.status().is_success() {
        return Err(McpError::UnexpectedStatus {
            status: resp.status().as_u16(),
            body: resp.text().await.unwrap_or_default(),
        });
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| McpError::Unreachable(format!("decode response: {e}")))?;
    Ok(vec![TextContent::json(v)])
}

async fn tool_get_index(client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let text = client.get_text("/api/v1/index").await?;
    Ok(vec![TextContent::markdown(text)])
}

async fn tool_get_log(args: Value, client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let since = arg_str(&args, "since")?;
    let limit: Option<String> = arg_i64_opt(&args, "limit")?.map(|n: i64| n.to_string());
    let query_str = build_query([
        ("since", since),
        ("limit", limit.as_deref()),
        ("format", Some("md")),
    ]);
    let path = if query_str.is_empty() {
        "/api/v1/log".to_string()
    } else {
        format!("/api/v1/log?{query_str}")
    };
    let text = client.get_text(&path).await?;
    Ok(vec![TextContent::markdown(text)])
}

async fn tool_lint(client: &McpRestClient) -> Result<Vec<TextContent>, McpError> {
    let v: Value = client.get("/api/v1/lint").await?;
    Ok(vec![TextContent::json(v)])
}

fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

impl McpRestClient {
    pub(crate) async fn http_post_multipart(
        &self,
        url: String,
        form: reqwest::multipart::Form,
    ) -> Result<reqwest::Response, McpError> {
        use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
        let mut headers = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(&format!("Bearer {}", self.api_key)) {
            headers.insert(AUTHORIZATION, v);
        }
        self.http
            .post(&url)
            .headers(headers)
            .multipart(form)
            .send()
            .await
            .map_err(|e: reqwest::Error| McpError::Unreachable(e.to_string()))
    }
}
