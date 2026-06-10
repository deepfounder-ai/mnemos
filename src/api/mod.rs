//! HTTP API (Axum). Mirrors `docs/api.md`: a public health probe + auth
//! endpoints, and an authenticated `/api/v1` surface guarded by the
//! `require_auth` middleware.
//!
//! File layout:
//! - [`dto`] — request / response types (serde-derived, hand-validated)
//! - [`error`] — `ApiError` + `IntoResponse` (the HTTP-aware error wrapper)
//! - [`handlers`] — one function per endpoint
//! - [`extract`] — `AuthContext` request extractor
//! - [`router`] — assembles the route table and runs the server

pub mod dto;
pub mod error;
pub mod extract;
pub mod handlers;
pub mod router;

pub use router::{router, serve};
