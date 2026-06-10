//! Connection pool + application state.

use std::path::Path;
use std::sync::Arc;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{ConnectOptions, SqlitePool};
use std::str::FromStr;
use std::time::Duration;

use crate::config::Config;
use crate::error::Result;

/// Shared application state handed to every service and HTTP handler.
#[derive(Clone)]
pub struct AppState {
    /// SQLite connection pool.
    pub db: SqlitePool,
    /// Effective configuration.
    pub config: Arc<Config>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &*self.config)
            .finish()
    }
}

/// Initialise the SQLite pool and run pending migrations.
///
/// `db_url` is whatever `Config::resolved_db_url()` produced. If the parent
/// directory of the file path does not exist, it is created first.
pub async fn init_pool(config: Config) -> Result<AppState> {
    let url = config.resolved_db_url();

    // Best-effort: make sure the directory exists for file-backed SQLite.
    if let Some(path) = url.split("://").nth(1).and_then(|s| s.split('?').next()) {
        if !path.is_empty() && path != ":memory:" {
            if let Some(parent) = Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }
    }

    let mut opts = SqliteConnectOptions::from_str(&url)
        .map_err(|e| crate::error::AppError::Internal(format!("invalid db url: {e}")))?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    // Disable verbose sqlx logging unless RUST_LOG/SQLX is at debug.
    opts = opts.log_statements(tracing::log::LevelFilter::Trace);

    // Run a one-shot PRAGMA before sqlx opens its migration transaction. The
    // migration file itself is wrapped in a transaction, and SQLite refuses
    // to switch journal_mode / synchronous / foreign_keys inside a tx.
    let probe_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(opts.clone())
        .await?;
    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&probe_pool)
        .await?;
    // Best-effort safety level for the migration: don't downgrade synchronous
    // if it can't be set inside the migration tx.
    let _ = sqlx::query("PRAGMA journal_mode = WAL;")
        .execute(&probe_pool)
        .await?;
    let _ = sqlx::query("PRAGMA synchronous = NORMAL;")
        .execute(&probe_pool)
        .await?;
    probe_pool.close().await;

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(opts)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(AppState {
        db: pool,
        config: Arc::new(config),
    })
}
