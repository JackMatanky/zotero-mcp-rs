//! MCP tool handlers and argument models for Zotero PDF attachment access.
//!
//! Covers `zotero_pdf` grouped-router actions: attachment path lookup,
//! page-range text extraction, and outline (table of contents) extraction.
//! Delegates path resolution and security enforcement to
//! [`crate::mcp::pdf`].

use std::path::{Path, PathBuf};

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    errors::ZoteroMcpError,
    mcp::{
        json_result,
        pdf::{
            ResolvedPdfPath, canonicalize_existing_path, find_pdf_path,
            resolve_attachment_pdf_path,
        },
        text_error, text_success,
    },
    pdf::{extract_pdf_outline, extract_pdf_pages},
    zotero::{ItemKey, ItemType, ZoteroClient},
};

/// Arguments for `zotero_get_pdf_path`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfPathArgs {
    /// Zotero item key ([`ItemKey`]) for parent item or attachment item.
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_read_pdf_pages`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ReadPdfPagesArgs {
    /// Zotero item key; direct PDF paths must resolve under configured or
    /// Zotero-reported PDF roots, otherwise direct-path opt-in is required.
    pub(crate) item_key_or_path: String,
    /// 1-based page numbers to extract (e.g. `[1, 2, 3]`).
    pub(crate) pages: Option<Vec<usize>>,
}
/// Arguments for `zotero_get_pdf_outline`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfOutlineArgs {
    /// Zotero item key; direct PDF paths must resolve under configured or
    /// Zotero-reported PDF roots, otherwise direct-path opt-in is required.
    pub(crate) item_key_or_path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroPdfCommand {
    Path(GetPdfPathArgs),
    ReadPages(ReadPdfPagesArgs),
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

    #[tool(
        name = "zotero_get_pdf_path",
        description = "Locate the local PDF file path for an item or its \
                       attachment",
        annotations(
            title = "Locate Item PDF",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_pdf_path(
        &self,
        Parameters(args): Parameters<GetPdfPathArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_pdf_path_impl(args).await
    }

    #[tool(
        name = "zotero_read_pdf_pages",
        description = "Extract raw text from specific 1-based pages of a PDF",
        annotations(
            title = "Read PDF Pages",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_read_pdf_pages(
        &self,
        Parameters(args): Parameters<ReadPdfPagesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_read_pdf_pages_impl(args).await
    }

    #[tool(
        name = "zotero_get_pdf_outline",
        description = "Extract the PDF outline (table of contents/bookmarks) \
                       for an item's PDF attachment or a direct PDF path",
        annotations(
            title = "Get PDF Outline",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_pdf_outline(
        &self,
        Parameters(args): Parameters<GetPdfOutlineArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_pdf_outline_impl(args).await
    }
}

impl ZoteroMcpServer {
    /// Handles Zotero PDF path discovery tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_pdf_path_impl(
        &self,
        args: GetPdfPathArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let item = match client.get_item(&args.item_key).await {
            Ok(item) => item,
            Err(e) => return Ok(text_error(&e)),
        };

        let bridge_roots = Vec::new();
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
            match client.get_item_children(&args.item_key).await {
                Ok(children) => find_pdf_path(&children, &bridge_roots)
                    .map(ResolvedPdfPath::into_path)
                    .map(|path| path.display().to_string()),
                Err(e) => return Ok(text_error(&e)),
            }
        };

        match found_path {
            Some(path) => Ok(text_success(path)),
            None => Ok(text_error("No PDF attachment found for item")),
        }
    }

    /// Resolves and security-validates the PDF file path for
    /// `item_key_or_path`, which may be an item key (parent or attachment)
    /// or a direct filesystem path.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`], [`ZoteroMcpError::Network`], or
    ///   [`ZoteroMcpError::Json`] if the item cannot be fetched
    /// - [`ZoteroMcpError::NotFound`] if the item has no PDF attachment (or its
    ///   children cannot be fetched)
    /// - [`ZoteroMcpError::InputRejected`] if the path fails security checks
    /// - [`ZoteroMcpError::Io`] if canonicalization or PDF validation fails
    pub(crate) async fn resolve_pdf_path(
        &self,
        item_key_or_path: &str,
    ) -> Result<PathBuf, ZoteroMcpError> {
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
            return Err(ZoteroMcpError::NotFound(format!(
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

    /// Handles PDF page extraction tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_read_pdf_pages_impl(
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
            self.state.security.max_pdf_bytes,
        )))
    }

    /// Handles PDF outline extraction tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_pdf_outline_impl(
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
            self.state.security.max_pdf_bytes,
        )))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        ZoteroMcpServer, mcp::zotero::fixtures::*, security::SecurityConfig,
        state::AppState,
    };

    mod pdf_pages {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn rejects_direct_path_by_default() {
            // Arrange
            let temp =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            let server = ZoteroMcpServer::new(AppState {
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

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
            let server = ZoteroMcpServer::new(AppState {
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

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
            let server = ZoteroMcpServer::new(AppState {
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

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
            let mut security = SecurityConfig::default();
            security.direct_file_paths = true;
            security.allowed_read_dirs =
                vec![root.path().canonicalize().unwrap()];
            let server = ZoteroMcpServer::new(AppState {
                better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
                security,
                ..AppState::from_env()
            });

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
            let mut security = SecurityConfig::default();
            security.direct_file_paths = true;
            security.allowed_read_dirs =
                vec![allowed.path().canonicalize().unwrap()];
            let server = ZoteroMcpServer::new(AppState {
                security,
                ..AppState::from_env()
            });

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
            let zotero_base = zotero_pdf_server(children);
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

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
            let zotero_base = zotero_pdf_server(children);
            let bridge_base = bridge_pdf_root("attanger-dest", root.path());
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

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
            let zotero_base = zotero_pdf_server(children);
            let bridge_base = bridge_pdf_root("attanger-dest", root.path());
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

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
            let zotero_base = zotero_pdf_server(children);
            let bridge_base =
                bridge_pdf_root("zotero-linked-base", base.path());
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: bridge_base,
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

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
            let server = ZoteroMcpServer::new(AppState {
                security: security_with_pdf_limit(1024),
                ..AppState::from_env()
            });

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
            crate::pdf::write_pdf_with_outline(&pdf);
            let mut security = SecurityConfig::default();
            security.direct_file_paths = true;
            security.allowed_read_dirs =
                vec![root.path().canonicalize().unwrap()];
            let server = ZoteroMcpServer::new(AppState {
                security,
                ..AppState::from_env()
            });

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
            crate::pdf::write_pdf_without_outline(&pdf);
            let mut security = SecurityConfig::default();
            security.direct_file_paths = true;
            security.allowed_read_dirs =
                vec![root.path().canonicalize().unwrap()];
            let server = ZoteroMcpServer::new(AppState {
                security,
                ..AppState::from_env()
            });

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
            crate::pdf::write_pdf_with_outline(pdf.path());
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
            let zotero_base = zotero_pdf_server(children);
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
                security: security_with_pdf_limit(1024 * 1024),
                ..AppState::from_env()
            });

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
