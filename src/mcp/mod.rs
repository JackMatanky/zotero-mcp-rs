//! Model Context Protocol (MCP) server & tool handlers.

mod better_bibtex;
mod better_notes;
mod connector_tools;
mod resources;
mod server;
mod zotero;

use rmcp::model::{CallToolResult, Content};
use serde::Serialize;
pub(crate) use server::ZoteroMcpServer;

fn text_success(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text.into())])
}

fn text_error(error: &(impl ToString + ?Sized)) -> CallToolResult {
    CallToolResult::error(vec![Content::text(error.to_string())])
}

fn text_result<E: ToString>(result: Result<String, E>) -> CallToolResult {
    match result {
        Ok(text) => text_success(text),
        Err(e) => text_error(&e),
    }
}

fn json_success<T: Serialize>(value: &T) -> CallToolResult {
    text_success(serde_json::to_string_pretty(value).unwrap_or_default())
}

fn json_result<T: Serialize, E: ToString>(
    result: Result<T, E>,
) -> CallToolResult {
    match result {
        Ok(value) => json_success(&value),
        Err(e) => text_error(&e),
    }
}
