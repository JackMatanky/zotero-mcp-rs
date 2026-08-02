//! Zotero Local API client and domain model layer.
//!
//! Provides [`ZoteroClient`] plus modules for the data shapes and operations
//! used by MCP tools.
//!
//! # Submodules and key types
//!
//! - [`ZoteroClient`]: async HTTP client scoped to one tool call.
//! - [`models`]: domain models, JSON shapes, and key newtypes.
//! - [`identifiers`]: DOI, arXiv, and ISBN metadata resolution.

mod analytics;
mod client;
mod collections;
pub(crate) mod identifiers;
mod items;
mod local_db;
mod models;
mod relations;
mod search;
mod tags;

#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use analytics::{
    DuplicateGroup, DuplicateType, ItemCoverageFlags, LibraryCoverage,
};
pub(crate) use client::ZoteroClient;
pub(crate) use collections::CollectionItemAction;
pub(crate) use identifiers::IdentifierKind;
pub(crate) use items::{AnnotationDraft, AnnotationPosition, TrashAction};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use local_db::{
    FulltextHit, LocalZoteroDb, NoteAnnotationHit, find_zotero_db,
};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use models::{
    AnnotationType, CitationKey, CollectionKey, CollectionParent, ItemKey,
    ItemType, LibraryVersion, LinkMode, TagName, ZoteroCollection, ZoteroItem,
};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use relations::RelatedItem;
pub(crate) use search::{
    JoinMode, SearchCondition, SearchField, SearchOperator, SortDirection,
    SortField,
};
