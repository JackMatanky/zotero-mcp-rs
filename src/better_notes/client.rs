//! Async client for the Better Notes bridge's HTTP companion API.

use serde::Serialize;
use serde_json::Value;

use crate::{
    better_notes::models::{
        BetterNotesStatus, MarkdownResponse, NoteItemResponse,
        NoteTreeResponse, RelationsResponse, TemplateResponse,
    },
    errors::ZoteroMcpError,
    state::AppState,
};

/// Client for the Better Notes bridge, scoped to a single tool call.
pub(crate) struct BetterNotesClient<'a> {
    state: &'a AppState,
}

#[expect(dead_code, reason = "Client methods invoked by MCP tool handlers")]
impl<'a> BetterNotesClient<'a> {
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
        }
    }

    /// Probes the Better Notes bridge for availability.
    ///
    /// Never returns an error: failures are captured in the returned
    /// [`BetterNotesStatus::error`] field instead of being propagated.
    pub(crate) async fn check_status(&self) -> BetterNotesStatus {
        let url = format!("{}/status", self.state.better_notes_url);
        match self.state.client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let val: serde_json::Value =
                        resp.json().await.unwrap_or_default();
                    BetterNotesStatus {
                        online: true,
                        url: self.state.better_notes_url.clone(),
                        version: val
                            .get("version")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned),
                        error: None,
                    }
                } else {
                    BetterNotesStatus {
                        online: false,
                        url: self.state.better_notes_url.clone(),
                        version: None,
                        error: Some(format!("HTTP {}", resp.status())),
                    }
                }
            }
            Err(e) => BetterNotesStatus {
                online: false,
                url: self.state.better_notes_url.clone(),
                version: None,
                error: Some(e.to_string()),
            },
        }
    }

    /// Posts `payload` as JSON to `endpoint` on the Better Notes bridge and
    /// decodes the response as `R`.
    ///
    /// # Errors
    ///
    /// - [`BetterNotes`] if the HTTP response is non-2xx
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if the response body fails to deserialize as `R`
    ///
    /// [`BetterNotes`]: ZoteroMcpError::BetterNotes
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    async fn post_json<P: Serialize, R: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        payload: P,
    ) -> Result<R, ZoteroMcpError> {
        let url = format!("{}{}", self.state.better_notes_url, endpoint);
        let resp = self
            .state
            .send_with_retry(self.state.client.post(&url).json(&payload))
            .await?;

        if !resp.status().is_success() {
            return Err(ZoteroMcpError::BetterNotes(format!(
                "HTTP {} calling {}",
                resp.status(),
                endpoint
            )));
        }

        let res: R = resp.json().await?;
        Ok(res)
    }

    /// Converts a note to Markdown, either by `item_key` (an existing
    /// Zotero note) or raw `html`.
    ///
    /// # Errors
    ///
    /// - [`BetterNotes`] if the bridge call fails
    ///
    /// [`BetterNotes`]: ZoteroMcpError::BetterNotes
    pub(crate) async fn to_markdown(
        &self,
        item_key: Option<&str>,
        html: Option<&str>,
    ) -> Result<String, ZoteroMcpError> {
        let payload = serde_json::json!({
            "itemKey": item_key,
            "html": html,
        });
        let res: MarkdownResponse =
            self.post_json("/notes/to-markdown", payload).await?;
        Ok(res.markdown)
    }

    /// Creates a note attached to `parent_key` from `markdown`, returning
    /// the created note's item key.
    ///
    /// Mutates the Zotero library; assumes the caller has already enforced
    /// [`AppState::check_write_permission`], and re-checks it itself
    /// before issuing the call.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`BetterNotes`] if the bridge call fails
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`BetterNotes`]: ZoteroMcpError::BetterNotes
    pub(crate) async fn convert_from_markdown(
        &self,
        parent_key: &str,
        markdown: &str,
    ) -> Result<String, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let payload = serde_json::json!({
            "parentKey": parent_key,
            "markdown": markdown,
        });
        let res: NoteItemResponse =
            self.post_json("/notes/from-markdown", payload).await?;
        Ok(res.item_key)
    }

    /// Runs the named Better Notes template `name` against `item_key`.
    ///
    /// # Errors
    ///
    /// - [`BetterNotes`] if the bridge call fails
    ///
    /// [`BetterNotes`]: ZoteroMcpError::BetterNotes
    pub(crate) async fn run_template(
        &self,
        name: &str,
        item_key: &str,
    ) -> Result<Value, ZoteroMcpError> {
        let payload = serde_json::json!({
            "name": name,
            "itemKey": item_key,
        });
        let res: TemplateResponse =
            self.post_json("/templates/run", payload).await?;
        Ok(res.result)
    }

    /// Fetches outlinks, backlinks, and other graph relations for
    /// `item_key`.
    ///
    /// # Errors
    ///
    /// - [`BetterNotes`] if the bridge call fails
    ///
    /// [`BetterNotes`]: ZoteroMcpError::BetterNotes
    pub(crate) async fn get_relations(
        &self,
        item_key: &str,
    ) -> Result<Value, ZoteroMcpError> {
        let payload = serde_json::json!({
            "itemKey": item_key,
        });
        let res: RelationsResponse =
            self.post_json("/relations/get", payload).await?;
        Ok(res.relations)
    }

    /// Fetches the full Better Notes hierarchy tree rooted at `item_key`.
    ///
    /// # Errors
    ///
    /// - [`BetterNotes`] if the bridge call fails
    ///
    /// [`BetterNotes`]: ZoteroMcpError::BetterNotes
    pub(crate) async fn get_tree(
        &self,
        item_key: &str,
    ) -> Result<Value, ZoteroMcpError> {
        let payload = serde_json::json!({
            "itemKey": item_key,
        });
        let res: NoteTreeResponse =
            self.post_json("/notes/tree", payload).await?;
        Ok(res.tree)
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

        /// Builds an [`AppState`] pointing `better_notes_url` at a fixture
        /// server, with `write_enabled` set for write-gate tests.
        pub(super) fn test_state(
            better_notes_url: String,
            write_enabled: bool,
        ) -> AppState {
            AppState {
                client: Client::new(),
                zotero_api_url: String::new(),
                better_bibtex_url: String::new(),
                better_notes_url,
                write_enabled,
            }
        }

        /// Formats a minimal raw HTTP/1.1 response with `status` (e.g.
        /// `"200 OK"`) and a JSON `body`, computing `Content-Length`
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
        /// the Better Notes bridge.
        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                for resp in responses {
                    let Ok((mut stream, _)) = listener.accept() else {
                        return;
                    };
                    let mut buf = [0_u8; 4096];
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
        async fn reports_online_with_version_when_bridge_responds_success() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"version":"1.0.0"}"#,
            )]);
            let state = test_state(base, false);

            // Act
            let status = BetterNotesClient::new(&state).check_status().await;

            // Assert
            assert!(status.online);
            assert_eq!(status.version.as_deref(), Some("1.0.0"));
            assert!(status.error.is_none());
        }

        #[tokio::test]
        async fn reports_offline_with_error_when_bridge_returns_error_status() {
            // Arrange
            let base = mock_server(vec![http_response("404 Not Found", "")]);
            let state = test_state(base, false);

            // Act
            let status = BetterNotesClient::new(&state).check_status().await;

            // Assert
            assert!(!status.online);
            assert!(status.error.unwrap().contains("404"));
        }

        #[tokio::test]
        async fn reports_offline_with_error_when_connection_fails() {
            // Arrange: port 0 is never a live listener, so the connection
            // is refused instantly.
            let state = test_state("http://127.0.0.1:0".to_owned(), false);

            // Act
            let status = BetterNotesClient::new(&state).check_status().await;

            // Assert
            assert!(!status.online);
            assert!(status.error.is_some());
        }
    }

    mod post_json {
        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        // Exercised indirectly through `to_markdown`, the simplest caller
        // of the shared `post_json` envelope handling.

        #[tokio::test]
        async fn returns_better_notes_error_when_response_is_non_success() {
            // Arrange
            let base = mock_server(vec![http_response("400 Bad Request", "")]);
            let state = test_state(base, false);

            // Act
            let err = BetterNotesClient::new(&state)
                .to_markdown(Some("NOTE1"), None)
                .await
                .unwrap_err();

            // Assert
            match err {
                ZoteroMcpError::BetterNotes(msg) => {
                    assert!(msg.contains("400"));
                    assert!(msg.contains("/notes/to-markdown"));
                }
                other => panic!("expected BetterNotes error, got {other:?}"),
            }
        }
    }

    mod to_markdown {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn returns_markdown_on_success() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r##"{"markdown":"# Hello"}"##,
            )]);
            let state = test_state(base, false);

            // Act
            let markdown = BetterNotesClient::new(&state)
                .to_markdown(Some("NOTE1"), None)
                .await
                .unwrap();

            // Assert
            assert_eq!(markdown, "# Hello");
        }
    }

    mod convert_from_markdown {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn rejects_when_write_is_disabled() {
            // Arrange
            let state = test_state(String::new(), false);

            // Act
            let err = BetterNotesClient::new(&state)
                .convert_from_markdown("PARENT1", "# Hello")
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }

        #[tokio::test]
        async fn returns_created_item_key_when_write_is_enabled() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"itemKey":"NOTE1"}"#,
            )]);
            let state = test_state(base, true);

            // Act
            let key = BetterNotesClient::new(&state)
                .convert_from_markdown("PARENT1", "# Hello")
                .await
                .unwrap();

            // Assert
            assert_eq!(key, "NOTE1");
        }
    }
}
