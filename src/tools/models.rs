#![expect(
    dead_code,
    reason = "MCP tool argument structs deserialized dynamically by rmcp router"
)]
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct EmptyArgs {}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRecentArgs {
    /// Number of items to return (default: 10, max: 100)
    pub(crate) limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query across title, creator, year, or fulltext
    pub(crate) query: String,
    /// Optional collection key to search within
    pub(crate) collection_key: Option<String>,
    /// Maximum number of results to return (default: 20)
    pub(crate) limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemMetadataArgs {
    /// Zotero item key
    pub(crate) item_key: String,
    /// Format: "json" or "bibtex" (default: "json")
    pub(crate) format: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCollectionItemsArgs {
    /// Zotero collection key
    pub(crate) collection_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemChildrenArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemFulltextArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfPathArgs {
    /// Zotero item key (parent item or attachment item)
    pub(crate) item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ReadPdfPagesArgs {
    /// Zotero item key or direct file path to PDF
    pub(crate) item_key_or_path: String,
    /// 1-based page numbers to extract (e.g. [1, 2, 3])
    pub(crate) pages: Option<Vec<usize>>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetNotesArgs {
    /// Zotero item key
    pub(crate) item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateNoteArgs {
    /// Parent item key
    pub(crate) parent_item_key: String,
    /// HTML or Markdown content for the note
    pub(crate) note_content: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateNoteArgs {
    /// Note item key
    pub(crate) note_item_key: String,
    /// Updated HTML or Markdown content
    pub(crate) note_content: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddByDoiArgs {
    /// DOI identifier (e.g. "10.1038/s41586-020-2649-2")
    pub(crate) doi: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddFromFileArgs {
    /// Absolute path to PDF, RIS, or BibTeX file
    pub(crate) file_path: String,
}

// --- Better BibTeX ---
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCitekeysArgs {
    /// Zotero item keys
    pub(crate) item_keys: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ExportItemsArgs {
    /// Zotero item keys or citation keys
    pub(crate) item_keys: Vec<String>,
    /// Translator format: "Better BibTeX", "Better BibLaTeX", or "CSL JSON"
    pub(crate) translator: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct BibliographyArgs {
    /// Zotero item keys or citation keys
    pub(crate) item_keys: Vec<String>,
    /// Citation style (e.g. "apa", "chicago-author-date")
    pub(crate) style: Option<String>,
    /// Locale (e.g. "en-US")
    pub(crate) locale: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct BetterBibtexSearchArgs {
    /// High-precision search query
    pub(crate) query: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct PandocFilterArgs {
    /// Zotero item keys or citation keys
    pub(crate) item_keys: Vec<String>,
    /// Whether to output CSL JSON metadata
    pub(crate) as_csl: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct RegenerateKeysArgs {
    /// Zotero item keys
    pub(crate) item_keys: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct AutoexportAddArgs {
    /// Collection key
    pub(crate) collection_key: String,
    /// Translator format
    pub(crate) translator: String,
    /// Output file path
    pub(crate) path: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ScanAuxArgs {
    /// Collection key
    pub(crate) collection_key: String,
    /// Local .aux file path
    pub(crate) aux_path: String,
}

// --- Better Notes ---
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ToMarkdownArgs {
    /// Note item key (optional if html provided)
    pub(crate) item_key: Option<String>,
    /// Note HTML content (optional if item_key provided)
    pub(crate) html: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct FromMarkdownArgs {
    /// Parent item key
    pub(crate) parent_key: String,
    /// Markdown string
    pub(crate) markdown: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct RunTemplateArgs {
    /// Template name or key
    pub(crate) name: String,
    /// Target Zotero item key
    pub(crate) item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteRelationsArgs {
    /// Note item key
    pub(crate) item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteTreeArgs {
    /// Note item key
    pub(crate) item_key: String,
}
