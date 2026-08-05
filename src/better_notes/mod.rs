//! Bridge to the Better Notes Zotero plugin's HTTP companion API.
//!
//! This module provides an async HTTP client and data models for communicating
//! with the Better Notes plugin running inside Zotero. It is used by the
//! `better_notes_*` MCP tools in `crate::mcp::better_notes` and the
//! note-rendering paths in `crate::mcp::zotero`.
//!
//! Re-exports [`BetterNotesClient`] and primary data types. For usage examples,
//! see [`BetterNotesClient`].

mod client;
mod models;

pub(crate) use client::BetterNotesClient;
pub(crate) use models::{NoteExportFormat, TemplateName};
