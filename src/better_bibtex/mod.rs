//! Bridge to the Better `BibTeX` plugin's JSON-RPC API, with a read-only
//! `SQLite` fallback for fast citekey lookups.
//!
//! Re-exports [`BetterBibtexClient`], used by every `better_bibtex_*` MCP tool.

mod client;
mod models;
/// Read-only `SQLite` citekey cache helpers.
pub(crate) mod sqlite;

pub(crate) use client::BetterBibtexClient;
pub(crate) use models::{
    AutoexportAddRequest, AuxFilePath, BibliographyFormat, CollectionPath,
    ExportFilePath, SearchQuery, TranslatorName,
};
