//! MCP tool handlers and argument models for Zotero notes and PDF annotations.
//!
//! This module defines the grouped `zotero_notes` read router and
//! `zotero_notes_write` write router, dispatching commands to note retrieval,
//! note creation, PDF annotation creation, and annotation synthesis handlers.
//!
//! # Main Types
//!
//! - [`ZoteroNotesCommand`] - Grouped-router command for read-only note actions
//! - [`ZoteroNotesWriteCommand`] - Grouped-router command for write note
//!   actions
//! - [`GetNotesArgs`] - Arguments for the `list` action of `zotero_notes`
//! - [`CreateNoteArgs`] - Arguments for the `create` action of
//!   `zotero_notes_write`
//! - [`SynthesizeAnnotationsArgs`] - Arguments for the `synthesize` action of
//!   `zotero_notes`
//! - [`CreateAnnotationArgs`] - Arguments for the `annotation` action of
//!   `zotero_notes_write`
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::ZoteroMcpServer;
//! # use zotero_mcp_rs::mcp::zotero::notes::{
//! #     ZoteroNotesCommand,
//! #     GetNotesArgs,
//! # };
//! # use rmcp::handler::server::wrapper::Parameters;
//! # async fn run(
//! #     server: ZoteroMcpServer,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let cmd = ZoteroNotesCommand::List(
//!     serde_json::from_value(
//!         serde_json::json!({
//!             "item_key": "ITEM1234"
//!         }),
//!     )?,
//! );
//! let result = server
//!     .zotero_notes(Parameters(cmd))
//!     .await?;
//! # Ok(())
//! # }
//! ```

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
        ZoteroClient, ZoteroItem,
    },
};

/// Arguments for the `list` action of `zotero_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetNotesArgs {
    /// Unique Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
}

/// Arguments for the `create` action of `zotero_notes_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateNoteArgs {
    /// Key of the parent item ([`ItemKey`]).
    parent_item_key: ItemKey,
    /// HTML or Markdown content for the note.
    note_content: String,
}

/// Arguments for the `synthesize` action of `zotero_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SynthesizeAnnotationsArgs {
    /// Unique Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
}

/// Arguments for the `annotation` action of `zotero_notes_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateAnnotationArgs {
    /// Key of the parent PDF attachment ([`ItemKey`]).
    parent_attachment_key: ItemKey,
    /// Type of annotation ([`AnnotationType`]).
    annotation_type: AnnotationType,
    /// Selected text (required for highlight/underline, omitted for note).
    text: Option<String>,
    /// Optional user comment attached to the annotation.
    comment: Option<String>,
    /// CSS-style hex color code, for example `"#ffd400"`.
    color: Option<String>,
    /// Optional PDF page label where the annotation appears.
    page_label: Option<String>,
    /// Zotero `annotationPosition` JSON object.
    position: AnnotationPosition,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Read commands dispatched by the `zotero_notes` MCP tool router.
pub(crate) enum ZoteroNotesCommand {
    /// List notes attached to an item.
    List(GetNotesArgs),
    /// Synthesize annotations into a structured note.
    Synthesize(SynthesizeAnnotationsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Write commands dispatched by the `zotero_notes` MCP tool router.
pub(crate) enum ZoteroNotesWriteCommand {
    /// Create a note on an item.
    Create(CreateNoteArgs),
    /// Create an annotation on an attached PDF.
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
    /// Dispatches read-only Zotero note commands (`list`, `synthesize`).
    ///
    /// Receives [`Parameters<ZoteroNotesCommand>`] and delegates execution to
    /// either note listing or annotation synthesis handlers.
    ///
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
    /// Dispatches write Zotero note commands (`create`, `annotation`).
    ///
    /// Receives [`Parameters<ZoteroNotesWriteCommand>`] and delegates execution
    /// to either note creation or PDF annotation creation handlers.
    ///
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
}

/// Filters child items to only those with `ItemType::Note`.
pub(crate) fn filter_notes(mut children: Vec<ZoteroItem>) -> Vec<ZoteroItem> {
    children.retain(|child| child.data.item_type == ItemType::Note);
    children
}

impl ZoteroMcpServer {
    /// Handles Zotero note retrieval tool calls.
    ///
    /// Fetches child items using [`ZoteroClient::get_item_children`] and
    /// filters results to items of type [`ItemType::Note`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_notes_impl(
        &self,
        args: GetNotesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item_children(&args.item_key).await {
            Ok(children) => Ok(json_success(&filter_notes(children))),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero note creation tool calls.
    ///
    /// Creates a child note attached to `args.parent_item_key` via
    /// [`ZoteroClient::create_note`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_create_note_impl(
        &self,
        args: CreateNoteArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.create_note(&args.parent_item_key, &args.note_content).await,
        ))
    }

    /// Handles Zotero annotation synthesis tool calls.
    ///
    /// Synthesizes annotations for the item specified by `args.item_key` using
    /// [`ZoteroClient::synthesize_annotations`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_synthesize_annotations_impl(
        &self,
        args: SynthesizeAnnotationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(text_result(client.synthesize_annotations(&args.item_key).await))
    }

    /// Handles Zotero PDF annotation creation tool calls.
    ///
    /// Constructs an [`AnnotationDraft`] from `args` and creates the annotation
    /// via [`ZoteroClient::create_annotation`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_create_annotation_impl(
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

    mod annotations {
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
