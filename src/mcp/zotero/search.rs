//! MCP tool handlers and argument models for Zotero library search.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::tags::SearchByTagArgs;
use crate::{
    ZoteroMcpServer,
    mcp::json_result,
    zotero::{
        CitationKey, CollectionKey, JoinMode, SearchCondition, SortDirection,
        SortField, ZoteroClient,
    },
};

/// Arguments for `zotero_search_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query matched against title, creator, year, or full-text
    /// content.
    query: String,
    /// Optional collection key ([`CollectionKey`]) to search within.
    collection_key: Option<CollectionKey>,
    /// 0-based offset into the full result set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    limit: Option<usize>,
}
impl SearchItemsArgs {
    pub(crate) fn for_connector(query: String) -> Self {
        Self {
            query,
            collection_key: None,
            start: None,
            limit: Some(20),
        }
    }
}

/// Arguments for `zotero_search_by_citation_key`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByCitationKeyArgs {
    /// Citation key ([`CitationKey`]) to match.
    citekey: CitationKey,
}
/// Arguments for `zotero_advanced_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions ([`SearchCondition`]).
    conditions: Vec<SearchCondition>,
    /// `"all"` (AND, default) or `"any"` (OR).
    join_mode: Option<JoinMode>,
    /// Sort field: `"dateAdded"`, `"dateModified"`, `"title"`, `"date"`, or
    /// `"creator"`.
    sort_by: Option<SortField>,
    /// Sort direction: `"asc"` or `"desc"` (default: `"asc"`).
    sort_direction: Option<SortDirection>,
    /// 0-based offset into the full result set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroSearchCommand {
    Items(SearchItemsArgs),
    Tag(SearchByTagArgs),
    CitationKey(SearchByCitationKeyArgs),
    Advanced(AdvancedSearchArgs),
    Duplicates(crate::mcp::zotero::duplicates::FindDuplicatesArgs),
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
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
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
}

impl ZoteroMcpServer {
    /// Handles Zotero item search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
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
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_search_by_citation_key_impl(
        &self,
        args: SearchByCitationKeyArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.search_by_citation_key(&args.citekey).await))
    }

    /// Handles Zotero structured search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
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
