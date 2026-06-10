//! CLI error type, exit-code mapping, and small print helpers.
//!
//! Exit codes are stable (see `docs/cli.md`): scripts branch on them.

use std::fmt;
use std::process::ExitCode;

/// Result alias used throughout the CLI command modules.
pub type CliResult<T> = std::result::Result<T, CliError>;

/// A CLI-side error. Carries enough context to pick a stable exit code.
#[derive(Debug)]
pub enum CliError {
    /// Misuse before any HTTP happened (e.g. no API key configured).
    Usage(String),
    /// Could not reach the server.
    Network(String),
    /// The server returned a non-2xx response.
    Http {
        status: u16,
        code: String,
        message: String,
    },
    /// Local I/O failure (reading a file, writing stdout).
    Io(std::io::Error),
    /// JSON (de)serialisation failure.
    Serde(serde_json::Error),
    /// Anything else.
    Other(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage(m) => write!(f, "{m}"),
            CliError::Network(m) => write!(f, "could not reach server: {m}"),
            CliError::Http {
                status,
                code,
                message,
            } => write!(f, "server error {status} ({code}): {message}"),
            CliError::Io(e) => write!(f, "io error: {e}"),
            CliError::Serde(e) => write!(f, "json error: {e}"),
            CliError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<reqwest::Error> for CliError {
    fn from(e: reqwest::Error) -> Self {
        CliError::Network(e.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::Serde(e)
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::Io(e)
    }
}

/// Map an error to a process exit code and print it to stderr.
pub fn fail(err: CliError) -> ExitCode {
    eprintln!("mnemos: {err}");
    ExitCode::from(exit_code(&err))
}

/// Stable exit codes, matching `docs/cli.md`.
fn exit_code(err: &CliError) -> u8 {
    match err {
        CliError::Usage(_) => 1,
        CliError::Network(_) => 3,
        CliError::Http { status, .. } => match status {
            401 | 403 => 4,
            422 => 5,
            404 => 6,
            409 => 7,
            500..=599 => 64,
            _ => 1,
        },
        CliError::Io(_) | CliError::Serde(_) | CliError::Other(_) => 1,
    }
}

/// Print a serialisable value as pretty JSON to stdout.
pub fn print_json<T: serde::Serialize>(value: &T) -> CliResult<()> {
    let s = serde_json::to_string_pretty(value)?;
    println!("{s}");
    Ok(())
}

/// Print a plain line to stdout.
pub fn print_line(s: impl AsRef<str>) {
    println!("{}", s.as_ref());
}
