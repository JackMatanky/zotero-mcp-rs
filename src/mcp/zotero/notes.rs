//! MCP tool handlers and argument models for Zotero note and PDF annotation
//! operations.
//!
//! Covers `zotero_notes` / `zotero_notes_write` grouped-router actions: note
//! listing, annotation synthesis into Markdown, note creation, and PDF
//! highlight/underline/note annotation creation.

use rmcp::model::CallToolResult;
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
