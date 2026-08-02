//! Zotero Local API JSON objects.
//!
//! Contains item, collection, creator, tag, and status payload shapes returned
//! by Zotero.

use serde::{Deserialize, Serialize};

use crate::zotero::{
    keys::{CollectionKey, ItemKey, LibraryVersion, TagName},
    types::{
        AnnotationType, CollectionParent, CreatorType, ItemType, LinkMode,
        TagOrigin,
    },
};

/// A single Zotero library item as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroItem {
    pub(crate) key: ItemKey,
    pub(crate) version: LibraryVersion,
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

/// Bibliographic, attachment, note, and annotation fields for a Zotero item.
///
/// Maps Zotero's `camelCase` JSON field names across item types.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZoteroItemData {
    pub(crate) key: ItemKey,
    #[serde(default)]
    pub(crate) version: LibraryVersion,
    #[serde(rename = "itemType", default)]
    pub(crate) item_type: ItemType,
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
    /// Zotero's native citation key field.
    ///
    /// Zotero 9 exposes this as `itemFields.citationKey`, and quicksearch can
    /// search it server-side. This field takes precedence over any
    /// `Citation Key: ...` line Better `BibTeX` may write to
    /// [`ZoteroItemData::extra`].
    #[serde(rename = "citationKey")]
    pub(crate) citation_key: Option<String>,
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
    pub(crate) collections: Vec<CollectionKey>,
    /// Map of Zotero relation predicate names to URI values.
    #[serde(default)]
    pub(crate) relations: serde_json::Value,
    pub(crate) date_added: Option<String>,
    pub(crate) date_modified: Option<String>,
    /// Parent item key for attachment and child note items.
    pub(crate) parent_item: Option<ItemKey>,
    /// Attachment storage mode (e.g. `"imported_file"` or `"linked_url"`).
    pub(crate) link_mode: Option<LinkMode>,
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
    pub(crate) annotation_type: Option<AnnotationType>,
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
    pub(crate) creator_type: Option<CreatorType>,
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
    pub(crate) tag: TagName,
    /// Tag origin: user-created vs. automatically assigned on import.
    #[serde(rename = "type", default)]
    pub(crate) origin: TagOrigin,
}

/// A Zotero collection as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollection {
    pub(crate) key: CollectionKey,
    pub(crate) version: LibraryVersion,
    pub(crate) data: ZoteroCollectionData,
}

/// Metadata payload for a [`ZoteroCollection`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollectionData {
    pub(crate) key: CollectionKey,
    pub(crate) name: String,
    /// Parent collection state.
    #[serde(rename = "parentCollection", default)]
    pub(crate) parent_collection: CollectionParent,
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
    use super::*;

    mod deserialization {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn leaves_omitted_optional_fields_as_none() {
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

            let item: ZoteroItem = serde_json::from_value(raw_json).unwrap();

            assert_eq!(item.key, "ABC12345");
            assert_eq!(
                item.data.title.as_deref(),
                Some("Quantum Computing Advances")
            );
            assert!(item.data.doi.is_none());
        }

        #[test]
        fn parses_creator_camel_case_names() {
            let raw_json = serde_json::json!({
                "creatorType": "author",
                "firstName": "Ada",
                "lastName": "Lovelace"
            });

            let creator: ZoteroCreator =
                serde_json::from_value(raw_json).unwrap();

            assert_eq!(creator.creator_type, Some(CreatorType::Author));
            assert_eq!(creator.first_name.as_deref(), Some("Ada"));
            assert_eq!(creator.last_name.as_deref(), Some("Lovelace"));
        }

        #[test]
        fn defaults_deleted_to_false_when_absent() {
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
        fn round_trips_deleted_flag() {
            let raw_json = serde_json::json!({
                "key": "ABC12345",
                "version": 42,
                "itemType": "journalArticle",
                "deleted": true
            });

            let data: ZoteroItemData =
                serde_json::from_value(raw_json).unwrap();
            assert!(data.deleted);

            let serialized = serde_json::to_string(&data).unwrap();
            assert!(serialized.contains("\"deleted\":true"));
        }

        #[test]
        fn parses_native_citation_key_field() {
            let raw_json = serde_json::json!({
                "key": "ABC12345",
                "version": 42,
                "itemType": "journalArticle",
                "citationKey": "smith2020deep"
            });

            let data: ZoteroItemData =
                serde_json::from_value(raw_json).unwrap();
            assert_eq!(data.citation_key.as_deref(), Some("smith2020deep"));
        }
        #[test]
        fn deserializes_collection_with_parent_collection_key() {
            let raw_json = serde_json::json!({
                "key": "COL12345",
                "version": 10,
                "data": {
                    "key": "COL12345",
                    "version": 10,
                    "name": "Machine Learning",
                    "parentCollection": "PARENT01"
                }
            });

            let col: ZoteroCollection =
                serde_json::from_value(raw_json).unwrap();
            assert_eq!(col.key, "COL12345");
            assert_eq!(col.data.name, "Machine Learning");
            assert_eq!(
                col.data.parent_collection,
                CollectionParent::Parent(CollectionKey::from("PARENT01"))
            );
        }
    }
}
