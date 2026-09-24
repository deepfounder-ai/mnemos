//! TypeSafe/Jev knowledge enrichment.
//!
//! An optional pass that uses the Jev decision model to:
//!
//! - classify a page's `page_type` (fill when missing; overwrite only when
//!   `type_override` is set),
//! - assign tags from the corpus's existing tag vocabulary (Jev never invents
//!   new tags — it only matches against a closed list),
//! - discover missing `related` edges by asking, pairwise, whether two pages
//!   should be cross-linked, then adding the edge on both pages.
//!
//! The pass only **adds** metadata; it never removes what an agent wrote (the
//! one exception is overwriting `page_type`, gated behind `type_override`).
//! It writes back through [`PageService`] directly, so it does not re-trigger
//! the API-layer create/update hook.
//!
//! Disabled unless `MNEMOS_TYPESAFE_API_KEY` is set — with no key,
//! [`enrich_page`] is a no-op and the server makes no external calls.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::core::frontmatter::PageType;
use crate::core::page::{Page, PageFilter, PageService};
use crate::core::typesafe::{q_choice, q_noul, TypeSafeClient};
use crate::error::Result;
use crate::storage::AppState;

/// What an enrichment pass changed for one page.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EnrichReport {
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_type_set: Option<String>,
    pub tags_added: Vec<String>,
    pub related_added: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

// Bound the questions per request so token cost and latency stay predictable.
const MAX_TAG_CANDIDATES: usize = 40;
const MAX_RELATED_CANDIDATES: usize = 30;
const TYPE_OVERRIDE_MIN_CONFIDENCE: f64 = 0.6;
const OPENING_CHARS: usize = 800;

/// Enrich a single page in place. Returns a report of what changed. When the
/// feature is disabled the report's `skipped` field explains why.
pub async fn enrich_page(state: &AppState, user_id: &str, slug: &str) -> Result<EnrichReport> {
    let mut report = EnrichReport {
        slug: slug.to_string(),
        ..Default::default()
    };

    let Some(client) = TypeSafeClient::from_config(&state.config.enrich) else {
        report.skipped = Some("enrichment disabled (no MNEMOS_TYPESAFE_API_KEY)".into());
        return Ok(report);
    };
    let cfg = &state.config.enrich;
    let svc = PageService::new(state.clone());

    // One read of the whole corpus: we need the tag vocabulary and the
    // candidate pool for related edges.
    let all = svc.list(user_id, &PageFilter::default()).await?;
    let Some(target) = all.iter().find(|p| p.slug == slug).cloned() else {
        report.skipped = Some("page not found".into());
        return Ok(report);
    };

    let opening: String = target.body.chars().take(OPENING_CHARS).collect();
    let st = json!({
        "title": target.title,
        "tags": target.frontmatter.tags,
        "content": opening,
    });

    let mut questions = Map::new();
    let want_type = target.frontmatter.page_type.is_none() || cfg.type_override;
    if want_type {
        questions.insert("__page_type".into(), page_type_question());
    }

    // Tag candidates: distinct corpus tags the page does not already carry,
    // ranked by how common they are (common tags are the shared vocabulary).
    let existing_tags: BTreeSet<&String> = target.frontmatter.tags.iter().collect();
    let tag_candidates = tag_vocabulary(&all, &existing_tags);
    for tag in &tag_candidates {
        questions.insert(
            format!("tag::{tag}"),
            q_noul(&format!("This page is fundamentally about the topic \"{tag}\".")),
        );
    }

    // Related candidates: pages sharing at least one tag, not self, not already
    // linked, ranked by tag overlap.
    let related_candidates = related_candidates(&target, &all);
    for cand in &related_candidates {
        let tags = cand.frontmatter.tags.join(", ");
        questions.insert(
            format!("rel::{}", cand.slug),
            q_noul(&format!(
                "This page and a separate page titled \"{}\" (topics: {}) cover closely \
                 related material and a reader of one would benefit from a link to the other.",
                cand.title, tags
            )),
        );
    }

    if questions.is_empty() {
        report.skipped = Some("nothing to ask (type set, no candidates)".into());
        return Ok(report);
    }

    let answers = match client.decide(st, Value::Object(questions)).await {
        Ok(a) => a,
        Err(e) => {
            report.skipped = Some(format!("typesafe error: {e}"));
            return Ok(report);
        }
    };

    // Build the mutated frontmatter for the target.
    let mut fm = target.frontmatter.clone();
    let mut changed = false;

    // page_type
    if want_type {
        if let Some((choice, confidence)) = answers.choice("__page_type") {
            if choice != "none" {
                if let Some(pt) = parse_page_type(&choice) {
                    let fill = fm.page_type.is_none();
                    let override_ok = cfg.type_override
                        && fm.page_type != Some(pt)
                        && confidence >= TYPE_OVERRIDE_MIN_CONFIDENCE;
                    if fill || override_ok {
                        fm.page_type = Some(pt);
                        report.page_type_set = Some(choice);
                        changed = true;
                    }
                }
            }
        }
    }

    // tags — accept above threshold, capped
    let mut scored_tags: Vec<(String, f64)> = tag_candidates
        .iter()
        .filter_map(|t| answers.noul(&format!("tag::{t}")).map(|p| (t.clone(), p)))
        .filter(|(_, p)| *p >= cfg.tag_threshold)
        .collect();
    scored_tags.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (tag, _) in scored_tags.into_iter().take(cfg.max_additions) {
        if !fm.tags.iter().any(|t| t == &tag) {
            fm.tags.push(tag.clone());
            report.tags_added.push(tag);
            changed = true;
        }
    }

    // related — accept above threshold, capped
    let mut scored_rel: Vec<(String, f64)> = related_candidates
        .iter()
        .filter_map(|c| answers.noul(&format!("rel::{}", c.slug)).map(|p| (c.slug.clone(), p)))
        .filter(|(_, p)| *p >= cfg.related_threshold)
        .collect();
    scored_rel.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let accepted_rel: Vec<String> = scored_rel
        .into_iter()
        .take(cfg.max_additions)
        .map(|(s, _)| s)
        .collect();
    for rel in &accepted_rel {
        if !fm.related.iter().any(|r| r == rel) {
            fm.related.push(rel.clone());
            report.related_added.push(rel.clone());
            changed = true;
        }
    }

    // Persist the target if anything changed.
    if changed {
        svc.update(user_id, slug, fm, &target.body).await?;
    }

    // Add the reverse edge on each newly linked page (bidirectional graph).
    for rel in &report.related_added {
        if let Some(other) = all.iter().find(|p| &p.slug == rel) {
            if !other.frontmatter.related.iter().any(|r| r == slug) {
                let mut ofm = other.frontmatter.clone();
                ofm.related.push(slug.to_string());
                let _ = svc.update(user_id, &other.slug, ofm, &other.body).await;
            }
        }
    }

    Ok(report)
}

/// The `page_type` classification question.
fn page_type_question() -> Value {
    q_choice(
        "What kind of knowledge page is this?",
        json!({
            "concept": "Explains what something is — a definition, a mechanism, a pattern.",
            "recipe": "Explains how to do something — steps, commands, a procedure.",
            "reference": "A lookup: tables, parameters, a catalog of facts.",
            "decision": "Records a choice of X over Y and the reasons for it.",
            "none": "Fits none of the above."
        }),
    )
}

/// Distinct tags across the corpus that the target lacks, most common first.
fn tag_vocabulary(all: &[Page], existing: &BTreeSet<&String>) -> Vec<String> {
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    for p in all {
        for t in &p.frontmatter.tags {
            if !existing.contains(t) {
                *freq.entry(t.clone()).or_default() += 1;
            }
        }
    }
    let mut v: Vec<(String, usize)> = freq.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(MAX_TAG_CANDIDATES).map(|(t, _)| t).collect()
}

/// Pages sharing ≥1 tag with the target, excluding self and already-linked,
/// ranked by tag overlap descending.
fn related_candidates(target: &Page, all: &[Page]) -> Vec<Page> {
    let target_tags: BTreeSet<&String> = target.frontmatter.tags.iter().collect();
    let already: BTreeSet<&String> = target.frontmatter.related.iter().collect();
    let mut scored: Vec<(usize, Page)> = all
        .iter()
        .filter(|p| p.slug != target.slug && !already.contains(&p.slug))
        .filter_map(|p| {
            let overlap = p
                .frontmatter
                .tags
                .iter()
                .filter(|t| target_tags.contains(t))
                .count();
            if overlap > 0 {
                Some((overlap, p.clone()))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.slug.cmp(&b.1.slug)));
    scored
        .into_iter()
        .take(MAX_RELATED_CANDIDATES)
        .map(|(_, p)| p)
        .collect()
}

fn parse_page_type(s: &str) -> Option<PageType> {
    serde_json::from_value(Value::String(s.to_string())).ok()
}

/// Enrich every page for a user (backfill). Returns per-page reports.
pub async fn enrich_all(state: &AppState, user_id: &str) -> Result<Vec<EnrichReport>> {
    let svc = PageService::new(state.clone());
    let slugs: Vec<String> = svc
        .list(user_id, &PageFilter::default())
        .await?
        .into_iter()
        .map(|p| p.slug)
        .collect();
    let mut reports = Vec::with_capacity(slugs.len());
    for slug in slugs {
        reports.push(enrich_page(state, user_id, &slug).await?);
    }
    Ok(reports)
}
