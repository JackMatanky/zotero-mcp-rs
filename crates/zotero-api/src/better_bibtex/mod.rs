//! Bridge to the Better `BibTeX` Zotero plugin JSON-RPC API.
//!
//! Provides [`BetterBibtexClient`] for issuing RPC requests to an active Better
//! `BibTeX` extension running inside Zotero.
//! See [`BetterBibtexClient`] for example usage.

mod client;
mod models;

pub use client::BetterBibtexClient;
pub use models::{
    AutoExportAddRequest, AuxFilePath, BibliographyContentType,
    BibliographyFormat, CollectionPath, CslStyleId, Locale, SearchQuery,
    TranslatorName,
};
