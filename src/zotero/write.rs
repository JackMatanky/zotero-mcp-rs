//! Write and mutation operations for the Zotero Local HTTP API.

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        client::ZoteroClient,
        models::{ZoteroCollection, ZoteroItem},
    },
};

impl ZoteroClient<'_> {
    /// Creates a note item attached to `parent_item_key` with body `note_content`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn create_note(
        &self,
        parent_item_key: &str,
        note_content: &str,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": "note",
            "parentItem": parent_item_key,
            "note": note_content,
        }]);

        let resp = self
            .state
            .send_with_retry(self.state.client.post(&url).json(&payload))
            .await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let created: Vec<ZoteroItem> = resp.json().await?;
        created.into_iter().next().ok_or_else(|| ZoteroMcpError::LocalApi {
            status: 500,
            message: "Created note array was empty".to_owned(),
        })
    }

    /// Creates a new collection with name `name` and optional `parent_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn create_collection(
        &self,
        name: &str,
        parent_key: Option<&str>,
    ) -> Result<ZoteroCollection, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/collections", self.state.zotero_api_url);
        let parent_val = match parent_key {
            Some(k) => serde_json::Value::String(k.to_owned()),
            None => serde_json::Value::String("false".to_owned()),
        };
        let payload = serde_json::json!([{
            "name": name,
            "parentCollection": parent_val,
        }]);

        let resp = self
            .state
            .send_with_retry(self.state.client.post(&url).json(&payload))
            .await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let created: Vec<ZoteroCollection> = resp.json().await?;
        created.into_iter().next().ok_or_else(|| ZoteroMcpError::LocalApi {
            status: 500,
            message: "Created collection array was empty".to_owned(),
        })
    }

    /// Searches collections by `query` matching collection names case-insensitively.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn search_collections(
        &self,
        query: &str,
    ) -> Result<Vec<ZoteroCollection>, ZoteroMcpError> {
        let collections = self.get_collections().await?;
        let query_lc = query.to_lowercase();
        let filtered = collections
            .into_iter()
            .filter(|c| c.data.name.to_lowercase().contains(&query_lc))
            .collect();
        Ok(filtered)
    }

    /// Adds or removes item keys to/from a collection.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn manage_collection_items(
        &self,
        collection_key: &str,
        item_keys: &[String],
        remove: bool,
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}/users/0/collections/{}/items",
            self.state.zotero_api_url, collection_key
        );
        let body_str = item_keys.join(" ");

        let req = if remove {
            self.state.client.delete(&url).body(body_str)
        } else {
            self.state.client.post(&url).body(body_str)
        };

        let resp = self.state.send_with_retry(req).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        Ok(())
    }

    /// Updates fields of an existing item using `PATCH`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn update_item(
        &self,
        item_key: &str,
        fields: serde_json::Value,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url =
            format!("{}/users/0/items/{}", self.state.zotero_api_url, item_key);
        let resp = self
            .state
            .send_with_retry(self.state.client.patch(&url).json(&fields))
            .await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        if let Ok(item) = resp.json::<ZoteroItem>().await {
            Ok(item)
        } else {
            self.get_item(item_key).await
        }
    }

    /// Attaches a linked file to a parent item.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn attach_file_link(
        &self,
        parent_item_key: &str,
        title: &str,
        file_path_or_url: &str,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": "attachment",
            "parentItem": parent_item_key,
            "title": title,
            "linkMode": "imported_file",
            "path": file_path_or_url,
            "contentType": content_type.unwrap_or("application/pdf"),
        }]);

        let resp = self
            .state
            .send_with_retry(self.state.client.post(&url).json(&payload))
            .await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let created: Vec<ZoteroItem> = resp.json().await?;
        created.into_iter().next().ok_or_else(|| ZoteroMcpError::LocalApi {
            status: 500,
            message: "Created attachment array was empty".to_owned(),
        })
    }

    /// Batch updates tags across multiple items by adding and/or removing tags.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn batch_update_tags(
        &self,
        item_keys: &[String],
        add_tags: &[String],
        remove_tags: &[String],
    ) -> Result<usize, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let mut count: usize = 0;
        for key in item_keys {
            let item = self.get_item(key).await?;
            let mut tags_set: std::collections::BTreeSet<String> =
                item.data.tags.into_iter().map(|t| t.tag).collect();
            for add in add_tags {
                tags_set.insert(add.clone());
            }
            for rem in remove_tags {
                tags_set.remove(rem);
            }
            let new_tags: Vec<serde_json::Value> = tags_set
                .into_iter()
                .map(|t| serde_json::json!({ "tag": t }))
                .collect();
            let patch_payload = serde_json::json!({
                "tags": new_tags,
                "version": item.version,
            });
            self.update_item(key, patch_payload).await?;
            count = count.saturating_add(1);
        }
        Ok(count)
    }

    /// Finds potential duplicate items in library or collection by matching title or DOI.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn find_duplicates(
        &self,
        collection_key: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, ZoteroMcpError> {
        let items = if let Some(col) = collection_key {
            self.get_collection_items(col).await?
        } else {
            let url = format!(
                "{}/users/0/items?limit=100",
                self.state.zotero_api_url
            );
            let resp =
                self.state.send_with_retry(self.state.client.get(&url)).await?;
            if !resp.status().is_success() {
                return Err(ZoteroMcpError::LocalApi {
                    status: resp.status().as_u16(),
                    message: resp.text().await.unwrap_or_default(),
                });
            }
            resp.json().await?
        };

        let mut doi_map: std::collections::BTreeMap<String, Vec<&ZoteroItem>> =
            std::collections::BTreeMap::new();
        let mut title_map: std::collections::BTreeMap<
            String,
            Vec<&ZoteroItem>,
        > = std::collections::BTreeMap::new();

        for item in &items {
            if let Some(ref doi) = item.data.doi {
                let clean_doi = doi.trim().to_lowercase();
                if !clean_doi.is_empty() {
                    doi_map.entry(clean_doi).or_default().push(item);
                }
            }
            if let Some(ref title) = item.data.title {
                let clean_title: String = title
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
                    .to_lowercase();
                if clean_title.len() >= 3 {
                    title_map.entry(clean_title).or_default().push(item);
                }
            }
        }

        let mut duplicates = Vec::new();
        for (doi, grouped) in doi_map {
            if grouped.len() > 1 {
                duplicates.push(serde_json::json!({
                    "reason": "matching_doi",
                    "match_key": doi,
                    "count": grouped.len(),
                    "items": grouped,
                }));
            }
        }
        for (title, grouped) in title_map {
            if grouped.len() > 1 {
                duplicates.push(serde_json::json!({
                    "reason": "matching_title",
                    "match_key": title,
                    "count": grouped.len(),
                    "items": grouped,
                }));
            }
        }

        Ok(duplicates)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::{
        super::client::tests::fixtures::{
            http_response, mock_server, test_state,
        },
        *,
    };

    #[tokio::test]
    async fn create_note_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err = ZoteroClient::new(&state)
            .create_note("PARENT1", "note body")
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn create_note_returns_created_item_on_success() {
        let created = json!([{
            "key": "NOTE1",
            "version": 1,
            "data": { "key": "NOTE1", "version": 1, "itemType": "note", "note": "note body" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let state = test_state(base, true);

        let item = ZoteroClient::new(&state)
            .create_note("PARENT1", "note body")
            .await
            .unwrap();
        assert_eq!(item.key, "NOTE1");
    }

    #[tokio::test]
    async fn create_collection_returns_created_collection_on_success() {
        let created = json!([{
            "key": "COL1",
            "version": 1,
            "data": { "key": "COL1", "name": "New Collection", "parentCollection": "false" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let state = test_state(base, true);

        let col = ZoteroClient::new(&state)
            .create_collection("New Collection", None)
            .await
            .unwrap();
        assert_eq!(col.key, "COL1");
    }

    #[tokio::test]
    async fn search_collections_returns_matching_items() {
        let collections = json!([
            { "key": "C1", "version": 1, "data": { "key": "C1", "name": "Quantum Physics" } },
            { "key": "C2", "version": 1, "data": { "key": "C2", "name": "Quantum Mechanics" } },
            { "key": "C3", "version": 1, "data": { "key": "C3", "name": "Biology" } }
        ]);
        let base = mock_server(vec![http_response(
            "200 OK",
            &collections.to_string(),
        )]);
        let state = test_state(base, false);

        let results = ZoteroClient::new(&state)
            .search_collections("quantum")
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn manage_collection_items_adds_and_removes() {
        let base = mock_server(vec![
            http_response("200 OK", ""),
            http_response("200 OK", ""),
        ]);
        let state = test_state(base, true);

        let res_add = ZoteroClient::new(&state)
            .manage_collection_items(
                "COL1",
                &["ITEM1".to_owned(), "ITEM2".to_owned()],
                false,
            )
            .await;
        assert!(res_add.is_ok());

        let res_rem = ZoteroClient::new(&state)
            .manage_collection_items("COL1", &["ITEM1".to_owned()], true)
            .await;
        assert!(res_rem.is_ok());
    }

    #[tokio::test]
    async fn update_item_updates_fields() {
        let item = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "title": "Updated Title" }
        });
        let base =
            mock_server(vec![http_response("200 OK", &item.to_string())]);
        let state = test_state(base, true);

        let res = ZoteroClient::new(&state)
            .update_item("ITEM1", json!({"title": "Updated Title"}))
            .await
            .unwrap();
        assert_eq!(res.key, "ITEM1");
    }

    #[tokio::test]
    async fn attach_file_link_creates_attachment() {
        let created = json!([{
            "key": "ATT1",
            "version": 1,
            "data": { "key": "ATT1", "version": 1, "itemType": "attachment", "title": "PDF Attachment" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let state = test_state(base, true);

        let item = ZoteroClient::new(&state)
            .attach_file_link("ITEM1", "PDF Attachment", "/path/file.pdf", None)
            .await
            .unwrap();
        assert_eq!(item.key, "ATT1");
    }

    #[tokio::test]
    async fn batch_update_tags_updates_tags_across_items() {
        let item = json!({
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Paper", "tags": [{ "tag": "old_tag" }] }
        });
        let updated = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "title": "Paper", "tags": [{ "tag": "new_tag" }] }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let state = test_state(base, true);

        let count = ZoteroClient::new(&state)
            .batch_update_tags(
                &["ITEM1".to_owned()],
                &["new_tag".to_owned()],
                &["old_tag".to_owned()],
            )
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn find_duplicates_detects_duplicates_by_title_and_doi() {
        let items = json!([
            {
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Unique Article Title", "doi": "10.1234/unique" }
            },
            {
                "key": "ITEM2",
                "version": 1,
                "data": { "key": "ITEM2", "itemType": "journalArticle", "title": "Unique Article Title", "doi": "10.1234/unique" }
            }
        ]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let duplicates =
            ZoteroClient::new(&state).find_duplicates(None).await.unwrap();
        assert_eq!(duplicates.len(), 2);
    }
}
