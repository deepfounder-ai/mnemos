# MCP Server Reference

> The mnemos MCP (Model Context Protocol) server. Exposes the same
> surface as the REST API, but in a form designed for LLM agents.

The MCP server is the **primary interface for agents**. REST and CLI exist
for humans and automation; the agent should prefer MCP whenever it is
available.

---

## Transport

**stdio only.** The server reads newline-delimited JSON-RPC 2.0 messages
from stdin and writes responses to stdout. Logging goes to stderr so it
does not pollute the protocol stream.

Typical launch:

```bash
mnemos mcp
```

The MCP server runs **in-process**: it opens the same SQLite + filesystem
store the REST server uses, directly — there is no upstream HTTP hop. It is
configured by environment variables (below), like any other MCP process in
your host.

---

## Authentication

The server needs two environment variables when spawned:

- `MNEMOS_DATA_DIR` — the data directory of the store to serve (must match the
  REST server's, default `./data`).
- `MNEMOS_API_KEY` — an API key (`mnemo_…`); the server resolves it against
  that store to pick the user whose namespace it serves.

`MNEMOS_API_URL` is **not** used by the MCP server (it only matters to the
REST/CLI HTTP client). The key is resolved once at startup; rotate it by
restarting the process with a new `MNEMOS_API_KEY`.

---

## Tools

All tools are listed under the MCP `tools/list` response. Their
`inputSchema` follows JSON Schema. Below is the full surface with the
canonical argument shapes.

### Pages

#### `list_pages`

List pages with optional filtering.

| Argument | Type   | Default | Notes                                  |
| -------- | ------ | ------- | -------------------------------------- |
| `query`  | string | —       | FTS5 query (over `title`, `body`, `tags`). |
| `tags`   | array of string | — | AND-filter.                  |
| `type`   | enum   | —       | `concept`, `recipe`, `reference`, `decision`. |
| `limit`  | int    | 50      | Max 200.                               |
| `offset` | int    | 0       |                                        |

Returns an array of page summaries (slug, title, page_type, scope,
tags, dates, excerpt).

#### `get_page`

Fetch a single page with full frontmatter and body.

| Argument | Type   | Required | Notes |
| -------- | ------ | -------- | ----- |
| `slug`   | string | yes      |       |

Returns the full page object.

#### `create_page`

Create a new page.

| Argument      | Type   | Required | Notes                                  |
| ------------- | ------ | -------- | -------------------------------------- |
| `slug`        | string | yes      | kebab-case, see `docs/api.md`.         |
| `title`       | string | yes      |                                        |
| `body`        | string | yes      | Markdown.                              |
| `frontmatter` | object | yes      | Must satisfy `docs/page-format.md`.    |

Returns `{ slug, id, created_at, updated_at }`.

#### `update_page`

Update an existing page. Both `body` and `frontmatter` are required; the
update is a full replacement, not a patch.

| Argument      | Type   | Required | Notes                                  |
| ------------- | ------ | -------- | -------------------------------------- |
| `slug`        | string | yes      | Path-style identifier.                 |
| `body`        | string | yes      |                                        |
| `frontmatter` | object | yes      |                                        |

#### `delete_page`

| Argument | Type   | Required | Notes |
| -------- | ------ | -------- | ----- |
| `slug`   | string | yes      |       |

#### `search_pages`

FTS5 search. Equivalent to `list_pages` with `query` set, but returns
richer excerpts.

| Argument | Type   | Default | Notes                  |
| -------- | ------ | ------- | ---------------------- |
| `query`  | string | —       | Required.              |
| `limit`  | int    | 20      | Max 100.               |

### Sources

#### `list_sources`

List sources for the user. No arguments.

#### `get_source`

Get source metadata.

| Argument | Type   | Required | Notes                  |
| -------- | ------ | -------- | ---------------------- |
| `id`     | string | yes      | The `source_id`.       |

#### `add_source_url`

Fetch a URL server-side and store as a source.

| Argument | Type   | Required | Notes                  |
| -------- | ------ | -------- | ---------------------- |
| `url`    | string | yes      | `http://` or `https://`. |
| `slug`   | string | yes      | Used for the source slug. |

#### `upload_source`

Upload raw content as a source. The body is the literal bytes
(markdown, plain text, HTML, etc.) — pass it as a string. For binary
content, base64-encode and decode on the server.

| Argument  | Type   | Required | Notes                  |
| --------- | ------ | -------- | ---------------------- |
| `content` | string | yes      |                        |
| `slug`    | string | yes      |                        |

### Index, log, lint

#### `get_index`

Returns the full `index.md` for the user as a string.

#### `get_log`

Returns the event log.

| Argument | Type   | Default | Notes                  |
| -------- | ------ | ------- | ---------------------- |
| `since`  | string | —       | ISO-8601 timestamp.    |
| `limit`  | int    | 100     |                        |

#### `lint`

Run the linter and return a structured findings report. No arguments.

---

## Resources

Read-only MCP resources, addressed by URI.

| URI                          | Content                          |
| ---------------------------- | -------------------------------- |
| `mnemos://index`             | The user's auto-generated `index.md`. |
| `mnemos://log`               | The user's append-only event log. |
| `mnemos://page/{slug}`       | A page (frontmatter + body) as a single markdown string. |
| `mnemos://source/{id}`       | A source's raw body.             |

Resources are streamed on demand; the server does not hold a copy
beyond the request lifetime.

---

## Prompts

Reserved for v0.2. The reserved names are:

- `ingest-source` — a workflow guide the host can render into the
  context to remind the agent of the ingest procedure.
- `lint-wiki` — same idea for the maintenance workflow.

Do not rely on these in v0.1.

---

## Connecting from popular hosts

The configuration shape is `"command"` + `"args"` + `"env"`. Below are
worked examples for the most common hosts.

### Claude Code

`~/.claude/mcp_servers.json` (or project-local `.mcp.json`):

```json
{
  "mcpServers": {
    "mnemos": {
      "command": "mnemos",
      "args": ["mcp"],
      "env": {
        "MNEMOS_DATA_DIR": "/path/to/mnemos/data",
        "MNEMOS_API_KEY": "mnemo_…"
      }
    }
  }
}
```

### Cursor

`~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "mnemos": {
      "command": "mnemos",
      "args": ["mcp"],
      "env": {
        "MNEMOS_DATA_DIR": "/path/to/mnemos/data",
        "MNEMOS_API_KEY": "mnemo_…"
      }
    }
  }
}
```

### Codex / OpenCode / Aider

These hosts accept a similar JSON shape — see the host's MCP docs. The
mnemos side is the same: spawn `mnemos mcp` with the two env vars.

### Verifying the connection

From the host's UI, list MCP tools. You should see all the tools in
[Tools](#tools). As a smoke test, call `list_pages` with no arguments —
the response is an empty array on a fresh user, or a list of summaries
otherwise.

---

## Example session (pseudocode)

```
agent → tools/call list_pages
  ← {"pages": [], "total": 0}

agent → tools/call add_source_url
  { "url": "https://example.com/article", "slug": "example-article" }
  ← { "id": "abc12345-example-article", … }

agent → tools/call create_page
  {
    "slug": "example-article-summary",
    "title": "Example article — summary",
    "body": "## Key points\n- …",
    "frontmatter": {
      "tags": ["example", "summary"],
      "created": "2026-04-17",
      "updated": "2026-04-17",
      "sources": [{ "type": "url", "ref": "abc12345-example-article" }],
      "scope": "global",
      "page_type": "concept",
      "related": []
    }
  }
  ← { "slug": "example-article-summary", "id": "…", … }

agent → tools/call lint
  ← { "findings": [], "summary": { "errors": 0, "warnings": 0, "info": 0 } }
```

The same flow is reproducible over REST (`docs/api.md`) or the CLI
(`docs/cli.md`). The agent should pick the interface that the host
provides.
