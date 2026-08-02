//! MCP tool handlers and argument models for Zotero item relations.
//!
//! Covers `zotero_relations` / `zotero_relations_write` grouped-router
//! actions: listing an item's `dc:relation` links, and bidirectionally
//! linking or unlinking two items.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, text_error, text_success},
    zotero::{ItemKey, ZoteroClient},
};

/// Arguments for `zotero_get_related_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRelatedItemsArgs {
    /// Zotero item key ([`ItemKey`]) whose related items to list.
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_add_item_relation`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddItemRelationArgs {
    /// Zotero item key ([`ItemKey`]) of the first item to link (bidirectional,
    /// order-independent).
    pub(crate) item_key: ItemKey,
    /// Zotero item key ([`ItemKey`]) of the second item to link
    /// (bidirectional, order-independent).
    pub(crate) related_item_key: ItemKey,
}
/// Arguments for `zotero_remove_item_relation`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RemoveItemRelationArgs {
    /// Zotero item key ([`ItemKey`]) of the first item to unlink
    /// (bidirectional, order-independent).
    pub(crate) item_key: ItemKey,
    /// Zotero item key ([`ItemKey`]) of the second item to unlink
    /// (bidirectional, order-independent).
    pub(crate) related_item_key: ItemKey,
}

impl ZoteroMcpServer {
    /// Handles Zotero related-item listing tool calls, returning the items
    /// linked to `item_key` as `RelatedItem` JSON.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_related_items_impl(
        &self,
        args: GetRelatedItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_related_items(&args.item_key).await))
    }

    /// Handles Zotero related-item linking tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_add_item_relation_impl(
        &self,
        args: AddItemRelationArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client
            .add_item_relation(&args.item_key, &args.related_item_key)
            .await
        {
            Ok(()) => Ok(text_success("Item relation added")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero related-item unlinking tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_remove_item_relation_impl(
        &self,
        args: RemoveItemRelationArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client
            .remove_item_relation(&args.item_key, &args.related_item_key)
            .await
        {
            Ok(()) => Ok(text_success("Item relation removed")),
            Err(e) => Ok(text_error(&e)),
        }
    }
}
