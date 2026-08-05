//! Security profiles, path allowlists, and input size constraints.
//!
//! Main types:
//! - [`SecurityProfile`] - Supported security profiles (Default, Workspace,
//!   `TrustedLocal`, `Hardened`)
//! - [`SecurityConfig`] - Path allowlists and size limits
//!
//! # Examples
//!
//! ```no_run
//! use zotero_mcp_rs::security::{SecurityConfig, SecurityProfile};
//!
//! let config = SecurityConfig::default();
//! assert_eq!(config.profile, SecurityProfile::Default);
//! ```

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::errors::ZoteroMcpError;

const DEFAULT_MAX_PDF_BYTES: u64 = 50 * 1024 * 1024;
const DEFAULT_MAX_HTTP_BODY_BYTES: usize = 10 * 1024 * 1024;
const DEFAULT_MAX_MARKDOWN_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_MAX_TEMPLATE_NAME_BYTES: usize = 128;
const HARDENED_MAX_PDF_BYTES: u64 = 25 * 1024 * 1024;
const HARDENED_MAX_HTTP_BODY_BYTES: usize = 2 * 1024 * 1024;
const HARDENED_MAX_MARKDOWN_BYTES: usize = 512 * 1024;
const HARDENED_MAX_HTML_BYTES: usize = 512 * 1024;

/// Security profiles supported by the Zotero MCP server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SecurityProfile {
    /// Default profile: conservative read-only access.
    Default,
    /// Workspace profile: allows reading and exports relative to current
    /// working directory.
    Workspace,
    /// Trusted local profile: allows reading from standard user
    /// document/download paths.
    TrustedLocal,
    /// Hardened profile: restricts maximum request/response sizes.
    Hardened,
}

/// Security configuration parameters controlling path access and size limits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SecurityConfig {
    /// Active security profile.
    pub(crate) profile: SecurityProfile,
    /// Whether direct file paths are enabled.
    pub(crate) direct_file_paths: bool,
    /// Whether file path checking is enabled.
    pub(crate) file_paths_enabled: bool,
    /// Allowed directories for reading files.
    pub(crate) allowed_read_dirs: Vec<PathBuf>,
    /// Allowed directories for auxiliary tools.
    pub(crate) allowed_aux_dirs: Vec<PathBuf>,
    /// Allowed directories for export files.
    pub(crate) allowed_export_dirs: Vec<PathBuf>,
    /// Maximum allowed PDF size in bytes.
    pub(crate) max_pdf_bytes: u64,
    /// Maximum allowed HTTP response body size in bytes.
    pub(crate) max_http_body_bytes: usize,
    /// Maximum allowed Markdown size in bytes.
    pub(crate) max_markdown_bytes: usize,
    /// Maximum allowed HTML size in bytes.
    pub(crate) max_html_bytes: usize,
    /// Maximum allowed template name size in bytes.
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
    /// Reads security configuration from environment variables.
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

    /// Checks if direct filepath access is enabled by policy.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if direct filepath access is disabled
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) fn check_direct_file_paths_enabled(
        &self,
    ) -> Result<(), ZoteroMcpError> {
        if self.direct_file_paths {
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

    /// Validates that a path exists and falls under one of the allowed
    /// `roots`.
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
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    /// [`Io`]: ZoteroMcpError::Io
    #[expect(
        clippy::unused_self,
        reason = "method keeps policy checks on SecurityConfig"
    )]
    #[expect(
        clippy::disallowed_methods,
        reason = "canonicalization is the security boundary for symlink-safe \
                  reads"
    )]
    pub(crate) fn check_existing_read_path<'a, I>(
        &self,
        path: &Path,
        roots: I,
        purpose: &str,
    ) -> Result<PathBuf, ZoteroMcpError>
    where
        I: IntoIterator<Item = &'a PathBuf>,
    {
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
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    #[expect(
        clippy::unused_self,
        reason = "method keeps policy checks on SecurityConfig"
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

    /// Checks that `path` points to a `.pdf` file within maximum allowed byte
    /// limits.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if `path` lacks a `.pdf` extension or exceeds
    ///   `max_pdf_bytes`
    /// - [`Io`] if file metadata retrieval fails
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    /// [`Io`]: ZoteroMcpError::Io
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
        if len > self.max_pdf_bytes {
            return Err(ZoteroMcpError::InputRejected(format!(
                "PDF file {} exceeds {} bytes",
                path.display(),
                self.max_pdf_bytes
            )));
        }
        Ok(())
    }

    /// Validates that `markdown` content does not exceed `max_markdown_bytes`.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds configured maximum limit
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) fn check_markdown_size(
        &self,
        markdown: &str,
    ) -> Result<(), ZoteroMcpError> {
        check_text_size(markdown, self.max_markdown_bytes, "markdown")
    }

    /// Validates that `html` content does not exceed `max_html_bytes`.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds configured maximum limit
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    #[allow(dead_code, reason = "HTML cap is enforced in the Zotero bridge")]
    pub(crate) fn check_html_size(
        &self,
        html: &str,
    ) -> Result<(), ZoteroMcpError> {
        check_text_size(html, self.max_html_bytes, "HTML")
    }

    /// Validates that template `name` does not exceed
    /// `max_template_name_bytes`.
    ///
    /// # Errors
    ///
    /// - [`InputRejected`] if size exceeds configured maximum limit
    ///
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    pub(crate) fn check_template_name_size(
        &self,
        name: &str,
    ) -> Result<(), ZoteroMcpError> {
        check_text_size(name, self.max_template_name_bytes, "template name")
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

#[expect(
    clippy::disallowed_methods,
    reason = "allowed-root comparisons must use canonical paths"
)]
fn path_is_allowed<'a, I>(path: &Path, roots: I) -> bool
where
    I: IntoIterator<Item = &'a PathBuf>,
{
    roots
        .into_iter()
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

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, path::Path};

    use pretty_assertions::assert_eq;

    use super::*;

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
        assert_eq!(config.allowed_export_dirs, [current_dir.join("exports")]);
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
        assert_eq!(config.allowed_export_dirs, [current_dir.join("exports")]);
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

    fn input_rejected_message(err: ZoteroMcpError) -> String {
        match err {
            ZoteroMcpError::InputRejected(message) => message,
            other => format!("expected InputRejected, got {other:?}"),
        }
    }

    #[test]
    fn direct_paths_disabled_returns_input_rejected() {
        let config = SecurityConfig::default();

        let err = config.check_direct_file_paths_enabled().unwrap_err();

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
        let config = SecurityConfig::default();

        let checked = config
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
        let config = SecurityConfig::default();

        let err = config
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
        let config = SecurityConfig::default();

        let err = config
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
        let config = SecurityConfig::default();

        let checked = config
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
        let config = SecurityConfig::default();

        let err = config
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
        let txt = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        fs::write(txt.path(), b"text").unwrap();
        let config = SecurityConfig {
            max_pdf_bytes: 3,
            ..SecurityConfig::default()
        };

        let extension_err = config.check_pdf_file(txt.path()).unwrap_err();
        assert!(input_rejected_message(extension_err).contains(".pdf"));

        let pdf = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        fs::write(pdf.path(), b"1234").unwrap();
        let size_err = config.check_pdf_file(pdf.path()).unwrap_err();
        assert!(input_rejected_message(size_err).contains("exceeds"));
    }

    #[test]
    fn size_helpers_reject_values_over_configured_max() {
        let config = SecurityConfig {
            max_markdown_bytes: 3,
            max_html_bytes: 3,
            max_template_name_bytes: 3,
            ..SecurityConfig::default()
        };

        assert!(
            input_rejected_message(
                config.check_markdown_size("hello").unwrap_err()
            )
            .contains("markdown")
        );
        assert!(
            input_rejected_message(
                config.check_html_size("<p>x</p>").unwrap_err()
            )
            .contains("HTML")
        );
        assert!(
            input_rejected_message(
                config.check_template_name_size("Export").unwrap_err()
            )
            .contains("template name")
        );
    }

    #[test]
    fn existing_read_path_rejects_empty_allowed_roots() {
        let file = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        fs::write(file.path(), b"%PDF").unwrap();
        let config = SecurityConfig::default();

        let err = config
            .check_existing_read_path(Path::new(file.path()), &[], "PDF read")
            .unwrap_err();

        assert!(input_rejected_message(err).contains("PDF read"));
    }
}
