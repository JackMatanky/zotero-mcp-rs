//! MCP tool handlers and argument models for Better Notes integration.
//!
//! This module provides handlers for interacting with the Zotero Better Notes
//! plugin. Supported operations include:
//! - Exporting notes to Markdown or HTML ([`NoteExportArgs`])
//! - Creating Zotero notes from Markdown content ([`FromMarkdownArgs`])
//! - Running note templates ([`RunTemplateArgs`])
//! - Querying note relations ([`NoteRelationsArgs`])
//! - Retrieving note tree structures ([`NoteTreeArgs`])

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    better_notes::{BetterNotesClient, NoteExportFormat, TemplateName},
    zotero::ItemKey,
};

// --- Argument Schemas ---

/// Arguments for exporting a Better Notes note to Markdown or HTML.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteExportArgs {
    /// Note item key ([`ItemKey`]) to export.
    pub(crate) item_key: ItemKey,
    /// Output format ([`NoteExportFormat`]), defaulting to Markdown when
    /// [`None`].
    pub(crate) format: Option<NoteExportFormat>,
}

/// Arguments for importing Markdown into a Better Notes note.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FromMarkdownArgs {
    /// Parent item key ([`ItemKey`]) to attach the converted note to.
    /// Omit for a top-level note.
    pub(crate) parent_key: Option<ItemKey>,
    /// Markdown string content to convert into HTML.
    pub(crate) markdown: String,
}

/// Arguments for executing a Better Notes template.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RunTemplateArgs {
    /// Name of the template ([`TemplateName`]) to execute.
    pub(crate) template_name: TemplateName,
    /// Target Zotero item key ([`ItemKey`]) for template execution.
    pub(crate) item_key: ItemKey,
}

/// Arguments for retrieving Better Notes note relations.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteRelationsArgs {
    /// Note item key ([`ItemKey`]) to retrieve relations for.
    pub(crate) item_key: ItemKey,
}

/// Arguments for retrieving a Better Notes note tree structure.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteTreeArgs {
    /// Note item key ([`ItemKey`]) to retrieve tree structure for.
    pub(crate) item_key: ItemKey,
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
        Ok(super::text_result(client.export(&args.item_key, args.format).await))
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
        Ok(super::text_result(
            client
                .convert_from_markdown(args.parent_key.as_ref(), &args.markdown)
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
        Ok(super::text_result(
            client.run_template(&args.template_name, &args.item_key).await,
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
        Ok(super::json_result(client.get_relations(&args.item_key).await))
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
        Ok(super::json_result(client.get_tree(&args.item_key).await))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::state::AppState;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        pub(super) fn better_notes_state(better_notes_url: String) -> AppState {
            AppState {
                zotero_api_url: String::new(),
                better_bibtex_url: String::new(),
                better_notes_url,
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            }
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
                    format: Some(NoteExportFormat::Markdown),
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
                    format: Some(NoteExportFormat::Html),
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
}
