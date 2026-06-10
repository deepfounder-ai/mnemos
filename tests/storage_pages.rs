//! Integration tests for the page repository (real SQLite, in-memory).
//!
//! We use a tempdir on disk for the SQLite file because the project's
//! `init_pool` always points to a file path. After init, the test runs
//! the full migration suite.

use mnemos::config::Config;
use mnemos::storage::{init_pool, page_repo, user_repo};

async fn boot() -> (mnemos::storage::AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config::for_test(dir.path().to_path_buf());
    let state = init_pool(cfg).await.expect("init pool");
    (state, dir)
}

async fn make_user(state: &mnemos::storage::AppState) -> String {
    let password_hash = mnemos::auth::hash_password("pw").unwrap();
    user_repo::insert(&state.db, "u", &password_hash)
        .await
        .expect("user")
        .id
}

#[tokio::test]
async fn insert_and_get_by_id() {
    let (state, _d) = boot().await;
    let user = make_user(&state).await;
    let row = page_repo::insert(&state.db, &user, "kafka", "Kafka", "{}", "body")
        .await
        .expect("insert");
    assert_eq!(row.slug, "kafka");
    assert_eq!(row.title, "Kafka");
    let fetched = page_repo::get_by_id(&state.db, &row.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, row.id);
    assert_eq!(fetched.body, "body");
}

#[tokio::test]
async fn get_by_slug() {
    let (state, _d) = boot().await;
    let user = make_user(&state).await;
    page_repo::insert(&state.db, &user, "alpha", "Alpha", "{}", "a").await.unwrap();
    let fetched = page_repo::get_by_slug(&state.db, &user, "alpha").await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().title, "Alpha");
    let missing = page_repo::get_by_slug(&state.db, &user, "nope").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn unique_slug_per_user() {
    let (state, _d) = boot().await;
    let user = make_user(&state).await;
    page_repo::insert(&state.db, &user, "x", "X", "{}", "").await.unwrap();
    // Same slug, same user -> conflict.
    let err = page_repo::insert(&state.db, &user, "x", "X2", "{}", "").await.unwrap_err();
    assert!(matches!(err, mnemos::error::AppError::Db(_)));
}

#[tokio::test]
async fn slug_can_repeat_across_users() {
    let (state, _d) = boot().await;
    let u1 = make_user(&state).await;
    let password_hash = mnemos::auth::hash_password("pw").unwrap();
    let u2 = user_repo::insert(&state.db, "u2", &password_hash).await.unwrap().id;
    page_repo::insert(&state.db, &u1, "shared", "Shared", "{}", "").await.unwrap();
    page_repo::insert(&state.db, &u2, "shared", "Shared", "{}", "").await.unwrap();
    let a = page_repo::get_by_slug(&state.db, &u1, "shared").await.unwrap().unwrap();
    let b = page_repo::get_by_slug(&state.db, &u2, "shared").await.unwrap().unwrap();
    assert_ne!(a.id, b.id);
}

#[tokio::test]
async fn update_content_bumps_updated_at() {
    let (state, _d) = boot().await;
    let user = make_user(&state).await;
    let row = page_repo::insert(&state.db, &user, "k", "K", "{}", "old").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let updated = page_repo::update_content(&state.db, &row.id, "K v2", "{}", "new")
        .await
        .unwrap();
    assert_eq!(updated.title, "K v2");
    assert_eq!(updated.body, "new");
    assert!(updated.updated_at > row.updated_at, "updated_at should advance");
    assert_eq!(updated.created_at, row.created_at, "created_at should not change");
}

#[tokio::test]
async fn delete_removes_row() {
    let (state, _d) = boot().await;
    let user = make_user(&state).await;
    let row = page_repo::insert(&state.db, &user, "k", "K", "{}", "").await.unwrap();
    assert!(page_repo::delete(&state.db, &row.id).await.unwrap());
    assert!(page_repo::get_by_id(&state.db, &row.id).await.unwrap().is_none());
    // Second delete returns false.
    assert!(!page_repo::delete(&state.db, &row.id).await.unwrap());
}

#[tokio::test]
async fn list_for_user_orders_by_updated_at() {
    let (state, _d) = boot().await;
    let user = make_user(&state).await;
    page_repo::insert(&state.db, &user, "a", "A", "{}", "").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    page_repo::insert(&state.db, &user, "b", "B", "{}", "").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let r = page_repo::insert(&state.db, &user, "c", "C", "{}", "").await.unwrap();
    // Force a bump on `a` so it sorts to the top.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    page_repo::update_content(&state.db, &r.id, "C2", "{}", "").await.unwrap(); // wrong, fix:
    let a_id = page_repo::get_by_slug(&state.db, &user, "a").await.unwrap().unwrap().id;
    page_repo::update_content(&state.db, &a_id, "A2", "{}", "").await.unwrap();
    let rows = page_repo::list_for_user(&state.db, &user).await.unwrap();
    assert_eq!(rows[0].slug, "a", "expected most-recently-updated first");
}

#[tokio::test]
async fn slugs_for_user() {
    let (state, _d) = boot().await;
    let user = make_user(&state).await;
    page_repo::insert(&state.db, &user, "a", "A", "{}", "").await.unwrap();
    page_repo::insert(&state.db, &user, "b", "B", "{}", "").await.unwrap();
    let slugs = page_repo::slugs_for_user(&state.db, &user).await.unwrap();
    assert_eq!(slugs.len(), 2);
    assert!(slugs.contains(&"a".to_string()));
    assert!(slugs.contains(&"b".to_string()));
}

#[tokio::test]
async fn cascade_delete_on_user_removal() {
    let (state, _d) = boot().await;
    let password_hash = mnemos::auth::hash_password("pw").unwrap();
    let user_id = user_repo::insert(&state.db, "tempo", &password_hash).await.unwrap().id;
    page_repo::insert(&state.db, &user_id, "p1", "P1", "{}", "").await.unwrap();
    page_repo::insert(&state.db, &user_id, "p2", "P2", "{}", "").await.unwrap();

    sqlx::query("DELETE FROM users WHERE id = ?1")
        .bind(&user_id)
        .execute(&state.db)
        .await
        .unwrap();

    let slugs = page_repo::slugs_for_user(&state.db, &user_id).await.unwrap();
    assert!(slugs.is_empty(), "expected pages to cascade-delete with user");
}
