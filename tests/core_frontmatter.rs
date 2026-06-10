//! Integration tests for the frontmatter module.

use mnemos::core::frontmatter::{
    parse, render, split_document, to_json, to_yaml, Frontmatter, PageType, Scope, SourceKind,
    SourceRef,
};
use mnemos::core::slug::slugify;

fn basic_fm() -> Frontmatter {
    Frontmatter {
        title: Some("Kafka".into()),
        tags: vec!["queue".into(), "streaming".into()],
        created: Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap()),
        updated: Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()),
        sources: vec![
            SourceRef::url("sources/abc-kafka.md", "https://kafka.apache.org"),
            SourceRef::session("2026-04-17-kafka"),
        ],
        scope: Scope::Global,
        page_type: Some(PageType::Concept),
        related: vec!["event-streaming".into()],
        project: None,
    }
}

#[test]
fn parse_minimal_yaml() {
    let yaml = "title: Hello\ntags: [a, b]\n";
    let fm = parse(yaml).unwrap();
    assert_eq!(fm.title.as_deref(), Some("Hello"));
    assert_eq!(fm.tags, vec!["a", "b"]);
    assert_eq!(fm.scope, Scope::Global);
    assert!(fm.page_type.is_none());
}

#[test]
fn parse_full_yaml() {
    let yaml = r#"
title: Kafka
tags: [kafka, queue]
created: 2026-04-17
updated: 2026-04-18
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
    assert_eq!(fm.title.as_deref(), Some("Kafka"));
    assert_eq!(fm.tags.len(), 2);
    assert_eq!(fm.sources.len(), 2);
    assert_eq!(fm.sources[0].kind, SourceKind::Url);
    assert_eq!(fm.sources[0].ref_, "sources/abc-kafka.md");
    assert_eq!(
        fm.sources[0].origin.as_deref(),
        Some("https://kafka.apache.org")
    );
    assert_eq!(fm.sources[1].kind, SourceKind::Session);
    assert_eq!(fm.page_type, Some(PageType::Concept));
    assert_eq!(fm.related, vec!["event-streaming"]);
    assert_eq!(fm.scope, Scope::Global);
}

#[test]
fn parse_invalid_yaml_returns_validation_error() {
    let bad = ":\n- this is not a mapping at the top\n";
    assert!(parse(bad).is_err());
}

#[test]
fn json_round_trip() {
    let fm = basic_fm();
    let s = to_json(&fm).unwrap();
    let fm2: Frontmatter = serde_json::from_str(&s).unwrap();
    assert_eq!(fm, fm2);
}

#[test]
fn yaml_round_trip() {
    let fm = basic_fm();
    let s = to_yaml(&fm).unwrap();
    let fm2 = parse(&s).unwrap();
    assert_eq!(fm, fm2);
}

#[test]
fn split_document_basic() {
    let doc = "---\ntitle: x\ntags: [t]\n---\n\nbody line 1\nbody line 2\n";
    let (fm, body) = split_document(doc).unwrap();
    assert_eq!(fm.title.as_deref(), Some("x"));
    assert!(body.starts_with("body line 1"));
}

#[test]
fn split_document_no_blank_line_after_fence() {
    let doc = "---\ntitle: y\n---\nbody line 1\n";
    let (fm, body) = split_document(doc).unwrap();
    assert_eq!(fm.title.as_deref(), Some("y"));
    // The leading newline is trimmed; body should start with the content.
    assert!(body.contains("body line 1"));
}

#[test]
fn split_document_missing_leading_fence() {
    let bad = "title: x\n---\nbody\n";
    assert!(split_document(bad).is_err());
}

#[test]
fn split_document_unterminated() {
    let bad = "---\ntitle: x\nstill yaml\n";
    assert!(split_document(bad).is_err());
}

#[test]
fn split_document_empty() {
    assert!(split_document("").is_err());
    assert!(split_document("   \n  ").is_err());
}

#[test]
fn render_full_document() {
    let fm = Frontmatter {
        title: Some("Hello".into()),
        tags: vec!["a".into()],
        ..Default::default()
    };
    let doc = render(&fm, "Body paragraph.").unwrap();
    assert!(doc.starts_with("---\n"));
    assert!(doc.contains("title: Hello"));
    assert!(doc.contains("Body paragraph."));
    let (fm2, body2) = split_document(&doc).unwrap();
    assert_eq!(fm, fm2);
    assert!(body2.contains("Body paragraph."));
}

#[test]
fn frontmatter_with_only_title_minimal() {
    let yaml = "title: Just a title\n";
    let fm = parse(yaml).unwrap();
    assert_eq!(fm.title.as_deref(), Some("Just a title"));
    assert!(fm.tags.is_empty());
    assert!(fm.related.is_empty());
    assert!(fm.sources.is_empty());
    assert_eq!(fm.scope, Scope::Global);
}

#[test]
fn scope_can_be_local() {
    let yaml = "title: x\nscope: local\n";
    let fm = parse(yaml).unwrap();
    assert_eq!(fm.scope, Scope::Local);
}

#[test]
fn slug_integration_with_title() {
    let yaml = "title: Some Article\n";
    let fm = parse(yaml).unwrap();
    let s = slugify(fm.title.as_deref().unwrap());
    assert_eq!(s, "some-article");
}

#[test]
fn sources_with_origin_none_omitted_in_yaml() {
    let fm = Frontmatter {
        sources: vec![SourceRef::upload("sources/x.md")],
        ..Default::default()
    };
    let s = to_yaml(&fm).unwrap();
    // `origin:` is skipped when None, so it must not appear.
    assert!(!s.contains("origin:"));
}
