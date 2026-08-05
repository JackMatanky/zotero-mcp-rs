//! MCP tool handlers and argument models for Zotero operations.
//!
//! Each submodule implements tool routers and request payload types for a
//! specific Zotero domain, bridging incoming MCP requests to the underlying
//! Zotero client API.
//!
//! # Submodules
//!
//! - [`annotations`]: Synthesize and annotation action handlers
//!   (`zotero_notes`, `zotero_notes_write`).
//! - [`attachments`]: Attachment file import action handlers
//!   (`zotero_items_write`).
//! - [`collections`]: Collection management tool handlers
//!   (`zotero_collections`, `zotero_collections_write`).
//! - [`coverage`]: Search coverage inspection handler (`zotero_search`).
//! - [`duplicates`]: Duplicate item detection handler (`zotero_search`).
//! - [`fulltext`]: Full-text content retrieval handler (`zotero_items`).
//! - [`items`]: Core item lifecycle handlers and compatibility dispatch
//!   (`zotero_items`, `zotero_items_write`).
//! - [`metadata`]: Item metadata retrieval and identifier lookup handlers
//!   (`zotero_items`, `zotero_items_write`).
//! - [`notes`]: Note listing and creation handlers (`zotero_notes`,
//!   `zotero_notes_write`).
//! - [`pdf`]: PDF retrieval and text extraction handler (`zotero_pdf`).
//! - [`relations`]: Related item relationship handlers (`zotero_relations`,
//!   `zotero_relations_write`).
//! - [`search`]: Item, tag, citation key, and advanced search handlers
//!   (`zotero_search`).
//! - [`sqlite`]: Local `SQLite` database search handler
//!   (`zotero_sqlite_search`).
//! - [`status`]: Zotero API connection status handler (`zotero_status`).
//! - [`tags`]: Tag management handlers (`zotero_tags`, `zotero_tags_write`).
//!
//! # Main Types
//!
//! - [`GetItemMetadataArgs`]: Arguments for item metadata retrieval.
//! - [`SearchItemsArgs`]: Arguments for Zotero item search.
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::mcp::zotero::SearchItemsArgs;
//! let args = SearchItemsArgs::for_connector("rust".to_string());
//! ```
mod annotations;
mod attachments;
mod collections;
mod coverage;
mod duplicates;
mod fulltext;
mod items;
mod metadata;
mod notes;
pub(crate) use notes::filter_notes;
mod pdf;
mod relations;
mod search;
mod sqlite;
mod status;
mod tags;

#[cfg(test)]
mod fixtures {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    use rmcp::model::CallToolResult;
    use serde_json::json;

    use crate::{security::SecurityConfig, state::AppState};
    pub(in crate::mcp::zotero) fn zotero_state(
        zotero_api_url: String,
    ) -> AppState {
        AppState {
            zotero_api_url,
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: String::new(),
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: true,
            ..AppState::from_env()
        }
    }

    pub(in crate::mcp::zotero) use crate::zotero::test_http::{
        http_response, http_response_with_headers,
    };

    pub(in crate::mcp::zotero) fn mock_server(
        responses: Vec<String>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) =
                    listener.accept().expect("accept connection");
                let mut buf = [0_u8; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    pub(in crate::mcp::zotero) fn security_with_pdf_limit(
        max_pdf_bytes: u64,
    ) -> SecurityConfig {
        SecurityConfig {
            max_pdf_bytes,
            ..SecurityConfig::default()
        }
    }

    pub(in crate::mcp::zotero) fn parent_journal_item() -> serde_json::Value {
        json!({
            "key": "ITEM0001",
            "version": 1,
            "data": {
                "key": "ITEM0001",
                "version": 1,
                "itemType": "journalArticle",
            },
        })
    }

    pub(in crate::mcp::zotero) fn zotero_pdf_server(
        children: &serde_json::Value,
    ) -> String {
        mock_server(vec![
            http_response("200 OK", &parent_journal_item().to_string()),
            http_response("200 OK", &children.to_string()),
        ])
    }

    pub(in crate::mcp::zotero) fn bridge_pdf_root(
        kind: &str,
        path: &std::path::Path,
    ) -> String {
        let body = json!({
            "roots": [{
                "kind": kind,
                "path": path.canonicalize().unwrap(),
            }],
        });
        mock_server(vec![http_response("200 OK", &body.to_string())])
    }

    pub(in crate::mcp::zotero) fn tool_text(res: &CallToolResult) -> String {
        res.content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default()
    }
}
