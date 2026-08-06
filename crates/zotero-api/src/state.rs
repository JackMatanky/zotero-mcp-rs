//! Shared runtime state threaded through every MCP tool handler.
//!
//! [`AppState`] bundles the configured backend URLs and a shared
//! [`reqwest::Client`], plus the write-permission gate that every mutating
//! operation checks before touching the Zotero library. This module also
//! provides [`AppState::send_with_retry`], the single retry policy used by all
//! three backend clients.
//!
//! # Examples
//!
//! ```no_run
//! use zotero_api::state::AppState;
//!
//! let state = AppState::from_env();
//! assert!(!state.is_write_enabled());
//! ```

use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use reqwest::{Client, RequestBuilder, Response, StatusCode};
use tokio::sync::OnceCell;

use crate::{
    errors::ZoteroApiError,
    security::SecurityConfig,
    semantic_search::{
        EmbeddingProvider, FastEmbedProvider, SemanticIndex, resolve_db_path,
        resolve_model_cache_dir,
    },
    zotero::{LocalZoteroDb, find_zotero_db},
};

const RETRY_MAX_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(200);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(5);

/// Cached handle to a Zotero `SQLite` database for a single library.
#[derive(Clone, Debug)]
pub(crate) struct CachedLocalZoteroDb {
    override_path: Option<PathBuf>,
    db: LocalZoteroDb,
}

/// Shared configuration and HTTP client for the Zotero, Better `BibTeX`, and
/// Better Notes backends.
///
/// Constructed once at startup via [`AppState::from_env`] and passed by
/// reference to every backend client for the lifetime of the server.
#[derive(Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "four independent env-var feature gates \
              (write/sqlite/semantic/connector_compat), not combinatorial UI \
              state"
)]
pub struct AppState {
    // Infrastructure & Security Profile
    /// Shared [`Client`] connection pool.
    client: Client,
    /// Security profile, path allowlists, and parser size caps.
    security: SecurityConfig,

    // Backend Base URLs
    /// Base URL for the Zotero Local HTTP API.
    zotero_api_url: String,
    /// Base URL for the Better `BibTeX` JSON-RPC endpoint.
    better_bibtex_url: String,
    /// Base URL for the Better Notes companion bridge endpoint.
    better_notes_url: String,
    /// Base URL for the `CrossRef` Works API (DOI resolution).
    crossref_url: String,
    /// Base URL for the Semantic Scholar Graph API (arXiv ID resolution).
    semantic_scholar_url: String,
    /// Base URL for the Open Library Books API (ISBN resolution).
    open_library_url: String,

    // Feature Gates & Permission Flags
    /// Whether write/mutation operations are allowed. Defaults to read-only;
    /// enable by setting `ZOTERO_WRITE_ENABLED`.
    write_enabled: bool,
    /// Whether direct read access to the local Zotero `SQLite` database is
    /// allowed. Defaults to false; enable by setting `ZOTERO_SQLITE_ACCESS`.
    sqlite_access: bool,
    /// Whether local semantic-search indexing/querying is allowed. Defaults
    /// to false; enable by setting `ZOTERO_SEMANTIC_SEARCH`.
    semantic_search_enabled: bool,
    /// Whether single-purpose connector compatibility tools (`search`,
    /// `fetch`) are enabled. Defaults to false; enable by setting
    /// `ZOTERO_CONNECTOR_COMPAT`.
    connector_compat: bool,

    // Local Storage & Cached Handles
    /// Optional direct path to `zotero.sqlite` for local database reads.
    zotero_db_path: Option<PathBuf>,
    /// Cached read-only local database handle shared across `SQLite` tool
    /// calls.
    local_zotero_db: Arc<OnceCell<CachedLocalZoteroDb>>,
    /// Optional direct path to the semantic search `SQLite` index file.
    semantic_db_path: Option<PathBuf>,
    /// Cached semantic index handle, opened lazily on first use.
    semantic_index: Arc<OnceCell<SemanticIndex>>,
    /// Cached embedding provider, loaded lazily on first use.
    embedding_provider: Arc<OnceCell<Arc<dyn EmbeddingProvider>>>,
}

impl AppState {
    /// Builds an [`AppState`] from environment variables.
    ///
    /// Reads backend URLs and feature gate configuration from the environment:
    ///
    /// * `ZOTERO_API_URL`: Base URL for Zotero Local HTTP API (default `http://127.0.0.1:23119/api`).
    /// * `BETTER_BIBTEX_URL`: Base URL for Better `BibTeX` JSON-RPC (default `http://127.0.0.1:23119/better-bibtex/json-rpc`).
    /// * `BETTER_NOTES_URL`: Base URL for Better Notes bridge (default `http://127.0.0.1:23119/better-notes`).
    /// * `CROSSREF_URL`: Base URL for `CrossRef` Works API (default `https://api.crossref.org`).
    /// * `SEMANTIC_SCHOLAR_URL`: Base URL for Semantic Scholar API (default `https://api.semanticscholar.org`).
    /// * `OPEN_LIBRARY_URL`: Base URL for Open Library Books API (default `https://openlibrary.org`).
    /// * `ZOTERO_WRITE_ENABLED`: Enables write operations when set to `"1"` or
    ///   `"true"` (default `false`).
    /// * `ZOTERO_SQLITE_ACCESS`: Enables direct local `SQLite` database reads
    ///   when set to `"1"` or `"true"` (default `false`).
    /// * `ZOTERO_DB_PATH`: Optional explicit path to `zotero.sqlite`.
    /// * `ZOTERO_SEMANTIC_SEARCH`: Enables local semantic search indexing when
    ///   set to `"1"` or `"true"` (default `false`).
    /// * `ZOTERO_SEMANTIC_DB_PATH`: Optional explicit path to semantic search
    ///   index database.
    /// * `ZOTERO_CONNECTOR_COMPAT`: Enables single-purpose connector tools
    ///   (`search`, `fetch`) when set to `"1"` or `"true"` (default `false`).
    ///
    /// Returns the constructed [`AppState`].
    #[inline]
    pub fn from_env() -> Self {
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

        let semantic_search_enabled = env::var("ZOTERO_SEMANTIC_SEARCH")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let semantic_db_path =
            env::var_os("ZOTERO_SEMANTIC_DB_PATH").map(PathBuf::from);
        let connector_compat = env::var("ZOTERO_CONNECTOR_COMPAT")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

        Self {
            client,
            security: SecurityConfig::from_env(),
            zotero_api_url,
            better_bibtex_url,
            better_notes_url,
            crossref_url,
            semantic_scholar_url,
            open_library_url,
            write_enabled,
            sqlite_access,
            semantic_search_enabled,
            connector_compat,
            zotero_db_path,
            local_zotero_db: Self::local_zotero_db_cache(),
            semantic_db_path,
            semantic_index: Arc::new(OnceCell::new()),
            embedding_provider: Arc::new(OnceCell::new()),
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
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    /// [`write_enabled`]: Self::write_enabled
    pub(crate) fn check_write_permission(&self) -> Result<(), ZoteroApiError> {
        if self.write_enabled {
            Ok(())
        } else {
            Err(ZoteroApiError::PermissionDenied(
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
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    pub(crate) fn check_sqlite_access(&self) -> Result<(), ZoteroApiError> {
        if self.sqlite_access {
            Ok(())
        } else {
            Err(ZoteroApiError::PermissionDenied(
                "Local sqlite access is disabled: set ZOTERO_SQLITE_ACCESS=1 \
                 to enable reading the Zotero database directly"
                    .to_owned(),
            ))
        }
    }

    /// Returns an uninitialized cached handle for local Zotero `SQLite` access.
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
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    /// [`LocalDb`]: ZoteroApiError::LocalDb
    /// [`Sqlite`]: ZoteroApiError::Sqlite
    #[inline]
    pub async fn local_zotero_db(
        &self,
    ) -> Result<&LocalZoteroDb, ZoteroApiError> {
        self.check_sqlite_access()?;
        let override_path = self.zotero_db_path().map(Path::to_path_buf);
        let cached = self
            .local_zotero_db
            .get_or_try_init(|| async move {
                let Some(db_path) = find_zotero_db(override_path.as_deref())
                else {
                    return Err(ZoteroApiError::LocalDb(
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
        if cached.override_path.as_deref() == self.zotero_db_path() {
            Ok(&cached.db)
        } else {
            Err(ZoteroApiError::LocalDb(
                "cached Zotero sqlite database path no longer matches state"
                    .to_owned(),
            ))
        }
    }

    /// Checks whether local semantic search (indexing and querying) is
    /// permitted.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if `semantic_search_enabled` is `false` (default)
    ///
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    pub(crate) fn check_semantic_search_enabled(
        &self,
    ) -> Result<(), ZoteroApiError> {
        if self.semantic_search_enabled {
            Ok(())
        } else {
            Err(ZoteroApiError::PermissionDenied(
                "Semantic search is disabled: set ZOTERO_SEMANTIC_SEARCH=1 to \
                 enable local embedding indexing and search"
                    .to_owned(),
            ))
        }
    }

    /// Returns the cached semantic search index, opening (and creating, if
    /// missing) it on first use.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if semantic search is disabled
    /// - [`LocalDb`] / [`Io`] / [`Sqlite`] if the index cannot be opened
    ///
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    /// [`LocalDb`]: ZoteroApiError::LocalDb
    /// [`Io`]: ZoteroApiError::Io
    /// [`Sqlite`]: ZoteroApiError::Sqlite
    #[inline]
    pub async fn semantic_index(
        &self,
    ) -> Result<&SemanticIndex, ZoteroApiError> {
        self.check_semantic_search_enabled()?;
        let db_path = resolve_db_path(self.semantic_db_path.as_deref())?;
        self.semantic_index
            .get_or_try_init(|| SemanticIndex::open(&db_path))
            .await
    }

    /// Returns the cached embedding provider, loading the local ONNX model on
    /// first use.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if semantic search is disabled
    /// - [`LocalDb`] if the data directory cannot be resolved
    /// - [`Embedding`] if the model fails to load
    ///
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    /// [`LocalDb`]: ZoteroApiError::LocalDb
    /// [`Embedding`]: ZoteroApiError::Embedding
    #[inline]
    pub async fn embedding_provider(
        &self,
    ) -> Result<Arc<dyn EmbeddingProvider>, ZoteroApiError> {
        self.check_semantic_search_enabled()?;
        let db_path = resolve_db_path(self.semantic_db_path.as_deref())?;
        let cache_dir = resolve_model_cache_dir(&db_path);
        let provider = self
            .embedding_provider
            .get_or_try_init(|| async {
                tokio::task::spawn_blocking(move || {
                    FastEmbedProvider::load(&cache_dir)
                        .map(|p| -> Arc<dyn EmbeddingProvider> { Arc::new(p) })
                })
                .await
                .map_err(|e| ZoteroApiError::Embedding(e.to_string()))?
            })
            .await?;
        Ok(Arc::clone(provider))
    }

    /// Checks if direct filepath access is enabled by security policy.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if direct filepath access is disabled
    ///
    /// [`InputRejected`]: ZoteroApiError::InputRejected
    #[inline]
    pub fn check_direct_file_paths_enabled(
        &self,
    ) -> Result<(), ZoteroApiError> {
        self.security.check_direct_file_paths_enabled()
    }

    /// Validates that a path exists and falls under one of the allowed `roots`.
    ///
    /// # Arguments
    ///
    /// * `path` - Target path to validate.
    /// * `roots` - Iterator of allowed parent root directories.
    /// * `purpose` - Human-readable label for error reporting.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if `path` is not inside an allowed root directory
    /// - [`Io`] if `path` does not exist or canonicalization fails
    ///
    /// [`InputRejected`]: ZoteroApiError::InputRejected
    /// [`Io`]: ZoteroApiError::Io
    #[inline]
    pub fn check_existing_read_path<'a, I>(
        &self,
        path: &Path,
        roots: I,
        purpose: &str,
    ) -> Result<PathBuf, ZoteroApiError>
    where
        I: IntoIterator<Item = &'a PathBuf>,
    {
        self.security.check_existing_read_path(path, roots, purpose)
    }

    /// Validates that an output `path` target directory is allowed for writes.
    ///
    /// # Arguments
    ///
    /// * `path` - Output target file path.
    /// * `roots` - Slice of allowed export/output root directories.
    /// * `purpose` - Human-readable label for error reporting.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if output parent directory is missing or not inside
    ///   allowed `roots`
    ///
    /// [`InputRejected`]: ZoteroApiError::InputRejected
    /// Validates that an output `path` target directory is allowed for writes.
    pub(crate) fn check_output_path(
        &self,
        path: &Path,
        roots: &[PathBuf],
        purpose: &str,
    ) -> Result<PathBuf, ZoteroApiError> {
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
    /// [`InputRejected`]: ZoteroApiError::InputRejected
    /// [`Io`]: ZoteroApiError::Io
    #[inline]
    pub fn check_pdf_file(&self, path: &Path) -> Result<(), ZoteroApiError> {
        self.security.check_pdf_file(path)
    }

    /// Validates that `markdown` content does not exceed maximum byte limits.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds maximum byte limits
    ///
    /// [`InputRejected`]: ZoteroApiError::InputRejected
    pub(crate) fn check_markdown_size(
        &self,
        markdown: &str,
    ) -> Result<(), ZoteroApiError> {
        self.security.check_markdown_size(markdown)
    }

    /// Validates that `html` content does not exceed maximum byte limits.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds maximum byte limits
    ///
    /// [`InputRejected`]: ZoteroApiError::InputRejected
    pub(crate) fn check_html_size(
        &self,
        html: &str,
    ) -> Result<(), ZoteroApiError> {
        self.security.check_html_size(html)
    }

    /// Validates that template `name` does not exceed maximum byte limits.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds maximum byte limits
    ///
    /// [`InputRejected`]: ZoteroApiError::InputRejected
    pub(crate) fn check_template_name_size(
        &self,
        name: &str,
    ) -> Result<(), ZoteroApiError> {
        self.security.check_template_name_size(name)
    }

    /// Sends `req`, retrying transient failures with exponential backoff.
    ///
    /// Retries on `5xx` responses, HTTP 429, timeouts, and connect errors, up
    /// to [`RETRY_MAX_ATTEMPTS`] attempts total, doubling the delay from
    /// [`RETRY_BASE_DELAY`] and capping it at [`RETRY_MAX_DELAY`]. Returns the
    /// first [`Response`] that isn't a transient failure, or the final
    /// attempt's outcome once retries are exhausted.
    ///
    /// # Errors
    ///
    /// - [`Network`] if every attempt fails at the transport level
    ///
    /// [`Network`]: ZoteroApiError::Network
    pub(crate) async fn send_with_retry(
        &self,
        req: RequestBuilder,
    ) -> Result<Response, ZoteroApiError> {
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
    /// [`InputRejected`]: ZoteroApiError::InputRejected
    #[inline]
    pub async fn read_limited_text(
        &self,
        mut resp: Response,
        max_bytes: usize,
        context: &str,
    ) -> Result<String, ZoteroApiError> {
        let max_bytes_u64 = u64::try_from(max_bytes).unwrap_or(u64::MAX);
        if resp.content_length().is_some_and(|len| len > max_bytes_u64) {
            return Err(ZoteroApiError::InputRejected(format!(
                "{context} exceeds {max_bytes} bytes"
            )));
        }

        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            if body.len().saturating_add(chunk.len()) > max_bytes {
                return Err(ZoteroApiError::InputRejected(format!(
                    "{context} exceeds {max_bytes} bytes"
                )));
            }
            body.extend_from_slice(&chunk);
        }

        String::from_utf8(body).map_err(|_| {
            ZoteroApiError::InputRejected(format!(
                "{context} is not valid UTF-8"
            ))
        })
    }

    /// Returns a reference to the shared [`Client`] connection pool.
    #[must_use]
    #[inline]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns a reference to the [`SecurityConfig`].
    #[must_use]
    #[inline]
    pub fn security(&self) -> &SecurityConfig {
        &self.security
    }

    /// Returns the Zotero Local HTTP API base URL.
    pub(crate) fn zotero_api_url(&self) -> &str {
        &self.zotero_api_url
    }

    /// Returns the Better `BibTeX` JSON-RPC base URL.
    pub(crate) fn better_bibtex_url(&self) -> &str {
        &self.better_bibtex_url
    }

    /// Returns the Better Notes bridge base URL.
    #[must_use]
    #[inline]
    pub fn better_notes_url(&self) -> &str {
        &self.better_notes_url
    }

    /// Returns the `CrossRef` Works API base URL.
    pub(crate) fn crossref_url(&self) -> &str {
        &self.crossref_url
    }

    /// Returns the Semantic Scholar API base URL.
    pub(crate) fn semantic_scholar_url(&self) -> &str {
        &self.semantic_scholar_url
    }

    /// Returns the Open Library Books API base URL.
    pub(crate) fn open_library_url(&self) -> &str {
        &self.open_library_url
    }

    /// Returns `true` if write operations are enabled.
    #[must_use]
    #[inline]
    pub fn is_write_enabled(&self) -> bool {
        self.write_enabled
    }

    /// Returns `true` if local `SQLite` database read access is enabled.
    #[must_use]
    #[inline]
    pub fn is_sqlite_access_enabled(&self) -> bool {
        self.sqlite_access
    }

    /// Returns `true` if local semantic search features are enabled.
    #[must_use]
    #[inline]
    pub fn is_semantic_search_enabled(&self) -> bool {
        self.semantic_search_enabled
    }

    /// Returns `true` if single-purpose connector tools are enabled.
    #[must_use]
    #[inline]
    pub fn is_connector_compat_enabled(&self) -> bool {
        self.connector_compat
    }

    /// Returns the optional explicit path to `zotero.sqlite`.
    pub(crate) fn zotero_db_path(&self) -> Option<&Path> {
        self.zotero_db_path.as_deref()
    }

    /// Checks that export output `path` is allowed by security policy.
    pub(crate) fn check_export_path(
        &self,
        path: &Path,
    ) -> Result<PathBuf, ZoteroApiError> {
        if !self.security.is_file_paths_enabled() {
            return Err(ZoteroApiError::InputRejected(
                "File path features are disabled; set \
                 ZOTERO_MCP_PROFILE=workspace or \
                 ZOTERO_FILE_PATHS_ENABLED=true"
                    .to_owned(),
            ));
        }
        self.check_output_path(
            path,
            self.security.allowed_export_dirs(),
            "auto-export output",
        )
    }

    /// Checks that AUX file `path` is allowed by security policy.
    pub(crate) fn check_aux_path(
        &self,
        path: &Path,
    ) -> Result<PathBuf, ZoteroApiError> {
        self.security.check_aux_path(path)
    }
}

#[cfg(any(test, feature = "test-util"))]
impl AppState {
    #[must_use]
    #[inline]
    pub fn test_default() -> Self {
        Self {
            client: Client::new(),
            security: SecurityConfig::default(),
            zotero_api_url: String::new(),
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: String::new(),
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: false,
            sqlite_access: false,
            semantic_search_enabled: false,
            connector_compat: false,
            zotero_db_path: None,
            local_zotero_db: Self::local_zotero_db_cache(),
            semantic_db_path: None,
            semantic_index: Arc::new(OnceCell::new()),
            embedding_provider: Arc::new(OnceCell::new()),
        }
    }

    #[must_use]
    #[inline]
    pub fn with_zotero_api_url<S: Into<String>>(mut self, url: S) -> Self {
        self.zotero_api_url = url.into();
        self
    }

    #[must_use]
    #[inline]
    pub fn with_better_bibtex_url<S: Into<String>>(mut self, url: S) -> Self {
        self.better_bibtex_url = url.into();
        self
    }

    #[must_use]
    #[inline]
    pub fn with_better_notes_url<S: Into<String>>(mut self, url: S) -> Self {
        self.better_notes_url = url.into();
        self
    }

    #[must_use]
    #[inline]
    pub fn with_crossref_url<S: Into<String>>(mut self, url: S) -> Self {
        self.crossref_url = url.into();
        self
    }

    #[must_use]
    #[inline]
    pub fn with_semantic_scholar_url<S: Into<String>>(
        mut self,
        url: S,
    ) -> Self {
        self.semantic_scholar_url = url.into();
        self
    }

    #[must_use]
    #[inline]
    pub fn with_open_library_url<S: Into<String>>(mut self, url: S) -> Self {
        self.open_library_url = url.into();
        self
    }

    #[must_use]
    #[inline]
    pub fn with_write_enabled(mut self, enabled: bool) -> Self {
        self.write_enabled = enabled;
        self
    }

    #[must_use]
    #[inline]
    pub fn with_sqlite_access(mut self, enabled: bool) -> Self {
        self.sqlite_access = enabled;
        self
    }

    #[must_use]
    #[inline]
    pub fn with_connector_compat(mut self, enabled: bool) -> Self {
        self.connector_compat = enabled;
        self
    }

    #[must_use]
    #[inline]
    pub fn with_zotero_db_path(mut self, path: Option<PathBuf>) -> Self {
        self.zotero_db_path = path;
        self
    }

    #[must_use]
    #[inline]
    pub fn with_semantic_search_enabled(mut self, enabled: bool) -> Self {
        self.semantic_search_enabled = enabled;
        self
    }

    #[must_use]
    #[inline]
    pub fn with_semantic_db_path(mut self, path: Option<PathBuf>) -> Self {
        self.semantic_db_path = path;
        self
    }

    #[must_use]
    #[inline]
    pub fn with_embedding_provider(
        mut self,
        provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        self.embedding_provider = Arc::new(OnceCell::new_with(Some(provider)));
        self
    }

    #[must_use]
    #[inline]
    pub fn with_security(mut self, security: SecurityConfig) -> Self {
        self.security = security;
        self
    }

    #[inline]
    pub fn security_mut(&mut self) -> &mut SecurityConfig {
        &mut self.security
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

        #[test]
        fn verifies_app_state_getters_and_builders() {
            use std::path::PathBuf;

            use crate::security::SecurityProfile;

            let db_path = PathBuf::from("/tmp/zotero.sqlite");
            let state = AppState::test_default()
                .with_semantic_scholar_url("http://scholar.test")
                .with_open_library_url("http://library.test")
                .with_zotero_db_path(Some(db_path.clone()));

            assert_eq!(state.semantic_scholar_url(), "http://scholar.test");
            assert_eq!(state.open_library_url(), "http://library.test");
            assert_eq!(state.zotero_db_path(), Some(db_path.as_path()));
            assert_eq!(state.security().profile(), SecurityProfile::Default);
        }

        use super::AppState;
        /// Builds an [`AppState`] with empty backend URLs, for tests that
        /// only exercise `write_enabled` or `send_with_retry`.
        pub(super) fn test_state(write_enabled: bool) -> AppState {
            AppState::test_default().with_write_enabled(write_enabled)
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
    mod from_env {
        use std::env;

        use pretty_assertions::assert_eq;

        use super::*;

        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        #[test]
        fn defaults_connector_compat_to_false_when_unset() {
            let _guard = ENV_LOCK.lock().unwrap();
            let previous = env::var_os("ZOTERO_CONNECTOR_COMPAT");
            env::remove_var("ZOTERO_CONNECTOR_COMPAT");
            let state = AppState::from_env();
            assert_eq!(state.is_connector_compat_enabled(), false);
            if let Some(val) = previous {
                env::set_var("ZOTERO_CONNECTOR_COMPAT", val);
            }
        }

        #[test]
        fn parses_connector_compat_flag_when_enabled() {
            let _guard = ENV_LOCK.lock().unwrap();
            let previous = env::var_os("ZOTERO_CONNECTOR_COMPAT");
            env::set_var("ZOTERO_CONNECTOR_COMPAT", "1");
            let state = AppState::from_env();
            assert_eq!(state.is_connector_compat_enabled(), true);
            if let Some(val) = previous {
                env::set_var("ZOTERO_CONNECTOR_COMPAT", val);
            } else {
                env::remove_var("ZOTERO_CONNECTOR_COMPAT");
            }
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
            assert!(matches!(result, Err(ZoteroApiError::PermissionDenied(_))));
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
            // Arrange: fixture is disabled by default; flip the gate on.
            let state = test_state(false).with_sqlite_access(true);

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
            assert!(matches!(result, Err(ZoteroApiError::PermissionDenied(_))));
        }
    }

    mod check_html_size {
        use super::{super::*, fixtures::test_state};

        #[test]
        fn check_html_size_rejects_oversized_html() {
            let state = test_state(false);
            let html = "x".repeat(state.security().max_html_bytes() + 1);

            let result = state.check_html_size(&html);

            assert!(matches!(result, Err(ZoteroApiError::InputRejected(_))));
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
            assert!(matches!(err, ZoteroApiError::Network(_)));
        }
    }
}
