//! MCP tool handler and argument model for Zotero Local API status.
//!
//! Main types:
//! - [`EmptyArgs`] - Arguments for tools that take no parameters

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, mcp::json_success, zotero::ZoteroClient};

/// Arguments for tools that take no parameters.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct EmptyArgs {}

#[tool_router(router = status_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_status",
        description = "Check Zotero Local API availability, version, and \
                       connectivity",
        annotations(
            title = "Check Zotero Connection",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_status(
        &self,
        Parameters(_args): Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_status_impl().await
    }
}

impl ZoteroMcpServer {
    /// Handles Zotero Local API status tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_status_impl(
        &self,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let status = client.check_status().await;
        Ok(json_success(&status))
    }
}
