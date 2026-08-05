//! MCP tool handlers for detecting duplicate items in a Zotero library.
//!
//! This module provides the execution logic for scanning items across the
//! library or within a specific collection, delegating duplicate detection
//! algorithms to [`ZoteroClient::find_duplicates`].
//!
//! # Main Types
//! - [`FindDuplicatesArgs`] - Arguments for the `duplicates` action
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::ZoteroMcpServer;
//! # use zotero_mcp_rs::mcp::zotero::duplicates::FindDuplicatesArgs;
//! # async fn run(server: ZoteroMcpServer) -> Result<(), Box<dyn std::error::Error>> {
//! let args = serde_json::from_value(serde_json::json!({
//!     "collection_key": "COLL1234"
//! }))?;
//! let result = server.zotero_find_duplicates_impl(args).await?;
//! # Ok(())
//! # }
//! ```
use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::json_result,
    zotero::{CollectionKey, ZoteroClient},
};

/// Arguments for the `duplicates` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key ([`CollectionKey`]) to scope duplicate search.
    collection_key: Option<CollectionKey>,
}

impl ZoteroMcpServer {
    /// Handles Zotero duplicate detection tool calls.
    ///
    /// Scans for potential duplicate items in the library or optional
    /// collection specified by `args` using [`ZoteroClient`].
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
