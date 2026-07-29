//! Bridge to the Zotero Local HTTP API.

mod client;
mod models;
mod read;
mod write;

pub(crate) use client::ZoteroClient;
