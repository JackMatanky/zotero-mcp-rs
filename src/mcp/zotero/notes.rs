//! MCP tool handlers and argument models for Zotero notes.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, json_success, text_error},
    zotero::{ItemKey, ItemType, ZoteroClient},
};

/// Arguments for the `list` action of `zotero_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetNotesArgs {
    /// Zotero item key ([`ItemKey`]).
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

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroNotesCommand {
    List(GetNotesArgs),
    Synthesize(crate::mcp::zotero::annotations::SynthesizeAnnotationsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroNotesWriteCommand {
    Create(CreateNoteArgs),
    Annotation(crate::mcp::zotero::annotations::CreateAnnotationArgs),
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
}

impl ZoteroMcpServer {
    /// Handles Zotero note retrieval tool calls.
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
    async fn zotero_create_note_impl(
        &self,
        args: CreateNoteArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.create_note(&args.parent_item_key, &args.note_content).await,
        ))
    }
}
