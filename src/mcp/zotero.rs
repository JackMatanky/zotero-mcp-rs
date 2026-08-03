//! MCP tool handlers and argument models for Zotero operations.
//!
//! Each sibling module owns one flat Zotero domain exposed to MCP clients:
//! - `status`: `zotero_status`
//! - `items`: `zotero_items` / `zotero_items_write` core item lifecycle plus
//!   compatibility dispatch
//! - `metadata`: `metadata` / `add_by_identifier` actions of `zotero_items` /
//!   `zotero_items_write`
//! - `collections`: `zotero_collections` / `zotero_collections_write`
//! - `notes`: `zotero_notes` / `zotero_notes_write` note list/create plus
//!   compatibility dispatch
//! - `annotations`: `synthesize` action of `zotero_notes` and `annotation`
//!   action of `zotero_notes_write`
//! - `attachments`: `attach_file` action of `zotero_items_write`
//! - `fulltext`: `fulltext` action of `zotero_items`
//! - `tags`: `zotero_tags` / `zotero_tags_write`
//! - `relations`: `zotero_relations` / `zotero_relations_write`
//! - `search`: `zotero_search` item/tag/citation-key/advanced search plus
//!   compatibility dispatch
//! - `duplicates`: `duplicates` action of `zotero_search`
//! - `coverage`: `coverage` action of `zotero_search`
//! - `sqlite`: `zotero_sqlite_search`
//! - `pdf`: `zotero_pdf`

mod annotations;
mod attachments;
mod collections;
mod coverage;
mod duplicates;
mod fulltext;
mod items;
mod metadata;
mod notes;
mod pdf;
mod relations;
mod search;
mod sqlite;
mod status;
mod tags;

pub(crate) use metadata::GetItemMetadataArgs;
pub(crate) use search::SearchItemsArgs;

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
