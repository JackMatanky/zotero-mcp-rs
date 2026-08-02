//! MCP tool handlers and argument models for Zotero library search.
//!
//! Covers `zotero_search` grouped-router actions: item search, tag search,
//! citation-key search, structured advanced search, duplicate detection, and
//! library coverage analysis.
//!
//! Tag-search arguments are defined in [`super::tags`] because `zotero_tags`
//! and `zotero_search` share that action.

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
    /// Search query matched against title, creator, year, or fulltext.
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

/// Arguments for `zotero_find_duplicates`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key ([`CollectionKey`]) to scope duplicate search.
    collection_key: Option<CollectionKey>,
}

/// Arguments for `zotero_library_coverage`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct LibraryCoverageArgs {
    /// Optional collection key ([`CollectionKey`]) to scope coverage analysis.
    collection_key: Option<CollectionKey>,
    /// 0-based offset into the item set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to analyze (default: 100, max: 500).
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
    Duplicates(FindDuplicatesArgs),
    Coverage(LibraryCoverageArgs),
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

    #[tool(
        name = "zotero_search_items",
        description = "Search items by title, creator, year, or fulltext query",
        annotations(
            title = "Search Items",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_items(
        &self,
        Parameters(args): Parameters<SearchItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_items_impl(args).await
    }

    #[tool(
        name = "zotero_search_by_tag",
        description = "Search Zotero items by tag string",
        annotations(
            title = "Search Items by Tag",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_tag(
        &self,
        Parameters(args): Parameters<SearchByTagArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_by_tag_impl(args).await
    }

    #[tool(
        name = "zotero_search_by_citation_key",
        description = "Search Zotero items by citation key string",
        annotations(
            title = "Search Items by Citation Key",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_citation_key(
        &self,
        Parameters(args): Parameters<SearchByCitationKeyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_by_citation_key_impl(args).await
    }

    #[tool(
        name = "zotero_advanced_search",
        description = "Advanced multi-condition structured search over item \
                       fields",
        annotations(
            title = "Advanced Item Search",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_advanced_search(
        &self,
        Parameters(args): Parameters<AdvancedSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_advanced_search_impl(args).await
    }

    #[tool(
        name = "zotero_find_duplicates",
        description = "Finds potential duplicate items in library or \
                       collection by matching title or DOI",
        annotations(
            title = "Find Duplicate Items",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_find_duplicates(
        &self,
        Parameters(args): Parameters<FindDuplicatesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_find_duplicates_impl(args).await
    }

    #[tool(
        name = "zotero_library_coverage",
        description = "Analyze library or collection statistics for PDF, DOI, \
                       and note coverage",
        annotations(
            title = "Library Coverage Report",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_library_coverage(
        &self,
        Parameters(args): Parameters<LibraryCoverageArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_library_coverage_impl(args).await
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

    /// Handles Zotero duplicate detection tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_find_duplicates_impl(
        &self,
        args: FindDuplicatesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.find_duplicates(args.collection_key.as_ref()).await,
        ))
    }

    /// Handles Zotero library coverage analysis tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_library_coverage_impl(
        &self,
        args: LibraryCoverageArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(100).min(500);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .get_library_coverage(
                    args.collection_key.as_ref(),
                    offset,
                    limit,
                )
                .await,
        ))
    }
}
