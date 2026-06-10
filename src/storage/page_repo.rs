//! `pages` table access.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{AppError, Result};

/// Row representation of the `pages` table.
#[derive(Debug, Clone)]
pub struct PageRow {
    pub id: String,
    pub user_id: String,
    pub slug: String,
    pub title: String,
    pub frontmatter_json: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PageRow {
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self> {
        let parse_dt = |s: String| -> Result<DateTime<Utc>> {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| AppError::Internal(format!("invalid datetime '{s}': {e}")))
        };
        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            slug: row.try_get("slug")?,
            title: row.try_get("title")?,
            frontmatter_json: row.try_get("frontmatter_json")?,
            body: row.try_get("body")?,
            created_at: parse_dt(row.try_get("created_at")?)?,
            updated_at: parse_dt(row.try_get("updated_at")?)?,
        })
    }
}

/// Insert a new page row. Returns the row.
#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &SqlitePool,
    user_id: &str,
    slug: &str,
    title: &str,
    frontmatter_json: &str,
    body: &str,
) -> Result<PageRow> {
    let id = Uuid::now_v7().to_string();
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO pages (id, user_id, slug, title, frontmatter_json, body, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(slug)
    .bind(title)
    .bind(frontmatter_json)
    .bind(body)
    .bind(&now_str)
    .bind(&now_str)
    .execute(pool)
    .await?;

    get_by_id(pool, &id)
        .await?
        .ok_or_else(|| AppError::Internal("inserted page not found".into()))
}

/// Fetch a page by primary key.
pub async fn get_by_id(pool: &SqlitePool, id: &str) -> Result<Option<PageRow>> {
    let row = sqlx::query("SELECT * FROM pages WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(PageRow::from_row(&r)?)),
        None => Ok(None),
    }
}

/// Fetch a page by (user_id, slug).
pub async fn get_by_slug(pool: &SqlitePool, user_id: &str, slug: &str) -> Result<Option<PageRow>> {
    let row = sqlx::query("SELECT * FROM pages WHERE user_id = ?1 AND slug = ?2")
        .bind(user_id)
        .bind(slug)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(PageRow::from_row(&r)?)),
        None => Ok(None),
    }
}

/// List all pages for a user, ordered by `updated_at` desc.
pub async fn list_for_user(pool: &SqlitePool, user_id: &str) -> Result<Vec<PageRow>> {
    let rows = sqlx::query(
        "SELECT * FROM pages WHERE user_id = ?1 ORDER BY updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(PageRow::from_row).collect()
}

/// Update title, frontmatter and body. `updated_at` is refreshed.
pub async fn update_content(
    pool: &SqlitePool,
    id: &str,
    title: &str,
    frontmatter_json: &str,
    body: &str,
) -> Result<PageRow> {
    let now_str = Utc::now().to_rfc3339();
    let affected = sqlx::query(
        r#"
        UPDATE pages
        SET title = ?1, frontmatter_json = ?2, body = ?3, updated_at = ?4
        WHERE id = ?5
        "#,
    )
    .bind(title)
    .bind(frontmatter_json)
    .bind(body)
    .bind(&now_str)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("page id={id}")));
    }

    get_by_id(pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("page id={id}")))
}

/// Delete a page by id. Returns true if a row was removed.
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool> {
    let n = sqlx::query("DELETE FROM pages WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(n > 0)
}

/// All slugs for a user (used by lint and search).
pub async fn slugs_for_user(pool: &SqlitePool, user_id: &str) -> Result<Vec<String>> {
    let rows = sqlx::query("SELECT slug FROM pages WHERE user_id = ?1")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|r| r.try_get::<String, _>("slug").map_err(AppError::from))
        .collect()
}
