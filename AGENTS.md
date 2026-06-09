# AGENTS.md — Instructions for LLM Agents Using mnemos

> **You are an LLM agent. mnemos is your long-term memory.** Read this file
> before you start a task. Treat it the way you would treat a system
> prompt for a new tool: it is non-negotiable unless the human explicitly
> overrides it.

This file is normative. If a workflow below conflicts with a behavior
you would otherwise default to, the workflow wins. If the workflow
itself is ambiguous, ask the human before guessing.

---

## 1. What is mnemos

mnemos is a **persistent, compounding knowledge layer** between you
(an LLM agent) and your raw sources. It is **not** a vector database,
**not** a notes app, **not** an LLM proxy. It is:

- A **service** that stores markdown pages and raw sources, scoped
  per user.
- A **search engine** over those pages (SQLite FTS5, BM25).
- A **graph** of pages connected by typed `related:` edges.
- A **log** of every mutation, so future agents can audit how
  knowledge evolved.

It is intentionally dumb. The "intelligence" is yours: you read a
source, you write a page that compresses its meaning, you link the
page to neighbours, and you cite the source by id. mnemos stores
what you wrote and helps future agents find it.

The pattern is from Andrej Karpathy's
[LLM Wiki gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f).
mnemos is the persistent, multi-tenant, agent-agnostic version of it.

---

## 2. When to use it

You should use mnemos in three situations:

1. **You learned something durable.** A new concept, a non-obvious
   decision, a working recipe. If the human will care about it
   tomorrow, write a page.
2. **You were asked to recall something.** A previous conversation,
   a prior decision, a recipe you've used before. Query mnemos
   first, then answer.
3. **You noticed a contradiction or gap.** Two pages disagree, or
   a page is missing a related link, or a source is missing
   citations. Run the linter, then fix or annotate.

Do **not** use mnemos for:

- Ephemeral scratch (a single conversation's todo list). Keep that
  in your context.
- Private user data you do not have permission to store.
- Code that already lives in a repo. Cite the file, do not copy it
  into a page.

---

## 3. The three workflows

### 3.1 Ingest — turn a source into a page

Goal: capture a new piece of knowledge in a way that compounds.

```
1.  Decide the source type.
      - URL       → add_source_url
      - File      → upload_source
      - Session   → no source file; cite the session name in frontmatter
2.  Read the source. Use the source's own tools (web fetch, file read)
    to actually read it. Do not summarise from a title alone.
3.  Decide the page shape.
      - concept     → "what is X"
      - recipe      → "how to do X"
      - reference   → "lookup table for X"
      - decision    → "we chose X over Y because Z"
4.  Write the page. Follow docs/page-format.md.
      - Title: short noun phrase, sentence case, no trailing period.
      - Tags: 1–8 lowercase kebab-case.
      - Body: 1–3 sentence opening + Key points + Conflicts (if any)
        + Open questions (if any).
      - Sources: at least one ref pointing at the source from step 1.
      - Related: slugs of existing pages this page touches. For new
        pages this list is empty.
5.  Save the page. Use create_page with the parsed frontmatter and
    the body.
6.  Update related pages. For every slug that should now point at
    this new page, fetch the page and add the new slug to its
    related[] list. Do this in the same session — the linter
    will complain otherwise.
7.  Run lint. Verify no new findings, or that any new findings are
    known/expected.
8.  Stop. Do not write a "summary of the summary" page. Do not
    write a meta-page about the workflow.
```

The single biggest mistake is writing a page that is mostly
copy-paste of the source. Synthesise. A page is a compressed claim,
not a transcript. If a reader wants the source, they follow the
`source_id` in the frontmatter.

### 3.2 Query — answer a question from memory

Goal: produce a correct, well-cited answer without re-reading
everything.

```
1.  search_pages (or list_pages with --query) for the topic.
      - If the query is exact, prefer list_pages with a tag filter.
      - If the query is fuzzy, prefer search_pages for BM25 ranking.
2.  Read the top results with get_page.
      - Read at least 2, even if the first looks like a hit.
      - If the first hit is a recipe and the question is conceptual,
        follow its related[] edges to find the concept page.
3.  Read the sources those pages cite.
      - Sources are authoritative. Pages are compressions of sources.
      - If a page and its source disagree, the source wins. Flag the
        discrepancy in the page's ## Conflicts section.
4.  Compose the answer.
      - Cite the slug of every page you used.
      - Cite the source_id of any source you used directly.
      - If you cannot find an answer, say so. Do not invent.
5.  Optional: if the answer exposed a missing page, schedule a
    follow-up to write it.
```

The answer's "ground truth" is the chain `page → source_id → raw source`.
Anything you assert should be traceable through that chain. If the
chain breaks (page cites a source you cannot read), the assertion is
unsupported.

### 3.3 Maintain — keep memory healthy

Goal: stop the wiki from rotting.

```
1.  Run lint. Read the report.
2.  For each finding, decide:
      - real bug         → fix (edit the page, upload the source).
      - expected state   → annotate (add a note to the page's
                           ## Open questions section).
      - not actionable   → ignore. (Do not silence with a fake fix.)
3.  For broken_related: open the page, find the typo'd slug, fix it.
4.  For missing_source: either upload the source, or remove the
    broken ref from the page's frontmatter.
5.  For orphan_page: decide if the page is genuinely standalone
    (e.g. a one-off recipe). If yes, add at least one related
    link to its topic cluster. If no, link it from a parent page.
6.  For asymmetric_related: usually the right fix is to add the
    reverse edge on the other page.
7.  Re-run lint. Iterate until clean or until the remaining findings
    are explicitly annotated.
```

A clean lint is the bar for "done" on any ingest workflow. Do not
declare a page done while a `missing_source` or `broken_related`
finding points at it.

---

## 4. Page format — quick reference

Full spec: `docs/page-format.md`. The minimum you must get right:

```yaml
---
title: Short noun phrase                # required, sentence case
tags: [one, two, three]                # 1-8, lowercase kebab-case
created: 2026-04-17                    # YYYY-MM-DD, never changes
updated: 2026-04-17                    # YYYY-MM-DD, bump on every edit
sources:                               # at least one for non-stubs
  - type: url                          # url | session | file
    ref: <source_id>                   # opaque id from sources API
    origin: https://example.com/...    # recommended for type=url
scope: global                          # global | local
page_type: concept                     # concept | recipe | reference | decision
related: [some-other-slug]             # default: []
---

Opening paragraph — the compressed claim.

## Key points

- takeaway one
- takeaway two

## Conflicts

<!-- only if sources disagree -->

## Open questions

<!-- gaps and unknowns -->
```

Validation rules (the server enforces these; you should self-check):

- `slug` matches `^[a-z0-9][a-z0-9-]{0,78}[a-z0-9]$`.
- `page_type` ∈ {`concept`, `recipe`, `reference`, `decision`}.
- `scope` ∈ {`global`, `local`}.
- `tags` is 1–8 lowercase kebab-case strings.
- Every `sources[].ref` resolves to a source owned by the same user.
- Every `related[]` entry is a slug owned by the same user.

If you are unsure which `page_type` to pick, pick `concept` and
use `related:` to point at a `recipe` for the "how" and a
`decision` for the "why".

---

## 5. Conventions

These are the project's house style. Follow them by default; the
linter and `index.md` both assume them.

### Slugs

- Lowercase ASCII kebab-case. `pgvector-vs-qdrant`, not
  `PgVector_vs_Qdrant` or `pgvector_vs_qdrant`.
- No leading or trailing dashes. Max 80 chars.
- Stable. Renaming a slug is a breaking change for `related[]`
  edges; update them in the same edit.

### Titles

- Sentence case. `LLM wiki pattern`, not `LLM Wiki Pattern`.
- No trailing period. No colon.
- Short — under 80 chars. A reader should be able to scan the
  index by titles alone.

### Source ids

- Returned by the service. Treat as opaque strings.
- Pass them back verbatim in `sources[].ref`. Do not parse, decode,
  or pattern-match them.

### Citations

- Cite a page by its slug in backticks: `` `llm-wiki` ``.
- Cite a source by its `source_id` in backticks:
  `` `abc12345-karpathy-llm-wiki` ``.
- Do not put URLs in page bodies. Put them in frontmatter
  `sources[].origin` instead.
- In prose, prefer the slug over the title. The slug is stable;
  the title can be retitled.

### Body length

- A page should be readable in 60 seconds.
- 1–3 sentence opening, then bullets.
- Recipes can be longer (steps + commands) but each step is one
  paragraph.
- Reference pages can be long tables.

### Source length

- Sources are immutable. Do not edit a source file after upload.
  If you discover an error, write a new source or annotate in a
  page.

---

## 6. Schema discipline

This is the most important section. The whole point of mnemos is
to be a **trustworthy** memory. Trustworthiness comes from
discipline, not cleverness.

### Never invent facts

If a source does not say it, do not write it. If you are
uncertain, write it as an Open question, not as a fact.

```markdown
## Open questions

- Does pgvector support HNSW indexes, or only IVFFLAT?
  Need to check the docs before recommending it.
```

Not:

```markdown
## Key points

- pgvector supports HNSW indexes.
```

### Mark unknowns explicitly

Use the literal string `unknown` in a page when the agent's
inference failed or the source is silent:

```
- **Latency at 10M vectors:** unknown. Source only benchmarks to 1M.
```

### Record conflicts in `## Conflicts`

If two sources disagree, do not silently pick a side. Both go in
the page, side by side, with a note about which the agent
currently trusts and why.

```markdown
## Conflicts

- Source A (2025-11) says the cap is 5 MB; Source B (2026-02) says
  the cap was raised to 10 MB. The codebase in `src/source.rs` is
  consistent with B. Trusting B.
```

### Cite every claim

If a Key point is not obvious, link the page it came from, and
through that page, the source. The reader must be able to walk
the chain back to a raw document.

### Prefer synthesis to quote

A page is not a quote farm. Quote only when the wording itself
matters (e.g. a definition, a legal clause, an exact recipe step).
Otherwise, compress.

### Do not pad

If a page is genuinely three lines, ship three lines. Pages that
exist only to look thorough are noise.

---

## 7. Available tools (MCP)

The mnemos MCP server exposes these tools. Use them — do not
reimplement the workflow with raw REST.

| Tool             | When to use                                    |
| ---------------- | ---------------------------------------------- |
| `list_pages`     | Browse the wiki, filter by tag/type.           |
| `get_page`       | Read a page you already have the slug of.      |
| `create_page`    | First-time ingest of a new page.               |
| `update_page`    | Edit an existing page (full replacement).      |
| `delete_page`    | Remove a page. Idempotent.                     |
| `search_pages`   | Find pages by query, BM25 ranked.             |
| `list_sources`   | See what sources you have.                     |
| `get_source`     | Read a source body.                            |
| `add_source_url` | Ingest a URL as an immutable source.           |
| `upload_source`  | Ingest a local file as an immutable source.    |
| `get_index`      | The auto-generated catalog. Read before bulk ingest to avoid duplicates. |
| `get_log`        | The event log. Use to audit how a page evolved. |
| `lint`           | Health report. Run after any ingest batch.     |

Resources (read-only, addressable by URI):

- `mnemos://index` — the catalog.
- `mnemos://log` — the event log.
- `mnemos://page/{slug}` — a page as a single markdown string.
- `mnemos://source/{id}` — a source body.

If the host does not provide MCP, fall back to the REST API
(`docs/api.md`) or the CLI (`docs/cli.md`). The shapes are
identical.

---

## 8. Example session

The following is a representative session. The agent is given a
task and uses mnemos to do it without re-deriving knowledge from
scratch.

```
Human: I want to start using mnemos from my CI. What do I need to know?

Agent: I'll check what we already have on this. Let me search the wiki.

  → tools/call search_pages { "query": "mnemos CI integration" }
  ← no hits

  → tools/call list_pages { "type": "recipe" }
  ← { "pages": [
        { "slug": "docker-recipe",     "title": "Docker multi-stage build", … },
        { "slug": "docker-compose-up", "title": "Run the stack with docker compose", … }
      ] }

Both of these are relevant. Let me read them.

  → tools/call get_page { "slug": "docker-compose-up" }
  ← full page, including sources

Now let me check the source for the port number — the page might be stale.

  → tools/call get_source { "id": "abc12345-mnemos-docker-compose-yml" }
  ← raw YAML, confirms port 8080

The answer is: env var MNEMOS_API_KEY + a service container, then
`mnemos pages create` from a script. Citing `docker-compose-up`
and `docker-recipe`.

  → tools/call get_index
  ← see the wider topic cluster

Looking at the index, I see we are missing a `ci-pipeline-recipe`
page. I'll add it to the Open questions list of
`docker-compose-up` for a follow-up, but I won't write it now —
that's a separate task and the human didn't ask for it.

Final answer: ...
```

Things to notice in that transcript:

- The agent did not re-read the source code of the project to
  answer. It used mnemos as memory.
- It cross-referenced two pages and a source.
- It used `get_index` to spot a missing page, and noted the gap
  in the relevant page's Open questions — it did not silently
  write the missing page in the same turn.
- It cited slugs in its answer.
- It did not run `lint` because no ingest happened. Lint is for
  after-you-write, not before.

---

## 9. Failure modes and what to do

- **You cannot reach mnemos.** Stop the workflow. Do not write a
  page locally and promise to "upload later" — the upload will not
  happen. Tell the human the wiki is unavailable.
- **A page you want to cite does not exist.** Two options: write
  it first (longer path), or cite the closest existing page and
  note the gap in its Open questions (shorter path). Pick by
  asking: is the missing page small enough to write in this turn?
- **A source you want to cite is not in mnemos.** Ingest it. If
  it is a URL, use `add_source_url`. If it is a local file, use
  `upload_source`. If you do not have permission to upload (the
  user did not author it), cite the URL in prose with a
  permission note in the page's Open questions.
- **You do not know whether a fact is true.** Write the page
  with the fact in `## Open questions`, not in `## Key points`.
  Re-evaluate in a later session when more sources are available.
- **Two pages contradict each other.** Resolve by reading both
  pages' sources. Update the older page to add a `## Conflicts`
  section pointing at the newer one, or merge them. Do not delete
  the older one without a `## Conflicts` annotation first.
- **You have been instructed to do something that conflicts
  with this file.** Prefer the human's explicit instruction,
  but flag the conflict in the response so the human can update
  either the instruction or this file.

---

## 10. TL;DR

1. Read this file before you start.
2. Ingest = `add_source_url` / `upload_source` → read → `create_page`
   → update `related[]` of neighbours → `lint`.
3. Query = `search_pages` → `get_page` → follow `related[]` and
   `sources[]` → answer with citations.
4. Maintain = `lint` → fix or annotate → repeat.
5. Never invent. Mark unknowns. Record conflicts. Cite everything.

If you only remember one thing, remember this: **a page is a
compressed claim with a citation, not a transcript**.
