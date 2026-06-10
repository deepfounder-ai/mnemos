# mnemos

Cloud memory for AI agents — a persistent, compounding knowledge layer
inspired by [Andrej Karpathy's LLM Wiki
pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f).

> *The service is dumb storage + search. The LLM agent does the smart
> work: it reads raw sources, synthesises them into markdown pages, and
> the wiki compounds over time.*

## Phase 1 — Foundation

This commit establishes the foundational scaffold:

- Cargo workspace (binary `mnemos` + library for reuse)
- SQLite schema with FTS5 search, migrations, and indexes
- Domain core: pages, sources, frontmatter, index/log builders, search, lint
- Auth: argon2 passwords, SHA-256-hashed API keys, Axum middleware
- HTTP API scaffold (healthz endpoint, JSON 404)
- MCP stub (stdio)
- CLI surface (clap) with `serve`, `mcp`, and stub subcommands
- Unit + integration tests

## Quick start

```bash
cargo build --release
./target/release/mnemos --version
./target/release/mnemos serve
# in another shell:
curl http://localhost:8080/healthz
```

## Configuration (env)

| Variable                  | Default            | Purpose                              |
| ------------------------- | ------------------ | ------------------------------------ |
| `MNEMOS_HOST`             | `0.0.0.0`          | HTTP bind host                       |
| `MNEMOS_PORT`             | `8080`             | HTTP bind port                       |
| `MNEMOS_DATA_DIR`         | `./data`           | SQLite file + per-user on-disk data  |
| `MNEMOS_DB_URL`           | (auto)             | SQLx connection string               |
| `MNEMOS_LOG`              | `info`             | tracing-subscriber filter            |
| `MNEMOS_MAX_SOURCE_BYTES` | `10485760` (10MB)  | Max URL/upload source size           |
| `MNEMOS_SOURCE_TIMEOUT`   | `30`               | URL fetch timeout in seconds         |

## Status

The API, MCP, and CLI surfaces will be filled in by subsequent tasks.
See the project scratchpad and `docs/` for the full roadmap.
