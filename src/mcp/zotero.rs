//! MCP tool handlers and argument models for Zotero Local API tools.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    better_bibtex::{BetterBibtexClient, TranslatorName},
    errors::ZoteroMcpError,
    pdf::extract_pdf_pages,
    zotero::{
        AnnotationDraft, AnnotationType, CitationKey, CollectionItemAction,
        CollectionKey, ItemKey, ItemType, SearchCondition, SearchField,
        SearchOperator, TagName, TrashAction, ZoteroClient, ZoteroItem,
    },
};

// --- Argument Schemas ---

/// Arguments for tools that take no parameters.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct EmptyArgs {}

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

// --- Zotero Read Operations ---

/// Arguments for `zotero_get_recent`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRecentArgs {
    /// Maximum number of items to return (default: 10, max: 100).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query across title, creator, year, or fulltext.
    pub(crate) query: String,
    /// Optional collection key to search within.
    pub(crate) collection_key: Option<CollectionKey>,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_get_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemArgs {
    /// Zotero item key.
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_get_item_metadata`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemMetadataArgs {
    /// Zotero item key.
    pub(crate) item_key: ItemKey,
    /// Format: `"json"` or `"bibtex"` (default: `"json"`).
    pub(crate) format: Option<MetadataFormat>,
}

/// Arguments for `zotero_get_collection_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCollectionItemsArgs {
    /// Zotero collection key.
    pub(crate) collection_key: CollectionKey,
}

/// Arguments for `zotero_get_item_children`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemChildrenArgs {
    /// Zotero item key.
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_get_item_fulltext`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemFulltextArgs {
    /// Zotero item key.
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_get_pdf_path`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfPathArgs {
    /// Zotero item key (parent item or attachment item).
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_read_pdf_pages`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ReadPdfPagesArgs {
    /// Zotero item key or direct file path to PDF.
    pub(crate) item_key_or_path: String,
    /// 1-based page numbers to extract (e.g. `[1, 2, 3]`).
    pub(crate) pages: Option<Vec<usize>>,
}

/// Arguments for `zotero_get_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetNotesArgs {
    /// Zotero item key.
    pub(crate) item_key: ItemKey,
}

// --- Zotero Write Operations ---

/// Arguments for `zotero_create_note`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateNoteArgs {
    /// Key of the parent item.
    pub(crate) parent_item_key: ItemKey,
    /// HTML or Markdown content for the note.
    pub(crate) note_content: String,
}

/// Arguments for `zotero_create_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateCollectionArgs {
    /// Name of the collection to create.
    pub(crate) name: String,
    /// Optional key of the parent collection.
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
    /// Zotero collection key.
    pub(crate) collection_key: CollectionKey,
    /// List of item keys to add or remove.
    pub(crate) item_keys: Vec<ItemKey>,
    /// Set to `true` to remove items instead of adding them.
    pub(crate) remove: Option<bool>,
}

/// Arguments for `zotero_update_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateItemArgs {
    /// Zotero item key.
    pub(crate) item_key: ItemKey,
    /// JSON object containing fields to update.
    pub(crate) fields: serde_json::Value,
}

/// Arguments for `zotero_attach_file`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AttachFileArgs {
    /// Key of the parent item.
    pub(crate) parent_item_key: ItemKey,
    /// Display title for the attachment.
    pub(crate) title: String,
    /// File path or URL.
    pub(crate) path_or_url: String,
    /// Optional content type (default: "application/pdf").
    pub(crate) content_type: Option<String>,
}

/// Arguments for `zotero_batch_update_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BatchUpdateTagsArgs {
    /// List of item keys.
    pub(crate) item_keys: Vec<ItemKey>,
    /// Tags to add.
    pub(crate) add_tags: Option<Vec<TagName>>,
    /// Tags to remove.
    pub(crate) remove_tags: Option<Vec<TagName>>,
}

/// Arguments for `zotero_delete_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteItemArgs {
    /// Key of the item to permanently delete.
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_trash_item` and `zotero_restore_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct TrashItemArgs {
    /// Key of the item to move to or restore from trash.
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_delete_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteCollectionArgs {
    /// Key of the collection to permanently delete.
    pub(crate) collection_key: CollectionKey,
}

/// Arguments for `zotero_update_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateCollectionArgs {
    /// Zotero collection key.
    pub(crate) collection_key: CollectionKey,
    /// New name for the collection.
    pub(crate) name: Option<String>,
    /// New parent collection key; pass an empty string to move the
    /// collection to the top level.
    pub(crate) parent_key: Option<CollectionKey>,
}

// --- Zotero Tag Administration ---

/// Arguments for `zotero_list_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ListTagsArgs {
    /// Maximum number of tags to return (default: 100).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_rename_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RenameTagArgs {
    /// Existing tag name.
    pub(crate) old_tag: TagName,
    /// New tag name.
    pub(crate) new_tag: TagName,
}

/// Arguments for `zotero_delete_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteTagsArgs {
    /// Tag names to delete from the library (up to 50).
    pub(crate) tags: Vec<TagName>,
}

// --- Zotero Identifier Lookup ---

/// Arguments for `zotero_add_by_identifier`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddByIdentifierArgs {
    /// Kind of identifier ([`IdentifierKind`](crate::zotero::IdentifierKind):
    /// `"doi"`, `"arxiv"`, or `"isbn"`).
    pub(crate) kind: crate::zotero::IdentifierKind,
    /// The DOI, arXiv ID, or ISBN to resolve.
    pub(crate) identifier: String,
    /// Optional collection key to file the new item into.
    pub(crate) collection_key: Option<CollectionKey>,
}

// --- Zotero Discovery & Analysis ---

/// Arguments for `zotero_find_duplicates`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key to scope duplicate search.
    pub(crate) collection_key: Option<CollectionKey>,
}

/// Arguments for `zotero_search_by_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByTagArgs {
    /// Tag name to search for.
    pub(crate) tag: TagName,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_by_citation_key`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByCitationKeyArgs {
    /// Citation key to match.
    pub(crate) citekey: CitationKey,
}

/// Arguments for `zotero_advanced_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions, e.g.
    /// `[{"field": "title", "operator": "contains", "value": "..."}]`.
    pub(crate) conditions: Vec<SearchCondition>,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_library_coverage`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct LibraryCoverageArgs {
    /// Optional collection key to scope coverage analysis.
    pub(crate) collection_key: Option<CollectionKey>,
}

/// Arguments for `zotero_get_unfiled_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetUnfiledItemsArgs {
    /// Maximum number of items to return (default: 50).
    pub(crate) limit: Option<usize>,
}

// --- Zotero Annotation Synthesis ---

/// Arguments for `zotero_synthesize_annotations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SynthesizeAnnotationsArgs {
    /// Zotero item key.
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_create_annotation`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateAnnotationArgs {
    /// Key of the parent PDF attachment.
    pub(crate) parent_attachment_key: ItemKey,
    /// Type of annotation: `"highlight"`, `"underline"`, or `"note"`.
    pub(crate) annotation_type: AnnotationType,
    /// Selected text (required for highlight/underline, omit for note).
    pub(crate) text: Option<String>,
    /// Optional user comment attached to the annotation.
    pub(crate) comment: Option<String>,
    /// CSS-style hex color, e.g. `"#ffd400"`.
    pub(crate) color: Option<String>,
    /// Optional PDF page label where the annotation appears.
    pub(crate) page_label: Option<String>,
    /// Raw Zotero `annotationPosition` JSON object, e.g.
    /// `{"pageIndex":0,"rects":[[100,200,300,220]]}`.
    pub(crate) position: serde_json::Value,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    // --- Zotero Diagnostics & Status ---

    /// Handles Zotero Local API status tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_status_impl(
        &self,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let status = client.check_status().await;
        Ok(super::json_success(&status))
    }

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
        Ok(super::json_result(client.get_recent_items(limit).await))
    }

    /// Handles Zotero item search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_items_impl(
        &self,
        args: SearchItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client
                .search_items(&args.query, args.collection_key.as_ref(), limit)
                .await,
        ))
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
        Ok(super::json_result(client.get_item(&args.item_key).await))
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
            Ok(super::text_result(result))
        } else {
            let client = ZoteroClient::new(&self.state);
            Ok(super::json_result(client.get_item(&args.item_key).await))
        }
    }

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
        Ok(super::json_result(
            client.get_collection_items(&args.collection_key).await,
        ))
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
        Ok(super::json_result(client.get_item_children(&args.item_key).await))
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
        Ok(super::text_result(client.get_item_fulltext(&args.item_key).await))
    }

    /// Handles Zotero PDF path discovery tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_pdf_path_impl(
        &self,
        args: GetPdfPathArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let item = match client.get_item(&args.item_key).await {
            Ok(item) => item,
            Err(e) => return Ok(super::text_error(&e)),
        };

        let found_path = if item.data.item_type == ItemType::Attachment {
            item.data.path
        } else {
            match client.get_item_children(&args.item_key).await {
                Ok(children) => find_pdf_path(&children),
                Err(e) => return Ok(super::text_error(&e)),
            }
        };

        match found_path {
            Some(path) => Ok(super::text_success(path)),
            None => Ok(super::text_error("No PDF attachment found for item")),
        }
    }

    /// Handles PDF page extraction tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_read_pdf_pages_impl(
        &self,
        args: ReadPdfPagesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let pdf_path = if std::path::Path::new(&args.item_key_or_path).exists()
        {
            args.item_key_or_path.clone()
        } else {
            let client = ZoteroClient::new(&self.state);
            let item_key_str = &args.item_key_or_path;
            let item_key = ItemKey::from(item_key_str.as_str());
            let item = match client.get_item(&item_key).await {
                Ok(item) => item,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![
                        rmcp::model::Content::text(format!(
                            "Failed to locate PDF for key '{item_key_str}': \
                             {e}"
                        )),
                    ]));
                }
            };

            let found_path = if item.data.item_type == ItemType::Attachment {
                item.data.path
            } else {
                client
                    .get_item_children(&item_key)
                    .await
                    .ok()
                    .and_then(|children| find_pdf_path(&children))
            };

            match found_path {
                Some(p) => p,
                None => {
                    return Ok(CallToolResult::error(vec![
                        rmcp::model::Content::text(format!(
                            "No PDF file path found for key: {item_key_str}"
                        )),
                    ]));
                }
            }
        };

        let pages_ref = args.pages.as_deref();
        Ok(super::json_result(extract_pdf_pages(
            std::path::Path::new(&pdf_path),
            pages_ref,
        )))
    }

    /// Handles Zotero note retrieval tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_notes_impl(
        &self,
        args: GetNotesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item_children(&args.item_key).await {
            Ok(children) => {
                let notes: Vec<_> = children
                    .into_iter()
                    .filter(|c| c.data.item_type == ItemType::Note)
                    .collect();
                Ok(super::json_success(&notes))
            }
            Err(e) => Ok(super::text_error(&e)),
        }
    }

    /// Handles Zotero note creation tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_note_impl(
        &self,
        args: CreateNoteArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client.create_note(&args.parent_item_key, &args.note_content).await,
        ))
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
        Ok(super::json_result(
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
        Ok(super::json_result(client.search_collections(&args.query).await))
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
            Ok(()) => {
                Ok(super::text_success("Collection items updated successfully"))
            }
            Err(e) => Ok(super::text_error(&e)),
        }
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
        Ok(super::json_result(
            client.update_item(&args.item_key, args.fields).await,
        ))
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
        Ok(super::json_result(
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
            Ok(count) => Ok(super::text_success(format!(
                "Batch updated tags on {count} items"
            ))),
            Err(e) => Ok(super::text_error(&e)),
        }
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
            Ok(()) => Ok(super::text_success("Item permanently deleted")),
            Err(e) => Ok(super::text_error(&e)),
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
        Ok(super::json_result(
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
        Ok(super::json_result(
            client.set_item_deleted(&args.item_key, TrashAction::Restore).await,
        ))
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
            Ok(()) => Ok(super::text_success("Collection permanently deleted")),
            Err(e) => Ok(super::text_error(&e)),
        }
    }

    /// Handles Zotero duplicate detection tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_find_duplicates_impl(
        &self,
        args: FindDuplicatesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client.find_duplicates(args.collection_key.as_ref()).await,
        ))
    }

    /// Handles Zotero tag search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_tag_impl(
        &self,
        args: SearchByTagArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(client.search_by_tag(&args.tag, limit).await))
    }

    /// Handles Zotero citation-key search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_citation_key_impl(
        &self,
        args: SearchByCitationKeyArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client.search_by_citation_key(&args.citekey).await,
        ))
    }

    /// Handles Zotero structured search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_advanced_search_impl(
        &self,
        args: AdvancedSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client.advanced_search(args.conditions, limit).await,
        ))
    }

    /// Handles Zotero library coverage analysis tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_library_coverage_impl(
        &self,
        args: LibraryCoverageArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client.get_library_coverage(args.collection_key.as_ref()).await,
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
        Ok(super::json_result(client.get_unfiled_items(limit).await))
    }

    /// Handles Zotero annotation synthesis tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_synthesize_annotations_impl(
        &self,
        args: SynthesizeAnnotationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(super::text_result(
            client.synthesize_annotations(&args.item_key).await,
        ))
    }

    /// Handles Zotero PDF annotation creation tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_annotation_impl(
        &self,
        args: CreateAnnotationArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let draft = AnnotationDraft {
            parent_attachment_key: args.parent_attachment_key,
            annotation_type: args.annotation_type,
            text: args.text,
            comment: args.comment,
            color: args.color,
            page_label: args.page_label,
            position: args.position,
        };
        Ok(super::json_result(client.create_annotation(draft).await))
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
            Err(e) => return Ok(super::text_error(&e)),
        };

        if !draft.title.is_empty() {
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::Is,
                value: draft.title.clone(),
            };
            let existing = client.advanced_search(vec![cond], 1).await;
            if let Ok(matches) = existing {
                if let Some(found) = matches.into_iter().next() {
                    return Ok(super::json_success(&found));
                }
            }
        }

        if let Some(col) = args.collection_key {
            draft.collections.push(col);
        }
        Ok(super::json_result(client.create_item_from_metadata(draft).await))
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
        Ok(super::json_result(
            client
                .update_collection(
                    &args.collection_key,
                    args.name.as_deref(),
                    args.parent_key.as_ref(),
                )
                .await,
        ))
    }

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
        Ok(super::json_result(client.list_tags(limit).await))
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
                Ok(super::text_success(format!("Renamed tag on {count} items")))
            }
            Err(e) => Ok(super::text_error(&e)),
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
            Ok(()) => Ok(super::text_success("Tags deleted")),
            Err(e) => Ok(super::text_error(&e)),
        }
    }
}
fn find_pdf_path(children: &[ZoteroItem]) -> Option<String> {
    children.iter().find_map(|child| {
        let is_pdf = child.data.item_type == ItemType::Attachment
            && child
                .data
                .content_type
                .as_deref()
                .is_some_and(|ct| ct.contains("pdf"));
        if is_pdf {
            child.data.path.clone()
        } else {
            None
        }
    })
}
