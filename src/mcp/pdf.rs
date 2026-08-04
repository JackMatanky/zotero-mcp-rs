//! PDF path resolution and security policy enforcement for Zotero attachments.
//!
//! This module provides path resolution logic for both Zotero-managed
//! (`imported_file`) and linked (`linked_file`) PDF attachments. It queries
//! companion bridge endpoints to discover valid Zotero storage directories and
//! validates target paths against security configuration limits.
//!
//! Main types:
//! - [`ResolvedPdfPath`] - Resolved filesystem path for a Zotero PDF attachment
//! - [`BridgePdfRoot`] - Bridge file-roots response for Zotero storage
//!   validation

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    errors::ZoteroMcpError,
    zotero::{ItemType, LinkMode, ZoteroItem},
};

const ZOTERO_ATTACHMENTS_PREFIX: &str = "attachments:";
const BRIDGE_FILE_ROOTS_PATH: &str = "/file-roots";

/// Resolved filesystem path for a Zotero PDF attachment item.
pub(super) enum ResolvedPdfPath {
    /// Imported attachment path already trusted by Zotero's enclosure link.
    Trusted(PathBuf),
    /// Linked-file path that must be checked against allowed roots.
    NeedsRootCheck(PathBuf),
}

impl ResolvedPdfPath {
    /// Consumes the wrapper and returns the underlying [`PathBuf`].
    pub(super) fn into_path(self) -> PathBuf {
        match self {
            Self::Trusted(path) | Self::NeedsRootCheck(path) => path,
        }
    }
}

/// Searches child items of a Zotero item for the first valid PDF attachment
/// path.
pub(super) fn find_pdf_path(
    children: &[ZoteroItem],
    bridge_roots: &[BridgePdfRoot],
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
    bridge_roots: &[BridgePdfRoot],
) -> Option<ResolvedPdfPath> {
    if item.data.item_type != ItemType::Attachment {
        return None;
    }

    if matches!(item.data.link_mode.as_ref(), Some(LinkMode::ImportedFile)) {
        if let Some(path) = enclosure_file_path(item) {
            return Some(ResolvedPdfPath::Trusted(path));
        }
    }

    if matches!(item.data.link_mode.as_ref(), Some(LinkMode::LinkedFile)) {
        if let Some(path) =
            item.data.path.as_deref().and_then(|path| {
                resolve_linked_attachment_path(path, bridge_roots)
            })
        {
            return Some(ResolvedPdfPath::NeedsRootCheck(path));
        }
    }

    if item.data.content_type.as_deref() == Some("application/pdf") {
        if let Some(path) =
            item.data.path.as_deref().and_then(|path| {
                resolve_linked_attachment_path(path, bridge_roots)
            })
        {
            return Some(ResolvedPdfPath::NeedsRootCheck(path));
        }
    }

    None
}

/// Resolves `raw_path` relative to bridge-reported linked base roots if
/// prefixed with `attachments:`.
fn resolve_linked_attachment_path(
    raw_path: &str,
    bridge_roots: &[BridgePdfRoot],
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
/// [`FileRootKind::ZoteroLinkedBase`] category.
fn linked_base_roots(
    bridge_roots: &[BridgePdfRoot],
) -> impl Iterator<Item = &PathBuf> {
    bridge_roots
        .iter()
        .filter(|root| root.kind == FileRootKind::ZoteroLinkedBase)
        .map(|root| &root.path)
}

impl ZoteroMcpServer {
    /// Fetches allowed Zotero PDF storage and linked file root directories from
    /// the bridge script.
    ///
    /// Queries the `/file-roots` bridge endpoint for reported storage
    /// directories, linked file base directories, and plugin destination roots
    /// (such as Attanger). Returns an empty [`Vec`] if the bridge is
    /// unreachable or returns invalid JSON.
    pub(super) async fn fetch_bridge_pdf_roots(&self) -> Vec<BridgePdfRoot> {
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
            .map(|root| BridgePdfRoot {
                kind: root.kind,
                path: PathBuf::from(root.path),
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
        bridge_roots: &[BridgePdfRoot],
        direct_input: bool,
    ) -> Result<PathBuf, ZoteroMcpError> {
        let bridge_paths = bridge_roots.iter().map(|root| &root.path);
        let roots =
            self.state.security.allowed_read_dirs.iter().chain(bridge_paths);
        match self.state.check_existing_read_path(path, roots, "PDF read") {
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

/// Single typed file root reported by the bridge and accepted for PDF reads.
pub(super) struct BridgePdfRoot {
    /// Root category reported by the bridge.
    kind: FileRootKind,
    /// Canonical or configured root path.
    path: PathBuf,
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

/// Single file root reported by the bridge.
#[derive(Debug, Deserialize)]
struct BridgeFileRoot {
    /// Root category (e.g., [`FileRootKind::ZoteroStorage`]).
    kind: FileRootKind,
    /// Filesystem path to the root directory.
    path: String,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::zotero::ZoteroItem;

    mod path_resolution {
        use pretty_assertions::assert_eq;

        use super::*;
        #[test]
        fn file_url_to_path_parses_valid_file_url() {
            // Arrange
            let href = "file:///tmp/document.pdf";

            // Act
            let path = file_url_to_path(href);

            // Assert
            assert_eq!(path, Some(PathBuf::from("/tmp/document.pdf")));
        }

        #[test]
        fn file_url_to_path_returns_none_for_non_file_url() {
            // Arrange
            let href = "https://example.com/document.pdf";

            // Act
            let path = file_url_to_path(href);

            // Assert
            assert_eq!(path, None);
        }

        #[test]
        fn resolve_linked_attachment_path_resolves_attachment_prefix() {
            // Arrange
            let raw_path = "attachments:subfolder/paper.pdf";
            let base_dir = PathBuf::from("/zotero/base");
            let bridge_roots = vec![BridgePdfRoot {
                kind: FileRootKind::ZoteroLinkedBase,
                path: base_dir.clone(),
            }];

            // Act
            let resolved =
                resolve_linked_attachment_path(raw_path, &bridge_roots);

            // Assert
            assert_eq!(resolved, Some(base_dir.join("subfolder/paper.pdf")));
        }

        #[test]
        fn resolve_linked_attachment_path_returns_raw_path_when_unprefixed() {
            // Arrange
            let raw_path = "subfolder/paper.pdf";
            let bridge_roots = vec![BridgePdfRoot {
                kind: FileRootKind::ZoteroLinkedBase,
                path: PathBuf::from("/zotero/base"),
            }];

            // Act
            let resolved =
                resolve_linked_attachment_path(raw_path, &bridge_roots);

            // Assert
            assert_eq!(resolved, Some(PathBuf::from("subfolder/paper.pdf")));
        }

        #[test]
        fn enclosure_file_path_extracts_path_from_imported_attachment() {
            // Arrange
            let item: ZoteroItem = serde_json::from_value(json!({
                "key": "PDF01",
                "version": 1,
                "links": {
                    "enclosure": {
                        "href": "file:///storage/PDF01/paper.pdf",
                        "type": "application/pdf",
                        "title": "paper.pdf"
                    }
                },
                "data": {
                    "key": "PDF01",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "imported_file",
                    "contentType": "application/pdf",
                    "filename": "paper.pdf"
                }
            }))
            .unwrap();

            // Act
            let path = enclosure_file_path(&item);

            // Assert
            assert_eq!(path, Some(PathBuf::from("/storage/PDF01/paper.pdf")));
        }

        #[test]
        fn resolve_attachment_pdf_path_returns_none_for_non_attachment_item() {
            // Arrange
            let item: ZoteroItem = serde_json::from_value(json!({
                "key": "ITEM01",
                "version": 1,
                "data": {
                    "key": "ITEM01",
                    "version": 1,
                    "itemType": "journalArticle"
                }
            }))
            .unwrap();

            // Act
            let resolved = resolve_attachment_pdf_path(&item, &[]);

            // Assert
            assert!(resolved.is_none());
        }

        #[test]
        fn find_pdf_path_returns_first_valid_attachment() {
            // Arrange
            let children: Vec<ZoteroItem> = serde_json::from_value(json!([
                {
                    "key": "NOTE01",
                    "version": 1,
                    "data": {
                        "key": "NOTE01",
                        "version": 1,
                        "itemType": "note"
                    }
                },
                {
                    "key": "PDF01",
                    "version": 1,
                    "links": {
                        "enclosure": {
                            "href": "file:///tmp/paper.pdf",
                            "type": "application/pdf"
                        }
                    },
                    "data": {
                        "key": "PDF01",
                        "version": 1,
                        "itemType": "attachment",
                        "linkMode": "imported_file",
                        "contentType": "application/pdf",
                        "filename": "paper.pdf"
                    }
                }
            ]))
            .unwrap();

            // Act
            let resolved = find_pdf_path(&children, &[]);

            // Assert
            assert!(matches!(
                resolved,
                Some(ResolvedPdfPath::Trusted(path))
                    if path == std::path::Path::new("/tmp/paper.pdf")
            ));
        }
    }

    mod roots_and_security {
        use pretty_assertions::assert_eq;

        use super::*;
        #[test]
        fn file_root_kind_deserializes_kebab_case_and_identifies_pdf_roots() {
            // Arrange & Act
            let storage: FileRootKind =
                serde_json::from_str("\"zotero-storage\"").unwrap();
            let linked: FileRootKind =
                serde_json::from_str("\"zotero-linked-base\"").unwrap();
            let attanger: FileRootKind =
                serde_json::from_str("\"attanger-dest\"").unwrap();
            let other: FileRootKind =
                serde_json::from_str("\"unrecognized-root\"").unwrap();

            // Assert
            assert_eq!(storage, FileRootKind::ZoteroStorage);
            assert_eq!(linked, FileRootKind::ZoteroLinkedBase);
            assert_eq!(attanger, FileRootKind::AttangerDest);
            assert_eq!(other, FileRootKind::Other);

            assert_ne!(storage, FileRootKind::Other);
        }

        #[test]
        fn bridge_pdf_roots_keep_root_kind_typed() {
            // Arrange
            let roots = vec![
                BridgePdfRoot {
                    kind: FileRootKind::ZoteroStorage,
                    path: PathBuf::from("/bridge/storage"),
                },
                BridgePdfRoot {
                    kind: FileRootKind::Other,
                    path: PathBuf::from("/bridge/ignored"),
                },
            ];

            // Act
            let linked_roots = linked_base_roots(&roots).collect::<Vec<_>>();

            // Assert
            assert!(linked_roots.is_empty());
        }
    }
}
