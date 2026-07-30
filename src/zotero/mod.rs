//! Bridge to the Zotero Local HTTP API.

mod client;
pub(crate) mod identifiers;
mod models;
mod read;
mod write;

pub(crate) use client::ZoteroClient;
pub(crate) use identifiers::IdentifierKind;
pub(crate) use models::{
    AnnotationType, CollectionKey, ItemKey, ItemType, ZoteroCollection,
    ZoteroItem,
};
