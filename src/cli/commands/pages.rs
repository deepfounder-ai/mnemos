//! `pages list | get | create | update | delete`.

use std::io::Read;

use serde::Deserialize;
use serde_json::json;

use crate::cli::client::Client;
use crate::cli::output::{print_json, print_line, CliError, CliResult};
use crate::cli::PagesCmd;
use crate::core::frontmatter::{self, Frontmatter};

#[derive(Debug, Deserialize)]
struct PageSummary {
    slug: String,
    title: String,
    #[serde(default)]
    page_type: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PageList {
    pages: Vec<PageSummary>,
    #[serde(default)]
    total: usize,
}

pub async fn run(client: Client, json: bool, cmd: PagesCmd) -> CliResult<()> {
    match cmd {
        PagesCmd::List {
            tag,
            page_type,
            query,
        } => {
            let mut params: Vec<(String, String)> = Vec::new();
            if let Some(t) = tag {
                params.push(("tag".into(), t));
            }
            if let Some(t) = page_type {
                params.push(("type".into(), t));
            }
            if let Some(q) = query {
                params.push(("q".into(), q));
            }
            let path = with_query("/api/v1/pages", &params);
            let list: PageList = client.get(&path).await?;
            if json {
                return print_json(&json!({
                    "pages": list.pages.iter().map(|p| json!({
                        "slug": p.slug, "title": p.title, "page_type": p.page_type,
                        "tags": p.tags, "updated_at": p.updated_at
                    })).collect::<Vec<_>>(),
                    "total": list.total,
                }));
            }
            print_line(format!("{:<32} {:<12} {}", "SLUG", "TYPE", "TITLE"));
            for p in &list.pages {
                print_line(format!(
                    "{:<32} {:<12} {}",
                    p.slug,
                    p.page_type.as_deref().unwrap_or("-"),
                    p.title
                ));
            }
            Ok(())
        }
        PagesCmd::Get { slug } => {
            if json {
                let v: serde_json::Value = client.get(&format!("/api/v1/pages/{slug}")).await?;
                print_json(&v)
            } else {
                let md = client
                    .get_text(&format!("/api/v1/pages/{slug}/raw"))
                    .await?;
                print!("{md}");
                Ok(())
            }
        }
        PagesCmd::Create {
            slug,
            title,
            from_file,
            from_stdin,
        } => {
            let (fm, body) = read_document(from_file, from_stdin)?;
            let mut payload = json!({ "slug": slug, "body": body, "frontmatter": fm });
            if let Some(t) = title {
                payload["title"] = json!(t);
            }
            let v: serde_json::Value = client.post("/api/v1/pages", &payload).await?;
            emit_ref(json, &v)
        }
        PagesCmd::Update {
            slug,
            from_file,
            from_stdin,
        } => {
            let (fm, body) = read_document(from_file, from_stdin)?;
            let payload = json!({ "body": body, "frontmatter": fm });
            let v: serde_json::Value = client
                .put(&format!("/api/v1/pages/{slug}"), &payload)
                .await?;
            emit_ref(json, &v)
        }
        PagesCmd::Delete { slug } => {
            let _: serde_json::Value = client.delete(&format!("/api/v1/pages/{slug}")).await?;
            if json {
                return print_json(&json!({ "deleted": slug }));
            }
            print_line(format!("deleted {slug}"));
            Ok(())
        }
    }
}

fn emit_ref(json: bool, v: &serde_json::Value) -> CliResult<()> {
    if json {
        return print_json(v);
    }
    let slug = v.get("slug").and_then(|x| x.as_str()).unwrap_or("?");
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("?");
    print_line(format!("{slug}  (id {id})"));
    Ok(())
}

/// Read a page document (markdown with leading `---` frontmatter) from a
/// file or stdin, and split it into frontmatter + body.
fn read_document(from_file: Option<String>, from_stdin: bool) -> CliResult<(Frontmatter, String)> {
    let raw = match (from_file, from_stdin) {
        (Some(path), _) => std::fs::read_to_string(&path)?,
        (None, true) => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
        (None, false) => {
            return Err(CliError::Usage(
                "provide --from-file <path> or --from-stdin".into(),
            ))
        }
    };
    frontmatter::split_document(&raw).map_err(|e| CliError::Other(e.to_string()))
}

/// Append a query string to a path. Values are percent-encoded minimally
/// (space → %20) — enough for slugs, tags, and short queries.
fn with_query(base: &str, params: &[(String, String)]) -> String {
    if params.is_empty() {
        return base.to_string();
    }
    let qs: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, encode(v)))
        .collect();
    format!("{base}?{}", qs.join("&"))
}

fn encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".to_string(),
            other => other
                .to_string()
                .bytes()
                .map(|b| format!("%{b:02X}"))
                .collect(),
        })
        .collect()
}
