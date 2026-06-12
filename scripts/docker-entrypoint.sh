#!/bin/sh
# Entrypoint that makes the data directory writable regardless of how the
# host mounts it (named volume, bind mount, etc.), then drops privileges to
# the unprivileged `mnemos` user before running the binary.
#
# Cloud platforms (EasyPanel, Coolify, Dokku, …) frequently bind-mount the
# data directory owned by root. A container running as a fixed non-root UID
# then cannot create the SQLite database and crashes on startup. Starting as
# root, fixing ownership, and stepping down with gosu solves this everywhere
# while keeping the server process unprivileged.
set -e

DATA_DIR="${MNEMOS_DATA_DIR:-/data}"

# Only the root case needs the chown + privilege drop. If the image is already
# run as a non-root user (e.g. `docker run --user`), just exec directly.
if [ "$(id -u)" = "0" ]; then
    mkdir -p "$DATA_DIR"
    chown -R mnemos:mnemos "$DATA_DIR" 2>/dev/null || true
    exec gosu mnemos "$@"
fi

exec "$@"
