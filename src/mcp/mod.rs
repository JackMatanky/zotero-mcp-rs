//! Model Context Protocol (MCP) server integration and tool handlers.
//!
//! This module implements the MCP server interface ([`ZoteroMcpServer`])
//! exposing Zotero capabilities to AI models through standard MCP tools,
//! resources, and prompts.
//!
//! # Submodules
//!
//! - `better_bibtex`: Tools for Better `BibTeX` integration (citekeys,
//!   bibliographies, search).
//! - `better_notes`: Tools for Better Notes plugin integration (export, import,
//!   templates, trees).
//! - `connector_tools`: High-level `search` and `fetch` compatibility tools.
//! - `pdf`: PDF path resolution and security policy enforcement.
//! - `resources`: Resource and prompt definitions (`zotero://...`).
//! - `server`: Main server struct definition and tool routing logic.
//! - `zotero`: Core Zotero Local API tools (read, write, collections, tags,
//!   annotations).

mod better_bibtex;
mod better_notes;
mod connector_tools;
mod pdf;
mod resources;
mod server;
mod zotero;

use rmcp::model::{CallToolResult, Content};
use serde::Serialize;
pub(crate) use server::ZoteroMcpServer;

/// Wraps a text message in a successful [`CallToolResult`].
fn text_success(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text.into())])
}

/// Wraps an error message in an error [`CallToolResult`].
fn text_error(error: &(impl ToString + ?Sized)) -> CallToolResult {
    CallToolResult::error(vec![Content::text(error.to_string())])
}

/// Converts a text [`Result<String, E>`] into a [`CallToolResult`].
fn text_result<E: ToString>(result: Result<String, E>) -> CallToolResult {
    match result {
        Ok(text) => text_success(text),
        Err(e) => text_error(&e),
    }
}

/// Formats a value as pretty JSON and wraps it in a successful
/// [`CallToolResult`].
fn json_success<T: Serialize>(value: &T) -> CallToolResult {
    text_success(serde_json::to_string_pretty(value).unwrap_or_default())
}

/// Converts a [`Result<T, E>`] into a JSON-formatted [`CallToolResult`].
fn json_result<T: Serialize, E: ToString>(
    result: Result<T, E>,
) -> CallToolResult {
    match result {
        Ok(value) => json_success(&value),
        Err(e) => text_error(&e),
    }
}
