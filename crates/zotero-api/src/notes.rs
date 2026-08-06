//! Note and annotation operations for the Zotero Local HTTP API.
//!
//! Provides methods on [`ZoteroClient`] for creating child note items and PDF
//! annotations, plus helpers for synthesizing annotations and notes into
//! structured Markdown documents.
//!
//! # Main Types
//!
//! - [`AnnotationDraft`] - Payload for creating a PDF annotation
//! - [`AnnotationPosition`] - Serialized annotation position payload
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_api::errors::ZoteroApiError;
//! # use zotero_api::AppState;
//! # use zotero_api::{ItemKey, ZoteroClient};
//! # async fn example() -> Result<(), ZoteroApiError> {
//! let state = AppState::from_env();
//! let client = ZoteroClient::new(&state);
//! let parent_key = ItemKey::from("PARENT01");
//! let note = client.create_note(&parent_key, "<p>Meeting notes</p>").await?;
//! let _serialized = serde_json::to_string(&note);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};

use crate::{
    client::ZoteroClient,
    errors::ZoteroApiError,
    keys::ItemKey,
    objects::ZoteroItem,
    types::{AnnotationType, ItemType},
};

/// Serialized Zotero annotation position payload.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct AnnotationPosition(serde_json::Value);

impl AnnotationPosition {
    /// Serializes the position to the JSON string expected by the Zotero API.
    fn as_zotero_string(&self) -> String {
        self.0.to_string()
    }
}

impl From<serde_json::Value> for AnnotationPosition {
    #[inline]
    fn from(value: serde_json::Value) -> Self {
        Self(value)
    }
}

/// Payload for creating a PDF annotation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnnotationDraft {
    /// Key of the parent PDF attachment item.
    pub parent_attachment_key: ItemKey,
    /// Annotation kind (`highlight`, `underline`, `note`, etc.).
    pub annotation_type: AnnotationType,
    /// Optional highlighted or extracted text string.
    pub text: Option<String>,
    /// Optional user comment attached to the annotation.
    pub comment: Option<String>,
    /// Optional CSS hex color string (e.g. `"#ffd400"`).
    pub color: Option<String>,
    /// Optional PDF page label where the annotation appears.
    pub page_label: Option<String>,
    /// Serialized annotation coordinates payload.
    pub position: AnnotationPosition,
}

impl ZoteroClient<'_> {
    /// Creates an HTML note item attached to `parent_item_key` with body
    /// `note_content`.
    ///
    /// Verifies write permissions and issues `POST <prefix>/items` with
    /// `itemType: "note"`.
    ///
    /// # Arguments
    ///
    /// * `parent_item_key` - Key of the parent item to attach the note to.
    /// * `note_content` - HTML or text body content for the note.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled
    ///   in [`AppState`](crate::state::AppState).
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the creation request.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if response payload decoding fails.
    #[inline]
    pub async fn create_note(
        &self,
        parent_item_key: &ItemKey,
        note_content: &str,
    ) -> Result<ZoteroItem, ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/items",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let payload = serde_json::json!([{
            "itemType": ItemType::Note,
            "parentItem": parent_item_key,
            "note": note_content,
        }]);

        self.post_json_first(&url, &payload, "Created note array was empty")
            .await
    }

    /// Creates a PDF annotation item attached to a parent PDF attachment item.
    ///
    /// Verifies write permissions and posts an `annotation` item containing
    /// type, text, comment, CSS color, page label, and position payload
    /// parameters.
    ///
    /// # Arguments
    ///
    /// * `draft` - Detailed annotation properties including parent attachment
    ///   key and coordinates.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the annotation creation
    ///   request.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if response decoding fails.
    #[inline]
    pub async fn create_annotation(
        &self,
        draft: AnnotationDraft,
    ) -> Result<ZoteroItem, ZoteroApiError> {
        self.state.check_write_permission()?;
        let position = draft.position.as_zotero_string();
        let url = format!(
            "{}{}/items",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
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

    /// Extracts and synthesizes all annotations and notes attached to
    /// `item_key` into a Markdown document.
    ///
    /// Fetches the parent item and its child items via
    /// [`get_item_children`](Self::get_item_children). Formats highlights,
    /// comments, tags, and standalone child notes into structured Markdown
    /// headings.
    ///
    /// # Arguments
    ///
    /// * `item_key` - Key of the target item.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::NotFound`] if `item_key` does not exist.
    /// - [`ZoteroApiError::LocalApi`] if fetching parent or child items fails.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn synthesize_annotations(
        &self,
        item_key: &ItemKey,
    ) -> Result<String, ZoteroApiError> {
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
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::{
        client::{
            ZoteroClient,
            test_http::{MockServer, http_response, request_body},
        },
        state::AppState,
    };

    fn state(zotero_api_url: impl AsRef<str>, write_enabled: bool) -> AppState {
        AppState::test_default()
            .with_zotero_api_url(zotero_api_url.as_ref())
            .with_write_enabled(write_enabled)
    }

    #[tokio::test]
    async fn posts_note_payload_for_parent_item() {
        let response = json!([{
            "key":"NOTE0001",
            "version":1,
            "data":{
                "key":"NOTE0001",
                "version":1,
                "itemType":"note"
            }
        }])
        .to_string();
        let (server, recorded) =
            MockServer::recording(vec![http_response("200 OK", &response)]);
        let app = state(server.url(), true);

        let result = ZoteroClient::new(&app)
            .create_note(&ItemKey::from("PARENT01"), "<p>Note</p>")
            .await;

        assert!(result.is_ok(), "note creation should succeed: {result:?}");
        let requests = recorded.lock().expect("request log lock");
        let payload = requests
            .first()
            .and_then(|request| request_body(request).ok())
            .and_then(|body| {
                body.as_array().and_then(|array| array.first()).cloned()
            })
            .unwrap_or_default();
        assert_eq!(payload.get("itemType"), Some(&json!("note")));
        assert_eq!(payload.get("parentItem"), Some(&json!("PARENT01")));
        assert_eq!(payload.get("note"), Some(&json!("<p>Note</p>")));
    }

    #[tokio::test]
    async fn denies_writes_when_write_permission_is_disabled() {
        let app = state("http://127.0.0.1:1", false);

        let result = ZoteroClient::new(&app)
            .create_note(&ItemKey::from("PARENT01"), "<p>Note</p>")
            .await;

        assert!(
            matches!(result, Err(ZoteroApiError::PermissionDenied(_))),
            "write-disabled note should fail before HTTP: {result:?}"
        );
    }

    mod annotations {
        use super::*;
        use crate::{
            keys::LibraryVersion, objects::ZoteroItemData,
            types::AnnotationType,
        };

        mod formatting {
            use pretty_assertions::assert_eq;

            use super::*;
            #[test]
            fn formats_annotations_section_with_highlights_and_notes() {
                let annotation = ZoteroItem {
                    key: ItemKey::from("ANN00001"),
                    version: LibraryVersion(1),
                    library: None,
                    links: None,
                    meta: None,
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
                    library: None,
                    links: None,
                    meta: None,
                    data: ZoteroItemData {
                        key: ItemKey::from("NOTE0001"),
                        version: LibraryVersion(1),
                        item_type: ItemType::Note,
                        note: Some("<p>Main note text</p>".to_owned()),
                        ..Default::default()
                    },
                };

                let result = format_notes_section(&note_item, &[]);

                assert_eq!(
                    result,
                    "## Note Content\n\n<p>Main note text</p>\n\n"
                );
            }

            #[test]
            fn formats_child_notes_section() {
                let main_item = ZoteroItem {
                    key: ItemKey::from("ITEM0001"),
                    version: LibraryVersion(1),
                    library: None,
                    links: None,
                    meta: None,
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
                    library: None,
                    links: None,
                    meta: None,
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
                    "## Child Notes\n\n### Note 1\n\n<p>Child note \
                     text</p>\n\n"
                );
            }

            #[test]
            fn returns_empty_when_no_annotations_or_notes() {
                let item = ZoteroItem {
                    key: ItemKey::from("ITEM0001"),
                    version: LibraryVersion(1),
                    library: None,
                    links: None,
                    meta: None,
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
}
