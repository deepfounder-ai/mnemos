# Contributing to mnemos

Thanks for your interest! mnemos is a small, layered Rust service — the goal
is to keep it simple and well-tested.

## Getting set up

```bash
git clone https://github.com/OWNER/mnemos && cd mnemos
cargo build
cargo test
```

Requires Rust 1.75+ (`rustup toolchain install stable`).

## Before you open a PR

Run the same checks CI does:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --locked
```

All three must be clean. New behaviour needs tests:

- domain logic → unit tests in the module (`#[cfg(test)]`) or `tests/core_*.rs`
- HTTP behaviour → `tests/api_integration.rs`
- MCP behaviour → `tests/mcp_stdio.rs`
- CLI behaviour → `tests/cli_e2e.rs`

## Architecture

Layers depend only downward (see the diagram in `README.md`). Transport
layers (`api`, `mcp`, `cli`) are thin adapters over `core` services; put
business logic in `core`, not in handlers. Error mapping lives in
`src/error.rs`; the wire error envelope is `{"error":{"code","message"}}`.

## Commits

Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, …).
Keep the subject ≤ 50 chars; explain the "why" in the body when it isn't
obvious. Update `CHANGELOG.md` under `[Unreleased]` for user-visible changes.

## Docs

The contracts in `docs/` (`api.md`, `mcp.md`, `cli.md`, `page-format.md`) are
the source of truth for the wire surfaces — update them in the same PR as the
code that changes behaviour.
