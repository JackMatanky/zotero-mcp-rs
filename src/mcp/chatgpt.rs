//! MCP tool handlers, argument models, and unit tests for `ChatGPT` connector compatibility tools.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::zotero::{GetItemMetadataArgs, SearchItemsArgs},
};

// --- Argument Schemas ---

/// Arguments for `search` (`ChatGPT` connector compatibility).
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchArgs {
    /// Search query
    pub(crate) query: String,
}

/// Arguments for `fetch` (`ChatGPT` connector compatibility).
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FetchArgs {
    /// Zotero item key or ID to fetch
    pub(crate) id: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
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

    pub(crate) async fn chatgpt_fetch_impl(
        &self,
        args: FetchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_metadata_impl(GetItemMetadataArgs {
            item_key: args.id,
            format: Some("json".to_owned()),
        })
        .await
    }
}
