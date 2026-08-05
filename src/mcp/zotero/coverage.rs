//! MCP tool handlers for calculating coverage metrics across a Zotero library.
//!
//! This module provides the execution logic for analyzing attachment and
//! metadata completeness, delegating calculations to
//! [`ZoteroClient::get_library_coverage`].
//!
//! # Main Types
//! - [`LibraryCoverageArgs`] - Arguments for the `coverage` action
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::ZoteroMcpServer;
//! # use zotero_mcp_rs::mcp::zotero::coverage::LibraryCoverageArgs;
//! # async fn run(server: ZoteroMcpServer) -> Result<(), Box<dyn std::error::Error>> {
//! let args = serde_json::from_value(serde_json::json!({
//!     "collection_key": null,
//!     "start": 0,
//!     "limit": 50
//! }))?;
//! let result = server.zotero_library_coverage_impl(args).await?;
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

/// Arguments for the `coverage` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct LibraryCoverageArgs {
    /// Optional collection key ([`CollectionKey`]) to scope coverage analysis.
    collection_key: Option<CollectionKey>,
    /// Zero-based offset into the item set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to analyze (default: 100, max: 500).
    limit: Option<usize>,
}

impl ZoteroMcpServer {
    /// Handles Zotero library coverage analysis tool calls.
    ///
    /// Analyzes library coverage metrics for the requested range using
    /// [`ZoteroClient`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_library_coverage_impl(
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
