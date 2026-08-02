//! MCP tool handlers for Zotero attachment items.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::json_result,
    zotero::{ItemKey, ZoteroClient},
};

/// Arguments for `zotero_attach_file`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AttachFileArgs {
    /// Key of the parent item ([`ItemKey`]).
    parent_item_key: ItemKey,
    /// Display title for the attachment.
    title: String,
    /// File path or URL.
    path_or_url: String,
    /// Optional content type (default: `"application/pdf"`).
    content_type: Option<String>,
}

impl ZoteroMcpServer {
    /// Handles Zotero linked-file attachment tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_attach_file_impl(
        &self,
        args: AttachFileArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .attach_file_link(
                    &args.parent_item_key,
                    &args.title,
                    &args.path_or_url,
                    args.content_type.as_deref(),
                )
                .await,
        ))
    }
}
