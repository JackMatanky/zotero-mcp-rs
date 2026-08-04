//! MCP tool handlers for Zotero attachment items.
//!
//! Main types:
//! - [`AttachFileArgs`] - Arguments for the `attach_file` action
//! - [`ImportPdfArgs`] - Arguments for the `import_pdf` action

use std::path::Path;

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, text_error},
    zotero::{ItemKey, ZoteroClient},
};

/// Arguments for the `attach_file` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AttachFileArgs {
    /// Key of the parent item ([`ItemKey`]).
    parent_item_key: ItemKey,
    /// Display title for the attachment.
    title: String,
    /// File path or URL.
    path_or_url: String,
    /// Optional content type (default: `"application/pdf"`).
    content_type: Option<String>,
}

/// Arguments for the `import_pdf` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ImportPdfArgs {
    /// Optional key of the parent item ([`ItemKey`]); omitted to create a
    /// top-level attachment.
    parent_item_key: Option<ItemKey>,
    /// Display title for the attachment.
    title: String,
    /// Local path to the PDF file to import.
    file_path: String,
    /// Optional content type (default: `"application/pdf"`).
    content_type: Option<String>,
}

impl ZoteroMcpServer {
    /// Handles Zotero linked-file attachment tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_attach_file_impl(
        &self,
        args: AttachFileArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .attach_file_link(
                    &args.parent_item_key,
                    &args.title,
                    &args.path_or_url,
                    args.content_type.as_deref(),
                )
                .await,
        ))
    }

    /// Handles Zotero PDF import tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_import_pdf_impl(
        &self,
        args: ImportPdfArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bridge_roots = self.fetch_bridge_pdf_roots().await;
        let checked = match self.validate_pdf_read_path(
            Path::new(&args.file_path),
            &bridge_roots,
            true,
        ) {
            Ok(path) => path,
            Err(e) => return Ok(text_error(&e)),
        };
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .import_pdf_file(
                    args.parent_item_key.as_ref(),
                    &args.title,
                    &checked,
                    args.content_type.as_deref(),
                )
                .await,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ZoteroMcpServer,
        mcp::zotero::fixtures::{
            http_response, mock_server, tool_text, zotero_state,
        },
        security::SecurityConfig,
    };

    fn import_server(upload_base: &str) -> String {
        let created = serde_json::json!([{
            "key": "ATTACH01",
            "version": 1,
            "data": { "key": "ATTACH01", "version": 1, "itemType": "attachment" },
        }])
        .to_string();
        let phase1 = serde_json::json!({
            "url": format!("{upload_base}/upload"),
            "uploadKey": "uk",
            "contentType": "application/pdf",
            "prefix": "",
            "suffix": "",
        })
        .to_string();
        mock_server(vec![
            http_response("200 OK", &created),
            http_response("200 OK", &phase1),
            http_response("204 No Content", ""),
        ])
    }

    #[tokio::test]
    async fn import_pdf_uploads_file_and_reports_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pdf_path = dir.path().join("paper.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4\n%%EOF\n").expect("write pdf");

        let upload_base = mock_server(vec![http_response("201 Created", "")]);
        let base = import_server(&upload_base);

        let mut app = zotero_state(base);
        app.security = SecurityConfig {
            direct_file_paths: true,
            allowed_read_dirs: vec![dir.path().to_path_buf()],
            ..app.security
        };
        let server = ZoteroMcpServer::new(app);

        let res = server
            .zotero_import_pdf_impl(ImportPdfArgs {
                parent_item_key: Some("PARENT01".into()),
                title: "Paper".to_owned(),
                file_path: pdf_path.to_string_lossy().into_owned(),
                content_type: None,
            })
            .await
            .expect("import ok");

        assert_eq!(res.is_error, Some(false));
        assert!(tool_text(&res).contains("ATTACH01"));
    }
}
