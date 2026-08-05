//! MCP tool handlers and argument models for Zotero library search.
//!
//! Exposes the `zotero_search` MCP tool router, allowing clients to search
//! items by full text, tag, citation key, or structured multi-condition
//! queries.
//!
//! # Main Types
//!
//! - [`ZoteroSearchCommand`] - Grouped-router command for search actions
//! - [`SearchItemsArgs`] - Arguments for full-text search across item fields
//! - [`SearchByCitationKeyArgs`] - Arguments for searching items by citation
//!   key
//! - [`AdvancedSearchArgs`] - Arguments for structured multi-condition search
//!
//! # Examples
//!
//! ```no_run
//! # use rmcp::handler::server::wrapper::Parameters;
//! # use zotero_mcp_rs::ZoteroMcpServer;
//! # use zotero_mcp_rs::mcp::zotero::search::{
//! #     ZoteroSearchCommand,
//! #     SearchItemsArgs,
//! # };
//! # async fn run(
//! #     server: ZoteroMcpServer,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let args = Parameters(ZoteroSearchCommand::Items(
//!     SearchItemsArgs::for_connector("quantum computing".to_string()),
//! ));
//! let result = server.zotero_search(args).await?;
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Arguments for the connector-compatible `search` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ConnectorSearchArgs {
    /// Search query string matched against title, creator, or metadata
    /// fields.
    pub(crate) query: String,
}

use super::tags::SearchByTagArgs;
use crate::{
    ZoteroMcpServer,
    mcp::json_result,
    zotero::{
        CitationKey, CollectionKey, JoinMode, SearchCondition, SortDirection,
        SortField, ZoteroClient,
    },
};

/// Arguments for the `items` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query matched against title, creator, year, or full-text
    /// content.
    query: String,
    /// Optional collection key ([`CollectionKey`]) to search within.
    collection_key: Option<CollectionKey>,
    /// Zero-based offset into the full result set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    limit: Option<usize>,
}

impl SearchItemsArgs {
    /// Constructs full-text search arguments with default offset and limit.
    pub(crate) fn for_connector(query: String) -> Self {
        Self {
            query,
            collection_key: None,
            start: None,
            limit: Some(20),
        }
    }
}

/// Arguments for the `citation_key` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByCitationKeyArgs {
    /// Citation key ([`CitationKey`]) to match.
    citekey: CitationKey,
}

/// Arguments for the `advanced` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions ([`SearchCondition`]).
    conditions: Vec<SearchCondition>,
    /// Match mode: `"all"` ([`JoinMode::All`], AND, default) or `"any"`
    /// ([`JoinMode::Any`], OR).
    join_mode: Option<JoinMode>,
    /// Sort field ([`SortField`]): `"dateAdded"`, `"dateModified"`, `"title"`,
    /// `"date"`, or `"creator"`.
    sort_by: Option<SortField>,
    /// Sort direction: `"asc"` or `"desc"` ([`SortDirection`], default:
    /// `"asc"`).
    sort_direction: Option<SortDirection>,
    /// Zero-based offset into the full result set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Search commands dispatched by the `zotero_search` MCP tool router.
pub(crate) enum ZoteroSearchCommand {
    /// Full-text search across item fields.
    Items(SearchItemsArgs),
    /// Find items by tag name.
    Tag(SearchByTagArgs),
    /// Find items by `BibTeX` citation key.
    CitationKey(SearchByCitationKeyArgs),
    /// Run a structured search with multiple conditions.
    Advanced(AdvancedSearchArgs),
    /// Find potential duplicate items in a library.
    Duplicates(crate::mcp::zotero::duplicates::FindDuplicatesArgs),
    /// Report coverage statistics for a library.
    Coverage(crate::mcp::zotero::coverage::LibraryCoverageArgs),
}

#[tool_router(router = search_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_search",
        description = "Grouped Zotero search router. action: items, tag, \
                       citation_key, advanced, duplicates, coverage",
        annotations(
            title = "Search Zotero Library",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// Dispatches search requests to the appropriate search handler.
    ///
    /// Accepts a [`Parameters<ZoteroSearchCommand>`] containing the specific
    /// action and parameters, routing it to internal search handlers.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if the underlying tool handler fails or
    /// returns an error.
    pub(crate) async fn zotero_search(
        &self,
        Parameters(args): Parameters<ZoteroSearchCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroSearchCommand::Items(args) => {
                self.zotero_search_items_impl(args).await
            }
            ZoteroSearchCommand::Tag(args) => {
                self.zotero_search_by_tag_impl(args).await
            }
            ZoteroSearchCommand::CitationKey(args) => {
                self.zotero_search_by_citation_key_impl(args).await
            }
            ZoteroSearchCommand::Advanced(args) => {
                self.zotero_advanced_search_impl(args).await
            }
            ZoteroSearchCommand::Duplicates(args) => {
                self.zotero_find_duplicates_impl(args).await
            }
            ZoteroSearchCommand::Coverage(args) => {
                self.zotero_library_coverage_impl(args).await
            }
        }
    }

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
        Parameters(args): Parameters<ConnectorSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_items_impl(SearchItemsArgs::for_connector(
            args.query,
        ))
        .await
    }
}

impl ZoteroMcpServer {
    /// Handles Zotero item search tool calls.
    ///
    /// Queries the Zotero API using the provided [`SearchItemsArgs`] parameters
    /// and returns matching items as MCP JSON content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if protocol-level failures occur. Backend
    /// failures from [`ZoteroClient::search_items`] are formatted as MCP JSON
    /// error responses.
    pub(crate) async fn zotero_search_items_impl(
        &self,
        args: SearchItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .search_items(
                    &args.query,
                    args.collection_key.as_ref(),
                    offset,
                    limit,
                )
                .await,
        ))
    }

    /// Handles Zotero citation-key search tool calls.
    ///
    /// Queries the Zotero API using the provided [`SearchByCitationKeyArgs`]
    /// parameters and returns matching items as MCP JSON content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if protocol-level failures occur. Backend
    /// failures from [`ZoteroClient::search_by_citation_key`] are formatted as
    /// MCP JSON error responses.
    async fn zotero_search_by_citation_key_impl(
        &self,
        args: SearchByCitationKeyArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.search_by_citation_key(&args.citekey).await))
    }

    /// Handles Zotero structured search tool calls.
    ///
    /// Executes a multi-condition query against the Zotero API using
    /// [`AdvancedSearchArgs`] and returns matching items as MCP JSON
    /// content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if protocol-level failures occur. Backend
    /// failures from [`ZoteroClient::advanced_search`] are formatted as MCP
    /// JSON error responses.
    async fn zotero_advanced_search_impl(
        &self,
        args: AdvancedSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .advanced_search(
                    args.conditions,
                    args.join_mode.unwrap_or_default(),
                    args.sort_by,
                    args.sort_direction.unwrap_or_default(),
                    offset,
                    limit,
                )
                .await,
        ))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ZoteroMcpServer, mcp::zotero::fixtures::*};

    mod connector_operations {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn connector_search_returns_matching_items() {
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

            let res = server
                .connector_search(Parameters(ConnectorSearchArgs {
                    query: "quantum".to_owned(),
                }))
                .await
                .expect("search succeeded");

            assert_eq!(res.is_error, Some(false));
        }
    }
}
