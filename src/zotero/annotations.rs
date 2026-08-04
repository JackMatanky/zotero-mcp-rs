//! PDF annotation creation and annotation/note synthesis operations.
//!
//! Main types:
//! - [`AnnotationDraft`] - Payload for creating a PDF annotation
//! - [`AnnotationPosition`] - Serialized annotation position payload

use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        AnnotationType, ItemKey, ItemType, ZoteroItem, client::ZoteroClient,
    },
};

/// Serialized Zotero annotation position payload.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub(crate) struct AnnotationPosition(serde_json::Value);

impl AnnotationPosition {
    /// Serializes the position to the JSON string expected by the Zotero API.
    fn as_zotero_string(&self) -> String {
        self.0.to_string()
    }
}
impl schemars::JsonSchema for AnnotationPosition {
    #[inline]
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AnnotationPosition".into()
    }

    #[inline]
    fn json_schema(
        generator: &mut schemars::SchemaGenerator,
    ) -> schemars::Schema {
        serde_json::Value::json_schema(generator)
    }
}

impl From<serde_json::Value> for AnnotationPosition {
    fn from(value: serde_json::Value) -> Self {
        Self(value)
    }
}

/// Payload for creating a PDF annotation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct AnnotationDraft {
    pub(crate) parent_attachment_key: ItemKey,
    pub(crate) annotation_type: AnnotationType,
    pub(crate) text: Option<String>,
    pub(crate) comment: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) page_label: Option<String>,
    pub(crate) position: AnnotationPosition,
}

impl ZoteroClient<'_> {
    /// Creates a PDF annotation attached to a parent attachment item.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if response decoding fails
    pub(crate) async fn create_annotation(
        &self,
        draft: AnnotationDraft,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let position = draft.position.as_zotero_string();
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": ItemType::Annotation,
            "parentItem": draft.parent_attachment_key,
            "annotationType": draft.annotation_type,
            "annotationText": draft.text,
            "annotationComment": draft.comment.as_deref().unwrap_or(""),
            "annotationColor": draft.color.as_deref().unwrap_or("#ffd400"),
            "annotationPageLabel": draft.page_label,
            "annotationPosition": position,
        }]);
        self.post_json_first(
            &url,
            &payload,
            "Created annotation array was empty",
        )
        .await
    }

    /// Extracts and synthesizes annotations and notes for `item_key` into
    /// structured Markdown.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn synthesize_annotations(
        &self,
        item_key: &ItemKey,
    ) -> Result<String, ZoteroMcpError> {
        use std::fmt::Write as _;

        let item = self.get_item(item_key).await?;
        let children =
            self.get_item_children(item_key).await.unwrap_or_default();

        let mut md = String::new();
        let title = item.data.title.as_deref().unwrap_or(item_key.as_str());
        let _ = writeln!(md, "# Annotations & Notes: {title}\n");

        if let Some(ref doi) = item.data.doi {
            let _ = writeln!(md, "**DOI:** {doi}");
        }
        if let Some(ref date) = item.data.date {
            let _ = writeln!(md, "**Date:** {date}");
        }
        md.push('\n');
        md.push_str(&format_annotations_section(&children));
        md.push_str(&format_notes_section(&item, &children));

        Ok(md)
    }
}

/// Formats PDF annotations attached to child items into a Markdown section.
fn format_annotations_section(children: &[ZoteroItem]) -> String {
    use std::fmt::Write as _;

    let mut section = String::new();
    let annotations: Vec<_> = children
        .iter()
        .filter(|c| c.data.item_type == ItemType::Annotation)
        .collect();

    if annotations.is_empty() {
        return section;
    }

    let _ = writeln!(section, "## PDF Annotations\n");
    for ann in annotations {
        let text = ann.data.annotation_text.as_deref().unwrap_or("");
        let comment = ann.data.annotation_comment.as_deref().unwrap_or("");
        let page = ann.data.annotation_page_label.as_deref().unwrap_or("");

        if !text.is_empty() {
            if page.is_empty() {
                let _ = writeln!(section, "> \"{text}\"");
            } else {
                let _ = writeln!(section, "> \"{text}\" (p. {page})");
            }
        }
        if !comment.is_empty() {
            let _ = writeln!(section, "Comment: {comment}");
        }
        section.push('\n');
    }

    section
}

/// Formats child notes and standalone item notes into a Markdown section.
fn format_notes_section(item: &ZoteroItem, children: &[ZoteroItem]) -> String {
    use std::fmt::Write as _;

    let mut section = String::new();
    let child_notes: Vec<_> = children
        .iter()
        .filter(|c| c.data.item_type == ItemType::Note)
        .collect();

    if item.data.item_type == ItemType::Note {
        if let Some(ref note) = item.data.note {
            let _ = writeln!(section, "## Note Content\n\n{note}\n");
        }
    }

    if !child_notes.is_empty() {
        let _ = writeln!(section, "## Child Notes\n");
        for (idx, note_item) in child_notes.iter().enumerate() {
            if let Some(ref body) = note_item.data.note {
                let num = idx.saturating_add(1);
                let _ = writeln!(section, "### Note {num}\n\n{body}\n");
            }
        }
    }

    section
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zotero::{
        AnnotationType, LibraryVersion, objects::ZoteroItemData,
    };

    mod formatting {
        use pretty_assertions::assert_eq;

        use super::*;
        #[test]
        fn formats_annotations_section_with_highlights_and_notes() {
            let annotation = ZoteroItem {
                key: ItemKey::from("ANN00001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("ANN00001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::Annotation,
                    annotation_type: Some(AnnotationType::Highlight),
                    annotation_text: Some("Important concept".to_owned()),
                    annotation_comment: Some("Check this out".to_owned()),
                    annotation_page_label: Some("42".to_owned()),
                    ..Default::default()
                },
            };

            let annotations = vec![annotation];
            let result = format_annotations_section(&annotations);

            assert_eq!(
                result,
                "## PDF Annotations\n\n> \"Important concept\" (p. \
                 42)\nComment: Check this out\n\n"
            );
        }

        #[test]
        fn formats_standalone_note_section() {
            let note_item = ZoteroItem {
                key: ItemKey::from("NOTE0001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("NOTE0001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::Note,
                    note: Some("<p>Main note text</p>".to_owned()),
                    ..Default::default()
                },
            };

            let result = format_notes_section(&note_item, &[]);

            assert_eq!(result, "## Note Content\n\n<p>Main note text</p>\n\n");
        }

        #[test]
        fn formats_child_notes_section() {
            let main_item = ZoteroItem {
                key: ItemKey::from("ITEM0001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("ITEM0001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::JournalArticle,
                    ..Default::default()
                },
            };
            let child_note = ZoteroItem {
                key: ItemKey::from("NOTE0001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("NOTE0001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::Note,
                    note: Some("<p>Child note text</p>".to_owned()),
                    ..Default::default()
                },
            };

            let child_notes = vec![child_note];
            let result = format_notes_section(&main_item, &child_notes);

            assert_eq!(
                result,
                "## Child Notes\n\n### Note 1\n\n<p>Child note text</p>\n\n"
            );
        }

        #[test]
        fn returns_empty_when_no_annotations_or_notes() {
            let item = ZoteroItem {
                key: ItemKey::from("ITEM0001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("ITEM0001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::JournalArticle,
                    ..Default::default()
                },
            };

            assert_eq!(format_annotations_section(&[]), "");
            assert_eq!(format_notes_section(&item, &[]), "");
        }
    }
}
