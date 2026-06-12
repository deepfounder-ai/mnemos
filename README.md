# mnemos

**Cloud memory for AI agents** — a persistent, multi-tenant knowledge layer
your agents read from and write to. Inspired by
[Andrej Karpathy's LLM Wiki pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f).

One small Rust binary exposes the same memory over three surfaces:

- **REST API** (`axum`) — for apps and automation.
- **MCP server** (stdio) — the native interface for LLM agents (Claude Code,
  Cursor, …).
- **CLI** (`clap`) — for humans and scripts.

> The service is intentionally **dumb storage + search**. The intelligence is
> the agent's: it reads raw sources, synthesises them into short markdown
> *pages*, links them with typed `related:` edges, and the wiki compounds
> over time.

---

## Features

- 📚 **Pages** — markdown + YAML frontmatter, one compressed claim per page.
- 🔗 **Graph** — typed `related:` edges between pages.
- 📎 **Sources** — immutable URL fetches and file uploads, cited by id.
- 🔎 **Search** — SQLite **FTS5** with BM25 ranking.
- 🧹 **Lint** — finds orphans, broken links, missing source refs.
- 🧾 **Audit log** — every mutation is recorded, per user.
- 🔐 **Multi-tenant** — per-user isolation; argon2 passwords + SHA-256-hashed
  `mnemo_…` API keys.
- 🦀 Single static-ish binary, SQLite-backed, no external services.

---

## Quick start

### 1. EasyPanel (one click)

[![Deploy on EasyPanel](https://easypanel.io/img/deploy-on-easypanel.svg)](https://easypanel.io/docs/templates/mnemos)

Or use the template directly: **Services → New Service → Template → search "mnemos"**.

After deploy, open the service URL — the landing page walks you through registering a user and connecting Claude Code with `mnemos setup`.

### 2. One line (Docker)

```bash
curl -fsSL https://raw.githubusercontent.com/deepfounder-ai/mnemos/main/scripts/install.sh | sh
```

Starts a container, waits until it is healthy, registers a first user, and
prints your API key + dashboard URL. Override defaults with env vars
(`MNEMOS_PORT`, `MNEMOS_USER`, `MNEMOS_IMAGE`, …).

### 2. Docker Compose (from a clone)

```bash
git clone https://github.com/deepfounder-ai/mnemos && cd mnemos
docker compose up -d --build
curl http://localhost:8080/healthz
```

### 3. Plain `docker run`

```bash
docker run -d --name mnemos -p 8080:8080 -v mnemos-data:/data \
  ghcr.io/deepfounder-ai/mnemos:latest
```

### 4. From source (Rust 1.75+)

```bash
cargo build --release
./target/release/mnemos serve            # http://0.0.0.0:8080
```

Open <http://localhost:8080/> in a browser for a dashboard with copy-paste
connection instructions — including a ready-made prompt you can hand to an
LLM so it connects itself.

---

## First steps

```bash
# point the CLI at your server
export MNEMOS_API_URL=http://localhost:8080

# register (prints an API key once — save it)
mnemos user register alice --password-stdin <<< 'correct horse battery staple'
export MNEMOS_API_KEY=mnemo_…

# write and recall memory
mnemos pages create llm-wiki --from-file ./page.md
mnemos search "vector search"
mnemos pages get llm-wiki
mnemos lint
```

The same operations are available over REST (`docs/api.md`) and MCP
(`docs/mcp.md`).

---

## Connect an LLM agent (MCP)

Two ways to connect, both speaking the same MCP surface.

### Remote (HTTP) — recommended for a hosted server

The server exposes MCP over HTTP at `POST /mcp` (JSON-RPC, Streamable HTTP).
No binary install — point the client straight at the URL with a Bearer key:

```bash
claude mcp add --transport http --scope user mnemos \
  https://your-host/mcp \
  --header "Authorization: Bearer mnemo_…"
```

The same URL works as a remote MCP server for the Claude API `mcp_servers`
field (`authorization_token` = the API key). claude.ai web Connectors need
OAuth and are not yet supported.

### Local (stdio) — `mnemos mcp`

`mnemos mcp` runs a stdio MCP server that proxies to a REST API
(`MNEMOS_API_URL` + `MNEMOS_API_KEY`). The quickest setup is:

```bash
MNEMOS_API_URL=https://your-host MNEMOS_API_KEY=mnemo_… mnemos setup
```

…which runs `claude mcp add` for you. Or configure the host manually:

```json
{
  "mcpServers": {
    "mnemos": {
      "command": "mnemos",
      "args": ["mcp"],
      "env": {
        "MNEMOS_API_URL": "https://your-host",
        "MNEMOS_API_KEY": "mnemo_…"
      }
    }
  }
}
```

Tools: `list_pages`, `get_page`, `create_page`, `update_page`, `delete_page`,
`search_pages`, `list_sources`, `get_source`, `add_source_url`,
`upload_source`, `get_index`, `get_log`, `lint`. Resources: `mnemos://index`,
`mnemos://log`, `mnemos://page/{slug}`, `mnemos://source/{id}`.

---

## Configuration (env)

| Variable                     | Default            | Purpose                              |
| ---------------------------- | ------------------ | ------------------------------------ |
| `MNEMOS_HOST`                | `0.0.0.0`          | HTTP bind host                       |
| `MNEMOS_PORT`                | `8080`             | HTTP bind port                       |
| `MNEMOS_DATA_DIR`            | `./data`           | SQLite file + per-user on-disk data  |
| `MNEMOS_DB_URL`              | (auto)             | SQLx connection string override      |
| `MNEMOS_LOG` / `RUST_LOG`    | `info`             | `tracing-subscriber` filter          |
| `MNEMOS_MAX_SOURCE_BYTES`    | `10485760` (10 MB) | Max URL/upload source size           |
| `MNEMOS_SOURCE_TIMEOUT_SECS` | `30`               | URL fetch timeout, seconds           |
| `MNEMOS_SECRET`              | (unset)            | If set, gates `auth/register` behind a matching `secret` |

CLI / stdio-MCP: `MNEMOS_API_URL` (default `http://localhost:8080`) and
`MNEMOS_API_KEY` select the server the CLI and `mnemos mcp` talk to.

---

## Develop

```bash
cargo test            # unit + integration (API, MCP-over-stdio, CLI e2e)
cargo clippy --all-targets
cargo fmt
```

Layered architecture (each layer depends only on those below):

```
cli / mcp / api  (transport)
        │
      core        (page, source, frontmatter, index, log, search, lint)
        │
  storage (sqlx + fs) · auth (users, api keys, middleware)
        │
   error · config
```

---

## Documentation

- [AGENTS.md](AGENTS.md) — **read this if you are an LLM agent**: the ingest /
  query / maintain workflow contract.
- [docs/page-format.md](docs/page-format.md) — frontmatter + body spec.
- [docs/api.md](docs/api.md) — REST endpoints, auth, errors, curl examples.
- [docs/mcp.md](docs/mcp.md) — MCP tools, resources, host configuration.
- [docs/cli.md](docs/cli.md) — every subcommand, env config, exit codes.
- [examples/](examples/) — worked pages (one per `page_type`) + a sample source.
- [CHANGELOG.md](CHANGELOG.md) — release notes.

## License

[Apache-2.0](LICENSE).
