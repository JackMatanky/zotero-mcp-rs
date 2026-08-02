//! MCP tool handlers and argument models for Zotero collection operations.
//!
//! Covers `zotero_collections` / `zotero_collections_write` grouped-router
//! actions: collection item listing, name search, unfiled items, creation,
//! item membership management, rename/move, and deletion.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, text_error, text_success},
    zotero::{
        CollectionItemAction, CollectionKey, CollectionParent, ItemKey,
        ZoteroClient,
    },
};

/// Arguments for `zotero_get_collection_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCollectionItemsArgs {
    /// Zotero collection key ([`CollectionKey`]).
    pub(crate) collection_key: CollectionKey,
}
/// Arguments for `zotero_create_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateCollectionArgs {
    /// Name of the collection to create.
    pub(crate) name: String,
    /// Optional parent collection key ([`CollectionKey`]).
    pub(crate) parent_key: Option<CollectionKey>,
}
/// Arguments for `zotero_search_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchCollectionsArgs {
    /// Search query matching collection names.
    pub(crate) query: String,
}
/// Arguments for `zotero_manage_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ManageCollectionsArgs {
    /// Zotero collection key ([`CollectionKey`]).
    pub(crate) collection_key: CollectionKey,
    /// List of item keys ([`ItemKey`]) to add or remove.
    pub(crate) item_keys: Vec<ItemKey>,
    /// Set to `true` to remove items instead of adding them.
    pub(crate) remove: Option<bool>,
}
/// Arguments for `zotero_delete_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteCollectionArgs {
    /// Key of the collection ([`CollectionKey`]) to permanently delete.
    pub(crate) collection_key: CollectionKey,
}
/// Arguments for `zotero_update_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateCollectionArgs {
    /// Zotero collection key ([`CollectionKey`]).
    pub(crate) collection_key: CollectionKey,
    /// New name for the collection.
    pub(crate) name: Option<String>,
    /// New parent collection. Omit to keep current parent; pass `false` or an
    /// empty string to move the collection to the top level.
    pub(crate) parent_key: Option<CollectionParent>,
}
/// Arguments for `zotero_get_unfiled_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetUnfiledItemsArgs {
    /// Maximum number of items to return (default: 50).
    pub(crate) limit: Option<usize>,
}

impl ZoteroMcpServer {
    /// Handles Zotero collection item listing tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_collection_items_impl(
        &self,
        args: GetCollectionItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_collection_items(&args.collection_key).await))
    }

    /// Handles Zotero collection creation tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_collection_impl(
        &self,
        args: CreateCollectionArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .create_collection(&args.name, args.parent_key.as_ref())
                .await,
        ))
    }

    /// Handles Zotero collection search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_collections_impl(
        &self,
        args: SearchCollectionsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.search_collections(&args.query).await))
    }

    /// Handles Zotero collection item membership tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_manage_collections_impl(
        &self,
        args: ManageCollectionsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let action = if args.remove.unwrap_or(false) {
            CollectionItemAction::Remove
        } else {
            CollectionItemAction::Add
        };
        match client
            .manage_collection_items(
                &args.collection_key,
                &args.item_keys,
                action,
            )
            .await
        {
            Ok(()) => Ok(text_success("Collection items updated successfully")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero collection permanent deletion tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_delete_collection_impl(
        &self,
        args: DeleteCollectionArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.delete_collection(&args.collection_key).await {
            Ok(()) => Ok(text_success("Collection permanently deleted")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero collection rename/move tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_update_collection_impl(
        &self,
        args: UpdateCollectionArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .update_collection(
                    &args.collection_key,
                    args.name.as_deref(),
                    args.parent_key.as_ref(),
                )
                .await,
        ))
    }

    /// Handles Zotero unfiled items listing tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_unfiled_items_impl(
        &self,
        args: GetUnfiledItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(50);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_unfiled_items(limit).await))
    }
}
