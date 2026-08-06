//! Domain library for the Zotero Local API, Better `BibTeX`, Better Notes, and
//! semantic search.
//!
//! `zotero-api` provides strongly-typed, async Rust abstractions for inspecting
//! and mutating local Zotero reference management libraries. It supports the
//! HTTP Local API, Better `BibTeX` export engine, Better Notes companion
//! plugin, local `SQLite` database access, and vector semantic search.
//!
//! # Main Components
//!
//! - [`ZoteroClient`]: Core HTTP client for the Zotero Local API (items,
//!   collections, tags, searches, keys).
//! - [`BetterBibtexClient`]: Client for the Better `BibTeX` extension (citation
//!   keys, JSON-RPC auto-export, Aux scanning).
//! - [`BetterNotesClient`]: Client for the Better Notes plugin (Markdown
//!   conversion, note exporting).
//! - [`LocalZoteroDb`]: Direct read-only `SQLite` database query interface.
//! - [`SemanticIndex`]: Local vector embedding index for note and annotation
//!   similarity search.
//! # Examples
//!
//! ```no_run
//! use zotero_api::{AppState, ZoteroClient};
//!
//! # async fn run() -> Result<(), zotero_api::ZoteroApiError> {
//! let state = AppState::from_env();
//! let client = ZoteroClient::new(&state);
//! let status = client.check_status().await;
//! println!("Status online: {}", status.online);
//! # Ok(())
//! # }
//! ```

#[macro_use]
mod macros;
pub mod better_bibtex;
pub mod better_notes;
pub(crate) mod bibtex;
pub mod client;
pub(crate) mod collections;
pub(crate) mod deleted;
pub mod errors;
pub(crate) mod items;
pub(crate) mod keys;
pub(crate) mod metadata;
pub(crate) mod notes;
pub(crate) mod objects;
pub mod pdf;
pub(crate) mod relations;
pub(crate) mod search;
pub(crate) mod searches;
pub mod security;
pub mod semantic_search;
pub(crate) mod settings;
pub mod sqlite;
pub(crate) mod state;
pub(crate) mod tags;
pub(crate) mod types;

pub use better_bibtex::{
    AutoExportAddRequest, AuxFilePath, BetterBibtexClient,
    BibliographyContentType, BibliographyFormat, CollectionPath, CslStyleId,
    Locale, SearchQuery, TranslatorName,
};
pub use better_notes::{BetterNotesClient, NoteExportFormat, TemplateName};
pub use bibtex::{item_to_bibtex, items_to_bibtex};
pub use client::{LibraryTarget, LocalAuthResponse, ZoteroClient};
pub use collections::CollectionItemAction;
pub use deleted::DeletedObjectsResponse;
pub use errors::ZoteroApiError;
pub use items::TrashAction;
pub use keys::{CitationKey, CollectionKey, ItemKey, LibraryVersion, TagName};
pub use metadata::{IdentifierKind, ItemDraft, resolve_metadata};
pub use notes::{AnnotationDraft, AnnotationPosition};
pub use objects::{
    BatchWriteResponse, ItemLinks, ItemMeta, LibraryInfo, LocalApiStatus,
    ZoteroCollection, ZoteroItem,
};
pub use pdf::*;
pub use relations::RelatedItem;
pub use search::{
    JoinMode, PaginationInfo, SearchCondition, SearchField, SearchOperator,
    SearchPage, SortDirection, SortField,
};
pub use searches::SavedSearch;
pub use security::{SecurityConfig, SecurityProfile};
pub use semantic_search::{
    Embedding, EmbeddingProvider, FastEmbedProvider, SemanticIndex,
};
pub use settings::SettingEntry;
pub use sqlite::{
    FulltextHit, LocalZoteroDb, NoteAnnotationHit, find_zotero_db,
};
pub use state::AppState;
pub use types::{AnnotationType, CollectionParent, ItemType, LinkMode};
