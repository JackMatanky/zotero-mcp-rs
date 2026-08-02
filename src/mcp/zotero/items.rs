//! MCP tool handlers and argument models for core Zotero item operations.
//!
//! Covers `zotero_items` / `zotero_items_write` grouped-router actions:
//! recent items, single-item retrieval and metadata (JSON or Better
//! `BibTeX`), child listing, fulltext, field updates, trash/restore/delete,
//! file attachment, and DOI/arXiv/ISBN identifier resolution.

use rmcp::model::CallToolResult;
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
