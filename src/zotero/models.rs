//! Serde models mirroring the Zotero Local API's JSON item and collection
//! shapes.

use serde::{Deserialize, Serialize};

/// A single Zotero library item as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroItem {
    pub(crate) key: String,
    pub(crate) version: u64,
    #[serde(default)]
    pub(crate) library: serde_json::Value,
    #[serde(default)]
    pub(crate) links: serde_json::Value,
    #[serde(default)]
    pub(crate) meta: serde_json::Value,
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
    pub(crate) key: String,
    pub(crate) version: u64,
    #[serde(rename = "itemType")]
    pub(crate) item_type: String,
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) creators: Vec<ZoteroCreator>,
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
    pub(crate) extra: Option<String>,
    #[serde(default)]
    pub(crate) tags: Vec<ZoteroTag>,
    #[serde(default)]
    pub(crate) collections: Vec<String>,
    #[serde(default)]
    pub(crate) relations: serde_json::Value,
    pub(crate) date_added: Option<String>,
    pub(crate) date_modified: Option<String>,
    // For attachments
    pub(crate) parent_item: Option<String>,
    pub(crate) link_mode: Option<String>,
    #[serde(rename = "contentType")]
    pub(crate) content_type: Option<String>,
    pub(crate) charset: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) path: Option<String>,
    // For notes
    pub(crate) note: Option<String>,
}

/// An author, editor, or other creator credited on an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCreator {
    #[serde(rename = "creatorType")]
    pub(crate) creator_type: Option<String>,
    pub(crate) first_name: Option<String>,
    pub(crate) last_name: Option<String>,
    pub(crate) name: Option<String>,
}

/// A tag attached to an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroTag {
    pub(crate) tag: String,
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

/// Metadata for a [`ZoteroCollection`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollectionData {
    pub(crate) key: String,
    pub(crate) name: String,
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
    fn test_zotero_item_deserialization() {
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
    }
}
