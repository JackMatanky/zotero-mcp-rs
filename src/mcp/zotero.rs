//! MCP tool handlers and argument models for Zotero Local API operations.
//!
//! This module provides argument models and server implementation methods for
//! core Zotero operations:
//! - **Read operations**: Recent items, searches, item metadata, collection
//!   contents, fulltext, PDF pages, notes
//! - **Write operations**: Note and collection creation, field updates, file
//!   attachments, tag updates, trashing, deletion
//! - **Tag administration**: Tag listing, renaming, and batch deletion
//! - **Identifier resolution**: Item creation via DOI, arXiv ID, or ISBN
//! - **Discovery & analysis**: Duplicate finding, citation key lookup, advanced
//!   searches, library coverage analysis
//! - **Annotation synthesis**: Reading and creating PDF annotations

use std::path::{Path, PathBuf};

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use super::pdf::{
    canonicalize_existing_path, find_pdf_path, resolve_attachment_pdf_path,
};
use crate::{
    ZoteroMcpServer,
    better_bibtex::{BetterBibtexClient, TranslatorName},
    errors::ZoteroMcpError,
    pdf::{extract_pdf_outline, extract_pdf_pages},
    zotero::{
        AnnotationDraft, AnnotationType, CitationKey, CollectionItemAction,
        CollectionKey, ItemKey, ItemType, JoinMode, LocalZoteroDb,
        SearchCondition, SearchField, SearchOperator, SortDirection, SortField,
        TagName, TrashAction, ZoteroClient, find_zotero_db,
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
    /// Search query matched against title, creator, year, or fulltext.
    pub(crate) query: String,
    /// Optional collection key ([`CollectionKey`]) to search within.
    pub(crate) collection_key: Option<CollectionKey>,
    /// 0-based offset into the full result set (default: 0).
    pub(crate) start: Option<usize>,
    /// Maximum number of items to return (default: 20).
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

/// Arguments for `zotero_get_collection_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCollectionItemsArgs {
    /// Zotero collection key ([`CollectionKey`]).
    pub(crate) collection_key: CollectionKey,
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

/// Arguments for `zotero_get_pdf_path`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfPathArgs {
    /// Zotero item key ([`ItemKey`]) for parent item or attachment item.
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_read_pdf_pages`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ReadPdfPagesArgs {
    /// Zotero item key; direct PDF paths must resolve under configured or
    /// Zotero-reported PDF roots, otherwise direct-path opt-in is required.
    pub(crate) item_key_or_path: String,
    /// 1-based page numbers to extract (e.g. `[1, 2, 3]`).
    pub(crate) pages: Option<Vec<usize>>,
}

/// Arguments for `zotero_get_pdf_outline`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfOutlineArgs {
    /// Zotero item key; direct PDF paths must resolve under configured or
    /// Zotero-reported PDF roots, otherwise direct-path opt-in is required.
    pub(crate) item_key_or_path: String,
}

/// Arguments for `zotero_get_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetNotesArgs {
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
}

// --- Zotero Write Operations ---

/// Arguments for `zotero_create_note`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateNoteArgs {
    /// Key of the parent item ([`ItemKey`]).
    pub(crate) parent_item_key: ItemKey,
    /// HTML or Markdown content for the note.
    pub(crate) note_content: String,
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
    /// New parent collection key ([`CollectionKey`]); pass an empty string to
    /// move the collection to the top level.
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

// --- Zotero Identifier Lookup ---

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

// --- Zotero Discovery & Analysis ---

/// Arguments for `zotero_find_duplicates`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key ([`CollectionKey`]) to scope duplicate search.
    pub(crate) collection_key: Option<CollectionKey>,
}

/// Arguments for `zotero_search_by_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByTagArgs {
    /// Tag name ([`TagName`]) to search for.
    pub(crate) tag: TagName,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_by_citation_key`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByCitationKeyArgs {
    /// Citation key ([`CitationKey`]) to match.
    pub(crate) citekey: CitationKey,
}

/// Arguments for `zotero_advanced_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions ([`SearchCondition`]).
    pub(crate) conditions: Vec<SearchCondition>,
    /// `"all"` (AND, default) or `"any"` (OR).
    pub(crate) join_mode: Option<JoinMode>,
    /// Sort field: `"dateAdded"`, `"dateModified"`, `"title"`, `"date"`, or
    /// `"creator"`.
    pub(crate) sort_by: Option<SortField>,
    /// Sort direction: `"asc"` or `"desc"` (default: `"asc"`).
    pub(crate) sort_direction: Option<SortDirection>,
    /// 0-based offset into the full result set (default: 0).
    pub(crate) start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_fulltext_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FulltextSearchArgs {
    /// Free-text query matched against title, creators, DOI, and indexed
    /// fulltext.
    pub(crate) query: String,
    /// Maximum number of results to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_notes_annotations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchNotesAnnotationsArgs {
    /// Free-text query matched against note body and annotation text/comment.
    pub(crate) query: String,
    /// Maximum number of results to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_library_coverage`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct LibraryCoverageArgs {
    /// Optional collection key ([`CollectionKey`]) to scope coverage analysis.
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
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
}

/// Arguments for `zotero_create_annotation`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateAnnotationArgs {
    /// Key of the parent PDF attachment ([`ItemKey`]).
    pub(crate) parent_attachment_key: ItemKey,
    /// Type of annotation ([`AnnotationType`]).
    pub(crate) annotation_type: AnnotationType,
    /// Selected text (required for highlight/underline, omit for note).
    pub(crate) text: Option<String>,
    /// Optional user comment attached to the annotation.
    pub(crate) comment: Option<String>,
    /// CSS-style hex color, e.g. `"#ffd400"`.
    pub(crate) color: Option<String>,
    /// Optional PDF page label where the annotation appears.
    pub(crate) page_label: Option<String>,
    /// Raw Zotero `annotationPosition` JSON object.
    pub(crate) position: serde_json::Value,
}

// --- Zotero Related Items ---

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
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client
                .search_items(
                    &args.query,
                    args.collection_key.as_ref(),
                    offset,
                    limit,
                )
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

        let bridge_roots = Vec::new();
        let found_path = if item.data.item_type == ItemType::Attachment {
            item.data
                .path
                .as_deref()
                .map(PathBuf::from)
                .or_else(|| {
                    resolve_attachment_pdf_path(&item, &bridge_roots)
                        .map(|resolved| resolved.path)
                })
                .map(|path| path.display().to_string())
        } else {
            match client.get_item_children(&args.item_key).await {
                Ok(children) => find_pdf_path(&children, &bridge_roots)
                    .map(|resolved| resolved.path.display().to_string()),
                Err(e) => return Ok(super::text_error(&e)),
            }
        };

        match found_path {
            Some(path) => Ok(super::text_success(path)),
            None => Ok(super::text_error("No PDF attachment found for item")),
        }
    }

    /// Resolves and security-validates the PDF file path for
    /// `item_key_or_path`, which may be an item key (parent or attachment)
    /// or a direct filesystem path.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`], [`ZoteroMcpError::Network`], or
    ///   [`ZoteroMcpError::Json`] if the item cannot be fetched
    /// - [`ZoteroMcpError::NotFound`] if the item has no PDF attachment (or its
    ///   children cannot be fetched)
    /// - [`ZoteroMcpError::InputRejected`] if the path fails security checks
    /// - [`ZoteroMcpError::Io`] if canonicalization or PDF validation fails
    pub(crate) async fn resolve_pdf_path(
        &self,
        item_key_or_path: &str,
    ) -> Result<PathBuf, ZoteroMcpError> {
        let bridge_roots = self.fetch_bridge_pdf_roots().await;
        if Path::new(item_key_or_path).exists() {
            return self.validate_pdf_read_path(
                Path::new(item_key_or_path),
                &bridge_roots,
                true,
            );
        }

        let client = ZoteroClient::new(&self.state);
        let item_key = ItemKey::from(item_key_or_path);
        let item = client.get_item(&item_key).await?;

        let resolved =
            if item.data.item_type == ItemType::Attachment {
                resolve_attachment_pdf_path(&item, &bridge_roots)
            } else {
                client.get_item_children(&item_key).await.ok().and_then(
                    |children| find_pdf_path(&children, &bridge_roots),
                )
            };
        let Some(resolved) = resolved else {
            return Err(ZoteroMcpError::NotFound(format!(
                "No PDF file path found for key: {item_key_or_path}"
            )));
        };

        if resolved.requires_root_check {
            self.validate_pdf_read_path(&resolved.path, &bridge_roots, false)
        } else {
            let checked = canonicalize_existing_path(&resolved.path)?;
            self.state.check_pdf_file(&checked)?;
            Ok(checked)
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
        let pdf_path = match self.resolve_pdf_path(&args.item_key_or_path).await
        {
            Ok(path) => path,
            Err(e) => return Ok(super::text_error(&e)),
        };
        let pages_ref = args.pages.as_deref();
        Ok(super::json_result(extract_pdf_pages(
            &pdf_path,
            pages_ref,
            self.state.security.max_pdf_bytes,
        )))
    }

    /// Handles PDF outline extraction tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_pdf_outline_impl(
        &self,
        args: GetPdfOutlineArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let pdf_path = match self.resolve_pdf_path(&args.item_key_or_path).await
        {
            Ok(path) => path,
            Err(e) => return Ok(super::text_error(&e)),
        };
        Ok(super::json_result(extract_pdf_outline(
            &pdf_path,
            self.state.security.max_pdf_bytes,
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
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client
                .advanced_search(
                    args.conditions,
                    args.join_mode.unwrap_or_default(),
                    args.sort_by,
                    args.sort_direction.unwrap_or_default(),
                    offset,
                    limit,
                )
                .await,
        ))
    }

    /// Handles local full-text search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_fulltext_search_impl(
        &self,
        args: FulltextSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let state = &self.state;
        let result = async {
            state.check_sqlite_access()?;
            let Some(db_path) = find_zotero_db() else {
                return Err(ZoteroMcpError::LocalDb(
                    "Zotero sqlite database not found".to_owned(),
                ));
            };
            let db = LocalZoteroDb::open(&db_path).await?;
            db.search_fulltext(&args.query, limit).await
        }
        .await;
        Ok(super::json_result(result))
    }

    /// Handles local note/annotation search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_notes_annotations_impl(
        &self,
        args: SearchNotesAnnotationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let state = &self.state;
        let result = async {
            state.check_sqlite_access()?;
            let Some(db_path) = find_zotero_db() else {
                return Err(ZoteroMcpError::LocalDb(
                    "Zotero sqlite database not found".to_owned(),
                ));
            };
            let db = LocalZoteroDb::open(&db_path).await?;
            db.search_notes_annotations(&args.query, limit).await
        }
        .await;
        Ok(super::json_result(result))
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
        Ok(super::json_result(client.get_related_items(&args.item_key).await))
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
            Ok(()) => Ok(super::text_success("Item relation added")),
            Err(e) => Ok(super::text_error(&e)),
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
            Ok(()) => Ok(super::text_success("Item relation removed")),
            Err(e) => Ok(super::text_error(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::state::{AppState, SecurityConfig};

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use rmcp::model::CallToolResult;
        use serde_json::json;

        use crate::state::{AppState, SecurityConfig};
        pub(super) fn zotero_state(zotero_api_url: String) -> AppState {
            AppState {
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            }
        }

        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        pub(super) fn http_response_with_headers(
            status: &str,
            headers: &[(&str, &str)],
            body: &str,
        ) -> String {
            let hdrs = headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}\r\n"))
                .collect::<Vec<_>>()
                .join("");
            format!(
                "HTTP/1.1 {status}\r\n{hdrs}Content-Length: \
                 {}\r\nContent-Type: application/json\r\nConnection: \
                 close\r\n\r\n{body}",
                body.len()
            )
        }

        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }

        pub(super) fn security_with_pdf_limit(
            max_pdf_bytes: u64,
        ) -> SecurityConfig {
            SecurityConfig {
                max_pdf_bytes,
                ..SecurityConfig::default()
            }
        }

        pub(super) fn parent_journal_item() -> serde_json::Value {
            json!({
                "key": "ITEM0001",
                "version": 1,
                "data": {
                    "key": "ITEM0001",
                    "version": 1,
                    "itemType": "journalArticle",
                },
            })
        }

        pub(super) fn zotero_pdf_server(children: serde_json::Value) -> String {
            mock_server(vec![
                http_response("200 OK", &parent_journal_item().to_string()),
                http_response("200 OK", &children.to_string()),
            ])
        }

        pub(super) fn bridge_pdf_root(
            kind: &str,
            path: &std::path::Path,
        ) -> String {
            let body = json!({
                "roots": [{
                    "kind": kind,
                    "path": path.canonicalize().unwrap(),
                }],
            });
            mock_server(vec![http_response("200 OK", &body.to_string())])
        }

        pub(super) fn tool_text(res: &CallToolResult) -> String {
            res.content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.to_string())
                .unwrap_or_default()
        }
    }

    use fixtures::*;

    mod read_operations {
        use pretty_assertions::assert_eq;

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

        #[tokio::test]
        async fn list_tags_returns_tags() {
            // Arrange
            let tags = json!([{"tag": "quantum", "meta": {"numItems": 3}}]);
            let base =
                mock_server(vec![http_response("200 OK", &tags.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_list_tags_impl(ListTagsArgs {
                    limit: Some(50),
                })
                .await
                .expect("list tags ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    mod write_operations {
        use pretty_assertions::assert_eq;

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

        #[tokio::test]
        async fn rename_tag_patches_item_tags() {
            // Arrange
            let items = json!([{
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "tags": [{ "tag": "old_tag" }] }
            }]);
            let patched = json!({
                "key": "ITEM1",
                "version": 2,
                "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "tags": [{ "tag": "new_tag" }] }
            });
            let base = mock_server(vec![
                http_response("200 OK", &items.to_string()),
                http_response("200 OK", &patched.to_string()),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_rename_tag_impl(RenameTagArgs {
                    old_tag: "old_tag".into(),
                    new_tag: "new_tag".into(),
                })
                .await
                .expect("rename tag ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }

        #[tokio::test]
        async fn delete_tags_removes_tags() {
            // Arrange
            let base = mock_server(vec![
                http_response_with_headers(
                    "200 OK",
                    &[("Last-Modified-Version", "9")],
                    "[]",
                ),
                http_response("204 No Content", ""),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_delete_tags_impl(DeleteTagsArgs {
                    tags: vec!["old_tag".into()],
                })
                .await
                .expect("delete tags ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }

        #[tokio::test]
        async fn create_annotation_creates_pdf_annotation() {
            // Arrange
            let created = json!([{
                "key": "ANNOT1",
                "version": 1,
                "data": { "key": "ANNOT1", "version": 1, "itemType": "annotation", "annotationType": "highlight" }
            }]);
            let base = mock_server(vec![http_response(
                "200 OK",
                &created.to_string(),
            )]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_create_annotation_impl(CreateAnnotationArgs {
                    parent_attachment_key: "ATT1".into(),
                    annotation_type: AnnotationType::Highlight,
                    text: Some("selected text".to_owned()),
                    comment: None,
                    color: None,
                    page_label: None,
                    position: json!({"pageIndex": 0, "rects": [[100, 200, 300, 220]]}),
                })
                .await
                .expect("create annotation ok");

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

    mod pdf_pages {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn rejects_direct_path_by_default() {
            // Arrange
            let temp =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            let server = ZoteroMcpServer::new(AppState {
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: temp.path().display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("Direct file paths are disabled"));
        }

        #[tokio::test]
        async fn allows_direct_path_inside_bridge_pdf_root_without_direct_flag()
        {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let body = json!({
                "roots": [{
                    "kind": "attanger-dest",
                    "path": root.path().canonicalize().unwrap(),
                }],
            });
            let bridge_base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(AppState {
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: pdf.display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }

        #[tokio::test]
        async fn rejects_direct_path_outside_bridge_pdf_roots() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let pdf = outside.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let body = json!({
                "roots": [{
                    "kind": "attanger-dest",
                    "path": root.path().canonicalize().unwrap(),
                }],
            });
            let bridge_base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(AppState {
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: pdf.display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("Direct file paths are disabled"));
        }

        #[tokio::test]
        async fn allows_direct_path_inside_configured_root_when_bridge_unavailable()
         {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let mut security = SecurityConfig::default();
            security.direct_file_paths = true;
            security.allowed_read_dirs =
                vec![root.path().canonicalize().unwrap()];
            let server = ZoteroMcpServer::new(AppState {
                better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
                security,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: pdf.display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }

        #[tokio::test]
        async fn rejects_direct_path_outside_allowed_root() {
            // Arrange
            let allowed = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let pdf = outside.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let mut security = SecurityConfig::default();
            security.direct_file_paths = true;
            security.allowed_read_dirs =
                vec![allowed.path().canonicalize().unwrap()];
            let server = ZoteroMcpServer::new(AppState {
                security,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: pdf.display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("outside allowed"));
        }

        #[tokio::test]
        async fn reads_imported_attachment_enclosure_without_allowed_dirs() {
            // Arrange
            let pdf =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            std::fs::write(pdf.path(), b"not a pdf").unwrap();
            let file_url =
                url::Url::from_file_path(pdf.path()).unwrap().to_string();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "links": {
                    "enclosure": {
                        "href": file_url,
                        "type": "application/pdf",
                        "title": "bad.pdf",
                    },
                },
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "imported_file",
                    "contentType": "application/pdf",
                    "filename": "bad.pdf",
                },
            }]);
            let zotero_base = zotero_pdf_server(children);
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }

        #[tokio::test]
        async fn reads_linked_attanger_attachment_inside_bridge_root() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "linked_file",
                    "contentType": "application/pdf",
                    "path": pdf.display().to_string(),
                },
            }]);
            let zotero_base = zotero_pdf_server(children);
            let bridge_base = bridge_pdf_root("attanger-dest", root.path());
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }

        #[tokio::test]
        async fn rejects_linked_attachment_outside_pdf_roots() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let pdf = outside.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "linked_file",
                    "contentType": "application/pdf",
                    "path": pdf.display().to_string(),
                },
            }]);
            let zotero_base = zotero_pdf_server(children);
            let bridge_base = bridge_pdf_root("attanger-dest", root.path());
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("outside allowed"));
        }

        #[tokio::test]
        async fn resolves_relative_linked_attachment_from_zotero_base_root() {
            // Arrange
            let base = tempfile::TempDir::new().unwrap();
            let subdir = base.path().join("subdir");
            std::fs::create_dir_all(&subdir).unwrap();
            let pdf = subdir.join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "linked_file",
                    "contentType": "application/pdf",
                    "path": "attachments:subdir/bad.pdf",
                },
            }]);
            let zotero_base = zotero_pdf_server(children);
            let bridge_base =
                bridge_pdf_root("zotero-linked-base", base.path());
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }
    }

    mod pdf_outline {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn rejects_direct_path_by_default() {
            // Arrange
            let temp =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            let server = ZoteroMcpServer::new(AppState {
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                    item_key_or_path: temp.path().display().to_string(),
                })
                .await
                .expect("get pdf outline result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("Direct file paths are disabled"));
        }

        #[tokio::test]
        async fn returns_outline_for_direct_path_inside_configured_root() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("outline.pdf");
            crate::pdf::write_pdf_with_outline(&pdf);
            let mut security = SecurityConfig::default();
            security.direct_file_paths = true;
            security.allowed_read_dirs =
                vec![root.path().canonicalize().unwrap()];
            let server = ZoteroMcpServer::new(AppState {
                security,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                    item_key_or_path: pdf.display().to_string(),
                })
                .await
                .expect("get pdf outline result");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = tool_text(&res);
            assert!(text.contains("Chapter 1"));
            assert!(text.contains("Section 2.1"));
        }

        #[tokio::test]
        async fn returns_empty_outline_for_pdf_without_bookmarks() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("plain.pdf");
            crate::pdf::write_pdf_without_outline(&pdf);
            let mut security = SecurityConfig::default();
            security.direct_file_paths = true;
            security.allowed_read_dirs =
                vec![root.path().canonicalize().unwrap()];
            let server = ZoteroMcpServer::new(AppState {
                security,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                    item_key_or_path: pdf.display().to_string(),
                })
                .await
                .expect("get pdf outline result");

            // Assert
            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("[]"));
        }

        #[tokio::test]
        async fn reads_imported_attachment_enclosure_outline() {
            // Arrange
            let pdf =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            crate::pdf::write_pdf_with_outline(pdf.path());
            let file_url =
                url::Url::from_file_path(pdf.path()).unwrap().to_string();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "links": {
                    "enclosure": {
                        "href": file_url,
                        "type": "application/pdf",
                        "title": "outline.pdf",
                    },
                },
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "imported_file",
                    "contentType": "application/pdf",
                    "filename": "outline.pdf",
                },
            }]);
            let zotero_base = zotero_pdf_server(children);
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
                security: security_with_pdf_limit(1024 * 1024),
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                })
                .await
                .expect("get pdf outline result");

            // Assert
            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("Chapter 1"));
        }
    }

    mod related_items {
        use pretty_assertions::assert_eq;

        use super::*;

        fn item_json(key: &str, relations: &serde_json::Value) -> String {
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": "journalArticle",
                    "relations": relations.clone(),
                },
            })
            .to_string()
        }

        fn related_item_json(key: &str, title: &str) -> String {
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": "journalArticle",
                    "title": title,
                },
            })
            .to_string()
        }

        const URI_A_TO_B: &str = "http://zotero.org/users/0/items/ITEM0002";
        const URI_B_TO_A: &str = "http://zotero.org/users/0/items/ITEM0001";

        #[tokio::test]
        async fn get_related_items_returns_related_items() {
            // Arrange
            let source = item_json(
                "ITEM0001",
                &serde_json::json!({
                    "dc:relation": [URI_A_TO_B],
                }),
            );
            let base = mock_server(vec![
                http_response("200 OK", &source),
                http_response(
                    "200 OK",
                    &related_item_json("ITEM0002", "Related Article"),
                ),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_get_related_items_impl(GetRelatedItemsArgs {
                    item_key: "ITEM0001".into(),
                })
                .await
                .expect("get related items ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = tool_text(&res);
            assert!(text.contains("ITEM0002"));
            assert!(text.contains("Related Article"));
        }

        #[tokio::test]
        async fn add_item_relation_links_items_and_returns_success() {
            // Arrange
            let base = mock_server(vec![
                http_response("200 OK", &item_json("ITEM0001", &json!({}))),
                http_response("200 OK", &item_json("ITEM0002", &json!({}))),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0001",
                        &serde_json::json!({
                            "dc:relation": [URI_A_TO_B],
                        }),
                    ),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0002",
                        &serde_json::json!({
                            "dc:relation": [URI_B_TO_A],
                        }),
                    ),
                ),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_add_item_relation_impl(AddItemRelationArgs {
                    item_key: "ITEM0001".into(),
                    related_item_key: "ITEM0002".into(),
                })
                .await
                .expect("add item relation ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("Item relation added"));
        }

        #[tokio::test]
        async fn add_item_relation_returns_error_when_write_disabled() {
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
                .zotero_add_item_relation_impl(AddItemRelationArgs {
                    item_key: "ITEM0001".into(),
                    related_item_key: "ITEM0002".into(),
                })
                .await
                .expect("write disabled result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("Permission denied"));
        }

        #[tokio::test]
        async fn remove_item_relation_unlinks_items_and_returns_success() {
            // Arrange
            let base = mock_server(vec![
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0001",
                        &serde_json::json!({
                            "dc:relation": [URI_A_TO_B],
                        }),
                    ),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0002",
                        &serde_json::json!({
                            "dc:relation": [URI_B_TO_A],
                        }),
                    ),
                ),
                http_response("200 OK", &item_json("ITEM0001", &json!({}))),
                http_response("200 OK", &item_json("ITEM0002", &json!({}))),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_remove_item_relation_impl(RemoveItemRelationArgs {
                    item_key: "ITEM0001".into(),
                    related_item_key: "ITEM0002".into(),
                })
                .await
                .expect("remove item relation ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("Item relation removed"));
        }
    }

    mod sqlite_tools {
        use std::{path::Path, str::FromStr};

        use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

        use super::*;

        #[expect(
            clippy::too_many_lines,
            reason = "seeds a realistic Zotero schema across many tables"
        )]
        async fn seed_db(path: &Path) {
            let opts = SqliteConnectOptions::from_str(&format!(
                "sqlite://{}",
                path.display()
            ))
            .unwrap()
            .create_if_missing(true);
            let pool = SqlitePool::connect_with(opts).await.unwrap();
            sqlx::query(
                "CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, \
                 typeName TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE items (itemID INTEGER PRIMARY KEY, key TEXT, \
                 itemTypeID INTEGER, dateAdded TEXT, dateModified TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE fields (fieldID INTEGER PRIMARY KEY, fieldName \
                 TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemData (itemID INTEGER, fieldID INTEGER, \
                 valueID INTEGER)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, \
                 value TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, \
                 firstName TEXT, lastName TEXT, fieldMode INT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemCreators (itemID INTEGER, creatorID INTEGER)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("CREATE TABLE deletedItems (itemID INTEGER)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE fulltextWords (wordID INTEGER PRIMARY KEY, word \
                 TEXT UNIQUE)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE fulltextItemWords (wordID INT, itemID INT, \
                 PRIMARY KEY (wordID, itemID))",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemNotes (itemID INTEGER, parentItemID \
                 INTEGER, note TEXT, title TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemAnnotations (itemID INTEGER, parentItemID \
                 INTEGER, text TEXT, comment TEXT, type INTEGER, color TEXT, \
                 pageLabel TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemAttachments (itemID INTEGER, parentItemID \
                 INTEGER, path TEXT, contentType TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO fields (fieldID, fieldName) VALUES (1, 'title'), \
                 (16, 'extra'), (7, 'DOI')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO itemTypes (itemTypeID, typeName) VALUES (1, \
                 'journalArticle'), (2, 'note')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
                 dateModified) VALUES (1, 'K00001', 1, '2024-01-01', \
                 '2024-02-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO itemData (itemID, fieldID, valueID) VALUES (1, \
                 1, 100), (1, 7, 101)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO itemDataValues (valueID, value) VALUES (100, \
                 'Rust in Action'), (101, '10.1000/rust')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO fulltextWords (wordID, word) VALUES (1, 'the'), \
                 (2, 'borrow'), (3, 'checker'), (4, 'ensures'), (5, \
                 'memory'), (6, 'safety')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO fulltextItemWords (wordID, itemID) VALUES (1, \
                 1), (2, 1), (3, 1), (4, 1), (5, 1), (6, 1)",
            )
            .execute(&pool)
            .await
            .unwrap();
            pool.close().await;
        }

        #[tokio::test]
        async fn fulltext_tool_returns_gate_error_when_disabled() {
            let mut state = zotero_state(String::new());
            state.sqlite_access = false;
            let server = ZoteroMcpServer::new(state.clone());
            let res = server
                .zotero_fulltext_search_impl(FulltextSearchArgs {
                    query: "borrow".to_owned(),
                    limit: Some(10),
                })
                .await
                .unwrap();
            let text = tool_text(&res);
            assert!(text.contains("ZOTERO_SQLITE_ACCESS"));
        }

        #[tokio::test]
        async fn fulltext_tool_returns_hits_when_enabled() {
            let dir = tempfile::tempdir().unwrap();
            let db_path = dir.path().join("zotero.sqlite");
            seed_db(&db_path).await;

            let mut state = zotero_state(String::new());
            state.sqlite_access = true;
            std::env::set_var("ZOTERO_DB_PATH", &db_path);
            let server = ZoteroMcpServer::new(state);
            let res = server
                .zotero_fulltext_search_impl(FulltextSearchArgs {
                    query: "borrow checker".to_owned(),
                    limit: Some(10),
                })
                .await
                .unwrap();
            let text = tool_text(&res);
            assert!(text.contains("Rust in Action"));
        }
    }
}
