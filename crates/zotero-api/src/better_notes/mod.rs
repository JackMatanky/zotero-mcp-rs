//! Bridge to the Better Notes Zotero plugin HTTP companion API.
//!
//! Provides an async HTTP client and data models for communicating with the
//! Better Notes plugin running inside Zotero.
//!
//! Re-exports [`BetterNotesClient`] and primary data types.

mod client;
mod models;

pub use client::BetterNotesClient;
pub use models::{NoteExportFormat, TemplateName};
