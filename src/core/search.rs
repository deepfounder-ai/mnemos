//! FTS5-backed search. Wraps the `pages_fts` virtual table to rank
//! results by `bm25()` and respect user isolation.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::Result;
use crate::storage::AppState;

/// A single search hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageHit {
    pub slug: String,
    pub title: String,
    pub snippet: String,
    pub rank: f64,
}

/// Search service.
pub struct SearchService {
    pool: SqlitePool,
}

impl std::fmt::Debug for SearchService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchService").finish()
    }
}

impl SearchService {
    pub fn new(state: &AppState) -> Self {
        Self { pool: state.db.clone() }
    }

    /// Run a FTS5 query against `pages_fts` for a single user. Hits are
    /// ordered by `bm25()` rank (lower = more relevant). Returns at most
    /// `limit` rows.
    ///
    /// Multi-word queries are turned into an FTS5 OR expression so that
    /// either word matches (BM25 will still rank the best result first).
    /// If the caller wants a phrase match, they can quote the input
    /// themselves.
    pub async fn search(&self, user_id: &str, query: &str, limit: i64) -> Result<Vec<PageHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }

        // If the user already used FTS5 operators (AND, OR, NEAR, ...), pass
        // the query through verbatim. Otherwise, treat whitespace-separated
        // tokens as OR-joined terms.
        let fts_query = if q.contains('"') || q.contains(':') || q.contains(" AND ") || q.contains(" OR ") {
            q.to_string()
        } else {
            let terms: Vec<String> = q
                .split_whitespace()
                .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
                .collect();
            terms.join(" OR ")
        };

        let limit = limit.clamp(1, 1000);

        let rows = sqlx::query(
            r#"
            SELECT p.slug AS slug,
                   p.title AS title,
                   snippet(pages_fts, 1, '<mark>', '</mark>', '…', 12) AS snippet,
                   bm25(pages_fts) AS rank
            FROM pages_fts
            JOIN pages p ON p.rowid = pages_fts.rowid
            WHERE pages_fts MATCH ?1
              AND p.user_id = ?2
            ORDER BY rank
            LIMIT ?3
            "#,
        )
        .bind(&fts_query)
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut hits = Vec::with_capacity(rows.len());
        for r in &rows {
            hits.push(PageHit {
                slug: r.try_get("slug")?,
                title: r.try_get("title")?,
                snippet: r.try_get("snippet")?,
                rank: r.try_get("rank")?,
            });
        }
        Ok(hits)
    }
}
