//! HTTP API (Axum). Phase 1 only exposes the health endpoint and a
//! JSON 404/405 fallthrough; the rest of the surface lands in the
//! `api/handlers` module during the API task.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::storage::AppState;

/// Build the Axum router. Phase 1 = health + version + JSON fallthrough.
pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api_v1 = Router::new()
        // Phase-2 routes will be added here in the API task.
        ;

    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api/v1", api_v1)
        .fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(Arc::new(state))
}

/// Run the HTTP server on the configured host:port.
pub async fn serve(state: AppState, config: &Config) -> std::io::Result<()> {
    let app = router(state);
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "mnemos api listening");
    axum::serve(listener, app).await
}

async fn healthz(State(_state): State<Arc<AppState>>) -> Response {
    let body = json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "build_rev": crate::BUILD_REV,
    });
    (StatusCode::OK, Json(body)).into_response()
}

async fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "code": "not_found", "message": "route not found" })),
    )
        .into_response()
}
