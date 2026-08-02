//! MCP tool handlers and argument models for Zotero PDF attachment access.
//!
//! Covers `zotero_pdf` grouped-router actions: attachment path lookup,
//! page-range text extraction, and outline (table of contents) extraction.
//! Delegates path resolution and security enforcement to
//! [`crate::mcp::pdf`].

use std::path::{Path, PathBuf};

use rmcp::model::CallToolResult;
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
