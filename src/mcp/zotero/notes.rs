//! MCP tool handlers and argument models for Zotero note and PDF annotation
//! operations.
//!
//! Covers `zotero_notes` / `zotero_notes_write` grouped-router actions: note
//! listing, annotation synthesis into Markdown, note creation, and PDF
//! highlight/underline/note annotation creation.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, json_success, text_error, text_result},
    zotero::{
        AnnotationDraft, AnnotationPosition, AnnotationType, ItemKey, ItemType,
        ZoteroClient,
    },
};

/// Arguments for `zotero_get_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetNotesArgs {
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_create_note`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateNoteArgs {
    /// Key of the parent item ([`ItemKey`]).
    pub(crate) parent_item_key: ItemKey,
    /// HTML or Markdown content for the note.
    pub(crate) note_content: String,
}
/// Arguments for `zotero_synthesize_annotations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SynthesizeAnnotationsArgs {
    /// Zotero item key ([`ItemKey`]).
    pub(crate) item_key: ItemKey,
}
/// Arguments for `zotero_create_annotation`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateAnnotationArgs {
    /// Key of the parent PDF attachment ([`ItemKey`]).
    pub(crate) parent_attachment_key: ItemKey,
    /// Type of annotation ([`AnnotationType`]).
    pub(crate) annotation_type: AnnotationType,
    /// Selected text (required for highlight/underline, omit for note).
    pub(crate) text: Option<String>,
    /// Optional user comment attached to the annotation.
    pub(crate) comment: Option<String>,
    /// CSS-style hex color, e.g. `"#ffd400"`.
    pub(crate) color: Option<String>,
    /// Optional PDF page label where the annotation appears.
    pub(crate) page_label: Option<String>,
    /// Zotero `annotationPosition` JSON object.
    pub(crate) position: AnnotationPosition,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroNotesCommand {
    List(GetNotesArgs),
    Synthesize(SynthesizeAnnotationsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroNotesWriteCommand {
    Create(CreateNoteArgs),
    Annotation(CreateAnnotationArgs),
}

#[tool_router(router = notes_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_notes",
        description = "Grouped Zotero notes read router. action: list, \
                       synthesize",
        annotations(
            title = "Read Zotero Notes",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_notes(
        &self,
        Parameters(args): Parameters<ZoteroNotesCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroNotesCommand::List(args) => {
                self.zotero_get_notes_impl(args).await
            }
            ZoteroNotesCommand::Synthesize(args) => {
                self.zotero_synthesize_annotations_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_notes_write",
        description = "Grouped Zotero notes write router. action: create, \
                       annotation",
        annotations(
            title = "Write Zotero Notes",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_notes_write(
        &self,
        Parameters(args): Parameters<ZoteroNotesWriteCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroNotesWriteCommand::Create(args) => {
                self.zotero_create_note_impl(args).await
            }
            ZoteroNotesWriteCommand::Annotation(args) => {
                self.zotero_create_annotation_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_get_notes",
        description = "Fetch all note child items for a given item key",
        annotations(
            title = "Get Item Notes",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_notes(
        &self,
        Parameters(args): Parameters<GetNotesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_notes_impl(args).await
    }

    #[tool(
        name = "zotero_synthesize_annotations",
        description = "Extract and synthesize annotations and notes into \
                       structured Markdown",
        annotations(
            title = "Synthesize Annotations",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_synthesize_annotations(
        &self,
        Parameters(args): Parameters<SynthesizeAnnotationsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_synthesize_annotations_impl(args).await
    }

    #[tool(
        name = "zotero_create_note",
        description = "Attach a new note to an item (requires write \
                       permission)",
        annotations(
            title = "Create Note",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_note(
        &self,
        Parameters(args): Parameters<CreateNoteArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_create_note_impl(args).await
    }

    #[tool(
        name = "zotero_create_annotation",
        description = "Create a PDF highlight/underline/note annotation on an \
                       attachment (requires write permission)",
        annotations(
            title = "Create PDF Annotation",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_annotation(
        &self,
        Parameters(args): Parameters<CreateAnnotationArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_create_annotation_impl(args).await
    }
}

impl ZoteroMcpServer {
    /// Handles Zotero note retrieval tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_notes_impl(
        &self,
        args: GetNotesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item_children(&args.item_key).await {
            Ok(children) => {
                let notes: Vec<_> = children
                    .into_iter()
                    .filter(|c| c.data.item_type == ItemType::Note)
                    .collect();
                Ok(json_success(&notes))
            }
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero annotation synthesis tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_synthesize_annotations_impl(
        &self,
        args: SynthesizeAnnotationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(text_result(client.synthesize_annotations(&args.item_key).await))
    }

    /// Handles Zotero note creation tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_note_impl(
        &self,
        args: CreateNoteArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.create_note(&args.parent_item_key, &args.note_content).await,
        ))
    }

    /// Handles Zotero PDF annotation creation tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_annotation_impl(
        &self,
        args: CreateAnnotationArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let draft = AnnotationDraft {
            parent_attachment_key: args.parent_attachment_key,
            annotation_type: args.annotation_type,
            text: args.text,
            comment: args.comment,
            color: args.color,
            page_label: args.page_label,
            position: args.position,
        };
        Ok(json_result(client.create_annotation(draft).await))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        ZoteroMcpServer,
        mcp::zotero::fixtures::*,
        zotero::{AnnotationPosition, AnnotationType},
    };

    mod write_operations {

        use super::*;

        #[tokio::test]
        async fn create_annotation_creates_pdf_annotation() {
            // Arrange
            let created = json!([{
                "key": "ANNOT1",
                "version": 1,
                "data": { "key": "ANNOT1", "version": 1, "itemType": "annotation", "annotationType": "highlight" }
            }]);
            let base = mock_server(vec![http_response(
                "200 OK",
                &created.to_string(),
            )]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_create_annotation_impl(CreateAnnotationArgs {
                    parent_attachment_key: "ATT1".into(),
                    annotation_type: AnnotationType::Highlight,
                    text: Some("selected text".to_owned()),
                    comment: None,
                    color: None,
                    page_label: None,
                    position: AnnotationPosition::from(
                        json!({"pageIndex": 0, "rects": [[100, 200, 300, 220]]}),
                    ),
                })
                .await
                .expect("create annotation ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }
}
