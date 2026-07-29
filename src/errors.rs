use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum ZoteroMcpError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Local API error: HTTP {status} - {message}")]
    LocalApi { status: u16, message: String },

    #[error("Better BibTeX error: {0}")]
    BetterBibTeX(String),

    #[error("Better Notes error: {0}")]
    BetterNotes(String),

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Constructed when pdf-extract returns error")
    )]
    #[error("PDF extraction error: {0}")]
    PdfExtract(String),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Item not found: {0}")]
    NotFound(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}
