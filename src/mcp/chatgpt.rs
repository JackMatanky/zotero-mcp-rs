//! MCP tool handlers and argument models for `ChatGPT` connector compatibility.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::zotero::{GetItemMetadataArgs, SearchItemsArgs},
};

// --- Argument Schemas ---

/// Arguments for the `ChatGPT` compatibility `search` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchArgs {
    /// Search query string.
    pub(crate) query: String,
}

/// Arguments for the `ChatGPT` compatibility `fetch` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FetchArgs {
    /// Zotero item key or identifier to fetch.
    pub(crate) id: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    /// Executes a `ChatGPT` connector compatibility search using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if item search fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn chatgpt_search_impl(
        &self,
        args: SearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_items_impl(SearchItemsArgs {
            query: args.query,
            collection_key: None,
            limit: Some(20),
        })
        .await
    }

    /// Fetches Zotero item metadata for `ChatGPT` connector compatibility using
    /// `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if item retrieval fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn chatgpt_fetch_impl(
        &self,
        args: FetchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_metadata_impl(GetItemMetadataArgs {
            item_key: args.id.into(),
            format: None,
        })
        .await
    }
}
