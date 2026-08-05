//! Model Context Protocol (MCP) server integration and tool handlers.
//!
//! This module implements the MCP server interface ([`ZoteroMcpServer`]),
//! exposing Zotero capabilities to AI models through standard MCP tools,
//! resources, and prompts. It routes incoming JSON-RPC requests across
//! domain-specific tool routers and manages response formatting.
//!
//! # Submodules
//!
//! - [`better_bibtex`]: Tools for Better `BibTeX` integration (citekeys,
//!   bibliographies, search).
//! - [`better_notes`]: Tools for Better Notes plugin integration (export,
//!   import, templates, trees).
//! - [`catalog`]: Tool, resource, and prompt capability catalog and discovery.
//! - [`resources`]: Resource and prompt definitions (`zotero://...`).
//! - [`semantic_search`]: Local embedding index and search
//!   (`zotero_semantic_search`).
//! - [`server`]: Main server struct definition and tool routing logic.
//! - [`zotero`]: Core Zotero Local API tools (read, write, collections, tags,
//!   pdf, notes, search).
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::state::AppState;
//! # use zotero_mcp_rs::ZoteroMcpServer;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let state = AppState::from_env();
//! let server = ZoteroMcpServer::new(state);
//! # Ok(())
//! # }
//! ```

mod better_bibtex;
mod better_notes;
mod catalog;
mod resources;
mod semantic_search;
mod server;
mod zotero;

use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
pub(crate) use server::ZoteroMcpServer;

/// Wraps `text` in a successful [`CallToolResult`].
fn text_success(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text.into())])
}

/// Wraps `error` in an error [`CallToolResult`].
fn text_error(error: &(impl ToString + ?Sized)) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(error.to_string())])
}

/// Wraps `result` or `error` in a [`CallToolResult`], matching on [`Result`].
fn text_result<E: ToString>(result: Result<String, E>) -> CallToolResult {
    match result {
        Ok(text) => text_success(text),
        Err(e) => text_error(&e),
    }
}

/// Wraps `value` as pretty-printed JSON in a successful [`CallToolResult`].
fn json_success<T: Serialize>(value: &T) -> CallToolResult {
    text_success(serde_json::to_string_pretty(value).unwrap_or_default())
}

/// Wraps `result` or `error` in a JSON [`CallToolResult`], matching on
/// [`Result`].
fn json_result<T: Serialize, E: ToString>(
    result: Result<T, E>,
) -> CallToolResult {
    match result {
        Ok(value) => json_success(&value),
        Err(e) => text_error(&e),
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::*;

    mod formatting {
        use pretty_assertions::assert_eq;

        use super::*;

        #[derive(Serialize)]
        struct SampleData {
            id: u32,
            name: String,
        }

        #[test]
        fn text_success_wraps_text_in_successful_result() {
            // Act
            let res = text_success("Operation completed");

            assert_eq!(res.is_error, Some(false));
            assert_eq!(res.content.len(), 1);
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str());
            assert_eq!(text, Some("Operation completed"));
        }

        #[test]
        fn text_error_wraps_error_in_error_result() {
            // Act
            let res = text_error("Something went wrong");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert_eq!(res.content.len(), 1);
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str());
            assert_eq!(text, Some("Something went wrong"));
        }

        #[test]
        fn text_result_converts_ok_to_success() {
            // Arrange
            let res_ok: Result<String, &str> = Ok("Success payload".to_owned());

            // Act
            let tool_res = text_result(res_ok);

            assert_eq!(tool_res.is_error, Some(false));
        }

        #[test]
        fn text_result_converts_err_to_error() {
            // Arrange
            let res_err: Result<String, &str> = Err("Failure payload");

            // Act
            let tool_res = text_result(res_err);

            // Assert
            assert_eq!(tool_res.is_error, Some(true));
        }

        #[test]
        fn json_success_formats_value_as_pretty_json() {
            // Arrange
            let data = SampleData {
                id: 42,
                name: "Test Item".to_owned(),
            };

            // Act
            let res = json_success(&data);

            assert_eq!(res.is_error, Some(false));
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str())
                .unwrap_or_default();
            assert!(text.contains("\"id\": 42"));
            assert!(text.contains("\"name\": \"Test Item\""));
        }

        #[test]
        fn json_result_converts_ok_to_json_success() {
            // Arrange
            let data = SampleData {
                id: 1,
                name: "Ok Item".to_owned(),
            };
            let res_ok: Result<SampleData, &str> = Ok(data);

            // Act
            let tool_res = json_result(res_ok);

            assert_eq!(tool_res.is_error, Some(false));
        }

        #[test]
        fn json_result_converts_err_to_text_error() {
            // Arrange
            let res_err: Result<SampleData, &str> = Err("JSON error");

            // Act
            let tool_res = json_result(res_err);

            // Assert
            assert_eq!(tool_res.is_error, Some(true));
        }
    }
}
