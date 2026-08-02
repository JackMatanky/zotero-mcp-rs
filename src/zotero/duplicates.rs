//! Duplicate item detection for Zotero libraries and collections.

use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{CollectionKey, ItemKey, ZoteroItem, client::ZoteroClient},
};

/// Type of duplication criterion matched.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DuplicateType {
    Doi,
    Title,
}

/// Group of items identified as potential duplicates.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DuplicateGroup {
    pub(crate) match_type: DuplicateType,
    pub(crate) match_value: String,
    pub(crate) item_keys: Vec<ItemKey>,
}

impl ZoteroClient<'_> {
    /// Finds potential duplicate items in the library or optional
    /// `collection_key` by matching title or DOI.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
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

            let groups = find_duplicate_groups(&vec![item1, item2]);
            assert_eq!(groups.len(), 1);
            let first_group = groups.first().expect("group exists");
            assert_eq!(first_group.match_type, DuplicateType::Doi);
            assert_eq!(first_group.match_value, "10.1234/xyz");
            assert_eq!(first_group.item_keys, vec![
                ItemKey::from("ITEM0001"),
                ItemKey::from("ITEM0002")
            ]);
        }

        #[test]
        fn groups_items_by_matching_title_case_insensitively() {
            let item1 =
                make_item("ITEM0001", Some("Quantum Computing Advances"), None);
            let item2 =
                make_item("ITEM0002", Some("quantum computing advances"), None);

            let groups = find_duplicate_groups(&vec![item1, item2]);
            assert_eq!(groups.len(), 1);
            let first_group = groups.first().expect("group exists");
            assert_eq!(first_group.match_type, DuplicateType::Title);
            assert_eq!(first_group.match_value, "quantum computing advances");
        }

        #[test]
        fn returns_empty_when_no_duplicates_exist() {
            let item1 =
                make_item("ITEM0001", Some("Paper 1"), Some("10.1000/1"));
            let item2 =
                make_item("ITEM0002", Some("Paper 2"), Some("10.1000/2"));

            let groups = find_duplicate_groups(&vec![item1, item2]);
            assert!(groups.is_empty());
        }
    }

    mod scan_pagination {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::zotero::client::ZoteroClient;

        fn item_json(key: &str, title: &str) -> String {
            format!(
                r#"{{"key":"{key}","version":1,"data":{{"key":"{key}","version":1,"itemType":"journalArticle","title":"{title}"}}}}"#
            )
        }

        fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        fn mock_server(responses: Vec<String>) -> String {
            use std::{
                io::{Read, Write},
                net::TcpListener,
            };
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) = listener.accept().expect("accept");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
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

            let base = mock_server(vec![
                http_response("200 OK", &page1),
                http_response("200 OK", &page2),
            ]);
            let state = test_state(base);

            let groups =
                ZoteroClient::new(&state).find_duplicates(None).await.unwrap();

            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].match_type, DuplicateType::Title);
            assert_eq!(groups[0].item_keys.len(), 2);
        }
    }
}
