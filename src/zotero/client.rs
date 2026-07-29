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
}
