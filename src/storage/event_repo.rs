//! `events` table access — append-only audit log mirror.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct EventRow {
    pub id: i64,
    pub user_id: String,
    pub kind: String,
    pub ref_: Option<String>,
    pub ts: DateTime<Utc>,
    pub payload_json: Option<String>,
}

impl EventRow {
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self> {
        let parse_dt = |s: String| -> Result<DateTime<Utc>> {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| {
                    crate::error::AppError::Internal(format!("invalid datetime '{s}': {e}"))
                })
        };
        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            kind: row.try_get("kind")?,
            ref_: row.try_get("ref_").ok().or_else(|| row.try_get("ref").ok()),
            ts: parse_dt(row.try_get("ts")?)?,
            payload_json: row.try_get("payload_json")?,
        })
    }
}

pub async fn append(
    pool: &SqlitePool,
    user_id: &str,
    kind: &str,
    ref_: Option<&str>,
    payload: Option<&serde_json::Value>,
) -> Result<i64> {
    let now_str = Utc::now().to_rfc3339();
    let payload_str = match payload {
        Some(v) => Some(serde_json::to_string(v)?),
        None => None,
    };

    let res = sqlx::query(
        "INSERT INTO events (user_id, kind, ref, ts, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(user_id)
    .bind(kind)
    .bind(ref_)
    .bind(&now_str)
    .bind(payload_str)
    .execute(pool)
    .await?;
    Ok(res.last_insert_rowid())
}

#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

pub async fn list_for_user(
    pool: &SqlitePool,
    user_id: &str,
    filter: &EventFilter,
) -> Result<Vec<EventRow>> {
    let limit = filter.limit.unwrap_or(100).clamp(1, 1000);
    let rows = match filter.since {
        Some(since) => {
            let since_str = since.to_rfc3339();
            sqlx::query(
                "SELECT * FROM events WHERE user_id = ?1 AND ts >= ?2 ORDER BY ts DESC LIMIT ?3",
            )
            .bind(user_id)
            .bind(&since_str)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query("SELECT * FROM events WHERE user_id = ?1 ORDER BY ts DESC LIMIT ?2")
                .bind(user_id)
                .bind(limit)
                .fetch_all(pool)
                .await?
        }
    };
    rows.iter().map(EventRow::from_row).collect()
}
