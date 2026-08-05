//! MCP tool handlers for retrieving full-text content of Zotero items.
//!
//! This module provides the execution logic for extracting full text from
//! library attachments, delegating backend operations to
//! [`ZoteroClient::get_item_fulltext`].
//!
//! # Main Types
//! - [`GetItemFulltextArgs`] - Arguments for the `fulltext` action
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::ZoteroMcpServer;
//! # use zotero_mcp_rs::mcp::zotero::fulltext::GetItemFulltextArgs;
//! # async fn run(server: ZoteroMcpServer) -> Result<(), Box<dyn std::error::Error>> {
//! let args = serde_json::from_value(serde_json::json!({
//!     "item_key": "ABC12345"
//! }))?;
//! let result = server.zotero_get_item_fulltext_impl(args).await?;
//! # Ok(())
//! # }
//! ```
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
    /// Unique Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
}

impl ZoteroMcpServer {
    /// Handles Zotero full-text retrieval tool calls.
    ///
    /// Extracts full-text content for the item specified by `args.item_key`
    /// using the underlying [`ZoteroClient`].
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
