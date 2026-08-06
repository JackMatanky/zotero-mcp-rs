//! Protocol-agnostic domain library for the Zotero Local API, Better `BibTeX`,
//! Better Notes, and local semantic search.

#[macro_use]
mod macros;
pub mod better_bibtex;
pub mod better_notes;
pub mod bibtex;
pub mod client;
pub mod collections;
pub mod deleted;
pub mod errors;
pub mod items;
pub mod keys;
pub mod metadata;
pub mod notes;
pub mod objects;
pub mod pdf;
pub mod relations;
pub mod search;
pub mod searches;
pub mod security;
pub mod semantic_search;
pub mod settings;
pub mod sqlite;
pub mod state;
pub mod tags;
pub mod types;

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
pub use metadata::IdentifierKind;
pub use notes::{AnnotationDraft, AnnotationPosition};
pub use objects::{
    BatchWriteResponse, ItemLinks, ItemMeta, LibraryInfo, ZoteroCollection,
    ZoteroItem,
};
pub use pdf::*;
pub use search::{
    JoinMode, SearchCondition, SearchField, SearchOperator, SortDirection,
    SortField,
};
pub use searches::SavedSearch;
pub use security::{SecurityConfig, SecurityProfile};
pub use semantic_search::{
    Embedding, EmbeddingProvider, FastEmbedProvider, SemanticIndex,
};
pub use settings::SettingEntry;
pub use sqlite::{LocalZoteroDb, find_zotero_db};
pub use state::AppState;
pub use types::{AnnotationType, CollectionParent, ItemType, LinkMode};
