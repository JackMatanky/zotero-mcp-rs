//! MCP tool handlers and argument models for local Zotero `SQLite` search.
//!
//! Covers `zotero_sqlite_search` grouped-router actions (gated behind
//! `ZOTERO_SQLITE_ACCESS=1`): full-text search and note/annotation search
//! against Zotero's local database.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, mcp::json_result};

/// Arguments for `zotero_fulltext_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FulltextSearchArgs {
    /// Free-text query matched against title, creators, DOI, and indexed
    /// fulltext.
    query: String,
    /// Maximum number of results to return (default: 20).
    limit: Option<usize>,
}

/// Arguments for `zotero_search_notes_annotations`.
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
pub(crate) enum ZoteroSqliteSearchCommand {
    Fulltext(FulltextSearchArgs),
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

    #[tool(
        name = "zotero_fulltext_search",
        description = "Search Zotero's local sqlite database for full-text \
                       matches across titles, creators, and indexed PDF text \
                       (requires ZOTERO_SQLITE_ACCESS=1)",
        annotations(
            title = "Full-Text Search (SQLite)",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_fulltext_search(
        &self,
        Parameters(args): Parameters<FulltextSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_fulltext_search_impl(args).await
    }

    #[tool(
        name = "zotero_search_notes_annotations",
        description = "Search Zotero's local sqlite database for note and PDF \
                       annotation text (requires ZOTERO_SQLITE_ACCESS=1)",
        annotations(
            title = "Search Notes and Annotations",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_notes_annotations(
        &self,
        Parameters(args): Parameters<SearchNotesAnnotationsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_notes_annotations_impl(args).await
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
        use std::{path::Path, str::FromStr};

        use pretty_assertions::assert_eq;
        use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

        use super::*;

        #[expect(
            clippy::too_many_lines,
            reason = "seeds a realistic Zotero schema across many tables"
        )]
        async fn seed_db(path: &Path) {
            let opts = SqliteConnectOptions::from_str(&format!(
                "sqlite://{}",
                path.display()
            ))
            .unwrap()
            .create_if_missing(true);
            let pool = SqlitePool::connect_with(opts).await.unwrap();
            sqlx::query(
                "CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, \
                 typeName TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE items (itemID INTEGER PRIMARY KEY, key TEXT, \
                 itemTypeID INTEGER, dateAdded TEXT, dateModified TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE fields (fieldID INTEGER PRIMARY KEY, fieldName \
                 TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemData (itemID INTEGER, fieldID INTEGER, \
                 valueID INTEGER)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, \
                 value TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, \
                 firstName TEXT, lastName TEXT, fieldMode INT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemCreators (itemID INTEGER, creatorID INTEGER)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("CREATE TABLE deletedItems (itemID INTEGER)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE fulltextWords (wordID INTEGER PRIMARY KEY, word \
                 TEXT UNIQUE)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE fulltextItemWords (wordID INT, itemID INT, \
                 PRIMARY KEY (wordID, itemID))",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemNotes (itemID INTEGER, parentItemID \
                 INTEGER, note TEXT, title TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemAnnotations (itemID INTEGER, parentItemID \
                 INTEGER, text TEXT, comment TEXT, type INTEGER, color TEXT, \
                 pageLabel TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE itemAttachments (itemID INTEGER, parentItemID \
                 INTEGER, path TEXT, contentType TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO fields (fieldID, fieldName) VALUES (1, 'title'), \
                 (16, 'extra'), (7, 'DOI')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO itemTypes (itemTypeID, typeName) VALUES (1, \
                 'journalArticle'), (2, 'note'), (3, 'attachment')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
                 dateModified) VALUES (1, 'K00001', 1, '2024-01-01', \
                 '2024-02-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO itemData (itemID, fieldID, valueID) VALUES (1, \
                 1, 100), (1, 7, 101)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO itemDataValues (valueID, value) VALUES (100, \
                 'Rust in Action'), (101, '10.1000/rust')",
            )
            .execute(&pool)
            .await
            .unwrap();
            // attachment child (item 3) carries the indexed fulltext words
            sqlx::query(
                "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
                 dateModified) VALUES (3, 'A00001', 3, '2024-01-02', \
                 '2024-02-02')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO itemAttachments (itemID, parentItemID, path, \
                 contentType) VALUES (3, 1, 'storage:K00001.pdf', \
                 'application/pdf')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO fulltextWords (wordID, word) VALUES (1, 'the'), \
                 (2, 'borrow'), (3, 'checker'), (4, 'ensures'), (5, \
                 'memory'), (6, 'safety')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO fulltextItemWords (wordID, itemID) VALUES (1, \
                 3), (2, 3), (3, 3), (4, 3), (5, 3), (6, 3)",
            )
            .execute(&pool)
            .await
            .unwrap();
            pool.close().await;
        }

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
            seed_db(&db_path).await;

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
            seed_db(&db_path).await;
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
