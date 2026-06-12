//! Read-only MCP resources.

use serde::{Deserialize, Serialize};

use crate::mcp::{McpError, McpRestClient};

/// A `resources/list` entry.
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

/// Static catalog.
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
            name: "source".into(),
            description: "Raw source body by source id.".into(),
            mime_type: "text/markdown".into(),
        },
    ]
}

/// Resolve a `mnemos://` URI to its body.
pub async fn read(uri: &str, client: &McpRestClient) -> Result<Vec<ResourceContents>, McpError> {
    let parsed = parse_uri(uri)?;
    match parsed {
        Parsed::Index => Ok(vec![read_index(client).await?]),
        Parsed::Log => Ok(vec![read_log(client).await?]),
        Parsed::Page { slug } => Ok(vec![read_page(&slug, client).await?]),
        Parsed::Source { id } => Ok(vec![read_source(&id, client).await?]),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Index,
    Log,
    Page { slug: String },
    Source { id: String },
}

fn parse_uri(uri: &str) -> Result<Parsed, McpError> {
    let rest: &str = uri
        .strip_prefix("mnemos://")
        .ok_or_else(|| McpError::InvalidArgument(format!("not an mnemos:// URI: {uri}")))?;
    let (kind, tail): (&str, String) = match rest.split_once('/') {
        Some((a, b)) => (a, b.to_string()),
        None => (rest, String::new()),
    };
    match kind {
        "index" => Ok(Parsed::Index),
        "log" => Ok(Parsed::Log),
        "page" => {
            if tail.is_empty() {
                return Err(McpError::InvalidArgument(
                    "mnemos://page/{slug} requires a slug".into(),
                ));
            }
            Ok(Parsed::Page { slug: tail })
        }
        "source" => {
            if tail.is_empty() {
                return Err(McpError::InvalidArgument(
                    "mnemos://source/{id} requires an id".into(),
                ));
            }
            Ok(Parsed::Source { id: tail })
        }
        other => Err(McpError::InvalidArgument(format!(
            "unknown mnemos:// resource kind '{other}'"
        ))),
    }
}

async fn read_index(client: &McpRestClient) -> Result<ResourceContents, McpError> {
    let text = client.get_text("/api/v1/index").await?;
    Ok(ResourceContents {
        uri: "mnemos://index".into(),
        mime_type: "text/markdown".into(),
        text,
    })
}

async fn read_log(client: &McpRestClient) -> Result<ResourceContents, McpError> {
    let text = client.get_text("/api/v1/log?format=md").await?;
    Ok(ResourceContents {
        uri: "mnemos://log".into(),
        mime_type: "text/markdown".into(),
        text,
    })
}

async fn read_page(slug: &str, client: &McpRestClient) -> Result<ResourceContents, McpError> {
    let path = format!("/api/v1/pages/{slug}/raw");
    let text = client.get_text(&path).await?;
    Ok(ResourceContents {
        uri: format!("mnemos://page/{slug}"),
        mime_type: "text/markdown".into(),
        text,
    })
}

async fn read_source(id: &str, client: &McpRestClient) -> Result<ResourceContents, McpError> {
    let path = format!("/api/v1/sources/{id}/raw");
    let text = client.get_text(&path).await?;
    Ok(ResourceContents {
        uri: format!("mnemos://source/{id}"),
        mime_type: "text/markdown".into(),
        text,
    })
}
