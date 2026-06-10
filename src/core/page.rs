//! Page service — CRUD, list, and search-via-filter entry points.

use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::core::frontmatter::{self, Frontmatter};
use crate::core::slug;
use crate::error::{AppError, Result};
use crate::storage::{fs_layout, page_repo, AppState};

/// Page service. All methods are scoped by `user_id` for tenant isolation.
#[derive(Clone, Debug)]
pub struct PageService {
    state: AppState,
}

/// Combined view of a page: parsed frontmatter + body + DB metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub frontmatter: Frontmatter,
    pub body: String,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct PageFilter {
    /// Substring filter on title or slug.
    pub query: Option<String>,
    /// Tag filter — all tags must match.
    pub tag: Option<String>,
    /// Restrict to a particular page type.
    pub page_type: Option<String>,
    /// Restrict to a particular project.
    pub project: Option<String>,
    /// Maximum number of rows to return.
    pub limit: Option<i64>,
}

impl PageService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Create a new page. Returns the freshly-inserted page.
    pub async fn create(
        &self,
        user_id: &str,
        slug_input: &str,
        frontmatter: Frontmatter,
        body: &str,
    ) -> Result<Page> {
        let slug = normalize_slug(slug_input, &frontmatter)?;
        validate_unique_slug(user_id, &slug, &self.state).await?;

        let title = frontmatter
            .title
            .clone()
            .unwrap_or_else(|| title_from_body(body));

        let fm_json = frontmatter::to_json(&frontmatter)?;
        let row = page_repo::insert(&self.state.db, user_id, &slug, &title, &fm_json, body).await?;

        write_page_file(&self.state, user_id, &row).await?;
        rebuild_index(user_id, &self.state).await?;

        crate::storage::event_repo::append(
            &self.state.db,
            user_id,
            "page.create",
            Some(&slug),
            Some(&serde_json::json!({ "title": &title })),
        )
        .await?;

        page_from_row(row, frontmatter)
    }

    /// Fetch a page by slug.
    pub async fn get(&self, user_id: &str, slug_input: &str) -> Result<Page> {
        let slug = slug::slugify(slug_input);
        let row = page_repo::get_by_slug(&self.state.db, user_id, &slug)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("page '{slug}'")))?;
        let fm = parse_fm(&row.frontmatter_json)?;
        page_from_row(row, fm)
    }

    /// Update an existing page. The created_at is preserved; updated_at is
    /// refreshed. The slug stays the same — clients that want a different
    /// slug must delete and re-create.
    pub async fn update(
        &self,
        user_id: &str,
        slug_input: &str,
        mut frontmatter: Frontmatter,
        body: &str,
    ) -> Result<Page> {
        let slug = slug::slugify(slug_input);
        let existing = page_repo::get_by_slug(&self.state.db, user_id, &slug)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("page '{slug}'")))?;

        // Always bump updated_at; let callers opt out by passing updated: None.
        frontmatter.updated = Some(
            frontmatter
                .updated
                .unwrap_or_else(|| Utc::now().date_naive()),
        );
        let title = frontmatter
            .title
            .clone()
            .unwrap_or_else(|| title_from_body(body));

        let fm_json = frontmatter::to_json(&frontmatter)?;
        let row =
            page_repo::update_content(&self.state.db, &existing.id, &title, &fm_json, body).await?;

        write_page_file(&self.state, user_id, &row).await?;
        rebuild_index(user_id, &self.state).await?;

        crate::storage::event_repo::append(
            &self.state.db,
            user_id,
            "page.update",
            Some(&slug),
            None,
        )
        .await?;

        page_from_row(row, frontmatter)
    }

    /// Delete a page by slug.
    pub async fn delete(&self, user_id: &str, slug_input: &str) -> Result<()> {
        let slug = slug::slugify(slug_input);
        let row = page_repo::get_by_slug(&self.state.db, user_id, &slug)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("page '{slug}'")))?;

        page_repo::delete(&self.state.db, &row.id).await?;
        delete_page_file(&self.state, user_id, &slug).await?;
        rebuild_index(user_id, &self.state).await?;

        crate::storage::event_repo::append(
            &self.state.db,
            user_id,
            "page.delete",
            Some(&slug),
            None,
        )
        .await?;
        Ok(())
    }

    /// List pages for a user with optional filtering.
    pub async fn list(&self, user_id: &str, filter: &PageFilter) -> Result<Vec<Page>> {
        let mut rows = page_repo::list_for_user(&self.state.db, user_id).await?;
        rows.retain(|r| {
            if let Some(q) = &filter.query {
                let q = q.to_lowercase();
                if !r.title.to_lowercase().contains(&q) && !r.slug.to_lowercase().contains(&q) {
                    return false;
                }
            }
            if let Some(tag) = &filter.tag {
                let fm = parse_fm(&r.frontmatter_json).ok();
                let has_tag = fm.map(|f| f.tags.iter().any(|t| t == tag)).unwrap_or(false);
                if !has_tag {
                    return false;
                }
            }
            if let Some(pt) = &filter.page_type {
                let fm = parse_fm(&r.frontmatter_json).ok();
                let matches = fm
                    .and_then(|f| f.page_type)
                    .map(|p| p.as_str() == pt)
                    .unwrap_or(false);
                if !matches {
                    return false;
                }
            }
            if let Some(proj) = &filter.project {
                let fm = parse_fm(&r.frontmatter_json).ok();
                let matches = fm
                    .and_then(|f| f.project)
                    .map(|p| p == *proj)
                    .unwrap_or(false);
                if !matches {
                    return false;
                }
            }
            true
        });
        if let Some(lim) = filter.limit {
            rows.truncate(lim as usize);
        }
        rows.into_iter()
            .map(|r| {
                let fm = parse_fm(&r.frontmatter_json)?;
                page_from_row(r, fm)
            })
            .collect()
    }

    /// Return all slugs for a user. Used by lint and search.
    pub async fn slugs(&self, user_id: &str) -> Result<Vec<String>> {
        page_repo::slugs_for_user(&self.state.db, user_id).await
    }
}

fn normalize_slug(slug_input: &str, fm: &Frontmatter) -> Result<String> {
    if slug_input.is_empty() {
        if let Some(title) = &fm.title {
            return Ok(slug::slugify(title));
        }
        return Err(AppError::Validation("slug (or title) is required".into()));
    }
    let s = slug::slugify(slug_input);
    if s.is_empty() {
        return Err(AppError::Validation(format!(
            "input '{slug_input}' does not slugify to a valid identifier"
        )));
    }
    slug::validate(&s)?;
    Ok(s)
}

fn title_from_body(body: &str) -> String {
    body.lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| "untitled".to_string())
}

fn parse_fm(json: &str) -> Result<Frontmatter> {
    serde_json::from_str(json).map_err(AppError::from)
}

fn page_from_row(row: page_repo::PageRow, fm: Frontmatter) -> Result<Page> {
    Ok(Page {
        id: row.id,
        slug: row.slug,
        title: row.title,
        frontmatter: fm,
        body: row.body,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn validate_unique_slug(user_id: &str, slug: &str, state: &AppState) -> Result<()> {
    if page_repo::get_by_slug(&state.db, user_id, slug)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(format!(
            "page with slug '{slug}' already exists"
        )));
    }
    Ok(())
}

async fn write_page_file(state: &AppState, user_id: &str, row: &page_repo::PageRow) -> Result<()> {
    fs_layout::ensure_user_dirs(&state.config.data_dir, user_id)?;
    let fm: Frontmatter = parse_fm(&row.frontmatter_json)?;
    let body_md = frontmatter::render(&fm, &row.body)?;
    let path =
        fs_layout::pages_dir(&state.config.data_dir, user_id).join(format!("{}.md", row.slug));
    write_atomic(&path, &body_md).await
}

async fn delete_page_file(state: &AppState, user_id: &str, slug: &str) -> Result<()> {
    let path = fs_layout::pages_dir(&state.config.data_dir, user_id).join(format!("{slug}.md"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::Io(e)),
    }
}

async fn write_atomic(path: &Path, content: &str) -> Result<()> {
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

async fn rebuild_index(user_id: &str, state: &AppState) -> Result<()> {
    crate::core::index::IndexBuilder::new(state.clone())
        .rebuild_for_user(user_id)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    async fn svc() -> (PageService, AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = Config::for_test(dir.path().to_path_buf());
        let state = crate::storage::init_pool(cfg).await.expect("init pool");
        let svc = PageService::new(state.clone());
        (svc, state, dir)
    }

    async fn ensure_user(state: &AppState, username: &str) -> String {
        let password_hash = crate::auth::hash_password("pw").unwrap();
        let row = crate::storage::user_repo::insert(&state.db, username, &password_hash)
            .await
            .expect("user");
        row.id
    }

    #[tokio::test]
    async fn create_get_update_delete() {
        let (svc, state, _d) = svc().await;
        let user = ensure_user(&state, "alice").await;
        let fm = Frontmatter {
            title: Some("First".into()),
            ..Default::default()
        };
        let p = svc
            .create(&user, "first", fm, "body one")
            .await
            .expect("create");
        assert_eq!(p.slug, "first");
        assert_eq!(p.title, "First");

        let fetched = svc.get(&user, "first").await.expect("get");
        assert_eq!(fetched.body, "body one");

        let fm2 = Frontmatter {
            title: Some("First v2".into()),
            ..Default::default()
        };
        let updated = svc
            .update(&user, "first", fm2, "body two")
            .await
            .expect("update");
        assert_eq!(updated.title, "First v2");
        assert_eq!(updated.body, "body two");

        svc.delete(&user, "first").await.expect("delete");
        assert!(svc.get(&user, "first").await.is_err());
    }

    #[tokio::test]
    async fn slug_collisions_rejected() {
        let (svc, state, _d) = svc().await;
        let user = ensure_user(&state, "bob").await;
        let fm = Frontmatter {
            title: Some("X".into()),
            ..Default::default()
        };
        svc.create(&user, "x", fm.clone(), "1")
            .await
            .expect("first");
        assert!(svc.create(&user, "x", fm, "2").await.is_err());
    }

    #[tokio::test]
    async fn slug_auto_from_title() {
        let (svc, state, _d) = svc().await;
        let user = ensure_user(&state, "c").await;
        let fm = Frontmatter {
            title: Some("Some Title".into()),
            ..Default::default()
        };
        let p = svc.create(&user, "", fm, "body").await.expect("auto-slug");
        assert_eq!(p.slug, "some-title");
    }

    #[tokio::test]
    async fn list_filter_by_tag_and_type() {
        let (svc, state, _d) = svc().await;
        let user = ensure_user(&state, "d").await;
        let mut a = Frontmatter {
            title: Some("A".into()),
            tags: vec!["k".into()],
            page_type: Some(frontmatter::PageType::Concept),
            ..Default::default()
        };
        let mut b = Frontmatter {
            title: Some("B".into()),
            tags: vec!["k".into()],
            page_type: Some(frontmatter::PageType::Recipe),
            ..Default::default()
        };
        let c = Frontmatter {
            title: Some("C".into()),
            tags: vec!["x".into()],
            page_type: Some(frontmatter::PageType::Concept),
            ..Default::default()
        };
        svc.create(&user, "a", a.clone(), "1").await.unwrap();
        svc.create(&user, "b", b.clone(), "2").await.unwrap();
        svc.create(&user, "c", c.clone(), "3").await.unwrap();

        let f1 = PageFilter {
            tag: Some("k".into()),
            ..Default::default()
        };
        let r1 = svc.list(&user, &f1).await.unwrap();
        assert_eq!(r1.len(), 2);

        a.page_type = Some(frontmatter::PageType::Concept);
        b.page_type = Some(frontmatter::PageType::Recipe);
        let _ = a;
        let _ = b;
        let f2 = PageFilter {
            page_type: Some("recipe".into()),
            ..Default::default()
        };
        let r2 = svc.list(&user, &f2).await.unwrap();
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].slug, "b");
    }

    #[tokio::test]
    async fn user_isolation() {
        let (svc, state, _d) = svc().await;
        let u1 = ensure_user(&state, "iso1").await;
        let u2 = ensure_user(&state, "iso2").await;
        let fm = Frontmatter {
            title: Some("S".into()),
            ..Default::default()
        };
        svc.create(&u1, "shared", fm, "1").await.unwrap();
        // u2 cannot see u1's page.
        assert!(svc.get(&u2, "shared").await.is_err());
    }
}
