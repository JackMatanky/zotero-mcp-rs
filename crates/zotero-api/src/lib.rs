//! Protocol-agnostic domain library for the Zotero Local API, Better `BibTeX`,
//! Better Notes, and local semantic search.

#[macro_use]
mod macros;
pub mod better_bibtex;
pub mod better_notes;
pub mod errors;
pub mod pdf;
pub mod security;
pub mod semantic_search;
pub mod state;
pub mod zotero;

pub use better_bibtex::{
    AutoExportAddRequest, AuxFilePath, BetterBibtexClient,
    BibliographyContentType, BibliographyFormat, CollectionPath, CslStyleId,
    Locale, SearchQuery, TranslatorName,
};
pub use better_notes::{BetterNotesClient, NoteExportFormat, TemplateName};
pub use errors::ZoteroApiError;
pub use security::{SecurityConfig, SecurityProfile};
pub use semantic_search::{
    Embedding, EmbeddingProvider, FastEmbedProvider, SemanticIndex,
};
pub use state::AppState;
pub use zotero::{
    AnnotationDraft, AnnotationPosition, AnnotationType, CitationKey,
    CollectionItemAction, CollectionKey, CollectionParent, IdentifierKind,
    ItemKey, ItemType, JoinMode, LibraryVersion, LinkMode, LocalZoteroDb,
    SearchCondition, SearchField, SearchOperator, SortDirection, SortField,
    TagName, TrashAction, ZoteroClient, ZoteroCollection, ZoteroItem,
    find_zotero_db,
};
