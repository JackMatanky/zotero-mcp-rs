use crate::errors::ZoteroMcpError;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use std::env;
use std::time::Duration;

const RETRY_MAX_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(200);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub(crate) struct AppState {
    pub(crate) client: Client,
    pub(crate) zotero_api_url: String,
    pub(crate) better_bibtex_url: String,
    pub(crate) better_notes_url: String,
    // ponytail: write gate defaults to read-only; enabled via ZOTERO_WRITE_ENABLED
    pub(crate) write_enabled: bool,
}

impl AppState {
    pub(crate) fn from_env() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| Client::new());

        let zotero_api_url =
            env::var("ZOTERO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:23119/api".to_string());

        let better_bibtex_url = env::var("BETTER_BIBTEX_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:23119/better-bibtex/json-rpc".to_string());

        let better_notes_url = env::var("BETTER_NOTES_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:23119/better-notes".to_string());

        let write_enabled = env::var("ZOTERO_WRITE_ENABLED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Self {
            client,
            zotero_api_url,
            better_bibtex_url,
            better_notes_url,
            write_enabled,
        }
    }

    pub(crate) fn check_write_permission(&self) -> Result<(), ZoteroMcpError> {
        if !self.write_enabled {
            Err(ZoteroMcpError::PermissionDenied(
                "Write operation rejected: set ZOTERO_WRITE_ENABLED=1 to enable modifying Zotero library".to_string()
            ))
        } else {
            Ok(())
        }
    }

    /// Send a request, retrying transient failures (5xx, 429, timeouts, connect errors)
    /// with exponential backoff (200ms base, 5s cap, 3 attempts total).
    pub(crate) async fn send_with_retry(
        &self,
        req: RequestBuilder,
    ) -> Result<Response, ZoteroMcpError> {
        let mut delay = RETRY_BASE_DELAY;
        for _ in 1..RETRY_MAX_ATTEMPTS {
            let Some(attempt_req) = req.try_clone() else {
                return req.send().await.map_err(Into::into);
            };
            match attempt_req.send().await {
                Ok(resp) if !is_transient_status(resp.status()) => return Ok(resp),
                Ok(_) => {}
                Err(e) if !is_transient_error(&e) => return Err(e.into()),
                Err(_) => {}
            }
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(RETRY_MAX_DELAY);
        }
        req.send().await.map_err(Into::into)
    }
}

fn is_transient_status(status: StatusCode) -> bool {
    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
}

fn is_transient_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn test_is_transient_status() {
        assert!(is_transient_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_transient_status(StatusCode::BAD_GATEWAY));
        assert!(is_transient_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(!is_transient_status(StatusCode::OK));
        assert!(!is_transient_status(StatusCode::NOT_FOUND));
        assert!(!is_transient_status(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn test_send_with_retry_recovers_from_transient_5xx() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            for i in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let body = if i < 2 {
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                };
                let _ = stream.write_all(body.as_bytes());
            }
        });

        let state = AppState {
            client: Client::new(),
            zotero_api_url: String::new(),
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            write_enabled: false,
        };

        let url = format!("http://{addr}/");
        let resp = state.send_with_retry(state.client.get(&url)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
