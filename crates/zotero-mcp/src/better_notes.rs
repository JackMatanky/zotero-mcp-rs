//! MCP tool handlers and argument models for Better Notes integration.
//!
//! This module provides handlers for interacting with the Zotero Better Notes
//! plugin. Supported operations include:
//! - Exporting notes to Markdown or HTML ([`NoteExportArgs`])
//! - Creating Zotero notes from Markdown content ([`FromMarkdownArgs`])
//! - Running note templates ([`RunTemplateArgs`])
//! - Querying note relations ([`NoteRelationsArgs`])
//! - Retrieving note tree structures ([`NoteTreeArgs`])
//!
//! # Examples
//!
//! ```ignore
//! # use zotero_mcp::ZoteroMcpServer;
//! # use zotero_api::AppState;
//! # use zotero_mcp::better_notes::NoteExportArgs;
//! # use zotero_api::ItemKey;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let server = ZoteroMcpServer::new(AppState::from_env());
//! let args = NoteExportArgs {
//!     item_key: ItemKey::from("ABCD1234"),
//!     format: None,
//! };
//! let result = server.better_notes_export_impl(args).await?;
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::{BetterNotesClient, ItemKey, TemplateName};

use crate::ZoteroMcpServer;

/// Mirrors `zotero_api::NoteExportFormat` for MCP argument schemas.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NoteExportFormatArg {
    Markdown,
    Html,
}
impl From<NoteExportFormatArg> for zotero_api::NoteExportFormat {
    #[inline]
    fn from(value: NoteExportFormatArg) -> Self {
        match value {
            NoteExportFormatArg::Markdown => Self::Markdown,
            NoteExportFormatArg::Html => Self::Html,
        }
    }
}

// --- Argument Schemas ---

/// Arguments for exporting a Better Notes note to Markdown or HTML.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteExportArgs {
    /// Note item key ([`ItemKey`]) to export.
    pub(crate) item_key: String,
    /// Output format ([`NoteExportFormat`]), defaulting to Markdown when
    /// [`None`].
    pub(crate) format: Option<NoteExportFormatArg>,
}

/// Arguments for importing Markdown into a Better Notes note.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FromMarkdownArgs {
    /// Parent item key ([`ItemKey`]) to attach the converted note to.
    /// Omit for a top-level note.
    pub(crate) parent_key: Option<String>,
    /// Markdown string content to convert into HTML.
    pub(crate) markdown: String,
}

/// Arguments for executing a Better Notes template.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RunTemplateArgs {
    /// Name of the template ([`TemplateName`]) to execute.
    pub(crate) template_name: String,
    /// Target Zotero item key ([`ItemKey`]) for template execution.
    pub(crate) item_key: String,
}

/// Arguments for retrieving Better Notes note relations.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteRelationsArgs {
    /// Note item key ([`ItemKey`]) to retrieve relations for.
    pub(crate) item_key: String,
}

/// Arguments for retrieving a Better Notes note tree structure.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteTreeArgs {
    /// Note item key ([`ItemKey`]) to retrieve tree structure for.
    pub(crate) item_key: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    /// Exports a Better Notes note to Markdown or HTML using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if note export fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_notes_export_impl(
        &self,
        args: NoteExportArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        Ok(crate::response::text_result(
            client
                .export(
                    &ItemKey::from(args.item_key),
                    args.format.map(Into::into),
                )
                .await,
        ))
    }

    /// Converts Markdown content into a Better Notes note using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if Markdown conversion fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_notes_from_markdown_impl(
        &self,
        args: FromMarkdownArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        Ok(crate::response::text_result(
            client
                .convert_from_markdown(
                    args.parent_key.map(ItemKey::from).as_ref(),
                    &args.markdown,
                )
                .await
                .map(|key| key.to_string()),
        ))
    }

    /// Executes a Better Notes template against a target item using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if template execution fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_notes_run_template_impl(
        &self,
        args: RunTemplateArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        Ok(crate::response::text_result(
            client
                .run_template(
                    &TemplateName::from(args.template_name),
                    &ItemKey::from(args.item_key),
                )
                .await,
        ))
    }

    /// Retrieves Better Notes relations for a note using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if relation lookup fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_notes_get_relations_impl(
        &self,
        args: NoteRelationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        Ok(crate::response::json_result(
            client.get_relations(&ItemKey::from(args.item_key)).await,
        ))
    }

    /// Retrieves a Better Notes note tree structure using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if note tree retrieval fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_notes_get_tree_impl(
        &self,
        args: NoteTreeArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        Ok(crate::response::json_result(
            client.get_tree(&ItemKey::from(args.item_key)).await,
        ))
    }
}

/// Commands dispatched by the `better_notes` MCP tool router.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum BetterNotesCommand {
    /// Export a note to Markdown or HTML.
    Export(NoteExportArgs),
    /// Create a note from Markdown content.
    FromMarkdown(FromMarkdownArgs),
    /// Execute a note template.
    RunTemplate(RunTemplateArgs),
    /// Retrieve note relations.
    Relations(NoteRelationsArgs),
    /// Retrieve note tree structure.
    Tree(NoteTreeArgs),
}

#[tool_router(router = better_notes_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "better_notes",
        description = "Grouped Better Notes router. action: export, \
                       from_markdown, run_template, relations, tree",
        annotations(
            title = "Better Notes",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn better_notes(
        &self,
        Parameters(args): Parameters<BetterNotesCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            BetterNotesCommand::Export(args) => {
                self.better_notes_export_impl(args).await
            }
            BetterNotesCommand::FromMarkdown(args) => {
                self.better_notes_from_markdown_impl(args).await
            }
            BetterNotesCommand::RunTemplate(args) => {
                self.better_notes_run_template_impl(args).await
            }
            BetterNotesCommand::Relations(args) => {
                self.better_notes_get_relations_impl(args).await
            }
            BetterNotesCommand::Tree(args) => {
                self.better_notes_get_tree_impl(args).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zotero_api::AppState;

    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        pub(super) fn better_notes_state(better_notes_url: String) -> AppState {
            AppState::test_default()
                .with_better_notes_url(better_notes_url)
                .with_write_enabled(true)
        }

        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }
    }

    use fixtures::*;

    mod export {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn exports_note_as_markdown() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r##"{"content":"# Exported"}"##,
            )]);
            let server = ZoteroMcpServer::new(better_notes_state(base));

            // Act
            let res = server
                .better_notes_export_impl(NoteExportArgs {
                    item_key: "NOTE1".into(),
                    format: Some(NoteExportFormatArg::Markdown),
                })
                .await
                .expect("export ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str());
            assert_eq!(text, Some("# Exported"));
        }

        #[tokio::test]
        async fn exports_note_as_html() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"content":"<h1>Exported</h1>"}"#,
            )]);
            let server = ZoteroMcpServer::new(better_notes_state(base));

            // Act
            let res = server
                .better_notes_export_impl(NoteExportArgs {
                    item_key: "NOTE1".into(),
                    format: Some(NoteExportFormatArg::Html),
                })
                .await
                .expect("export ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str());
            assert_eq!(text, Some("<h1>Exported</h1>"));
        }
    }

    mod templates {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn runs_template_and_returns_rendered_text() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r##"{"result":"# Rendered"}"##,
            )]);
            let server = ZoteroMcpServer::new(better_notes_state(base));

            // Act
            let res = server
                .better_notes_run_template_impl(RunTemplateArgs {
                    template_name: "Export".into(),
                    item_key: "NOTE1".into(),
                })
                .await
                .expect("template ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str());
            assert_eq!(text, Some("# Rendered"));
        }
    }

    mod import {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn imports_markdown_into_note() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"itemKey":"NEWNOTE1"}"#,
            )]);
            let server = ZoteroMcpServer::new(better_notes_state(base));

            // Act
            let res = server
                .better_notes_from_markdown_impl(FromMarkdownArgs {
                    parent_key: Some("PARENT1".into()),
                    markdown: "# Note Title".to_owned(),
                })
                .await
                .expect("import ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str());
            assert_eq!(text, Some("NEWNOTE1"));
        }
    }

    mod relations_and_trees {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn fetches_note_relations() {
            // Arrange
            let body = json!({
                "relations": { "outbound": [], "inbound": [] }
            });
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(better_notes_state(base));

            // Act
            let res = server
                .better_notes_get_relations_impl(NoteRelationsArgs {
                    item_key: "NOTE1".into(),
                })
                .await
                .expect("relations ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }

        #[tokio::test]
        async fn fetches_note_tree() {
            // Arrange
            let body = json!({
                "tree": { "key": "NOTE1", "children": [] }
            });
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(better_notes_state(base));

            // Act
            let res = server
                .better_notes_get_tree_impl(NoteTreeArgs {
                    item_key: "NOTE1".into(),
                })
                .await
                .expect("tree ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    /// Reverse-exhaustive match on `zotero_api::NoteExportFormat`: if a
    /// variant is added there, this fails to compile until
    /// `NoteExportFormatArg` (and its `From` impl above) is updated too,
    /// catching schema drift a one-directional match cannot.
    mod arg_mirrors {
        use super::*;

        #[test]
        fn note_export_format_arg_covers_every_variant() {
            fn to_arg(
                format: zotero_api::NoteExportFormat,
            ) -> NoteExportFormatArg {
                match format {
                    zotero_api::NoteExportFormat::Markdown => {
                        NoteExportFormatArg::Markdown
                    }
                    zotero_api::NoteExportFormat::Html => {
                        NoteExportFormatArg::Html
                    }
                }
            }
            let _ = to_arg;
        }
    }
}
