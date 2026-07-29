use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroItem {
    pub key: String,
    pub version: u64,
    #[serde(default)]
    pub library: serde_json::Value,
    #[serde(default)]
    pub links: serde_json::Value,
    #[serde(default)]
    pub meta: serde_json::Value,
    pub data: ZoteroItemData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroItemData {
    pub key: String,
    pub version: u64,
    #[serde(rename = "itemType")]
    pub item_type: String,
    pub title: Option<String>,
    #[serde(default)]
    pub creators: Vec<ZoteroCreator>,
    pub abstract_note: Option<String>,
    pub publication_title: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub date: Option<String>,
    pub series: Option<String>,
    pub series_title: Option<String>,
    pub series_text: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub doi: Option<String>,
    pub isbn: Option<String>,
    pub issn: Option<String>,
    pub url: Option<String>,
    pub access_date: Option<String>,
    pub archive: Option<String>,
    pub archive_location: Option<String>,
    pub library_catalog: Option<String>,
    pub call_number: Option<String>,
    pub rights: Option<String>,
    pub extra: Option<String>,
    #[serde(default)]
    pub tags: Vec<ZoteroTag>,
    #[serde(default)]
    pub collections: Vec<String>,
    #[serde(default)]
    pub relations: serde_json::Value,
    pub date_added: Option<String>,
    pub date_modified: Option<String>,
    // For attachments
    pub parent_item: Option<String>,
    pub link_mode: Option<String>,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
    pub charset: Option<String>,
    pub filename: Option<String>,
    pub path: Option<String>,
    // For notes
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroCreator {
    #[serde(rename = "creatorType")]
    pub creator_type: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroTag {
    pub tag: String,
    #[serde(default)]
    pub type_num: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroCollection {
    pub key: String,
    pub version: u64,
    pub data: ZoteroCollectionData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroCollectionData {
    pub key: String,
    pub name: String,
    #[serde(rename = "parentCollection")]
    pub parent_collection: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalApiStatus {
    pub online: bool,
    pub url: String,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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
