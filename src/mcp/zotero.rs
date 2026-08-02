//! MCP tool handlers and argument models for core Zotero Local API
//! operations.
//!
//! Each sibling module owns one grouped-router domain exposed to MCP
//! clients:
//! - `items`: `zotero_items` / `zotero_items_write`
//! - `collections`: `zotero_collections` / `zotero_collections_write`
//! - `notes`: `zotero_notes` / `zotero_notes_write`
//! - `tags`: `zotero_tags` / `zotero_tags_write`
//! - `relations`: `zotero_relations` / `zotero_relations_write`
//! - `search`: `zotero_search`
//! - `sqlite`: `zotero_sqlite_search`
//! - `pdf`: `zotero_pdf`
//!
//! This module also hosts the standalone `zotero_status` tool and
//! re-exports every argument type for [`super::server`] and
//! [`super::connector_tools`].

mod collections;
mod items;
mod notes;
mod pdf;
mod relations;
mod search;
mod sqlite;
mod tags;

pub(crate) use items::GetItemMetadataArgs;
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
pub(crate) use search::SearchItemsArgs;
use serde::Deserialize;

use crate::{ZoteroMcpServer, mcp::json_success, zotero::ZoteroClient};

/// Arguments for tools that take no parameters.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct EmptyArgs {}

impl ZoteroMcpServer {
    /// Handles Zotero Local API status tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_status_impl(
        &self,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let status = client.check_status().await;
        Ok(json_success(&status))
    }
}

#[tool_router(router = status_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_status",
        description = "Check Zotero Local API availability, version, and \
                       connectivity",
        annotations(
            title = "Check Zotero Connection",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_status(
        &self,
        Parameters(_args): Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_status_impl().await
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    use rmcp::model::CallToolResult;
    use serde_json::json;

    use crate::{security::SecurityConfig, state::AppState};
    pub(crate) fn zotero_state(zotero_api_url: String) -> AppState {
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

    pub(crate) fn http_response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
             application/json\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    pub(crate) fn http_response_with_headers(
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

    pub(crate) fn mock_server(responses: Vec<String>) -> String {
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

    pub(crate) fn security_with_pdf_limit(
        max_pdf_bytes: u64,
    ) -> SecurityConfig {
        SecurityConfig {
            max_pdf_bytes,
            ..SecurityConfig::default()
        }
    }

    pub(crate) fn parent_journal_item() -> serde_json::Value {
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

    pub(crate) fn zotero_pdf_server(children: serde_json::Value) -> String {
        mock_server(vec![
            http_response("200 OK", &parent_journal_item().to_string()),
            http_response("200 OK", &children.to_string()),
        ])
    }

    pub(crate) fn bridge_pdf_root(
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

    pub(crate) fn tool_text(res: &CallToolResult) -> String {
        res.content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.to_string())
            .unwrap_or_default()
    }
}
