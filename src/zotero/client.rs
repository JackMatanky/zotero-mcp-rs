//! Async client for the Zotero Local HTTP API.

use reqwest::Response;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    errors::ZoteroMcpError, state::AppState, zotero::models::LocalApiStatus,
};

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
        version: u64,
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
    ) -> Result<u64, ZoteroMcpError> {
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
            .ok_or_else(|| ZoteroMcpError::LocalApi {
                status: 0,
                message: "Missing or invalid Last-Modified-Version header"
                    .to_owned(),
            })
    }
}

#[cfg(test)]
/// Shared test helpers for Zotero Local API client modules.
pub(crate) mod tests {
    use super::*;

    /// Fixtures and HTTP mock server for testing.
    pub(crate) mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        /// Builds an [`AppState`] fixture for testing with `zotero_api_url` and
        /// `write_enabled`.
        pub(crate) fn test_state(
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
                ..AppState::from_env()
            }
        }

        /// Formats a minimal HTTP response string with `status` and `body`.
        pub(crate) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Spawns a fixture HTTP server returning `responses` and returns its
        /// base URL.
        pub(crate) fn mock_server(responses: Vec<String>) -> String {
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
                .delete(&state.zotero_api_url.clone(), 5)
                .await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn returns_local_api_error_on_412() {
            let base =
                mock_server(vec![http_response("412 Precondition Failed", "")]);
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .delete(&state.zotero_api_url.clone(), 5)
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroMcpError::LocalApi { .. }));
        }
    }

    mod get_library_version {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use pretty_assertions::assert_eq;

        use super::{fixtures::test_state, *};

        #[tokio::test]
        async fn reads_last_modified_version_header() {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                let (mut stream, _) =
                    listener.accept().expect("accept connection");
                let mut buf = [0_u8; 1024];
                let _ = stream.read(&mut buf);
                let response = "HTTP/1.1 200 OK\r\nContent-Length: \
                                2\r\nContent-Type: \
                                application/json\r\nLast-Modified-Version: \
                                42\r\nConnection: close\r\n\r\n[]";
                let _ = stream.write_all(response.as_bytes());
            });
            let state = test_state(format!("http://{addr}"), false);

            let version =
                ZoteroClient::new(&state).get_library_version().await.unwrap();

            assert_eq!(version, 42);
        }

        #[tokio::test]
        async fn errors_when_header_missing() {
            let base = fixtures::mock_server(vec![fixtures::http_response(
                "200 OK", "[]",
            )]);
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .get_library_version()
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroMcpError::LocalApi { .. }));
        }
    }
}
