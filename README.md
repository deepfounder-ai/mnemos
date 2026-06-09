# mnemos

[![CI](https://img.shields.io/badge/CI-passing-brightgreen)](#development)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust 1.75+](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

Cloud memory for AI agents. A persistent, compounding knowledge layer
between an LLM agent and its raw sources.

mnemos is the multi-tenant, agent-agnostic version of Karpathy's
[LLM Wiki pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f):
markdown pages, immutable sources, an auto-generated index, and an
append-only event log. The agent writes; the service stores and
searches.

---

## What is this

mnemos is a small Rust service that gives an LLM agent long-term
memory. The agent reads a raw source (a URL, a file, a transcript of
a session), writes a compressed markdown page that captures the
meaning, and links the page to its neighbours. mnemos stores the
pages, indexes them with SQLite FTS5, and exposes them back to
agents — or to humans — through three interfaces: a REST API, an
MCP server, and a CLI.

The service itself is intentionally dumb. There is no built-in LLM,
no vector embeddings, no automatic summarisation. The "intelligence"
is the agent's job; mnemos is the durable substrate. This means it
is cheap (one small static binary, SQLite, no GPU), portable
(distroless Docker image, ~40 MB), and forever agent-agnostic —
swap Claude for GPT for Gemini for a local model and the wiki does
not change.

The unit of knowledge is a **page**: a markdown file with a typed
YAML frontmatter (title, tags, sources, scope, page_type, related).
Pages are grouped by `page_type` (concept, recipe, reference,
decision) in an auto-generated `index.md`, audited by a linter,
and connected by typed `related:` edges. Sources are immutable
hash-addressed files; pages cite them by id. The whole wiki is
diffable, auditable, and exportable as plain text.

---

## Quick start

### With `docker compose` (one command)

```bash
curl -O https://example.invalid/mnemos/docker-compose.yml
docker compose up -d
# wait ~2s for the server, then:
docker compose exec mnemos mnemos user register demo --password-stdin <<< 'demo-pass-demo-pass'
```

The compose file mounts a `data/` volume for per-user storage and a
`mnemos.db` SQLite file. To stop and wipe state, `docker compose
down -v`.

### With `cargo install`

```bash
cargo install --git https://github.com/example/mnemos mnemos
mnemos serve --host 127.0.0.1 --port 8080 &
mnemos user register demo --password-stdin <<< 'demo-pass-demo-pass'
```

### With the CLI: register, login, and a first page

```bash
# Register (prints user_id and initial API key — save the key)
mnemos user register demo --password-stdin <<< 'demo-pass-demo-pass'
# → {"user_id":"…","api_key":"mnemo_…"}
export MNEMOS_API_KEY=mnemo_…

# Add a source
mnemos sources add-url https://example.com/article --slug example-article

# Create a page from a local file
cat > /tmp/page.md <<'EOF'
---
title: Example article summary
tags: [example, summary]
created: 2026-04-17
updated: 2026-04-17
sources:
  - type: url
    ref: <paste source_id from previous command>
    origin: https://example.com/article
scope: global
page_type: concept
related: []
---

## Key points
- The article argues X.
EOF

mnemos pages create example-article-summary \
  --title "Example article summary" \
  --from-file /tmp/page.md

# Search
mnemos search "example article"

# Lint
mnemos lint
```

---

## Architecture

mnemos is a single Rust binary that runs three protocols over a
shared core.

```
            +---------------------------+
            |       LLM agent           |
            |  (Claude / GPT / local)   |
            +-------------+-------------+
                          |
            +-------------+-------------+      MCP (stdio)
            |   mnemos mcp  (subprocess) | <---------------------+
            +-------------+-------------+                       |
                          |                                     |
                          v                                     |
            +-------------+-------------+      REST (HTTP)     |
            |      mnemos serve         | <---------------------+
            |  (Axum + Tokio)           |
            +-------------+-------------+
                          |
              +-----------+-----------+
              |                       |
              v                       v
    +-------------------+    +-------------------+
    |   core services   |    |   auth + keys     |
    | page / source /   |    | argon2 / SHA-256  |
    | frontmatter /     |    +-------------------+
    | index / log /     |
    | search / lint     |
    +---------+---------+
              |
              v
    +-------------------+         +-------------------+
    | storage (sqlx)    |         | filesystem        |
    | mnemos.db         |         | data/users/<id>/  |
    | metadata + FTS5   |         |  pages/*.md       |
    +-------------------+         |  sources/*.md     |
                                  |  index.md, log.md |
                                  +-------------------+
```

- **Core** (`src/core/`) — page, source, frontmatter, index, log,
  search, lint. Pure domain logic. No I/O types leak in.
- **Storage** (`src/storage/`) — sqlx repos for metadata +
  FTS5, plus a filesystem layout for page and source bodies.
- **Auth** (`src/auth/`) — argon2 password hashing, SHA-256
  hashed API keys (`mnemo_…`), Axum middleware that turns a
  bearer token into a `user_id`.
- **API** (`src/api/`) — Axum router, one handler per
  endpoint in `docs/api.md`.
- **MCP** (`src/mcp/`) — stdio MCP server, tools mirror REST.
- **CLI** (`src/cli/`) — clap derive subcommands.

The CLI is just a thin HTTP client over the REST API. The MCP
server is a stdio JSON-RPC adapter over the same REST API. There
is one storage layer; three protocols above it.

---

## Use cases

- **Personal knowledge base for a coding agent.** Drop in
  `AGENTS.md`, point the agent at mnemos, and it accumulates
  recipes, decisions, and concepts across sessions — even
  across machines, since the data lives on a server.
- **Team memory for a multi-agent workflow.** Each agent has
  its own API key, but the wiki is shared per user. Two agents
  on the same user account can read each other's pages and
  build on them.
- **Decision log.** Every architectural choice becomes a
  `page_type: decision` page with sources, alternatives, and
  consequences. Audit-friendly and grep-friendly.
- **Citable recall.** A chatbot that needs to answer
  questions with citations. The agent queries mnemos, walks
  the `page → source` chain, and answers with slugs and
  source ids the human can verify.

---

## Documentation

- [AGENTS.md](AGENTS.md) — **read this if you are an LLM agent.**
  Schema for ingest, query, and maintain workflows.
- [docs/page-format.md](docs/page-format.md) — frontmatter
  reference, body structure, validation rules.
- [docs/api.md](docs/api.md) — REST API endpoints, auth,
  errors, curl examples.
- [docs/mcp.md](docs/mcp.md) — MCP server, tools, resources,
  client configuration.
- [docs/cli.md](docs/cli.md) — every `mnemos` subcommand with
  examples.
- [examples/pages/](examples/pages/) — three worked pages
  (concept, decision, recipe).
- [CHANGELOG.md](CHANGELOG.md) — release notes.

---

## Development

Build, test, and run the docs locally:

```bash
# build
cargo build

# unit + integration tests
cargo test

# run the server
cargo run -- serve --port 8080

# register a user
echo 'hunter2hunter2' | cargo run -- user register alice --password-stdin

# generate shell completions
cargo run -- completions bash > mnemos.bash
```

Project layout (single binary, library for reuse):

```
src/
  main.rs         # binary entry — clap dispatch
  lib.rs          # re-exports
  config.rs       # env-based config
  error.rs        # thiserror AppError + ApiError
  auth/           # user, api_key, password, middleware
  core/           # page, source, frontmatter, index, log, search, lint
  storage/        # sqlx repos + filesystem layout
  api/            # axum router, handlers, middleware
  mcp/            # mcp server (tools + resources)
  cli/            # clap subcommands
migrations/       # SQLx migrations
tests/            # integration tests (api, mcp, cli)
docs/             # documentation
examples/         # worked example pages and sources
```

Coding conventions:

- `cargo fmt` and `cargo clippy --all-targets -- -D warnings`
  on every commit.
- All write paths go through `core::page` / `core::source` —
  handlers never touch SQL directly.
- New endpoints come with a test in `tests/api_integration.rs`
  and a doc snippet in `docs/api.md`.
- New MCP tools come with a doc snippet in `docs/mcp.md`.
- Errors implement `IntoResponse`; domain errors map to HTTP
  via `ApiError`.

### Releasing

Bump the version in `Cargo.toml`, update `CHANGELOG.md`, tag.
The CI builds a multi-stage distroless image and pushes it as
`ghcr.io/example/mnemos:<version>`.

---

## Out of scope (for v0.1)

- Built-in LLM ingest (Claude/OpenAI proxy). Interface is
  reserved, no implementation.
- Embeddings-based semantic search. FTS5 BM25 only.
- Web UI. CLI + MCP are the interfaces.
- Multi-user collaboration (sharing, ACLs). Each user has
  an isolated namespace.
- Object storage backend (S3/R2). Local filesystem only.
- Encryption at rest.
- Vector DB, hybrid search, re-ranking.

These are tracked for v0.2+; see the CHANGELOG.

---

## License

MIT. See [LICENSE](LICENSE).
