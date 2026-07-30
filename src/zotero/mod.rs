//! Bridge to the Zotero Local HTTP API.

mod analytics;
mod client;
mod collections;
pub(crate) mod identifiers;
mod items;
mod models;
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
pub(crate) use models::{
    AnnotationType, CitationKey, CollectionKey, ItemKey, ItemType,
    LibraryVersion, TagName, ZoteroCollection, ZoteroItem,
};
pub(crate) use search::{SearchCondition, SearchField, SearchOperator};
