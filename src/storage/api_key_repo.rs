//! `api_keys` table access.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct ApiKeyRow {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl ApiKeyRow {
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self> {
        let parse_dt = |s: String| -> Result<DateTime<Utc>> {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| AppError::Internal(format!("invalid datetime '{s}': {e}")))
        };
        let parse_dt_opt = |s: Option<String>| -> Result<Option<DateTime<Utc>>> {
            match s {
                Some(v) => Ok(Some(parse_dt(v)?)),
                None => Ok(None),
            }
        };
        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            name: row.try_get("name")?,
            key_hash: row.try_get("key_hash")?,
            created_at: parse_dt(row.try_get("created_at")?)?,
            last_used_at: parse_dt_opt(row.try_get("last_used_at")?)?,
        })
    }
}

pub async fn insert(
    pool: &SqlitePool,
    user_id: &str,
    name: &str,
    key_hash: &str,
) -> Result<ApiKeyRow> {
    let id = Uuid::now_v7().to_string();
    let now_str = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO api_keys (id, user_id, name, key_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(name)
    .bind(key_hash)
    .bind(&now_str)
    .execute(pool)
    .await?;

    get_by_id(pool, &id)
        .await?
        .ok_or_else(|| AppError::Internal("inserted api key not found".into()))
}

pub async fn get_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ApiKeyRow>> {
    let row = sqlx::query("SELECT * FROM api_keys WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(ApiKeyRow::from_row(&r)?)),
        None => Ok(None),
    }
}

pub async fn get_by_hash(pool: &SqlitePool, key_hash: &str) -> Result<Option<ApiKeyRow>> {
    let row = sqlx::query("SELECT * FROM api_keys WHERE key_hash = ?1")
        .bind(key_hash)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(ApiKeyRow::from_row(&r)?)),
        None => Ok(None),
    }
}

pub async fn list_for_user(pool: &SqlitePool, user_id: &str) -> Result<Vec<ApiKeyRow>> {
    let rows = sqlx::query("SELECT * FROM api_keys WHERE user_id = ?1 ORDER BY created_at DESC")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(ApiKeyRow::from_row).collect()
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool> {
    let n = sqlx::query("DELETE FROM api_keys WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(n > 0)
}

/// Refresh `last_used_at` for an api key. Best-effort; errors are logged but
/// not propagated to the caller — we never want a logging side-effect to fail
/// a request.
pub async fn touch_last_used(pool: &SqlitePool, id: &str) {
    let now_str = Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query("UPDATE api_keys SET last_used_at = ?1 WHERE id = ?2")
        .bind(&now_str)
        .bind(id)
        .execute(pool)
        .await
    {
        tracing::warn!(error = %e, api_key_id = %id, "failed to touch api_key last_used_at");
    }
}
