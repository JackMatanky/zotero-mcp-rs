//! Async client for the Zotero Local HTTP API.
//!
//! Thin wrapper around [`reqwest`] calls to the local Zotero library server,
//! using [`AppState::send_with_retry`] for transient-failure retries.

use crate::errors::ZoteroMcpError;
use crate::state::AppState;
use crate::zotero::models::{LocalApiStatus, ZoteroCollection, ZoteroItem};
use reqwest::StatusCode;

/// Client for the Zotero Local HTTP API, scoped to a single tool call.
#[expect(dead_code, reason = "Client invoked by MCP tool handlers")]
pub(crate) struct ZoteroClient<'a> {
    state: &'a AppState,
}

#[expect(dead_code, reason = "Client methods invoked by MCP tool handlers")]
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
                            .map(|s| s.to_string()),
                        error: None,
                    }
                } else {
                    LocalApiStatus {
                        online: false,
                        url: self.state.zotero_api_url.clone(),
                        version: None,
                        error: Some(format!("HTTP status {}", status)),
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
            "{}/users/0/items?limit={}&sort=dateModified&direction=desc&itemType=-note",
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
            return Err(ZoteroMcpError::NotFound(format!("Item {}", item_key)));
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
            .to_string();
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
            message: "Created note array was empty".to_string(),
        })
    }
}
