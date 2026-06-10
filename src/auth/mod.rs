//! Authentication & authorisation: password hashing, API key generation,
//! user management, and the Axum middleware that translates a
//! `Authorization: Bearer mnemo_…` header into an authenticated `user_id`.

pub mod api_key;
pub mod middleware;
pub mod password;
pub mod user;

pub use api_key::{generate as generate_api_key, hash_api_key, ApiKey};
pub use middleware::{require_auth, AuthContext};
pub use password::{hash_password, verify_password};
pub use user::UserService;
