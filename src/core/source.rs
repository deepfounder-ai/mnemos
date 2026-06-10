//! Source service — register URLs, accept uploads, fetch raw bytes.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::core::slug;
use crate::error::{AppError, Result};
use crate::storage::{fs_layout, source_repo, AppState};

/// Source service.
#[derive(Clone)]
#[derive(Debug)]
pub struct SourceService {
    state: AppState,
    /// HTTP client used for URL fetches.
    http: reqwest::Client,
}

/// Source metadata returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub source_id: String,
    pub slug: String,
    pub r#type: String,
    pub origin: Option<String>,
    pub path: String,
    pub content_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl SourceService {
    pub fn new(state: AppState) -> Result<Self> {
        let timeout = Duration::from_secs(state.config.source_timeout_secs);
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("mnemos/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(AppError::from)?;
        Ok(Self { state, http })
    }

    /// Fetch a URL and store the response body as a new source.
    pub async fn register_url(
        &self,
        user_id: &str,
        url: &str,
        slug_input: &str,
    ) -> Result<Source> {
        let parsed = url::Url::parse(url)
            .map_err(|e| AppError::Validation(format!("invalid url: {e}")))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(AppError::Validation(format!(
                "unsupported url scheme: {}",
                parsed.scheme()
            )));
        }

        let slug = if slug_input.is_empty() {
            // Use the path's last non-empty segment, slugified.
            parsed
                .path_segments()
                .and_then(|mut s| s.next_back())
                .unwrap_or("source")
                .to_string()
        } else {
            slug::slugify(slug_input)
        };
        let slug = if slug.is_empty() {
            "source".to_string()
        } else {
            slug
        };
        slug::validate(&slug)?;

        let max = self.state.config.max_source_bytes;
        let resp = self.http.get(parsed.clone()).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(AppError::Validation(format!(
                "fetch failed: HTTP {status}"
            )));
        }

        // Bound the response size to avoid OOM.
        let bytes = resp.bytes().await?;
        if bytes.len() > max {
            return Err(AppError::Validation(format!(
                "response too large: {} bytes (max {max})",
                bytes.len()
            )));
        }
        self.persist_bytes(user_id, &slug, "url", Some(url), &bytes).await
    }

    /// Store a raw payload (uploaded file) as a new source.
    pub async fn upload(
        &self,
        user_id: &str,
        slug_input: &str,
        content: &[u8],
    ) -> Result<Source> {
        if content.is_empty() {
            return Err(AppError::Validation("uploaded content is empty".into()));
        }
        if content.len() > self.state.config.max_source_bytes {
            return Err(AppError::Validation(format!(
                "upload too large: {} bytes (max {})",
                content.len(),
                self.state.config.max_source_bytes
            )));
        }
        let slug = if slug_input.is_empty() {
            "upload".to_string()
        } else {
            slug::slugify(slug_input)
        };
        if slug.is_empty() {
            return Err(AppError::Validation("slug did not normalise".into()));
        }
        slug::validate(&slug)?;

        self.persist_bytes(user_id, &slug, "upload", None, content).await
    }

    /// List all sources for a user.
    pub async fn list(&self, user_id: &str) -> Result<Vec<Source>> {
        let rows = source_repo::list_for_user(&self.state.db, user_id).await?;
        Ok(rows.into_iter().map(source_from_row).collect())
    }

    /// Fetch source metadata.
    pub async fn get(&self, user_id: &str, source_id: &str) -> Result<Source> {
        let row = source_repo::get_by_source_id(&self.state.db, user_id, source_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("source '{source_id}'")))?;
        Ok(source_from_row(row))
    }

    /// Read raw bytes for a source.
    pub async fn get_raw(&self, user_id: &str, source_id: &str) -> Result<Vec<u8>> {
        let row = self.get(user_id, source_id).await?;
        let path = self.abs_source_path(user_id, &row.path);
        Ok(tokio::fs::read(&path).await?)
    }

    /// Resolved absolute path for a source. The DB stores a path relative to
    /// the user's directory; this resolves it against `data_dir`.
    fn abs_source_path(&self, user_id: &str, rel_path: &str) -> PathBuf {
        let user_root = self.state.config.data_dir.join("users").join(user_id);
        // Defend against path traversal: `..` would escape `user_root`.
        let candidate = user_root.join(rel_path);
        let normalised = candidate
            .components()
            .fold(PathBuf::new(), |mut acc, c| {
                match c {
                    std::path::Component::ParentDir => {
                        acc.pop();
                    }
                    other => acc.push(other.as_os_str()),
                }
                acc
            });
        if normalised.starts_with(&user_root) {
            normalised
        } else {
            user_root // safest fallback
        }
    }

    async fn persist_bytes(
        &self,
        user_id: &str,
        slug: &str,
        kind: &str,
        origin: Option<&str>,
        bytes: &[u8],
    ) -> Result<Source> {
        fs_layout::ensure_user_dirs(&self.state.config.data_dir, user_id)?;
        let source_id = Uuid::now_v7().simple().to_string();
        let content_hash = sha256_hex(bytes);
        let rel_path = format!("sources/{source_id}-{slug}.md");
        let abs_path = self.abs_source_path(user_id, &rel_path);

        // Wrap raw content as markdown with a leading provenance header so
        // it's useful even when viewed directly.
        let md = render_source_md(origin, bytes);
        write_atomic(&abs_path, &md).await?;

        let row = source_repo::insert(
            &self.state.db,
            user_id,
            &source_id,
            slug,
            kind,
            origin,
            &rel_path,
            &content_hash,
        )
        .await?;

        crate::storage::event_repo::append(
            &self.state.db,
            user_id,
            "source.create",
            Some(&source_id),
            Some(&serde_json::json!({ "slug": slug, "type": kind, "bytes": bytes.len() })),
        )
        .await?;

        Ok(source_from_row(row))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn render_source_md(origin: Option<&str>, bytes: &[u8]) -> String {
    let body = String::from_utf8_lossy(bytes);
    let header = match origin {
        Some(url) => format!("<!-- source: {url} -->\n\n"),
        None => String::new(),
    };
    format!("{header}{body}")
}

fn source_from_row(r: source_repo::SourceRow) -> Source {
    Source {
        id: r.id,
        source_id: r.source_id,
        slug: r.slug,
        r#type: r.r#type,
        origin: r.origin,
        path: r.path,
        content_hash: r.content_hash,
        created_at: r.created_at,
    }
}

async fn write_atomic(path: &std::path::Path, content: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = path.with_extension("md.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    async fn svc() -> (SourceService, AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = Config::for_test(dir.path().to_path_buf());
        let state = crate::storage::init_pool(cfg).await.expect("init pool");
        let svc = SourceService::new(state.clone()).expect("svc");
        (svc, state, dir)
    }

    async fn ensure_user(state: &AppState) -> String {
        let password_hash = crate::auth::hash_password("pw").unwrap();
        let row = crate::storage::user_repo::insert(&state.db, "u", &password_hash)
            .await
            .expect("user");
        row.id
    }

    #[tokio::test]
    async fn upload_and_read_back() {
        let (svc, state, _d) = svc().await;
        let user = ensure_user(&state).await;
        let s = svc
            .upload(&user, "notes", b"hello world")
            .await
            .expect("upload");
        assert_eq!(s.r#type, "upload");
        assert_eq!(s.slug, "notes");
        let raw = svc.get_raw(&user, &s.source_id).await.expect("raw");
        let raw_str = String::from_utf8_lossy(&raw);
        assert!(raw_str.contains("hello world"));
    }

    #[tokio::test]
    async fn upload_rejects_huge_payload() {
        let (svc, state, _d) = svc().await;
        let user = ensure_user(&state).await;
        // Build a vec just over the limit (10MB+1).
        let mut big = vec![0u8; 10 * 1024 * 1024 + 1];
        big[0] = b'a';
        assert!(svc.upload(&user, "big", &big).await.is_err());
    }

    #[tokio::test]
    async fn register_url_rejects_bad_scheme() {
        let (svc, state, _d) = svc().await;
        let user = ensure_user(&state).await;
        let err = svc
            .register_url(&user, "ftp://example.com", "x")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
