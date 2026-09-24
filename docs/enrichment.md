# Knowledge enrichment (TypeSafe / Jev)

Optional pass that uses TypeSafe's **Jev** decision model to classify pages and
grow the `related` graph. Off by default — with no API key the server makes no
external calls and behaves exactly as without this feature.

Jev never generates text. It picks from options you declare, so enrichment can
only **classify** and **match against closed lists**; it cannot invent tags or
write content.

## What it does

For one page, in a single Jev request:

| Pass | Jev question | Effect |
|------|--------------|--------|
| `page_type` | `choice` over `concept / recipe / reference / decision / none` | Fills `page_type` when missing. Overwrites only with `MNEMOS_ENRICH_TYPE_OVERRIDE=true` and confidence ≥ 0.6. |
| tags | one `noul` per tag in the corpus vocabulary (top 40 by frequency) | Adds tags scoring ≥ `MNEMOS_ENRICH_TAG_THRESHOLD`. Never invents a tag. |
| related | one `noul` per candidate page sharing ≥ 1 tag (top 30 by overlap) | Adds edges scoring ≥ `MNEMOS_ENRICH_RELATED_THRESHOLD`, **on both pages**. |

Additions are capped at `MNEMOS_ENRICH_MAX_ADDITIONS` per pass. Enrichment only
adds metadata — it never removes tags or edges an agent wrote.

## When it runs

- **Automatically**, in the background, after every `create_page` /
  `update_page`. The write returns immediately; enrichment never fails or
  slows a write. Passes are serialised so two passes can't race on one page.
- **On demand** for backfill:

  ```bash
  mnemos enrich                  # every page
  mnemos enrich --slug my-page   # one page
  # or: POST /api/v1/enrich[?slug=…]   (409 if the feature is disabled)
  ```

Results appear in the server log (`enrich: page processed …`) and, for the
on-demand call, in the response.

## Configuration

| Variable | Default | Purpose |
|----------|---------|---------|
| `MNEMOS_TYPESAFE_API_KEY` | (unset) | Enables the feature. |
| `MNEMOS_TYPESAFE_URL` | `https://api.typesafe.ai/v1/systemone` | Endpoint. |
| `MNEMOS_TYPESAFE_MODEL` | `jev-1.13.0` | **Pinned** model. Don't use `jev-latest` — answers shift between releases. |
| `MNEMOS_ENRICH_RELATED_THRESHOLD` | `0.6` | `noul` cut-off for adding an edge. |
| `MNEMOS_ENRICH_TAG_THRESHOLD` | `0.8` | `noul` cut-off for adding a tag. |
| `MNEMOS_ENRICH_TYPE_OVERRIDE` | `false` | Allow overwriting an existing `page_type`. |
| `MNEMOS_ENRICH_MAX_ADDITIONS` | `8` | Max tags and max edges added per pass. |

## Calibration

Defaults were calibrated on 2026-09-24 against a 58-page corpus with
`jev-1.13.0`, using the corpus's own metadata as ground truth: `related[]` and
`page_type` were stripped and re-predicted; 2 tags per page were hidden and
re-predicted. Precision below is a **lower bound** — the hand-built graph is
incomplete, and a manual review of "false" edges in the 0.60–0.75 band found
~10 of 14 were sensible links that simply hadn't been added.

**related** (167 true edges; 92% reachable via the shared-tag prefilter):

| threshold | precision (lower bound) | recall |
|-----------|------------------------|--------|
| 0.50 | 54% | 50% |
| **0.60** | **59%** | **44%** |
| 0.75 | 62% | 23% |
| 0.80 | 66% | 11% |

Median `noul` for true edges was 0.52 vs 0.17 for non-edges — Jev separates
them, but true-edge scores are moderate, so high thresholds starve the graph.

**page_type**: 79% agreement with the hand labels (88% at confidence ≥ 0.9).
Most disagreements were the corpus over-labelling `concept` (e.g. an "X vs Y"
page predicted as `decision`, a syntax lookup as `reference`) — which is why
enrichment only fills a *missing* type by default.

**tags** at 0.8: 71% precision (lower bound), recovers 15% of hidden tags
against a 56% ceiling (hidden tags unique to one page aren't in the vocabulary).

Re-run calibration if you change the model version or the corpus grows a lot.
Cost of the full run: ~214K input tokens ≈ $0.01.

## Cost

$0.042 per 1M input tokens, output free. One pass is one request of roughly the
page opening plus ~70 short questions — a full backfill of a few hundred pages
costs well under a cent.
