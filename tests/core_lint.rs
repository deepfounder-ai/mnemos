//! Integration tests for the linter.

use mnemos::config::Config;
use mnemos::core::frontmatter::{Frontmatter, SourceRef};
use mnemos::core::lint::{FindingKind, Linter, Severity};
use mnemos::core::page::PageService;
use mnemos::core::source::SourceService;
use mnemos::storage::{init_pool, user_repo};

async fn boot() -> (
    PageService,
    SourceService,
    Linter,
    mnemos::storage::AppState,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config::for_test(dir.path().to_path_buf());
    let state = init_pool(cfg).await.expect("init pool");
    let pages = PageService::new(state.clone());
    let sources = SourceService::new(state.clone()).expect("sources");
    let linter = Linter::new(state.clone());
    (pages, sources, linter, state, dir)
}

async fn ensure_user(state: &mnemos::storage::AppState, name: &str) -> String {
    let password_hash = mnemos::auth::hash_password("pw").unwrap();
    user_repo::insert(&state.db, name, &password_hash)
        .await
        .expect("user")
        .id
}

#[tokio::test]
async fn empty_wiki_is_clean() {
    let (_p, _s, linter, state, _d) = boot().await;
    let user = ensure_user(&state, "u").await;
    let report = linter.run(&user).await.unwrap();
    assert!(
        report.findings.is_empty(),
        "expected clean report, got: {:#?}",
        report.findings
    );
}

#[tokio::test]
async fn detects_orphan_page() {
    let (pages, _s, linter, state, _d) = boot().await;
    let user = ensure_user(&state, "u").await;
    let fm = Frontmatter {
        title: Some("Orphan".into()),
        ..Default::default()
    };
    pages
        .create(&user, "orphan", fm, "no related links")
        .await
        .unwrap();

    let report = linter.run(&user).await.unwrap();
    let orphans: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::Orphan)
        .collect();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].ref_, "orphan");
    assert_eq!(orphans[0].severity, Severity::Info);
}

#[tokio::test]
async fn detects_broken_related() {
    let (pages, _s, linter, state, _d) = boot().await;
    let user = ensure_user(&state, "u").await;
    let fm = Frontmatter {
        title: Some("A".into()),
        related: vec!["nope".into()],
        ..Default::default()
    };
    pages.create(&user, "a", fm, "").await.unwrap();
    let report = linter.run(&user).await.unwrap();
    let broken: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::BrokenRelated)
        .collect();
    assert_eq!(broken.len(), 1);
    assert!(broken[0].message.contains("nope"));
}

#[tokio::test]
async fn detects_missing_source() {
    let (pages, _s, linter, state, _d) = boot().await;
    let user = ensure_user(&state, "u").await;
    let fm = Frontmatter {
        title: Some("With source".into()),
        sources: vec![SourceRef::url(
            "sources/ghost-id-slug.md",
            "https://example.com",
        )],
        ..Default::default()
    };
    pages.create(&user, "with-source", fm, "").await.unwrap();
    let report = linter.run(&user).await.unwrap();
    let missing: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::MissingSource)
        .collect();
    assert_eq!(missing.len(), 1);
    assert!(missing[0].message.contains("ghost-id"));
}

#[tokio::test]
async fn happy_path_no_findings() {
    let (pages, sources, linter, state, _d) = boot().await;
    let user = ensure_user(&state, "u").await;
    // Create a real source so the frontmatter ref resolves.
    let src = sources
        .upload(&user, "notes", b"raw source content")
        .await
        .unwrap();
    let mut fm_a = Frontmatter {
        title: Some("A".into()),
        related: vec!["b".into()],
        ..Default::default()
    };
    fm_a.sources = vec![SourceRef::url(
        format!("sources/{}-notes.md", src.source_id),
        "https://example.com",
    )];
    pages.create(&user, "a", fm_a, "a body").await.unwrap();
    let mut fm_b = Frontmatter {
        title: Some("B".into()),
        related: vec!["a".into()],
        ..Default::default()
    };
    fm_b.sources = vec![SourceRef::url(
        format!("sources/{}-notes.md", src.source_id),
        "https://example.com",
    )];
    pages.create(&user, "b", fm_b, "b body").await.unwrap();

    let report = linter.run(&user).await.unwrap();
    // No broken related, no missing source, no orphans.
    for f in &report.findings {
        assert!(
            f.kind != FindingKind::BrokenRelated,
            "unexpected broken: {f:?}"
        );
        assert!(
            f.kind != FindingKind::MissingSource,
            "unexpected missing: {f:?}"
        );
        assert!(f.kind != FindingKind::Orphan, "unexpected orphan: {f:?}");
    }
}

#[tokio::test]
async fn user_isolation_in_lint() {
    let (pages, _s, linter, state, _d) = boot().await;
    let u1 = ensure_user(&state, "u1").await;
    let u2 = ensure_user(&state, "u2").await;
    // u1 has a broken-related page; u2 has nothing.
    let fm = Frontmatter {
        title: Some("X".into()),
        related: vec!["ghost".into()],
        ..Default::default()
    };
    pages.create(&u1, "x", fm, "").await.unwrap();
    let r1 = linter.run(&u1).await.unwrap();
    let r2 = linter.run(&u2).await.unwrap();
    assert!(!r1.findings.is_empty());
    assert!(r2.findings.is_empty(), "u2 must not see u1's findings");
}
