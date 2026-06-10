//! Read-only MCP resources.
//!
//! URI scheme: `mnemos://...`. The four canonical resources mirror the
//! scratchpad contract:
//!
//! - `mnemos://index`           → `index.md` (auto-generated catalog)
//! - `mnemos://log`             → `log.md` (append-only event log)
//! - `mnemos://page/{slug}`     → rendered page markdown
//! - `mnemos://source/{id}`     → raw source bytes (as text)

use serde::{Deserialize, Serialize};

use crate::core::index::IndexBuilder;
use crate::core::log::LogBuilder;
use crate::core::page::PageService;
use crate::core::source::SourceService;
use crate::error::{AppError, Result};
use crate::mcp::AuthedContext;
use crate::storage::fs_layout;

/// A `resources/list` entry. We only model `text` resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// A `resources/read` body item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContents {
    pub uri: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadParams {
    pub uri: String,
}

/// Static catalog. Pagination isn't used (the wiki lives behind a per-user
/// filter that the resources themselves apply).
pub fn list_resources() -> Vec<Resource> {
    vec![
        Resource {
            uri: "mnemos://index".into(),
            name: "index".into(),
            description: "Auto-generated catalog of all wiki pages (index.md).".into(),
            mime_type: "text/markdown".into(),
        },
        Resource {
            uri: "mnemos://log".into(),
            name: "log".into(),
            description: "Append-only event log (log.md).".into(),
            mime_type: "text/markdown".into(),
        },
        Resource {
            uri: "mnemos://page/{slug}".into(),
            name: "page".into(),
            description: "Rendered wiki page by slug. Returns 404 if not found.".into(),
            mime_type: "text/markdown".into(),
        },
        Resource {
            uri: "mnemos://source/{id}".into(),
            description: "Raw source body by source id.".into(),
            name: "source".into(),
            mime_type: "text/markdown".into(),
        },
    ]
}

/// Resolve a `mnemos://` URI to its body. The returned `ResourceContents`
/// is what `resources/read` puts on the wire.
pub async fn read(uri: &str, user: &AuthedContext) -> Result<Vec<ResourceContents>> {
    let parsed = parse_uri(uri)?;
    match parsed {
        Parsed::Index => Ok(vec![read_index(user).await?]),
        Parsed::Log => Ok(vec![read_log(user).await?]),
        Parsed::Page { slug } => Ok(vec![read_page(&slug, user).await?]),
        Parsed::Source { id } => Ok(vec![read_source(&id, user).await?]),
    }
}

#[derive(Debug)]
enum Parsed {
    Index,
    Log,
    Page { slug: String },
    Source { id: String },
}

fn parse_uri(uri: &str) -> Result<Parsed> {
    let rest = uri
        .strip_prefix("mnemos://")
        .ok_or_else(|| AppError::Validation(format!("not an mnemos:// URI: {uri}")))?;
    let (kind, tail) = rest
        .split_once('/')
        .map(|(a, b)| (a, b.to_string()))
        .unwrap_or((rest, String::new()));
    match kind {
        "index" => Ok(Parsed::Index),
        "log" => Ok(Parsed::Log),
        "page" => {
            if tail.is_empty() {
                return Err(AppError::Validation(
                    "mnemos://page/{slug} requires a slug".into(),
                ));
            }
            Ok(Parsed::Page { slug: tail })
        }
        "source" => {
            if tail.is_empty() {
                return Err(AppError::Validation(
                    "mnemos://source/{id} requires an id".into(),
                ));
            }
            Ok(Parsed::Source { id: tail })
        }
        other => Err(AppError::Validation(format!(
            "unknown mnemos:// resource kind '{other}'"
        ))),
    }
}

async fn read_index(user: &AuthedContext) -> Result<ResourceContents> {
    let state = user.ctx.state.clone();
    let builder = IndexBuilder::new(state.clone());
    builder.rebuild_for_user(&user.user_id).await?;
    let path = fs_layout::index_path(&state.config.data_dir, &user.user_id);
    let text = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    Ok(ResourceContents {
        uri: "mnemos://index".into(),
        mime_type: "text/markdown".into(),
        text,
    })
}

async fn read_log(user: &AuthedContext) -> Result<ResourceContents> {
    let state = user.ctx.state.clone();
    let builder = LogBuilder::new(state.clone());
    builder
        .rebuild(&user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("rebuild log: {e}")))?;
    let path = fs_layout::log_path(&state.config.data_dir, &user.user_id);
    let text = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    Ok(ResourceContents {
        uri: "mnemos://log".into(),
        mime_type: "text/markdown".into(),
        text,
    })
}

async fn read_page(slug: &str, user: &AuthedContext) -> Result<ResourceContents> {
    let svc = PageService::new(user.ctx.state.clone());
    let page = svc.get(&user.user_id, slug).await?;
    let body = crate::core::frontmatter::render(&page.frontmatter, &page.body)?;
    Ok(ResourceContents {
        uri: format!("mnemos://page/{slug}"),
        mime_type: "text/markdown".into(),
        text: body,
    })
}

async fn read_source(id: &str, user: &AuthedContext) -> Result<ResourceContents> {
    let svc = SourceService::new(user.ctx.state.clone())?;
    let raw = svc.get_raw(&user.user_id, id).await?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    Ok(ResourceContents {
        uri: format!("mnemos://source/{id}"),
        mime_type: "text/markdown".into(),
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_uris() {
        assert!(matches!(
            parse_uri("mnemos://index").unwrap(),
            Parsed::Index
        ));
        assert!(matches!(parse_uri("mnemos://log").unwrap(), Parsed::Log));
        match parse_uri("mnemos://page/kafka").unwrap() {
            Parsed::Page { slug } => assert_eq!(slug, "kafka"),
            _ => panic!("expected page"),
        }
        match parse_uri("mnemos://source/abc123").unwrap() {
            Parsed::Source { id } => assert_eq!(id, "abc123"),
            _ => panic!("expected source"),
        }
    }

    #[test]
    fn parse_rejects_bad_scheme() {
        let err = parse_uri("http://example.com").unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn parse_rejects_missing_args() {
        assert!(parse_uri("mnemos://page/").is_err());
        assert!(parse_uri("mnemos://source/").is_err());
    }

    #[test]
    fn parse_rejects_unknown_kind() {
        let err = parse_uri("mnemos://weird/thing").unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn read_end_to_end() {
        use crate::config::Config;
        use crate::mcp::McpContext;
        use crate::storage::init_pool;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::for_test(dir.path().to_path_buf());
        let state = init_pool(cfg).await.unwrap();
        let ph = crate::auth::hash_password("pw").unwrap();
        let u = crate::storage::user_repo::insert(&state.db, "u", &ph)
            .await
            .unwrap();
        let user = AuthedContext {
            ctx: Arc::new(McpContext::new(state)),
            user_id: u.id,
            api_key_id: "k".into(),
        };

        // Create a page, then read it back via the resource URI.
        crate::mcp::tools::call_tool("create_page", json!({ "title": "T", "body": "B" }), &user)
            .await
            .unwrap();
        let contents = read("mnemos://page/t", &user).await.unwrap();
        assert_eq!(contents.len(), 1);
        assert!(contents[0].text.contains("title: T"));
        assert!(contents[0].text.contains("B"));

        // Index resource should be present.
        let index = read("mnemos://index", &user).await.unwrap();
        assert!(index[0].text.contains("T"));
    }
}
