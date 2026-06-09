---
title: Build and run a distroless Rust container for mnemos
tags: [recipe, docker, rust, distroless, mnemos]
created: 2026-04-17
updated: 2026-04-17
sources:
  - type: url
    ref: src-0020-distroless-rust-guide
    origin: https://github.com/GoogleContainerTools/distroless/blob/main/README.md
  - type: url
    ref: src-0021-rust-docker-official
    origin: https://hub.docker.com/_/rust
scope: local
page_type: recipe
related: [2026-04-17-llm-wiki]
---

Build a single static `mnemos` binary, package it in a
distroless image, and run it via `docker compose` with a mounted
data volume.

## Prerequisites

- Docker 24+
- A Rust 1.75+ toolchain if you want to build outside the
  container (the example Dockerfile builds inside).

## Steps

1. **Create the multi-stage Dockerfile** at the project root:

   ```Dockerfile
   # syntax=docker/dockerfile:1.7

   FROM rust:1.83-bookworm AS builder
   WORKDIR /build
   COPY . .
   RUN cargo build --release --locked

   FROM gcr.io/distroless/cc-debian12:nonroot
   COPY --from=builder /build/target/release/mnemos /usr/local/bin/mnemos
   EXPOSE 8080
   USER nonroot
   ENTRYPOINT ["/usr/local/bin/mnemos"]
   CMD ["serve", "--host", "0.0.0.0", "--port", "8080"]
   ```

2. **Build the image.**

   ```bash
   docker build -t mnemos:dev .
   ```

   Expected: a successful build, a final image around 40 MB.

3. **Create `docker-compose.yml`** at the project root:

   ```yaml
   services:
     mnemos:
       image: mnemos:dev
       restart: unless-stopped
       ports:
         - "8080:8080"
       volumes:
         - ./data:/app/data
         - ./mnemos.db:/app/mnemos.db
       environment:
         RUST_LOG: info
   ```

4. **Start the stack.**

   ```bash
   docker compose up -d
   docker compose ps    # confirm the container is running
   ```

5. **Smoke test.**

   ```bash
   curl -fsS http://localhost:8080/healthz
   # → ok
   ```

6. **Register a user and stash the API key.**

   ```bash
   KEY=$(docker compose exec -T mnemos \
     mnemos user register demo --password-stdin <<< 'demo-pass-demo-pass' \
     | jq -r .api_key)
   echo "$KEY"
   ```

   Save the key — it is the only time it will be printed.

7. **First ingest.**

   ```bash
   docker compose exec -T -e MNEMOS_API_KEY=$KEY mnemos \
     mnemos sources add-url https://example.com/article \
       --slug example-article
   ```

## Key points

- The distroless base is `gcr.io/distroless/cc-debian12:nonroot`.
  We use the `nonroot` variant to avoid running as root inside
  the container.
- The build stage is the heavy `rust:1.83-bookworm` image; the
  final image only carries the binary, libc, and CA certs.
  Nothing else — no shell, no package manager, no busybox.
- Data and the SQLite DB live on host-mounted volumes. To wipe
  state, `docker compose down -v` removes the named volumes
  (none in this compose — `./data` and `./mnemos.db` are
  bind mounts, so `rm -rf` is what you want).

## Conflicts

- Some teams use `rust:slim` for smaller build images. We keep
  the full `rust:1.83-bookworm` because the build is fast
  enough on a warm cache, and `slim` is missing pkg-config and
  several libraries the build needs.

## Open questions

- Should we ship a `--read-only` flag for the container? The
  binary is read-only in the image, but the SQLite WAL files
  are written at runtime. A `tmpfs` for `/tmp` would close
  that gap.
- Multi-arch build (linux/amd64 + linux/arm64) is not in this
  recipe. Add `docker buildx` + `--platform` when needed.
