//! `sources` table access.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{AppError, Result};

/// Source row.
#[derive(Debug, Clone)]
pub struct SourceRow {
    pub id: String,
    pub user_id: String,
    pub source_id: String,
    pub slug: String,
    pub r#type: String,
    pub origin: Option<String>,
    pub path: String,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
}

impl SourceRow {
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self> {
        let parse_dt = |s: String| -> Result<DateTime<Utc>> {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| AppError::Internal(format!("invalid datetime '{s}': {e}")))
        };
        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            source_id: row.try_get("source_id")?,
            slug: row.try_get("slug")?,
            r#type: row.try_get("type")?,
            origin: row.try_get("origin")?,
            path: row.try_get("path")?,
            content_hash: row.try_get("content_hash")?,
            created_at: parse_dt(row.try_get("created_at")?)?,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &SqlitePool,
    user_id: &str,
    source_id: &str,
    slug: &str,
    r#type: &str,
    origin: Option<&str>,
    path: &str,
    content_hash: &str,
) -> Result<SourceRow> {
    let id = Uuid::now_v7().to_string();
    let now_str = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO sources (id, user_id, source_id, slug, type, origin, path, content_hash, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(source_id)
    .bind(slug)
    .bind(r#type)
    .bind(origin)
    .bind(path)
    .bind(content_hash)
    .bind(&now_str)
    .execute(pool)
    .await?;

    get_by_id(pool, &id)
        .await?
        .ok_or_else(|| AppError::Internal("inserted source not found".into()))
}

pub async fn get_by_id(pool: &SqlitePool, id: &str) -> Result<Option<SourceRow>> {
    let row = sqlx::query("SELECT * FROM sources WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(SourceRow::from_row(&r)?)),
        None => Ok(None),
    }
}

pub async fn get_by_source_id(
    pool: &SqlitePool,
    user_id: &str,
    source_id: &str,
) -> Result<Option<SourceRow>> {
    let row = sqlx::query("SELECT * FROM sources WHERE user_id = ?1 AND source_id = ?2")
        .bind(user_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(SourceRow::from_row(&r)?)),
        None => Ok(None),
    }
}

pub async fn list_for_user(pool: &SqlitePool, user_id: &str) -> Result<Vec<SourceRow>> {
    let rows = sqlx::query("SELECT * FROM sources WHERE user_id = ?1 ORDER BY created_at DESC")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(SourceRow::from_row).collect()
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool> {
    let n = sqlx::query("DELETE FROM sources WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(n > 0)
}

/// Replace the `page_sources` join rows for a given page.
pub async fn set_page_sources(
    pool: &SqlitePool,
    page_id: &str,
    source_db_ids: &[String],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM page_sources WHERE page_id = ?1")
        .bind(page_id)
        .execute(&mut *tx)
        .await?;
    for sid in source_db_ids {
        sqlx::query("INSERT OR IGNORE INTO page_sources (page_id, source_id) VALUES (?1, ?2)")
            .bind(page_id)
            .bind(sid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
