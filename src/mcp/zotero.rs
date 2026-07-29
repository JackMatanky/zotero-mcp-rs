//! MCP tool handlers, argument models, and unit tests for Zotero Local API tools.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, pdf::extract_pdf_pages, zotero::ZoteroClient};

// --- Argument Schemas ---

/// Arguments for tools that take no parameters.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct EmptyArgs {}

/// Arguments for `zotero_get_recent`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRecentArgs {
    /// Number of items to return (default: 10, max: 100)
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query across title, creator, year, or fulltext
    pub(crate) query: String,
    /// Optional collection key to search within
    pub(crate) collection_key: Option<String>,
    /// Maximum number of results to return (default: 20)
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_get_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

/// Arguments for `zotero_get_item_metadata`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemMetadataArgs {
    /// Zotero item key
    pub(crate) item_key: String,
    /// Format: `"json"` or `"bibtex"` (default: `"json"`)
    pub(crate) format: Option<String>,
}

/// Arguments for `zotero_get_collection_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCollectionItemsArgs {
    /// Zotero collection key
    pub(crate) collection_key: String,
}

/// Arguments for `zotero_get_item_children`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemChildrenArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

/// Arguments for `zotero_get_item_fulltext`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemFulltextArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

/// Arguments for `zotero_get_pdf_path`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfPathArgs {
    /// Zotero item key (parent item or attachment item)
    pub(crate) item_key: String,
}

/// Arguments for `zotero_read_pdf_pages`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ReadPdfPagesArgs {
    /// Zotero item key or direct file path to PDF
    pub(crate) item_key_or_path: String,
    /// 1-based page numbers to extract (e.g. [1, 2, 3])
    pub(crate) pages: Option<Vec<usize>>,
}

/// Arguments for `zotero_get_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetNotesArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

/// Arguments for `zotero_create_note`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateNoteArgs {
    /// Parent item key
    pub(crate) parent_item_key: String,
    /// HTML or Markdown content for the note
    pub(crate) note_content: String,
}

/// Arguments for `zotero_create_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateCollectionArgs {
    /// Name of the collection to create
    pub(crate) name: String,
    /// Optional key of parent collection
    pub(crate) parent_key: Option<String>,
}

/// Arguments for `zotero_search_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchCollectionsArgs {
    /// Search query matching collection names
    pub(crate) query: String,
}

/// Arguments for `zotero_manage_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ManageCollectionsArgs {
    /// Zotero collection key
    pub(crate) collection_key: String,
    /// List of item keys to add or remove
    pub(crate) item_keys: Vec<String>,
    /// Set to true to remove items instead of adding them
    pub(crate) remove: Option<bool>,
}

/// Arguments for `zotero_update_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateItemArgs {
    /// Zotero item key
    pub(crate) item_key: String,
    /// JSON object containing fields to update
    pub(crate) fields: serde_json::Value,
}

/// Arguments for `zotero_attach_file`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AttachFileArgs {
    /// Parent item key
    pub(crate) parent_item_key: String,
    /// Display title for the attachment
    pub(crate) title: String,
    /// File path or URL
    pub(crate) path_or_url: String,
    /// Optional content type (default: "application/pdf")
    pub(crate) content_type: Option<String>,
}

/// Arguments for `zotero_batch_update_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BatchUpdateTagsArgs {
    /// List of item keys
    pub(crate) item_keys: Vec<String>,
    /// Tags to add
    pub(crate) add_tags: Option<Vec<String>>,
    /// Tags to remove
    pub(crate) remove_tags: Option<Vec<String>>,
}

/// Arguments for `zotero_find_duplicates`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key to scope duplicate search
    pub(crate) collection_key: Option<String>,
}

/// Arguments for `zotero_search_by_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByTagArgs {
    /// Tag string to search for
    pub(crate) tag: String,
    /// Maximum number of items to return (default: 20)
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_by_citation_key`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByCitationKeyArgs {
    /// Citation key string to match
    pub(crate) citekey: String,
}

/// Arguments for `zotero_advanced_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions: [{"field": "title", "operator": "contains", "value": "..."}]
    pub(crate) conditions: Vec<serde_json::Value>,
    /// Maximum number of items to return (default: 20)
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_library_coverage`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct LibraryCoverageArgs {
    /// Optional collection key to scope coverage analysis
    pub(crate) collection_key: Option<String>,
}

/// Arguments for `zotero_synthesize_annotations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SynthesizeAnnotationsArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    pub(crate) async fn zotero_status_impl(
        &self,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let status = client.check_status().await;
        let json_str =
            serde_json::to_string_pretty(&status).unwrap_or_default();
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(json_str)]))
    }

    pub(crate) async fn zotero_get_recent_impl(
        &self,
        args: GetRecentArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(10).min(100);
        let client = ZoteroClient::new(&self.state);
        match client.get_recent_items(limit).await {
            Ok(items) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&items).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_search_items_impl(
        &self,
        args: SearchItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        match client
            .search_items(&args.query, args.collection_key.as_deref(), limit)
            .await
        {
            Ok(items) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&items).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_get_item_impl(
        &self,
        args: GetItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item(&args.item_key).await {
            Ok(item) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&item).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_get_item_metadata_impl(
        &self,
        args: GetItemMetadataArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let format = args.format.as_deref().unwrap_or("json");
        if format.eq_ignore_ascii_case("bibtex") {
            let bbt_client =
                crate::better_bibtex::BetterBibtexClient::new(&self.state);
            let item_keys = vec![args.item_key.as_str()];
            match bbt_client.export_items(&item_keys, "bibtex").await {
                Ok(bibtex) => Ok(CallToolResult::success(vec![
                    rmcp::model::Content::text(bibtex),
                ])),
                Err(e) => {
                    Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                        e.to_string(),
                    )]))
                }
            }
        } else {
            let client = ZoteroClient::new(&self.state);
            match client.get_item(&args.item_key).await {
                Ok(item) => Ok(CallToolResult::success(vec![
                    rmcp::model::Content::text(
                        serde_json::to_string_pretty(&item).unwrap_or_default(),
                    ),
                ])),
                Err(e) => {
                    Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                        e.to_string(),
                    )]))
                }
            }
        }
    }

    pub(crate) async fn zotero_get_collection_items_impl(
        &self,
        args: GetCollectionItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_collection_items(&args.collection_key).await {
            Ok(items) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&items).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_get_item_children_impl(
        &self,
        args: GetItemChildrenArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item_children(&args.item_key).await {
            Ok(items) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&items).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_get_item_fulltext_impl(
        &self,
        args: GetItemFulltextArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item_fulltext(&args.item_key).await {
            Ok(text) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    text,
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    #[allow(
        clippy::cognitive_complexity,
        clippy::excessive_nesting,
        reason = "pdf locator logic"
    )]
    pub(crate) async fn zotero_get_pdf_path_impl(
        &self,
        args: GetPdfPathArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item(&args.item_key).await {
            Ok(item) => {
                if item.data.item_type == "attachment" {
                    if let Some(path) = item.data.path {
                        return Ok(CallToolResult::success(vec![
                            rmcp::model::Content::text(path),
                        ]));
                    }
                }
                match client.get_item_children(&args.item_key).await {
                    Ok(children) => {
                        for child in children {
                            if child.data.item_type == "attachment" {
                                if let Some(ct) = child.data.content_type {
                                    if ct.contains("pdf") {
                                        if let Some(path) = child.data.path {
                                            return Ok(
                                                CallToolResult::success(vec![
                                                    rmcp::model::Content::text(
                                                        path,
                                                    ),
                                                ]),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Ok(CallToolResult::error(vec![
                            rmcp::model::Content::text(
                                "No PDF attachment found for item".to_owned(),
                            ),
                        ]))
                    }
                    Err(e) => Ok(CallToolResult::error(vec![
                        rmcp::model::Content::text(e.to_string()),
                    ])),
                }
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    #[allow(
        clippy::cognitive_complexity,
        clippy::excessive_nesting,
        clippy::else_if_without_else,
        reason = "pdf reader logic"
    )]
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
            match client.get_item(item_key).await {
                Ok(item) => {
                    let mut found_path = None;
                    if item.data.item_type == "attachment" {
                        found_path = item.data.path;
                    } else if let Ok(children) =
                        client.get_item_children(item_key).await
                    {
                        for child in children {
                            if child.data.item_type == "attachment" {
                                if let Some(ct) = child.data.content_type {
                                    if ct.contains("pdf") {
                                        if let Some(p) = child.data.path {
                                            found_path = Some(p);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
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
                }
                Err(e) => {
                    return Ok(CallToolResult::error(vec![
                        rmcp::model::Content::text(format!(
                            "Failed to locate PDF for key '{item_key}': {e}"
                        )),
                    ]));
                }
            }
        };

        let pages_ref = args.pages.as_deref();
        match extract_pdf_pages(std::path::Path::new(&pdf_path), pages_ref) {
            Ok(extracted) => {
                let json_str = serde_json::to_string_pretty(&extracted)
                    .unwrap_or_default();
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    json_str,
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

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
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&notes).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_create_note_impl(
        &self,
        args: CreateNoteArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client
            .create_note(&args.parent_item_key, &args.note_content)
            .await
        {
            Ok(note) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&note).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_create_collection_impl(
        &self,
        args: CreateCollectionArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client
            .create_collection(&args.name, args.parent_key.as_deref())
            .await
        {
            Ok(col) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&col).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_search_collections_impl(
        &self,
        args: SearchCollectionsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.search_collections(&args.query).await {
            Ok(cols) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&cols).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

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
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    "Collection items updated successfully".to_owned(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_update_item_impl(
        &self,
        args: UpdateItemArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.update_item(&args.item_key, args.fields).await {
            Ok(item) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&item).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_attach_file_impl(
        &self,
        args: AttachFileArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client
            .attach_file_link(
                &args.parent_item_key,
                &args.title,
                &args.path_or_url,
                args.content_type.as_deref(),
            )
            .await
        {
            Ok(item) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&item).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_batch_update_tags_impl(
        &self,
        args: BatchUpdateTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let add = args.add_tags.unwrap_or_default();
        let rem = args.remove_tags.unwrap_or_default();
        match client.batch_update_tags(&args.item_keys, &add, &rem).await {
            Ok(count) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    format!("Batch updated tags on {count} items"),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_find_duplicates_impl(
        &self,
        args: FindDuplicatesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.find_duplicates(args.collection_key.as_deref()).await {
            Ok(dups) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&dups).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_search_by_tag_impl(
        &self,
        args: SearchByTagArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        match client.search_by_tag(&args.tag, limit).await {
            Ok(items) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&items).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_search_by_citation_key_impl(
        &self,
        args: SearchByCitationKeyArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.search_by_citation_key(&args.citekey).await {
            Ok(item) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&item).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_advanced_search_impl(
        &self,
        args: AdvancedSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        match client.advanced_search(args.conditions, limit).await {
            Ok(items) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&items).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_library_coverage_impl(
        &self,
        args: LibraryCoverageArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_library_coverage(args.collection_key.as_deref()).await
        {
            Ok(cov) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&cov).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn zotero_synthesize_annotations_impl(
        &self,
        args: SynthesizeAnnotationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.synthesize_annotations(&args.item_key).await {
            Ok(md) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    md,
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }
}
