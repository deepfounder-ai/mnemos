# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- REST API handlers (`src/api/handlers.rs`) covering the full `/api/v1`
  surface: auth (register/login/whoami/keys), pages CRUD + raw, sources
  (URL/upload/raw), search, index, log, and lint — guarded by the
  `require_auth` middleware.
- MCP stdio server (`src/mcp/mod.rs`): JSON-RPC 2.0 loop wiring the existing
  `tools` and `resources` modules, authenticated via `MNEMOS_API_KEY`.
- CLI HTTP client (`src/cli/`): `config`, `output` (stable exit codes),
  and per-family command modules (`user`, `keys`, `pages`, `sources`,
  `misc`) plus `completions` for bash/zsh/fish/elvish.
- Root landing page (`GET /`) with a copyable "connect your LLM" prompt;
  the server's host:port is substituted in at request time.
- Integration test suites: `tests/api_integration.rs` (full HTTP stack,
  tenant isolation, error envelopes), `tests/mcp_stdio.rs` (black-box
  JSON-RPC over the real binary), `tests/cli_e2e.rs` (CLI against a live
  server, exit-code contract). 123 tests total.
- Docker: multi-stage `Dockerfile` (debian-slim runtime, non-root, HEALTHCHECK),
  `docker-compose.yml`, `.dockerignore`, and a one-line installer
  `scripts/install.sh` (`curl … | sh`).
- GitHub: CI workflow (fmt + clippy `-D warnings` + tests + docker smoke
  build), GHCR publish workflow, Dependabot config, `CONTRIBUTING.md`.
- Initial documentation set (`README.md`, `AGENTS.md`, `docs/`, `examples/`, `CHANGELOG.md`).

### Fixed

- MCP `create_page` / `update_page` now accept `frontmatter` as either a JSON
  object or a JSON string. Some MCP hosts (e.g. Claude Code) serialise an
  object-typed tool argument as a string; previously that was rejected with a
  type error, so agents couldn't set tags/related/page_type over MCP.
- Auth middleware now returns the documented `{"error":{"code","message"}}`
  envelope (was a flat `{"code","message"}`) and sets `WWW-Authenticate: Bearer`
  on 401s.
- MCP `get_log` is now side-effect free — it no longer appends a `log.read`
  event on every read and pushes the `since`/`limit` filter down to SQL.
- `MAX_PAGE_LIMIT` lowered to 200 to match `docs/api.md`.
- README: corrected env var name (`MNEMOS_SOURCE_TIMEOUT_SECS`) and replaced
  the stale "Phase 1" status with user-facing quick-start docs.
- `AGENTS.md` — schema for LLM-driven ingest, query, and maintenance workflows.
- `docs/page-format.md` — frontmatter and body specification.
- `docs/api.md` — REST API reference.
- `docs/mcp.md` — Model Context Protocol server reference.
- `docs/cli.md` — `mnemos` command-line reference.
- `examples/pages/` — four worked examples covering every `page_type`:
  concept, decision, recipe, reference.
- `examples/sources/karpathy-gist.md` — sample immutable source.

## [0.1.0] — TBD

### Added

- Multi-tenant Rust service: `mnemos` binary, single static binary.
- REST API (`axum`) on `/api/v1` for pages, sources, auth, lint, index, log, search.
- MCP server (stdio) exposing the same surface as agent-friendly tools/resources.
- CLI (`clap`, derive) with subcommands: `serve`, `user`, `keys`, `pages`, `sources`, `search`, `index`, `log`, `lint`.
- Storage: SQLite (`sqlx`) for metadata, local filesystem for page and source bodies.
- Auth: username/password (argon2) + bearer API keys (`mnemo_…`, SHA-256 hashed at rest).
- Lint report covering orphan pages, broken `related:`, missing source refs, conflicts.
- FTS5-backed search over `(title, body, tags)`.
- Docker image (multi-stage, `debian:bookworm-slim` runtime, non-root) and `docker-compose.yml`.

[Unreleased]: https://example.invalid/mnemos/compare/0.1.0...HEAD
[0.1.0]: https://example.invalid/mnemos/releases/tag/0.1.0
