# REST API Reference

> mnemos HTTP API. All paths are under `/api/v1` unless otherwise noted.
> Base URL: `http://localhost:8080` (default; override with `MNEMOS_HOST` /
> `MNEMOS_PORT` env vars or `mnemos serve --host --port`).

This document is the source of truth for the wire protocol. The MCP server
(`docs/mcp.md`) and CLI (`docs/cli.md`) are thin wrappers around these
endpoints.

---

## Conventions

- **Content type.** Request and response bodies are `application/json` unless
  noted. Multipart only for `POST /api/v1/sources/upload`.
- **Charset.** UTF-8.
- **Timestamps.** ISO-8601 in UTC, e.g. `2026-04-17T12:34:56Z`.
- **IDs.** String (UUID v7). Generated server-side.
- **Authentication.** Bearer token in `Authorization: Bearer mnemo_…` header.
  The token is the API key shown once at creation; the server stores only
  the SHA-256 of the key.
- **Errors.** Non-2xx responses return a JSON object:
  ```json
  { "error": { "code": "string_code", "message": "human readable" } }
  ```
  Standard HTTP status codes are used. See [Errors](#errors).

---

## Authentication

### `POST /api/v1/auth/register`

Create a new user. Returns an initial API key.

**Request**

```json
{ "username": "alice", "password": "correct horse battery staple" }
```

**Response 201**

```json
{
  "user_id": "0190a4d6-2c0e-7abc-9def-0123456789ab",
  "api_key": "mnemo_5p9XKqV3yH8w2nQrJ7tL4bN1mZ8sDf0a"
}
```

> The `api_key` is **only returned at creation**. The server stores only
> the SHA-256 hash. If you lose it, create a new one with
> `POST /api/v1/auth/keys` — there is no recovery flow.

**Errors**

- `409 username_taken` — the username already exists.
- `422 invalid_input` — username/password fail validation (length, charset).

### `POST /api/v1/auth/login`

Exchange username + password for a new API key. Use sparingly — prefer
keeping long-lived keys per agent. Returns a fresh key on every call.

**Request** — same as `register`.

**Response 200** — same as `register`.

**Errors**

- `401 invalid_credentials`.

### `GET /api/v1/auth/keys`

List API keys for the authenticated user. **Does not return the keys**
themselves, only metadata.

**Response 200**

```json
{
  "keys": [
    {
      "id": "0190a4d6-2c0e-7abc-9def-0123456789ab",
      "name": "claude-code-laptop",
      "created_at": "2026-04-17T12:34:56Z",
      "last_used_at": "2026-04-17T15:02:11Z"
    }
  ]
}
```

### `POST /api/v1/auth/keys`

Create a new API key (e.g. for a second agent or device).

**Request**

```json
{ "name": "cursor-workstation" }
```

**Response 201**

```json
{
  "id": "0190a4d6-2c0e-7abc-9def-0123456789ab",
  "api_key": "mnemo_5p9XKqV3yH8w2nQrJ7tL4bN1mZ8sDf0a"
}
```

### `DELETE /api/v1/auth/keys/{id}`

Revoke a key. Idempotent — returns 204 even if the id does not exist
(the caller cannot probe for key ids).

---

## Pages

### `GET /api/v1/pages`

List pages. Supports filtering and search.

**Query parameters**

| Name  | Type   | Default | Notes                                  |
| ----- | ------ | ------- | -------------------------------------- |
| `tag` | string | —       | Repeatable. AND-filter across tags.    |
| `type`| enum   | —       | One of `concept`, `recipe`, `reference`, `decision`. |
| `q`   | string | —       | FTS5 query over `title`, `body`, `tags`. |
| `limit` | int  | 50      | Max 200.                              |
| `offset` | int | 0       |                                        |

**Response 200**

```json
{
  "pages": [
    {
      "slug": "llm-wiki",
      "title": "LLM wiki pattern",
      "page_type": "concept",
      "scope": "global",
      "tags": ["llm", "knowledge-management"],
      "created": "2026-04-17",
      "updated": "2026-04-17",
      "excerpt": "Persistent, compounding knowledge layer between an LLM agent and its raw sources…"
    }
  ],
  "total": 1,
  "limit": 50,
  "offset": 0
}
```

### `POST /api/v1/pages`

Create a page.

**Request**

```json
{
  "slug": "llm-wiki",
  "title": "LLM wiki pattern",
  "body": "# body markdown\n…",
  "frontmatter": {
    "tags": ["llm", "knowledge-management"],
    "created": "2026-04-17",
    "updated": "2026-04-17",
    "sources": [
      { "type": "url", "ref": "abc12345-karpathy-llm-wiki", "origin": "https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f" }
    ],
    "scope": "global",
    "page_type": "concept",
    "related": []
  }
}
```

`slug` is required and must match `^[a-z0-9][a-z0-9-]{0,78}[a-z0-9]$`.
`title` is required. `frontmatter` must include all required fields
(see `docs/page-format.md`).

**Response 201**

```json
{
  "slug": "llm-wiki",
  "id": "0190a4d6-2c0e-7abc-9def-0123456789ab",
  "created_at": "2026-04-17T12:34:56Z",
  "updated_at": "2026-04-17T12:34:56Z"
}
```

**Errors**

- `409 slug_taken`.
- `422 invalid_frontmatter` — see error `details` for the field that failed.
- `422 unknown_source` — a `sources[].ref` does not resolve to a source owned
  by this user.

### `GET /api/v1/pages/{slug}`

Fetch a single page (frontmatter + body).

**Response 200**

```json
{
  "slug": "llm-wiki",
  "title": "LLM wiki pattern",
  "body": "# body markdown\n…",
  "frontmatter": { "...": "..." },
  "created_at": "2026-04-17T12:34:56Z",
  "updated_at": "2026-04-17T12:34:56Z"
}
```

**Errors**

- `404 page_not_found`.

### `PUT /api/v1/pages/{slug}`

Update a page. `created` in the frontmatter is preserved. `updated` is
bumped to today if not explicitly provided.

**Request** — same shape as `POST /api/v1/pages`, minus `slug`. Both
`body` and `frontmatter` are required, and the body replaces the existing
one in full (no PATCH semantics in MVP).

**Response 200** — same as `POST /api/v1/pages`.

### `DELETE /api/v1/pages/{slug}`

Delete a page. Idempotent. Does not cascade to sources. The `related:`
lists of other pages that pointed here will be flagged by the next
`GET /api/v1/lint` run.

**Response 204.**

### `GET /api/v1/pages/{slug}/raw`

Return the raw markdown (frontmatter + body) as `text/markdown; charset=utf-8`.
Use this when you want the on-disk file shape (e.g. for diffing or for
editing locally and re-uploading).

**Response 200** — `text/markdown` body.

### `GET /api/v1/index`

Return the auto-generated `index.md` for the user. The service rebuilds
this on every page mutation; this endpoint is read-only.

**Response 200** — `text/markdown` body.

### `GET /api/v1/log`

Return the append-only event log. The structured form is JSON by default;
pass `?format=md` for the human-readable markdown form (one event per
line, prefixed with `[YYYY-MM-DDTHH:MM:SSZ]`).

**Query parameters**

| Name   | Type   | Default | Notes                              |
| ------ | ------ | ------- | ---------------------------------- |
| `format` | enum  | `json`  | `json` or `md`.                    |
| `since` | string | —       | ISO-8601 timestamp; return events strictly after. |
| `limit` | int   | 100     | Max 1000.                          |

**Response 200 (json)**

```json
{
  "events": [
    {
      "ts": "2026-04-17T12:34:56Z",
      "kind": "page.created",
      "ref": "llm-wiki",
      "payload": { "title": "LLM wiki pattern" }
    }
  ]
}
```

---

## Sources

### `GET /api/v1/sources`

List sources owned by the authenticated user.

**Response 200**

```json
{
  "sources": [
    {
      "id": "abc12345-karpathy-llm-wiki",
      "slug": "karpathy-llm-wiki",
      "type": "url",
      "origin": "https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f",
      "content_hash": "f2ca1bb6c7e907d06dafe4687e579fce76b37e4e93b7605022da52e6ccc26fd2",
      "bytes": 4321,
      "created_at": "2026-04-17T12:00:00Z"
    }
  ]
}
```

### `POST /api/v1/sources/url`

Fetch a URL server-side and store the body as a source.

**Request**

```json
{ "url": "https://example.com/article.html", "slug": "example-article" }
```

`url` must be `http://` or `https://`. The server enforces a 30 s timeout
and a 10 MB body cap. Non-2xx upstream responses surface as
`502 upstream_fetch_failed`.

**Response 201** — same shape as a single entry in `GET /api/v1/sources`.

**Errors**

- `422 invalid_url`.
- `413 source_too_large`.
- `504 upstream_timeout`.
- `502 upstream_fetch_failed`.

### `POST /api/v1/sources/upload`

Upload a file as a source. Multipart form:

```
POST /api/v1/sources/upload
Authorization: Bearer mnemo_…
Content-Type: multipart/form-data; boundary=…

--…
Content-Disposition: form-data; name="slug"

my-source
--…
Content-Disposition: form-data; name="file"; filename="notes.md"
Content-Type: text/markdown

…file body…
--…--
```

**Response 201** — same as a single entry in `GET /api/v1/sources`.

**Errors**

- `413 source_too_large` (> 10 MB).
- `422 invalid_slug`.

### `GET /api/v1/sources/{id}`

Get source metadata (not the body).

**Response 200** — single entry from `GET /api/v1/sources`.

### `GET /api/v1/sources/{id}/raw`

Get the raw source body. Content type is best-effort (`text/plain`,
`text/markdown`, `text/html`, `application/octet-stream` for unknown).

**Response 200** — body bytes.

---

## Lint

### `GET /api/v1/lint`

Run a linter over the user's namespace and return a structured report.

**Response 200**

```json
{
  "findings": [
    {
      "severity": "warning",
      "kind": "broken_related",
      "ref": "deploy-mnemos",
      "message": "related[] entry 'old-recipe-name' does not resolve to a page."
    },
    {
      "severity": "info",
      "kind": "orphan_page",
      "ref": "llm-wiki",
      "message": "Page has no related[] entries and is not referenced by any other page."
    }
  ],
  "summary": { "errors": 0, "warnings": 1, "info": 1 }
}
```

Finding kinds:

- `broken_related` — `related[]` entry does not resolve.
- `orphan_page` — page has no `related[]` and is not in any other page's
  `related[]`.
- `missing_source` — `sources[].ref` does not resolve to a source owned
  by this user.
- `asymmetric_related` — A lists B as related, but B does not list A.
- `stale_source` — `content_hash` no longer matches (sources are
  immutable in MVP, so this fires only on data corruption).

---

## Health

### `GET /healthz`

Liveness probe. Always returns `200 OK` if the process is up and the
event loop is responsive. Does not check the database.

**Response 200** — `text/plain` body `ok`.

### `GET /readyz` (planned for v0.2)

Readiness probe — checks DB connectivity and pending migrations.

---

## Errors

| Status | Code                  | Meaning                                                   |
| ------ | --------------------- | --------------------------------------------------------- |
| 400    | `bad_request`         | Malformed JSON, wrong content type, etc.                  |
| 401    | `unauthorized`        | Missing or invalid `Authorization` header.               |
| 403    | `forbidden`           | Authenticated, but the resource belongs to another user.  |
| 404    | `not_found`           | Resource does not exist.                                  |
| 409    | `conflict`            | Slug, username, or key name collision.                    |
| 413    | `payload_too_large`   | Request body or uploaded source exceeds the cap.          |
| 422    | `invalid_input`       | Validation failed. See `details` for which field.         |
| 500    | `internal`            | Unexpected server error. Logged with a request id.        |
| 502    | `upstream_fetch_failed` | Source URL fetch returned non-2xx.                      |
| 504    | `upstream_timeout`    | Source URL fetch timed out.                               |

Validation errors include a `details` field with field-level errors:

```json
{
  "error": {
    "code": "invalid_frontmatter",
    "message": "Frontmatter validation failed.",
    "details": [
      { "field": "page_type", "issue": "must be one of: concept, recipe, reference, decision" }
    ]
  }
}
```

---

## Worked examples

### Register, create a page, search

```bash
# 1. Register (one time per user)
curl -sS -X POST http://localhost:8080/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"hunter2hunter2"}'
# → {"user_id":"…","api_key":"mnemo_…"}

# Save it.
export MNEMOS_KEY=mnemo_…

# 2. Create a page
curl -sS -X POST http://localhost:8080/api/v1/pages \
  -H "Authorization: Bearer $MNEMOS_KEY" \
  -H 'Content-Type: application/json' \
  -d @page.json

# 3. Search
curl -sS "http://localhost:8080/api/v1/pages?q=wiki&type=concept" \
  -H "Authorization: Bearer $MNEMOS_KEY"
```

`page.json`:

```json
{
  "slug": "llm-wiki",
  "title": "LLM wiki pattern",
  "body": "## Key points\n- persistent layer…\n",
  "frontmatter": {
    "tags": ["llm", "knowledge-management"],
    "created": "2026-04-17",
    "updated": "2026-04-17",
    "sources": [
      { "type": "url", "ref": "abc12345-karpathy-llm-wiki",
        "origin": "https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f" }
    ],
    "scope": "global",
    "page_type": "concept",
    "related": []
  }
}
```

### Add a source by URL, then reference it from a page

```bash
# 1. Add the source
curl -sS -X POST http://localhost:8080/api/v1/sources/url \
  -H "Authorization: Bearer $MNEMOS_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/article","slug":"example-article"}'
# → {"id":"abc12345-example-article", …}

# 2. Reference it from a new page (use the returned id in frontmatter.sources[].ref)
```

### Lint, then fix findings

```bash
curl -sS http://localhost:8080/api/v1/lint \
  -H "Authorization: Bearer $MNEMOS_KEY" | jq
```

The agent's job is to read findings, decide which are real, and either
fix them (page update, source upload) or annotate them in the affected
page's `## Open questions` section if the fix is not yet known.
