//! MCP tool handlers and argument models for Zotero operations.
//!
//! Each sibling module owns one grouped-router domain exposed to MCP clients:
//! - `status`: `zotero_status`
//! - `items`: `zotero_items` / `zotero_items_write`
//! - `collections`: `zotero_collections` / `zotero_collections_write`
//! - `notes`: `zotero_notes` / `zotero_notes_write`
//! - `tags`: `zotero_tags` / `zotero_tags_write`
//! - `relations`: `zotero_relations` / `zotero_relations_write`
//! - `search`: `zotero_search`
//! - `sqlite`: `zotero_sqlite_search`
//! - `pdf`: `zotero_pdf`

mod collections;
mod items;
mod notes;
mod pdf;
mod relations;
mod search;
mod sqlite;
mod status;
mod tags;

pub(crate) use items::GetItemMetadataArgs;
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

    pub(in crate::mcp::zotero) fn http_response(
        status: &str,
        body: &str,
    ) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
             application/json\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    pub(in crate::mcp::zotero) fn http_response_with_headers(
        status: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> String {
        let hdrs = headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}\r\n"))
            .collect::<Vec<_>>()
            .join("");
        format!(
            "HTTP/1.1 {status}\r\n{hdrs}Content-Length: {}\r\nContent-Type: \
             application/json\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

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
        children: serde_json::Value,
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
            .map(|t| t.text.to_string())
            .unwrap_or_default()
    }
}
