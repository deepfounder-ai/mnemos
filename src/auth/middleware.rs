//! Axum middleware that turns a `Authorization: Bearer mnemo_…` header into
//! an authenticated [`AuthContext`] stored in the request extensions.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::auth::user::UserService;
use crate::storage::AppState;

/// Authenticated principal. Inserted into request extensions by [`require_auth`].
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub api_key_id: String,
}

/// Axum middleware. Returns 401 on missing / invalid credentials.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let Some(header_value) = auth_header else {
        return unauthorized("missing Authorization header");
    };

    let Some(plaintext) = header_value.strip_prefix("Bearer ").or_else(|| {
        // Allow `Token mnemo_…` for symmetry with some CLI conventions.
        header_value.strip_prefix("Token ")
    }) else {
        return unauthorized("unsupported Authorization scheme; expected Bearer mnemo_…");
    };

    let svc = UserService::new(state.clone());
    let resolved = match svc.resolve_api_key(plaintext.trim()).await {
        Ok(Some(r)) => r,
        Ok(None) => return unauthorized("invalid api key"),
        Err(e) => {
            tracing::error!(error = %e, "resolve_api_key failed");
            return internal_error();
        }
    };

    // Best-effort: refresh last_used_at in the background without blocking
    // the request.
    let pool = state.db.clone();
    let key_id = resolved.id.clone();
    tokio::spawn(async move {
        crate::storage::api_key_repo::touch_last_used(&pool, &key_id).await;
    });

    req.extensions_mut().insert(AuthContext {
        user_id: resolved.user_id,
        api_key_id: resolved.id,
    });

    next.run(req).await
}

fn unauthorized(msg: &'static str) -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": { "code": "unauthenticated", "message": msg } })),
    )
        .into_response();
    // RFC 6750 §3 — request Bearer authentication.
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        axum::http::HeaderValue::from_static("Bearer"),
    );
    response
}

fn internal_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": { "code": "internal", "message": "internal server error" } })),
    )
        .into_response()
}
