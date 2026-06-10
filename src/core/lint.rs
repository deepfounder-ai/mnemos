//! Linter. Inspects the wiki for orphans, broken `related:` links,
//! missing source references, and conflicts. Returns a structured
//! report — does **not** modify any state.

use std::collections::HashSet;

use serde::Serialize;

use crate::core::frontmatter::Frontmatter;
use crate::error::Result;
use crate::storage::{page_repo, source_repo, AppState};

/// Severity of a finding.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Kind of finding. Stable identifier used by clients to filter.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// Page has no `related:` links and nothing links to it.
    Orphan,
    /// `related:` points to a slug that does not exist.
    BrokenRelated,
    /// Page frontmatter references a source id that is not in the DB.
    MissingSource,
    /// Two pages share the same slug after slugify.
    SlugConflict,
    /// Frontmatter failed to parse; the page is rendered with empty fields.
    InvalidFrontmatter,
}

#[derive(Debug, Clone, Serialize)]
pub struct LintFinding {
    pub severity: Severity,
    pub kind: FindingKind,
    /// Slug or source id the finding refers to.
    pub ref_: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LintReport {
    pub user_id: String,
    pub findings: Vec<LintFinding>,
}

#[derive(Debug)]
pub struct Linter {
    state: AppState,
}

impl Linter {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Run the full set of lints for a user.
    pub async fn run(&self, user_id: &str) -> Result<LintReport> {
        let pages = page_repo::list_for_user(&self.state.db, user_id).await?;
        let sources = source_repo::list_for_user(&self.state.db, user_id).await?;

        // First pass: collect every slug the user owns. We need this *before*
        // we can decide whether a `related:` link is broken, because pages
        // are not returned in slug-alphabetical order.
        let slugs: HashSet<String> = pages.iter().map(|p| p.slug.clone()).collect();

        let mut findings = Vec::new();
        let mut referenced: HashSet<String> = HashSet::new();
        let mut seen_slugs: HashSet<String> = HashSet::new();

        for row in &pages {
            // Slug conflict (defensive: the DB has a unique constraint, but a
            // migration might have left duplicates behind).
            if !seen_slugs.insert(row.slug.clone()) {
                findings.push(LintFinding {
                    severity: Severity::Error,
                    kind: FindingKind::SlugConflict,
                    ref_: row.slug.clone(),
                    message: format!("slug '{}' appears more than once", row.slug),
                });
            }

            // Parse frontmatter.
            let fm: Frontmatter = match serde_json::from_str(&row.frontmatter_json) {
                Ok(f) => f,
                Err(e) => {
                    findings.push(LintFinding {
                        severity: Severity::Error,
                        kind: FindingKind::InvalidFrontmatter,
                        ref_: row.slug.clone(),
                        message: format!("frontmatter JSON is malformed: {e}"),
                    });
                    continue;
                }
            };

            // Broken `related:` and track reverse references.
            for r in &fm.related {
                referenced.insert(r.clone());
                if !slugs.contains(r) {
                    // Could be a forward reference, or a typo — flag it but
                    // with Warning so the user can choose to ignore.
                    findings.push(LintFinding {
                        severity: Severity::Warning,
                        kind: FindingKind::BrokenRelated,
                        ref_: row.slug.clone(),
                        message: format!("related slug '{r}' does not exist"),
                    });
                }
            }

            // Missing source references.
            for s in &fm.sources {
                // The `ref_` field is the relative path, e.g. `sources/<id>-<slug>.md`.
                // Match it against the stored `path` of any source in the DB.
                let found = sources.iter().any(|src| src.path == s.ref_);
                if !found {
                    findings.push(LintFinding {
                        severity: Severity::Warning,
                        kind: FindingKind::MissingSource,
                        ref_: row.slug.clone(),
                        message: format!(
                            "frontmatter references source '{}' which is not present in the DB",
                            s.ref_
                        ),
                    });
                }
            }
        }

        // Orphan detection: a page is orphan if no other page lists it in
        // `related:` and it itself has no `related:`.
        for row in &pages {
            let fm: Frontmatter = serde_json::from_str(&row.frontmatter_json).unwrap_or_default();
            if fm.related.is_empty() && !referenced.contains(&row.slug) {
                findings.push(LintFinding {
                    severity: Severity::Info,
                    kind: FindingKind::Orphan,
                    ref_: row.slug.clone(),
                    message: format!(
                        "page '{}' has no related links and is not referenced",
                        row.slug
                    ),
                });
            }
        }

        Ok(LintReport {
            user_id: user_id.to_string(),
            findings,
        })
    }
}
