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
| `MNEMOS_ENRICH_RELATED_THRESHOLD` | `0.75` | `noul` cut-off for adding an edge. |
| `MNEMOS_ENRICH_TAG_THRESHOLD` | `0.8` | `noul` cut-off for adding a tag. |
| `MNEMOS_ENRICH_TYPE_OVERRIDE` | `false` | Allow overwriting an existing `page_type`. |
| `MNEMOS_ENRICH_MAX_ADDITIONS` | `8` | Max tags and max edges added per pass. |

## Thresholds are unvalidated

The defaults are starting points, not tuned values. Calibrate before trusting
them: the existing `related[]` graph is free ground truth — strip edges from a
copy of the corpus, run enrichment, and measure how many original edges come
back (recall) against how many new edges are wrong (precision). Pick the
threshold from that curve, not from the default.

## Cost

$0.042 per 1M input tokens, output free. One pass is one request of roughly the
page opening plus ~70 short questions — a full backfill of a few hundred pages
costs well under a cent.
