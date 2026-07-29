//! Bridge to the Zotero Local HTTP API.
//!
//! Re-exports [`ZoteroClient`], the thin async wrapper used by every
//! `zotero_*` MCP tool to read and write the local Zotero library.

mod client;
mod models;

pub(crate) use client::ZoteroClient;
