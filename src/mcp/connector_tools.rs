//! Connector-compatible MCP `search` and `fetch` tools.
//!
//! Provides simplified, high-level compatibility wrappers (`search` and
//! `fetch`) for MCP clients expecting browser-connector style item retrieval.
//!
//! # Main Types
//!
//! - [`SearchArgs`]: Arguments for the connector-compatible `search` tool.
//! - [`FetchArgs`]: Arguments for the connector-compatible `fetch` tool.
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::mcp::connector_tools::SearchArgs;
//! let args = SearchArgs {
//!     query: "rust".to_string(),
//! };
//! ```
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::zotero::{GetItemMetadataArgs, SearchItemsArgs},
};

// --- Argument Schemas ---

/// Arguments for the connector-compatible `search` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchArgs {
    /// Search query string matched against title, creator, or metadata fields.
    pub(crate) query: String,
}

/// Arguments for the connector-compatible `fetch` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FetchArgs {
    /// Zotero item key or item identifier to fetch.
    pub(crate) id: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    /// Executes connector-compatible Zotero item search using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if item search fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn connector_search_impl(
        &self,
        args: SearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_items_impl(SearchItemsArgs::for_connector(
            args.query,
        ))
        .await
    }

    /// Fetches Zotero item metadata for connector-compatible clients using
    /// `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if item retrieval fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn connector_fetch_impl(
        &self,
        args: FetchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_metadata_impl(GetItemMetadataArgs::json(
            args.id.into(),
        ))
        .await
    }
}

#[tool_router(router = connector_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "search",
        description = "Connector search tool - search Zotero items by query",
        annotations(
            title = "Search Zotero",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn connector_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.connector_search_impl(args).await
    }

    #[tool(
        name = "fetch",
        description = "Connector fetch tool - get Zotero item metadata by \
                       item ID/key",
        annotations(
            title = "Fetch Zotero Item",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn connector_fetch(
        &self,
        Parameters(args): Parameters<FetchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.connector_fetch_impl(args).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::state::AppState;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        pub(super) fn zotero_state(zotero_api_url: String) -> AppState {
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

        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
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
    }

    use fixtures::*;

    mod search {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn connector_search_returns_matching_items() {
            // Arrange
            let item = json!({
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Physics Paper" }
            });
            let base = mock_server(vec![http_response(
                "200 OK",
                &json!([item]).to_string(),
            )]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .connector_search_impl(SearchArgs {
                    query: "quantum".to_owned(),
                })
                .await
                .expect("search succeeded");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    mod fetch {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn connector_fetch_returns_item_metadata() {
            // Arrange
            let item = json!({
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Physics Paper" }
            });
            let base =
                mock_server(vec![http_response("200 OK", &item.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .connector_fetch_impl(FetchArgs {
                    id: "ITEM1".to_owned(),
                })
                .await
                .expect("fetch succeeded");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }
}
