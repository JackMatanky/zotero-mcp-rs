//! MCP tool handlers for Zotero item full-text content.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::text_result,
    zotero::{ItemKey, ZoteroClient},
};

/// Arguments for the `fulltext` action of `zotero_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemFulltextArgs {
    /// Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
}

impl ZoteroMcpServer {
    /// Handles Zotero full-text retrieval tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_get_item_fulltext_impl(
        &self,
        args: GetItemFulltextArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(text_result(client.get_item_fulltext(&args.item_key).await))
    }
}
