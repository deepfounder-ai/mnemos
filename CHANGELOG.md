# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial documentation set (`README.md`, `AGENTS.md`, `docs/`, `examples/`, `CHANGELOG.md`).
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
- Docker image (multi-stage, distroless `cc-debian12` base) and `docker-compose.yml`.

[Unreleased]: https://example.invalid/mnemos/compare/0.1.0...HEAD
[0.1.0]: https://example.invalid/mnemos/releases/tag/0.1.0
