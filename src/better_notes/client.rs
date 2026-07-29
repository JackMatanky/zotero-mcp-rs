//! Async client for the Better Notes bridge's HTTP companion API.

use serde::Serialize;
use serde_json::Value;

use crate::{
    better_notes::models::{
        MarkdownResponse, NoteItemResponse, NoteTreeResponse,
        RelationsResponse, TemplateResponse,
    },
    errors::ZoteroMcpError,
    state::AppState,
};

/// Client for the Better Notes bridge, scoped to a single tool call.
pub(crate) struct BetterNotesClient<'a> {
    state: &'a AppState,
}

impl<'a> BetterNotesClient<'a> {
    /// Creates a Better Notes client borrowing shared [`AppState`].
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
        }
    }

    /// Converts a note to Markdown, either by `item_key` (an existing Zotero
    /// note) or raw `html`.
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

    /// Creates a note attached to `parent_key` from `markdown`, returning the
    /// created note's item key.
    ///
    /// Mutates the Zotero library; assumes the caller has already enforced
    /// [`AppState::check_write_permission`], and re-checks it itself before
    /// issuing the call.
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

        /// Formats a minimal JSON HTTP response for fixture servers.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Runs a one-shot fixture HTTP server and returns its base URL.
        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }
    }

    mod post_json {
        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        // Exercised indirectly through `to_markdown`, the simplest caller of
        // the shared `post_json` envelope handling.

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
            assert!(matches!(
                &err,
                ZoteroMcpError::BetterNotes(msg) if msg.contains("400") && msg.contains("/notes/to-markdown")
            ));
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
