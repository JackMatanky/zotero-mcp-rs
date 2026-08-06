//! MCP tool handler and argument model for checking Zotero Local API status.
//!
//! This module provides the `zotero_status` MCP tool and router to verify
//! Zotero API availability, version information, and connection state via
//! [`ZoteroClient::check_status`].
//!
//! # Main Types
//!
//! - [`EmptyArgs`] - Arguments for tools that take no parameters
//!
//! # Examples
//!
//! ```ignore
//! # use zotero_mcp::ZoteroMcpServer;
//! # use zotero_zotero::status::EmptyArgs;
//! # use rmcp::handler::server::wrapper::Parameters;
//! # async fn run(
//! #     server: ZoteroMcpServer,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let result = server.zotero_status(Parameters(EmptyArgs {})).await?;
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::ZoteroClient;

use crate::{ZoteroMcpServer, response::json_success};

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
    /// Checks Zotero Local API availability, version, and connectivity.
    ///
    /// Receives tool call parameters wrapped in [`Parameters<EmptyArgs>`] and
    /// delegates execution to
    /// [`zotero_status_impl`](ZoteroMcpServer::zotero_status_impl).
    ///
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
    /// Queries local Zotero status using [`ZoteroClient::check_status`] and
    /// formats the response as a JSON success object.
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
