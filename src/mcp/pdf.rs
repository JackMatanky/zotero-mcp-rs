//! PDF path resolution and security policy enforcement for Zotero attachments.
//!
//! This module provides path resolution logic for both Zotero-managed
//! (`imported_file`) and linked (`linked_file`) PDF attachments. It queries
//! companion bridge endpoints to discover valid Zotero storage directories and
//! validates target paths against security configuration limits.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    errors::ZoteroMcpError,
    zotero::{ItemType, ZoteroItem},
};

const ZOTERO_ATTACHMENTS_PREFIX: &str = "attachments:";
const BRIDGE_FILE_ROOTS_PATH: &str = "/file-roots";

/// Resolved filesystem path for a Zotero PDF attachment item.
pub(super) struct ResolvedPdfPath {
    /// Resolved path to the target PDF file.
    pub(super) path: PathBuf,
    /// Whether security root enforcement check is required for this path.
    pub(super) requires_root_check: bool,
}

/// Searches child items of a Zotero item for the first valid PDF attachment
/// path.
pub(super) fn find_pdf_path(
    children: &[ZoteroItem],
    bridge_roots: &[(String, PathBuf)],
) -> Option<ResolvedPdfPath> {
    children
        .iter()
        .find_map(|child| resolve_attachment_pdf_path(child, bridge_roots))
}

/// Resolves a Zotero attachment `item` to a [`ResolvedPdfPath`].
///
/// Handles both `imported_file` and `linked_file` attachment modes using
/// enclosure links and bridge roots. Returns [`None`] if the item is not a PDF
/// attachment.
pub(super) fn resolve_attachment_pdf_path(
    item: &ZoteroItem,
    bridge_roots: &[(String, PathBuf)],
) -> Option<ResolvedPdfPath> {
    if item.data.item_type != ItemType::Attachment {
        return None;
    }

    if item.data.link_mode.as_deref() == Some("imported_file") {
        if let Some(path) = enclosure_file_path(item) {
            return Some(ResolvedPdfPath {
                path,
                requires_root_check: false,
            });
        }
    }

    if item.data.link_mode.as_deref() == Some("linked_file") {
        if let Some(path) =
            item.data.path.as_deref().and_then(|path| {
                resolve_linked_attachment_path(path, bridge_roots)
            })
        {
            return Some(ResolvedPdfPath {
                path,
                requires_root_check: true,
            });
        }
    }

    if item.data.content_type.as_deref() == Some("application/pdf") {
        if let Some(path) =
            item.data.path.as_deref().and_then(|path| {
                resolve_linked_attachment_path(path, bridge_roots)
            })
        {
            return Some(ResolvedPdfPath {
                path,
                requires_root_check: true,
            });
        }
    }

    None
}

/// Resolves `raw_path` relative to bridge-reported linked base roots if
/// prefixed with `attachments:`.
fn resolve_linked_attachment_path(
    raw_path: &str,
    bridge_roots: &[(String, PathBuf)],
) -> Option<PathBuf> {
    let Some(relative) = raw_path.strip_prefix(ZOTERO_ATTACHMENTS_PREFIX)
    else {
        return Some(PathBuf::from(raw_path));
    };
    let relative = relative.trim_start_matches(['/', '\\']);
    linked_base_roots(bridge_roots).next().map(|root| root.join(relative))
}

/// Extracts the local file path from an imported attachment `item`'s enclosure
/// link.
fn enclosure_file_path(item: &ZoteroItem) -> Option<PathBuf> {
    let href = item.links.get("enclosure")?.get("href")?.as_str()?;
    file_url_to_path(href)
}

/// Converts a `file://` scheme URL `href` into a local [`PathBuf`].
///
/// Returns [`None`] if `href` is not a valid `file://` URL.
fn file_url_to_path(href: &str) -> Option<PathBuf> {
    let url = url::Url::parse(href).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path().ok()
}

/// Returns an iterator over bridge root paths belonging to the
/// `"zotero-linked-base"` category.
fn linked_base_roots(
    bridge_roots: &[(String, PathBuf)],
) -> impl Iterator<Item = &PathBuf> {
    bridge_roots
        .iter()
        .filter(|(kind, _)| kind == "zotero-linked-base")
        .map(|(_, path)| path)
}

impl ZoteroMcpServer {
    /// Fetches allowed Zotero PDF storage and linked file root directories from
    /// the bridge script.
    ///
    /// Queries the `/file-roots` bridge endpoint for reported storage
    /// directories, linked file base directories, and plugin destination roots
    /// (such as Attanger). Returns an empty [`Vec`] if the bridge is
    /// unreachable or returns invalid JSON.
    pub(super) async fn fetch_bridge_pdf_roots(
        &self,
    ) -> Vec<(String, PathBuf)> {
        let url = format!(
            "{}{}",
            self.state.better_notes_url.trim_end_matches('/'),
            BRIDGE_FILE_ROOTS_PATH
        );
        let resp = match self
            .state
            .client
            .post(url)
            .json(&serde_json::json!({}))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp,
            Ok(_) | Err(_) => return Vec::new(),
        };
        let Ok(body) = self
            .state
            .read_limited_text(
                resp,
                self.state.security.max_http_body_bytes,
                "file roots response",
            )
            .await
        else {
            return Vec::new();
        };
        let Ok(parsed) = serde_json::from_str::<BridgeFileRootsResponse>(&body)
        else {
            return Vec::new();
        };
        parsed
            .roots
            .into_iter()
            .filter(|root| {
                !matches!(root.kind, FileRootKind::Other)
                    && !root.path.is_empty()
            })
            .map(|root| {
                (root.kind.as_str().to_owned(), PathBuf::from(root.path))
            })
            .collect()
    }

    /// Validates that `path` is an existing PDF file allowed by configured
    /// security policies.
    ///
    /// Checks `path` against both user-configured allowed directories and
    /// reported `bridge_roots`. If `direct_input` is `true` and `path` is not
    /// under bridge roots, validates that direct file path access is explicitly
    /// enabled in security configuration.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::InputRejected`] if path access is disallowed, direct
    ///   paths are disabled, or the file is not a valid PDF / exceeds byte
    ///   limits
    pub(super) fn validate_pdf_read_path(
        &self,
        path: &Path,
        bridge_roots: &[(String, PathBuf)],
        direct_input: bool,
    ) -> Result<PathBuf, ZoteroMcpError> {
        let roots = merge_pdf_roots(
            &self.state.security.allowed_read_dirs,
            bridge_roots,
        );
        match self.state.check_existing_read_path(path, &roots, "PDF read") {
            Ok(checked) => {
                self.state.check_pdf_file(&checked)?;
                Ok(checked)
            }
            Err(_) if direct_input => {
                self.state.check_direct_file_paths_enabled()?;
                let checked = self.state.check_existing_read_path(
                    path,
                    &self.state.security.allowed_read_dirs,
                    "PDF read",
                )?;
                self.state.check_pdf_file(&checked)?;
                Ok(checked)
            }
            Err(e) => Err(e),
        }
    }
}

/// Canonicalizes an existing filesystem `path`.
///
/// # Errors
///
/// - [`ZoteroMcpError::Io`] if `path` does not exist or canonicalization fails
#[expect(
    clippy::disallowed_methods,
    reason = "canonicalization is the security boundary for imported Zotero \
              PDFs"
)]
pub(super) fn canonicalize_existing_path(
    path: &Path,
) -> Result<PathBuf, ZoteroMcpError> {
    Ok(std::fs::canonicalize(path)?)
}

/// Combines user-configured allowed directories with bridge-reported file roots
/// into a single [`Vec`].
fn merge_pdf_roots(
    configured: &[PathBuf],
    bridge_roots: &[(String, PathBuf)],
) -> Vec<PathBuf> {
    configured
        .iter()
        .cloned()
        .chain(bridge_roots.iter().map(|(_, path)| path.clone()))
        .collect()
}

/// Response payload returned by the bridge `/file-roots` endpoint.
#[derive(Debug, Deserialize)]
struct BridgeFileRootsResponse {
    /// List of file roots served by the bridge.
    #[serde(default)]
    roots: Vec<BridgeFileRoot>,
}

/// Category of file root directory reported by the Zotero companion bridge.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum FileRootKind {
    /// Managed Zotero storage directory.
    ZoteroStorage,
    /// Zotero linked file base directory.
    ZoteroLinkedBase,
    /// Destination directory configured in Attanger.
    AttangerDest,
    /// Any other root category not relevant to PDF path resolution.
    #[serde(other)]
    Other,
}

impl FileRootKind {
    /// Borrows the string identifier corresponding to this root category.
    fn as_str(self) -> &'static str {
        match self {
            Self::ZoteroStorage => "zotero-storage",
            Self::ZoteroLinkedBase => "zotero-linked-base",
            Self::AttangerDest => "attanger-dest",
            Self::Other => "other",
        }
    }
}

/// Single file root reported by the bridge.
#[derive(Debug, Deserialize)]
struct BridgeFileRoot {
    /// Root category (e.g., [`FileRootKind::ZoteroStorage`]).
    kind: FileRootKind,
    /// Filesystem path to the root directory.
    path: String,
}
