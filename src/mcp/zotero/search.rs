//! MCP tool handlers and argument models for Zotero library search.
//!
//! Covers `zotero_search` grouped-router actions: item search, tag search,
//! citation-key search, structured advanced search, duplicate detection, and
//! library coverage analysis.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::json_result,
    zotero::{
        CitationKey, CollectionKey, JoinMode, SearchCondition, SortDirection,
        SortField, TagName, ZoteroClient,
    },
};

/// Arguments for `zotero_search_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query matched against title, creator, year, or fulltext.
    pub(crate) query: String,
    /// Optional collection key ([`CollectionKey`]) to search within.
    pub(crate) collection_key: Option<CollectionKey>,
    /// 0-based offset into the full result set (default: 0).
    pub(crate) start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}
/// Arguments for `zotero_find_duplicates`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key ([`CollectionKey`]) to scope duplicate search.
    pub(crate) collection_key: Option<CollectionKey>,
}
/// Arguments for `zotero_search_by_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByTagArgs {
    /// Tag name ([`TagName`]) to search for.
    pub(crate) tag: TagName,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}
/// Arguments for `zotero_search_by_citation_key`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByCitationKeyArgs {
    /// Citation key ([`CitationKey`]) to match.
    pub(crate) citekey: CitationKey,
}
/// Arguments for `zotero_advanced_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions ([`SearchCondition`]).
    pub(crate) conditions: Vec<SearchCondition>,
    /// `"all"` (AND, default) or `"any"` (OR).
    pub(crate) join_mode: Option<JoinMode>,
    /// Sort field: `"dateAdded"`, `"dateModified"`, `"title"`, `"date"`, or
    /// `"creator"`.
    pub(crate) sort_by: Option<SortField>,
    /// Sort direction: `"asc"` or `"desc"` (default: `"asc"`).
    pub(crate) sort_direction: Option<SortDirection>,
    /// 0-based offset into the full result set (default: 0).
    pub(crate) start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}
/// Arguments for `zotero_library_coverage`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct LibraryCoverageArgs {
    /// Optional collection key ([`CollectionKey`]) to scope coverage analysis.
    pub(crate) collection_key: Option<CollectionKey>,
    /// 0-based offset into the item set (default: 0).
    pub(crate) start: Option<usize>,
    /// Maximum number of items to analyze (default: 100, max: 500).
    pub(crate) limit: Option<usize>,
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

    /// Handles Zotero duplicate detection tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_find_duplicates_impl(
        &self,
        args: FindDuplicatesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.find_duplicates(args.collection_key.as_ref()).await,
        ))
    }

    /// Handles Zotero tag search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_tag_impl(
        &self,
        args: SearchByTagArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.search_by_tag(&args.tag, limit).await))
    }

    /// Handles Zotero citation-key search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_citation_key_impl(
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
    pub(crate) async fn zotero_advanced_search_impl(
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

    /// Handles Zotero library coverage analysis tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_library_coverage_impl(
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
