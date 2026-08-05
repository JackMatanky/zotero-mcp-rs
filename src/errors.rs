//! Crate-wide error type unifying failures from every backend.
//!
//! [`ZoteroMcpError`] is the single error type returned by every fallible
//! operation in the crate, built with `thiserror` so each variant's `Display`
//! message doubles as the text surfaced back to MCP clients.
//!
//! # Examples
//!
//! ```no_run
//! use zotero_mcp_rs::errors::ZoteroMcpError;
//!
//! fn check_found(found: bool) -> Result<(), ZoteroMcpError> {
//!     if !found {
//!         return Err(ZoteroMcpError::NotFound("item missing".to_string()));
//!     }
//!     Ok(())
//! }
//! ```

use thiserror::Error;

/// Unifies failures from the Zotero Local API, Better `BibTeX`, Better Notes,
/// or local filesystem.
#[derive(Debug, Error)]
pub(crate) enum ZoteroMcpError {
    /// Network or HTTP transport failure from [`reqwest`].
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Zotero Local HTTP API responded with a non-2xx status code.
    #[error("Local API error: HTTP {status} - {message}")]
    LocalApi {
        /// HTTP status code returned by the Zotero Local API.
        status: u16,
        /// Error message or body returned by the Zotero Local API.
        message: String,
    },

    /// Better `BibTeX` JSON-RPC endpoint returned an error or invalid response.
    #[error("Better BibTeX error: {0}")]
    BetterBibTeX(String),

    /// Better Notes companion bridge endpoint returned an error or invalid
    /// response.
    #[error("Better Notes error: {0}")]
    BetterNotes(String),

    /// PDF text extraction failed.
    #[error("PDF extraction error: {0}")]
    PdfExtract(String),

    /// Local embedding generation failed (model load, inference, or a
    /// poisoned model mutex).
    #[error("Embedding error: {0}")]
    Embedding(String),

    /// Input/output failure from [`std::io`].
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A local Zotero `SQLite` database could not be read.
    ///
    /// Occurs when the discovery step fails (no Zotero profile found), the
    /// database cannot be opened read-only, or it is not a Zotero database.
    #[error("Local database error: {0}")]
    LocalDb(String),

    /// A `SQLite` query or connection against the local Zotero database failed.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] sqlx::Error),

    /// Write operation attempted when write permission is disabled in
    /// [`AppState`].
    ///
    /// [`AppState`]: crate::state::AppState
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// User-controlled input failed local security policy.
    #[error("Input rejected: {0}")]
    InputRejected(String),

    /// Requested Zotero library item, collection, or resource was not found.
    #[error("Item not found: {0}")]
    NotFound(String),

    /// JSON serialization or deserialization failure from [`serde_json`].
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl ZoteroMcpError {
    /// Returns a sanitized error message suitable for external MCP clients,
    /// suppressing sensitive internal paths, system details, and database
    /// queries.
    pub(crate) fn client_message(&self) -> String {
        match self {
            Self::Sqlite(_) => "Local database query failed".to_owned(),
            Self::Io(err) => format!("I/O error: {}", err.kind()),
            Self::Network(_) => "Upstream network request failed".to_owned(),
            Self::LocalApi {
                status,
                message,
            } => {
                format!("Local API HTTP {status}: {message}")
            }
            _ => self.to_string(),
        }
    }
}
