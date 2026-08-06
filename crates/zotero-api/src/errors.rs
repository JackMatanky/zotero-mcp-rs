//! Crate-wide error type unifying failures across all backends.
//!
//! [`ZoteroApiError`] is the unified error type returned by fallible operations
//! in this crate, wrapping network transport failures, protocol errors,
//! database access failures, permission denials, and serialization errors.
//!
//! # Examples
//!
//! ```
//! use zotero_api::ZoteroApiError;
//!
//! fn check_found(found: bool) -> Result<(), ZoteroApiError> {
//!     if !found {
//!         return Err(ZoteroApiError::NotFound("item missing".to_string()));
//!     }
//!     Ok(())
//! }
//! ```

use thiserror::Error;

/// Unified error type for all Zotero API and backend operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ZoteroApiError {
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

    /// Local embedding generation failed.
    #[error("Embedding error: {0}")]
    Embedding(String),

    /// Input/output failure from [`std::io`].
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Local Zotero `SQLite` database could not be located or read.
    #[error("Local database error: {0}")]
    LocalDb(String),

    /// `SQLite` query or connection against the local Zotero database failed.
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
