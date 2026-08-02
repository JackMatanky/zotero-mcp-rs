//! MCP tool handlers and argument models for Zotero collection operations.
//!
//! Covers `zotero_collections` / `zotero_collections_write` grouped-router
//! actions: collection item listing, name search, creation, item membership
//! management, rename/move, deletion, plus compatible unfiled-item dispatch.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
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
    collection_key: CollectionKey,
}
/// Arguments for `zotero_search_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchCollectionsArgs {
    /// Search query matching collection names.
    query: String,
}
/// Arguments for `zotero_create_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateCollectionArgs {
    /// Name of the collection to create.
    name: String,
    /// Optional parent collection key ([`CollectionKey`]).
    parent_key: Option<CollectionKey>,
}
/// Arguments for `zotero_manage_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ManageCollectionsArgs {
    /// Zotero collection key ([`CollectionKey`]).
    collection_key: CollectionKey,
    /// List of item keys ([`ItemKey`]) to add or remove.
    item_keys: Vec<ItemKey>,
    /// Set to `true` to remove items instead of adding them.
    remove: Option<bool>,
}
/// Arguments for `zotero_update_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateCollectionArgs {
    /// Zotero collection key ([`CollectionKey`]).
    collection_key: CollectionKey,
    /// New name for the collection.
    name: Option<String>,
    /// New parent collection. Omit to keep current parent; pass `false` or an
    /// empty string to move the collection to the top level.
    parent_key: Option<CollectionParent>,
}
/// Arguments for `zotero_delete_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteCollectionArgs {
    /// Key of the collection ([`CollectionKey`]) to permanently delete.
    collection_key: CollectionKey,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroCollectionsCommand {
    Items(GetCollectionItemsArgs),
    Search(SearchCollectionsArgs),
    Unfiled(crate::mcp::zotero::items::GetUnfiledItemsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroCollectionsWriteCommand {
    Create(CreateCollectionArgs),
    Manage(ManageCollectionsArgs),
    Update(UpdateCollectionArgs),
    Delete(DeleteCollectionArgs),
}

#[tool_router(router = collections_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_collections",
        description = "Grouped Zotero collection read router. action: items, \
                       search, unfiled",
        annotations(
            title = "Read Zotero Collections",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_collections(
        &self,
        Parameters(args): Parameters<ZoteroCollectionsCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroCollectionsCommand::Items(args) => {
                self.zotero_get_collection_items_impl(args).await
            }
            ZoteroCollectionsCommand::Search(args) => {
                self.zotero_search_collections_impl(args).await
            }
            ZoteroCollectionsCommand::Unfiled(args) => {
                self.zotero_get_unfiled_items_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_collections_write",
        description = "Grouped Zotero collection write router. action: \
                       create, manage, update, delete",
        annotations(
            title = "Write Zotero Collections",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_collections_write(
        &self,
        Parameters(args): Parameters<ZoteroCollectionsWriteCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroCollectionsWriteCommand::Create(args) => {
                self.zotero_create_collection_impl(args).await
            }
            ZoteroCollectionsWriteCommand::Manage(args) => {
                self.zotero_manage_collections_impl(args).await
            }
            ZoteroCollectionsWriteCommand::Update(args) => {
                self.zotero_update_collection_impl(args).await
            }
            ZoteroCollectionsWriteCommand::Delete(args) => {
                self.zotero_delete_collection_impl(args).await
            }
        }
    }
}

impl ZoteroMcpServer {
    /// Handles Zotero collection item listing tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_collection_items_impl(
        &self,
        args: GetCollectionItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_collection_items(&args.collection_key).await))
    }

    /// Handles Zotero collection search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_search_collections_impl(
        &self,
        args: SearchCollectionsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.search_collections(&args.query).await))
    }

    /// Handles Zotero collection creation tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_create_collection_impl(
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

    /// Handles Zotero collection item membership tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_manage_collections_impl(
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

    /// Handles Zotero collection rename/move tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_update_collection_impl(
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

    /// Handles Zotero collection permanent deletion tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_delete_collection_impl(
        &self,
        args: DeleteCollectionArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.delete_collection(&args.collection_key).await {
            Ok(()) => Ok(text_success("Collection permanently deleted")),
            Err(e) => Ok(text_error(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ZoteroMcpServer, mcp::zotero::fixtures::*};

    mod write_operations {

        use super::*;

        #[tokio::test]
        async fn delete_collection_removes_collection() {
            // Arrange
            let collection = json!({
                "key": "COL1",
                "version": 1,
                "data": { "key": "COL1", "name": "Old Collection", "parentCollection": false }
            });
            let base = mock_server(vec![
                http_response("200 OK", &collection.to_string()),
                http_response("204 No Content", ""),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_delete_collection_impl(DeleteCollectionArgs {
                    collection_key: "COL1".into(),
                })
                .await
                .expect("delete collection ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
        #[tokio::test]
        async fn update_collection_renames_collection() {
            // Arrange
            let current = json!({
                "key": "COL1",
                "version": 3,
                "data": { "key": "COL1", "name": "Old Name", "parentCollection": false }
            });
            let updated = json!({
                "key": "COL1",
                "version": 4,
                "data": { "key": "COL1", "name": "New Name", "parentCollection": false }
            });
            let base = mock_server(vec![
                http_response("200 OK", &current.to_string()),
                http_response("200 OK", &updated.to_string()),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_update_collection_impl(UpdateCollectionArgs {
                    collection_key: "COL1".into(),
                    name: Some("New Name".to_owned()),
                    parent_key: None,
                })
                .await
                .expect("update collection ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }
}
