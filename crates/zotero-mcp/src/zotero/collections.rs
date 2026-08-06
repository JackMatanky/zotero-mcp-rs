//! MCP tool handlers and argument models for Zotero collection operations.
//!
//! Exposes the `zotero_collections` and `zotero_collections_write` MCP tool
//! routers for collection item listing, name search, creation, item membership
//! management, update, and deletion.
//!
//! # Main Types
//!
//! - [`ZoteroCollectionsCommand`] - Grouped-router command for read-only
//!   collection actions
//! - [`ZoteroCollectionsWriteCommand`] - Grouped-router command for write
//!   collection actions
//! - [`GetCollectionItemsArgs`] - Arguments for listing items in a collection
//! - [`SearchCollectionsArgs`] - Arguments for searching collections by name
//! - [`CreateCollectionArgs`] - Arguments for creating a new collection
//! - [`ManageCollectionsArgs`] - Arguments for adding or removing collection
//!   items
//! - [`UpdateCollectionArgs`] - Arguments for updating or moving a collection
//! - [`DeleteCollectionArgs`] - Arguments for deleting a collection
//!
//! # Examples
//!
//! ```ignore
//! # use rmcp::handler::server::wrapper::Parameters;
//! # use zotero_mcp::ZoteroMcpServer;
//! # use zotero_zotero::collections::{
//! #     ZoteroCollectionsCommand,
//! #     SearchCollectionsArgs,
//! # };
//! # async fn run(
//! #     server: ZoteroMcpServer,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let args =
//!     Parameters(ZoteroCollectionsCommand::Search(SearchCollectionsArgs {
//!         query: "machine learning".to_string(),
//!     }));
//! let result = server.zotero_collections(args).await?;
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::{CollectionItemAction, CollectionKey, ItemKey, ZoteroClient};

use crate::{
    ZoteroMcpServer,
    response::{json_result, text_error, text_success},
};

/// Mirrors `zotero_api::CollectionParent`'s string sentinel rule: an empty
/// string or the literal `"false"` clears the parent (top level); any other
/// string sets it. `None` (field omitted) is handled by the caller as "leave
/// the parent unchanged".
fn parse_collection_parent(raw: String) -> zotero_api::CollectionParent {
    if raw.is_empty() || raw == "false" {
        zotero_api::CollectionParent::TopLevel
    } else {
        zotero_api::CollectionParent::Parent(raw.into())
    }
}

/// Arguments for the `items` action of `zotero_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCollectionItemsArgs {
    /// Zotero collection key ([`CollectionKey`]).
    collection_key: String,
}

/// Arguments for the `search` action of `zotero_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchCollectionsArgs {
    /// Search query matching collection names.
    query: String,
}

/// Arguments for the `create` action of `zotero_collections_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateCollectionArgs {
    /// Name of the collection to create.
    name: String,
    /// Optional parent collection key ([`CollectionKey`]).
    parent_key: Option<String>,
}

/// Arguments for the `manage` action of `zotero_collections_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ManageCollectionsArgs {
    /// Zotero collection key ([`CollectionKey`]).
    collection_key: String,
    /// List of item keys ([`ItemKey`]) to add or remove.
    item_keys: Vec<String>,
    /// Set to `true` to remove items instead of adding them.
    remove: Option<bool>,
}

/// Arguments for the `update` action of `zotero_collections_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateCollectionArgs {
    /// Zotero collection key ([`CollectionKey`]).
    collection_key: String,
    /// New name for the collection.
    name: Option<String>,
    /// New parent collection. Omit to keep current parent; pass `false` or an
    /// empty string to move the collection to the top level.
    parent_key: Option<String>,
}

/// Arguments for the `delete` action of `zotero_collections_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteCollectionArgs {
    /// Key of the collection ([`CollectionKey`]) to permanently delete.
    collection_key: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Read commands dispatched by the `zotero_collections` MCP tool router.
pub(crate) enum ZoteroCollectionsCommand {
    /// List items in a collection.
    Items(GetCollectionItemsArgs),
    /// Search collections by name or query.
    Search(SearchCollectionsArgs),
    /// List items not filed in any collection.
    Unfiled(crate::zotero::items::GetUnfiledItemsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Write commands dispatched by the `zotero_collections` MCP tool router.
pub(crate) enum ZoteroCollectionsWriteCommand {
    /// Create a new collection.
    Create(CreateCollectionArgs),
    /// Move items between collections.
    Manage(ManageCollectionsArgs),
    /// Rename or update a collection.
    Update(UpdateCollectionArgs),
    /// Permanently delete a collection.
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
    /// Dispatches collection read requests.
    ///
    /// Accepts a [`Parameters<ZoteroCollectionsCommand>`] containing the
    /// specific action and parameters, routing it to internal collection
    /// read handlers.
    ///
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
    /// Dispatches collection modification requests.
    ///
    /// Accepts a [`Parameters<ZoteroCollectionsWriteCommand>`] containing the
    /// specific action and parameters, routing it to internal collection
    /// write handlers.
    ///
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
    /// Queries the Zotero API using [`GetCollectionItemsArgs`] parameters and
    /// returns items belonging to the specified collection as MCP JSON
    /// content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::get_collection_items`] are returned as MCP
    /// error content.
    async fn zotero_get_collection_items_impl(
        &self,
        args: GetCollectionItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let collection_key: CollectionKey = args.collection_key.into();
        Ok(json_result(client.get_collection_items(&collection_key).await))
    }

    /// Handles Zotero collection search tool calls.
    ///
    /// Queries the Zotero API using [`SearchCollectionsArgs`] parameters and
    /// returns matching collection keys and metadata as MCP JSON content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::search_collections`] are returned as MCP
    /// error content.
    async fn zotero_search_collections_impl(
        &self,
        args: SearchCollectionsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.search_collections(&args.query).await))
    }

    /// Handles Zotero collection creation tool calls.
    ///
    /// Creates a new collection using [`CreateCollectionArgs`] parameters and
    /// returns the created collection data as MCP JSON content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::create_collection`] are returned as MCP
    /// error content.
    async fn zotero_create_collection_impl(
        &self,
        args: CreateCollectionArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .create_collection(
                    &args.name,
                    args.parent_key.map(CollectionKey::from).as_ref(),
                )
                .await,
        ))
    }

    /// Handles Zotero collection item membership tool calls.
    ///
    /// Adds or removes items from a collection using [`ManageCollectionsArgs`]
    /// parameters.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::manage_collection_items`] are returned as
    /// MCP error content.
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
        let collection_key: CollectionKey = args.collection_key.into();
        let item_keys: Vec<ItemKey> =
            args.item_keys.into_iter().map(Into::into).collect();
        match client
            .manage_collection_items(&collection_key, &item_keys, action)
            .await
        {
            Ok(()) => Ok(text_success("Collection items updated successfully")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero collection rename/move tool calls.
    ///
    /// Renames or repositions a collection using [`UpdateCollectionArgs`]
    /// parameters.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::update_collection`] are returned as MCP
    /// error content.
    async fn zotero_update_collection_impl(
        &self,
        args: UpdateCollectionArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .update_collection(
                    &CollectionKey::from(args.collection_key),
                    args.name.as_deref(),
                    args.parent_key.map(parse_collection_parent).as_ref(),
                )
                .await,
        ))
    }

    /// Handles Zotero collection permanent deletion tool calls.
    ///
    /// Permanently deletes a collection using [`DeleteCollectionArgs`]
    /// parameters.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::delete_collection`] are returned as MCP
    /// error content.
    async fn zotero_delete_collection_impl(
        &self,
        args: DeleteCollectionArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let collection_key: CollectionKey = args.collection_key.into();
        match client.delete_collection(&collection_key).await {
            Ok(()) => Ok(text_success("Collection permanently deleted")),
            Err(e) => Ok(text_error(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ZoteroMcpServer, zotero::fixtures::*};

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
