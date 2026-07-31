//! Response shapes returned by the Better Notes bridge's HTTP endpoints.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::zotero::ItemKey;

/// Name of a Better Notes template, e.g. `"default"` or a custom template name.
#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Deserialize,
    Serialize,
    JsonSchema,
)]
#[serde(transparent)]
pub(crate) struct TemplateName(pub(crate) String);

impl TemplateName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TemplateName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for TemplateName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for TemplateName {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl AsRef<str> for TemplateName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for TemplateName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

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
pub(crate) enum NoteExportFormat {
    /// Return Markdown content.
    #[default]
    Markdown,
    /// Return HTML content.
    Html,
}

impl NoteExportFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Html => "html",
        }
    }
}

/// Response body of the note-export endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NoteExportResponse {
    /// Exported note content.
    pub(crate) content: String,
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
    /// Rendered template output.
    pub(crate) result: String,
}

/// Response body of the note-relations endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RelationsResponse {
    /// Inbound and outbound note relation linkages.
    pub(crate) relations: NoteRelations,
}

/// Inbound and outbound note-link relation sets.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NoteRelations {
    /// Links from this note to other notes.
    pub(crate) outbound: Vec<NoteRelationLink>,
    /// Links from other notes to this note.
    pub(crate) inbound: Vec<NoteRelationLink>,
}

/// One directed Better Notes note-link relation.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteRelationLink {
    /// Library ID of the source note.
    #[serde(rename = "fromLibID")]
    pub(crate) from_lib_id: u64,
    /// Item key of the source note.
    pub(crate) from_key: ItemKey,
    /// Library ID of the target note.
    #[serde(rename = "toLibID")]
    pub(crate) to_lib_id: u64,
    /// Item key of the target note.
    pub(crate) to_key: ItemKey,
    /// Line index containing the source link.
    pub(crate) from_line: u64,
    /// Target line index, if the link targets a line.
    pub(crate) to_line: Option<u64>,
    /// Target heading section, if the link targets a section.
    pub(crate) to_section: Option<String>,
    /// Raw `zotero://note/...` URL.
    pub(crate) url: String,
}

/// Response body of the note-tree endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NoteTreeResponse {
    /// Hierarchical tree structure of notes as JSON.
    pub(crate) tree: serde_json::Value,
}
