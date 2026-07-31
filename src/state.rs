//! Shared runtime state threaded through every MCP tool handler.
//!
//! [`AppState`] bundles the configured backend URLs and a shared
//! [`reqwest::Client`], plus the write-permission gate that every mutating
//! operation checks before touching the Zotero library. This module also
//! provides [`AppState::send_with_retry`], the single retry policy used by
//! all three backend clients.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::{Client, RequestBuilder, Response, StatusCode};

use crate::errors::ZoteroMcpError;

const RETRY_MAX_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(200);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(5);

const DEFAULT_MAX_PDF_BYTES: u64 = 50 * 1024 * 1024;
const DEFAULT_MAX_HTTP_BODY_BYTES: usize = 10 * 1024 * 1024;
const DEFAULT_MAX_MARKDOWN_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_MAX_TEMPLATE_NAME_BYTES: usize = 128;
const HARDENED_MAX_PDF_BYTES: u64 = 25 * 1024 * 1024;
const HARDENED_MAX_HTTP_BODY_BYTES: usize = 2 * 1024 * 1024;
const HARDENED_MAX_MARKDOWN_BYTES: usize = 512 * 1024;
const HARDENED_MAX_HTML_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SecurityProfile {
    Default,
    Workspace,
    TrustedLocal,
    Hardened,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SecurityConfig {
    pub(crate) profile: SecurityProfile,
    pub(crate) direct_file_paths: bool,
    pub(crate) file_paths_enabled: bool,
    pub(crate) allowed_read_dirs: Vec<PathBuf>,
    pub(crate) allowed_aux_dirs: Vec<PathBuf>,
    pub(crate) allowed_export_dirs: Vec<PathBuf>,
    pub(crate) max_pdf_bytes: u64,
    pub(crate) max_http_body_bytes: usize,
    pub(crate) max_markdown_bytes: usize,
    pub(crate) max_html_bytes: usize,
    pub(crate) max_template_name_bytes: usize,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            profile: SecurityProfile::Default,
            direct_file_paths: false,
            file_paths_enabled: false,
            allowed_read_dirs: Vec::new(),
            allowed_aux_dirs: Vec::new(),
            allowed_export_dirs: Vec::new(),
            max_pdf_bytes: DEFAULT_MAX_PDF_BYTES,
            max_http_body_bytes: DEFAULT_MAX_HTTP_BODY_BYTES,
            max_markdown_bytes: DEFAULT_MAX_MARKDOWN_BYTES,
            max_html_bytes: DEFAULT_MAX_HTML_BYTES,
            max_template_name_bytes: DEFAULT_MAX_TEMPLATE_NAME_BYTES,
        }
    }
}

impl SecurityConfig {
    #[expect(
        clippy::disallowed_methods,
        reason = "profile defaults intentionally use the process working \
                  directory"
    )]
    pub(crate) fn from_env() -> Self {
        let current_dir =
            env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home_dir = env::var_os("HOME").map(PathBuf::from);
        Self::from_sources(
            |name| env::var_os(name),
            &current_dir,
            home_dir.as_deref(),
        )
    }

    fn from_sources<F>(
        mut get_var: F,
        current_dir: &Path,
        home_dir: Option<&Path>,
    ) -> Self
    where
        F: FnMut(&str) -> Option<OsString>,
    {
        let profile = get_var("ZOTERO_MCP_PROFILE")
            .and_then(|v| v.into_string().ok())
            .and_then(|v| match v.as_str() {
                "workspace" => Some(SecurityProfile::Workspace),
                "trusted-local" => Some(SecurityProfile::TrustedLocal),
                "hardened" => Some(SecurityProfile::Hardened),
                "default" => Some(SecurityProfile::Default),
                _ => None,
            })
            .unwrap_or(SecurityProfile::Default);

        let mut config = match profile {
            SecurityProfile::Default => Self::default(),
            SecurityProfile::Workspace => Self {
                profile,
                direct_file_paths: true,
                file_paths_enabled: true,
                allowed_read_dirs: vec![current_dir.to_path_buf()],
                allowed_aux_dirs: vec![current_dir.to_path_buf()],
                allowed_export_dirs: vec![current_dir.join("exports")],
                ..Self::default()
            },
            SecurityProfile::TrustedLocal => {
                let mut config = Self {
                    profile,
                    direct_file_paths: true,
                    file_paths_enabled: true,
                    ..Self::default()
                };
                if let Some(home) = home_dir {
                    config.allowed_read_dirs = vec![
                        home.join("Documents"),
                        home.join("Downloads"),
                        home.join("Zotero/storage"),
                    ];
                    config.allowed_aux_dirs =
                        vec![home.join("Documents"), home.join("Downloads")];
                    config.allowed_export_dirs =
                        vec![home.join("Documents/Zotero Exports")];
                }
                config
            }
            SecurityProfile::Hardened => Self {
                profile,
                max_pdf_bytes: HARDENED_MAX_PDF_BYTES,
                max_http_body_bytes: HARDENED_MAX_HTTP_BODY_BYTES,
                max_markdown_bytes: HARDENED_MAX_MARKDOWN_BYTES,
                max_html_bytes: HARDENED_MAX_HTML_BYTES,
                ..Self::default()
            },
        };

        if let Some(value) =
            get_var("ZOTERO_DIRECT_FILE_PATHS").and_then(parse_bool)
        {
            config.direct_file_paths = value;
        }
        if let Some(value) =
            get_var("ZOTERO_FILE_PATHS_ENABLED").and_then(parse_bool)
        {
            config.file_paths_enabled = value;
        }
        if let Some(value) = get_var("ZOTERO_ALLOWED_READ_DIRS") {
            config.allowed_read_dirs = env::split_paths(&value).collect();
        }
        if let Some(value) = get_var("ZOTERO_ALLOWED_AUX_DIRS") {
            config.allowed_aux_dirs = env::split_paths(&value).collect();
        }
        if let Some(value) = get_var("ZOTERO_ALLOWED_EXPORT_DIRS") {
            config.allowed_export_dirs = env::split_paths(&value).collect();
        }
        if let Some(value) = get_var("ZOTERO_MAX_PDF_BYTES").and_then(parse_u64)
        {
            config.max_pdf_bytes = value;
        }
        if let Some(value) =
            get_var("ZOTERO_MAX_HTTP_BODY_BYTES").and_then(parse_usize)
        {
            config.max_http_body_bytes = value;
        }
        if let Some(value) =
            get_var("ZOTERO_MAX_MARKDOWN_BYTES").and_then(parse_usize)
        {
            config.max_markdown_bytes = value;
        }
        if let Some(value) =
            get_var("ZOTERO_MAX_HTML_BYTES").and_then(parse_usize)
        {
            config.max_html_bytes = value;
        }
        if let Some(value) =
            get_var("ZOTERO_MAX_TEMPLATE_NAME_BYTES").and_then(parse_usize)
        {
            config.max_template_name_bytes = value;
        }
        config
    }
}

fn parse_bool(value: OsString) -> Option<bool> {
    let value = value.into_string().ok()?;
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" => Some(true),
        "0" | "false" | "no" | "n" => Some(false),
        _ => None,
    }
}

fn parse_u64(value: OsString) -> Option<u64> {
    value.into_string().ok()?.parse().ok()
}

fn parse_usize(value: OsString) -> Option<usize> {
    value.into_string().ok()?.parse().ok()
}

/// Shared configuration and HTTP client for the Zotero, Better `BibTeX`, and
/// Better Notes backends.
///
/// Constructed once at startup via [`AppState::from_env`] and passed by
/// reference to every backend client for the lifetime of the server.
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
}

impl AppState {
    /// Builds an [`AppState`] from environment variables.
    ///
    /// Reads `ZOTERO_API_URL`, `BETTER_BIBTEX_URL`, `BETTER_NOTES_URL`,
    /// `CROSSREF_URL`, `SEMANTIC_SCHOLAR_URL`, and `OPEN_LIBRARY_URL` for the
    /// backend URLs (defaulting to standard local Zotero plugin ports or
    /// public endpoints when unset), and `ZOTERO_WRITE_ENABLED` (`"1"` or
    /// `"true"`, case-insensitive) to opt into write operations, defaulting to
    /// read-only. Returns the constructed [`AppState`].
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

    pub(crate) fn check_direct_file_paths_enabled(
        &self,
    ) -> Result<(), ZoteroMcpError> {
        if self.security.direct_file_paths {
            Ok(())
        } else {
            Err(ZoteroMcpError::InputRejected(
                "Direct file paths are disabled; set \
                 ZOTERO_MCP_PROFILE=workspace or \
                 ZOTERO_DIRECT_FILE_PATHS=true with ZOTERO_ALLOWED_READ_DIRS"
                    .to_owned(),
            ))
        }
    }

    #[expect(
        clippy::unused_self,
        reason = "method keeps all policy checks on AppState"
    )]
    #[expect(
        clippy::disallowed_methods,
        reason = "canonicalization is the security boundary for symlink-safe \
                  reads"
    )]
    pub(crate) fn check_existing_read_path(
        &self,
        path: &Path,
        roots: &[PathBuf],
        purpose: &str,
    ) -> Result<PathBuf, ZoteroMcpError> {
        let checked = std::fs::canonicalize(path)?;
        if path_is_allowed(&checked, roots) {
            Ok(checked)
        } else {
            Err(ZoteroMcpError::InputRejected(format!(
                "{purpose} path {} is outside allowed directories",
                checked.display()
            )))
        }
    }

    #[expect(
        clippy::unused_self,
        reason = "method keeps all policy checks on AppState"
    )]
    #[expect(
        clippy::disallowed_methods,
        reason = "canonicalization is the security boundary for symlink-safe \
                  outputs"
    )]
    pub(crate) fn check_output_path(
        &self,
        path: &Path,
        roots: &[PathBuf],
        purpose: &str,
    ) -> Result<PathBuf, ZoteroMcpError> {
        let Some(parent) = path.parent() else {
            return Err(ZoteroMcpError::InputRejected(format!(
                "{purpose} parent directory is missing"
            )));
        };
        let parent = std::fs::canonicalize(parent).map_err(|_| {
            ZoteroMcpError::InputRejected(format!(
                "{purpose} parent directory is missing"
            ))
        })?;
        if !path_is_allowed(&parent, roots) {
            return Err(ZoteroMcpError::InputRejected(format!(
                "{purpose} path {} is outside allowed directories",
                path.display()
            )));
        }
        let file_name = path.file_name().ok_or_else(|| {
            ZoteroMcpError::InputRejected(format!(
                "{purpose} output file name is missing"
            ))
        })?;
        Ok(parent.join(file_name))
    }

    pub(crate) fn check_pdf_file(
        &self,
        path: &Path,
    ) -> Result<(), ZoteroMcpError> {
        let is_pdf = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));
        if !is_pdf {
            return Err(ZoteroMcpError::InputRejected(format!(
                "PDF read path must have a .pdf extension: {}",
                path.display()
            )));
        }
        let len = std::fs::metadata(path)?.len();
        if len > self.security.max_pdf_bytes {
            return Err(ZoteroMcpError::InputRejected(format!(
                "PDF file {} exceeds {} bytes",
                path.display(),
                self.security.max_pdf_bytes
            )));
        }
        Ok(())
    }

    pub(crate) fn check_markdown_size(
        &self,
        markdown: &str,
    ) -> Result<(), ZoteroMcpError> {
        check_text_size(markdown, self.security.max_markdown_bytes, "markdown")
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "HTML cap is enforced in the Zotero bridge"
        )
    )]
    pub(crate) fn check_html_size(
        &self,
        html: &str,
    ) -> Result<(), ZoteroMcpError> {
        check_text_size(html, self.security.max_html_bytes, "HTML")
    }

    pub(crate) fn check_template_name_size(
        &self,
        name: &str,
    ) -> Result<(), ZoteroMcpError> {
        check_text_size(
            name,
            self.security.max_template_name_bytes,
            "template name",
        )
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

#[expect(
    clippy::disallowed_methods,
    reason = "allowed-root comparisons must use canonical paths"
)]
fn path_is_allowed(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .any(|root| path.starts_with(root))
}

fn check_text_size(
    value: &str,
    max_bytes: usize,
    field: &str,
) -> Result<(), ZoteroMcpError> {
    if value.len() > max_bytes {
        Err(ZoteroMcpError::InputRejected(format!(
            "{field} exceeds {max_bytes} bytes"
        )))
    } else {
        Ok(())
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

        use super::{AppState, SecurityConfig};

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

    mod security_config {
        use std::{ffi::OsString, path::Path};

        use pretty_assertions::assert_eq;

        use super::super::*;

        fn config_from<'a>(
            vars: &'a [(&'a str, &'a str)],
            current_dir: &Path,
            home_dir: Option<&Path>,
        ) -> SecurityConfig {
            SecurityConfig::from_sources(
                |name| {
                    vars.iter()
                        .find(|(key, _)| *key == name)
                        .map(|(_, value)| OsString::from(value))
                },
                current_dir,
                home_dir,
            )
        }

        #[test]
        fn default_profile_disables_direct_and_file_paths() {
            let current_dir = Path::new("/work/project");

            let config = config_from(&[], current_dir, None);

            assert_eq!(config.profile, SecurityProfile::Default);
            assert!(!config.direct_file_paths);
            assert!(!config.file_paths_enabled);
            assert!(config.allowed_read_dirs.is_empty());
            assert!(config.allowed_aux_dirs.is_empty());
            assert!(config.allowed_export_dirs.is_empty());
            assert_eq!(config.max_pdf_bytes, 50 * 1024 * 1024);
            assert_eq!(config.max_http_body_bytes, 10 * 1024 * 1024);
            assert_eq!(config.max_markdown_bytes, 2 * 1024 * 1024);
            assert_eq!(config.max_html_bytes, 2 * 1024 * 1024);
            assert_eq!(config.max_template_name_bytes, 128);
        }

        #[test]
        fn workspace_profile_uses_current_directory_defaults() {
            let current_dir = Path::new("/work/project");

            let config = config_from(
                &[("ZOTERO_MCP_PROFILE", "workspace")],
                current_dir,
                None,
            );

            assert_eq!(config.profile, SecurityProfile::Workspace);
            assert!(config.direct_file_paths);
            assert!(config.file_paths_enabled);
            assert_eq!(config.allowed_read_dirs, [current_dir.to_path_buf()]);
            assert_eq!(config.allowed_aux_dirs, [current_dir.to_path_buf()]);
            assert_eq!(config.allowed_export_dirs, [
                current_dir.join("exports")
            ]);
            assert_eq!(config.max_pdf_bytes, 50 * 1024 * 1024);
            assert_eq!(config.max_http_body_bytes, 10 * 1024 * 1024);
            assert_eq!(config.max_markdown_bytes, 2 * 1024 * 1024);
            assert_eq!(config.max_html_bytes, 2 * 1024 * 1024);
            assert_eq!(config.max_template_name_bytes, 128);
        }

        #[test]
        fn trusted_local_profile_uses_home_subdirectories() {
            let current_dir = Path::new("/work/project");
            let home_dir = Path::new("/home/alice");

            let config = config_from(
                &[("ZOTERO_MCP_PROFILE", "trusted-local")],
                current_dir,
                Some(home_dir),
            );

            assert_eq!(config.profile, SecurityProfile::TrustedLocal);
            assert_eq!(config.allowed_read_dirs, [
                home_dir.join("Documents"),
                home_dir.join("Downloads"),
                home_dir.join("Zotero/storage"),
            ]);
            assert_eq!(config.allowed_aux_dirs, [
                home_dir.join("Documents"),
                home_dir.join("Downloads")
            ]);
            assert_eq!(config.allowed_export_dirs, [
                home_dir.join("Documents/Zotero Exports")
            ]);
        }

        #[test]
        fn trusted_local_without_home_has_no_implicit_dirs() {
            let current_dir = Path::new("/work/project");

            let config = config_from(
                &[("ZOTERO_MCP_PROFILE", "trusted-local")],
                current_dir,
                None,
            );

            assert_eq!(config.profile, SecurityProfile::TrustedLocal);
            assert!(config.allowed_read_dirs.is_empty());
            assert!(config.allowed_aux_dirs.is_empty());
            assert!(config.allowed_export_dirs.is_empty());
        }

        #[test]
        fn hardened_profile_tightens_caps() {
            let current_dir = Path::new("/work/project");

            let config = config_from(
                &[("ZOTERO_MCP_PROFILE", "hardened")],
                current_dir,
                None,
            );

            assert_eq!(config.profile, SecurityProfile::Hardened);
            assert!(!config.direct_file_paths);
            assert!(!config.file_paths_enabled);
            assert_eq!(config.max_pdf_bytes, 25 * 1024 * 1024);
            assert_eq!(config.max_http_body_bytes, 2 * 1024 * 1024);
            assert_eq!(config.max_markdown_bytes, 512 * 1024);
            assert_eq!(config.max_html_bytes, 512 * 1024);
            assert_eq!(config.max_template_name_bytes, 128);
        }

        #[test]
        fn explicit_env_overrides_profile_defaults() {
            let current_dir = Path::new("/work/project");
            let separator = if cfg!(windows) {
                ";"
            } else {
                ":"
            };
            let read_dirs = format!("/one{separator}/two");

            let config = config_from(
                &[
                    ("ZOTERO_MCP_PROFILE", "workspace"),
                    ("ZOTERO_DIRECT_FILE_PATHS", "false"),
                    ("ZOTERO_FILE_PATHS_ENABLED", "false"),
                    ("ZOTERO_ALLOWED_READ_DIRS", &read_dirs),
                    ("ZOTERO_MAX_PDF_BYTES", "123"),
                ],
                current_dir,
                None,
            );

            assert_eq!(config.profile, SecurityProfile::Workspace);
            assert!(!config.direct_file_paths);
            assert!(!config.file_paths_enabled);
            assert_eq!(config.allowed_read_dirs, [
                Path::new("/one").to_path_buf(),
                Path::new("/two").to_path_buf()
            ]);
            assert_eq!(config.allowed_aux_dirs, [current_dir.to_path_buf()]);
            assert_eq!(config.allowed_export_dirs, [
                current_dir.join("exports")
            ]);
            assert_eq!(config.max_pdf_bytes, 123);
        }

        #[test]
        fn invalid_numeric_override_keeps_profile_default() {
            let current_dir = Path::new("/work/project");

            let config = config_from(
                &[("ZOTERO_MAX_PDF_BYTES", "not-a-number")],
                current_dir,
                None,
            );

            assert_eq!(config.max_pdf_bytes, 50 * 1024 * 1024);
        }
    }

    mod security_policy {
        use std::{fs, path::Path};

        use super::{super::*, fixtures::test_state};

        fn state_with_security(security: SecurityConfig) -> AppState {
            AppState {
                security,
                ..test_state(false)
            }
        }

        fn input_rejected_message(err: ZoteroMcpError) -> String {
            match err {
                ZoteroMcpError::InputRejected(message) => message,
                other => panic!("expected InputRejected, got {other:?}"),
            }
        }

        #[test]
        fn direct_paths_disabled_returns_input_rejected() {
            let state = state_with_security(SecurityConfig::default());

            let err = state.check_direct_file_paths_enabled().unwrap_err();

            assert!(
                input_rejected_message(err)
                    .contains("Direct file paths are disabled")
            );
        }

        #[test]
        fn existing_read_path_accepts_file_under_allowed_root() {
            let root = tempfile::TempDir::new().unwrap();
            let file = root.path().join("paper.pdf");
            fs::write(&file, b"%PDF").unwrap();
            let state = state_with_security(SecurityConfig::default());

            let checked = state
                .check_existing_read_path(
                    &file,
                    &[root.path().canonicalize().unwrap()],
                    "PDF read",
                )
                .unwrap();

            assert_eq!(checked, file.canonicalize().unwrap());
        }

        #[test]
        fn existing_read_path_rejects_file_outside_allowed_roots() {
            let allowed = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let file = outside.path().join("paper.pdf");
            fs::write(&file, b"%PDF").unwrap();
            let state = state_with_security(SecurityConfig::default());

            let err = state
                .check_existing_read_path(
                    &file,
                    &[allowed.path().canonicalize().unwrap()],
                    "PDF read",
                )
                .unwrap_err();

            let message = input_rejected_message(err);
            assert!(message.contains("PDF read"));
            assert!(message.contains("outside allowed"));
        }

        #[cfg(unix)]
        #[test]
        fn existing_read_path_rejects_symlink_escape() {
            let allowed = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let target = outside.path().join("paper.pdf");
            let link = allowed.path().join("linked.pdf");
            fs::write(&target, b"%PDF").unwrap();
            std::os::unix::fs::symlink(&target, &link).unwrap();
            let state = state_with_security(SecurityConfig::default());

            let err = state
                .check_existing_read_path(
                    &link,
                    &[allowed.path().canonicalize().unwrap()],
                    "PDF read",
                )
                .unwrap_err();

            assert!(input_rejected_message(err).contains("outside allowed"));
        }

        #[test]
        fn output_path_accepts_missing_file_under_allowed_root() {
            let root = tempfile::TempDir::new().unwrap();
            let output = root.path().join("exports.bib");
            let state = state_with_security(SecurityConfig::default());

            let checked = state
                .check_output_path(
                    &output,
                    &[root.path().canonicalize().unwrap()],
                    "auto-export output",
                )
                .unwrap();

            assert_eq!(
                checked,
                root.path().canonicalize().unwrap().join("exports.bib")
            );
        }

        #[test]
        fn output_path_rejects_missing_parent_directory() {
            let root = tempfile::TempDir::new().unwrap();
            let output = root.path().join("missing").join("exports.bib");
            let state = state_with_security(SecurityConfig::default());

            let err = state
                .check_output_path(
                    &output,
                    &[root.path().canonicalize().unwrap()],
                    "auto-export output",
                )
                .unwrap_err();

            assert!(input_rejected_message(err).contains("parent directory"));
        }

        #[test]
        fn pdf_file_rejects_non_pdf_extension_and_oversized_files() {
            let txt =
                tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
            fs::write(txt.path(), b"text").unwrap();
            let mut security = SecurityConfig::default();
            security.max_pdf_bytes = 3;
            let state = state_with_security(security);

            let extension_err = state.check_pdf_file(txt.path()).unwrap_err();
            assert!(input_rejected_message(extension_err).contains(".pdf"));

            let pdf =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            fs::write(pdf.path(), b"1234").unwrap();
            let size_err = state.check_pdf_file(pdf.path()).unwrap_err();
            assert!(input_rejected_message(size_err).contains("exceeds"));
        }

        #[test]
        fn size_helpers_reject_values_over_configured_max() {
            let mut security = SecurityConfig::default();
            security.max_markdown_bytes = 3;
            security.max_html_bytes = 3;
            security.max_template_name_bytes = 3;
            let state = state_with_security(security);

            assert!(
                input_rejected_message(
                    state.check_markdown_size("hello").unwrap_err()
                )
                .contains("markdown")
            );
            assert!(
                input_rejected_message(
                    state.check_html_size("<p>x</p>").unwrap_err()
                )
                .contains("HTML")
            );
            assert!(
                input_rejected_message(
                    state.check_template_name_size("Export").unwrap_err()
                )
                .contains("template name")
            );
        }

        #[test]
        fn existing_read_path_rejects_empty_allowed_roots() {
            let file =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            fs::write(file.path(), b"%PDF").unwrap();
            let state = state_with_security(SecurityConfig::default());

            let err = state
                .check_existing_read_path(
                    Path::new(file.path()),
                    &[],
                    "PDF read",
                )
                .unwrap_err();

            assert!(input_rejected_message(err).contains("PDF read"));
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
