//! Shared runtime state threaded through every MCP tool handler.
//!
//! [`AppState`] bundles the configured backend URLs and a shared
//! [`reqwest::Client`], plus the write-permission gate that every mutating
//! operation checks before touching the Zotero library. This module also
//! provides [`AppState::send_with_retry`], the single retry policy used by
//! all three backend clients.

use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use reqwest::{Client, RequestBuilder, Response, StatusCode};
use tokio::sync::OnceCell;

use crate::{
    errors::ZoteroMcpError,
    security::SecurityConfig,
    zotero::{LocalZoteroDb, find_zotero_db},
};

const RETRY_MAX_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(200);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(5);

/// Shared configuration and HTTP client for the Zotero, Better `BibTeX`, and
/// Better Notes backends.
///
/// Constructed once at startup via [`AppState::from_env`] and passed by
/// reference to every backend client for the lifetime of the server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolExposureMode {
    Compact,
    Gated,
    All,
}

impl ToolExposureMode {
    fn from_env() -> Self {
        env::var("ZOTERO_MCP_MODE")
            .map_or(Self::Compact, |value| Self::from_env_value(&value))
    }

    fn from_env_value(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "all" => Self::All,
            "gated" => Self::Gated,
            _ => Self::Compact,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CachedLocalZoteroDb {
    override_path: Option<PathBuf>,
    db: LocalZoteroDb,
}

#[derive(Clone, Debug)]
pub(crate) struct AppState {
    /// Shared [`Client`] connection pool.
    pub(crate) client: Client,
    /// Base URL for the Zotero Local HTTP API.
    pub(crate) zotero_api_url: String,
    /// Base URL for the Better `BibTeX` JSON-RPC endpoint.
    pub(crate) better_bibtex_url: String,
    /// Base URL for the Better Notes companion bridge endpoint.
    pub(crate) better_notes_url: String,
    /// Base URL for the `CrossRef` Works API (DOI resolution).
    pub(crate) crossref_url: String,
    /// Base URL for the Semantic Scholar Graph API (arXiv ID resolution).
    pub(crate) semantic_scholar_url: String,
    /// Base URL for the Open Library Books API (ISBN resolution).
    pub(crate) open_library_url: String,
    /// Security profile, path allowlists, and parser size caps.
    pub(crate) security: SecurityConfig,
    /// Whether write/mutation operations are allowed. Defaults to read-only;
    /// enable by setting `ZOTERO_WRITE_ENABLED`.
    pub(crate) write_enabled: bool,
    /// Whether direct read access to the local Zotero `SQLite` database is
    /// allowed. Defaults to false; enable by setting `ZOTERO_SQLITE_ACCESS`.
    pub(crate) sqlite_access: bool,
    /// Optional direct path to `zotero.sqlite` for local database reads.
    pub(crate) zotero_db_path: Option<PathBuf>,
    /// Cached read-only local database handle shared across SQLite tool calls.
    pub(crate) local_zotero_db: Arc<OnceCell<CachedLocalZoteroDb>>,
    /// Which MCP tools are advertised to clients. Defaults to compact mode.
    pub(crate) tool_mode: ToolExposureMode,
}

impl AppState {
    /// Builds an [`AppState`] from environment variables.
    ///
    /// Reads `ZOTERO_API_URL`, `BETTER_BIBTEX_URL`, `BETTER_NOTES_URL`,
    /// `CROSSREF_URL`, `SEMANTIC_SCHOLAR_URL`, and `OPEN_LIBRARY_URL` for the
    /// backend URLs (defaulting to standard local Zotero plugin ports or
    /// public endpoints when unset), and `ZOTERO_WRITE_ENABLED` (`"1"` or
    /// `"true"`, case-insensitive) to opt into write operations, defaulting to
    /// read-only. `ZOTERO_SQLITE_ACCESS` (`"1"` or `"true"`,
    /// case-insensitive) likewise gates direct reads of the local Zotero
    /// `SQLite` database, defaulting to disabled. `ZOTERO_DB_PATH` optionally
    /// points directly to `zotero.sqlite` when `SQLite` access is enabled.
    /// Returns the constructed [`AppState`].
    pub(crate) fn from_env() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| Client::new());

        let zotero_api_url = env::var("ZOTERO_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:23119/api".to_owned());

        let better_bibtex_url =
            env::var("BETTER_BIBTEX_URL").unwrap_or_else(|_| {
                "http://127.0.0.1:23119/better-bibtex/json-rpc".to_owned()
            });

        let better_notes_url =
            env::var("BETTER_NOTES_URL").unwrap_or_else(|_| {
                "http://127.0.0.1:23119/better-notes".to_owned()
            });

        let crossref_url = env::var("CROSSREF_URL")
            .unwrap_or_else(|_| "https://api.crossref.org".to_owned());
        let semantic_scholar_url = env::var("SEMANTIC_SCHOLAR_URL")
            .unwrap_or_else(|_| "https://api.semanticscholar.org".to_owned());
        let open_library_url = env::var("OPEN_LIBRARY_URL")
            .unwrap_or_else(|_| "https://openlibrary.org".to_owned());

        let write_enabled = env::var("ZOTERO_WRITE_ENABLED")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

        let sqlite_access = env::var("ZOTERO_SQLITE_ACCESS")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let zotero_db_path = env::var_os("ZOTERO_DB_PATH").map(PathBuf::from);
        let tool_mode = ToolExposureMode::from_env();

        Self {
            client,
            zotero_api_url,
            better_bibtex_url,
            better_notes_url,
            crossref_url,
            semantic_scholar_url,
            open_library_url,
            security: SecurityConfig::from_env(),
            write_enabled,
            sqlite_access,
            zotero_db_path,
            local_zotero_db: Self::local_zotero_db_cache(),
            tool_mode,
        }
    }

    /// Checks whether write operations are permitted.
    ///
    /// Every mutating backend call must invoke this before touching the
    /// Zotero library.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if [`write_enabled`] is `false` (the default)
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`write_enabled`]: Self::write_enabled
    pub(crate) fn check_write_permission(&self) -> Result<(), ZoteroMcpError> {
        if self.write_enabled {
            Ok(())
        } else {
            Err(ZoteroMcpError::PermissionDenied(
                "Write operation rejected: set ZOTERO_WRITE_ENABLED=1 to \
                 enable modifying Zotero library"
                    .to_owned(),
            ))
        }
    }

    /// Checks whether local `SQLite` database read access is permitted.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if `sqlite_access` is `false` (the default)
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    pub(crate) fn check_sqlite_access(&self) -> Result<(), ZoteroMcpError> {
        if self.sqlite_access {
            Ok(())
        } else {
            Err(ZoteroMcpError::PermissionDenied(
                "Local sqlite access is disabled: set ZOTERO_SQLITE_ACCESS=1 \
                 to enable reading the Zotero database directly"
                    .to_owned(),
            ))
        }
    }

    pub(crate) fn local_zotero_db_cache() -> Arc<OnceCell<CachedLocalZoteroDb>>
    {
        Arc::new(OnceCell::new())
    }

    /// Returns the cached local Zotero database, opening it on first use.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if local `SQLite` access is disabled
    /// - [`LocalDb`] if the database cannot be located
    /// - [`Sqlite`] if opening or probing the database fails
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`LocalDb`]: ZoteroMcpError::LocalDb
    /// [`Sqlite`]: ZoteroMcpError::Sqlite
    pub(crate) async fn local_zotero_db(
        &self,
    ) -> Result<&LocalZoteroDb, ZoteroMcpError> {
        self.check_sqlite_access()?;
        let override_path = self.zotero_db_path.clone();
        let cached = self
            .local_zotero_db
            .get_or_try_init(|| async move {
                let Some(db_path) = find_zotero_db(override_path.as_deref())
                else {
                    return Err(ZoteroMcpError::LocalDb(
                        "Zotero sqlite database not found".to_owned(),
                    ));
                };
                let db = LocalZoteroDb::open(&db_path).await?;
                Ok(CachedLocalZoteroDb {
                    override_path,
                    db,
                })
            })
            .await?;
        if cached.override_path == self.zotero_db_path {
            Ok(&cached.db)
        } else {
            Err(ZoteroMcpError::LocalDb(
                "cached Zotero sqlite database path no longer matches state"
                    .to_owned(),
            ))
        }
    }

    /// Checks if direct file path access is enabled by security policy.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if direct file path access is disabled
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) fn check_direct_file_paths_enabled(
        &self,
    ) -> Result<(), ZoteroMcpError> {
        self.security.check_direct_file_paths_enabled()
    }

    /// Validates that an existing `path` falls under one of the allowed
    /// `roots`.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if `path` is not inside an allowed root directory
    /// - [`Io`] if `path` does not exist or canonicalization fails
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    /// [`Io`]: ZoteroMcpError::Io
    pub(crate) fn check_existing_read_path<'a, I>(
        &self,
        path: &Path,
        roots: I,
        purpose: &str,
    ) -> Result<PathBuf, ZoteroMcpError>
    where
        I: IntoIterator<Item = &'a PathBuf>,
    {
        self.security.check_existing_read_path(path, roots, purpose)
    }

    /// Validates that an output `path` target directory is allowed for writes.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if output parent directory is missing or not inside
    ///   allowed `roots`
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) fn check_output_path(
        &self,
        path: &Path,
        roots: &[PathBuf],
        purpose: &str,
    ) -> Result<PathBuf, ZoteroMcpError> {
        self.security.check_output_path(path, roots, purpose)
    }

    /// Checks that `path` points to a `.pdf` file within maximum allowed byte
    /// limits.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if `path` lacks a `.pdf` extension or exceeds
    ///   maximum byte limits
    /// - [`Io`] if file metadata retrieval fails
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    /// [`Io`]: ZoteroMcpError::Io
    pub(crate) fn check_pdf_file(
        &self,
        path: &Path,
    ) -> Result<(), ZoteroMcpError> {
        self.security.check_pdf_file(path)
    }

    /// Validates that `markdown` content does not exceed maximum byte limits.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds maximum byte limits
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) fn check_markdown_size(
        &self,
        markdown: &str,
    ) -> Result<(), ZoteroMcpError> {
        self.security.check_markdown_size(markdown)
    }

    /// Validates that `html` content does not exceed maximum byte limits.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds maximum byte limits
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) fn check_html_size(
        &self,
        html: &str,
    ) -> Result<(), ZoteroMcpError> {
        self.security.check_html_size(html)
    }

    /// Validates that template `name` does not exceed maximum byte limits.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds maximum byte limits
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) fn check_template_name_size(
        &self,
        name: &str,
    ) -> Result<(), ZoteroMcpError> {
        self.security.check_template_name_size(name)
    }

    /// Sends `req`, retrying transient failures with exponential backoff.
    ///
    /// Retries on `5xx` responses, HTTP 429, timeouts, and connect errors, up
    /// to [`RETRY_MAX_ATTEMPTS`] attempts total, doubling the delay from
    /// [`RETRY_BASE_DELAY`] and capping it at [`RETRY_MAX_DELAY`]. Returns
    /// the first [`Response`] that isn't a transient failure, or the final
    /// attempt's outcome once retries are exhausted.
    ///
    /// # Errors
    ///
    /// - [`Network`] if every attempt fails at the transport level
    ///
    /// [`Network`]: ZoteroMcpError::Network
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
                Ok(resp) if !is_transient_status(resp.status()) => {
                    return Ok(resp);
                }
                Err(e) if !is_transient_error(&e) => return Err(e.into()),
                Ok(_) | Err(_) => {}
            }
            tokio::time::sleep(delay).await;
            delay = delay.saturating_mul(2).min(RETRY_MAX_DELAY);
        }
        req.send().await.map_err(Into::into)
    }

    /// Reads HTTP `resp` up to `max_bytes`, returning the body text.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if body length exceeds `max_bytes` or contains
    ///   invalid UTF-8
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) async fn read_limited_text(
        &self,
        mut resp: Response,
        max_bytes: usize,
        context: &str,
    ) -> Result<String, ZoteroMcpError> {
        let max_bytes_u64 = u64::try_from(max_bytes).unwrap_or(u64::MAX);
        if resp.content_length().is_some_and(|len| len > max_bytes_u64) {
            return Err(ZoteroMcpError::InputRejected(format!(
                "{context} exceeds {max_bytes} bytes"
            )));
        }

        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            if body.len().saturating_add(chunk.len()) > max_bytes {
                return Err(ZoteroMcpError::InputRejected(format!(
                    "{context} exceeds {max_bytes} bytes"
                )));
            }
            body.extend_from_slice(&chunk);
        }

        String::from_utf8(body).map_err(|_| {
            ZoteroMcpError::InputRejected(format!(
                "{context} is not valid UTF-8"
            ))
        })
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

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use reqwest::Client;

        use super::{AppState, ToolExposureMode};
        use crate::security::SecurityConfig;

        /// Builds an [`AppState`] with empty backend URLs, for tests that
        /// only exercise `write_enabled` or `send_with_retry`.
        pub(super) fn test_state(write_enabled: bool) -> AppState {
            AppState {
                client: Client::new(),
                zotero_api_url: String::new(),
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled,
                sqlite_access: false,
                zotero_db_path: None,
                local_zotero_db: AppState::local_zotero_db_cache(),
                tool_mode: ToolExposureMode::Compact,
                security: SecurityConfig::default(),
            }
        }

        /// Spawns a background thread serving one canned raw HTTP response
        /// per accepted connection, in order. Returns the bound
        /// `http://host:port` base URL.
        pub(super) fn mock_server(responses: Vec<&'static str>) -> String {
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

    mod tool_exposure_mode {
        use super::super::ToolExposureMode;

        #[test]
        fn parses_all_mode() {
            assert_eq!(
                ToolExposureMode::from_env_value("all"),
                ToolExposureMode::All
            );
        }

        #[test]
        fn parses_gated_mode() {
            assert_eq!(
                ToolExposureMode::from_env_value("gated"),
                ToolExposureMode::Gated
            );
        }

        #[test]
        fn defaults_unknown_mode_to_compact() {
            assert_eq!(
                ToolExposureMode::from_env_value("unknown"),
                ToolExposureMode::Compact
            );
        }
    }

    mod is_transient_status {
        use super::*;

        #[test]
        fn classifies_5xx_and_429_as_transient() {
            assert!(is_transient_status(StatusCode::INTERNAL_SERVER_ERROR));
            assert!(is_transient_status(StatusCode::BAD_GATEWAY));
            assert!(is_transient_status(StatusCode::TOO_MANY_REQUESTS));
        }

        #[test]
        fn classifies_success_and_non_429_client_errors_as_not_transient() {
            assert!(!is_transient_status(StatusCode::OK));
            assert!(!is_transient_status(StatusCode::NOT_FOUND));
            assert!(!is_transient_status(StatusCode::BAD_REQUEST));
        }
    }

    mod check_write_permission {
        use super::{super::*, fixtures::test_state};

        #[test]
        fn rejects_when_write_is_disabled_by_default() {
            // Arrange
            let state = test_state(false);

            // Act
            let result = state.check_write_permission();

            // Assert
            assert!(matches!(result, Err(ZoteroMcpError::PermissionDenied(_))));
        }

        #[test]
        fn allows_when_write_is_enabled() {
            // Arrange
            let state = test_state(true);

            // Act
            let result = state.check_write_permission();

            // Assert
            assert!(result.is_ok());
        }
    }

    mod check_sqlite_access {
        use super::{super::*, fixtures::test_state};

        #[test]
        fn permits_when_enabled() {
            // Arrange: fixture defaults to disabled; flip the gate on.
            let mut state = test_state(false);
            state.sqlite_access = true;

            // Act
            let result = state.check_sqlite_access();

            // Assert
            assert!(result.is_ok());
        }

        #[test]
        fn rejects_when_disabled() {
            // Arrange
            let state = test_state(false);

            // Act
            let result = state.check_sqlite_access();

            // Assert
            assert!(matches!(result, Err(ZoteroMcpError::PermissionDenied(_))));
        }
    }

    mod check_html_size {
        use super::{super::*, fixtures::test_state};

        #[test]
        fn check_html_size_rejects_oversized_html() {
            let state = test_state(false);
            let html = "x".repeat(state.security.max_html_bytes + 1);

            let result = state.check_html_size(&html);

            assert!(matches!(result, Err(ZoteroMcpError::InputRejected(_))));
        }
    }

    mod send_with_retry {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{mock_server, test_state},
        };

        #[tokio::test]
        async fn recovers_after_transient_5xx_errors() {
            // Arrange
            let base = mock_server(vec![
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: \
                 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: \
                 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: \
                 close\r\n\r\nok",
            ]);
            let state = test_state(false);
            let url = format!("{base}/");

            // Act
            let resp =
                state.send_with_retry(state.client.get(&url)).await.unwrap();

            // Assert
            assert_eq!(resp.status(), StatusCode::OK);
        }

        #[tokio::test]
        async fn returns_immediately_on_non_transient_status() {
            // Arrange: only one response is queued — a second accept()
            // would hang if a 404 were incorrectly retried.
            let base = mock_server(vec![
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: \
                 close\r\n\r\n",
            ]);
            let state = test_state(false);
            let url = format!("{base}/");

            // Act
            let resp =
                state.send_with_retry(state.client.get(&url)).await.unwrap();

            // Assert
            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        }

        #[tokio::test]
        async fn returns_last_response_after_exhausting_retries_on_persistent_5xx()
         {
            // Arrange: every attempt (RETRY_MAX_ATTEMPTS of them) stays
            // transient, so the final attempt's response is still returned
            // rather than an error.
            let responses =
                vec![
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Length: \
                     0\r\nConnection: close\r\n\r\n";
                    usize::try_from(RETRY_MAX_ATTEMPTS).unwrap_or(3)
                ];
            let base = mock_server(responses);
            let state = test_state(false);
            let url = format!("{base}/");

            // Act
            let resp =
                state.send_with_retry(state.client.get(&url)).await.unwrap();

            // Assert
            assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        }

        #[tokio::test]
        async fn returns_network_error_after_exhausting_retries_on_connection_refused()
         {
            // Arrange: port 0 is never a live listener, so every attempt is
            // refused — exercises is_transient_error's connect-error branch.
            let state = test_state(false);
            let url = "http://127.0.0.1:0/";

            // Act
            let err =
                state.send_with_retry(state.client.get(url)).await.unwrap_err();

            // Assert
            assert!(matches!(err, ZoteroMcpError::Network(_)));
        }
    }
}
