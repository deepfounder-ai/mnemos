//! HTTP error type for the `mnemos` REST API.
//!
//! [`ApiError`] wraps a status code and a structured body, and implements
//! `axum::response::IntoResponse` so any handler can `?`-propagate it and
//! still produce a uniform JSON error envelope.
//!
//! The on-the-wire shape is:
//!
//! ```json
//! { "error": { "code": "<stable_code>", "message": "<human readable>", "details": <optional> } }
//! ```
//!
//! 401 responses also carry `WWW-Authenticate: Bearer` per RFC 6750 §3.
//!
//! Construction paths:
//! - direct: [`ApiError::new`] / [`ApiError::with_details`]
//! - from a domain error: `ApiError::from(AppError)` (the common path,
//!   used implicitly by `?` on `Result<_, AppError>` inside a handler).
//!
//! Status code mapping for [`AppError`]:
//!
//! | `AppError` variant                              | HTTP status               |
//! |-------------------------------------------------|---------------------------|
//! | `NotFound`                                      | 404                       |
//! | `Conflict`                                      | 409                       |
//! | `Validation`, `BadRequest`                      | 400                       |
//! | `Unprocessable`                                 | 422                       |
//! | `Unauthenticated`, `InvalidApiKey`, `InvalidCredentials` | 401 (with `WWW-Authenticate`) |
//! | `Forbidden`                                     | 403                       |
//! | `PayloadTooLarge`                               | 413                       |
//! | `Io`, `Db`, `Migration`, `Serde`, `Yaml`, `Http`, `Internal` | 500 |

use serde_json::Value as JsonValue;

use crate::error::AppError;

/// HTTP-aware error wrapper used by the `api` layer.
#[derive(Debug)]
pub struct ApiError {
    pub status: axum::http::StatusCode,
    pub body: ApiErrorBody,
}

/// JSON body returned to clients on error. Wrapped under `error` in the
/// final response.
#[derive(Debug, serde::Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<JsonValue>,
}

impl ApiError {
    /// Build a new `ApiError` from a status code, error code, and human
    /// message.
    pub fn new(
        status: axum::http::StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status,
            body: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                details: None,
            },
        }
    }

    /// Attach a structured `details` payload (rendered under `error.details`).
    pub fn with_details(mut self, details: JsonValue) -> Self {
        self.body.details = Some(details);
        self
    }
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        let status = match &err {
            AppError::NotFound(_) => axum::http::StatusCode::NOT_FOUND,
            AppError::Conflict(_) => axum::http::StatusCode::CONFLICT,
            AppError::Validation(_) | AppError::BadRequest(_) => {
                axum::http::StatusCode::BAD_REQUEST
            }
            AppError::Unprocessable(_) => axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Unauthenticated | AppError::InvalidApiKey | AppError::InvalidCredentials => {
                axum::http::StatusCode::UNAUTHORIZED
            }
            AppError::Forbidden(_) => axum::http::StatusCode::FORBIDDEN,
            AppError::PayloadTooLarge(_) => axum::http::StatusCode::PAYLOAD_TOO_LARGE,
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
                details: None,
            },
        }
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status;
        let body = axum::Json(serde_json::json!({ "error": self.body }));
        let mut response = (status, body).into_response();
        if status == axum::http::StatusCode::UNAUTHORIZED {
            // RFC 6750 §3 — request Bearer authentication.
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}
