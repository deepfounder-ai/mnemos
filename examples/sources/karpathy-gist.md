<!--
This file is a worked example of a mnemos source body.
It is intentionally short and self-contained so the example pages
that cite it can be read without leaving the repo.

In production, a source body is whatever the upstream produced
(raw HTML, PDF text, the markdown export of a chat session).
The service stores it verbatim; the frontmatter and the
`source_id` are added by the service at upload time, not by the
author.

The "frontmatter" block below is for human readability only.
The mnemos source schema is: { id, slug, type, origin, content_hash, ... }
-->

# Karpathy — LLM Wiki pattern (gist 442a6bf555914893e9891c11519de94f)

> Persistent, compounding knowledge layer for an LLM agent.

The pattern has three components, all stored as plain text:

1. **Raw sources** — immutable. URLs, PDFs, session transcripts.
   Never edited after the agent first sees them.
2. **Wiki pages** — markdown + YAML frontmatter. Authored by the
   LLM. Synthesised, not transcribed. Cite sources by id.
3. **Index** and **log** — auto-generated. The index is a
   human-readable catalog (grouped by page type). The log is an
   append-only event stream that records every mutation.

The LLM does the "smart" work: read source → write page → link
neighbours → cite sources. The substrate (filesystem, search
index, log) is dumb storage.

## Why it works

A session-only LLM forgets everything when the context window
rolls over. The wiki is what persists. A future agent, on a
fresh session, can read the wiki and resume where the previous
one left off — without re-deriving knowledge from the raw
sources.

The discipline that makes it work is the **schema discipline**:
every claim is compressed, every compression cites a source,
every conflict is recorded, every gap is annotated. Without
discipline the wiki rots into a heap of transcripts.

## Key terms

- **Page** — a wiki entry. Frontmatter + body.
- **Source** — an immutable raw input.
- **Index** — the auto-generated catalog.
- **Log** — the append-only mutation history.

## See also

- `examples/pages/2026-04-17-llm-wiki.md` — the page that cites
  this source.
