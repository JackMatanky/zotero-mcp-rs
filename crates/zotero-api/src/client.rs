//! Async HTTP client for the Zotero Local API.
//!
//! Defines [`ZoteroClient`], the primary HTTP request builder and dispatcher
//! for Zotero Local API operations. The client handles target library scoping,
//! authentication headers, error conversion, and response decoding.
//!
//! # Key Types
//!
//! - [`ZoteroClient`]: API client borrowing shared application state.
//! - [`LibraryTarget`]: Enum selecting User or Group library target.
//! - [`LocalAuthResponse`]: Authentication token response payload.
//!
//! # Examples
//!
//! ```no_run
//! use zotero_api::{AppState, ZoteroClient};
//!
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
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    errors::ZoteroApiError,
    keys::LibraryVersion,
    objects::{LocalApiStatus, ZoteroItem},
    state::AppState,
};

/// One page of Zotero items and the optional `Total-Results` header count.
pub(super) struct ItemsPage {
    /// Fetched items for the requested page.
    pub(super) items: Vec<ZoteroItem>,
    /// Total number of matching items across all pages, if provided by Zotero.
    pub(super) total: Option<usize>,
}

/// Target Zotero library (User or Group).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LibraryTarget {
    /// User library with ID (default `User(0)` for active local user).
    User(u64),
    /// Group library with group ID.
    Group(u64),
}

impl Default for LibraryTarget {
    #[inline]
    fn default() -> Self {
        Self::User(0)
    }
}

impl LibraryTarget {
    /// Returns the URL path prefix for this library target (e.g. `/users/0` or
    /// `/groups/12345`).
    #[must_use]
    #[inline]
    pub fn target_prefix(&self) -> String {
        match self {
            Self::User(id) => format!("/users/{id}"),
            Self::Group(id) => format!("/groups/{id}"),
        }
    }
}

/// Response payload returned by `POST /api/local/authorize`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalAuthResponse {
    /// Generated local API write key / token.
    pub secret: String,
    /// Optional backoff delay in seconds if user interaction is pending.
    pub backoff: Option<u64>,
}

/// Client for the Zotero Local HTTP API, scoped to a single tool call.
///
/// Wraps shared application state ([`AppState`]) to issue HTTP requests against
/// Zotero's local REST API with automatic retries and error mapping.
pub struct ZoteroClient<'a> {
    /// Borrowed shared application state containing HTTP client and API
    /// configuration.
    pub(super) state: &'a AppState,
    /// Target Zotero library scope.
    pub(super) target: LibraryTarget,
}

impl<'a> ZoteroClient<'a> {
    /// Creates a Zotero Local API client borrowing shared [`AppState`].
    #[must_use]
    #[inline]
    pub fn new(state: &'a AppState) -> Self {
        Self {
            state,
            target: LibraryTarget::default(),
        }
    }

    /// Scopes the client to a specific [`LibraryTarget`] (User or Group).
    #[must_use]
    #[inline]
    pub fn with_target(mut self, target: LibraryTarget) -> Self {
        self.target = target;
        self
    }

    /// Returns the active [`LibraryTarget`].
    #[must_use]
    #[inline]
    pub fn target(&self) -> LibraryTarget {
        self.target
    }

    /// Returns the target library URL prefix (e.g. `/users/0` or
    /// `/groups/12345`).
    #[must_use]
    #[inline]
    pub fn target_prefix(&self) -> String {
        self.target.target_prefix()
    }

    /// Applies local write key headers if configured in [`AppState`].
    pub(super) fn apply_auth_headers(
        &self,
        mut req: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        if let Some(key) = self.state.local_write_key() {
            req = req.header("Zotero-Write-Key", key);
        }
        req
    }

    /// Probes the Zotero Local API for availability.
    ///
    /// Issues a lightweight request against the items endpoint. Connection and
    /// HTTP status failures are captured in the returned
    /// [`LocalApiStatus::error`] field rather than being returned as an error.
    #[inline]
    pub async fn check_status(&self) -> LocalApiStatus {
        let url = format!(
            "{}{}/items?limit=1",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let req = self.apply_auth_headers(self.state.client().get(&url));
        match req.send().await {
            Ok(resp) => {
                if let Some(header_val) = resp.headers().get("zotero-server-id")
                {
                    if let Ok(server_id) = header_val.to_str() {
                        self.state.set_server_id(server_id);
                    }
                }
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
    /// - [`ZoteroApiError::LocalApi`]: If `resp` status is not a successful
    ///   HTTP status.
    pub(super) async fn ensure_success(
        &self,
        resp: Response,
    ) -> Result<Response, ZoteroApiError> {
        if let Some(header_val) = resp.headers().get("zotero-server-id") {
            if let Ok(server_id) = header_val.to_str() {
                match self.state.server_id() {
                    Some(expected_id) if expected_id != server_id => {
                        return Err(ZoteroApiError::LocalApi {
                            status: 412,
                            message: format!(
                                "Zotero Server ID mismatch: expected \
                                 '{expected_id}', got '{server_id}'"
                            ),
                        });
                    }
                    None => self.state.set_server_id(server_id),
                    _ => {}
                }
            }
        }

        if resp.status().is_success() {
            return Ok(resp);
        }
        Err(ZoteroApiError::LocalApi {
            status: resp.status().as_u16(),
            message: resp.text().await.unwrap_or_default(),
        })
    }

    /// Requests local API write authorization from Zotero via `POST
    /// /api/local/authorize`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`]: If Zotero rejects the request.
    /// - [`ZoteroApiError::Network`]: Transport errors.
    #[inline]
    pub async fn request_local_authorization(
        &self,
        app_name: &str,
    ) -> Result<LocalAuthResponse, ZoteroApiError> {
        let base = self.state.zotero_api_url().trim_end_matches('/');
        let url = if base.ends_with("/api") {
            format!("{base}/local/authorize")
        } else {
            format!("{base}/api/local/authorize")
        };
        let body = serde_json::json!({ "appName": app_name });
        let req = self.state.client().post(&url).json(&body);
        let resp = self.state.send_with_retry(req).await?;
        let auth: LocalAuthResponse =
            self.ensure_success(resp).await?.json().await?;
        self.state.set_local_write_key(&auth.secret);
        Ok(auth)
    }

    /// Fetches JSON data from `url` and decodes the response body.
    ///
    /// Returns the decoded payload of type `T`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`]: If Zotero responds with a non-2xx HTTP
    ///   status.
    /// - [`ZoteroApiError::Network`]: If the request fails at the transport
    ///   level.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    /// [`Json`]: ZoteroApiError::Json
    pub(super) async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, ZoteroApiError> {
        let req = self.apply_auth_headers(self.state.client().get(url));
        let resp = self.state.send_with_retry(req).await?;
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
        let req = self.apply_auth_headers(self.state.client().get(url));
        let resp = self.state.send_with_retry(req).await?;
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
        let req = self
            .apply_auth_headers(self.state.client().post(url).json(payload));
        let resp = self.state.send_with_retry(req).await?;
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
            .apply_auth_headers(self.state.client().delete(url))
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
        let url = format!(
            "{}{}/items?limit=1",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let req = self.apply_auth_headers(self.state.client().get(&url));
        let resp =
            self.ensure_success(self.state.send_with_retry(req).await?).await?;
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
        pub(super) use super::super::test_http::{
            MockServer, http_response, http_response_with_headers,
        };
        use crate::state::AppState;

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
#[cfg(any(test, feature = "test-util"))]
pub mod test_http {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread::{self, JoinHandle},
    };

    pub type RequestLog = Arc<Mutex<Vec<String>>>;

    pub struct MockServer {
        base_url: String,
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockServer {
        #[must_use]
        #[inline]
        pub fn new(responses: Vec<String>) -> Self {
            Self::with_log(responses, None)
        }

        #[must_use]
        #[inline]
        pub fn recording(responses: Vec<String>) -> (Self, RequestLog) {
            let recorded = Arc::new(Mutex::new(Vec::new()));
            let server = Self::with_log(responses, Some(Arc::clone(&recorded)));
            (server, recorded)
        }

        #[must_use]
        #[inline]
        pub fn url(&self) -> &str {
            &self.base_url
        }

        fn with_log(
            responses: Vec<String>,
            recorded: Option<RequestLog>,
        ) -> Self {
            #[expect(clippy::expect_used, reason = "test-only mock server")]
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind test listener");
            #[expect(clippy::expect_used, reason = "test-only mock server")]
            let addr = listener.local_addr().expect("test listener address");
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let handle = thread::spawn(move || {
                serve_responses(
                    &listener,
                    &responses,
                    recorded.as_ref(),
                    &thread_stop,
                );
            });

            Self {
                base_url: format!("http://{addr}"),
                stop,
                handle: Some(handle),
            }
        }
    }

    impl Drop for MockServer {
        #[inline]
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(addr) = self.base_url.strip_prefix("http://") {
                let _ = TcpStream::connect(addr);
            }
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    #[must_use]
    #[inline]
    pub fn http_response(status: &str, body: &str) -> String {
        http_response_with_headers(status, &[], body)
    }

    #[must_use]
    #[inline]
    pub fn http_response_with_headers(
        status: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> String {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
             application/json\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }
        response.push_str("Connection: close\r\n\r\n");
        response.push_str(body);
        response
    }

    fn serve_responses(
        listener: &TcpListener,
        responses: &[String],
        recorded: Option<&RequestLog>,
        stop: &AtomicBool,
    ) {
        for response in responses {
            if !serve_response(listener, response, recorded, stop) {
                break;
            }
        }
    }

    fn serve_response(
        listener: &TcpListener,
        response: &str,
        recorded: Option<&RequestLog>,
        stop: &AtomicBool,
    ) -> bool {
        let Ok((mut stream, _)) = listener.accept() else {
            return false;
        };
        if stop.load(Ordering::Acquire) {
            return false;
        }
        record_or_drain_request(&mut stream, recorded);
        let _ = stream.write_all(response.as_bytes());
        true
    }

    fn record_or_drain_request(
        stream: &mut TcpStream,
        recorded: Option<&RequestLog>,
    ) {
        if let Some(recorded) = recorded {
            #[expect(clippy::expect_used, reason = "test-only mock server")]
            let mut log = recorded.lock().expect("request log lock");
            log.push(read_request(stream));
            return;
        }
        let mut buf = [0_u8; 1024];
        let _ = stream.read(&mut buf);
    }

    /// Parses request body JSON string.
    ///
    /// # Errors
    ///
    /// - [`serde_json::Error`]: If JSON parsing fails.
    #[inline]
    pub fn request_body(
        raw: &str,
    ) -> Result<serde_json::Value, serde_json::Error> {
        let body = raw.split_once("\r\n\r\n").map_or("", |(_, body)| body);
        serde_json::from_str(body)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut buf = [0_u8; 1024];
        let mut data = Vec::new();
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            data.extend_from_slice(buf.get(..n).unwrap_or_default());
            if request_complete(&data) {
                break;
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    fn request_complete(data: &[u8]) -> bool {
        let Some((head_end, content_length)) = request_meta(data) else {
            return false;
        };
        data.len() >= head_end.saturating_add(content_length)
    }

    fn request_meta(data: &[u8]) -> Option<(usize, usize)> {
        let head_end =
            data.windows(4).position(|w| w == b"\r\n\r\n")?.saturating_add(4);
        let head =
            String::from_utf8_lossy(data.get(..head_end).unwrap_or_default());
        let content_length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        Some((head_end, content_length))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn drop_stops_server_with_unconsumed_responses() {
            let server = MockServer::new(vec![http_response("200 OK", "{}")]);
            drop(server);
        }
    }
}
