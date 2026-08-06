//! Async client for the Zotero Local HTTP API.
//!
//! Defines [`ZoteroClient`], the request wrapper shared by Zotero domain
//! modules. The client centralizes authentication, retries, HTTP error
//! conversion, pagination helpers, and JSON decoding.
//!
//! # Key Types
//!
//! - [`ZoteroClient`]: API client borrowing shared application state.
//! - [`LocalApiStatus`]: Health check payload returned by status probes.
//!
//! # Examples
//!
//! Constructing a [`ZoteroClient`] from shared [`AppState`] and checking Local
//! API availability:
//!
//! ```no_run
//! # use zotero_api::AppState;
//! # use zotero_api::zotero::ZoteroClient;
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let state = AppState::from_env();
//! let client = ZoteroClient::new(&state);
//! let status = client.check_status().await;
//! if status.online {
//!     println!("Connected to Zotero version: {:?}", status.version);
//! }
//! # Ok(())
//! # }
//! ```

use reqwest::Response;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    errors::ZoteroApiError,
    state::AppState,
    zotero::{LibraryVersion, ZoteroItem, objects::LocalApiStatus},
};

/// One page of Zotero items and the optional `Total-Results` header count.
pub(super) struct ItemsPage {
    /// Fetched items for the requested page.
    pub(super) items: Vec<ZoteroItem>,
    /// Total number of matching items across all pages, if provided by Zotero.
    pub(super) total: Option<usize>,
}

/// Client for the Zotero Local HTTP API, scoped to a single tool call.
///
/// Wraps shared application state ([`AppState`]) to issue HTTP requests
/// against Zotero's local REST API with automatic retries and error
/// mapping.
pub struct ZoteroClient<'a> {
    /// Borrowed shared application state containing HTTP client and API
    /// configuration.
    pub(super) state: &'a AppState,
}

impl<'a> ZoteroClient<'a> {
    /// Creates a Zotero Local API client borrowing shared [`AppState`].
    #[must_use]
    #[inline]
    pub fn new(state: &'a AppState) -> Self {
        Self {
            state,
        }
    }

    /// Probes the Zotero Local API for availability.
    ///
    /// Issues a lightweight `items?limit=1` request. Connection and HTTP status
    /// failures are captured in the returned [`LocalApiStatus::error`] field
    /// rather than being propagated as an error.
    #[inline]
    pub async fn check_status(&self) -> LocalApiStatus {
        let url =
            format!("{}/users/0/items?limit=1", self.state.zotero_api_url());
        match self.state.client().get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    LocalApiStatus {
                        online: true,
                        url: self.state.zotero_api_url().to_owned(),
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
                        url: self.state.zotero_api_url().to_owned(),
                        version: None,
                        error: Some(format!("HTTP status {status}")),
                    }
                }
            }
            Err(e) => LocalApiStatus {
                online: false,
                url: self.state.zotero_api_url().to_owned(),
                version: None,
                error: Some(e.to_string()),
            },
        }
    }

    /// Converts non-success HTTP responses into a [`ZoteroApiError`].
    ///
    /// Evaluates `resp` and returns it unchanged if successful.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If `resp` status is not a successful HTTP status
    ///   (non-2xx).
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    pub(super) async fn ensure_success(
        &self,
        resp: Response,
    ) -> Result<Response, ZoteroApiError> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        Err(ZoteroApiError::LocalApi {
            status: resp.status().as_u16(),
            message: resp.text().await.unwrap_or_default(),
        })
    }

    /// Fetches JSON data from `url` and decodes the response body.
    ///
    /// Returns the decoded payload of type `T`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx HTTP status.
    /// - [`Network`]: If the request fails at the transport level.
    /// - [`Json`]: If the response body cannot be decoded.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    /// [`Json`]: ZoteroApiError::Json
    pub(super) async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, ZoteroApiError> {
        let resp =
            self.state.send_with_retry(self.state.client().get(url)).await?;
        Ok(self.ensure_success(resp).await?.json().await?)
    }

    /// Fetches every page of a paginated list endpoint.
    ///
    /// Appends `start` and `limit` query parameters to `url` on every request,
    /// starting at `start=0`, and stops when a page returns fewer than
    /// `page_size` items (Zotero respects `start`/`limit`).
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx HTTP status.
    /// - [`Network`]: If the request fails at the transport level.
    /// - [`Json`]: If a response body cannot be decoded.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    /// [`Json`]: ZoteroApiError::Json
    #[inline]
    pub async fn get_all_json<T: DeserializeOwned>(
        &self,
        url: &str,
        page_size: usize,
    ) -> Result<Vec<T>, ZoteroApiError> {
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

    /// Fetches one page of a paginated list endpoint, returning the
    /// `Total-Results` header count.
    ///
    /// Used by server-side search so pagination can report the true total
    /// without scanning every page.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx HTTP status.
    /// - [`Network`]: If the request fails at the transport level.
    /// - [`Json`]: If the response body cannot be decoded.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    /// [`Json`]: ZoteroApiError::Json
    pub(super) async fn get_items_with_total(
        &self,
        url: &str,
    ) -> Result<ItemsPage, ZoteroApiError> {
        let resp =
            self.state.send_with_retry(self.state.client().get(url)).await?;
        let resp = self.ensure_success(resp).await?;
        let total = resp
            .headers()
            .get("Total-Results")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok());
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
    /// * `url` - Target API endpoint URL.
    /// * `payload` - JSON-serializable request payload.
    /// * `empty_message` - Error message to return if Zotero returns an empty
    ///   array.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status, or returns an
    ///   empty array.
    /// - [`Network`]: If the request fails at the transport level.
    /// - [`Json`]: If the response body cannot be decoded.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    /// [`Json`]: ZoteroApiError::Json
    pub(super) async fn post_json_first<T: DeserializeOwned, P: Serialize>(
        &self,
        url: &str,
        payload: &P,
        empty_message: &'static str,
    ) -> Result<T, ZoteroApiError> {
        let resp = self
            .state
            .send_with_retry(self.state.client().post(url).json(payload))
            .await?;
        let created: Vec<T> = self.ensure_success(resp).await?.json().await?;
        created.into_iter().next().ok_or_else(|| ZoteroApiError::LocalApi {
            status: 500,
            message: empty_message.to_owned(),
        })
    }

    /// Sends a `DELETE` request to `url` with version concurrency control.
    ///
    /// Attaches an `If-Unmodified-Since-Version` header set to `version`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx HTTP status.
    /// - [`Network`]: If the request fails at the transport level.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    pub(super) async fn delete(
        &self,
        url: &str,
        version: LibraryVersion,
    ) -> Result<(), ZoteroApiError> {
        let req = self
            .state
            .client()
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
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status, or the
    ///   response lacks a valid `Last-Modified-Version` header.
    /// - [`Network`]: If the request fails at the transport level.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    pub(super) async fn get_library_version(
        &self,
    ) -> Result<LibraryVersion, ZoteroApiError> {
        let url =
            format!("{}/users/0/items?limit=1", self.state.zotero_api_url());
        let resp = self
            .ensure_success(
                self.state
                    .send_with_retry(self.state.client().get(&url))
                    .await?,
            )
            .await?;
        resp.headers()
            .get("Last-Modified-Version")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(LibraryVersion::from)
            .ok_or_else(|| ZoteroApiError::LocalApi {
                status: 0,
                message: "Missing or invalid Last-Modified-Version header"
                    .to_owned(),
            })
    }
}

/// Appends `start` and `limit` query parameters to `url`, preserving any
/// existing query string.
///
/// # Arguments
///
/// * `url` - Target URL string.
/// * `start` - Starting item index (zero-based).
/// * `limit` - Maximum number of items to return.
pub(super) fn add_pagination(url: &str, start: usize, limit: usize) -> String {
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
        use super::AppState;
        pub(super) use crate::zotero::test_http::{
            MockServer, http_response, http_response_with_headers,
        };

        /// Builds an [`AppState`] fixture for testing with `zotero_api_url` and
        /// `write_enabled`.
        pub(super) fn test_state(
            zotero_api_url: impl AsRef<str>,
            write_enabled: bool,
        ) -> AppState {
            AppState::test_default()
                .with_zotero_api_url(zotero_api_url.as_ref())
                .with_write_enabled(write_enabled)
        }
    }

    mod check_status {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{
                MockServer, http_response, http_response_with_headers,
                test_state,
            },
            *,
        };

        #[tokio::test]
        async fn returns_online_true_on_200_ok() {
            let server = MockServer::new(vec![http_response("200 OK", "[]")]);
            let base = server.url();
            let state = test_state(base, false);

            let status = ZoteroClient::new(&state).check_status().await;

            assert!(status.online);
            assert_eq!(status.error, None);
        }

        #[tokio::test]
        async fn returns_online_false_with_error_on_500() {
            let server =
                MockServer::new(vec![http_response("500 Internal Error", "")]);
            let base = server.url();
            let state = test_state(base, false);

            let status = ZoteroClient::new(&state).check_status().await;

            assert!(!status.online);
            assert_eq!(
                status.error,
                Some("HTTP status 500 Internal Server Error".to_owned())
            );
        }

        #[tokio::test]
        async fn check_status_captures_api_version_header() {
            let server = MockServer::new(vec![http_response_with_headers(
                "200 OK",
                &[("zotero-api-version", "7.0.0")],
                "[]",
            )]);
            let base = server.url();
            let state = test_state(base, false);

            let status = ZoteroClient::new(&state).check_status().await;

            assert!(status.online, "200 OK should report the local API online");
            assert_eq!(status.version.as_deref(), Some("7.0.0"));
        }

        #[tokio::test]
        async fn returns_online_false_on_connection_failure() {
            let state = test_state("http://127.0.0.1:1", false);

            let status = ZoteroClient::new(&state).check_status().await;

            assert!(!status.online);
            assert!(status.error.is_some());
        }
    }

    mod ensure_success {
        use super::{
            fixtures::{MockServer, http_response, test_state},
            *,
        };

        #[tokio::test]
        async fn returns_response_when_status_is_success() {
            let server = MockServer::new(vec![http_response("200 OK", "{}")]);
            let base = server.url();
            let state = test_state(base, false);

            let resp = state
                .client()
                .get(format!("{base}/test"))
                .send()
                .await
                .unwrap();
            let result = ZoteroClient::new(&state).ensure_success(resp).await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn returns_local_api_error_when_status_is_non_2xx() {
            let server = MockServer::new(vec![http_response(
                "400 Bad Request",
                "error details",
            )]);
            let base = server.url();
            let state = test_state(base, false);

            let resp = state
                .client()
                .get(format!("{base}/test"))
                .send()
                .await
                .unwrap();
            let err = ZoteroClient::new(&state)
                .ensure_success(resp)
                .await
                .unwrap_err();
            let ZoteroApiError::LocalApi {
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
            fixtures::{MockServer, http_response, test_state},
            *,
        };

        #[tokio::test]
        async fn returns_local_api_error_when_array_is_empty() {
            let server = MockServer::new(vec![http_response("200 OK", "[]")]);
            let base = server.url();
            let state = test_state(base, true);

            let err = ZoteroClient::new(&state)
                .post_json_first::<serde_json::Value, _>(
                    &format!("{base}/items"),
                    &json!({}),
                    "No item created",
                )
                .await
                .unwrap_err();
            let ZoteroApiError::LocalApi {
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
            fixtures::{MockServer, http_response, test_state},
            *,
        };

        #[tokio::test]
        async fn sends_if_unmodified_since_version_header_and_succeeds_on_204()
        {
            let server =
                MockServer::new(vec![http_response("204 No Content", "")]);
            let base = server.url();
            let state = test_state(base, false);

            let result = ZoteroClient::new(&state)
                .delete(state.zotero_api_url(), LibraryVersion(5))
                .await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn returns_local_api_error_on_412() {
            let server = MockServer::new(vec![http_response(
                "412 Precondition Failed",
                "",
            )]);
            let base = server.url();
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .delete(state.zotero_api_url(), LibraryVersion(5))
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroApiError::LocalApi { .. }));
        }
    }

    mod get_library_version {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{
                MockServer, http_response, http_response_with_headers,
                test_state,
            },
            *,
        };

        #[tokio::test]
        async fn reads_last_modified_version_header() {
            let server = MockServer::new(vec![http_response_with_headers(
                "200 OK",
                &[("Last-Modified-Version", "42")],
                "[]",
            )]);
            let base = server.url();
            let state = test_state(base, false);

            let version =
                ZoteroClient::new(&state).get_library_version().await.unwrap();

            assert_eq!(version, LibraryVersion(42));
        }

        #[tokio::test]
        async fn returns_error_when_header_missing() {
            let server = MockServer::new(vec![http_response("200 OK", "[]")]);
            let base = server.url();
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .get_library_version()
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroApiError::LocalApi { .. }));
        }

        #[tokio::test]
        async fn returns_error_when_header_is_not_a_number() {
            let server = MockServer::new(vec![http_response_with_headers(
                "200 OK",
                &[("Last-Modified-Version", "not_a_num")],
                "[]",
            )]);
            let base = server.url();
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .get_library_version()
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroApiError::LocalApi { .. }));
        }
    }

    mod get_all_json {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{MockServer, http_response, test_state},
            *,
        };

        #[tokio::test]
        async fn get_all_json_returns_empty_without_request_when_page_size_is_zero()
         {
            let (server, recorded) =
                MockServer::recording(vec![http_response(
                    "200 OK",
                    r#"[{"key":"A"}]"#,
                )]);
            let base = server.url();
            let state = test_state(base, false);
            let url = format!("{base}/users/0/items");

            let result: Result<Vec<serde_json::Value>, _> =
                ZoteroClient::new(&state).get_all_json(&url, 0).await;

            assert!(
                result.is_ok(),
                "page size zero should return Ok: {result:?}"
            );
            assert_eq!(
                result.unwrap_or_default(),
                Vec::<serde_json::Value>::new()
            );
            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 0);
        }

        #[tokio::test]
        async fn fetches_every_page_until_a_short_page() {
            let server = MockServer::new(vec![
                http_response("200 OK", r#"[{"key":"A"},{"key":"B"}]"#),
                http_response("200 OK", r#"[{"key":"C"}]"#),
            ]);
            let base = server.url();
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url());
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
            let server = MockServer::new(vec![http_response(
                "200 OK",
                r#"[{"key":"A"}]"#,
            )]);
            let base = server.url();
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url());
            let items: Vec<serde_json::Value> =
                ZoteroClient::new(&state).get_all_json(&url, 2).await.unwrap();

            assert_eq!(items.len(), 1);
        }

        #[tokio::test]
        async fn stops_on_empty_final_page_when_total_is_exact_multiple() {
            let server = MockServer::new(vec![
                http_response("200 OK", r#"[{"key":"A"},{"key":"B"}]"#),
                http_response("200 OK", r"[]"),
            ]);
            let base = server.url();
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url());
            let items: Vec<serde_json::Value> =
                ZoteroClient::new(&state).get_all_json(&url, 2).await.unwrap();

            assert_eq!(items.len(), 2);
        }
    }

    mod get_items_with_total {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{
                MockServer, http_response, http_response_with_headers,
                test_state,
            },
            *,
        };

        const ITEMS: &str = r#"[{"key":"A","version":1,"data":{"key":"A","version":1,"itemType":"journalArticle","title":"A"}}]"#;

        #[tokio::test]
        async fn parses_numeric_total_results_header() {
            let server = MockServer::new(vec![http_response_with_headers(
                "200 OK",
                &[("Total-Results", "42")],
                ITEMS,
            )]);
            let base = server.url();
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url());
            let page = ZoteroClient::new(&state)
                .get_items_with_total(&url)
                .await
                .unwrap();

            assert_eq!(page.total, Some(42));
            assert_eq!(page.items.len(), 1);
        }

        #[tokio::test]
        async fn returns_unknown_total_when_header_absent() {
            let server = MockServer::new(vec![http_response("200 OK", ITEMS)]);
            let base = server.url();
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url());
            let page = ZoteroClient::new(&state)
                .get_items_with_total(&url)
                .await
                .unwrap();

            assert_eq!(page.total, None);
            assert_eq!(page.items.len(), 1);
        }

        #[tokio::test]
        async fn returns_unknown_total_when_header_is_non_numeric() {
            let server = MockServer::new(vec![http_response_with_headers(
                "200 OK",
                &[("Total-Results", "abc")],
                ITEMS,
            )]);
            let base = server.url();
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url());
            let page = ZoteroClient::new(&state)
                .get_items_with_total(&url)
                .await
                .unwrap();

            assert_eq!(page.total, None);
            assert_eq!(page.items.len(), 1);
        }
    }

    mod add_pagination {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn add_pagination_appends_query_to_url_without_existing_query() {
            assert_eq!(
                add_pagination("http://x/items", 10, 25),
                "http://x/items?start=10&limit=25"
            );
        }

        #[test]
        fn preserves_existing_query_string() {
            assert_eq!(
                add_pagination("http://x/items?foo=1", 0, 2),
                "http://x/items?foo=1&start=0&limit=2"
            );
        }
    }
}
