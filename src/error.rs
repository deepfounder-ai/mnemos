//! Unified error type for the `mnemos` service.
//!
//! [`AppError`] is the canonical error returned by every core and storage
//! function. The HTTP layer maps it to [`ApiError`], which implements
//! `axum::response::IntoResponse` to produce a JSON error body.

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

    #[error("authentication required")]
    Unauthenticated,

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("invalid input: {0}")]
    BadRequest(String),

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
            AppError::Unauthenticated => "unauthenticated",
            AppError::Forbidden(_) => "forbidden",
            AppError::BadRequest(_) => "bad_request",
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

/// HTTP-aware error wrapper used by the `api` layer.
#[derive(Debug)]
pub struct ApiError {
    pub status: axum::http::StatusCode,
    pub body: ApiErrorBody,
}

/// JSON body returned to clients on error.
#[derive(Debug, serde::Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(status: axum::http::StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ApiErrorBody {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        let status = match &err {
            AppError::NotFound(_) => axum::http::StatusCode::NOT_FOUND,
            AppError::Conflict(_) => axum::http::StatusCode::CONFLICT,
            AppError::Validation(_) | AppError::BadRequest(_) => axum::http::StatusCode::BAD_REQUEST,
            AppError::Unauthenticated | AppError::InvalidApiKey | AppError::InvalidCredentials => {
                axum::http::StatusCode::UNAUTHORIZED
            }
            AppError::Forbidden(_) => axum::http::StatusCode::FORBIDDEN,
            AppError::Io(_)
            | AppError::Db(_)
            | AppError::Migration(_)
            | AppError::Serde(_)
            | AppError::Yaml(_)
            | AppError::Http(_)
            | AppError::Internal(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = err.to_string();
        let code = err.code();
        // Log internal errors with full context, but never expose details to the
        // client beyond the code.
        if status == axum::http::StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %err, "internal error");
        }
        ApiError {
            status,
            body: ApiErrorBody {
                code: code.to_string(),
                // Avoid leaking internal details.
                message: if status == axum::http::StatusCode::INTERNAL_SERVER_ERROR {
                    "internal server error".to_string()
                } else {
                    message
                },
            },
        }
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = axum::Json(self.body);
        (self.status, body).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}
