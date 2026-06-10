//! High-level user service: register, login, list / create / revoke keys.

use crate::auth::{hash_password, verify_password, ApiKey};
use crate::error::{AppError, Result};
use crate::storage::{api_key_repo, user_repo, AppState};

/// High-level user operations.
#[derive(Clone, Debug)]
pub struct UserService {
    state: AppState,
}

impl UserService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Register a new user with a username + password. On success, returns
    /// the user row and a freshly-issued API key (plaintext shown once).
    pub async fn register(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(user_repo::UserRow, ApiKey)> {
        validate_username(username)?;
        if password.is_empty() {
            return Err(AppError::Validation("password must not be empty".into()));
        }

        if user_repo::get_by_username(&self.state.db, username)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "username '{username}' is already taken"
            )));
        }

        let password_hash = hash_password(password)?;
        let user = user_repo::insert(&self.state.db, username, &password_hash).await?;
        let key_row = api_key_repo::insert(
            &self.state.db,
            &user.id,
            "default",
            // Placeholder; replaced by generated plaintext.
            "pending",
        )
        .await?;

        // Generate the real key tied to the freshly-inserted id.
        let key = crate::auth::api_key::generate(&key_row.id, "default");
        // Persist the actual hash and rename the row.
        sqlx::query("UPDATE api_keys SET key_hash = ?1 WHERE id = ?2")
            .bind(&key.hash)
            .bind(&key_row.id)
            .execute(&self.state.db)
            .await?;

        // Append a structured event.
        crate::storage::event_repo::append(
            &self.state.db,
            &user.id,
            "user.register",
            Some(&user.id),
            Some(&serde_json::json!({ "username": username })),
        )
        .await?;

        Ok((user, key))
    }

    /// Validate a (username, password) pair. Returns the user row and a
    /// freshly-issued API key on success.
    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(user_repo::UserRow, ApiKey)> {
        let user = user_repo::get_by_username(&self.state.db, username)
            .await?
            .ok_or(AppError::InvalidCredentials)?;

        if !verify_password(password, &user.password_hash)? {
            return Err(AppError::InvalidCredentials);
        }

        // Create a fresh key per login.
        let key_row = api_key_repo::insert(&self.state.db, &user.id, "login", "pending").await?;
        let key = crate::auth::api_key::generate(&key_row.id, "login");
        sqlx::query("UPDATE api_keys SET key_hash = ?1 WHERE id = ?2")
            .bind(&key.hash)
            .bind(&key_row.id)
            .execute(&self.state.db)
            .await?;

        crate::storage::event_repo::append(
            &self.state.db,
            &user.id,
            "user.login",
            Some(&user.id),
            None,
        )
        .await?;

        Ok((user, key))
    }

    /// Resolve an API key plaintext to its user id. Returns `None` if the key
    /// is unknown or malformed.
    pub async fn resolve_api_key(
        &self,
        plaintext: &str,
    ) -> Result<Option<api_key_repo::ApiKeyRow>> {
        if !crate::auth::api_key::looks_like_api_key(plaintext) {
            return Ok(None);
        }
        let hash = crate::auth::api_key::hash_api_key(plaintext);
        let row = api_key_repo::get_by_hash(&self.state.db, &hash).await?;
        Ok(row)
    }

    /// Issue a new API key for a user, with a friendly name.
    pub async fn create_api_key(&self, user_id: &str, name: &str) -> Result<ApiKey> {
        if name.trim().is_empty() {
            return Err(AppError::Validation("key name must not be empty".into()));
        }
        let key_row = api_key_repo::insert(&self.state.db, user_id, name, "pending").await?;
        let key = crate::auth::api_key::generate(&key_row.id, name);
        sqlx::query("UPDATE api_keys SET key_hash = ?1 WHERE id = ?2")
            .bind(&key.hash)
            .bind(&key_row.id)
            .execute(&self.state.db)
            .await?;

        crate::storage::event_repo::append(
            &self.state.db,
            user_id,
            "api_key.create",
            Some(&key_row.id),
            Some(&serde_json::json!({ "name": name })),
        )
        .await?;

        Ok(key)
    }

    /// List all API keys for a user (metadata only — no plaintext).
    pub async fn list_api_keys(&self, user_id: &str) -> Result<Vec<api_key_repo::ApiKeyRow>> {
        api_key_repo::list_for_user(&self.state.db, user_id).await
    }

    /// Revoke an API key by id. Returns `true` if it existed and was deleted.
    pub async fn revoke_api_key(&self, user_id: &str, key_id: &str) -> Result<bool> {
        let row = api_key_repo::get_by_id(&self.state.db, key_id).await?;
        match row {
            Some(r) if r.user_id == user_id => {
                let removed = api_key_repo::delete(&self.state.db, key_id).await?;
                if removed {
                    crate::storage::event_repo::append(
                        &self.state.db,
                        user_id,
                        "api_key.revoke",
                        Some(key_id),
                        None,
                    )
                    .await?;
                }
                Ok(removed)
            }
            Some(_) => Err(AppError::Forbidden(
                "api key belongs to another user".into(),
            )),
            None => Ok(false),
        }
    }
}

fn validate_username(username: &str) -> Result<()> {
    if username.is_empty() {
        return Err(AppError::Validation("username must not be empty".into()));
    }
    if username.len() > 64 {
        return Err(AppError::Validation("username is too long (max 64)".into()));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(AppError::Validation(
            "username may only contain [A-Za-z0-9_.-]".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    async fn svc() -> (UserService, TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = Config::for_test(dir.path().to_path_buf());
        let state = crate::storage::init_pool(cfg).await.expect("init pool");
        (UserService::new(state), dir)
    }

    #[tokio::test]
    async fn register_then_login() {
        let (svc, _d) = svc().await;
        let (user, key) = svc.register("alice", "hunter2").await.expect("register");
        assert_eq!(user.username, "alice");
        assert!(key.plaintext.starts_with("mnemo_"));

        // Login with same credentials works.
        let (u2, k2) = svc.login("alice", "hunter2").await.expect("login");
        assert_eq!(u2.id, user.id);
        assert!(k2.plaintext.starts_with("mnemo_"));
    }

    #[tokio::test]
    async fn register_duplicate_rejected() {
        let (svc, _d) = svc().await;
        svc.register("bob", "x").await.expect("first");
        let err = svc.register("bob", "x").await.unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[tokio::test]
    async fn register_bad_username() {
        let (svc, _d) = svc().await;
        assert!(svc.register("", "x").await.is_err());
        assert!(svc.register("hi!", "x").await.is_err());
        assert!(svc.register(&"a".repeat(65), "x").await.is_err());
    }

    #[tokio::test]
    async fn login_wrong_password() {
        let (svc, _d) = svc().await;
        svc.register("carol", "right").await.expect("register");
        let err = svc.login("carol", "wrong").await.unwrap_err();
        assert!(matches!(err, AppError::InvalidCredentials));
    }

    #[tokio::test]
    async fn api_key_lifecycle() {
        let (svc, _d) = svc().await;
        let (user, _k0) = svc.register("dave", "x").await.expect("register");
        let k = svc.create_api_key(&user.id, "laptop").await.expect("key");
        let resolved = svc.resolve_api_key(&k.plaintext).await.expect("resolve");
        let resolved = resolved.expect("some");
        assert_eq!(resolved.user_id, user.id);
        assert_eq!(resolved.name, "laptop");

        // Revoke.
        let removed = svc
            .revoke_api_key(&user.id, &resolved.id)
            .await
            .expect("revoke");
        assert!(removed);
        assert!(svc
            .resolve_api_key(&k.plaintext)
            .await
            .expect("resolve2")
            .is_none());
    }
}
