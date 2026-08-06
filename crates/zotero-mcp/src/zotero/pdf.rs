//! MCP tool handlers, security policies, and argument models for Zotero PDF
//! attachment access.
//!
//! Handles the `zotero_pdf` grouped-router tool calls for finding PDF file
//! paths, extracting text from page ranges, and retrieving PDF outlines (table
//! of contents). Provides path resolution logic for both Zotero-managed
//! (`imported_file`) and linked (`linked_file`) PDF attachments, querying
//! companion bridge endpoints to discover valid Zotero storage directories and
//! validating target paths against security configuration limits.
//!
//! # Main Types
//!
//! - [`ZoteroPdfCommand`]: Grouped-router command for PDF actions (`Path`,
//!   `ReadPages`, `Outline`).
//! - [`GetPdfPathArgs`]: Arguments for discovering the local file path of a PDF
//!   attachment.
//! - [`ReadPdfPagesArgs`]: Arguments for extracting text from specific PDF
//!   pages.
//! - [`GetPdfOutlineArgs`]: Arguments for extracting the table of contents /
//!   outline of a PDF.
//! - [`ResolvedPdfPath`]: Resolved filesystem path for a Zotero PDF attachment.
//! - [`BridgePdfRoot`]: Bridge file-roots response for Zotero storage
//!   validation.

use std::path::{Path, PathBuf};

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::{
    ItemKey, ItemType, LinkMode, ZoteroApiError, ZoteroClient, ZoteroItem,
    pdf::{extract_pdf_outline, extract_pdf_pages},
};

use crate::{
    ZoteroMcpServer,
    response::{json_result, text_error, text_success},
};

const ZOTERO_ATTACHMENTS_PREFIX: &str = "attachments:";
const BRIDGE_FILE_ROOTS_PATH: &str = "/file-roots";

/// Resolved filesystem path for a Zotero PDF attachment item.
pub(crate) enum ResolvedPdfPath {
    /// Imported attachment path already trusted by Zotero's enclosure link.
    Trusted(PathBuf),
    /// Linked-file path that must be checked against allowed roots.
    NeedsRootCheck(PathBuf),
}

impl ResolvedPdfPath {
    /// Consumes the wrapper and returns the underlying [`PathBuf`].
    pub(crate) fn into_path(self) -> PathBuf {
        match self {
            Self::Trusted(path) | Self::NeedsRootCheck(path) => path,
        }
    }
}

/// Searches child items of a Zotero item for the first valid PDF attachment
/// path.
pub(crate) fn find_pdf_path(
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
pub(crate) fn resolve_attachment_pdf_path(
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

/// Extracts the local filepath from an imported attachment `item`'s enclosure
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

/// Single typed file root reported by the bridge and accepted for PDF reads.
pub(crate) struct BridgePdfRoot {
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

/// Canonicalizes an existing filesystem `path`.
///
/// # Errors
///
/// - [`ZoteroApiError::Io`] if `path` does not exist or canonicalization fails
#[expect(
    clippy::disallowed_methods,
    reason = "canonicalization is the security boundary for imported Zotero \
              PDFs"
)]
pub(crate) fn canonicalize_existing_path(
    path: &Path,
) -> Result<PathBuf, ZoteroApiError> {
    Ok(std::fs::canonicalize(path)?)
}

impl ZoteroMcpServer {
    /// Fetches allowed Zotero PDF storage and linked file root directories from
    /// the bridge script.
    ///
    /// Queries the `/file-roots` bridge endpoint for reported storage
    /// directories, linked file base directories, and plugin destination roots
    /// (such as Attanger). Returns an empty [`Vec`] if the bridge is
    /// unreachable or returns invalid JSON.
    pub(in crate::zotero) async fn fetch_bridge_pdf_roots(
        &self,
    ) -> Vec<BridgePdfRoot> {
        let url = format!(
            "{}{}",
            self.state.better_notes_url().trim_end_matches('/'),
            BRIDGE_FILE_ROOTS_PATH
        );
        let resp = match self
            .state
            .client()
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
                self.state.security().max_http_body_bytes(),
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
    /// under bridge roots, validates that direct filepath access is explicitly
    /// enabled in security configuration.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path to validate.
    /// * `bridge_roots` - Bridge file roots fetched from
    ///   [`ZoteroMcpServer::fetch_bridge_pdf_roots`].
    /// * `direct_input` - Whether `path` comes directly from user input rather
    ///   than a Zotero item lookup.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::InputRejected`]: If path access is disallowed,
    ///   direct paths are disabled, or the file is not a valid PDF or exceeds
    ///   byte limits.
    pub(in crate::zotero) fn validate_pdf_read_path(
        &self,
        path: &Path,
        bridge_roots: &[BridgePdfRoot],
        direct_input: bool,
    ) -> Result<PathBuf, ZoteroApiError> {
        let bridge_paths = bridge_roots.iter().map(|root| &root.path);
        let roots = self
            .state
            .security()
            .allowed_read_dirs()
            .iter()
            .chain(bridge_paths);
        match self.state.check_existing_read_path(path, roots, "PDF read") {
            Ok(checked) => {
                self.state.check_pdf_file(&checked)?;
                Ok(checked)
            }
            Err(_) if direct_input => {
                self.state.check_direct_file_paths_enabled()?;
                let checked = self.state.check_existing_read_path(
                    path,
                    self.state.security().allowed_read_dirs(),
                    "PDF read",
                )?;
                self.state.check_pdf_file(&checked)?;
                Ok(checked)
            }
            Err(e) => Err(e),
        }
    }
}

/// Arguments for the `path` action of `zotero_pdf`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfPathArgs {
    /// Zotero item key ([`ItemKey`]) for parent item or attachment item.
    item_key: String,
}

/// Arguments for the `read_pages` action of `zotero_pdf`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ReadPdfPagesArgs {
    /// Zotero item key; direct PDF paths must resolve under configured or
    /// Zotero-reported PDF roots, otherwise direct-path opt-in is required.
    item_key_or_path: String,
    /// 1-based page numbers to extract (e.g. `[1, 2, 3]`).
    pages: Option<Vec<usize>>,
}

/// Arguments for the `outline` action of `zotero_pdf`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfOutlineArgs {
    /// Zotero item key; direct PDF paths must resolve under configured or
    /// Zotero-reported PDF roots, otherwise direct-path opt-in is required.
    item_key_or_path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// PDF commands dispatched by the `zotero_pdf` MCP tool router.
pub(crate) enum ZoteroPdfCommand {
    /// Get the local file path for a PDF attachment.
    Path(GetPdfPathArgs),
    /// Extract text from specific PDF pages.
    ReadPages(ReadPdfPagesArgs),
    /// Get the table of contents / outline of a PDF.
    Outline(GetPdfOutlineArgs),
}

#[tool_router(router = pdf_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_pdf",
        description = "Grouped Zotero PDF router. action: path, read_pages, \
                       outline",
        annotations(
            title = "Read Zotero PDFs",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// Dispatches PDF tool commands to internal handlers.
    ///
    /// Receives parsed `args` wrapped in [`Parameters`], routing `path`,
    /// `read_pages`, or `outline` actions.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_pdf(
        &self,
        Parameters(args): Parameters<ZoteroPdfCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroPdfCommand::Path(args) => {
                self.zotero_get_pdf_path_impl(args).await
            }
            ZoteroPdfCommand::ReadPages(args) => {
                self.zotero_read_pdf_pages_impl(args).await
            }
            ZoteroPdfCommand::Outline(args) => {
                self.zotero_get_pdf_outline_impl(args).await
            }
        }
    }
}

impl ZoteroMcpServer {
    /// Handles Zotero PDF path discovery tool calls via
    /// [`ZoteroClient::get_item`] and [`ZoteroClient::get_item_children`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_pdf_path_impl(
        &self,
        args: GetPdfPathArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let item_key = ItemKey::from(args.item_key);
        let item = match client.get_item(&item_key).await {
            Ok(item) => item,
            Err(e) => return Ok(text_error(&e)),
        };

        let bridge_roots = self.fetch_bridge_pdf_roots().await;
        let found_path = if item.data.item_type == ItemType::Attachment {
            item.data
                .path
                .as_deref()
                .map(PathBuf::from)
                .or_else(|| {
                    resolve_attachment_pdf_path(&item, &bridge_roots)
                        .map(ResolvedPdfPath::into_path)
                })
                .map(|path| path.display().to_string())
        } else {
            match client.get_item_children(&item_key).await {
                Ok(children) => find_pdf_path(&children, &bridge_roots)
                    .map(ResolvedPdfPath::into_path)
                    .map(|path| path.display().to_string()),
                Err(e) => return Ok(text_error(&e)),
            }
        };

        match found_path {
            Some(path) => Ok(text_success(path)),
            None => Ok(text_error(&ZoteroApiError::NotFound(
                "No PDF attachment found for item".to_owned(),
            ))),
        }
    }

    /// Resolves and security-validates the PDF file path for
    /// `item_key_or_path`, which may be an item key (parent or attachment)
    /// or a direct filesystem path.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`], [`ZoteroApiError::Network`], or
    ///   [`ZoteroApiError::Json`] if the item cannot be fetched
    /// - [`ZoteroApiError::NotFound`] if the item has no PDF attachment (or its
    ///   children cannot be fetched)
    /// - [`ZoteroApiError::InputRejected`] if the path fails security checks
    /// - [`ZoteroApiError::Io`] if canonicalization or PDF validation fails
    async fn resolve_pdf_path(
        &self,
        item_key_or_path: &str,
    ) -> Result<PathBuf, ZoteroApiError> {
        let bridge_roots = self.fetch_bridge_pdf_roots().await;
        if Path::new(item_key_or_path).exists() {
            return self.validate_pdf_read_path(
                Path::new(item_key_or_path),
                &bridge_roots,
                true,
            );
        }

        let client = ZoteroClient::new(&self.state);
        let item_key = ItemKey::from(item_key_or_path);
        let item = client.get_item(&item_key).await?;

        let resolved =
            if item.data.item_type == ItemType::Attachment {
                resolve_attachment_pdf_path(&item, &bridge_roots)
            } else {
                client.get_item_children(&item_key).await.ok().and_then(
                    |children| find_pdf_path(&children, &bridge_roots),
                )
            };
        let Some(resolved) = resolved else {
            return Err(ZoteroApiError::NotFound(format!(
                "No PDF file path found for key: {item_key_or_path}"
            )));
        };

        match resolved {
            ResolvedPdfPath::NeedsRootCheck(path) => {
                self.validate_pdf_read_path(&path, &bridge_roots, false)
            }
            ResolvedPdfPath::Trusted(path) => {
                let checked = canonicalize_existing_path(&path)?;
                self.state.check_pdf_file(&checked)?;
                Ok(checked)
            }
        }
    }

    /// Handles PDF page extraction tool calls via [`extract_pdf_pages`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_read_pdf_pages_impl(
        &self,
        args: ReadPdfPagesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let pdf_path = match self.resolve_pdf_path(&args.item_key_or_path).await
        {
            Ok(path) => path,
            Err(e) => return Ok(text_error(&e)),
        };
        let pages_ref = args.pages.as_deref();
        Ok(json_result(extract_pdf_pages(
            &pdf_path,
            pages_ref,
            self.state.security().max_pdf_bytes(),
        )))
    }

    /// Handles PDF outline extraction tool calls via [`extract_pdf_outline`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_pdf_outline_impl(
        &self,
        args: GetPdfOutlineArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let pdf_path = match self.resolve_pdf_path(&args.item_key_or_path).await
        {
            Ok(path) => path,
            Err(e) => return Ok(text_error(&e)),
        };
        Ok(json_result(extract_pdf_outline(
            &pdf_path,
            self.state.security().max_pdf_bytes(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zotero_api::{AppState, SecurityConfig};

    use super::*;
    use crate::{ZoteroMcpServer, zotero::fixtures::*};

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
            let linked_roots = linked_base_roots(&roots);

            // Assert
            assert_eq!(linked_roots.count(), 0);
        }
    }

    mod pdf_pages {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn rejects_direct_path_by_default() {
            // Arrange
            let temp =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_security(security_with_pdf_limit(1024)),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: temp.path().display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("Direct file paths are disabled"));
        }

        #[tokio::test]
        async fn allows_direct_path_inside_bridge_pdf_root_without_direct_flag()
        {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let body = json!({
                "roots": [{
                    "kind": "attanger-dest",
                    "path": root.path().canonicalize().unwrap(),
                }],
            });
            let bridge_base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_better_notes_url(bridge_base)
                    .with_security(security_with_pdf_limit(1024)),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: pdf.display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }

        #[tokio::test]
        async fn rejects_direct_path_outside_bridge_pdf_roots() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let pdf = outside.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let body = json!({
                "roots": [{
                    "kind": "attanger-dest",
                    "path": root.path().canonicalize().unwrap(),
                }],
            });
            let bridge_base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_better_notes_url(bridge_base)
                    .with_security(security_with_pdf_limit(1024)),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: pdf.display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("Direct file paths are disabled"));
        }

        #[tokio::test]
        async fn allows_direct_path_inside_configured_root_when_bridge_unavailable()
         {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let mut security = SecurityConfig::from_env();
            security.set_direct_file_paths_enabled(true);
            security.set_file_paths_enabled(true);
            security.set_allowed_read_dirs(vec![
                root.path().canonicalize().unwrap(),
            ]);
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_better_notes_url(
                        "http://127.0.0.1:9/better-notes".to_owned(),
                    )
                    .with_security(security),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: pdf.display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }

        #[tokio::test]
        async fn rejects_direct_path_outside_allowed_root() {
            // Arrange
            let allowed = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let pdf = outside.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let mut security = SecurityConfig::from_env();
            security.set_direct_file_paths_enabled(true);
            security.set_file_paths_enabled(true);
            security.set_allowed_read_dirs(vec![
                allowed.path().canonicalize().unwrap(),
            ]);
            let server = ZoteroMcpServer::new(
                AppState::from_env().with_security(security),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: pdf.display().to_string(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("outside allowed"));
        }

        #[tokio::test]
        async fn reads_imported_attachment_enclosure_without_allowed_dirs() {
            // Arrange
            let pdf =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            std::fs::write(pdf.path(), b"not a pdf").unwrap();
            let file_url =
                url::Url::from_file_path(pdf.path()).unwrap().to_string();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "links": {
                    "enclosure": {
                        "href": file_url,
                        "type": "application/pdf",
                        "title": "bad.pdf",
                    },
                },
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "imported_file",
                    "contentType": "application/pdf",
                    "filename": "bad.pdf",
                },
            }]);
            let zotero_base = zotero_pdf_server(&children);
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_zotero_api_url(zotero_base)
                    .with_better_notes_url(
                        "http://127.0.0.1:9/better-notes".to_owned(),
                    )
                    .with_security(security_with_pdf_limit(1024)),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }

        #[tokio::test]
        async fn reads_linked_attanger_attachment_inside_bridge_root() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "linked_file",
                    "contentType": "application/pdf",
                    "path": pdf.display().to_string(),
                },
            }]);
            let zotero_base = zotero_pdf_server(&children);
            let bridge_base = bridge_pdf_root("attanger-dest", root.path());
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_zotero_api_url(zotero_base)
                    .with_better_notes_url(bridge_base)
                    .with_security(security_with_pdf_limit(1024)),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }

        #[tokio::test]
        async fn rejects_linked_attachment_outside_pdf_roots() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let pdf = outside.path().join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "linked_file",
                    "contentType": "application/pdf",
                    "path": pdf.display().to_string(),
                },
            }]);
            let zotero_base = zotero_pdf_server(&children);
            let bridge_base = bridge_pdf_root("attanger-dest", root.path());
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_zotero_api_url(zotero_base)
                    .with_better_notes_url(bridge_base)
                    .with_security(security_with_pdf_limit(1024)),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("outside allowed"));
        }

        #[tokio::test]
        async fn resolves_relative_linked_attachment_from_zotero_base_root() {
            // Arrange
            let base = tempfile::TempDir::new().unwrap();
            let subdir = base.path().join("subdir");
            std::fs::create_dir_all(&subdir).unwrap();
            let pdf = subdir.join("bad.pdf");
            std::fs::write(&pdf, b"not a pdf").unwrap();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "linked_file",
                    "contentType": "application/pdf",
                    "path": "attachments:subdir/bad.pdf",
                },
            }]);
            let zotero_base = zotero_pdf_server(&children);
            let bridge_base =
                bridge_pdf_root("zotero-linked-base", base.path());
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_zotero_api_url(zotero_base)
                    .with_better_notes_url(bridge_base)
                    .with_security(security_with_pdf_limit(1024)),
            );

            // Act
            let res = server
                .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                    pages: None,
                })
                .await
                .expect("read pdf pages result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("PDF extraction error"));
        }
    }

    mod pdf_outline {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn rejects_direct_path_by_default() {
            // Arrange
            let temp =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_security(security_with_pdf_limit(1024)),
            );

            // Act
            let res = server
                .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                    item_key_or_path: temp.path().display().to_string(),
                })
                .await
                .expect("get pdf outline result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("Direct file paths are disabled"));
        }

        #[tokio::test]
        async fn returns_outline_for_direct_path_inside_configured_root() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("outline.pdf");
            zotero_api::pdf::write_pdf_with_outline(&pdf);
            let mut security = SecurityConfig::from_env();
            security.set_direct_file_paths_enabled(true);
            security.set_file_paths_enabled(true);
            security.set_allowed_read_dirs(vec![
                root.path().canonicalize().unwrap(),
            ]);
            let server = ZoteroMcpServer::new(
                AppState::from_env().with_security(security),
            );

            // Act
            let res = server
                .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                    item_key_or_path: pdf.display().to_string(),
                })
                .await
                .expect("get pdf outline result");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = tool_text(&res);
            assert!(text.contains("Chapter 1"));
            assert!(text.contains("Section 2.1"));
        }

        #[tokio::test]
        async fn returns_empty_outline_for_pdf_without_bookmarks() {
            // Arrange
            let root = tempfile::TempDir::new().unwrap();
            let pdf = root.path().join("plain.pdf");
            zotero_api::pdf::write_pdf_without_outline(&pdf);
            let mut security = SecurityConfig::from_env();
            security.set_direct_file_paths_enabled(true);
            security.set_file_paths_enabled(true);
            security.set_allowed_read_dirs(vec![
                root.path().canonicalize().unwrap(),
            ]);
            let server = ZoteroMcpServer::new(
                AppState::from_env().with_security(security),
            );

            // Act
            let res = server
                .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                    item_key_or_path: pdf.display().to_string(),
                })
                .await
                .expect("get pdf outline result");

            // Assert
            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("[]"));
        }

        #[tokio::test]
        async fn reads_imported_attachment_enclosure_outline() {
            // Arrange
            let pdf =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            zotero_api::pdf::write_pdf_with_outline(pdf.path());
            let file_url =
                url::Url::from_file_path(pdf.path()).unwrap().to_string();
            let children = json!([{
                "key": "PDF00001",
                "version": 1,
                "links": {
                    "enclosure": {
                        "href": file_url,
                        "type": "application/pdf",
                        "title": "outline.pdf",
                    },
                },
                "data": {
                    "key": "PDF00001",
                    "version": 1,
                    "itemType": "attachment",
                    "linkMode": "imported_file",
                    "contentType": "application/pdf",
                    "filename": "outline.pdf",
                },
            }]);
            let zotero_base = zotero_pdf_server(&children);
            let server = ZoteroMcpServer::new(
                AppState::from_env()
                    .with_zotero_api_url(zotero_base)
                    .with_better_notes_url(
                        "http://127.0.0.1:9/better-notes".to_owned(),
                    )
                    .with_security(security_with_pdf_limit(1024 * 1024)),
            );

            // Act
            let res = server
                .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                    item_key_or_path: "ITEM0001".to_owned(),
                })
                .await
                .expect("get pdf outline result");

            // Assert
            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("Chapter 1"));
        }
    }
}
