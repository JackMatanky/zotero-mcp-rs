//! Async client for the Zotero Local HTTP API.
//!
//! Thin wrapper around [`reqwest`] calls to the local Zotero library server,
//! using [`AppState::send_with_retry`] for transient-failure retries.

use reqwest::StatusCode;

use crate::{
    errors::ZoteroMcpError,
    state::AppState,
    zotero::models::{LocalApiStatus, ZoteroCollection, ZoteroItem},
};

/// Client for the Zotero Local HTTP API, scoped to a single tool call.
pub(crate) struct ZoteroClient<'a> {
    state: &'a AppState,
}

impl<'a> ZoteroClient<'a> {
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
        }
    }

    /// Probes the Zotero Local API for availability.
    ///
    /// Issues a lightweight `items?limit=1` request. Never returns an
    /// error: connection and non-2xx failures are captured in the returned
    /// [`LocalApiStatus::error`] field instead of being propagated, so
    /// callers can always surface a diagnostic result.
    pub(crate) async fn check_status(&self) -> LocalApiStatus {
        let url =
            format!("{}/users/0/items?limit=1", self.state.zotero_api_url);
        match self.state.client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    LocalApiStatus {
                        online: true,
                        url: self.state.zotero_api_url.clone(),
                        version: resp
                            .headers()
                            .get("zotero-api-version")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_owned),
                        error: None,
                    }
                } else {
                    LocalApiStatus {
                        online: false,
                        url: self.state.zotero_api_url.clone(),
                        version: None,
                        error: Some(format!("HTTP status {status}")),
                    }
                }
            }
            Err(e) => LocalApiStatus {
                online: false,
                url: self.state.zotero_api_url.clone(),
                version: None,
                error: Some(e.to_string()),
            },
        }
    }

    /// Fetches the `limit` most recently modified library items (notes
    /// excluded).
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(crate) async fn get_recent_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items?limit={}&sort=dateModified&direction=desc&\
             itemType=-note",
            self.state.zotero_api_url, limit
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Searches library items by `query` (title, creator, year, or
    /// fulltext), optionally scoped to `collection_key`, returning at most
    /// `limit` results. Notes are excluded.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(crate) async fn search_items(
        &self,
        query: &str,
        collection_key: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let base = match collection_key {
            Some(col) => format!(
                "{}/users/0/collections/{}/items",
                self.state.zotero_api_url, col
            ),
            None => format!("{}/users/0/items", self.state.zotero_api_url),
        };
        let encoded_q = urlencoding::encode(query);
        let url = format!("{base}?q={encoded_q}&limit={limit}&itemType=-note");

        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Fetches the item identified by `item_key`.
    ///
    /// # Errors
    ///
    /// - [`NotFound`] if no item with that key exists
    /// - [`LocalApi`] if the Local API responds with another non-2xx status
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`NotFound`]: ZoteroMcpError::NotFound
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(crate) async fn get_item(
        &self,
        item_key: &str,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        let url =
            format!("{}/users/0/items/{}", self.state.zotero_api_url, item_key);
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ZoteroMcpError::NotFound(format!("Item {item_key}")));
        }
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let item: ZoteroItem = resp.json().await?;
        Ok(item)
    }

    /// Fetches every collection in the library.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(crate) async fn get_collections(
        &self,
    ) -> Result<Vec<ZoteroCollection>, ZoteroMcpError> {
        let url = format!("{}/users/0/collections", self.state.zotero_api_url);
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let collections: Vec<ZoteroCollection> = resp.json().await?;
        Ok(collections)
    }

    /// Fetches every item inside the collection identified by
    /// `collection_key`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(crate) async fn get_collection_items(
        &self,
        collection_key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/collections/{}/items",
            self.state.zotero_api_url, collection_key
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Fetches the child items (notes and attachments) of `item_key`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(crate) async fn get_item_children(
        &self,
        item_key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/children",
            self.state.zotero_api_url, item_key
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Fetches Zotero's indexed fulltext content for `item_key`, or an
    /// empty string if none has been indexed.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(crate) async fn get_item_fulltext(
        &self,
        item_key: &str,
    ) -> Result<String, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/fulltext",
            self.state.zotero_api_url, item_key
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let val: serde_json::Value = resp.json().await?;
        let content = val
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        Ok(content)
    }

    /// Creates a note item attached to `parent_item_key` with body
    /// `note_content`, returning the created item.
    ///
    /// Assumes the caller has already enforced
    /// [`AppState::check_write_permission`]; this method re-checks it
    /// itself before issuing the write.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`LocalApi`] if the Local API responds with a non-2xx status, or
    ///   returns an empty result for the created note
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
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
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`LocalApi`] if the Local API responds with non-2xx or empty array
    /// - [`Network`] if transport fails
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
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
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
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
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
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
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
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`LocalApi`] if the Local API responds with non-2xx or empty array
    /// - [`Network`] if transport fails
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
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
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
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
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
    /// Searches items by tag name.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
    pub(crate) async fn search_by_tag(
        &self,
        tag: &str,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let encoded_tag = urlencoding::encode(tag);
        let url = format!(
            "{}/users/0/items?tag={}&limit={}&itemType=-note",
            self.state.zotero_api_url, encoded_tag, limit
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Searches items by citation key in `extra` field or query.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
    pub(crate) async fn search_by_citation_key(
        &self,
        citekey: &str,
    ) -> Result<Option<ZoteroItem>, ZoteroMcpError> {
        let items = self.search_items(citekey, None, 20).await?;
        let citekey_lc = citekey.to_lowercase();
        for item in items {
            if let Some(ref extra) = item.data.extra {
                let extra_lc = extra.to_lowercase();
                if extra_lc.contains(&format!("citation key: {citekey_lc}"))
                    || extra_lc.contains(&format!("citationkey: {citekey_lc}"))
                    || extra_lc.contains(&citekey_lc)
                {
                    return Ok(Some(item));
                }
            }
        }
        Ok(None)
    }
    /// Advanced multi-condition structured search over item fields.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
    pub(crate) async fn advanced_search(
        &self,
        conditions: Vec<serde_json::Value>,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let items = self.get_recent_items(100).await?;
        let mut results = Vec::new();

        for item in items {
            let mut matches_all = true;
            for cond in &conditions {
                let field =
                    cond.get("field").and_then(|v| v.as_str()).unwrap_or("");
                let op = cond
                    .get("operator")
                    .and_then(|v| v.as_str())
                    .unwrap_or("contains");
                let val = cond
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();

                let val_matched = match field {
                    "itemType" | "item_type" => {
                        item.data.item_type.to_lowercase()
                    }
                    "doi" => {
                        item.data.doi.as_deref().unwrap_or("").to_lowercase()
                    }
                    "year" | "date" => {
                        item.data.date.as_deref().unwrap_or("").to_lowercase()
                    }
                    "tag" => item
                        .data
                        .tags
                        .iter()
                        .map(|t| t.tag.to_lowercase())
                        .collect::<Vec<_>>()
                        .join(" "),
                    "abstract" | "abstractNote" => item
                        .data
                        .abstract_note
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase(),
                    "creator" | "author" => item
                        .data
                        .creators
                        .iter()
                        .map(|c| {
                            format!(
                                "{} {}",
                                c.first_name.as_deref().unwrap_or(""),
                                c.last_name.as_deref().unwrap_or("")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase(),
                    _ => {
                        item.data.title.as_deref().unwrap_or("").to_lowercase()
                    }
                };

                let cond_pass = match op {
                    "equals" => val_matched == val,
                    "is_not" => !val_matched.contains(&val),
                    _ => val_matched.contains(&val),
                };

                if !cond_pass {
                    matches_all = false;
                    break;
                }
            }

            if matches_all {
                results.push(item);
                if results.len() >= limit {
                    break;
                }
            }
        }

        Ok(results)
    }

    /// Computes library or collection coverage statistics (PDF, DOI, Notes).
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
    pub(crate) async fn get_library_coverage(
        &self,
        collection_key: Option<&str>,
    ) -> Result<serde_json::Value, ZoteroMcpError> {
        let items = match collection_key {
            Some(col) => self.get_collection_items(col).await?,
            None => self.get_recent_items(100).await?,
        };

        let total_items = items.len();
        let mut items_with_doi: usize = 0;
        let mut items_with_pdf: usize = 0;
        let mut items_with_notes: usize = 0;

        for item in &items {
            if item.data.doi.as_deref().is_some_and(|d| !d.trim().is_empty()) {
                items_with_doi = items_with_doi.saturating_add(1);
            }

            if let Ok(children) = self.get_item_children(&item.key).await {
                let has_pdf = children.iter().any(|c| {
                    c.data.item_type == "attachment"
                        && c.data
                            .content_type
                            .as_deref()
                            .is_some_and(|ct| ct.contains("pdf"))
                });
                if has_pdf {
                    items_with_pdf = items_with_pdf.saturating_add(1);
                }

                let has_note =
                    children.iter().any(|c| c.data.item_type == "note");
                if has_note {
                    items_with_notes = items_with_notes.saturating_add(1);
                }
            }
        }

        #[allow(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_lossless,
            reason = "coverage percentage calculation"
        )]
        let total_f = total_items as f64;
        #[allow(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_lossless,
            reason = "coverage percentage calculation"
        )]
        let doi_pct = if total_items > 0 {
            (items_with_doi as f64 / total_f) * 100.0
        } else {
            0.0
        };
        #[allow(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_lossless,
            reason = "coverage percentage calculation"
        )]
        let pdf_pct = if total_items > 0 {
            (items_with_pdf as f64 / total_f) * 100.0
        } else {
            0.0
        };
        #[allow(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_lossless,
            reason = "coverage percentage calculation"
        )]
        let notes_pct = if total_items > 0 {
            (items_with_notes as f64 / total_f) * 100.0
        } else {
            0.0
        };

        Ok(serde_json::json!({
            "total_items": total_items,
            "items_with_doi": items_with_doi,
            "doi_coverage_pct": doi_pct,
            "items_with_pdf": items_with_pdf,
            "pdf_coverage_pct": pdf_pct,
            "items_with_notes": items_with_notes,
            "notes_coverage_pct": notes_pct,
        }))
    }
    /// Extracts and synthesizes annotations and notes into structured Markdown.
    ///
    /// # Errors
    ///
    /// - [`NotFound`] if the item does not exist
    /// - [`LocalApi`] if the Local API responds with non-2xx
    /// - [`Network`] if transport fails
    pub(crate) async fn synthesize_annotations(
        &self,
        item_key: &str,
    ) -> Result<String, ZoteroMcpError> {
        use std::fmt::Write as _;

        let item = self.get_item(item_key).await?;
        let children =
            self.get_item_children(item_key).await.unwrap_or_default();

        let mut md = String::new();
        let title = item.data.title.as_deref().unwrap_or(item_key);
        let _ = writeln!(md, "# Annotations & Notes: {title}\n");

        if let Some(ref doi) = item.data.doi {
            let _ = writeln!(md, "**DOI:** {doi}");
        }
        if let Some(ref date) = item.data.date {
            let _ = writeln!(md, "**Date:** {date}");
        }
        md.push('\n');

        let mut has_annotations = false;
        md.push_str("## Highlights & Annotations\n\n");
        for child in &children {
            if child.data.item_type == "annotation" {
                has_annotations = true;
                let page =
                    child.data.annotation_page_label.as_deref().unwrap_or("?");
                if let Some(ref text) = child.data.annotation_text {
                    let _ = writeln!(md, "> \"{text}\" (p. {page})\n");
                }
                if let Some(ref comment) = child.data.annotation_comment {
                    let _ = writeln!(md, "**Comment:** {comment}\n");
                }
            }
        }
        if !has_annotations {
            md.push_str("*No direct annotations found.*\n\n");
        }

        let mut has_notes = false;
        md.push_str("## Notes\n\n");
        if let Some(ref note) = item.data.note {
            has_notes = true;
            let _ = writeln!(md, "{note}\n");
        }
        for child in &children {
            if child.data.item_type == "note" {
                if let Some(ref note) = child.data.note {
                    has_notes = true;
                    let _ = writeln!(md, "{note}\n");
                }
            }
        }
        if !has_notes {
            md.push_str("*No notes found.*\n\n");
        }
        Ok(md)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use reqwest::Client;

        use super::AppState;

        /// Builds an [`AppState`] pointing `zotero_api_url` at a fixture
        /// server, with `write_enabled` set for write-gate tests.
        pub(super) fn test_state(
            zotero_api_url: String,
            write_enabled: bool,
        ) -> AppState {
            AppState {
                client: Client::new(),
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                write_enabled,
            }
        }

        /// Formats a minimal raw HTTP/1.1 response with `status` (e.g.
        /// `"200 OK"`) and a JSON/text `body`, computing `Content-Length`
        /// automatically.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: \
                 close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Spawns a background thread serving one canned raw HTTP response
        /// (see [`http_response`]) per accepted connection, in order.
        /// Returns the bound `http://host:port` base URL, standing in for
        /// the Zotero Local API.
        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                let mut it = responses.into_iter();
                while let (Some(resp), Ok((mut stream, _))) =
                    (it.next(), listener.accept())
                {
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(resp.as_bytes());
                }
            });
            format!("http://{addr}")
        }
    }

    mod check_status {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn reports_online_with_version_when_api_responds_success() {
            // Arrange
            let body = "[]";
            let raw = format!(
                "HTTP/1.1 200 OK\r\nzotero-api-version: 3\r\nContent-Length: \
                 {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let base = mock_server(vec![raw]);
            let state = test_state(base, false);

            // Act
            let status = ZoteroClient::new(&state).check_status().await;

            // Assert
            assert!(status.online);
            assert_eq!(status.version.as_deref(), Some("3"));
            assert!(status.error.is_none());
        }

        #[tokio::test]
        async fn reports_offline_with_error_when_api_returns_error_status() {
            // Arrange
            let base = mock_server(vec![http_response(
                "500 Internal Server Error",
                "",
            )]);
            let state = test_state(base, false);

            // Act
            let status = ZoteroClient::new(&state).check_status().await;

            // Assert
            assert!(!status.online);
            assert!(status.error.unwrap().contains("500"));
        }

        #[tokio::test]
        async fn reports_offline_with_error_when_connection_fails() {
            // Arrange: port 0 is never a live listener, so the connection
            // is refused instantly.
            let state = test_state("http://127.0.0.1:0/api".to_owned(), false);

            // Act
            let status = ZoteroClient::new(&state).check_status().await;

            // Assert
            assert!(!status.online);
            assert!(status.error.is_some());
        }
    }

    mod get_recent_items {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn returns_items_on_success() {
            // Arrange
            let items = json!([{
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle" }
            }]);
            let base =
                mock_server(vec![http_response("200 OK", &items.to_string())]);
            let state = test_state(base, false);

            // Act
            let result =
                ZoteroClient::new(&state).get_recent_items(5).await.unwrap();

            // Assert
            assert_eq!(result.len(), 1);
            assert_eq!(result.first().map(|i| i.key.as_str()), Some("ITEM1"));
        }

        #[tokio::test]
        async fn returns_local_api_error_when_response_is_non_success() {
            // Arrange
            let base = mock_server(vec![http_response(
                "400 Bad Request",
                "invalid limit",
            )]);
            let state = test_state(base, false);

            // Act
            let err = ZoteroClient::new(&state)
                .get_recent_items(5)
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(
                &err,
                ZoteroMcpError::LocalApi { status: 400, message } if message == "invalid limit"
            ));
        }
    }

    mod get_item {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn returns_item_on_success() {
            // Arrange
            let item = json!({
                "key": "ITEM2",
                "version": 7,
                "data": { "key": "ITEM2", "version": 7, "itemType": "book" }
            });
            let base =
                mock_server(vec![http_response("200 OK", &item.to_string())]);
            let state = test_state(base, false);

            // Act
            let result =
                ZoteroClient::new(&state).get_item("ITEM2").await.unwrap();

            // Assert
            assert_eq!(result.key, "ITEM2");
        }

        #[tokio::test]
        async fn returns_not_found_error_when_response_is_404() {
            // Arrange
            let base = mock_server(vec![http_response("404 Not Found", "")]);
            let state = test_state(base, false);

            // Act
            let err = ZoteroClient::new(&state)
                .get_item("MISSING")
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(err, ZoteroMcpError::NotFound(_)));
        }
    }

    mod get_item_fulltext {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn returns_empty_string_when_content_field_is_missing() {
            // Arrange
            let base = mock_server(vec![http_response("200 OK", "{}")]);
            let state = test_state(base, false);

            // Act
            let text = ZoteroClient::new(&state)
                .get_item_fulltext("ITEM3")
                .await
                .unwrap();

            // Assert
            assert_eq!(text, "");
        }

        #[tokio::test]
        async fn returns_indexed_content_when_present() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"content":"indexed body text"}"#,
            )]);
            let state = test_state(base, false);

            // Act
            let text = ZoteroClient::new(&state)
                .get_item_fulltext("ITEM3")
                .await
                .unwrap();

            // Assert
            assert_eq!(text, "indexed body text");
        }
    }

    mod create_note {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn rejects_when_write_is_disabled() {
            // Arrange
            let state = test_state(String::new(), false);

            // Act
            let err = ZoteroClient::new(&state)
                .create_note("PARENT1", "note body")
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }

        #[tokio::test]
        async fn returns_created_item_on_success() {
            // Arrange
            let created = json!([{
                "key": "NOTE1",
                "version": 1,
                "data": { "key": "NOTE1", "version": 1, "itemType": "note", "note": "note body" }
            }]);
            let base = mock_server(vec![http_response(
                "200 OK",
                &created.to_string(),
            )]);
            let state = test_state(base, true);

            // Act
            let item = ZoteroClient::new(&state)
                .create_note("PARENT1", "note body")
                .await
                .unwrap();

            // Assert
            assert_eq!(item.key, "NOTE1");
        }

        #[tokio::test]
        async fn returns_local_api_error_when_response_array_is_empty() {
            // Arrange
            let base = mock_server(vec![http_response("200 OK", "[]")]);
            let state = test_state(base, true);

            // Act
            let err = ZoteroClient::new(&state)
                .create_note("PARENT1", "note body")
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(
                &err,
                ZoteroMcpError::LocalApi {
                    status: 500,
                    ..
                }
            ));
        }
    }
    mod create_collection {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn rejects_when_write_is_disabled() {
            let state = test_state(String::new(), false);
            let err = ZoteroClient::new(&state)
                .create_collection("New Col", None)
                .await
                .unwrap_err();
            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }

        #[tokio::test]
        async fn returns_created_collection_on_success() {
            let created = json!([{
                "key": "COL123",
                "version": 1,
                "data": { "key": "COL123", "name": "New Col", "parentCollection": "false" }
            }]);
            let base = mock_server(vec![http_response(
                "200 OK",
                &created.to_string(),
            )]);
            let state = test_state(base, true);

            let col = ZoteroClient::new(&state)
                .create_collection("New Col", None)
                .await
                .unwrap();
            assert_eq!(col.key, "COL123");
            assert_eq!(col.data.name, "New Col");
        }
    }

    mod search_collections {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn filters_collections_by_query_case_insensitively() {
            let cols = json!([
                { "key": "C1", "version": 1, "data": { "key": "C1", "name": "Machine Learning", "parentCollection": "false" } },
                { "key": "C2", "version": 1, "data": { "key": "C2", "name": "Deep Learning", "parentCollection": "false" } },
                { "key": "C3", "version": 1, "data": { "key": "C3", "name": "History", "parentCollection": "false" } }
            ]);
            let base =
                mock_server(vec![http_response("200 OK", &cols.to_string())]);
            let state = test_state(base, false);

            let results = ZoteroClient::new(&state)
                .search_collections("learning")
                .await
                .unwrap();
            assert_eq!(results.len(), 2);
            assert_eq!(results.first().expect("first collection").key, "C1");
            assert_eq!(results.get(1).expect("second collection").key, "C2");
        }
    }

    mod manage_collection_items {
        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn rejects_when_write_is_disabled() {
            let state = test_state(String::new(), false);
            let err = ZoteroClient::new(&state)
                .manage_collection_items("COL1", &["ITEM1".to_owned()], false)
                .await
                .unwrap_err();
            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }

        #[tokio::test]
        async fn adds_items_to_collection_on_success() {
            let base = mock_server(vec![http_response("200 OK", "")]);
            let state = test_state(base, true);

            let res = ZoteroClient::new(&state)
                .manage_collection_items(
                    "COL1",
                    &["ITEM1".to_owned(), "ITEM2".to_owned()],
                    false,
                )
                .await;
            assert!(res.is_ok());
        }

        #[tokio::test]
        async fn removes_items_from_collection_on_success() {
            let base = mock_server(vec![http_response("200 OK", "")]);
            let state = test_state(base, true);

            let res = ZoteroClient::new(&state)
                .manage_collection_items("COL1", &["ITEM1".to_owned()], true)
                .await;
            assert!(res.is_ok());
        }
    }
    mod update_item {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn rejects_when_write_is_disabled() {
            let state = test_state(String::new(), false);
            let err = ZoteroClient::new(&state)
                .update_item("ITEM1", json!({"title": "Updated"}))
                .await
                .unwrap_err();
            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }

        #[tokio::test]
        async fn returns_updated_item_on_success() {
            let updated = json!({
                "key": "ITEM1",
                "version": 2,
                "data": { "key": "ITEM1", "title": "Updated Title", "itemType": "journalArticle" }
            });
            let base = mock_server(vec![http_response(
                "200 OK",
                &updated.to_string(),
            )]);
            let state = test_state(base, true);

            let item = ZoteroClient::new(&state)
                .update_item("ITEM1", json!({"title": "Updated Title"}))
                .await
                .unwrap();
            assert_eq!(item.data.title.as_deref(), Some("Updated Title"));
        }
    }

    mod attach_file_link {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn rejects_when_write_is_disabled() {
            let state = test_state(String::new(), false);
            let err = ZoteroClient::new(&state)
                .attach_file_link(
                    "PARENT1",
                    "File Title",
                    "/path/to/file.pdf",
                    None,
                )
                .await
                .unwrap_err();
            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }

        #[tokio::test]
        async fn attaches_file_link_on_success() {
            let created = json!([{
                "key": "ATTACH1",
                "version": 1,
                "data": {
                    "key": "ATTACH1",
                    "itemType": "attachment",
                    "parentItem": "PARENT1",
                    "title": "File Title",
                    "path": "/path/to/file.pdf"
                }
            }]);
            let base = mock_server(vec![http_response(
                "200 OK",
                &created.to_string(),
            )]);
            let state = test_state(base, true);

            let item = ZoteroClient::new(&state)
                .attach_file_link(
                    "PARENT1",
                    "File Title",
                    "/path/to/file.pdf",
                    None,
                )
                .await
                .unwrap();
            assert_eq!(item.key, "ATTACH1");
        }
    }

    mod batch_update_tags {
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn rejects_when_write_is_disabled() {
            let state = test_state(String::new(), false);
            let err = ZoteroClient::new(&state)
                .batch_update_tags(
                    &["ITEM1".to_owned()],
                    &["tag1".to_owned()],
                    &[],
                )
                .await
                .unwrap_err();
            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }

        #[tokio::test]
        async fn updates_tags_across_items() {
            let get_resp = json!({
                "key": "ITEM1",
                "version": 1,
                "data": {
                    "key": "ITEM1",
                    "itemType": "journalArticle",
                    "tags": [{ "tag": "old_tag" }]
                }
            });
            let patch_resp = json!({
                "key": "ITEM1",
                "version": 2,
                "data": {
                    "key": "ITEM1",
                    "itemType": "journalArticle",
                    "tags": [{ "tag": "new_tag" }]
                }
            });
            let base = mock_server(vec![
                http_response("200 OK", &get_resp.to_string()),
                http_response("200 OK", &patch_resp.to_string()),
            ]);
            let state = test_state(base, true);

            let updated_count = ZoteroClient::new(&state)
                .batch_update_tags(
                    &["ITEM1".to_owned()],
                    &["new_tag".to_owned()],
                    &["old_tag".to_owned()],
                )
                .await
                .unwrap();
            assert_eq!(updated_count, 1);
        }
    }
    mod find_duplicates {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn detects_duplicates_by_title_and_doi() {
            let items = json!([
                {
                    "key": "ITEM1",
                    "version": 1,
                    "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Computing Basics", "doi": "10.1234/qc1" }
                },
                {
                    "key": "ITEM2",
                    "version": 1,
                    "data": { "key": "ITEM2", "itemType": "journalArticle", "title": "quantum computing basics", "doi": "10.1234/qc1" }
                },
                {
                    "key": "ITEM3",
                    "version": 1,
                    "data": { "key": "ITEM3", "itemType": "journalArticle", "title": "Unique Article Title", "doi": "10.1234/unique" }
                }
            ]);
            let base =
                mock_server(vec![http_response("200 OK", &items.to_string())]);
            let state = test_state(base, false);

            let dups =
                ZoteroClient::new(&state).find_duplicates(None).await.unwrap();
            assert_eq!(dups.len(), 2); // 1 for doi, 1 for title
        }
    }
    mod search_by_tag {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn returns_matching_items() {
            let items = json!([
                {
                    "key": "ITEM1",
                    "version": 1,
                    "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Paper 1", "tags": [{ "tag": "ml" }] }
                }
            ]);
            let base =
                mock_server(vec![http_response("200 OK", &items.to_string())]);
            let state = test_state(base, false);

            let res = ZoteroClient::new(&state)
                .search_by_tag("ml", 10)
                .await
                .unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(res.first().expect("item").key, "ITEM1");
        }
    }

    mod search_by_citation_key {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn matches_item_by_citation_key_in_extra() {
            let items = json!([
                {
                    "key": "ITEM1",
                    "version": 1,
                    "data": {
                        "key": "ITEM1",
                        "itemType": "journalArticle",
                        "title": "Deep Learning Paper",
                        "extra": "Citation Key: smith2020deep"
                    }
                }
            ]);
            let base =
                mock_server(vec![http_response("200 OK", &items.to_string())]);
            let state = test_state(base, false);

            let res = ZoteroClient::new(&state)
                .search_by_citation_key("smith2020deep")
                .await
                .unwrap();
            assert!(res.is_some());
            assert_eq!(res.unwrap().key, "ITEM1");
        }
    }
    mod advanced_search {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn filters_items_by_conditions() {
            let items = json!([
                {
                    "key": "ITEM1",
                    "version": 1,
                    "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Computing", "doi": "10.1000/1" }
                },
                {
                    "key": "ITEM2",
                    "version": 1,
                    "data": { "key": "ITEM2", "itemType": "book", "title": "Classical Mechanics" }
                }
            ]);
            let base =
                mock_server(vec![http_response("200 OK", &items.to_string())]);
            let state = test_state(base, false);

            let conds = vec![
                json!({"field": "title", "operator": "contains", "value": "quantum"}),
            ];
            let res = ZoteroClient::new(&state)
                .advanced_search(conds, 10)
                .await
                .unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(res.first().expect("item").key, "ITEM1");
        }
    }

    mod get_library_coverage {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn computes_coverage_metrics() {
            let items = json!([
                {
                    "key": "ITEM1",
                    "version": 1,
                    "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Paper 1", "doi": "10.1000/1" }
                }
            ]);
            let children = json!([
                {
                    "key": "CHILD1",
                    "version": 1,
                    "data": { "key": "CHILD1", "itemType": "attachment", "contentType": "application/pdf" }
                }
            ]);
            let base = mock_server(vec![
                http_response("200 OK", &items.to_string()),
                http_response("200 OK", &children.to_string()),
            ]);
            let state = test_state(base, false);

            let coverage = ZoteroClient::new(&state)
                .get_library_coverage(None)
                .await
                .unwrap();
            assert_eq!(coverage.get("total_items"), Some(&json!(1)));
            assert_eq!(coverage.get("items_with_doi"), Some(&json!(1)));
            assert_eq!(coverage.get("items_with_pdf"), Some(&json!(1)));
        }
    }
    mod synthesize_annotations {
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn formats_highlights_and_notes_to_markdown() {
            let item = json!({
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Physics Paper", "doi": "10.1000/1" }
            });
            let children = json!([
                {
                    "key": "ANN1",
                    "version": 1,
                    "data": {
                        "key": "ANN1",
                        "itemType": "annotation",
                        "annotationText": "Key discovery in quantum state",
                        "annotationComment": "Important finding",
                        "annotationPageLabel": "12"
                    }
                },
                {
                    "key": "NOTE1",
                    "version": 1,
                    "data": {
                        "key": "NOTE1",
                        "itemType": "note",
                        "note": "Summary of paper methods"
                    }
                }
            ]);
            let base = mock_server(vec![
                http_response("200 OK", &item.to_string()),
                http_response("200 OK", &children.to_string()),
            ]);
            let state = test_state(base, false);

            let md = ZoteroClient::new(&state)
                .synthesize_annotations("ITEM1")
                .await
                .unwrap();
            assert!(
                md.contains("# Annotations & Notes: Quantum Physics Paper")
            );
            assert!(
                md.contains("> \"Key discovery in quantum state\" (p. 12)")
            );
            assert!(md.contains("**Comment:** Important finding"));
            assert!(md.contains("Summary of paper methods"));
        }
    }
}
