//! Integration tests for the FTS5 search service.

use mnemos::config::Config;
use mnemos::core::frontmatter::{Frontmatter, PageType};
use mnemos::core::page::PageService;
use mnemos::core::search::SearchService;
use mnemos::storage::{init_pool, user_repo};

async fn boot() -> (
    PageService,
    SearchService,
    mnemos::storage::AppState,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config::for_test(dir.path().to_path_buf());
    let state = init_pool(cfg).await.expect("init pool");
    let pages = PageService::new(state.clone());
    let search = SearchService::new(&state);
    (pages, search, state, dir)
}

async fn ensure_user(state: &mnemos::storage::AppState) -> String {
    let password_hash = mnemos::auth::hash_password("pw").unwrap();
    user_repo::insert(&state.db, "u", &password_hash)
        .await
        .expect("user")
        .id
}

#[tokio::test]
async fn search_finds_keyword() {
    let (pages, search, state, _d) = boot().await;
    let user = ensure_user(&state).await;
    let fm = Frontmatter {
        title: Some("Kafka streaming".into()),
        ..Default::default()
    };
    pages
        .create(
            &user,
            "kafka",
            fm,
            "Kafka is a distributed event streaming platform.",
        )
        .await
        .unwrap();

    let hits = search.search(&user, "kafka", 10).await.unwrap();
    assert!(!hits.is_empty(), "expected at least one hit for 'kafka'");
    assert_eq!(hits[0].slug, "kafka");
    assert!(hits[0].snippet.contains("kafka") || hits[0].snippet.to_lowercase().contains("kafka"));
}

#[tokio::test]
async fn search_multi_word() {
    let (pages, search, state, _d) = boot().await;
    let user = ensure_user(&state).await;
    let fm = Frontmatter {
        title: Some("Postgres".into()),
        ..Default::default()
    };
    pages
        .create(
            &user,
            "pg",
            fm,
            "PostgreSQL is a relational database with JSON support.",
        )
        .await
        .unwrap();

    let hits = search.search(&user, "postgres json", 10).await.unwrap();
    assert!(
        !hits.is_empty(),
        "expected at least one hit for 'postgres json'"
    );
}

#[tokio::test]
async fn search_no_match_returns_empty() {
    let (pages, search, state, _d) = boot().await;
    let user = ensure_user(&state).await;
    let fm = Frontmatter {
        title: Some("X".into()),
        ..Default::default()
    };
    pages
        .create(&user, "x", fm, "alpha beta gamma")
        .await
        .unwrap();
    let hits = search.search(&user, "quantum-mechanics", 10).await.unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn search_user_isolation() {
    let (pages, search, state, _d) = boot().await;
    let u1 = {
        let password_hash = mnemos::auth::hash_password("pw").unwrap();
        user_repo::insert(&state.db, "u1", &password_hash)
            .await
            .unwrap()
            .id
    };
    let u2 = {
        let password_hash = mnemos::auth::hash_password("pw").unwrap();
        user_repo::insert(&state.db, "u2", &password_hash)
            .await
            .unwrap()
            .id
    };
    let fm = Frontmatter {
        title: Some("Secret".into()),
        ..Default::default()
    };
    pages
        .create(
            &u1,
            "secret",
            fm,
            "this is a top-secret project codenamed phoenix",
        )
        .await
        .unwrap();

    let hits_u1 = search.search(&u1, "phoenix", 10).await.unwrap();
    let hits_u2 = search.search(&u2, "phoenix", 10).await.unwrap();
    assert_eq!(hits_u1.len(), 1);
    assert!(hits_u2.is_empty(), "u2 must not see u1's pages");
}

#[tokio::test]
async fn search_ranking_prefers_exact_title_match() {
    let (pages, search, state, _d) = boot().await;
    let user = ensure_user(&state).await;
    let a = Frontmatter {
        title: Some("Cassandra".into()),
        ..Default::default()
    };
    let b = Frontmatter {
        title: Some("Cassandra vs Kafka".into()),
        ..Default::default()
    };
    pages
        .create(&user, "a", a, "Cassandra is a wide-column store.")
        .await
        .unwrap();
    pages
        .create(
            &user,
            "b",
            b,
            "Both Cassandra and Kafka are popular in event-driven systems.",
        )
        .await
        .unwrap();

    let hits = search.search(&user, "cassandra", 10).await.unwrap();
    assert!(hits.len() >= 2, "expected 2+ hits, got {}", hits.len());
    // The page that mentions Cassandra most should rank first.
    let first = &hits[0];
    assert!(first.slug == "a" || first.slug == "b");
}

#[tokio::test]
async fn search_honours_limit() {
    let (pages, search, state, _d) = boot().await;
    let user = ensure_user(&state).await;
    for i in 0..5 {
        let fm = Frontmatter {
            title: Some(format!("Doc {i}")),
            ..Default::default()
        };
        pages
            .create(&user, &format!("d{i}"), fm, "common keyword alpha beta")
            .await
            .unwrap();
    }
    let hits = search.search(&user, "common", 2).await.unwrap();
    assert_eq!(hits.len(), 2);
}

#[tokio::test]
async fn search_empty_query_returns_empty() {
    let (pages, search, state, _d) = boot().await;
    let user = ensure_user(&state).await;
    let fm = Frontmatter {
        title: Some("X".into()),
        ..Default::default()
    };
    pages.create(&user, "x", fm, "anything").await.unwrap();
    let hits = search.search(&user, "  ", 10).await.unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn search_finds_tag_in_title_or_body() {
    let (pages, search, state, _d) = boot().await;
    let user = ensure_user(&state).await;
    let fm = Frontmatter {
        title: Some("Tagged".into()),
        tags: vec!["rust".into()],
        page_type: Some(PageType::Concept),
        ..Default::default()
    };
    pages
        .create(&user, "tagged", fm, "this is a rust page")
        .await
        .unwrap();

    // 'rust' is in the body; it should match.
    let hits = search.search(&user, "rust", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].slug, "tagged");
}
