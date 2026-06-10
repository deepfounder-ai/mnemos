//! `mnemos` — cloud memory for AI agents.
//!
//! This crate is organised as a single-binary + library so that integration
//! tests and downstream consumers can re-use the domain services without
//! spawning the `mnemos` process.
//!
//! Layered architecture (lower layers depend only on layers below them):
//!
//! ```text
//! cli / mcp / api (transport)
//!        │
//!        ▼
//!     core (page, source, index, log, search, lint)
//!        │
//!        ▼
//!    storage (sqlx repos + fs)   auth (user, api_key, middleware)
//!        │
//!        ▼
//!     error, config
//! ```
//!
//! See `docs/page-format.md` (filled in by the docs task) for the wiki page
//! format. The Rust types in [`core::frontmatter`] mirror the frontmatter
//! structure that the service accepts and emits.

#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]

pub mod api;
pub mod auth;
pub mod cli;
pub mod config;
pub mod core;
pub mod error;
pub mod mcp;
pub mod storage;

pub use error::{AppError, Result};

/// Library version (mirrors `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Git revision embedded at build time, if any.
pub const BUILD_REV: Option<&str> = option_env!("VERGEN_GIT_SHA");
