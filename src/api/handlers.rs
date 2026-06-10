//! HTTP handlers. Each handler is a thin adapter: parse the request,
//! call a `core` / `auth` service scoped by the authenticated user, and
//! shape the response to match `docs/api.md`. Domain errors flow back as
//! [`AppError`] and are mapped to [`ApiError`] by the `?` operator.

use axum::extract::{Multipart, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::api::dto::*;
use crate::auth::middleware::AuthContext;
use crate::auth::UserService;
use crate::core::frontmatter::{self, Frontmatter};
use crate::core::index::IndexBuilder;
use crate::core::lint::Linter;
use crate::core::page::{PageFilter, PageService};
use crate::core::search::SearchService;
use crate::core::source::SourceService;
use crate::error::{ApiError, AppError};
use crate::storage::{event_repo, fs_layout, user_repo, AppState};

type ApiResult<T> = Result<T, ApiError>;

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Root landing page: a human-readable description of the service and how to
/// connect over REST, the CLI, and MCP. Served as HTML so it renders in a
/// browser; machine clients use `/healthz` and `/api/v1/*`. The configured
/// host:port is substituted into the page so copy-paste blocks point at the
/// running instance.
pub async fn root(State(state): State<AppState>) -> Response {
    let host = if state.config.host == "0.0.0.0" || state.config.host.is_empty() {
        "127.0.0.1".to_string()
    } else {
        state.config.host.clone()
    };
    let base = format!("http://{host}:{}", state.config.port);
    let html = LANDING_HTML.replace("__BASE__", &base);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
        .into_response()
}

const LANDING_HTML: &str = r###"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>mnemos</title>
<style>
  :root { color-scheme: light dark; }
  body { font: 15px/1.6 system-ui, -apple-system, sans-serif; max-width: 760px;
         margin: 3rem auto; padding: 0 1.2rem; }
  h1 { margin-bottom: .2rem; }
  .tag { color: #888; margin-top: 0; }
  code, pre { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
  pre { background: rgba(127,127,127,.12); padding: .8rem 1rem; border-radius: 8px;
        overflow-x: auto; }
  code { background: rgba(127,127,127,.15); padding: .1rem .35rem; border-radius: 4px; }
  pre code { background: none; padding: 0; }
  h2 { margin-top: 2rem; border-bottom: 1px solid rgba(127,127,127,.25); padding-bottom: .3rem; }
  table { border-collapse: collapse; width: 100%; }
  td, th { text-align: left; padding: .35rem .6rem; border-bottom: 1px solid rgba(127,127,127,.18); }
  .muted { color: #888; font-size: .9em; }
  .llm { border: 1px solid rgba(127,127,127,.3); border-radius: 10px; padding: 1rem 1.2rem;
         background: rgba(100,160,255,.08); margin-top: 1.5rem; }
  .llm-head { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
  .llm-head h2 { border: 0; margin: 0; }
  #copy-llm { font: inherit; cursor: pointer; border: 1px solid rgba(127,127,127,.4);
              border-radius: 7px; padding: .4rem .9rem; background: rgba(127,127,127,.1); }
  #copy-llm:hover { background: rgba(127,127,127,.2); }
  #llm-prompt { white-space: pre-wrap; margin-top: .8rem; max-height: 22rem; overflow-y: auto; }
</style>
</head>
<body>
<h1>mnemos</h1>
<p class="tag">Cloud memory for AI agents — a persistent wiki + sources + index,
exposed over REST, MCP, and a CLI.</p>

<p><strong>What it is.</strong> mnemos is a compounding knowledge layer between
an LLM agent and its raw sources. Agents read a source, write a short markdown
<em>page</em> that compresses its meaning, link pages with typed
<code>related:</code> edges, and cite sources by id. It is multi-tenant
(per-user namespaces), searchable (SQLite FTS5, BM25), and append-only logged.
It is intentionally dumb storage — the intelligence is the agent's.</p>

<div class="llm">
  <div class="llm-head">
    <h2>Copy this to your LLM — it builds a memory skill</h2>
    <button id="copy-llm" type="button">Copy</button>
  </div>
  <p class="muted">Paste this into your agent. It tells the model to create a
  reusable <strong>skill</strong> (a <code>SKILL.md</code>) that wires mnemos
  in as persistent memory, then start using it. Replace
  <code>YOUR_API_KEY</code> with a key from
  <code>POST /api/v1/auth/register</code> (or let the agent register one).</p>
  <pre id="llm-prompt"><code>Create a reusable "skill" that gives you persistent long-term memory backed by
an mnemos server, then start using it. mnemos is a per-user wiki of markdown
"pages" + immutable "sources", with FTS5 search, a typed `related:` graph, and
an append-only log.

SERVER:  __BASE__
API KEY: YOUR_API_KEY

mnemos is MCP-native — use the MCP tools, not raw HTTP. A skill is just text,
so it can't connect MCP by itself: STEP 1 registers the MCP server, STEP 2
writes the skill that drives it.

STEP 1 — Register the mnemos MCP server with your host. For Claude Code, add
this to .mcp.json (project) or ~/.claude/mcp.json (global), then reload so the
tools appear:

{
  "mcpServers": {
    "mnemos": {
      "command": "mnemos",
      "args": ["mcp"],
      "env": { "MNEMOS_API_URL": "__BASE__", "MNEMOS_API_KEY": "YOUR_API_KEY" }
    }
  }
}

(The `mnemos` binary must be on PATH. No API key yet? Register one:
POST __BASE__/api/v1/auth/register {"username","password"} returns an api_key.)

STEP 2 — Write the skill to  .claude/skills/mnemos-memory/SKILL.md  with EXACTLY
this content (YAML frontmatter + markdown body — the Claude Code / Agent skill
format):

---
name: mnemos-memory
description: Persistent long-term memory backed by an mnemos server, used via
  the mnemos MCP tools. Use BEFORE answering anything that may depend on prior
  knowledge (search_pages first), and AFTER learning something durable
  (create_page). Also for recall of past decisions, recipes, and facts.
---

# mnemos memory

Long-term memory lives in the `mnemos` MCP server (already registered in
.mcp.json). Always use its MCP tools — never call the HTTP API directly.

## When to use this skill
- RECALL: before answering, call `search_pages` for the topic, then `get_page`
  on the top hits.
- CAPTURE: after learning a durable fact, decision, or recipe, call
  `create_page` (or `update_page` to revise one).
- MAINTAIN: after writing, call `lint` and fix what it reports.

## MCP tools
list_pages, get_page, create_page, update_page, delete_page, search_pages,
list_sources, get_source, add_source_url, upload_source, get_index, get_log,
lint. Resources: mnemos://index, mnemos://log, mnemos://page/{slug},
mnemos://source/{id}.

create_page / update_page take: title, body, and a frontmatter object
{ tags:[...], page_type:"concept|recipe|reference|decision", scope:"global",
related:[] }.

## Rules
- Query memory first; cite the slug of every page you use.
- Never invent facts. Unknowns go under "## Open questions", disagreements under
  "## Conflicts". A page is a compressed claim with a citation, not a transcript.

STEP 3 — Verify by calling the `list_pages` MCP tool. If the tools are not
visible, the host hasn't loaded the server from STEP 1 — reload/restart it.

(Fallback only if your host cannot run MCP at all: the same operations exist
over REST at __BASE__/api/v1 with header `Authorization: Bearer YOUR_API_KEY` —
see __BASE__/ for details. Prefer MCP.)

Do this now, then confirm the MCP tools are available and the skill file exists.</code></pre>
</div>

<h2>Endpoints</h2>
<table>
<tr><th>Path</th><th>Purpose</th></tr>
<tr><td><code>GET /healthz</code></td><td>Liveness probe (no auth).</td></tr>
<tr><td><code>POST /api/v1/auth/register</code></td><td>Create a user, get first API key.</td></tr>
<tr><td><code>POST /api/v1/auth/login</code></td><td>Exchange username+password for a fresh key.</td></tr>
<tr><td><code>GET|POST /api/v1/pages</code></td><td>List / create wiki pages.</td></tr>
<tr><td><code>GET|PUT|DELETE /api/v1/pages/{slug}</code></td><td>Read / update / delete a page.</td></tr>
<tr><td><code>GET /api/v1/sources</code>, <code>POST .../url</code>, <code>POST .../upload</code></td><td>Manage immutable sources.</td></tr>
<tr><td><code>GET /api/v1/search?q=</code></td><td>FTS5 ranked search.</td></tr>
<tr><td><code>GET /api/v1/{index,log,lint}</code></td><td>Catalog, event log, health report.</td></tr>
</table>

<h2>Connect over REST</h2>
<p>All <code>/api/v1</code> routes need <code>Authorization: Bearer mnemo_…</code>.
Register once, save the key (it is shown only at creation):</p>
<pre><code># 1. register
curl -sS -X POST http://127.0.0.1:8090/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"correct horse battery staple"}'
# -> {"user_id":"…","api_key":"mnemo_…", …}

# 2. use the key
export KEY=mnemo_…
curl -sS http://127.0.0.1:8090/api/v1/pages -H "Authorization: Bearer $KEY"</code></pre>

<h2>Connect over the CLI</h2>
<pre><code>export MNEMOS_API_URL=http://127.0.0.1:8090
export MNEMOS_API_KEY=mnemo_…
mnemos pages list
mnemos search "vector search"
mnemos lint</code></pre>

<h2>Connect over MCP</h2>
<p>Point your agent host at the <code>mnemos mcp</code> stdio server. It runs
in-process against the same store, so the REST server is not required.</p>
<pre><code>{
  "mcpServers": {
    "mnemos": {
      "command": "mnemos",
      "args": ["mcp"],
      "env": {
        "MNEMOS_API_URL": "http://127.0.0.1:8090",
        "MNEMOS_API_KEY": "mnemo_…"
      }
    }
  }
}</code></pre>

<p class="muted">Full references: REST <code>docs/api.md</code>, CLI
<code>docs/cli.md</code>, MCP <code>docs/mcp.md</code>, page format
<code>docs/page-format.md</code>. Agent workflow contract: <code>AGENTS.md</code>.</p>
<script>
  (function () {
    var btn = document.getElementById('copy-llm');
    var pre = document.getElementById('llm-prompt');
    if (!btn || !pre) return;
    btn.addEventListener('click', function () {
      var text = pre.innerText;
      var done = function () { btn.textContent = 'Copied'; setTimeout(function () { btn.textContent = 'Copy'; }, 1500); };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(done, function () { fallback(text, done); });
      } else {
        fallback(text, done);
      }
    });
    function fallback(text, done) {
      var ta = document.createElement('textarea');
      ta.value = text; document.body.appendChild(ta); ta.select();
      try { document.execCommand('copy'); done(); } catch (e) {}
      document.body.removeChild(ta);
    }
  })();
</script>
</body>
</html>
"###;

pub async fn healthz() -> Response {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok",
            version: crate::VERSION,
            build_rev: crate::BUILD_REV,
        }),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<Response> {
    validate_api_username(&req.username).map_err(unprocessable)?;
    validate_api_password(&req.password).map_err(unprocessable)?;

    let svc = UserService::new(state);
    let (user, key) = svc.register(&req.username, &req.password).await?;
    let body = AuthResponse::new(user.id, user.username, key.id, key.plaintext);
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Response> {
    let svc = UserService::new(state);
    let (user, key) = svc.login(&req.username, &req.password).await?;
    let body = AuthResponse::new(user.id, user.username, key.id, key.plaintext);
    Ok((StatusCode::OK, Json(body)).into_response())
}

pub async fn whoami(State(state): State<AppState>, ctx: AuthContext) -> ApiResult<Response> {
    let user = user_repo::get_by_id(&state.db, &ctx.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("user".into()))?;
    Ok(Json(json!({
        "user_id": user.id,
        "username": user.username,
        "api_key_id": ctx.api_key_id,
    }))
    .into_response())
}

pub async fn list_keys(State(state): State<AppState>, ctx: AuthContext) -> ApiResult<Response> {
    let svc = UserService::new(state);
    let rows = svc.list_api_keys(&ctx.user_id).await?;
    let keys: Vec<KeyView> = rows.into_iter().map(KeyView::from).collect();
    Ok(Json(json!({ "keys": keys })).into_response())
}

pub async fn create_key(
    State(state): State<AppState>,
    ctx: AuthContext,
    Json(req): Json<CreateKeyRequest>,
) -> ApiResult<Response> {
    validate_key_name(&req.name).map_err(unprocessable)?;
    let svc = UserService::new(state);
    let key = svc.create_api_key(&ctx.user_id, &req.name).await?;
    let view = CreatedKeyView {
        view: KeyView {
            id: key.id,
            name: key.name,
            created_at: chrono::Utc::now(),
            last_used_at: None,
        },
        api_key: key.plaintext,
    };
    Ok((StatusCode::CREATED, Json(view)).into_response())
}

pub async fn revoke_key(
    State(state): State<AppState>,
    ctx: AuthContext,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let svc = UserService::new(state);
    // Idempotent: a missing id still returns 204 so callers cannot probe.
    svc.revoke_api_key(&ctx.user_id, &id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

pub async fn list_pages(
    State(state): State<AppState>,
    ctx: AuthContext,
    Query(q): Query<ListPagesQuery>,
) -> ApiResult<Response> {
    let limit = q
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);

    let filter = PageFilter {
        query: q.q.clone(),
        tag: q.tag.clone(),
        page_type: q.page_type.clone(),
        project: q.project.clone(),
        limit: None, // apply limit/offset after, so `total` is meaningful
    };
    let svc = PageService::new(state);
    let all = svc.list(&ctx.user_id, &filter).await?;
    let total = all.len();
    let items: Vec<PageSummary> = all
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(PageSummary::from)
        .collect();
    Ok(Json(json!({
        "pages": items,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))
    .into_response())
}

pub async fn create_page(
    State(state): State<AppState>,
    ctx: AuthContext,
    Json(req): Json<CreatePageRequest>,
) -> ApiResult<Response> {
    let body = req.body.unwrap_or_default();
    let mut fm = resolve_frontmatter(req.frontmatter, req.frontmatter_yaml)?;
    if let Some(title) = req.title {
        fm.title = Some(title);
    }
    let slug = req.slug.unwrap_or_default();

    let svc = PageService::new(state);
    let page = svc.create(&ctx.user_id, &slug, fm, &body).await?;
    Ok((StatusCode::CREATED, Json(page_ref(&page))).into_response())
}

pub async fn get_page(
    State(state): State<AppState>,
    ctx: AuthContext,
    Path(slug): Path<String>,
) -> ApiResult<Response> {
    let svc = PageService::new(state);
    let page = svc.get(&ctx.user_id, &slug).await?;
    Ok(Json(PageView::from(page)).into_response())
}

pub async fn update_page(
    State(state): State<AppState>,
    ctx: AuthContext,
    Path(slug): Path<String>,
    Json(req): Json<UpdatePageRequest>,
) -> ApiResult<Response> {
    let svc = PageService::new(state);
    let existing = svc.get(&ctx.user_id, &slug).await?;

    let mut fm = if req.frontmatter.is_some() || req.frontmatter_yaml.is_some() {
        let mut parsed = resolve_frontmatter(req.frontmatter, req.frontmatter_yaml)?;
        // Preserve the original creation date across full replacements.
        if parsed.created.is_none() {
            parsed.created = existing.frontmatter.created;
        }
        parsed
    } else {
        existing.frontmatter.clone()
    };
    if let Some(title) = req.title {
        fm.title = Some(title);
    }
    let body = req.body.unwrap_or(existing.body);

    let page = svc.update(&ctx.user_id, &slug, fm, &body).await?;
    Ok(Json(page_ref(&page)).into_response())
}

pub async fn delete_page(
    State(state): State<AppState>,
    ctx: AuthContext,
    Path(slug): Path<String>,
) -> ApiResult<Response> {
    let svc = PageService::new(state);
    // Idempotent: a missing page is still a 204.
    match svc.delete(&ctx.user_id, &slug).await {
        Ok(()) | Err(AppError::NotFound(_)) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => Err(e.into()),
    }
}

pub async fn get_page_raw(
    State(state): State<AppState>,
    ctx: AuthContext,
    Path(slug): Path<String>,
) -> ApiResult<Response> {
    let svc = PageService::new(state);
    let page = svc.get(&ctx.user_id, &slug).await?;
    let md = frontmatter::render(&page.frontmatter, &page.body)?;
    Ok(markdown(md))
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

pub async fn list_sources(State(state): State<AppState>, ctx: AuthContext) -> ApiResult<Response> {
    let svc = SourceService::new(state)?;
    let sources = svc.list(&ctx.user_id).await?;
    let views: Vec<SourceView> = sources.into_iter().map(SourceView::from).collect();
    Ok(Json(json!({ "sources": views })).into_response())
}

pub async fn add_source_url(
    State(state): State<AppState>,
    ctx: AuthContext,
    Json(req): Json<AddSourceUrlRequest>,
) -> ApiResult<Response> {
    let svc = SourceService::new(state)?;
    let slug = req.slug.unwrap_or_default();
    let source = svc.register_url(&ctx.user_id, &req.url, &slug).await?;
    Ok((StatusCode::CREATED, Json(SourceView::from(source))).into_response())
}

pub async fn upload_source(
    State(state): State<AppState>,
    ctx: AuthContext,
    mut multipart: Multipart,
) -> ApiResult<Response> {
    let mut slug = String::new();
    let mut content: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart: {e}")))?
    {
        match field.name() {
            Some("slug") => {
                slug = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("slug field: {e}")))?;
            }
            Some("file") => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("file field: {e}")))?;
                content = Some(bytes.to_vec());
            }
            _ => { /* ignore unknown fields */ }
        }
    }

    let content = content.ok_or_else(|| AppError::BadRequest("missing `file` field".into()))?;
    let svc = SourceService::new(state)?;
    let source = svc.upload(&ctx.user_id, &slug, &content).await?;
    Ok((StatusCode::CREATED, Json(SourceView::from(source))).into_response())
}

pub async fn get_source(
    State(state): State<AppState>,
    ctx: AuthContext,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let svc = SourceService::new(state)?;
    let source = svc.get(&ctx.user_id, &id).await?;
    Ok(Json(SourceView::from(source)).into_response())
}

pub async fn get_source_raw(
    State(state): State<AppState>,
    ctx: AuthContext,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let svc = SourceService::new(state)?;
    let raw = svc.get_raw(&ctx.user_id, &id).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        raw,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

pub async fn search(
    State(state): State<AppState>,
    ctx: AuthContext,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Response> {
    let query = q.q.unwrap_or_default();
    let limit = q.limit.unwrap_or(10).clamp(1, 100);
    let svc = SearchService::new(&state);
    let hits = svc.search(&ctx.user_id, &query, limit).await?;
    Ok(Json(json!({ "query": query, "hits": hits, "count": hits.len() })).into_response())
}

// ---------------------------------------------------------------------------
// Index / Log / Lint
// ---------------------------------------------------------------------------

pub async fn get_index(State(state): State<AppState>, ctx: AuthContext) -> ApiResult<Response> {
    IndexBuilder::new(state.clone())
        .rebuild_for_user(&ctx.user_id)
        .await?;
    let path = fs_layout::index_path(&state.config.data_dir, &ctx.user_id);
    let text = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(AppError::from(e).into()),
    };
    Ok(markdown(text))
}

pub async fn get_log(
    State(state): State<AppState>,
    ctx: AuthContext,
    Query(q): Query<LogQuery>,
) -> ApiResult<Response> {
    let since = match q.since.as_deref() {
        Some(s) => Some(
            chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|e| AppError::Validation(format!("invalid `since`: {e}")))?
                .with_timezone(&chrono::Utc),
        ),
        None => None,
    };
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let filter = event_repo::EventFilter {
        since,
        limit: Some(limit),
    };
    let rows = event_repo::list_for_user(&state.db, &ctx.user_id, &filter).await?;

    let format = q.format.as_deref().unwrap_or("json");
    if format == "md" || format == "markdown" {
        let mut out = String::new();
        for e in rows.iter().rev() {
            out.push_str(&crate::core::log::format_log_line(
                e.ts,
                &e.kind,
                e.ref_.as_deref(),
            ));
            out.push('\n');
        }
        Ok(markdown(out))
    } else {
        let events: Vec<LogEvent> = rows.into_iter().map(LogEvent::from).collect();
        let total = events.len();
        Ok(Json(LogJson {
            items: events,
            total,
        })
        .into_response())
    }
}

pub async fn lint(State(state): State<AppState>, ctx: AuthContext) -> ApiResult<Response> {
    let report = Linter::new(state).run(&ctx.user_id).await?;
    let summary = LintSummary::from_findings(&report.findings);
    Ok(Json(LintView {
        findings: report.findings,
        summary,
    })
    .into_response())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the `{ slug, id, created_at, updated_at }` reference object returned
/// by create/update.
fn page_ref(page: &crate::core::page::Page) -> serde_json::Value {
    json!({
        "slug": page.slug,
        "id": page.id,
        "created_at": page.created_at,
        "updated_at": page.updated_at,
    })
}

fn resolve_frontmatter(
    fm: Option<Frontmatter>,
    yaml: Option<String>,
) -> Result<Frontmatter, AppError> {
    match (fm, yaml) {
        (Some(fm), _) => Ok(fm),
        (None, Some(y)) => frontmatter::parse(&y),
        (None, None) => Ok(Frontmatter::default()),
    }
}

fn markdown(body: String) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        body,
    )
        .into_response()
}

fn unprocessable(msg: String) -> AppError {
    AppError::Unprocessable(msg)
}
