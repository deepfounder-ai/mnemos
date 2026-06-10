//! `log.md` builder. Append-only event log with timestamp prefix.
//!
//! Format: `[YYYY-MM-DDTHH:MM:SSZ] <kind> | <ref>\n`
//!
//! The on-disk log is a denormalised view of the `events` table.

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::storage::{event_repo, fs_layout, AppState};

#[derive(Debug)]
pub struct LogBuilder {
    state: AppState,
}

impl LogBuilder {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Append a single event to the on-disk log.
    pub async fn append(
        &self,
        user_id: &str,
        kind: &str,
        ref_: Option<&str>,
    ) -> Result<()> {
        fs_layout::ensure_user_dirs(&self.state.config.data_dir, user_id)?;
        let path = fs_layout::log_path(&self.state.config.data_dir, user_id);
        let line = format_log_line(Utc::now(), kind, ref_);
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(f, "{line}")?;
        f.sync_all()?;
        Ok(())
    }

    /// Read the full log file. Returns empty string if it doesn't exist yet.
    pub async fn read(&self, user_id: &str) -> Result<String> {
        let path = fs_layout::log_path(&self.state.config.data_dir, user_id);
        match tokio::fs::read_to_string(&path).await {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Rebuild the on-disk log from the `events` table. Useful for
    /// reconciliation; not part of the hot path.
    pub async fn rebuild(&self, user_id: &str) -> Result<usize> {
        fs_layout::ensure_user_dirs(&self.state.config.data_dir, user_id)?;
        let path = fs_layout::log_path(&self.state.config.data_dir, user_id);
        let filter = event_repo::EventFilter { limit: Some(10_000), ..Default::default() };
        let events = event_repo::list_for_user(&self.state.db, user_id, &filter).await?;
        let mut buf = String::new();
        // oldest first in the file, so reverse.
        for e in events.into_iter().rev() {
            buf.push_str(&format_log_line(e.ts, &e.kind, e.ref_.as_deref()));
            buf.push('\n');
        }
        let len = buf.len();
        tokio::fs::write(&path, buf).await?;
        Ok(len)
    }
}

/// Format a single log line.
pub fn format_log_line(ts: DateTime<Utc>, kind: &str, ref_: Option<&str>) -> String {
    let ts = ts.format("%Y-%m-%dT%H:%M:%SZ");
    match ref_ {
        Some(r) => format!("[{ts}] {kind} | {r}"),
        None => format!("[{ts}] {kind}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_line_with_ref() {
        let ts = "2026-04-17T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let line = format_log_line(ts, "page.create", Some("kafka"));
        assert_eq!(line, "[2026-04-17T12:00:00Z] page.create | kafka");
    }

    #[test]
    fn format_line_no_ref() {
        let ts = "2026-04-17T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let line = format_log_line(ts, "user.register", None);
        assert_eq!(line, "[2026-04-17T12:00:00Z] user.register");
    }
}
