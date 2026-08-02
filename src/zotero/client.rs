//! Async client for the Zotero Local HTTP API.
//!
//! Provides the core [`ZoteroClient`] wrapper around reqwest, handling
//! authentication, request retries, error conversion, and response
//! deserialization.
//!
//! # Key Types
//!
//! - [`ZoteroClient`] - Main API client borrowing shared application state
//! - [`LocalApiStatus`] - Health check status report payload

use reqwest::Response;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    errors::ZoteroMcpError,
    state::AppState,
    zotero::models::{LibraryVersion, LocalApiStatus, ZoteroItem},
};

/// A page of items plus the server-reported total result count.
pub(super) struct ItemsPage {
    pub(super) items: Vec<ZoteroItem>,
    pub(super) total: usize,
}

/// Client for the Zotero Local HTTP API, scoped to a single tool call.
pub(crate) struct ZoteroClient<'a> {
    pub(super) state: &'a AppState,
}

impl<'a> ZoteroClient<'a> {
    /// Creates a Zotero Local API client borrowing shared [`AppState`].
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
        }
    }

    /// Probes the Zotero Local API for availability.
    ///
    /// Issues a lightweight `items?limit=1` request. Connection and HTTP status
    /// failures are captured in the returned [`LocalApiStatus::error`]
    /// field rather than being propagated as an error.
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

    /// Converts non-success HTTP responses into a [`ZoteroMcpError`].
    ///
    /// Evaluates `resp` and returns it unchanged if successful.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if `resp` status is not a successful HTTP status
    ///   (non-2xx)
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    pub(super) async fn ensure_success(
        &self,
        resp: Response,
    ) -> Result<Response, ZoteroMcpError> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        Err(ZoteroMcpError::LocalApi {
            status: resp.status().as_u16(),
            message: resp.text().await.unwrap_or_default(),
        })
    }

    /// Sends a GET request to `url` and decodes the JSON response body.
    ///
    /// Returns the decoded payload of type `T`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if Zotero responds with a non-2xx HTTP status
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if the response body cannot be decoded
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(super) async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, ZoteroMcpError> {
        let resp =
            self.state.send_with_retry(self.state.client.get(url)).await?;
        Ok(self.ensure_success(resp).await?.json().await?)
    }

    /// Fetches every page of a paginated list endpoint, stopping when a page
    /// returns fewer than `page_size` items (Zotero respects `start`/`limit`).
    ///
    /// `start`/`limit` query parameters are appended to `url` on every request,
    /// starting at `start=0`; any existing query string is preserved.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if Zotero responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if a response body cannot be decoded
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(super) async fn get_all_json<T: DeserializeOwned>(
        &self,
        url: &str,
        page_size: usize,
    ) -> Result<Vec<T>, ZoteroMcpError> {
        if page_size == 0 {
            return Ok(Vec::new());
        }
        let mut all = Vec::new();
        let mut start = 0_usize;
        loop {
            let page_url = add_pagination(url, start, page_size);
            let page: Vec<T> = self.get_json(&page_url).await?;
            let len = page.len();
            all.extend(page);
            if len < page_size {
                break;
            }
            start = start.saturating_add(page_size);
        }
        Ok(all)
    }

    /// Fetches one page of a paginated list endpoint, also returning the
    /// `Total-Results` response header (the full result count) when present.
    ///
    /// Used by server-side search so pagination can report the true total
    /// without scanning every page.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if Zotero responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if the response body cannot be decoded
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(super) async fn get_items_with_total(
        &self,
        url: &str,
    ) -> Result<ItemsPage, ZoteroMcpError> {
        let resp =
            self.state.send_with_retry(self.state.client.get(url)).await?;
        let resp = self.ensure_success(resp).await?;
        let total = resp
            .headers()
            .get("Total-Results")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let items = resp.json().await?;
        Ok(ItemsPage {
            items,
            total,
        })
    }

    /// Sends a JSON POST request to `url` and returns the first item from the
    /// array response.
    ///
    /// # Arguments
    ///
    /// * `url` - Target API endpoint URL
    /// * `payload` - JSON-serializable request payload
    /// * `empty_message` - Error message to return if Zotero returns an empty
    ///   array
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if Zotero responds with a non-2xx status, or returns an
    ///   empty array
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if the response body cannot be decoded
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(super) async fn post_json_first<T: DeserializeOwned, P: Serialize>(
        &self,
        url: &str,
        payload: &P,
        empty_message: &'static str,
    ) -> Result<T, ZoteroMcpError> {
        let resp = self
            .state
            .send_with_retry(self.state.client.post(url).json(payload))
            .await?;
        let created: Vec<T> = self.ensure_success(resp).await?.json().await?;
        created.into_iter().next().ok_or_else(|| ZoteroMcpError::LocalApi {
            status: 500,
            message: empty_message.to_owned(),
        })
    }

    /// Sends a `DELETE` request to `url` with an `If-Unmodified-Since-Version`
    /// header for `version`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if Zotero responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(super) async fn delete(
        &self,
        url: &str,
        version: LibraryVersion,
    ) -> Result<(), ZoteroMcpError> {
        let req = self
            .state
            .client
            .delete(url)
            .header("If-Unmodified-Since-Version", version.to_string());
        self.ensure_success(self.state.send_with_retry(req).await?).await?;
        Ok(())
    }

    /// Fetches the current library version counter via the
    /// `Last-Modified-Version` response header.
    ///
    /// Issues a lightweight `items?limit=1` request to inspect response
    /// headers.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if Zotero responds with a non-2xx status, or the response
    ///   lacks a valid `Last-Modified-Version` header
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    pub(super) async fn get_library_version(
        &self,
    ) -> Result<LibraryVersion, ZoteroMcpError> {
        let url =
            format!("{}/users/0/items?limit=1", self.state.zotero_api_url);
        let resp = self
            .ensure_success(
                self.state.send_with_retry(self.state.client.get(&url)).await?,
            )
            .await?;
        resp.headers()
            .get("Last-Modified-Version")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(LibraryVersion::from)
            .ok_or_else(|| ZoteroMcpError::LocalApi {
                status: 0,
                message: "Missing or invalid Last-Modified-Version header"
                    .to_owned(),
            })
    }
}

/// Appends `start`/`limit` query parameters to `url`, preserving any existing
/// query string.
fn add_pagination(url: &str, start: usize, limit: usize) -> String {
    let sep = if url.contains('?') {
        '&'
    } else {
        '?'
    };
    format!("{url}{sep}start={start}&limit={limit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        /// Builds an [`AppState`] fixture for testing with `zotero_api_url` and
        /// `write_enabled`.
        pub(super) fn test_state(
            zotero_api_url: String,
            write_enabled: bool,
        ) -> AppState {
            AppState {
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled,
                sqlite_access: false,
                ..AppState::from_env()
            }
        }

        /// Formats a minimal HTTP response string with `status` and `body`.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Formats an HTTP response string with additional header lines.
        pub(super) fn http_response_with_headers(
            status: &str,
            headers: &[(&str, &str)],
            body: &str,
        ) -> String {
            let mut header_text = String::new();
            for (name, val) in headers {
                use std::fmt::Write as _;
                let _ = writeln!(header_text, "{name}: {val}");
            }
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\n{header_text}Connection: \
                 close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Spawns a fixture HTTP server returning `responses` and returns its
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
    }

    mod check_status {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{http_response, mock_server, test_state},
            *,
        };

        #[tokio::test]
        async fn returns_online_true_on_200_ok() {
            let base = mock_server(vec![http_response("200 OK", "[]")]);
            let state = test_state(base, false);

            let status = ZoteroClient::new(&state).check_status().await;

            assert!(status.online);
            assert_eq!(status.error, None);
        }

        #[tokio::test]
        async fn returns_online_false_with_error_on_500() {
            let base =
                mock_server(vec![http_response("500 Internal Error", "")]);
            let state = test_state(base, false);

            let status = ZoteroClient::new(&state).check_status().await;

            assert!(!status.online);
            assert_eq!(
                status.error,
                Some("HTTP status 500 Internal Server Error".to_owned())
            );
        }

        #[tokio::test]
        async fn returns_online_false_on_connection_failure() {
            let state = test_state("http://127.0.0.1:1".to_owned(), false);

            let status = ZoteroClient::new(&state).check_status().await;

            assert!(!status.online);
            assert!(status.error.is_some());
        }
    }

    mod ensure_success {
        use super::{
            fixtures::{http_response, mock_server, test_state},
            *,
        };

        #[tokio::test]
        async fn returns_response_when_status_is_success() {
            let base = mock_server(vec![http_response("200 OK", "{}")]);
            let state = test_state(base.clone(), false);

            let resp =
                state.client.get(format!("{base}/test")).send().await.unwrap();
            let result = ZoteroClient::new(&state).ensure_success(resp).await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn returns_local_api_error_when_status_is_non_2xx() {
            let base = mock_server(vec![http_response(
                "400 Bad Request",
                "error details",
            )]);
            let state = test_state(base.clone(), false);

            let resp =
                state.client.get(format!("{base}/test")).send().await.unwrap();
            let err = ZoteroClient::new(&state)
                .ensure_success(resp)
                .await
                .unwrap_err();
            let ZoteroMcpError::LocalApi {
                status,
                message,
            } = err
            else {
                return;
            };
            assert_eq!(status, 400);
            assert_eq!(message, "error details");
        }
    }

    mod post_json_first {
        use serde_json::json;

        use super::{
            fixtures::{http_response, mock_server, test_state},
            *,
        };

        #[tokio::test]
        async fn returns_local_api_error_when_array_is_empty() {
            let base = mock_server(vec![http_response("200 OK", "[]")]);
            let state = test_state(base.clone(), true);

            let err = ZoteroClient::new(&state)
                .post_json_first::<serde_json::Value, _>(
                    &format!("{base}/items"),
                    &json!({}),
                    "No item created",
                )
                .await
                .unwrap_err();
            let ZoteroMcpError::LocalApi {
                status,
                message,
            } = err
            else {
                return;
            };
            assert_eq!(status, 500);
            assert_eq!(message, "No item created");
        }
    }

    mod delete {
        use super::{
            fixtures::{http_response, mock_server, test_state},
            *,
        };

        #[tokio::test]
        async fn sends_if_unmodified_since_version_header_and_succeeds_on_204()
        {
            let base = mock_server(vec![http_response("204 No Content", "")]);
            let state = test_state(base, false);

            let result = ZoteroClient::new(&state)
                .delete(&state.zotero_api_url.clone(), LibraryVersion(5))
                .await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn returns_local_api_error_on_412() {
            let base =
                mock_server(vec![http_response("412 Precondition Failed", "")]);
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .delete(&state.zotero_api_url.clone(), LibraryVersion(5))
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroMcpError::LocalApi { .. }));
        }
    }

    mod get_library_version {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{
                http_response, http_response_with_headers, mock_server,
                test_state,
            },
            *,
        };

        #[tokio::test]
        async fn reads_last_modified_version_header() {
            let base = mock_server(vec![http_response_with_headers(
                "200 OK",
                &[("Last-Modified-Version", "42")],
                "[]",
            )]);
            let state = test_state(base, false);

            let version =
                ZoteroClient::new(&state).get_library_version().await.unwrap();

            assert_eq!(version, LibraryVersion(42));
        }

        #[tokio::test]
        async fn returns_error_when_header_missing() {
            let base = mock_server(vec![http_response("200 OK", "[]")]);
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .get_library_version()
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroMcpError::LocalApi { .. }));
        }

        #[tokio::test]
        async fn returns_error_when_header_is_not_a_number() {
            let base = mock_server(vec![http_response_with_headers(
                "200 OK",
                &[("Last-Modified-Version", "not_a_num")],
                "[]",
            )]);
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .get_library_version()
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroMcpError::LocalApi { .. }));
        }
    }

    mod get_all_json {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{http_response, mock_server, test_state},
            *,
        };

        #[tokio::test]
        async fn fetches_every_page_until_a_short_page() {
            let base = mock_server(vec![
                http_response("200 OK", r#"[{"key":"A"},{"key":"B"}]"#),
                http_response("200 OK", r#"[{"key":"C"}]"#),
            ]);
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url);
            let items: Vec<serde_json::Value> =
                ZoteroClient::new(&state).get_all_json(&url, 2).await.unwrap();

            assert_eq!(items.len(), 3);
            let keys: Vec<&str> = items
                .iter()
                .map(|i| i["key"].as_str().unwrap_or_default())
                .collect();
            assert_eq!(keys, vec!["A", "B", "C"]);
        }

        #[tokio::test]
        async fn single_page_when_first_page_is_short() {
            let base =
                mock_server(vec![http_response("200 OK", r#"[{"key":"A"}]"#)]);
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url);
            let items: Vec<serde_json::Value> =
                ZoteroClient::new(&state).get_all_json(&url, 2).await.unwrap();

            assert_eq!(items.len(), 1);
        }

        #[tokio::test]
        async fn stops_on_empty_final_page_when_total_is_exact_multiple() {
            let base = mock_server(vec![
                http_response("200 OK", r#"[{"key":"A"},{"key":"B"}]"#),
                http_response("200 OK", r#"[]"#),
            ]);
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url);
            let items: Vec<serde_json::Value> =
                ZoteroClient::new(&state).get_all_json(&url, 2).await.unwrap();

            assert_eq!(items.len(), 2);
        }
    }

    mod get_items_with_total {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{
                http_response, http_response_with_headers, mock_server,
                test_state,
            },
            *,
        };

        const ITEMS: &str = r#"[{"key":"A","version":1,"data":{"key":"A","version":1,"itemType":"journalArticle","title":"A"}}]"#;

        #[tokio::test]
        async fn parses_numeric_total_results_header() {
            let base = mock_server(vec![http_response_with_headers(
                "200 OK",
                &[("Total-Results", "42")],
                ITEMS,
            )]);
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url);
            let page = ZoteroClient::new(&state)
                .get_items_with_total(&url)
                .await
                .unwrap();

            assert_eq!(page.total, 42);
            assert_eq!(page.items.len(), 1);
        }

        #[tokio::test]
        async fn falls_back_to_zero_when_header_absent() {
            let base = mock_server(vec![http_response("200 OK", ITEMS)]);
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url);
            let page = ZoteroClient::new(&state)
                .get_items_with_total(&url)
                .await
                .unwrap();

            assert_eq!(page.total, 0);
            assert_eq!(page.items.len(), 1);
        }

        #[tokio::test]
        async fn falls_back_to_zero_when_header_is_non_numeric() {
            let base = mock_server(vec![http_response_with_headers(
                "200 OK",
                &[("Total-Results", "abc")],
                ITEMS,
            )]);
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url);
            let page = ZoteroClient::new(&state)
                .get_items_with_total(&url)
                .await
                .unwrap();

            assert_eq!(page.total, 0);
            assert_eq!(page.items.len(), 1);
        }
    }

    mod add_pagination {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn preserves_existing_query_string() {
            assert_eq!(
                add_pagination("http://x/items?foo=1", 0, 2),
                "http://x/items?foo=1&start=0&limit=2"
            );
        }
    }
}
