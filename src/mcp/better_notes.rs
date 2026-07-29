//! MCP tool handlers and argument models for Better Notes tools.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, better_notes::BetterNotesClient};

// --- Argument Schemas ---

/// Arguments for `better_notes_to_markdown`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ToMarkdownArgs {
    /// Note item key.
    pub(crate) item_key: String,
    /// Format: `"html"` or `"markdown"` (default: `"markdown"`).
    pub(crate) format: Option<String>,
}

/// Arguments for `better_notes_from_markdown`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FromMarkdownArgs {
    /// Parent item key to attach the converted note to.
    pub(crate) parent_key: String,
    /// Markdown string to convert into HTML.
    pub(crate) markdown: String,
}

/// Arguments for `better_notes_run_template`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RunTemplateArgs {
    /// Template name to execute.
    pub(crate) template_name: String,
    /// Target Zotero item key.
    pub(crate) item_key: String,
}

/// Arguments for `better_notes_get_relations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteRelationsArgs {
    /// Note item key.
    pub(crate) item_key: String,
}

/// Arguments for `better_notes_get_tree`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteTreeArgs {
    /// Note item key.
    pub(crate) item_key: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    /// Handles Better Notes Markdown export tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
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

    /// Handles Better Notes Markdown import tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
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

    /// Handles Better Notes template execution tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_run_template_impl(
        &self,
        args: RunTemplateArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        Ok(super::json_result(
            client.run_template(&args.template_name, &args.item_key).await,
        ))
    }

    /// Handles Better Notes relation lookup tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_get_relations_impl(
        &self,
        args: NoteRelationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        Ok(super::json_result(client.get_relations(&args.item_key).await))
    }

    /// Handles Better Notes tree lookup tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_get_tree_impl(
        &self,
        args: NoteTreeArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        Ok(super::json_result(client.get_tree(&args.item_key).await))
    }
}
