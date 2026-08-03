//! MCP tool handler and argument models for local semantic search.
//!
//! Covers the `zotero_semantic_search` grouped-router actions (gated behind
//! `ZOTERO_SEMANTIC_SEARCH=1`): search, index, and status.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, mcp::json_result, zotero::ZoteroClient};

/// Arguments for the `search` action of `zotero_semantic_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SemanticSearchArgs {
    /// Natural-language query embedded and compared against indexed content.
    query: String,
    /// Maximum number of results to return (default: 20).
    limit: Option<usize>,
    /// Minimum cosine similarity for a result to be included (default: 0.3).
    min_similarity: Option<f32>,
}

/// Arguments for the `index` action of `zotero_semantic_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SemanticIndexArgs {
    /// Re-index every item regardless of stored `dateModified` (default:
    /// false — only changed/new items are re-embedded).
    force: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroSemanticSearchCommand {
    Search(SemanticSearchArgs),
    Index(SemanticIndexArgs),
    Status,
}

#[tool_router(router = semantic_search_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_semantic_search",
        description = "Grouped local semantic search router. action: search, \
                       index, status",
        annotations(
            title = "Semantic Search",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_semantic_search(
        &self,
        Parameters(args): Parameters<ZoteroSemanticSearchCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroSemanticSearchCommand::Search(args) => {
                self.semantic_search_impl(args).await
            }
            ZoteroSemanticSearchCommand::Index(args) => {
                self.semantic_index_impl(args).await
            }
            ZoteroSemanticSearchCommand::Status => {
                self.semantic_status_impl().await
            }
        }
    }
}

impl ZoteroMcpServer {
    async fn semantic_search_impl(
        &self,
        args: SemanticSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let min_similarity = args
            .min_similarity
            .unwrap_or(crate::semantic_search::DEFAULT_MIN_SIMILARITY);
        let state = &self.state;
        let result = async {
            let index = state.semantic_index().await?;
            let provider = state.embedding_provider().await?;
            let all_chunks = index.load_all_chunks().await?;
            crate::semantic_search::search_library(
                &provider,
                &all_chunks,
                &args.query,
                limit,
                min_similarity,
            )
            .await
        }
        .await;
        Ok(json_result(result))
    }

    async fn semantic_index_impl(
        &self,
        args: SemanticIndexArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let force = args.force.unwrap_or(false);
        let state = &self.state;
        let result = async {
            let index = state.semantic_index().await?;
            let provider = state.embedding_provider().await?;
            let client = ZoteroClient::new(state);
            crate::semantic_search::index_library(
                &client, index, &provider, force,
            )
            .await
        }
        .await;
        Ok(json_result(result))
    }

    async fn semantic_status_impl(
        &self,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let state = &self.state;
        let result = async {
            let index = state.semantic_index().await?;
            index.stats().await
        }
        .await;
        Ok(json_result(result))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::OnceCell;

    use super::*;
    use crate::{
        semantic_search::{Embedding, EmbeddingProvider, SemanticIndex},
        state::AppState,
        zotero::{
            ItemKey,
            test_http::{MockServer, http_response},
        },
    };

    /// Deterministic test [`EmbeddingProvider`]: every text embeds to the
    /// fixed vector supplied at construction.
    #[derive(Debug)]
    struct FixedProvider {
        vector: Vec<f32>,
    }

    impl EmbeddingProvider for FixedProvider {
        fn embed(
            &self,
            texts: &[String],
        ) -> Result<Vec<Embedding>, crate::errors::ZoteroMcpError> {
            Ok(texts
                .iter()
                .map(|_| Embedding::from(self.vector.clone()))
                .collect())
        }
    }

    fn new_chunk(
        chunk_index: i64,
        text: &str,
        value: f32,
    ) -> crate::semantic_search::NewChunk {
        crate::semantic_search::NewChunk {
            chunk_index,
            chunk_text: text.to_owned(),
            embedding: Embedding::from(vec![value]),
        }
    }

    /// Builds an [`AppState`] fixture with semantic search enabled, pointed
    /// at a private tempdir index db, and with `embedding_provider`
    /// pre-populated (bypassing `FastEmbedProvider::load`'s real ONNX model
    /// load/download entirely).
    fn semantic_state(
        zotero_api_url: String,
        db_path: std::path::PathBuf,
        provider: Arc<dyn EmbeddingProvider>,
    ) -> AppState {
        AppState {
            zotero_api_url,
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: String::new(),
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: false,
            semantic_search_enabled: true,
            semantic_db_path: Some(db_path),
            embedding_provider: Arc::new(OnceCell::new_with(Some(provider))),
            ..AppState::from_env()
        }
    }

    fn tool_text(res: &CallToolResult) -> String {
        res.content
            .first()
            .and_then(|c| c.as_text())
            .map_or_else(String::new, |t| t.text.clone())
    }

    #[tokio::test]
    async fn denies_semantic_search_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = semantic_state(
            String::new(),
            dir.path().join("embeddings.sqlite"),
            Arc::new(FixedProvider {
                vector: vec![1.0],
            }),
        );
        state.semantic_search_enabled = false;
        let server = ZoteroMcpServer::new(state);

        let res = server.semantic_status_impl().await.unwrap();

        let text = tool_text(&res);
        assert!(text.contains("ZOTERO_SEMANTIC_SEARCH"));
    }

    #[tokio::test]
    async fn status_action_reports_indexed_counts() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("embeddings.sqlite");
        let index = SemanticIndex::open(&db_path).await.unwrap();
        index
            .upsert_item(
                &ItemKey::from("ITEM1"),
                Some("Title"),
                Some("2024-01-01"),
                &[new_chunk(0, "text", 1.0)],
            )
            .await
            .unwrap();
        let state = semantic_state(
            String::new(),
            db_path,
            Arc::new(FixedProvider {
                vector: vec![1.0],
            }),
        );
        let server = ZoteroMcpServer::new(state);

        let res = server.semantic_status_impl().await.unwrap();

        let text = tool_text(&res);
        assert!(text.contains("\"indexed_items\": 1"));
        assert!(text.contains("\"indexed_chunks\": 1"));
    }

    #[tokio::test]
    async fn search_action_returns_matching_hit() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("embeddings.sqlite");
        let index = SemanticIndex::open(&db_path).await.unwrap();
        index
            .upsert_item(
                &ItemKey::from("ITEM1"),
                Some("Matching Title"),
                Some("2024-01-01"),
                &[new_chunk(0, "relevant text", 1.0)],
            )
            .await
            .unwrap();
        let state = semantic_state(
            String::new(),
            db_path,
            Arc::new(FixedProvider {
                vector: vec![1.0],
            }),
        );
        let server = ZoteroMcpServer::new(state);

        let res = server
            .semantic_search_impl(SemanticSearchArgs {
                query: "anything".to_owned(),
                limit: Some(5),
                min_similarity: Some(0.5),
            })
            .await
            .unwrap();

        let text = tool_text(&res);
        assert!(text.contains("ITEM1"));
        assert!(text.contains("Matching Title"));
    }

    #[tokio::test]
    async fn index_action_indexes_items_from_mocked_local_api() {
        let items = r#"[{"key":"ITEM1","version":1,"data":{"key":"ITEM1","version":1,"itemType":"journalArticle","title":"A Paper","abstractNote":"An abstract."}}]"#;
        let server_http = MockServer::new(vec![
            http_response("200 OK", items),
            http_response("200 OK", "[]"),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let state = semantic_state(
            server_http.url().to_owned(),
            dir.path().join("embeddings.sqlite"),
            Arc::new(FixedProvider {
                vector: vec![1.0],
            }),
        );
        let server = ZoteroMcpServer::new(state);

        let res = server
            .semantic_index_impl(SemanticIndexArgs {
                force: Some(false),
            })
            .await
            .unwrap();

        let text = tool_text(&res);
        assert!(text.contains("\"items_indexed\": 1"));
    }
}
