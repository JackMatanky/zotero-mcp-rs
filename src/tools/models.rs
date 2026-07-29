#![expect(
    dead_code,
    reason = "MCP tool argument structs deserialized dynamically by rmcp router"
)]
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct EmptyArgs {}

#[derive(Deserialize, JsonSchema)]
pub struct GetRecentArgs {
    /// Number of items to return (default: 10, max: 100)
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchItemsArgs {
    /// Search query across title, creator, year, or fulltext
    pub query: String,
    /// Optional collection key to search within
    pub collection_key: Option<String>,
    /// Maximum number of results to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetItemArgs {
    /// Zotero item key
    pub item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetItemMetadataArgs {
    /// Zotero item key
    pub item_key: String,
    /// Format: "json" or "bibtex" (default: "json")
    pub format: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetCollectionItemsArgs {
    /// Zotero collection key
    pub collection_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetItemChildrenArgs {
    /// Zotero item key
    pub item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetItemFulltextArgs {
    /// Zotero item key
    pub item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetPdfPathArgs {
    /// Zotero item key (parent item or attachment item)
    pub item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReadPdfPagesArgs {
    /// Zotero item key or direct file path to PDF
    pub item_key_or_path: String,
    /// 1-based page numbers to extract (e.g. [1, 2, 3])
    pub pages: Option<Vec<usize>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetNotesArgs {
    /// Zotero item key
    pub item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateNoteArgs {
    /// Parent item key
    pub parent_item_key: String,
    /// HTML or Markdown content for the note
    pub note_content: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateNoteArgs {
    /// Note item key
    pub note_item_key: String,
    /// Updated HTML or Markdown content
    pub note_content: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct AddByDoiArgs {
    /// DOI identifier (e.g. "10.1038/s41586-020-2649-2")
    pub doi: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct AddFromFileArgs {
    /// Absolute path to PDF, RIS, or BibTeX file
    pub file_path: String,
}

// --- Better BibTeX ---
#[derive(Deserialize, JsonSchema)]
pub struct GetCitekeysArgs {
    /// Zotero item keys
    pub item_keys: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ExportItemsArgs {
    /// Zotero item keys or citation keys
    pub item_keys: Vec<String>,
    /// Translator format: "Better BibTeX", "Better BibLaTeX", or "CSL JSON"
    pub translator: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct BibliographyArgs {
    /// Zotero item keys or citation keys
    pub item_keys: Vec<String>,
    /// Citation style (e.g. "apa", "chicago-author-date")
    pub style: Option<String>,
    /// Locale (e.g. "en-US")
    pub locale: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct BetterBibtexSearchArgs {
    /// High-precision search query
    pub query: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct PandocFilterArgs {
    /// Zotero item keys or citation keys
    pub item_keys: Vec<String>,
    /// Whether to output CSL JSON metadata
    pub as_csl: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RegenerateKeysArgs {
    /// Zotero item keys
    pub item_keys: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AutoexportAddArgs {
    /// Collection key
    pub collection_key: String,
    /// Translator format
    pub translator: String,
    /// Output file path
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ScanAuxArgs {
    /// Collection key
    pub collection_key: String,
    /// Local .aux file path
    pub aux_path: String,
}

// --- Better Notes ---
#[derive(Deserialize, JsonSchema)]
pub struct ToMarkdownArgs {
    /// Note item key (optional if html provided)
    pub item_key: Option<String>,
    /// Note HTML content (optional if item_key provided)
    pub html: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct FromMarkdownArgs {
    /// Parent item key
    pub parent_key: String,
    /// Markdown string
    pub markdown: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct RunTemplateArgs {
    /// Template name or key
    pub name: String,
    /// Target Zotero item key
    pub item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteRelationsArgs {
    /// Note item key
    pub item_key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteTreeArgs {
    /// Note item key
    pub item_key: String,
}
