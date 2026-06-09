---
title: pgvector vs qdrant for v0.1
tags: [decision, vector-search, postgresql, rust, v0-1]
created: 2026-04-17
updated: 2026-04-17
sources:
  - type: url
    ref: src-0010-pgvector-readme
    origin: https://github.com/pgvector/pgvector
  - type: url
    ref: src-0011-qdrant-overview
    origin: https://qdrant.tech/documentation/overview/
scope: local
page_type: decision
related: [2026-04-17-llm-wiki]
---

## Context

mnemos v0.1 needs a vector search backend for semantic recall
over wiki pages. The two candidates are:

- **pgvector** — a PostgreSQL extension. We already use SQLite
  for metadata. pgvector would mean a second database.
- **qdrant** — a standalone vector DB. We would add a third
  running service to the stack.

Constraints: must be embeddable in the distroless Docker image
in v0.1, must work with a single-binary deploy story, must not
block the "ship the wiki first" goal. v0.1 explicitly defers
embeddings-based search — FTS5 is sufficient for the first
release. The decision is for v0.2+ and should be deferred, not
re-opened in this page.

## Decision

We are **not** picking either for v0.1. v0.1 ships with FTS5
BM25 only. For v0.2, the working assumption is **pgvector** if
we move off SQLite for metadata, or **qdrant** if we stay on
SQLite. This page records the call so the next agent does not
re-derive it.

## Alternatives

- **pgvector.** Pros: single datastore, transactional with
  metadata, mature operators story. Cons: forces a move to
  PostgreSQL (extra service, more RAM, more ops surface); the
  extension is not in the default image.
- **qdrant.** Pros: purpose-built, fast, gRPC + REST, Rust
  client. Cons: another service to deploy and back up; data
  lives outside the SQLite file the rest of the wiki is in.
- **sqlite-vec / sqlite-vss.** Pros: stays in SQLite, no
  extra service. Cons: immature, smaller operator surface,
  unclear production track record. Revisit when it stabilises.

## Consequences

- v0.1 has no semantic search. This is acceptable — FTS5
  covers the "I know the word" case, which is the dominant
  one for an internal wiki.
- v0.2 will need to make a real call. The two viable
  candidates are pgvector (if we adopt Postgres) and qdrant
  (if we stay on SQLite). sqlite-vec is a dark horse.
- The page `llm-wiki` is the parent concept; the parent
  decision-log page is the next layer up.

## Conflicts

<!-- none at this revision -->
