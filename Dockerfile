# syntax=docker/dockerfile:1

# ---- build stage -----------------------------------------------------------
# Full rust image: it ships gcc, which `libsqlite3-sys` (bundled SQLite)
# needs to compile.
# Latest stable rust: transitive deps (icu_* via idna) bump their MSRV
# frequently; tracking stable avoids whack-a-mole pinning.
FROM rust:1-bookworm AS builder
WORKDIR /app

# Skip link-time optimization to keep peak linker RAM modest (saves ~1-2GB)
# without serializing compilation. Compilation parallelism stays at the default
# (all cores), so the build is fast. On a RAM-starved builder, pass
# --build-arg RUSTFLAGS="-C lto=off -C codegen-units=16" and
# CARGO_BUILD_JOBS via the environment to trade speed for lower peak memory.
ARG RUSTFLAGS="-C lto=off"
ENV RUSTFLAGS=${RUSTFLAGS}

# Optional cap on compile parallelism. Leave empty for cargo's default
# (all cores). On a low-RAM builder, pass --build-arg CARGO_JOBS=2 so parallel
# rustc processes don't exhaust memory and produce truncated rlibs.
ARG CARGO_JOBS=""

# Build from the full sources in one shot. A previous version used the
# "dummy main.rs to cache deps" trick, but it silently shipped the empty stub
# binary (cargo skipped the real rebuild when Docker COPY backdated mtimes),
# producing a container that started and immediately exited 0. A single honest
# build is slower without a warm cache but always correct. BuildKit layer
# caching still skips this step entirely when nothing changed.
COPY . .
RUN cargo build --release --locked ${CARGO_JOBS:+-j ${CARGO_JOBS}} \
    && strip target/release/mnemos

# ---- runtime stage ---------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates lets `add_source_url` reach https origins; curl powers the
# container HEALTHCHECK; gosu drops privileges in the entrypoint. Everything
# else is stripped.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gosu \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /home/mnemos mnemos \
    && mkdir -p /data && chown mnemos:mnemos /data

COPY --from=builder /app/target/release/mnemos /usr/local/bin/mnemos
COPY scripts/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# NOTE: we intentionally run as root so the entrypoint can fix ownership of a
# host-mounted data dir, then step down to `mnemos` via gosu. The server
# process itself never runs as root.
ENV MNEMOS_HOST=0.0.0.0 \
    MNEMOS_PORT=8080 \
    MNEMOS_DATA_DIR=/data \
    MNEMOS_LOG=info
VOLUME ["/data"]
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1

ENTRYPOINT ["docker-entrypoint.sh", "mnemos"]
CMD ["serve"]
