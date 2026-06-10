//! `index.md` builder. Generates a catalog of all pages for a user,
//! grouped by `page_type` (concept / recipe / reference / decision).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::core::frontmatter::PageType;
use crate::error::Result;
use crate::storage::{fs_layout, page_repo, AppState};

#[derive(Debug)]
pub struct IndexBuilder {
    state: AppState,
}

impl IndexBuilder {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Rebuild the index.md for a user. Reads all pages, groups them by
    /// `page_type`, and writes the result to
    /// `data/users/<id>/index.md`.
    pub async fn rebuild_for_user(&self, user_id: &str) -> Result<()> {
        fs_layout::ensure_user_dirs(&self.state.config.data_dir, user_id)?;
        let rows = page_repo::list_for_user(&self.state.db, user_id).await?;
        let mut groups: BTreeMap<&'static str, Vec<IndexEntry>> = BTreeMap::new();
        for row in &rows {
            let fm: crate::core::frontmatter::Frontmatter =
                serde_json::from_str(&row.frontmatter_json).unwrap_or_default();
            let key = fm.page_type.map(|p| p.as_str()).unwrap_or("uncategorized");
            groups.entry(key).or_default().push(IndexEntry {
                slug: row.slug.clone(),
                title: row.title.clone(),
                tags: fm.tags,
                updated_at: row.updated_at,
            });
        }
        let body = render(&groups);
        let path = fs_layout::index_path(&self.state.config.data_dir, user_id);
        tokio::fs::write(&path, &body).await?;
        Ok(())
    }
}

#[derive(Clone)]
struct IndexEntry {
    slug: String,
    title: String,
    tags: Vec<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn render(groups: &BTreeMap<&'static str, Vec<IndexEntry>>) -> String {
    let mut out = String::new();
    out.push_str("# Index\n\n");
    out.push_str("Auto-generated catalog of all wiki pages. Re-built on every write.\n\n");
    out.push_str(&format!(
        "_Total pages: _{}\n\n",
        groups.values().map(|v| v.len()).sum::<usize>()
    ));

    for (kind, entries) in groups {
        if entries.is_empty() {
            continue;
        }
        let _ = writeln!(out, "## {}\n", title_case(kind));
        // Sort by title for stability.
        let mut sorted = entries.clone();
        sorted.sort_by_key(|a| a.title.to_lowercase());
        for e in sorted {
            let tags = if e.tags.is_empty() {
                String::new()
            } else {
                format!(" `{}`", e.tags.join(", "))
            };
            let _ = writeln!(
                out,
                "- [{}](pages/{}.md){}  — _{}_",
                e.title,
                e.slug,
                tags,
                e.updated_at.format("%Y-%m-%d")
            );
        }
        out.push('\n');
    }
    out
}

fn title_case(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(c) => c.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

#[allow(dead_code)]
const ALL_TYPES: &[PageType] = &[
    PageType::Concept,
    PageType::Recipe,
    PageType::Reference,
    PageType::Decision,
];
