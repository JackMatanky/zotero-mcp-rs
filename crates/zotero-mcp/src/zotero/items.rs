//! MCP tool handlers and argument models for core Zotero item operations.
//!
//! Handles `zotero_items` (read-only) and `zotero_items_write` (mutation)
//! grouped-router tool calls for item lifecycle management. Converts incoming
//! MCP tool parameters into calls on [`ZoteroClient`] for retrieving, creating,
//! updating, trashing, restoring, and deleting Zotero items, as well as
//! metadata, full-text extraction, and attachment operations.
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
//! - [`GetItemFulltextArgs`]: Arguments for retrieving full-text content of an
//!   item.
//! - [`AttachFileArgs`]: Arguments for attaching a file or URL to an item.
//! - [`ImportPdfArgs`]: Arguments for importing a PDF file into a library.
//! - [`MetadataFormat`]: Output format for item metadata responses (JSON or
//!   `BibTeX`).
//! - [`GetItemMetadataArgs`]: Arguments for retrieving formatted item metadata.
//! - [`AddByIdentifierArgs`]: Arguments for adding library items by external
//!   identifier.

use std::path::Path;

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::{
    BetterBibtexClient, ItemKey, JoinMode, SearchCondition, SearchField,
    SearchOperator, SortOrder, TranslatorName, TrashAction, ZoteroApiError,
    ZoteroClient,
};

use crate::{
    ZoteroMcpServer,
    response::{json_result, json_success, text_error, text_result},
};

/// Mirrors `zotero_api::IdentifierKind` for MCP argument schemas.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum IdentifierKindArg {
    /// Digital Object Identifier resolved via Crossref.
    Doi,
    /// arXiv identifier resolved via Semantic Scholar.
    Arxiv,
    /// International Standard Book Number resolved via Open Library.
    Isbn,
}

impl From<IdentifierKindArg> for zotero_api::IdentifierKind {
    #[inline]
    fn from(value: IdentifierKindArg) -> Self {
        match value {
            IdentifierKindArg::Doi => Self::Doi,
            IdentifierKindArg::Arxiv => Self::Arxiv,
            IdentifierKindArg::Isbn => Self::Isbn,
        }
    }
}

/// Arguments for the connector-compatible `fetch` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ConnectorFetchArgs {
    /// Zotero item key or item identifier to fetch.
    pub(crate) id: String,
}

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
    item_key: String,
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
    item_key: String,
}

/// Arguments for the `update` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateItemArgs {
    /// Zotero item key ([`ItemKey`]).
    item_key: String,
    /// JSON object containing fields to update.
    fields: serde_json::Value,
}

/// Arguments for the `delete` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteItemArgs {
    /// Key of the item ([`ItemKey`]) to permanently delete.
    item_key: String,
}

/// Arguments for the `trash` and `restore` actions of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct TrashItemArgs {
    /// Key of the item ([`ItemKey`]) to move to or restore from trash.
    item_key: String,
}

/// Arguments for the `fulltext` action of `zotero_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemFulltextArgs {
    /// Unique Zotero item key ([`ItemKey`]).
    item_key: String,
}

/// Arguments for the `attach_file` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AttachFileArgs {
    /// Key of the parent item ([`ItemKey`]).
    parent_item_key: String,
    /// Display title for the attachment.
    title: String,
    /// File path or URL.
    path_or_url: String,
    /// Optional content type (default: `"application/pdf"`).
    content_type: Option<String>,
}

/// Arguments for the `import_pdf` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ImportPdfArgs {
    /// Optional key of the parent item ([`ItemKey`]); omitted to create a
    /// top-level attachment.
    parent_item_key: Option<String>,
    /// Display title for the attachment.
    title: String,
    /// Local path to the PDF file to import.
    file_path: String,
    /// Optional content type (default: `"application/pdf"`).
    content_type: Option<String>,
}

#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema,
)]
/// Output format for item metadata responses.
#[serde(rename_all = "lowercase")]
pub(in crate::zotero) enum MetadataFormat {
    /// Return Zotero item metadata as JSON.
    #[default]
    Json,
    /// Return item metadata as Better `BibTeX`.
    Bibtex,
}

/// Arguments for the `metadata` action of `zotero_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemMetadataArgs {
    /// Zotero item key ([`ItemKey`]).
    item_key: String,
    /// Format: `"json"` or `"bibtex"` ([`MetadataFormat`]), defaulting to
    /// `"json"`.
    format: Option<MetadataFormat>,
}

impl GetItemMetadataArgs {
    /// Constructs metadata request arguments with default JSON format.
    pub(crate) fn json(item_key: String) -> Self {
        Self {
            item_key,
            format: None,
        }
    }
}

/// Arguments for the `add_by_identifier` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddByIdentifierArgs {
    /// Kind of identifier ([`IdentifierKind`](zotero_api::IdentifierKind)).
    kind: IdentifierKindArg,
    /// The DOI, arXiv ID, or ISBN to resolve.
    identifier: String,
    /// Optional collection key ([`CollectionKey`](zotero_api::CollectionKey))
    /// to file the new item into.
    collection_key: Option<String>,
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
    Metadata(GetItemMetadataArgs),
    /// List child items (notes, attachments) of an item.
    Children(GetItemChildrenArgs),
    /// Retrieve full-text content extracted from an item's attachments.
    Fulltext(GetItemFulltextArgs),
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
    AddByIdentifier(AddByIdentifierArgs),
    /// Attach a file to an item.
    AttachFile(AttachFileArgs),
    /// Import a PDF and attach it to an item.
    ImportPdf(ImportPdfArgs),
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

    #[tool(
        name = "fetch",
        description = "Connector fetch tool - get Zotero item metadata by \
                       item ID/key",
        annotations(
            title = "Fetch Zotero Item",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn connector_fetch(
        &self,
        Parameters(args): Parameters<ConnectorFetchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_metadata_impl(GetItemMetadataArgs::json(args.id))
            .await
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
    pub(in crate::zotero) async fn zotero_get_recent_impl(
        &self,
        args: GetRecentArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(10).min(100);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_recent_items(limit).await))
    }

    /// Handles single Zotero item lookup tool calls via
    /// [`ZoteroClient::get_item`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_get_item_impl(
        &self,
        args: GetItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_item(&ItemKey::from(args.item_key)).await))
    }

    /// Handles Zotero item update tool calls via [`ZoteroClient::update_item`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_update_item_impl(
        &self,
        args: UpdateItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .update_item(&ItemKey::from(args.item_key), args.fields)
                .await,
        ))
    }

    /// Handles Zotero item deletion tool calls via
    /// [`ZoteroClient::delete_item`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_delete_item_impl(
        &self,
        args: DeleteItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let res = client.delete_item(&ItemKey::from(args.item_key)).await;
        Ok(text_result(res.map(|()| "item permanently deleted".to_owned())))
    }

    /// Handles moving a Zotero item to trash via
    /// [`ZoteroClient::set_item_deleted`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_trash_item_impl(
        &self,
        args: TrashItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .set_item_deleted(
                    &ItemKey::from(args.item_key),
                    TrashAction::MoveToTrash,
                )
                .await,
        ))
    }

    /// Handles restoring a Zotero item from trash via
    /// [`ZoteroClient::set_item_deleted`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_restore_item_impl(
        &self,
        args: TrashItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .set_item_deleted(
                    &ItemKey::from(args.item_key),
                    TrashAction::Restore,
                )
                .await,
        ))
    }

    /// Handles Zotero item child listing tool calls via
    /// [`ZoteroClient::get_item_children`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_get_item_children_impl(
        &self,
        args: GetItemChildrenArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.get_item_children(&ItemKey::from(args.item_key)).await,
        ))
    }

    /// Handles Zotero unfiled items listing tool calls via
    /// [`ZoteroClient::get_unfiled_items`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_get_unfiled_items_impl(
        &self,
        args: GetUnfiledItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(50);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_unfiled_items(limit).await))
    }

    /// Handles Zotero full-text retrieval tool calls.
    ///
    /// Extracts full-text content for the item specified by `args.item_key`
    /// using the underlying [`ZoteroClient`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_get_item_fulltext_impl(
        &self,
        args: GetItemFulltextArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(text_result(
            client.get_item_fulltext(&ItemKey::from(args.item_key)).await,
        ))
    }

    /// Handles Zotero linked-file attachment tool calls.
    ///
    /// Links an external filepath or URL specified by `args` to the parent item
    /// using [`ZoteroClient::attach_file_link`].
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_attach_file_impl(
        &self,
        args: AttachFileArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .attach_file_link(
                    &ItemKey::from(args.parent_item_key),
                    &args.title,
                    &args.path_or_url,
                    args.content_type.as_deref(),
                )
                .await,
        ))
    }

    /// Handles Zotero PDF import tool calls.
    ///
    /// Validates the PDF filepath against permitted bridge roots before
    /// importing the PDF using [`ZoteroClient::import_pdf_file`].
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_import_pdf_impl(
        &self,
        args: ImportPdfArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bridge_roots = self.fetch_bridge_pdf_roots().await;
        let checked = match self.validate_pdf_read_path(
            Path::new(&args.file_path),
            &bridge_roots,
            true,
        ) {
            Ok(path) => path,
            Err(e) => return Ok(text_error(&e)),
        };
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .import_pdf_file(
                    args.parent_item_key.map(ItemKey::from).as_ref(),
                    &args.title,
                    &checked,
                    args.content_type.as_deref(),
                )
                .await,
        ))
    }

    /// Handles Zotero item metadata formatting tool calls.
    ///
    /// Fetches metadata for an item specified by [`GetItemMetadataArgs`] and
    /// converts it into the requested [`MetadataFormat`] (JSON or
    /// `BibTeX`).
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_get_item_metadata_impl(
        &self,
        args: GetItemMetadataArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let item_key = ItemKey::from(args.item_key);
        if args.format.unwrap_or_default() == MetadataFormat::Bibtex {
            let bbt_client = BetterBibtexClient::new(&self.state);
            let translator = TranslatorName::from("bibtex");
            let result = async {
                let citekeys = bbt_client
                    .get_citekeys(std::slice::from_ref(&item_key))
                    .await?;
                let citekey = citekeys
                    .get(&item_key)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        ZoteroApiError::BetterBibTeX(format!(
                            "no citation key for item {item_key}"
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
            Ok(json_result(client.get_item(&item_key).await))
        }
    }

    /// Handles Zotero add-by-identifier tool calls.
    ///
    /// Resolves the identifier via a public metadata API using
    /// [`AddByIdentifierArgs`] and creates the item, returning the existing
    /// item instead if an exact title match is already present in the library.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_add_by_identifier_impl(
        &self,
        args: AddByIdentifierArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let mut draft = match zotero_api::resolve_metadata(
            &self.state,
            args.kind.into(),
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
                    SortOrder::Asc,
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
            draft.collections.push(col.into());
        }
        Ok(json_result(client.create_item_from_metadata(draft).await))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zotero_api::AppState;

    use super::*;
    use crate::{ZoteroMcpServer, zotero::fixtures::*};

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
            let server = ZoteroMcpServer::new(
                AppState::test_default().with_write_enabled(false),
            );

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

    mod connector_operations {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn connector_fetch_returns_item_metadata() {
            let item = json!({
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Physics Paper" }
            });
            let base =
                mock_server(vec![http_response("200 OK", &item.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            let res = server
                .connector_fetch(Parameters(ConnectorFetchArgs {
                    id: "ITEM1".to_owned(),
                }))
                .await
                .expect("fetch succeeded");

            assert_eq!(res.is_error, Some(false));
        }
    }

    mod attachments {
        use zotero_api::SecurityConfig;

        use super::*;

        fn import_server(upload_base: &str) -> String {
            let created = serde_json::json!([{
                "key": "ATTACH01",
                "version": 1,
                "data": { "key": "ATTACH01", "version": 1, "itemType": "attachment" },
            }])
            .to_string();
            let phase1 = serde_json::json!({
                "url": format!("{upload_base}/upload"),
                "uploadKey": "uk",
                "contentType": "application/pdf",
                "prefix": "",
                "suffix": "",
            })
            .to_string();
            mock_server(vec![
                http_response("200 OK", &created),
                http_response("200 OK", &phase1),
                http_response("204 No Content", ""),
            ])
        }

        #[tokio::test]
        async fn import_pdf_uploads_file_and_reports_success() {
            let dir = tempfile::tempdir().expect("temp dir");
            let pdf_path = dir.path().join("paper.pdf");
            std::fs::write(&pdf_path, b"%PDF-1.4\n%%EOF\n").expect("write pdf");

            let upload_base =
                mock_server(vec![http_response("201 Created", "")]);
            let base = import_server(&upload_base);

            let mut security = SecurityConfig::from_env();
            security.set_file_paths_enabled(true);
            security.set_allowed_read_dirs(vec![dir.path().to_path_buf()]);
            let app = zotero_state(base).with_security(security);
            let server = ZoteroMcpServer::new(app);

            let res = server
                .zotero_import_pdf_impl(ImportPdfArgs {
                    parent_item_key: Some("PARENT01".into()),
                    title: "Paper".to_owned(),
                    file_path: pdf_path.to_string_lossy().into_owned(),
                    content_type: None,
                })
                .await
                .expect("import ok");

            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("ATTACH01"));
        }
    }

    mod metadata {
        use pretty_assertions::assert_eq;

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
            let server = ZoteroMcpServer::new(
                AppState::test_default()
                    .with_zotero_api_url(zotero_base)
                    .with_crossref_url(crossref_base)
                    .with_write_enabled(true),
            );

            // Act
            let res = server
                .zotero_add_by_identifier_impl(AddByIdentifierArgs {
                    kind: IdentifierKindArg::Doi,
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
            let server = ZoteroMcpServer::new(
                AppState::test_default()
                    .with_zotero_api_url(zotero_base)
                    .with_crossref_url(crossref_base)
                    .with_write_enabled(true),
            );

            // Act
            let res = server
                .zotero_add_by_identifier_impl(AddByIdentifierArgs {
                    kind: IdentifierKindArg::Doi,
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

    /// Reverse-exhaustive match on `zotero_api::IdentifierKind`: if a
    /// variant is added there, this fails to compile until
    /// `IdentifierKindArg` (and its `From` impl above) is updated too,
    /// catching schema drift a one-directional match cannot.
    mod arg_mirrors {
        use super::*;

        #[test]
        fn identifier_kind_arg_covers_every_variant() {
            fn to_arg(kind: zotero_api::IdentifierKind) -> IdentifierKindArg {
                match kind {
                    zotero_api::IdentifierKind::Doi => IdentifierKindArg::Doi,
                    zotero_api::IdentifierKind::Arxiv => {
                        IdentifierKindArg::Arxiv
                    }
                    zotero_api::IdentifierKind::Isbn => IdentifierKindArg::Isbn,
                }
            }
            let _ = to_arg;
        }
    }
}
