use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BetterNotesStatus {
    pub(crate) online: bool,
    pub(crate) url: String,
    pub(crate) version: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MarkdownResponse {
    pub(crate) markdown: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NoteItemResponse {
    #[serde(rename = "itemKey")]
    pub(crate) item_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TemplateResponse {
    pub(crate) result: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RelationsResponse {
    pub(crate) relations: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NoteTreeResponse {
    pub(crate) tree: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_better_notes_status_serde() {
        let status = BetterNotesStatus {
            online: true,
            url: "http://127.0.0.1:23119/better-notes".to_string(),
            version: Some("1.0.0".to_string()),
            error: None,
        };
        let val = serde_json::to_value(&status).unwrap();
        assert_eq!(val["online"], true);
        assert_eq!(val["version"], "1.0.0");
    }
}
