//! HTTP API (Axum). Mirrors `docs/api.md`: a public health probe + auth
//! endpoints, and an authenticated `/api/v1` surface guarded by the
//! `require_auth` middleware.

pub mod dto;
pub mod extract;
pub mod handlers;

use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::auth::middleware::require_auth;
use crate::config::Config;
use crate::storage::AppState;

/// Build the Axum router.
///
/// Routes split into three groups:
/// - public: `/healthz`, `/api/v1/auth/{register,login}`
/// - authenticated: everything else under `/api/v1`, behind `require_auth`.
pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Authenticated routes. The `require_auth` middleware injects an
    // `AuthContext` into request extensions, which handlers pull out via the
    // `AuthContext` extractor.
    let protected = Router::new()
        .route("/auth/whoami", get(handlers::whoami))
        .route(
            "/auth/keys",
            get(handlers::list_keys).post(handlers::create_key),
        )
        .route("/auth/keys/:id", delete(handlers::revoke_key))
        .route(
            "/pages",
            get(handlers::list_pages).post(handlers::create_page),
        )
        .route(
            "/pages/:slug",
            get(handlers::get_page)
                .put(handlers::update_page)
                .delete(handlers::delete_page),
        )
        .route("/pages/:slug/raw", get(handlers::get_page_raw))
        .route("/sources", get(handlers::list_sources))
        .route("/sources/url", post(handlers::add_source_url))
        .route("/sources/upload", post(handlers::upload_source))
        .route("/sources/:id", get(handlers::get_source))
        .route("/sources/:id/raw", get(handlers::get_source_raw))
        .route("/search", get(handlers::search))
        .route("/index", get(handlers::get_index))
        .route("/log", get(handlers::get_log))
        .route("/lint", get(handlers::lint))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    // Public auth routes (no middleware).
    let public_auth = Router::new()
        .route("/auth/register", post(handlers::register))
        .route("/auth/login", post(handlers::login));

    let api_v1 = public_auth.merge(protected);

    Router::new()
        .route("/", get(handlers::root))
        .route("/healthz", get(handlers::healthz))
        .nest("/api/v1", api_v1)
        .fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// Run the HTTP server on the configured host:port.
pub async fn serve(state: AppState, config: &Config) -> std::io::Result<()> {
    let app = router(state);
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "mnemos api listening");
    axum::serve(listener, app).await
}

async fn not_found() -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "error": { "code": "not_found", "message": "route not found" }
        })),
    )
        .into_response()
}
