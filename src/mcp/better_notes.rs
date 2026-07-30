//! MCP tool handlers and argument models for Better Notes integration.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, better_notes::BetterNotesClient};

// --- Argument Schemas ---

/// Arguments for exporting a Better Notes note to Markdown.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ToMarkdownArgs {
    /// Note item key to export.
    pub(crate) item_key: String,
    /// Output format (`"html"` or `"markdown"`), defaulting to `"markdown"` when [`None`].
    pub(crate) format: Option<String>,
}

/// Arguments for importing Markdown into a Better Notes note.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FromMarkdownArgs {
    /// Parent item key to attach the converted note to.
    pub(crate) parent_key: String,
    /// Markdown string content to convert into HTML.
    pub(crate) markdown: String,
}

/// Arguments for executing a Better Notes template.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RunTemplateArgs {
    /// Name of the template to execute.
    pub(crate) template_name: String,
    /// Target Zotero item key for template execution.
    pub(crate) item_key: String,
}

/// Arguments for retrieving Better Notes note relations.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteRelationsArgs {
    /// Note item key to retrieve relations for.
    pub(crate) item_key: String,
}

/// Arguments for retrieving a Better Notes note tree structure.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteTreeArgs {
    /// Note item key to retrieve tree structure for.
    pub(crate) item_key: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    /// Exports a Better Notes note to Markdown or HTML using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if Markdown export fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_notes_to_markdown_impl(
        &self,
        args: ToMarkdownArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        let format = args.format.as_deref();
        Ok(super::text_result(
            client.to_markdown(Some(&args.item_key), format).await,
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
        Ok(super::text_result(
            client
                .convert_from_markdown(&args.parent_key, &args.markdown)
                .await,
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
        Ok(super::json_result(
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
