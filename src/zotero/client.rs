//! Async client for the Zotero Local HTTP API.

use crate::{state::AppState, zotero::models::LocalApiStatus};

/// Client for the Zotero Local HTTP API, scoped to a single tool call.
pub(crate) struct ZoteroClient<'a> {
    pub(super) state: &'a AppState,
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
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        pub(crate) fn test_state(
            zotero_api_url: String,
            write_enabled: bool,
        ) -> AppState {
            AppState {
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                write_enabled,
                ..AppState::from_env()
            }
        }

        pub(crate) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\n\r\n{body}",
                body.len()
            )
        }

        #[expect(
            clippy::excessive_nesting,
            reason = "mock HTTP server thread loop"
        )]
        pub(crate) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let Ok((mut stream, _)) = listener.accept() else {
                        continue;
                    };
                    let mut buf = [0u8; 1024];
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
}
