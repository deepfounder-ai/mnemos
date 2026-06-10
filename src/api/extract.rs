//! `AuthContext` request extractor.
//!
//! The auth middleware in `crate::auth::middleware::require_auth` injects an
//! [`AuthContext`] into the request extensions on every authenticated
//! route. This extractor pulls it back out, returning a 401 if the
//! middleware was bypassed (e.g. on a public route that nevertheless
//! tries to read the auth context).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::auth::middleware::AuthContext;
use crate::storage::AppState;

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthContext
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(ctx) = parts.extensions.get::<AuthContext>().cloned() {
            return Ok(ctx);
        }
        // Should never happen on routes wired through `require_auth`, but
        // we surface a clean 401 instead of a 500 if a misconfiguration
        // leaves the extension empty.
        Err(unauthorized())
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": { "code": "unauthenticated", "message": "missing auth context" }
        })),
    )
        .into_response()
}
