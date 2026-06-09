# CLI Reference

> The `mnemos` command-line tool. Single binary, subcommands via
> `clap` (derive API). The CLI is a thin wrapper over the REST API and
> uses the same JSON shapes.

---

## Install

### From source (requires Rust 1.75+)

```bash
cargo install --git https://github.com/example/mnemos mnemos
# or, from a local checkout:
cargo install --path .
```

### From Docker

```bash
docker run --rm -i ghcr.io/example/mnemos:latest --help
```

### Shell completions

`mnemos` ships with `clap` completions for `bash`, `zsh`, `fish`, and
`elvish`. Generate them with:

```bash
# bash
mnemos completions bash > ~/.local/share/bash-completion/completions/mnemos

# zsh — pick the right _dir, e.g. site-functions
mnemos completions zsh > "${fpath[1]}/_mnemos"

# fish
mnemos completions fish > ~/.config/fish/completions/mnemos.fish

# elvish
mnemos completions elvish > ~/.elvish/lib/mnemos.elv
```

Restart the shell (or `source` the file) for completions to activate.

---

## Configuration

The CLI reads configuration from environment variables. There is no
config file in v0.1; passing `--api-url` / `--api-key` on the command
line also works and overrides the env.

| Variable           | Default                 | Notes                              |
| ------------------ | ----------------------- | ---------------------------------- |
| `MNEMOS_API_URL`   | `http://localhost:8080` | Base URL of the mnemos server.     |
| `MNEMOS_API_KEY`   | —                       | API key. Required for all commands except `user register`, `user login`, `serve`, `completions`. |
| `MNEMOS_CONFIG`    | —                       | Path to a TOML config file (planned for v0.2). |
| `RUST_LOG`         | `info`                  | Standard `tracing` filter.         |

For convenience, persist the key in your shell rc:

```bash
export MNEMOS_API_URL=https://mnemos.example.com
export MNEMOS_API_KEY=mnemo_…
```

---

## Command tree

```
mnemos
├── serve            # run the HTTP server
├── completions      # generate shell completions
├── user
│   ├── register
│   ├── login
│   └── whoami
├── keys
│   ├── list
│   ├── create
│   └── revoke
├── pages
│   ├── list
│   ├── get
│   ├── create
│   ├── update
│   └── delete
├── sources
│   ├── list
│   ├── add-url
│   ├── upload
│   └── get
├── search
├── index
├── log
└── lint
```

---

## `serve` — run the HTTP server

```bash
mnemos serve [--host 0.0.0.0] [--port 8080] [--data-dir ./data] [--db ./mnemos.db]
```

| Flag        | Env               | Default       | Notes                              |
| ----------- | ----------------- | ------------- | ---------------------------------- |
| `--host`    | `MNEMOS_HOST`     | `0.0.0.0`     | Bind address.                      |
| `--port`    | `MNEMOS_PORT`     | `8080`        | Bind port.                         |
| `--data-dir`| `MNEMOS_DATA_DIR` | `./data`      | Per-user namespace root.           |
| `--db`      | `MNEMOS_DB`       | `./mnemos.db` | SQLite path.                       |

The server runs migrations on startup. Logs are JSON by default; pass
`RUST_LOG=mnemos=debug` for verbose output.

---

## `user` — account management

### `mnemos user register <username>`

```bash
mnemos user register alice --password-stdin   # recommended
# or
mnemos user register alice                     # prompts for password (TTY only)
```

Prints the new `user_id` and **initial API key** to stdout. Save the
key — it is not recoverable.

`--password-stdin` reads the password from stdin (trim trailing
newline). Suitable for shell pipelines and secret managers.

### `mnemos user login <username>`

```bash
mnemos user login alice --password-stdin
```

Exchanges username + password for a **fresh** API key. The key is
printed once. Old keys are not invalidated — revoke them explicitly
with `mnemos keys revoke`.

### `mnemos user whoami`

Prints the user_id and username of the API key in `MNEMOS_API_KEY`.

---

## `keys` — API key management

### `mnemos keys list`

```text
ID                                    NAME                     CREATED              LAST USED
0190a4d6-2c0e-7abc-9def-0123456789ab  claude-code-laptop       2026-04-17T12:34Z    2026-04-17T15:02Z
```

### `mnemos keys create <name>`

```bash
mnemos keys create cursor-workstation
# → key id + secret key (printed once)
```

### `mnemos keys revoke <id>`

```bash
mnemos keys revoke 0190a4d6-2c0e-7abc-9def-0123456789ab
```

Idempotent. There is no confirmation prompt.

---

## `pages` — page CRUD

### `mnemos pages list [--tag T] [--type T] [--query Q]`

Filter flags are repeatable. `--tag rust --tag axum` ANDs.

```bash
mnemos pages list --type concept --tag llm
mnemos pages list --query "vector search"
```

Output is a table; pass `--json` for the raw API response.

### `mnemos pages get <slug>`

```bash
mnemos pages get llm-wiki
```

Prints the page as `text/markdown` (frontmatter + body) so the output
is diff-friendly. Pass `--json` for the structured form.

### `mnemos pages create <slug> [--title T] [--from-file F] [--from-stdin]`

```bash
# From a file (typical authoring workflow)
mnemos pages create my-recipe --title "My recipe" --from-file ./my-recipe.md

# From stdin
mnemos pages create my-recipe --title "My recipe" --from-stdin < ./my-recipe.md
```

The input must already include the YAML frontmatter. The CLI parses
the frontmatter, posts the body and the parsed metadata to the API,
and prints the new page id.

If the slug already exists, the call fails with a non-zero exit code —
use `update` instead.

### `mnemos pages update <slug> [--from-file F] [--from-stdin]`

Same as `create`, but targets an existing slug. Both frontmatter and
body are replaced.

### `mnemos pages delete <slug>`

```bash
mnemos pages delete old-recipe
```

Idempotent. No prompt.

---

## `sources` — source management

### `mnemos sources list`

Table of sources with id, slug, type, origin, content_hash, bytes,
created_at.

### `mnemos sources add-url <url> [--slug S]`

```bash
mnemos sources add-url https://example.com/article --slug example-article
```

Fetches the URL server-side. The CLI does not pull the body itself;
the server stores the bytes. Use this to keep the source immutable
even if the upstream changes.

### `mnemos sources upload <file> [--slug S]`

```bash
mnemos sources upload ./notes.md --slug meeting-2026-04-17
```

Slugs are derived from the filename if `--slug` is omitted.

### `mnemos sources get <id>`

Prints the raw source body to stdout.

---

## `search`

```bash
mnemos search "vector search" --limit 10
```

Convenience over `pages list --query …`. Always uses FTS5 ranking.

---

## `index`

```bash
mnemos index
```

Prints the auto-generated `index.md` for the current user.

---

## `log`

```bash
mnemos log --since 2026-04-17T00:00:00Z --limit 50
mnemos log --format md
```

Default output is a JSON array; `--format md` renders the human-readable
markdown form. Both come from the same underlying event log.

---

## `lint`

```bash
mnemos lint
```

Runs the server-side linter and prints a human-readable report. Exit
code is `0` on no findings, `1` if any error-level findings are
present. Warning- and info-level findings still produce exit code `0`.

---

## Exit codes

| Code | Meaning                                                  |
| ---- | -------------------------------------------------------- |
| 0    | Success.                                                 |
| 1    | Server returned a finding (lint) or a generic CLI error. |
| 2    | Invalid arguments (`clap` error).                        |
| 3    | Network / connection error (could not reach the API).    |
| 4    | Auth error (401/403 from the API).                        |
| 5    | Validation error (422 from the API).                      |
| 6    | Not found (404 from the API).                            |
| 7    | Conflict (409 from the API).                             |
| 64   | Unexpected server error (5xx).                           |

Scripts can rely on these codes for branching. Anything not listed
above is treated as a bug and should be reported.

---

## Worked example: end-to-end

```bash
# 1. Boot the server
docker compose up -d
# (or: cargo run -- serve)

# 2. Register and stash the key
KEY=$(mnemos user register alice --password-stdin <<< 'hunter2hunter2' | jq -r .api_key)
export MNEMOS_API_KEY=$KEY

# 3. Add a source
mnemos sources add-url https://example.com/article --slug example-article
# → prints {"id":"abc12345-example-article", …}

# 4. Create a page from a local file
cat > /tmp/example-page.md <<'EOF'
---
title: Example article summary
tags: [example, summary]
created: 2026-04-17
updated: 2026-04-17
sources:
  - type: url
    ref: abc12345-example-article
    origin: https://example.com/article
scope: global
page_type: concept
related: []
---

## Key points

- The article argues X.
- It supports X with Y.
EOF

mnemos pages create example-article-summary \
  --title "Example article summary" \
  --from-file /tmp/example-page.md

# 5. Lint
mnemos lint
```
