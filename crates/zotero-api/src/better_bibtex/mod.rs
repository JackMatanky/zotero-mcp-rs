//! Bridge to the Better `BibTeX` Zotero plugin JSON-RPC API.
//!
//! Provides [`BetterBibtexClient`] for issuing RPC requests to an active Better
//! `BibTeX` extension running inside Zotero. Used by MCP tool handlers in
//! `crate::mcp::better_bibtex` to search citation keys, export bibliographies,
//! add auto-exports, and scan `LaTeX` `.aux` files.
//!
//! See [`BetterBibtexClient`] for example usage.

mod client;
mod models;

pub use client::BetterBibtexClient;
pub use models::{
    AutoExportAddRequest, AuxFilePath, BibliographyContentType,
    BibliographyFormat, CollectionPath, CslStyleId, Locale, SearchQuery,
    TranslatorName,
};
