//! MCP tool handlers and argument models for core Zotero item operations.
//!
//! Handles `zotero_items` (read-only) and `zotero_items_write` (mutation)
//! grouped-router tool calls for item lifecycle management. Converts incoming
//! MCP tool parameters into calls on [`ZoteroClient`] for retrieving, creating,
//! updating, trashing, restoring, and deleting Zotero items, with compatible
//! dispatch to metadata, full-text, and attachment modules.
//!
//! # Main Types
//!
//! - [`ZoteroItemsCommand`]: Grouped-router command for read-only item actions.
//! - [`ZoteroItemsWriteCommand`]: Grouped-router command for write item
//!   actions.
//! - [`GetRecentArgs`]: Arguments for retrieving recently added or modified
//!   items.
//! - [`GetItemArgs`]: Arguments for retrieving a single item by key.
//! - [`GetUnfiledItemsArgs`]: Arguments for listing items not assigned to any
//!   collection.
//! - [`GetItemChildrenArgs`]: Arguments for listing child items (notes,
//!   attachments).
//! - [`UpdateItemArgs`]: Arguments for updating fields on an existing item.
//! - [`DeleteItemArgs`]: Arguments for permanently deleting an item.
//! - [`TrashItemArgs`]: Arguments for trashing or restoring an item.
//!
//! # Examples
//!
//! ```no_run
//! # use rmcp::handler::server::wrapper::Parameters;
//! # use zotero_mcp_rs::{ZoteroMcpServer, state::AppState};
//! # use zotero_mcp_rs::mcp::zotero::items::{ZoteroItemsCommand, GetRecentArgs};
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let server = ZoteroMcpServer::new(AppState::from_env());
//! let args = ZoteroItemsCommand::Recent(GetRecentArgs { limit: Some(10) });
//! let result = server.zotero_items(Parameters(args)).await?;
//! # Ok(())
//! # }
//! ```
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, text_error, text_success},
    zotero::{ItemKey, TrashAction, ZoteroClient},
};

/// Arguments for the `recent` action of `zotero_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRecentArgs {
    /// Maximum number of items to return (default: 10, max: 100).
    limit: Option<usize>,
}
/// Arguments for the `get` action of `zotero_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemArgs {
    /// Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
}
/// Arguments for the `unfiled` action of `zotero_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetUnfiledItemsArgs {
    /// Maximum number of items to return (default: 50).
    limit: Option<usize>,
}
/// Arguments for the `children` action of `zotero_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemChildrenArgs {
    /// Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
}
/// Arguments for the `update` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateItemArgs {
    /// Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
    /// JSON object containing fields to update.
    fields: serde_json::Value,
}
/// Arguments for the `delete` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteItemArgs {
    /// Key of the item ([`ItemKey`]) to permanently delete.
    item_key: ItemKey,
}
/// Arguments for the `trash` and `restore` actions of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct TrashItemArgs {
    /// Key of the item ([`ItemKey`]) to move to or restore from trash.
    item_key: ItemKey,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Read commands dispatched by the `zotero_items` MCP tool router.
pub(crate) enum ZoteroItemsCommand {
    /// Fetch recently added or modified items.
    Recent(GetRecentArgs),
    /// Get a single item by key.
    Get(GetItemArgs),
    /// Retrieve metadata for an item in various formats.
    Metadata(crate::mcp::zotero::metadata::GetItemMetadataArgs),
    /// List child items (notes, attachments) of an item.
    Children(GetItemChildrenArgs),
    /// Retrieve full-text content extracted from an item's attachments.
    Fulltext(crate::mcp::zotero::fulltext::GetItemFulltextArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Write commands dispatched by the `zotero_items` MCP tool router.
pub(crate) enum ZoteroItemsWriteCommand {
    /// Update fields on an existing item.
    Update(UpdateItemArgs),
    /// Permanently delete an item (must be trashed first).
    Delete(DeleteItemArgs),
    /// Move an item to the trash.
    Trash(TrashItemArgs),
    /// Restore an item from the trash.
    Restore(TrashItemArgs),
    /// Create an item by DOI, ISBN, arXiv ID, or other identifier.
    AddByIdentifier(crate::mcp::zotero::metadata::AddByIdentifierArgs),
    /// Attach a file to an item.
    AttachFile(crate::mcp::zotero::attachments::AttachFileArgs),
    /// Import a PDF and attach it to an item.
    ImportPdf(crate::mcp::zotero::attachments::ImportPdfArgs),
}

#[tool_router(router = items_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_items",
        description = "Grouped Zotero item read router. action: recent, get, \
                       metadata, children, fulltext",
        annotations(
            title = "Read Zotero Items",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// Dispatches read-only item tool commands to internal handlers.
    ///
    /// Receives parsed `args` wrapped in [`Parameters`], routing `recent`,
    /// `get`, `metadata`, `children`, or `fulltext` actions.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_items(
        &self,
        Parameters(args): Parameters<ZoteroItemsCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroItemsCommand::Recent(args) => {
                self.zotero_get_recent_impl(args).await
            }
            ZoteroItemsCommand::Get(args) => {
                self.zotero_get_item_impl(args).await
            }
            ZoteroItemsCommand::Metadata(args) => {
                self.zotero_get_item_metadata_impl(args).await
            }
            ZoteroItemsCommand::Children(args) => {
                self.zotero_get_item_children_impl(args).await
            }
            ZoteroItemsCommand::Fulltext(args) => {
                self.zotero_get_item_fulltext_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_items_write",
        description = "Grouped Zotero item write router. action: update, \
                       delete, trash, restore, add_by_identifier, \
                       attach_file, import_pdf",
        annotations(
            title = "Write Zotero Items",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    /// Dispatches item write tool commands to internal handlers.
    ///
    /// Receives parsed `args` wrapped in [`Parameters`], routing `update`,
    /// `delete`, `trash`, `restore`, `add_by_identifier`, `attach_file`, or
    /// `import_pdf` actions.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_items_write(
        &self,
        Parameters(args): Parameters<ZoteroItemsWriteCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroItemsWriteCommand::Update(args) => {
                self.zotero_update_item_impl(args).await
            }
            ZoteroItemsWriteCommand::Delete(args) => {
                self.zotero_delete_item_impl(args).await
            }
            ZoteroItemsWriteCommand::Trash(args) => {
                self.zotero_trash_item_impl(args).await
            }
            ZoteroItemsWriteCommand::Restore(args) => {
                self.zotero_restore_item_impl(args).await
            }
            ZoteroItemsWriteCommand::AddByIdentifier(args) => {
                self.zotero_add_by_identifier_impl(args).await
            }
            ZoteroItemsWriteCommand::AttachFile(args) => {
                self.zotero_attach_file_impl(args).await
            }
            ZoteroItemsWriteCommand::ImportPdf(args) => {
                self.zotero_import_pdf_impl(args).await
            }
        }
    }
}

impl ZoteroMcpServer {
    /// Handles recent Zotero item lookup tool calls via
    /// [`ZoteroClient::get_recent_items`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_recent_impl(
        &self,
        args: GetRecentArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(10).min(100);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_recent_items(limit).await))
    }

    /// Handles Zotero item retrieval tool calls via [`ZoteroClient::get_item`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_item_impl(
        &self,
        args: GetItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_item(&args.item_key).await))
    }

    /// Handles Zotero child item listing tool calls via
    /// [`ZoteroClient::get_item_children`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_item_children_impl(
        &self,
        args: GetItemChildrenArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_item_children(&args.item_key).await))
    }

    /// Handles Zotero item update tool calls via [`ZoteroClient::update_item`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_update_item_impl(
        &self,
        args: UpdateItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.update_item(&args.item_key, args.fields).await))
    }

    /// Handles Zotero item permanent deletion tool calls via
    /// [`ZoteroClient::delete_item`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_delete_item_impl(
        &self,
        args: DeleteItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.delete_item(&args.item_key).await {
            Ok(()) => Ok(text_success("Item permanently deleted")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero item trash tool calls via
    /// [`ZoteroClient::set_item_deleted`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_trash_item_impl(
        &self,
        args: TrashItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .set_item_deleted(&args.item_key, TrashAction::MoveToTrash)
                .await,
        ))
    }

    /// Handles Zotero item restore-from-trash tool calls via
    /// [`ZoteroClient::set_item_deleted`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_restore_item_impl(
        &self,
        args: TrashItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.set_item_deleted(&args.item_key, TrashAction::Restore).await,
        ))
    }

    /// Handles Zotero unfiled items listing tool calls via
    /// [`ZoteroClient::get_unfiled_items`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_get_unfiled_items_impl(
        &self,
        args: GetUnfiledItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(50);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_unfiled_items(limit).await))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ZoteroMcpServer, mcp::zotero::fixtures::*, state::AppState};

    mod read_operations {

        use super::*;

        #[tokio::test]
        async fn get_recent_returns_items() {
            // Arrange
            let items = json!([{
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Test Title" }
            }]);
            let base =
                mock_server(vec![http_response("200 OK", &items.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_get_recent_impl(GetRecentArgs {
                    limit: Some(10),
                })
                .await
                .expect("get recent ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    mod unfiled_operations {

        use super::*;

        #[tokio::test]
        async fn get_unfiled_items_returns_items() {
            // Arrange
            let items = json!([{
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Unfiled Item", "collections": [] }
            }]);
            let base =
                mock_server(vec![http_response("200 OK", &items.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_get_unfiled_items_impl(GetUnfiledItemsArgs {
                    limit: Some(50),
                })
                .await
                .expect("get unfiled ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    mod write_operations {

        use super::*;

        #[tokio::test]
        async fn delete_item_deletes_item() {
            // Arrange
            let item = json!({
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle" }
            });
            let base = mock_server(vec![
                http_response("200 OK", &item.to_string()),
                http_response("204 No Content", ""),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_delete_item_impl(DeleteItemArgs {
                    item_key: "ITEM1".into(),
                })
                .await
                .expect("delete item ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
        #[tokio::test]
        async fn trash_item_moves_item_to_trash() {
            // Arrange
            let item = json!({
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle" }
            });
            let updated = json!({
                "key": "ITEM1",
                "version": 2,
                "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "deleted": true }
            });
            let base = mock_server(vec![
                http_response("200 OK", &item.to_string()),
                http_response("200 OK", &updated.to_string()),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_trash_item_impl(TrashItemArgs {
                    item_key: "ITEM1".into(),
                })
                .await
                .expect("trash item ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
        #[tokio::test]
        async fn restore_item_restores_item_from_trash() {
            // Arrange
            let item = json!({
                "key": "ITEM1",
                "version": 2,
                "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "deleted": true }
            });
            let updated = json!({
                "key": "ITEM1",
                "version": 3,
                "data": { "key": "ITEM1", "version": 3, "itemType": "journalArticle", "deleted": false }
            });
            let base = mock_server(vec![
                http_response("200 OK", &item.to_string()),
                http_response("200 OK", &updated.to_string()),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_restore_item_impl(TrashItemArgs {
                    item_key: "ITEM1".into(),
                })
                .await
                .expect("restore item ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
        #[tokio::test]
        async fn delete_item_returns_error_when_write_disabled() {
            // Arrange
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: String::new(),
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                write_enabled: false,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_delete_item_impl(DeleteItemArgs {
                    item_key: "ITEM1".into(),
                })
                .await
                .expect("write disabled result");

            // Assert
            assert_eq!(res.is_error, Some(true));
        }
    }
}
