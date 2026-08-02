//! Zotero Local API client and flat domain layer.
//!
//! Provides [`ZoteroClient`] plus domain modules for Zotero keys, controlled
//! vocabulary types, API objects, resource endpoints, item subdomains,
//! metadata lookup, derived views, and direct `zotero.sqlite` access.

mod annotations;
mod attachments;
mod client;
mod collections;
mod coverage;
mod duplicates;
mod fulltext;
mod items;
mod keys;
pub(crate) mod metadata;
mod notes;
mod objects;
mod relations;
mod search;
mod sqlite;
mod tags;
mod types;

pub(crate) use annotations::{AnnotationDraft, AnnotationPosition};
pub(crate) use client::ZoteroClient;
pub(crate) use collections::CollectionItemAction;
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use coverage::{ItemCoverageFlags, LibraryCoverage};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use duplicates::{DuplicateGroup, DuplicateType};
pub(crate) use items::TrashAction;
pub(crate) use keys::{
    CitationKey, CollectionKey, ItemKey, LibraryVersion, TagName,
};
pub(crate) use metadata::IdentifierKind;
pub(crate) use objects::{ZoteroCollection, ZoteroItem};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use relations::RelatedItem;
pub(crate) use search::{
    JoinMode, SearchCondition, SearchField, SearchOperator, SortDirection,
    SortField,
};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use sqlite::{
    FulltextHit, LocalZoteroDb, NoteAnnotationHit, find_zotero_db,
};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use types::{AnnotationType, CollectionParent, ItemType, LinkMode};
