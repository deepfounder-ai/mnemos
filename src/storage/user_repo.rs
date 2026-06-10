//! `users` table access.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{AppError, Result};

/// User row.
#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

impl UserRow {
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self> {
        let parse_dt = |s: String| -> Result<DateTime<Utc>> {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| AppError::Internal(format!("invalid datetime '{s}': {e}")))
        };
        Ok(Self {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            password_hash: row.try_get("password_hash")?,
            created_at: parse_dt(row.try_get("created_at")?)?,
        })
    }
}

pub async fn insert(pool: &SqlitePool, username: &str, password_hash: &str) -> Result<UserRow> {
    let id = Uuid::now_v7().to_string();
    let now_str = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&id)
    .bind(username)
    .bind(password_hash)
    .bind(&now_str)
    .execute(pool)
    .await?;

    get_by_id(pool, &id)
        .await?
        .ok_or_else(|| AppError::Internal("inserted user not found".into()))
}

pub async fn get_by_id(pool: &SqlitePool, id: &str) -> Result<Option<UserRow>> {
    let row = sqlx::query("SELECT * FROM users WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(UserRow::from_row(&r)?)),
        None => Ok(None),
    }
}

pub async fn get_by_username(pool: &SqlitePool, username: &str) -> Result<Option<UserRow>> {
    let row = sqlx::query("SELECT * FROM users WHERE username = ?1")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(UserRow::from_row(&r)?)),
        None => Ok(None),
    }
}
