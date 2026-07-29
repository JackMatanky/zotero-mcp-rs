//! Argument types for every MCP tool, deserialized by the `rmcp` router.
//!
//! Field doc comments here are not just internal rustdoc: `schemars` uses
//! them to build the JSON Schema `description` shown to MCP clients, so
//! wording is user-facing, not just for maintainers.

use schemars::JsonSchema;
use serde::Deserialize;

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

// --- Better BibTeX ---
/// Arguments for `better_bibtex_get_citekeys`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCitekeysArgs {
    /// Zotero item keys
    pub(crate) item_keys: Vec<String>,
}

/// Arguments for `better_bibtex_export_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ExportItemsArgs {
    /// Zotero item keys or citation keys
    pub(crate) item_keys: Vec<String>,
    /// Translator format: `"Better BibTeX"`, `"Better BibLaTeX"`, or `"CSL
    /// JSON"`
    pub(crate) translator: Option<String>,
}

/// Arguments for `better_bibtex_bibliography`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BibliographyArgs {
    /// Zotero item keys or citation keys
    pub(crate) item_keys: Vec<String>,
    /// Citation style (e.g. "apa", "chicago-author-date")
    pub(crate) style: Option<String>,
    /// Locale (e.g. "en-US")
    pub(crate) locale: Option<String>,
}

/// Arguments for `better_bibtex_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BetterBibtexSearchArgs {
    /// High-precision search query
    pub(crate) query: String,
}

/// Arguments for `better_bibtex_pandoc_filter`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct PandocFilterArgs {
    /// Zotero item keys or citation keys
    pub(crate) item_keys: Vec<String>,
    /// Whether to output CSL JSON metadata
    pub(crate) as_csl: Option<bool>,
}

/// Arguments for `better_bibtex_regenerate_keys`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RegenerateKeysArgs {
    /// Zotero item keys
    pub(crate) item_keys: Vec<String>,
}

/// Arguments for `better_bibtex_autoexport_add`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AutoexportAddArgs {
    /// Collection key
    pub(crate) collection_key: String,
    /// Translator format
    pub(crate) translator: String,
    /// Output file path
    pub(crate) path: String,
}

/// Arguments for `better_bibtex_scan_aux`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ScanAuxArgs {
    /// Collection key
    pub(crate) collection_key: String,
    /// Local .aux file path
    pub(crate) aux_path: String,
}

// --- Better Notes ---
/// Arguments for `better_notes_to_markdown`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ToMarkdownArgs {
    /// Note item key (optional if html provided)
    pub(crate) item_key: Option<String>,
    /// Note HTML content (optional if `item_key` provided)
    pub(crate) html: Option<String>,
}

/// Arguments for `better_notes_from_markdown`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FromMarkdownArgs {
    /// Parent item key
    pub(crate) parent_key: String,
    /// Markdown string
    pub(crate) markdown: String,
}

/// Arguments for `better_notes_run_template`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RunTemplateArgs {
    /// Template name or key
    pub(crate) name: String,
    /// Target Zotero item key
    pub(crate) item_key: String,
}

/// Arguments for `better_notes_get_relations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteRelationsArgs {
    /// Note item key
    pub(crate) item_key: String,
}

/// Arguments for `better_notes_get_tree`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteTreeArgs {
    /// Note item key
    pub(crate) item_key: String,
}
/// Arguments for `zotero_create_collection`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateCollectionArgs {
    /// Name of the collection to create
    pub(crate) name: String,
    /// Optional key of parent collection
    pub(crate) parent_collection: Option<String>,
}

/// Arguments for `zotero_search_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchCollectionsArgs {
    /// Search query string matching collection names
    pub(crate) query: String,
}

/// Arguments for `zotero_manage_collections`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ManageCollectionsArgs {
    /// Target collection key
    pub(crate) collection_key: String,
    /// List of item keys to add or remove
    pub(crate) item_keys: Vec<String>,
    /// Set to true to remove items instead of adding them
    pub(crate) remove: Option<bool>,
}
/// Arguments for `zotero_update_item`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateItemArgs {
    /// Item key to update
    pub(crate) item_key: String,
    /// JSON object containing fields to update (e.g. title, tags, abstractNote)
    pub(crate) fields: serde_json::Value,
}

/// Arguments for `zotero_attach_file`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AttachFileArgs {
    /// Parent item key to attach the file to
    pub(crate) parent_item_key: String,
    /// Title of the attachment item
    pub(crate) title: String,
    /// Local file path or URL to link
    pub(crate) path_or_url: String,
    /// Optional MIME content type (defaults to application/pdf)
    pub(crate) content_type: Option<String>,
}

/// Arguments for `zotero_batch_update_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BatchUpdateTagsArgs {
    /// Item keys to update tags on
    pub(crate) item_keys: Vec<String>,
    /// Tags to add to every target item
    pub(crate) add_tags: Option<Vec<String>>,
    /// Tags to remove from every target item
    pub(crate) remove_tags: Option<Vec<String>>,
}
/// Arguments for `zotero_find_duplicates`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key to scope duplicate detection
    pub(crate) collection_key: Option<String>,
}
/// Arguments for `zotero_search_by_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByTagArgs {
    /// Tag string to search for
    pub(crate) tag: String,
    /// Maximum number of items to return (default: 50)
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
    /// List of search conditions: `[{"field": "title"|"tag"|"doi"|"creator", "operator": "contains"|"equals"|"is_not", "value": "..."}]`
    pub(crate) conditions: Vec<serde_json::Value>,
    /// Maximum number of items to return (default: 50)
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
    /// Item key to extract annotations and notes from
    pub(crate) item_key: String,
}
/// Arguments for `search` (`ChatGPT` connector compatibility).
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchArgs {
    /// Search query string
    pub(crate) query: String,
}

/// Arguments for `fetch` (`ChatGPT` connector compatibility).
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FetchArgs {
    /// Item key or ID to fetch
    pub(crate) id: String,
}
