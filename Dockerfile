# syntax=docker/dockerfile:1

# ---- build stage -----------------------------------------------------------
# Full rust image: it ships gcc, which `libsqlite3-sys` (bundled SQLite)
# needs to compile.
FROM rust:1.85-bookworm AS builder
WORKDIR /app

# Prime the dependency cache: copy manifests first, build a dummy target, then
# copy the real sources. This keeps `cargo build` cached across source-only
# changes.
# Memory-efficient build settings for servers with limited RAM (e.g. EasyPanel).
# These override Cargo.toml profile settings:
#   - lto=off: skip link-time optimization (saves ~1-2GB peak RAM)
#   - codegen-units=16: split into smaller units compiled one at a time
#   - jobs=1: only one unit in flight at a time (lowest peak RAM)
# Override: --build-arg CARGO_BUILD_JOBS=4 --build-arg RUSTFLAGS=""
ARG CARGO_BUILD_JOBS=1
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
ARG RUSTFLAGS="-C lto=off -C codegen-units=16"
ENV RUSTFLAGS=${RUSTFLAGS}

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > src/lib.rs \
    && cargo build --release --locked --quiet || true \
    && rm -rf src

COPY . .
# Touch sources so cargo rebuilds them after the dummy build above.
RUN cargo build --release --locked \
    && strip target/release/mnemos || true

# ---- runtime stage ---------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates lets `add_source_url` reach https origins; curl powers the
# container HEALTHCHECK. Everything else is stripped.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /home/mnemos mnemos \
    && mkdir -p /data && chown mnemos:mnemos /data

COPY --from=builder /app/target/release/mnemos /usr/local/bin/mnemos

USER mnemos
ENV MNEMOS_HOST=0.0.0.0 \
    MNEMOS_PORT=8080 \
    MNEMOS_DATA_DIR=/data \
    MNEMOS_LOG=info
VOLUME ["/data"]
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1

ENTRYPOINT ["mnemos"]
CMD ["serve"]
