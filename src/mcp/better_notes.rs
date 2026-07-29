//! MCP tool handlers, argument models, and unit tests for Better Notes tools.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, better_notes::BetterNotesClient};

// --- Argument Schemas ---

/// Arguments for `better_notes_to_markdown`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ToMarkdownArgs {
    /// Note item key
    pub(crate) item_key: String,
    /// Format: `"html"` or `"markdown"` (default: `"markdown"`)
    pub(crate) format: Option<String>,
}

/// Arguments for `better_notes_from_markdown`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FromMarkdownArgs {
    /// Parent item key to attach the converted note to
    pub(crate) parent_key: String,
    /// Markdown string to convert into HTML
    pub(crate) markdown: String,
}

/// Arguments for `better_notes_run_template`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RunTemplateArgs {
    /// Template name to execute
    pub(crate) template_name: String,
    /// Target Zotero item key
    pub(crate) item_key: String,
}

/// Arguments for `better_notes_get_relations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteRelationsArgs {
    /// Note item key
    pub(crate) item_key: String,
}

/// Arguments for `better_notes_get_tree`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct NoteTreeArgs {
    /// Note item key
    pub(crate) item_key: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    pub(crate) async fn better_notes_to_markdown_impl(
        &self,
        args: ToMarkdownArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        let format = args.format.as_deref();
        match client.to_markdown(Some(&args.item_key), format).await {
            Ok(output) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    output,
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_notes_from_markdown_impl(
        &self,
        args: FromMarkdownArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        match client
            .convert_from_markdown(&args.parent_key, &args.markdown)
            .await
        {
            Ok(key) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    key,
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_notes_run_template_impl(
        &self,
        args: RunTemplateArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        match client.run_template(&args.template_name, &args.item_key).await {
            Ok(result) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&result).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_notes_get_relations_impl(
        &self,
        args: NoteRelationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        match client.get_relations(&args.item_key).await {
            Ok(relations) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&relations)
                        .unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_notes_get_tree_impl(
        &self,
        args: NoteTreeArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterNotesClient::new(&self.state);
        match client.get_tree(&args.item_key).await {
            Ok(tree) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&tree).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }
}
