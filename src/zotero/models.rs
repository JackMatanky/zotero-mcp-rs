//! Serde models mirroring the Zotero Local API's JSON item and collection
//! shapes.

use serde::{Deserialize, Serialize};

/// A single Zotero library item as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroItem {
    pub(crate) key: String,
    pub(crate) version: u64,
    /// Owning library metadata object.
    #[serde(default)]
    pub(crate) library: serde_json::Value,
    /// HATEOAS API link objects.
    #[serde(default)]
    pub(crate) links: serde_json::Value,
    /// Item metadata containing creator summary and child counts.
    #[serde(default)]
    pub(crate) meta: serde_json::Value,
    pub(crate) data: ZoteroItemData,
}

/// Bibliographic and attachment fields carried by a Zotero item.
///
/// Maps Zotero's `camelCase` JSON field names across all item types
/// (`itemType`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZoteroItemData {
    pub(crate) key: String,
    #[serde(default)]
    pub(crate) version: u64,
    #[serde(rename = "itemType", default)]
    pub(crate) item_type: String,
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) creators: Vec<ZoteroCreator>,
    /// Abstract or summary text string.
    pub(crate) abstract_note: Option<String>,
    pub(crate) publication_title: Option<String>,
    pub(crate) volume: Option<String>,
    pub(crate) issue: Option<String>,
    pub(crate) pages: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) series: Option<String>,
    pub(crate) series_title: Option<String>,
    pub(crate) series_text: Option<String>,
    pub(crate) journal_abbreviation: Option<String>,
    pub(crate) doi: Option<String>,
    pub(crate) isbn: Option<String>,
    pub(crate) issn: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) access_date: Option<String>,
    pub(crate) archive: Option<String>,
    pub(crate) archive_location: Option<String>,
    pub(crate) library_catalog: Option<String>,
    pub(crate) call_number: Option<String>,
    pub(crate) rights: Option<String>,
    /// Free-form extra metadata field (e.g. citation keys or custom fields).
    pub(crate) extra: Option<String>,
    #[serde(default)]
    pub(crate) tags: Vec<ZoteroTag>,
    #[serde(default)]
    pub(crate) collections: Vec<String>,
    /// Zotero relation URIs map.
    #[serde(default)]
    pub(crate) relations: serde_json::Value,
    pub(crate) date_added: Option<String>,
    pub(crate) date_modified: Option<String>,
    /// Parent item key for attachment and child note items.
    pub(crate) parent_item: Option<String>,
    /// Attachment storage mode (e.g. `"imported_file"` or `"linked_url"`).
    pub(crate) link_mode: Option<String>,
    /// Attachment MIME content type.
    #[serde(rename = "contentType")]
    pub(crate) content_type: Option<String>,
    pub(crate) charset: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) path: Option<String>,
    /// HTML content body for note items.
    pub(crate) note: Option<String>,
    /// PDF annotation kind (e.g. `"highlight"`, `"underline"`, or `"note"`).
    #[serde(rename = "annotationType")]
    pub(crate) annotation_type: Option<String>,
    /// Selected text for PDF highlight/underline annotations.
    #[serde(rename = "annotationText")]
    pub(crate) annotation_text: Option<String>,
    /// User comment attached to a PDF annotation.
    #[serde(rename = "annotationComment")]
    pub(crate) annotation_comment: Option<String>,
    /// CSS hex color string for PDF annotations.
    #[serde(rename = "annotationColor")]
    pub(crate) annotation_color: Option<String>,
    /// PDF page label where the annotation appears.
    #[serde(rename = "annotationPageLabel")]
    pub(crate) annotation_page_label: Option<String>,
    /// Whether the item is in the trash.
    #[serde(default)]
    pub(crate) deleted: bool,
}

/// An author, editor, or other creator credited on an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCreator {
    /// Creator role (e.g. `"author"`, `"editor"`).
    #[serde(rename = "creatorType")]
    pub(crate) creator_type: Option<String>,
    #[serde(rename = "firstName")]
    pub(crate) first_name: Option<String>,
    #[serde(rename = "lastName")]
    pub(crate) last_name: Option<String>,
    /// Single-field name for institutional or single-field creators.
    pub(crate) name: Option<String>,
}

/// A tag attached to an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroTag {
    pub(crate) tag: String,
    /// Tag origin (0 = user tag, 1 = automatic tag).
    #[serde(default)]
    pub(crate) type_num: u8,
}

/// A Zotero collection as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollection {
    pub(crate) key: String,
    pub(crate) version: u64,
    pub(crate) data: ZoteroCollectionData,
}

/// Metadata payload for a [`ZoteroCollection`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollectionData {
    pub(crate) key: String,
    pub(crate) name: String,
    /// Key of parent collection, or `false` if top-level.
    #[serde(rename = "parentCollection")]
    pub(crate) parent_collection: Option<serde_json::Value>,
}

/// Result of probing the Zotero Local API for availability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalApiStatus {
    pub(crate) online: bool,
    pub(crate) url: String,
    pub(crate) version: Option<String>,
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

    #[test]
    fn deserializes_creator_camel_case_names() {
        let raw_json = serde_json::json!({
            "creatorType": "author",
            "firstName": "Ada",
            "lastName": "Lovelace"
        });

        let creator: ZoteroCreator = serde_json::from_value(raw_json).unwrap();

        assert_eq!(creator.creator_type.as_deref(), Some("author"));
        assert_eq!(creator.first_name.as_deref(), Some("Ada"));
        assert_eq!(creator.last_name.as_deref(), Some("Lovelace"));
    }

    #[test]
    fn deleted_defaults_to_false_when_absent_from_json() {
        let raw_json = serde_json::json!({
            "key": "ABC12345",
            "version": 42,
            "data": {
                "key": "ABC12345",
                "version": 42,
                "itemType": "journalArticle"
            }
        });

        let item: ZoteroItem = serde_json::from_value(raw_json).unwrap();

        assert!(!item.data.deleted);
    }

    #[test]
    fn deleted_round_trips_true() {
        let raw_json = serde_json::json!({
            "key": "ABC12345",
            "version": 42,
            "itemType": "journalArticle",
            "deleted": true
        });

        let data: ZoteroItemData = serde_json::from_value(raw_json).unwrap();
        assert!(data.deleted);

        let serialized = serde_json::to_string(&data).unwrap();
        assert!(serialized.contains("\"deleted\":true"));
    }
}
