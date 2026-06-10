# Page Format Specification

> Canonical reference for mnemos wiki pages. **AGENTS.md is normative for LLM
> behavior; this file is normative for page structure.** When in doubt, treat
> the examples in `examples/pages/` as the ground truth.

A page is a single UTF-8 markdown file composed of:

1. **YAML frontmatter** — machine-readable metadata. Required.
2. **Markdown body** — LLM-authored synthesis. Required.

```
---
title: ...
tags: [...]
created: YYYY-MM-DD
updated: YYYY-MM-DD
sources:
  - type: url
    ref: <source_id>
    origin: https://...
scope: global
page_type: concept
related: [<slug>, ...]
---

# body starts here
```

The frontmatter is delimited by `---` lines at the very top of the file. There
must be no blank line before the closing `---`.

---

## Frontmatter fields

All fields are required unless marked otherwise. The `date` format is
ISO-8601 calendar date `YYYY-MM-DD` (UTC). Strings are UTF-8.

| Field           | Type                          | Required | Notes                                            |
| --------------- | ----------------------------- | -------- | ------------------------------------------------ |
| `title`         | string                        | yes      | Short noun phrase, sentence case, no trailing `.` |
| `tags`          | list of string                | yes      | Lowercase, kebab-case, 1–8 entries               |
| `created`       | string (date)                 | yes      | First publication date, never changes after set  |
| `updated`       | string (date)                 | yes      | Bump on every content edit                       |
| `sources`       | list of source ref            | yes      | At least one for a non-stub page                 |
| `scope`         | enum: `global` \| `local`     | yes      | `global` = reusable knowledge; `local` = project-specific |
| `page_type`     | enum: see below               | yes      | Determines index grouping                       |
| `related`       | list of slug                  | no       | Default `[]`. Each entry is a sibling page slug  |

### `title`

A short, descriptive noun phrase. Sentence case. No trailing period. No
colons. Keep it under 80 characters.

Good: `pgvector vs qdrant`, `docker multi-stage builds`, `LLM wiki pattern`.
Bad: `Some thoughts about pgvector and qdrant and which one to use.`.

### `tags`

- 1–8 entries.
- Lowercase.
- Kebab-case for multi-word tags (`vector-search`, not `vector_search`).
- Used by FTS5 search and the index builder for grouping.

### `created` / `updated`

`YYYY-MM-DD` in UTC. `created` is set on first write and never modified.
`updated` must be bumped on every body or frontmatter edit (the service does
this automatically; agents should still set it correctly when writing through
the CLI or `mnemos pages update --from-file`).

### `sources`

A list of source references. Each entry has:

```yaml
sources:
  - type: url          # url | session | file
    ref: <source_id>   # opaque identifier
    origin: https://example.com/article  # optional, recommended for url
```

`type` is one of:

- `url` — a fetched web source. `origin` is the original URL.
- `session` — derived from an agent session. `ref` is the session name
  (e.g. `2026-04-17-llm-wiki-research`).
- `file` — an uploaded file. `ref` is the source ID returned by
  `mnemos sources upload`.

`ref` is the `source_id` returned by the service. The detailed source body
lives separately (immutable, hash-addressed); see
[Source reference format](#source-reference-format).

### `scope`

- `global` — knowledge that is reusable across projects and contexts
  (e.g. `LLM wiki pattern`, `Rust ownership rules`).
- `local` — knowledge tied to a specific project, repo, or system
  (e.g. decision logs, deploy recipes, runbooks).

The lint report flags `local` pages that have no `related:` and no obvious
project tag, on the assumption that local pages should cluster.

### `page_type`

- `concept` — explains what something *is*. Synthesis + key points.
- `recipe` — how to do something. Numbered steps, copy-pasteable commands.
- `reference` — a lookup table / API summary / cheatsheet.
- `decision` — a decision record (what was decided, why, alternatives, tradeoffs).

The index builder groups pages by `page_type` in the auto-generated
`index.md`. Pick the dominant type. If a page is genuinely two types, link
the secondary type via `related:`.

### `related`

A list of slugs. Each entry must be a slug of an existing page owned by the
same user. The lint check verifies every entry resolves.

Use `related:` for:

- Concept ↔ decision pairs (e.g. `LLM wiki pattern` is related to
  `decide-on-llm-wiki-architecture`).
- Concept ↔ recipe pairs (e.g. `docker multi-stage builds` is related to
  `recipe: deploy-mnemos-with-docker-compose`).

Do not use `related:` for hierarchical or "see also" connections that
should be embedded in prose instead. `related:` is for graph edges, not
inline links.

---

## Body structure

A page body is plain markdown. There is no required layout, but a
well-formed page typically contains:

```markdown
A short opening paragraph that states the claim or purpose in 1–3 sentences.

## Key points

- Compressed takeaway #1
- Compressed takeaway #2
- Compressed takeaway #3

## Conflicts

<!-- If sources contradict each other, list the disagreement here. -->

## Open questions

<!-- Gaps, unknowns, things to verify. -->
```

Section names and order are conventional, not enforced. Lint does **not**
require any specific heading. But the four-section shape (`opening` +
`Key points` + `Conflicts` + `Open questions`) makes pages scannable and
helps the agent produce useful diffs across edits.

### What goes in the body

- **Synthesis, not copy-paste.** Quote sparingly (with `>`), but the body
  must be a compressed restatement, not a transcript.
- **Citations live in the frontmatter**, not the body. The body should
  read naturally without naming source files.
- **Conflict markers.** When sources disagree, do not paper over it.
  Use the `## Conflicts` section and quote both sides.

### What does not go in the body

- Authoring notes, TODO comments, scratch reasoning. Move these to
  `## Open questions`.
- Source paths or raw URLs. Use the frontmatter `sources:` list.
- Large code dumps. Recipes should be small, runnable, and run end-to-end.

---

## Source reference format

Sources are immutable, hash-addressed files. A source is created by:

- `POST /api/v1/sources/url` — service fetches the URL (≤ 10 MB, 30 s timeout).
- `POST /api/v1/sources/upload` — multipart upload.
- `mnemos sources add-url` / `mnemos sources upload`.

The service returns a `source_id` (string). The body is stored verbatim and
its SHA-256 is recorded as `content_hash`. The body is **never modified**
after creation.

Pages reference sources by `source_id` in their frontmatter `sources:` list.
The page's `page_sources` join table is derived from the frontmatter and
serves search and lint.

Source bodies themselves are not wiki pages — they are raw, immutable
evidence. They do not get indexed in `index.md` and they are not
lint-checked for `related:` or `page_type`.

---

## Related links semantics

`related:` is a typed edge in the knowledge graph, not a free-form
backlink. The semantics are:

- **Bidirectional expectation.** If page A lists B as `related:`, B should
  usually list A as `related:` too. The linter warns (does not fail) on
  asymmetric edges.
- **Same owner.** Related pages must belong to the same user. Cross-user
  edges are not supported in MVP.
- **Stable slugs.** Slugs are permanent identifiers. Renaming a slug is a
  breaking change — update all `related:` entries that point at the old
  slug and re-run `mnemos lint`.

---

## Worked examples

All examples below are real, validated pages shipped in `examples/pages/`.
They use the same fields, the same body shape, and the same citation
discipline the agent is expected to follow. Together they cover every
enum value of `page_type`.

### Concept page — `2026-04-17-llm-wiki.md`

> Self-describes the pattern mnemos implements. Pure concept, no steps.
> → See `examples/pages/2026-04-17-llm-wiki.md`.

### Decision page — `2026-04-17-pgvector-vs-qdrant.md`

> Records a decision: "we use pgvector, not qdrant, for v0.1". Follows
> the decision-record shape: context, decision, alternatives, consequences.
> → See `examples/pages/2026-04-17-pgvector-vs-qdrant.md`.

### Recipe page — `2026-04-17-docker-recipe.md`

> Step-by-step: how to build a distroless image for a Rust binary and run
> it via `docker-compose`. Numbered steps, copy-pasteable commands.
> → See `examples/pages/2026-04-17-docker-recipe.md`.

### Reference page — `2026-04-17-env-vars.md`

> A pure lookup table: every `MNEMOS_*` environment variable, the
> default, and the purpose, grouped by component (server, CLI, MCP
> server, path defaults). The body is mostly tabular — synthesis is
> confined to the "Conventions" and "Key points" sections.
> → See `examples/pages/2026-04-17-env-vars.md`.

---

## Validation checklist (manual)

When the agent writes a page, it should run through this list before
calling `create_page` / `update_page`:

- [ ] Frontmatter parses as YAML.
- [ ] All required fields present (`title`, `tags`, `created`, `updated`,
      `sources`, `scope`, `page_type`).
- [ ] `page_type` is one of the four enum values.
- [ ] `scope` is `global` or `local`.
- [ ] Every `sources[].ref` is an existing `source_id` for the same user.
- [ ] Every `related[]` entry is a slug of an existing page for the same
      user, **or** the page is being created in the same batch.
- [ ] `tags` has 1–8 entries, all lowercase kebab-case.
- [ ] `updated` ≥ `created`.
- [ ] Body is non-empty.
- [ ] If the page is a `decision`, the body contains a "Context", "Decision",
      "Alternatives", and "Consequences" section (recommended, not enforced).

The service runs a subset of these checks server-side and rejects bad
pages with HTTP 422. See `docs/api.md` for error responses.
