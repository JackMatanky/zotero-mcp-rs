//! Serde models mirroring the Zotero Local API's JSON item and collection
//! shapes.

use serde::{Deserialize, Serialize};

/// A single Zotero library item as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroItem {
    /// Unique 8-character item key.
    pub(crate) key: String,
    /// Library version counter.
    pub(crate) version: u64,
    /// Owning library metadata.
    #[serde(default)]
    pub(crate) library: serde_json::Value,
    /// API HATEOAS link objects.
    #[serde(default)]
    pub(crate) links: serde_json::Value,
    /// Metadata object (e.g. creator summary, numChildren).
    #[serde(default)]
    pub(crate) meta: serde_json::Value,
    /// Core bibliographic and type-specific data payload.
    pub(crate) data: ZoteroItemData,
}

/// Bibliographic and attachment fields carried by a Zotero item.
///
/// Maps Zotero's `camelCase` JSON field names. Covers every item type the
/// Local API can return; most fields only apply to specific item types
/// (`itemType`). Notably, `parent_item`, `link_mode`, `content_type`,
/// `charset`, `filename`, and `path` are populated only for attachments,
/// and `note` only for notes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZoteroItemData {
    /// Unique item key repeated inside the `data` payload.
    pub(crate) key: String,
    /// Item version counter repeated inside the `data` payload.
    #[serde(default)]
    pub(crate) version: u64,
    /// Zotero item type, such as `journalArticle`, `attachment`, or `note`.
    #[serde(rename = "itemType", default)]
    pub(crate) item_type: String,
    /// Item title.
    pub(crate) title: Option<String>,
    /// Creators credited on the item.
    #[serde(default)]
    pub(crate) creators: Vec<ZoteroCreator>,
    /// Abstract or summary text.
    pub(crate) abstract_note: Option<String>,
    /// Publication title, such as journal or book title.
    pub(crate) publication_title: Option<String>,
    /// Volume number.
    pub(crate) volume: Option<String>,
    /// Issue number.
    pub(crate) issue: Option<String>,
    /// Page range or locator.
    pub(crate) pages: Option<String>,
    /// Publication date string.
    pub(crate) date: Option<String>,
    /// Series name.
    pub(crate) series: Option<String>,
    /// Series title.
    pub(crate) series_title: Option<String>,
    /// Series text.
    pub(crate) series_text: Option<String>,
    /// Journal abbreviation.
    pub(crate) journal_abbreviation: Option<String>,
    /// Digital Object Identifier.
    pub(crate) doi: Option<String>,
    /// International Standard Book Number.
    pub(crate) isbn: Option<String>,
    /// International Standard Serial Number.
    pub(crate) issn: Option<String>,
    /// Source URL.
    pub(crate) url: Option<String>,
    /// Access date string.
    pub(crate) access_date: Option<String>,
    /// Archive name.
    pub(crate) archive: Option<String>,
    /// Location within an archive.
    pub(crate) archive_location: Option<String>,
    /// Library catalog source.
    pub(crate) library_catalog: Option<String>,
    /// Call number.
    pub(crate) call_number: Option<String>,
    /// Rights statement.
    pub(crate) rights: Option<String>,
    /// Free-form extra metadata field.
    pub(crate) extra: Option<String>,
    /// Tags attached to the item.
    #[serde(default)]
    pub(crate) tags: Vec<ZoteroTag>,
    /// Collection keys containing the item.
    #[serde(default)]
    pub(crate) collections: Vec<String>,
    /// Zotero relation metadata.
    #[serde(default)]
    pub(crate) relations: serde_json::Value,
    /// Item creation timestamp.
    pub(crate) date_added: Option<String>,
    /// Item last-modified timestamp.
    pub(crate) date_modified: Option<String>,
    /// Parent item key for attachments and child notes.
    pub(crate) parent_item: Option<String>,
    /// Attachment link mode.
    pub(crate) link_mode: Option<String>,
    /// Attachment MIME content type.
    #[serde(rename = "contentType")]
    pub(crate) content_type: Option<String>,
    /// Attachment character set.
    pub(crate) charset: Option<String>,
    /// Attachment filename.
    pub(crate) filename: Option<String>,
    /// Attachment file path.
    pub(crate) path: Option<String>,
    /// HTML note body.
    pub(crate) note: Option<String>,
    /// Annotation kind, such as highlight or note.
    #[serde(rename = "annotationType")]
    pub(crate) annotation_type: Option<String>,
    /// Text selected by an annotation.
    #[serde(rename = "annotationText")]
    pub(crate) annotation_text: Option<String>,
    /// User comment attached to an annotation.
    #[serde(rename = "annotationComment")]
    pub(crate) annotation_comment: Option<String>,
    /// Annotation color as a CSS-style value.
    #[serde(rename = "annotationColor")]
    pub(crate) annotation_color: Option<String>,
    /// PDF page label where the annotation appears.
    #[serde(rename = "annotationPageLabel")]
    pub(crate) annotation_page_label: Option<String>,
}

/// An author, editor, or other creator credited on an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCreator {
    /// Role of the creator (e.g. `"author"`, `"editor"`).
    #[serde(rename = "creatorType")]
    pub(crate) creator_type: Option<String>,
    /// First name or given name.
    pub(crate) first_name: Option<String>,
    /// Last name or surname.
    pub(crate) last_name: Option<String>,
    /// Single-field name for institutional/single-field creators.
    pub(crate) name: Option<String>,
}

/// A tag attached to an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroTag {
    /// Tag text string.
    pub(crate) tag: String,
    /// Tag type number (0 = user tag, 1 = automatic tag).
    #[serde(default)]
    pub(crate) type_num: u8,
}

/// A Zotero collection as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollection {
    /// Unique 8-character collection key.
    pub(crate) key: String,
    /// Collection version counter.
    pub(crate) version: u64,
    /// Collection data payload containing name and parent linkage.
    pub(crate) data: ZoteroCollectionData,
}

/// Metadata for a [`ZoteroCollection`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollectionData {
    /// Unique 8-character collection key.
    pub(crate) key: String,
    /// Name of the collection.
    pub(crate) name: String,
    /// Key of parent collection, or `false` if top-level.
    #[serde(rename = "parentCollection")]
    pub(crate) parent_collection: Option<serde_json::Value>,
}

/// Result of probing the Zotero Local API for availability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalApiStatus {
    /// Whether the Local API is online and responding.
    pub(crate) online: bool,
    /// Configured Local API URL.
    pub(crate) url: String,
    /// API version string returned in headers.
    pub(crate) version: Option<String>,
    /// Diagnostic error message if probing failed.
    pub(crate) error: Option<String>,
}
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn deserializes_item_leaving_omitted_optional_fields_as_none() {
        // Arrange
        let raw_json = serde_json::json!({
            "key": "ABC12345",
            "version": 42,
            "data": {
                "key": "ABC12345",
                "version": 42,
                "itemType": "journalArticle",
                "title": "Quantum Computing Advances"
            }
        });

        // Act
        let item: ZoteroItem = serde_json::from_value(raw_json).unwrap();

        // Assert
        assert_eq!(item.key, "ABC12345");
        assert_eq!(
            item.data.title.as_deref(),
            Some("Quantum Computing Advances")
        );
        assert!(item.data.doi.is_none());
    }
}
