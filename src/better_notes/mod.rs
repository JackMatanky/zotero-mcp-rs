//! Bridge to the Better Notes plugin's HTTP companion API.
//!
//! Re-exports [`BetterNotesClient`], used by every `better_notes_*` MCP tool
//! (and the note-rendering path of `zotero_get_notes`).

mod client;
mod models;

pub(crate) use client::BetterNotesClient;
pub(crate) use models::{NoteExportFormat, TemplateName};
