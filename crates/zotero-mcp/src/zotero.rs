//! MCP tool handlers and argument models for Zotero operations.
//!
//! Each submodule implements tool routers and request payload types for a
//! specific Zotero domain, bridging incoming MCP requests to the underlying
//! Zotero client API.
//!
//! # Submodules
//!
//! - [`collections`]: Collection management tool handlers
//!   (`zotero_collections`, `zotero_collections_write`).
//! - [`items`]: Core item lifecycle handlers, metadata, attachments, fulltext,
//!   and compatibility dispatch (`zotero_items`, `zotero_items_write`).
//! - [`notes`]: Note listing, creation, and PDF annotation synthesis handlers
//!   (`zotero_notes`, `zotero_notes_write`).
//! - [`pdf`]: PDF retrieval, security path validation, and text extraction
//!   handlers (`zotero_pdf`).
//! - [`relations`]: Related item relationship handlers (`zotero_relations`,
//!   `zotero_relations_write`).
//! - [`search`]: Item, tag, citation key, advanced, duplicate, and coverage
//!   search handlers (`zotero_search`).
//! - [`sqlite`]: Local `SQLite` database search handler
//!   (`zotero_sqlite_search`).
//! - [`status`]: Zotero API connection status handler (`zotero_status`).
//! - [`tags`]: Tag management handlers (`zotero_tags`, `zotero_tags_write`).
//!
//! # Main Types
//!
//! - [`GetItemMetadataArgs`](items::GetItemMetadataArgs): Arguments for item
//!   metadata retrieval.
//! - [`SearchItemsArgs`](search::SearchItemsArgs): Arguments for Zotero item
//!   search.
//!
//! # Examples
//!
//! ```ignore
//! # use zotero_zotero::search::SearchItemsArgs;
//! let args = SearchItemsArgs::for_connector("rust".to_string());
//! ```

mod collections;
mod items;
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
    use zotero_api::{AppState, SecurityConfig};

    pub(in crate::zotero) fn zotero_state(zotero_api_url: String) -> AppState {
        AppState::test_default()
            .with_zotero_api_url(zotero_api_url)
            .with_write_enabled(true)
    }

    pub(in crate::zotero) use zotero_api::client::test_http::{
        http_response, http_response_with_headers,
    };

    pub(in crate::zotero) fn mock_server(responses: Vec<String>) -> String {
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

    pub(in crate::zotero) fn security_with_pdf_limit(
        max_pdf_bytes: u64,
    ) -> SecurityConfig {
        let mut config = SecurityConfig::default();
        config.set_max_pdf_bytes(max_pdf_bytes);
        config
    }

    pub(in crate::zotero) fn parent_journal_item() -> serde_json::Value {
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

    pub(in crate::zotero) fn zotero_pdf_server(
        children: &serde_json::Value,
    ) -> String {
        mock_server(vec![
            http_response("200 OK", &parent_journal_item().to_string()),
            http_response("200 OK", &children.to_string()),
        ])
    }

    pub(in crate::zotero) fn bridge_pdf_root(
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

    pub(in crate::zotero) fn tool_text(res: &CallToolResult) -> String {
        res.content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default()
    }
}
