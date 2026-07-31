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
