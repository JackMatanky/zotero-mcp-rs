//! Duplicate item detection for Zotero libraries and collections.
//!
//! Provides duplicate detection routines that fetch items using
//! [`ZoteroClient`] and group them in memory by matching DOI or normalized
//! title strings. This module is called by duplicate detection MCP tool
//! handlers in `crate::mcp::zotero`.
//!
//! # Main Types
//!
//! - [`DuplicateGroup`] - Group of items identified as potential duplicates
//! - [`DuplicateType`] - Duplication criterion (`Doi` or `Title`)
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::errors::ZoteroMcpError;
//! # use zotero_mcp_rs::state::AppState;
//! # use zotero_mcp_rs::zotero::ZoteroClient;
//! # async fn example() -> Result<(), ZoteroMcpError> {
//! let state = AppState::from_env();
//! let client = ZoteroClient::new(&state);
//! let duplicates = client.find_duplicates(None).await?;
//! println!("Found {} duplicate groups", duplicates.len());
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{CollectionKey, ItemKey, ZoteroItem, client::ZoteroClient},
};

/// Type of duplication criterion matched.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DuplicateType {
    /// Matched by DOI string.
    Doi,
    /// Matched by normalized title string.
    Title,
}

/// Group of items identified as potential duplicates.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DuplicateGroup {
    /// Duplication criterion matched (`Doi` or `Title`).
    pub(crate) match_type: DuplicateType,
    /// Matched DOI or normalized title string.
    pub(crate) match_value: String,
    /// Item keys belonging to this duplicate group.
    pub(crate) item_keys: Vec<ItemKey>,
}

impl ZoteroClient<'_> {
    /// Finds potential duplicate items in the entire library or optional
    /// `collection_key` by matching title or DOI.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code
    /// - [`ZoteroMcpError::Network`] if the HTTP request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded
    pub(crate) async fn find_duplicates(
        &self,
        collection_key: Option<&CollectionKey>,
    ) -> Result<Vec<DuplicateGroup>, ZoteroMcpError> {
        let items = if let Some(col) = collection_key {
            self.get_collection_items(col).await?
        } else {
            self.get_all_items().await?
        };

        Ok(find_duplicate_groups(&items))
    }
}

/// Group items by matching DOI or title to identify potential duplicate items.
fn find_duplicate_groups(items: &[ZoteroItem]) -> Vec<DuplicateGroup> {
    let mut doi_map: std::collections::BTreeMap<String, Vec<&ZoteroItem>> =
        std::collections::BTreeMap::new();
    let mut title_map: std::collections::BTreeMap<String, Vec<&ZoteroItem>> =
        std::collections::BTreeMap::new();

    for item in items {
        if let Some(ref doi) = item.data.doi {
            if !doi.trim().is_empty() {
                doi_map
                    .entry(doi.trim().to_lowercase())
                    .or_default()
                    .push(item);
            }
        }
        if let Some(ref title) = item.data.title {
            let t = title.trim().to_lowercase();
            if t.len() > 5 {
                title_map.entry(t).or_default().push(item);
            }
        }
    }

    let mut duplicates = Vec::new();
    for (doi, grouped) in doi_map {
        if grouped.len() > 1 {
            duplicates.push(DuplicateGroup {
                match_type: DuplicateType::Doi,
                match_value: doi,
                item_keys: grouped.iter().map(|i| i.key.clone()).collect(),
            });
        }
    }
    for (title, grouped) in title_map {
        if grouped.len() > 1 {
            duplicates.push(DuplicateGroup {
                match_type: DuplicateType::Title,
                match_value: title,
                item_keys: grouped.iter().map(|i| i.key.clone()).collect(),
            });
        }
    }

    duplicates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zotero::{ItemType, LibraryVersion, objects::ZoteroItemData};

    mod duplicate_groups {
        use pretty_assertions::assert_eq;

        use super::*;

        fn make_item(
            key: &str,
            title: Option<&str>,
            doi: Option<&str>,
        ) -> ZoteroItem {
            ZoteroItem {
                key: ItemKey::from(key),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from(key),
                    version: LibraryVersion(1),
                    item_type: ItemType::JournalArticle,
                    title: title.map(ToOwned::to_owned),
                    doi: doi.map(ToOwned::to_owned),
                    ..Default::default()
                },
            }
        }

        #[test]
        fn groups_items_by_matching_doi_case_insensitively() {
            let item1 =
                make_item("ITEM0001", Some("Paper A"), Some("10.1234/XYZ"));
            let item2 =
                make_item("ITEM0002", Some("Paper B"), Some("10.1234/xyz "));

            let items = vec![item1, item2];
            let groups = find_duplicate_groups(&items);
            assert_eq!(groups.len(), 1);
            assert_eq!(
                groups.first().map(|group| group.match_type),
                Some(DuplicateType::Doi)
            );
            assert_eq!(
                groups.first().map(|group| group.match_value.as_str()),
                Some("10.1234/xyz")
            );
            assert_eq!(
                groups.first().map(|group| group.item_keys.as_slice()),
                Some(
                    &[ItemKey::from("ITEM0001"), ItemKey::from("ITEM0002")][..]
                )
            );
        }

        #[test]
        fn groups_items_by_matching_title_case_insensitively() {
            let item1 =
                make_item("ITEM0001", Some("Quantum Computing Advances"), None);
            let item2 =
                make_item("ITEM0002", Some("quantum computing advances"), None);

            let items = vec![item1, item2];
            let groups = find_duplicate_groups(&items);
            assert_eq!(groups.len(), 1);
            assert_eq!(
                groups.first().map(|group| group.match_type),
                Some(DuplicateType::Title)
            );
            assert_eq!(
                groups.first().map(|group| group.match_value.as_str()),
                Some("quantum computing advances")
            );
        }

        #[test]
        fn ignores_blank_doi_and_titles_that_are_too_short() {
            let item1 = make_item("ITEM0001", Some("AI"), Some("  "));
            let item2 = make_item("ITEM0002", Some("ai"), Some(""));

            let items = vec![item1, item2];
            let groups = find_duplicate_groups(&items);

            assert!(
                groups.is_empty(),
                "blank DOI and titles shorter than three characters must be \
                 ignored"
            );
        }

        #[test]
        fn returns_empty_when_no_duplicates_exist() {
            let item1 =
                make_item("ITEM0001", Some("Paper 1"), Some("10.1000/1"));
            let item2 =
                make_item("ITEM0002", Some("Paper 2"), Some("10.1000/2"));

            let items = vec![item1, item2];
            let groups = find_duplicate_groups(&items);
            assert!(
                groups.is_empty(),
                "distinct DOI/title pairs must not group"
            );
        }
    }

    mod scan_pagination {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::zotero::{
            client::ZoteroClient,
            test_http::{MockServer, http_response},
        };

        fn item_json(key: &str, title: &str) -> String {
            format!(
                r#"{{"key":"{key}","version":1,"data":{{"key":"{key}","version":1,"itemType":"journalArticle","title":"{title}"}}}}"#
            )
        }

        fn test_state(zotero_api_url: String) -> crate::state::AppState {
            crate::state::AppState {
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: false,
                ..crate::state::AppState::from_env()
            }
        }

        #[tokio::test]
        async fn find_duplicates_scans_more_than_one_hundred_items() {
            // 120 unique items => 2 pages (100 + 20). Two items share a title
            // ("Shared Title") so exactly one duplicate group must be found.
            let mut bodies = Vec::new();
            for i in 0..100 {
                bodies.push(item_json(
                    &format!("K{i:04}"),
                    &format!("Title {i}"),
                ));
            }
            bodies.push(item_json("K9900", "Shared Title"));
            bodies.push(item_json("K9901", "Shared Title"));
            for i in 102..120 {
                bodies.push(item_json(
                    &format!("K{i:04}"),
                    &format!("Title {i}"),
                ));
            }
            // Use take/skip (not `bodies[a..b]` slicing) to avoid the
            // `indexing_slicing` deny lint.
            let page1 = format!(
                "[{}]",
                bodies
                    .iter()
                    .take(100)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let page2 = format!(
                "[{}]",
                bodies
                    .iter()
                    .skip(100)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            );

            let server = MockServer::new(vec![
                http_response("200 OK", &page1),
                http_response("200 OK", &page2),
            ]);
            let base = server.url();
            let state = test_state(base.to_owned());

            let groups =
                ZoteroClient::new(&state).find_duplicates(None).await.unwrap();

            assert_eq!(groups.len(), 1);
            assert_eq!(
                groups.first().map(|group| group.match_type),
                Some(DuplicateType::Title)
            );
            assert_eq!(
                groups.first().map(|group| group.item_keys.len()),
                Some(2)
            );
        }
    }
}
