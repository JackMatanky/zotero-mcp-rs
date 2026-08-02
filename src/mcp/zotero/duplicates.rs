//! MCP tool handlers for Zotero duplicate detection.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::json_result,
    zotero::{CollectionKey, ZoteroClient},
};

/// Arguments for `zotero_find_duplicates`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key ([`CollectionKey`]) to scope duplicate search.
    collection_key: Option<CollectionKey>,
}

impl ZoteroMcpServer {
    /// Handles Zotero duplicate detection tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_find_duplicates_impl(
        &self,
        args: FindDuplicatesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.find_duplicates(args.collection_key.as_ref()).await,
        ))
    }
}
