//! MCP tool handlers and argument models for local Zotero `SQLite` search.
//!
//! Covers `zotero_sqlite_search` grouped-router actions (gated behind
//! `ZOTERO_SQLITE_ACCESS=1`): full-text search and note/annotation search
//! against Zotero's local database.
//!
//! Main types:
//! - [`ZoteroSqliteSearchCommand`] - Grouped-router command for `SQLite` search
//!   actions

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, mcp::json_result};

/// Arguments for the `fulltext` action of `zotero_sqlite_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FulltextSearchArgs {
    /// Free-text query matched against title, creators, DOI, and indexed
    /// full-text content.
    query: String,
    /// Maximum number of results to return (default: 20).
    limit: Option<usize>,
}

/// Arguments for the `notes_annotations` action of `zotero_sqlite_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchNotesAnnotationsArgs {
    /// Free-text query matched against note body and annotation text/comment.
    query: String,
    /// Maximum number of results to return (default: 20).
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Search commands dispatched by the `zotero_sqlite_search` MCP tool router.
pub(crate) enum ZoteroSqliteSearchCommand {
    /// Full-text search against the Zotero `SQLite` database.
    Fulltext(FulltextSearchArgs),
    /// Search notes and annotations by content.
    NotesAnnotations(SearchNotesAnnotationsArgs),
}

#[tool_router(router = sqlite_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_sqlite_search",
        description = "Grouped local SQLite search router. action: fulltext, \
                       notes_annotations",
        annotations(
            title = "Search Zotero SQLite Database",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_sqlite_search(
        &self,
        Parameters(args): Parameters<ZoteroSqliteSearchCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroSqliteSearchCommand::Fulltext(args) => {
                self.zotero_fulltext_search_impl(args).await
            }
            ZoteroSqliteSearchCommand::NotesAnnotations(args) => {
                self.zotero_search_notes_annotations_impl(args).await
            }
        }
    }
}

impl ZoteroMcpServer {
    /// Handles local full-text search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_fulltext_search_impl(
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
    async fn zotero_search_notes_annotations_impl(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ZoteroMcpServer, mcp::zotero::fixtures::*};

    mod sqlite_tools {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::zotero::test_sqlite::seed_zotero_db as seed_db;

        #[tokio::test]
        async fn fulltext_tool_returns_gate_error_when_disabled() {
            let mut state = zotero_state(String::new());
            state.sqlite_access = false;
            let server = ZoteroMcpServer::new(state.clone());
            let res = server
                .zotero_fulltext_search_impl(FulltextSearchArgs {
                    query: "borrow".to_owned(),
                    limit: Some(10),
                })
                .await
                .unwrap();
            let text = tool_text(&res);
            assert!(text.contains("ZOTERO_SQLITE_ACCESS"));
        }

        #[tokio::test]
        async fn fulltext_tool_returns_hits_when_enabled() {
            let dir = tempfile::tempdir().unwrap();
            let db_path = dir.path().join("zotero.sqlite");
            seed_db(&db_path).await.unwrap();

            let mut state = zotero_state(String::new());
            state.sqlite_access = true;
            state.zotero_db_path = Some(db_path);
            let server = ZoteroMcpServer::new(state);
            let res = server
                .zotero_fulltext_search_impl(FulltextSearchArgs {
                    query: "borrow checker".to_owned(),
                    limit: Some(10),
                })
                .await
                .unwrap();
            let text = tool_text(&res);
            assert!(text.contains("Rust in Action"));
        }

        #[tokio::test]
        async fn fulltext_tool_uses_state_db_path_without_env_var() {
            let dir = tempfile::tempdir().unwrap();
            let db_path = dir.path().join("zotero.sqlite");
            seed_db(&db_path).await.unwrap();
            let previous = std::env::var_os("ZOTERO_DB_PATH");
            std::env::remove_var("ZOTERO_DB_PATH");

            let mut state = zotero_state(String::new());
            state.sqlite_access = true;
            state.zotero_db_path = Some(db_path);
            let server = ZoteroMcpServer::new(state);
            let res = server
                .zotero_fulltext_search_impl(FulltextSearchArgs {
                    query: "borrow checker".to_owned(),
                    limit: Some(10),
                })
                .await
                .unwrap();

            if let Some(value) = previous {
                std::env::set_var("ZOTERO_DB_PATH", value);
            }
            let text = tool_text(&res);
            assert_eq!(res.is_error, Some(false));
            assert!(text.contains("Rust in Action"));
        }
    }
}
