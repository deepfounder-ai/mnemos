//! `sources list | add-url | upload | get`.

use serde::Deserialize;
use serde_json::json;

use crate::cli::client::Client;
use crate::cli::output::{print_json, print_line, CliResult};
use crate::cli::SourcesCmd;

#[derive(Debug, Deserialize)]
struct SourceView {
    #[serde(default)]
    source_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
    slug: String,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    origin: Option<String>,
    #[serde(default)]
    content_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SourceList {
    sources: Vec<SourceView>,
}

pub async fn run(client: Client, json: bool, cmd: SourcesCmd) -> CliResult<()> {
    match cmd {
        SourcesCmd::List => {
            let list: SourceList = client.get("/api/v1/sources").await?;
            if json {
                return print_json(
                    &json!({ "sources": list.sources.iter().map(view_json).collect::<Vec<_>>() }),
                );
            }
            print_line(format!(
                "{:<40} {:<20} {:<8} {}",
                "SOURCE_ID", "SLUG", "TYPE", "ORIGIN"
            ));
            for s in &list.sources {
                print_line(format!(
                    "{:<40} {:<20} {:<8} {}",
                    s.source_id.as_deref().or(s.id.as_deref()).unwrap_or("-"),
                    s.slug,
                    s.kind.as_deref().unwrap_or("-"),
                    s.origin.as_deref().unwrap_or("-"),
                ));
            }
            Ok(())
        }
        SourcesCmd::AddUrl { url, slug } => {
            let mut payload = json!({ "url": url });
            if let Some(s) = slug {
                payload["slug"] = json!(s);
            }
            let v: serde_json::Value = client.post("/api/v1/sources/url", &payload).await?;
            emit_source(json, &v)
        }
        SourcesCmd::Upload { file, slug } => {
            let bytes = std::fs::read(&file)?;
            let filename = std::path::Path::new(&file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("upload")
                .to_string();
            let slug = slug.unwrap_or_else(|| derive_slug(&filename));
            let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
            let form = reqwest::multipart::Form::new()
                .text("slug", slug)
                .part("file", part);
            let v: serde_json::Value = client
                .post_multipart("/api/v1/sources/upload", form)
                .await?;
            emit_source(json, &v)
        }
        SourcesCmd::Get { id } => {
            let body = client
                .get_text(&format!("/api/v1/sources/{id}/raw"))
                .await?;
            print!("{body}");
            Ok(())
        }
    }
}

fn emit_source(json: bool, v: &serde_json::Value) -> CliResult<()> {
    if json {
        return print_json(v);
    }
    let sid = v
        .get("source_id")
        .or_else(|| v.get("id"))
        .and_then(|x| x.as_str())
        .unwrap_or("?");
    let slug = v.get("slug").and_then(|x| x.as_str()).unwrap_or("?");
    print_line(format!("{sid}  ({slug})"));
    Ok(())
}

fn view_json(s: &SourceView) -> serde_json::Value {
    json!({
        "source_id": s.source_id,
        "id": s.id,
        "slug": s.slug,
        "type": s.kind,
        "origin": s.origin,
        "content_hash": s.content_hash,
    })
}

fn derive_slug(filename: &str) -> String {
    let stem = filename
        .rsplit_once('.')
        .map(|(a, _)| a)
        .unwrap_or(filename);
    stem.to_string()
}
