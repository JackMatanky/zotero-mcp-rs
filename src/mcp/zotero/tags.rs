//! MCP tool handlers and argument models for Zotero tag administration.
//!
//! Covers `zotero_tags` / `zotero_tags_write` grouped-router actions: tag
//! listing, batch add/remove across items, renaming, and deletion.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, text_error, text_success},
    zotero::{ItemKey, TagName, ZoteroClient},
};

/// Arguments for `zotero_list_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ListTagsArgs {
    /// Maximum number of tags to return (default: 100).
    pub(crate) limit: Option<usize>,
}
/// Arguments for `zotero_rename_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RenameTagArgs {
    /// Existing tag name ([`TagName`]).
    pub(crate) old_tag: TagName,
    /// New tag name ([`TagName`]).
    pub(crate) new_tag: TagName,
}
/// Arguments for `zotero_delete_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteTagsArgs {
    /// Tag names ([`TagName`]) to delete from the library (up to 50).
    pub(crate) tags: Vec<TagName>,
}
/// Arguments for `zotero_batch_update_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BatchUpdateTagsArgs {
    /// List of item keys ([`ItemKey`]).
    pub(crate) item_keys: Vec<ItemKey>,
    /// Tags ([`TagName`]) to add.
    pub(crate) add_tags: Option<Vec<TagName>>,
    /// Tags ([`TagName`]) to remove.
    pub(crate) remove_tags: Option<Vec<TagName>>,
}

impl ZoteroMcpServer {
    /// Handles Zotero tag listing tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_list_tags_impl(
        &self,
        args: ListTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(100);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.list_tags(limit).await))
    }

    /// Handles Zotero tag rename tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_rename_tag_impl(
        &self,
        args: RenameTagArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let old_tag = args.old_tag;
        let new_tag = args.new_tag;
        match client.rename_tag(&old_tag, &new_tag).await {
            Ok(count) => {
                Ok(text_success(format!("Renamed tag on {count} items")))
            }
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero tag deletion tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_delete_tags_impl(
        &self,
        args: DeleteTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let tags = args.tags;
        match client.delete_tags(&tags).await {
            Ok(()) => Ok(text_success("Tags deleted")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero batch tag update tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_batch_update_tags_impl(
        &self,
        args: BatchUpdateTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let add = args.add_tags.unwrap_or_default();
        let rem = args.remove_tags.unwrap_or_default();
        match client.batch_update_tags(&args.item_keys, &add, &rem).await {
            Ok(count) => {
                Ok(text_success(format!("Batch updated tags on {count} items")))
            }
            Err(e) => Ok(text_error(&e)),
        }
    }
}
