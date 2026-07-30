//! Async client for the Better Notes bridge's HTTP companion API.

use serde::Serialize;
use serde_json::Value;

use crate::{
    better_notes::models::{
        NoteExportFormat, NoteExportResponse, NoteItemResponse,
        NoteTreeResponse, RelationsResponse, TemplateResponse,
    },
    errors::ZoteroMcpError,
    state::AppState,
    zotero::ItemKey,
};

/// Client for the Better Notes bridge, scoped to a single tool call.
pub(crate) struct BetterNotesClient<'a> {
    state: &'a AppState,
}

impl<'a> BetterNotesClient<'a> {
    /// Creates a Better Notes client borrowing shared `state` ([`AppState`]).
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
        }
    }

    /// Exports an existing Zotero note through the Better Notes bridge as
    /// Markdown or HTML.
    ///
    /// # Errors
    ///
    /// - [`BetterNotes`] if the bridge call fails
    ///
    /// [`BetterNotes`]: ZoteroMcpError::BetterNotes
    pub(crate) async fn export(
        &self,
        item_key: &ItemKey,
        format: Option<NoteExportFormat>,
    ) -> Result<String, ZoteroMcpError> {
        let format = format.unwrap_or_default();
        let payload = serde_json::json!({
            "itemKey": item_key,
            "format": format.as_str(),
        });
        let res: NoteExportResponse =
            self.post_json("/notes/export", payload).await?;
        Ok(res.content)
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
        parent_key: &ItemKey,
        markdown: &str,
    ) -> Result<ItemKey, ZoteroMcpError> {
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
        item_key: &ItemKey,
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
        item_key: &ItemKey,
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
        item_key: &ItemKey,
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

        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::mpsc::{self, Receiver},
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
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled,
            }
        }

        /// Formats a minimal JSON HTTP response with `status` and `body` for
        /// fixture servers.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Runs a one-shot fixture HTTP server for `responses` and returns its
        /// base URL.
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

        /// Runs a fixture HTTP server, captures each accepted request, and
        /// returns its base URL plus the request receiver.
        pub(super) fn mock_server_with_requests(
            responses: Vec<String>,
        ) -> (String, Receiver<String>) {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            let (requests_tx, requests_rx) = mpsc::channel();
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut buf = [0_u8; 1024];
                    let bytes_read =
                        stream.read(&mut buf).expect("read request");
                    requests_tx
                        .send(
                            String::from_utf8_lossy(&buf[..bytes_read])
                                .into_owned(),
                        )
                        .expect("send request");
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            (format!("http://{addr}"), requests_rx)
        }
    }

    mod post_json {
        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        // Exercised indirectly through `export`, the simplest caller of the
        // shared `post_json` envelope handling.

        #[tokio::test]
        async fn returns_better_notes_error_when_response_is_non_success() {
            // Arrange
            let base = mock_server(vec![http_response("400 Bad Request", "")]);
            let state = test_state(base, false);

            // Act
            let err = BetterNotesClient::new(&state)
                .export(&"NOTE1".into(), None)
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(
                &err,
                ZoteroMcpError::BetterNotes(msg) if msg.contains("400") && msg.contains("/notes/export")
            ));
        }
    }

    mod export {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server_with_requests, test_state},
        };

        #[tokio::test]
        async fn exports_markdown_by_default() {
            // Arrange
            let (base, requests) =
                mock_server_with_requests(vec![http_response(
                    "200 OK",
                    r##"{"content":"# Hello"}"##,
                )]);
            let state = test_state(base, false);

            // Act
            let markdown = BetterNotesClient::new(&state)
                .export(&"NOTE1".into(), None)
                .await
                .unwrap();

            // Assert
            assert_eq!(markdown, "# Hello");
            let request = requests.recv().expect("captured request");
            assert!(request.starts_with("POST /notes/export HTTP/1.1"));
            assert!(request.contains(r#""itemKey":"NOTE1""#));
            assert!(request.contains(r#""format":"markdown""#));
        }

        #[tokio::test]
        async fn exports_html_when_requested() {
            // Arrange
            let (base, requests) =
                mock_server_with_requests(vec![http_response(
                    "200 OK",
                    r#"{"content":"<h1>Hello</h1>"}"#,
                )]);
            let state = test_state(base, false);

            // Act
            let html = BetterNotesClient::new(&state)
                .export(&"NOTE1".into(), Some(NoteExportFormat::Html))
                .await
                .unwrap();

            // Assert
            assert_eq!(html, "<h1>Hello</h1>");
            let request = requests.recv().expect("captured request");
            assert!(request.starts_with("POST /notes/export HTTP/1.1"));
            assert!(request.contains(r#""itemKey":"NOTE1""#));
            assert!(request.contains(r#""format":"html""#));
        }

        #[tokio::test]
        async fn returns_json_error_when_export_response_lacks_content() {
            // Arrange
            let (base, _requests) =
                mock_server_with_requests(vec![http_response(
                    "200 OK",
                    r##"{"markdown":"# Old shape"}"##,
                )]);
            let state = test_state(base, false);

            // Act
            let err = BetterNotesClient::new(&state)
                .export(&"NOTE1".into(), Some(NoteExportFormat::Markdown))
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(err, ZoteroMcpError::Json(_)));
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
                .convert_from_markdown(&"PARENT1".into(), "# Hello")
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
                .convert_from_markdown(&"PARENT1".into(), "# Hello")
                .await
                .unwrap();

            // Assert
            assert_eq!(key, "NOTE1");
        }
    }
}
