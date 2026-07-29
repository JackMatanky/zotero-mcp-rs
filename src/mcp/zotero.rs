//! MCP tool handlers and argument models for Zotero Local API tools.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    pdf::extract_pdf_pages,
    zotero::{ZoteroClient, ZoteroItem},
};

// --- Argument Schemas ---

/// Arguments for tools that take no parameters.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct EmptyArgs {}

// --- Zotero Read Operations ---

/// Arguments for `zotero_get_recent`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRecentArgs {
    /// Number of items to return (default: 10, max: 100).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query across title, creator, year, or fulltext.
    pub(crate) query: String,
    /// Optional collection key to search within.
    pub(crate) collection_key: Option<String>,
    /// Maximum number of results to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_get_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemArgs {
    /// Zotero item key.
    pub(crate) item_key: String,
}

/// Arguments for `zotero_get_item_metadata`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemMetadataArgs {
    /// Zotero item key.
    pub(crate) item_key: String,
    /// Format: `"json"` or `"bibtex"` (default: `"json"`).
    pub(crate) format: Option<String>,
}

/// Arguments for `zotero_get_collection_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCollectionItemsArgs {
    /// Zotero collection key.
    pub(crate) collection_key: String,
}

/// Arguments for `zotero_get_item_children`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemChildrenArgs {
    /// Zotero item key.
    pub(crate) item_key: String,
}

/// Arguments for `zotero_get_item_fulltext`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemFulltextArgs {
    /// Zotero item key.
    pub(crate) item_key: String,
}

/// Arguments for `zotero_get_pdf_path`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfPathArgs {
    /// Zotero item key (parent item or attachment item).
    pub(crate) item_key: String,
}

/// Arguments for `zotero_read_pdf_pages`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ReadPdfPagesArgs {
    /// Zotero item key or direct file path to PDF.
    pub(crate) item_key_or_path: String,
    /// 1-based page numbers to extract (e.g. [1, 2, 3]).
    pub(crate) pages: Option<Vec<usize>>,
}

/// Arguments for `zotero_get_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetNotesArgs {
    /// Zotero item key.
    pub(crate) item_key: String,
}

// --- Zotero Write Operations ---

/// Arguments for `zotero_create_note`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateNoteArgs {
    /// Parent item key.
    pub(crate) parent_item_key: String,
    /// HTML or Markdown content for the note.
    pub(crate) note_content: String,
}

/// Arguments for `zotero_create_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateCollectionArgs {
    /// Name of the collection to create.
    pub(crate) name: String,
    /// Optional key of parent collection.
    pub(crate) parent_key: Option<String>,
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
    pub(crate) collection_key: String,
    /// List of item keys to add or remove.
    pub(crate) item_keys: Vec<String>,
    /// Set to true to remove items instead of adding them.
    pub(crate) remove: Option<bool>,
}

/// Arguments for `zotero_update_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateItemArgs {
    /// Zotero item key.
    pub(crate) item_key: String,
    /// JSON object containing fields to update.
    pub(crate) fields: serde_json::Value,
}

/// Arguments for `zotero_attach_file`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AttachFileArgs {
    /// Parent item key.
    pub(crate) parent_item_key: String,
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
    pub(crate) item_keys: Vec<String>,
    /// Tags to add.
    pub(crate) add_tags: Option<Vec<String>>,
    /// Tags to remove.
    pub(crate) remove_tags: Option<Vec<String>>,
}

// --- Zotero Discovery & Analysis ---

/// Arguments for `zotero_find_duplicates`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key to scope duplicate search.
    pub(crate) collection_key: Option<String>,
}

/// Arguments for `zotero_search_by_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByTagArgs {
    /// Tag string to search for.
    pub(crate) tag: String,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_by_citation_key`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByCitationKeyArgs {
    /// Citation key string to match.
    pub(crate) citekey: String,
}

/// Arguments for `zotero_advanced_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions: [{"field": "title", "operator": "contains",
    /// "value": "..."}].
    pub(crate) conditions: Vec<serde_json::Value>,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_library_coverage`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct LibraryCoverageArgs {
    /// Optional collection key to scope coverage analysis.
    pub(crate) collection_key: Option<String>,
}

// --- Zotero Annotation Synthesis ---

/// Arguments for `zotero_synthesize_annotations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SynthesizeAnnotationsArgs {
    /// Zotero item key.
    pub(crate) item_key: String,
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
                .search_items(
                    &args.query,
                    args.collection_key.as_deref(),
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
        let format = args.format.as_deref().unwrap_or("json");
        if format.eq_ignore_ascii_case("bibtex") {
            let bbt_client =
                crate::better_bibtex::BetterBibtexClient::new(&self.state);
            let item_keys = vec![args.item_key.as_str()];
            Ok(super::text_result(
                bbt_client.export_items(&item_keys, "bibtex").await,
            ))
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

        let found_path = if item.data.item_type == "attachment" {
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
            let item_key = &args.item_key_or_path;
            let item = match client.get_item(item_key).await {
                Ok(item) => item,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![
                        rmcp::model::Content::text(format!(
                            "Failed to locate PDF for key '{item_key}': {e}"
                        )),
                    ]));
                }
            };

            let found_path = if item.data.item_type == "attachment" {
                item.data.path
            } else {
                client
                    .get_item_children(item_key)
                    .await
                    .ok()
                    .and_then(|children| find_pdf_path(&children))
            };

            match found_path {
                Some(p) => p,
                None => {
                    return Ok(CallToolResult::error(vec![
                        rmcp::model::Content::text(format!(
                            "No PDF file path found for key: {item_key}"
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
                    .filter(|c| c.data.item_type == "note")
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
                .create_collection(&args.name, args.parent_key.as_deref())
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
        let remove = args.remove.unwrap_or(false);
        match client
            .manage_collection_items(
                &args.collection_key,
                &args.item_keys,
                remove,
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
            client.find_duplicates(args.collection_key.as_deref()).await,
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
            client.get_library_coverage(args.collection_key.as_deref()).await,
        ))
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
}
fn find_pdf_path(children: &[ZoteroItem]) -> Option<String> {
    children.iter().find_map(|child| {
        let is_pdf = child.data.item_type == "attachment"
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
