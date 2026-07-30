//! Response shapes returned by the Better Notes bridge's HTTP endpoints.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::zotero::ItemKey;

/// Output format for Better Notes note export.
#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Deserialize,
    Serialize,
    JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BetterNotesFormat {
    /// Return Markdown content.
    #[default]
    Markdown,
    /// Return HTML content.
    Html,
}

impl BetterNotesFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Html => "html",
        }
    }
}

/// Response body of the Markdown-conversion endpoints.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MarkdownResponse {
    /// Converted Markdown text content.
    pub(crate) markdown: String,
}

/// Response body of the note-creation endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NoteItemResponse {
    /// Item key of the created note.
    #[serde(rename = "itemKey")]
    pub(crate) item_key: ItemKey,
}

/// Response body of the template-run endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TemplateResponse {
    /// JSON value result produced by running the template.
    pub(crate) result: serde_json::Value,
}

/// Response body of the note-relations endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RelationsResponse {
    /// JSON array or object containing note relation linkages.
    pub(crate) relations: serde_json::Value,
}

/// Response body of the note-tree endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NoteTreeResponse {
    /// Hierarchical tree structure of notes as JSON.
    pub(crate) tree: serde_json::Value,
}
