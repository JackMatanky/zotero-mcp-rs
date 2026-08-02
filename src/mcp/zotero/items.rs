//! MCP tool handlers and argument models for core Zotero item operations.
//!
//! Covers `zotero_items` / `zotero_items_write` grouped-router actions:
//! recent items, single-item retrieval and metadata (JSON or Better
//! `BibTeX`), child listing, fulltext, field updates, trash/restore/delete,
//! file attachment, and DOI/arXiv/ISBN identifier resolution.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    better_bibtex::{BetterBibtexClient, TranslatorName},
    errors::ZoteroMcpError,
    mcp::{json_result, json_success, text_error, text_result, text_success},
    zotero::{
        CollectionKey, ItemKey, JoinMode, SearchCondition, SearchField,
        SearchOperator, SortDirection, TrashAction, ZoteroClient,
    },
};

/// Output format for `zotero_get_item_metadata`.
#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MetadataFormat {
    /// Return Zotero item metadata as JSON.
    #[default]
    Json,
    /// Return item metadata as Better `BibTeX`.
    Bibtex,
}
/// Arguments for `zotero_get_recent`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRecentArgs {
    /// Maximum number of items to return (default: 10, max: 100).
    pub(crate) limit: Option<usize>,
}
/// Arguments for `zotero_get_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemArgs {
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_get_item_metadata`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemMetadataArgs {
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
    /// Format: `"json"` or `"bibtex"` ([`MetadataFormat`]), defaulting to
    /// `"json"`.
    pub(crate) format: Option<MetadataFormat>,
}
/// Arguments for `zotero_get_item_children`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemChildrenArgs {
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_get_item_fulltext`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemFulltextArgs {
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_update_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateItemArgs {
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
    /// JSON object containing fields to update.
    pub(crate) fields: serde_json::Value,
}
/// Arguments for `zotero_attach_file`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AttachFileArgs {
    /// Key of the parent item ([`ItemKey`]).
    pub(crate) parent_item_key: ItemKey,
    /// Display title for the attachment.
    pub(crate) title: String,
    /// File path or URL.
    pub(crate) path_or_url: String,
    /// Optional content type (default: `"application/pdf"`).
    pub(crate) content_type: Option<String>,
}
/// Arguments for `zotero_delete_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteItemArgs {
    /// Key of the item ([`ItemKey`]) to permanently delete.
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_trash_item` and `zotero_restore_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct TrashItemArgs {
    /// Key of the item ([`ItemKey`]) to move to or restore from trash.
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_add_by_identifier`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddByIdentifierArgs {
    /// Kind of identifier ([`IdentifierKind`](crate::zotero::IdentifierKind)).
    pub(crate) kind: crate::zotero::IdentifierKind,
    /// The DOI, arXiv ID, or ISBN to resolve.
    pub(crate) identifier: String,
    /// Optional collection key ([`CollectionKey`]) to file the new item into.
    pub(crate) collection_key: Option<CollectionKey>,
}

impl ZoteroMcpServer {
    /// Handles recent Zotero item lookup tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_recent_impl(
        &self,
        args: GetRecentArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(10).min(100);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_recent_items(limit).await))
    }

    /// Handles Zotero item retrieval tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_impl(
        &self,
        args: GetItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_item(&args.item_key).await))
    }

    /// Handles Zotero item metadata formatting tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_metadata_impl(
        &self,
        args: GetItemMetadataArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if args.format.unwrap_or_default() == MetadataFormat::Bibtex {
            let bbt_client = BetterBibtexClient::new(&self.state);
            let translator = TranslatorName::from("bibtex");
            let result = async {
                let citekeys = bbt_client
                    .get_citekeys(std::slice::from_ref(&args.item_key))
                    .await?;
                let citekey = citekeys
                    .get(&args.item_key)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        ZoteroMcpError::BetterBibTeX(format!(
                            "no citation key for item {}",
                            args.item_key
                        ))
                    })?;
                bbt_client
                    .export_items(std::slice::from_ref(citekey), &translator)
                    .await
            }
            .await;
            Ok(text_result(result))
        } else {
            let client = ZoteroClient::new(&self.state);
            Ok(json_result(client.get_item(&args.item_key).await))
        }
    }

    /// Handles Zotero child item listing tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_children_impl(
        &self,
        args: GetItemChildrenArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_item_children(&args.item_key).await))
    }

    /// Handles Zotero full-text retrieval tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_fulltext_impl(
        &self,
        args: GetItemFulltextArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(text_result(client.get_item_fulltext(&args.item_key).await))
    }

    /// Handles Zotero item update tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_update_item_impl(
        &self,
        args: UpdateItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.update_item(&args.item_key, args.fields).await))
    }

    /// Handles Zotero linked-file attachment tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_attach_file_impl(
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

    /// Handles Zotero item permanent deletion tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_delete_item_impl(
        &self,
        args: DeleteItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.delete_item(&args.item_key).await {
            Ok(()) => Ok(text_success("Item permanently deleted")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero item trash tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_trash_item_impl(
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

    /// Handles Zotero item restore-from-trash tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_restore_item_impl(
        &self,
        args: TrashItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.set_item_deleted(&args.item_key, TrashAction::Restore).await,
        ))
    }

    /// Handles Zotero add-by-identifier tool calls using `args`.
    ///
    /// Resolves the identifier via a public metadata API and creates the item,
    /// returning the existing item instead if an exact title match is already
    /// present in the library.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_add_by_identifier_impl(
        &self,
        args: AddByIdentifierArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let mut draft = match crate::zotero::identifiers::resolve_metadata(
            &self.state,
            args.kind,
            &args.identifier,
        )
        .await
        {
            Ok(d) => d,
            Err(e) => return Ok(text_error(&e)),
        };

        if !draft.title.is_empty() {
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::Is,
                value: draft.title.clone(),
            };
            let existing = client
                .advanced_search(
                    vec![cond],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    1,
                )
                .await;
            if let Ok(page) = existing {
                if let Some(found) = page.items.into_iter().next() {
                    return Ok(json_success(&found));
                }
            }
        }

        if let Some(col) = args.collection_key {
            draft.collections.push(col);
        }
        Ok(json_result(client.create_item_from_metadata(draft).await))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroItemsCommand {
    Recent(GetRecentArgs),
    Get(GetItemArgs),
    Metadata(GetItemMetadataArgs),
    Children(GetItemChildrenArgs),
    Fulltext(GetItemFulltextArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroItemsWriteCommand {
    Update(UpdateItemArgs),
    Delete(DeleteItemArgs),
    Trash(TrashItemArgs),
    Restore(TrashItemArgs),
    AddByIdentifier(AddByIdentifierArgs),
    AttachFile(AttachFileArgs),
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
                       delete, trash, restore, add_by_identifier, attach_file",
        annotations(
            title = "Write Zotero Items",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
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
        }
    }

    #[tool(
        name = "zotero_get_recent",
        description = "Fetch recently modified library items (notes excluded)",
        annotations(
            title = "Recently Modified Items",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_recent(
        &self,
        Parameters(args): Parameters<GetRecentArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_recent_impl(args).await
    }

    #[tool(
        name = "zotero_get_item",
        description = "Fetch a single Zotero item by its key",
        annotations(
            title = "Get Item",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item(
        &self,
        Parameters(args): Parameters<GetItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_impl(args).await
    }

    #[tool(
        name = "zotero_get_item_metadata",
        description = "Get metadata for an item as JSON or formatted BibTeX \
                       string",
        annotations(
            title = "Get Item Metadata",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_metadata(
        &self,
        Parameters(args): Parameters<GetItemMetadataArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_metadata_impl(args).await
    }

    #[tool(
        name = "zotero_get_item_children",
        description = "Get child items (notes, attachments) for a given item \
                       key",
        annotations(
            title = "Get Item Children",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_children(
        &self,
        Parameters(args): Parameters<GetItemChildrenArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_children_impl(args).await
    }

    #[tool(
        name = "zotero_get_item_fulltext",
        description = "Get Zotero's indexed fulltext for an item",
        annotations(
            title = "Get Item Full Text",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_fulltext(
        &self,
        Parameters(args): Parameters<GetItemFulltextArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_fulltext_impl(args).await
    }

    #[tool(
        name = "zotero_update_item",
        description = "Update fields of an existing item using PATCH \
                       (requires write permission)",
        annotations(
            title = "Update Item",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_update_item(
        &self,
        Parameters(args): Parameters<UpdateItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_update_item_impl(args).await
    }

    #[tool(
        name = "zotero_attach_file",
        description = "Attach a file link to a parent item (requires write \
                       permission)",
        annotations(
            title = "Attach File to Item",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_attach_file(
        &self,
        Parameters(args): Parameters<AttachFileArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_attach_file_impl(args).await
    }

    #[tool(
        name = "zotero_delete_item",
        description = "Permanently delete an item (article, note, annotation, \
                       or attachment) (requires write permission)",
        annotations(
            title = "Delete Item Permanently",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_delete_item(
        &self,
        Parameters(args): Parameters<DeleteItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_delete_item_impl(args).await
    }

    #[tool(
        name = "zotero_trash_item",
        description = "Move an item to trash, reversible (requires write \
                       permission)",
        annotations(
            title = "Move Item to Trash",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_trash_item(
        &self,
        Parameters(args): Parameters<TrashItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_trash_item_impl(args).await
    }

    #[tool(
        name = "zotero_restore_item",
        description = "Restore an item from trash (requires write permission)",
        annotations(
            title = "Restore Item from Trash",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_restore_item(
        &self,
        Parameters(args): Parameters<TrashItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_restore_item_impl(args).await
    }

    #[tool(
        name = "zotero_add_by_identifier",
        description = "Resolve a DOI, arXiv ID, or ISBN via public metadata \
                       APIs and add it to the library (returns the existing \
                       item instead of creating a duplicate if an exact title \
                       match is already present) (requires write permission)",
        annotations(
            title = "Add Item by Identifier",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_add_by_identifier(
        &self,
        Parameters(args): Parameters<AddByIdentifierArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_add_by_identifier_impl(args).await
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

    mod identifiers {

        use super::*;

        #[tokio::test]
        async fn add_by_identifier_creates_new_item() {
            // Arrange
            let crossref_body = json!({"message": {
                "title": ["A Great Paper"],
                "author": [{"given": "Sam", "family": "McAuthor"}],
                "published": {"date-parts": [[2021]]},
                "DOI": "10.1/xyz",
                "URL": "https://doi.org/10.1/xyz",
                "container-title": ["Journal of Things"]
            }});
            let crossref_base = mock_server(vec![http_response(
                "200 OK",
                &crossref_body.to_string(),
            )]);
            let created = json!([{
                "key": "NEWITEM1",
                "version": 1,
                "data": { "key": "NEWITEM1", "version": 1, "itemType": "journalArticle", "title": "A Great Paper" }
            }]);
            let zotero_base = mock_server(vec![
                http_response("200 OK", "[]"),
                http_response("200 OK", &created.to_string()),
            ]);
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: crossref_base,
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_add_by_identifier_impl(AddByIdentifierArgs {
                    kind: crate::zotero::IdentifierKind::Doi,
                    identifier: "10.1/xyz".to_owned(),
                    collection_key: None,
                })
                .await
                .expect("add by identifier ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }

        #[tokio::test]
        async fn add_by_identifier_returns_existing_item_when_duplicate_found()
        {
            // Arrange
            let crossref_body = json!({"message": {
                "title": ["A Great Paper"],
                "author": [{"given": "Sam", "family": "McAuthor"}],
                "published": {"date-parts": [[2021]]},
                "DOI": "10.1/xyz",
                "URL": "https://doi.org/10.1/xyz",
                "container-title": ["Journal of Things"]
            }});
            let crossref_base = mock_server(vec![http_response(
                "200 OK",
                &crossref_body.to_string(),
            )]);
            let existing = json!([{
                "key": "EXISTING1",
                "version": 1,
                "data": { "key": "EXISTING1", "version": 1, "itemType": "journalArticle", "title": "A Great Paper" }
            }]);
            let zotero_base = mock_server(vec![http_response(
                "200 OK",
                &existing.to_string(),
            )]);
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: crossref_base,
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_add_by_identifier_impl(AddByIdentifierArgs {
                    kind: crate::zotero::IdentifierKind::Doi,
                    identifier: "10.1/xyz".to_owned(),
                    collection_key: None,
                })
                .await
                .expect("add by identifier duplicate ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = tool_text(&res);
            assert!(text.contains("EXISTING1"));
        }
    }
}
