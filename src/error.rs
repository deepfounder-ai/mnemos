//! Unified error type for the `mnemos` service.
//!
//! [`AppError`] is the canonical error returned by every core and storage
//! function. The HTTP layer maps it to [`ApiError`] (defined in
//! [`crate::api::error`]), which implements `axum::response::IntoResponse` to
//! produce a JSON error body.
//!
//! `ApiError` is re-exported from this module for convenience — most
//! call-sites still want `use crate::error::{AppError, ApiError}`.

use thiserror::Error;

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, AppError>;

/// Domain / infrastructure error type.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("validation: {0}")]
    Validation(String),

    #[error("unprocessable: {0}")]
    Unprocessable(String),

    #[error("authentication required")]
    Unauthenticated,

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("invalid input: {0}")]
    BadRequest(String),

    #[error("payload too large: {0}")]
    PayloadTooLarge(String),

    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),

    #[error("database: {0}")]
    Db(#[from] sqlx::Error),

    #[error("migration: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("http: {0}")]
    Http(#[from] reqwest::Error),

    #[error("invalid api key")]
    InvalidApiKey,

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("internal: {0}")]
    Internal(String),
}

impl AppError {
    /// Stable error code used in HTTP responses.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Validation(_) => "validation",
            AppError::Unprocessable(_) => "unprocessable",
            AppError::Unauthenticated => "unauthenticated",
            AppError::Forbidden(_) => "forbidden",
            AppError::BadRequest(_) => "bad_request",
            AppError::PayloadTooLarge(_) => "payload_too_large",
            AppError::InvalidApiKey => "invalid_api_key",
            AppError::InvalidCredentials => "invalid_credentials",
            AppError::Io(_)
            | AppError::Db(_)
            | AppError::Migration(_)
            | AppError::Serde(_)
            | AppError::Yaml(_)
            | AppError::Http(_)
            | AppError::Internal(_) => "internal",
        }
    }
}

/// HTTP-aware error wrapper. Re-exported from [`crate::api::error`] so
/// call-sites can `use crate::error::ApiError` while the canonical
/// definition lives next to the HTTP layer that owns it.
pub use crate::api::error::{ApiError, ApiErrorBody};

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}
