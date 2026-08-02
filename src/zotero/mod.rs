//! Bridge to the Zotero Local HTTP API.
//!
//! Provides the main client [`ZoteroClient`] and submodules for domain models,
//! item mutations, collection hierarchy management, tag operations, search,
//! library analytics, and public identifier resolution.
//!
//! # Submodules & Key Types
//!
//! - [`ZoteroClient`] - Main async HTTP client scoped to tool calls
//! - [`models`] - Strongly-typed domain models, item shapes, and key newtypes
//! - [`identifiers`] - Metadata resolution for DOI, arXiv, and ISBN lookups

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
pub(crate) use items::{AnnotationDraft, TrashAction};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use local_db::{
    FulltextHit, LocalZoteroDb, NoteAnnotationHit, find_zotero_db,
};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use models::{
    AnnotationType, CitationKey, CollectionKey, ItemKey, ItemType,
    LibraryVersion, TagName, ZoteroCollection, ZoteroItem,
};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use relations::RelatedItem;
pub(crate) use search::{
    JoinMode, SearchCondition, SearchField, SearchOperator, SortDirection,
    SortField,
};
