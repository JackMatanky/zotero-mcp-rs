//! MCP tool handlers and argument models for Zotero notes.
//!
//! This module defines the grouped `zotero_notes` read router and
//! `zotero_notes_write` write router, dispatching commands to note retrieval,
//! note creation, and PDF annotation handlers.
//!
//! # Main Types
//!
//! - [`ZoteroNotesCommand`] - Grouped-router command for read-only note actions
//! - [`ZoteroNotesWriteCommand`] - Grouped-router command for write note
//!   actions
//! - [`GetNotesArgs`] - Arguments for the `list` action of `zotero_notes`
//! - [`CreateNoteArgs`] - Arguments for the `create` action of
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
    mcp::{json_result, json_success, text_error},
    zotero::{ItemKey, ItemType, ZoteroClient, ZoteroItem},
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

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Read commands dispatched by the `zotero_notes` MCP tool router.
pub(crate) enum ZoteroNotesCommand {
    /// List notes attached to an item.
    List(GetNotesArgs),
    /// Synthesize annotations into a structured note.
    Synthesize(crate::mcp::zotero::annotations::SynthesizeAnnotationsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Write commands dispatched by the `zotero_notes` MCP tool router.
pub(crate) enum ZoteroNotesWriteCommand {
    /// Create a note on an item.
    Create(CreateNoteArgs),
    /// Create an annotation on an attached PDF.
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
pub(crate) fn filter_notes(children: Vec<ZoteroItem>) -> Vec<ZoteroItem> {
    children
        .into_iter()
        .filter(|child| child.data.item_type == ItemType::Note)
        .collect()
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
}
