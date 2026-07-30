//! Bridge to the Better `BibTeX` plugin's JSON-RPC API.
//!
//! Re-exports [`BetterBibtexClient`], used by every `better_bibtex_*` MCP tool.

mod client;
mod models;

pub(crate) use client::BetterBibtexClient;
pub(crate) use models::{
    AutoExportAddRequest, AuxFilePath, BibliographyFormat, CollectionPath,
    SearchQuery, TranslatorName,
};
