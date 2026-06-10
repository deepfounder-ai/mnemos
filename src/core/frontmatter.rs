//! Frontmatter parser / serialiser + a typed view of the page metadata.
//!
//! The on-disk and on-API representation is the same JSON object. YAML is
//! accepted for human-friendly authoring (CLI, file upload) and converted
//! to JSON at the storage boundary.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

use crate::error::{AppError, Result};

/// Single frontmatter object. Mirrors the schema documented in the
/// project scratchpad.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frontmatter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceRef>,
    #[serde(default = "default_scope")]
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_type: Option<PageType>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

fn default_scope() -> Scope {
    Scope::Global
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Global,
    Local,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PageType {
    Concept,
    Recipe,
    Reference,
    Decision,
}

impl PageType {
    pub fn as_str(self) -> &'static str {
        match self {
            PageType::Concept => "concept",
            PageType::Recipe => "recipe",
            PageType::Reference => "reference",
            PageType::Decision => "decision",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRef {
    #[serde(rename = "type")]
    pub kind: SourceKind,
    /// Path within the user dir, e.g. `sources/abc1234-slug.md`. Maps to the
    /// `ref` key in YAML/JSON.
    #[serde(rename = "ref")]
    pub ref_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl SourceRef {
    pub fn url(ref_: impl Into<String>, origin: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Url,
            ref_: ref_.into(),
            origin: Some(origin.into()),
        }
    }
    pub fn upload(ref_: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Upload,
            ref_: ref_.into(),
            origin: None,
        }
    }
    pub fn session(ref_: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Session,
            ref_: ref_.into(),
            origin: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Url,
    Upload,
    Session,
}

/// Parse a frontmatter block. Accepts either YAML or JSON.
pub fn parse(input: &str) -> Result<Frontmatter> {
    // Try YAML first; serde_yaml also handles JSON since JSON is a subset of YAML.
    let value: YamlValue = serde_yaml::from_str(input)
        .map_err(|e| AppError::Validation(format!("frontmatter parse: {e}")))?;
    let json_value: JsonValue = serde_yaml::from_value(value)
        .map_err(|e| AppError::Validation(format!("frontmatter convert: {e}")))?;
    let fm: Frontmatter = serde_json::from_value(json_value)
        .map_err(|e| AppError::Validation(format!("frontmatter shape: {e}")))?;
    Ok(fm)
}

/// Serialise frontmatter to canonical JSON (used in the DB).
pub fn to_json(fm: &Frontmatter) -> Result<String> {
    Ok(serde_json::to_string(fm)?)
}

/// Serialise frontmatter to YAML (for human-friendly file output).
pub fn to_yaml(fm: &Frontmatter) -> Result<String> {
    Ok(serde_yaml::to_string(fm)?)
}

/// Parse a raw page body that includes a leading `---` YAML frontmatter
/// block. Returns the frontmatter and the remaining body.
///
/// The body is taken as-is after the closing `---`. We do **not** try to be
/// clever about indented / nested fences; we look for the first blank-line-
/// free `---` on its own line, then the next `---` on its own line.
pub fn split_document(input: &str) -> Result<(Frontmatter, String)> {
    let trimmed = input.trim_start_matches('\u{feff}'); // strip BOM
    let mut lines = trimmed.split_inclusive('\n');
    let first = lines
        .next()
        .ok_or_else(|| AppError::Validation("empty page body".into()))?;
    if first.trim() != "---" {
        return Err(AppError::Validation(
            "page body must start with `---` frontmatter fence".into(),
        ));
    }
    let mut yaml_lines: Vec<&str> = Vec::new();
    let mut closed = false;
    let mut consumed = 1usize;
    for (idx, line) in lines.enumerate() {
        consumed = idx + 2; // header line counts
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        yaml_lines.push(line);
    }
    if !closed {
        return Err(AppError::Validation(
            "unterminated frontmatter block (missing closing `---`)".into(),
        ));
    }
    let yaml = yaml_lines.join("");
    let fm = parse(&yaml)?;
    let body = trimmed
        .split('\n')
        .skip(consumed) // skip header + yaml + closing fence
        .collect::<Vec<&str>>()
        .join("\n");
    let body = body.trim_start_matches('\n').to_string();
    Ok((fm, body))
}

/// Render a full page (frontmatter + body) as markdown.
pub fn render(fm: &Frontmatter, body: &str) -> Result<String> {
    let yaml = to_yaml(fm)?;
    Ok(format!("---\n{yaml}---\n\n{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal() {
        let yaml = "title: Hello\ntags: [a, b]\n";
        let fm = parse(yaml).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Hello"));
        assert_eq!(fm.tags, vec!["a", "b"]);
        assert_eq!(fm.scope, Scope::Global);
    }

    #[test]
    fn parse_full() {
        let yaml = r#"
title: Kafka
tags: [kafka, queue]
created: 2026-04-17
updated: 2026-04-17
sources:
  - type: url
    ref: sources/abc-kafka.md
    origin: https://kafka.apache.org
  - type: session
    ref: 2026-04-17-kafka
scope: global
page_type: concept
related: [event-streaming]
"#;
        let fm = parse(yaml).unwrap();
        assert_eq!(fm.sources.len(), 2);
        assert_eq!(fm.sources[0].kind, SourceKind::Url);
        assert_eq!(fm.sources[0].ref_, "sources/abc-kafka.md");
        assert_eq!(
            fm.sources[0].origin.as_deref(),
            Some("https://kafka.apache.org")
        );
        assert_eq!(fm.related, vec!["event-streaming"]);
        assert_eq!(fm.page_type, Some(PageType::Concept));
    }

    #[test]
    fn split_document_basic() {
        let doc = "---\ntitle: x\n---\n\nbody line 1\nbody line 2\n";
        let (fm, body) = split_document(doc).unwrap();
        assert_eq!(fm.title.as_deref(), Some("x"));
        assert!(body.contains("body line 1"));
    }

    #[test]
    fn split_document_missing_fence() {
        let bad = "title: x\n---\nbody\n";
        assert!(split_document(bad).is_err());
    }

    #[test]
    fn split_document_unterminated() {
        let bad = "---\ntitle: x\nstill yaml\n";
        assert!(split_document(bad).is_err());
    }

    #[test]
    fn round_trip() {
        let fm = Frontmatter {
            title: Some("Round Trip".into()),
            tags: vec!["a".into(), "b".into()],
            created: Some(NaiveDate::from_ymd_opt(2026, 4, 17).unwrap()),
            updated: None,
            sources: vec![SourceRef::url("sources/abc.md", "https://example.com")],
            scope: Scope::Local,
            page_type: Some(PageType::Recipe),
            related: vec!["other".into()],
            project: None,
        };
        let yaml = to_yaml(&fm).unwrap();
        let fm2 = parse(&yaml).unwrap();
        assert_eq!(fm, fm2);
    }

    #[test]
    fn render_document() {
        let fm = Frontmatter {
            title: Some("Doc".into()),
            ..Default::default()
        };
        let out = render(&fm, "Hello body").unwrap();
        assert!(out.starts_with("---\n"));
        assert!(out.contains("title: Doc"));
        assert!(out.contains("Hello body"));
    }
}
