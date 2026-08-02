//! MCP tool handlers and argument models for local Zotero `SQLite` search.
//!
//! Covers `zotero_sqlite_search` grouped-router actions (gated behind
//! `ZOTERO_SQLITE_ACCESS=1`): full-text search and note/annotation search
//! against Zotero's local database.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, mcp::json_result};

/// Arguments for `zotero_fulltext_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FulltextSearchArgs {
    /// Free-text query matched against title, creators, DOI, and indexed
    /// fulltext.
    pub(crate) query: String,
    /// Maximum number of results to return (default: 20).
    pub(crate) limit: Option<usize>,
}
/// Arguments for `zotero_search_notes_annotations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchNotesAnnotationsArgs {
    /// Free-text query matched against note body and annotation text/comment.
    pub(crate) query: String,
    /// Maximum number of results to return (default: 20).
    pub(crate) limit: Option<usize>,
}

impl ZoteroMcpServer {
    /// Handles local full-text search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_fulltext_search_impl(
        &self,
        args: FulltextSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let state = &self.state;
        let result = async {
            let db = state.local_zotero_db().await?;
            db.search_fulltext(&args.query, limit).await
        }
        .await;
        Ok(json_result(result))
    }

    /// Handles local note/annotation search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_notes_annotations_impl(
        &self,
        args: SearchNotesAnnotationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let state = &self.state;
        let result = async {
            let db = state.local_zotero_db().await?;
            db.search_notes_annotations(&args.query, limit).await
        }
        .await;
        Ok(json_result(result))
    }
}
