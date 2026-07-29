//! Response shapes returned by the Better Notes bridge's HTTP endpoints.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Result of probing the Better Notes bridge for availability.
pub(crate) struct BetterNotesStatus {
    pub(crate) online: bool,
    pub(crate) url: String,
    pub(crate) version: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
/// Response body of the Markdown-conversion endpoints.
pub(crate) struct MarkdownResponse {
    pub(crate) markdown: String,
}

#[derive(Debug, Serialize, Deserialize)]
/// Response body of the note-creation endpoint.
pub(crate) struct NoteItemResponse {
    #[serde(rename = "itemKey")]
    pub(crate) item_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
/// Response body of the template-run endpoint.
pub(crate) struct TemplateResponse {
    pub(crate) result: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
/// Response body of the note-relations endpoint.
pub(crate) struct RelationsResponse {
    pub(crate) relations: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
/// Response body of the note-tree endpoint.
pub(crate) struct NoteTreeResponse {
    pub(crate) tree: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_better_notes_status_serde() {
        let status = BetterNotesStatus {
            online: true,
            url: "http://127.0.0.1:23119/better-notes".to_owned(),
            version: Some("1.0.0".to_owned()),
            error: None,
        };
        let val = serde_json::to_value(&status).unwrap();
        assert_eq!(val.get("online"), Some(&serde_json::json!(true)));
        assert_eq!(val.get("version"), Some(&serde_json::json!("1.0.0")));
    }
}
