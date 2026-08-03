//! Whole-library scan that builds/refreshes the semantic index.

use std::sync::Arc;

use serde::Serialize;

use crate::{
    errors::ZoteroMcpError,
    semantic_search::{
        EmbeddingProvider, MAX_CHUNK_CHARS, MAX_INDEXABLE_CHARS,
        chunking::chunk_text,
        embedding::normalize,
        store::{NewChunk, SemanticIndex},
    },
    zotero::{ItemType, ZoteroClient, ZoteroItem},
};

/// Per-item outcome of the library scan, used to bump exactly one
/// `IndexReport` counter per item.
enum IndexOutcome {
    Indexed,
    SkippedUnchanged,
    SkippedEmpty,
}

/// Result of the `index` action of `zotero_semantic_search`.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct IndexReport {
    pub(crate) items_scanned: usize,
    pub(crate) items_indexed: usize,
    pub(crate) items_skipped_unchanged: usize,
    pub(crate) items_skipped_empty: usize,
    pub(crate) items_deleted: usize,
    pub(crate) chunks_written: usize,
}

/// Scans the whole library, (re)indexing items whose `dateModified` changed
/// since the last index (or all items if `force` is `true`), and deletes
/// index entries for items no longer present in the library.
///
/// Text source per item: title + `abstractNote` (from item metadata) +
/// Zotero's own indexed fulltext content of the first attachment child that
/// has any (`ZoteroClient::get_item_fulltext`) — no local PDF extraction;
/// this only surfaces content Zotero itself has already indexed.
///
/// # Errors
///
/// - [`ZoteroMcpError::LocalApi`] / [`ZoteroMcpError::Network`] /
///   [`ZoteroMcpError::Json`] if the Zotero Local API is unreachable or returns
///   malformed data
/// - [`ZoteroMcpError::Sqlite`] if the index database fails
/// - [`ZoteroMcpError::Embedding`] if embedding generation fails
pub(crate) async fn index_library(
    client: &ZoteroClient<'_>,
    index: &SemanticIndex,
    provider: &Arc<dyn EmbeddingProvider>,
    force: bool,
) -> Result<IndexReport, ZoteroMcpError> {
    let all_items: Vec<ZoteroItem> = client.get_all_items().await?;

    let mut report = IndexReport::default();
    let mut current_keys = std::collections::HashSet::new();

    for item in &all_items {
        if item.data.deleted || !item.data.item_type.is_indexable() {
            continue;
        }
        report.items_scanned = report.items_scanned.saturating_add(1);
        current_keys.insert(item.key.clone());

        let outcome = if !force && is_unchanged(index, item).await? {
            IndexOutcome::SkippedUnchanged
        } else {
            index_one_item(client, index, provider, item, &mut report).await?
        };
        match outcome {
            IndexOutcome::Indexed => {
                report.items_indexed = report.items_indexed.saturating_add(1);
            }
            IndexOutcome::SkippedUnchanged => {
                report.items_skipped_unchanged =
                    report.items_skipped_unchanged.saturating_add(1);
            }
            IndexOutcome::SkippedEmpty => {
                report.items_skipped_empty =
                    report.items_skipped_empty.saturating_add(1);
            }
        }
    }

    for stale_key in index.all_item_keys().await? {
        if !current_keys.contains(&stale_key) {
            index.delete_item(&stale_key).await?;
            report.items_deleted = report.items_deleted.saturating_add(1);
        }
    }

    Ok(report)
}

/// Returns `true` if `item`'s stored `dateModified` already matches its
/// current metadata (so it can be skipped when not `force`-reindexing).
async fn is_unchanged(
    index: &SemanticIndex,
    item: &ZoteroItem,
) -> Result<bool, ZoteroMcpError> {
    let stored = index.stored_date_modified(&item.key).await?;
    Ok(stored.is_some()
        && stored.as_deref() == item.data.date_modified.as_deref())
}

/// Assembles, chunks, embeds, and stores one item's text, returning the
/// [`IndexOutcome`] so the caller bumps exactly one counter.
async fn index_one_item(
    client: &ZoteroClient<'_>,
    index: &SemanticIndex,
    provider: &Arc<dyn EmbeddingProvider>,
    item: &ZoteroItem,
    report: &mut IndexReport,
) -> Result<IndexOutcome, ZoteroMcpError> {
    let text = assemble_item_text(client, item).await?;
    let text = if text.chars().count() > MAX_INDEXABLE_CHARS {
        text.chars().take(MAX_INDEXABLE_CHARS).collect()
    } else {
        text
    };
    if text.trim().is_empty() {
        return Ok(IndexOutcome::SkippedEmpty);
    }

    let pieces = chunk_text(&text, MAX_CHUNK_CHARS);
    if pieces.is_empty() {
        return Ok(IndexOutcome::SkippedEmpty);
    }
    let mut vectors = provider.embed(&pieces)?;
    for vector in &mut vectors {
        normalize(vector);
    }
    let new_chunks: Vec<NewChunk> = pieces
        .into_iter()
        .zip(vectors)
        .enumerate()
        .map(|(idx, (chunk_text, embedding))| NewChunk {
            chunk_index: i64::try_from(idx).unwrap_or(i64::MAX),
            chunk_text,
            embedding,
        })
        .collect();
    report.chunks_written =
        report.chunks_written.saturating_add(new_chunks.len());
    index
        .upsert_item(
            &item.key,
            item.data.title.as_deref(),
            item.data.date_modified.as_deref(),
            &new_chunks,
        )
        .await?;
    Ok(IndexOutcome::Indexed)
}

/// Assembles the text to index for `item`: title, then abstract, then the
/// first non-empty Zotero-indexed fulltext among the item's attachment
/// children, each on its own paragraph (`\n\n`-joined) so `chunk_text` treats
/// them as separate paragraphs.
async fn assemble_item_text(
    client: &ZoteroClient<'_>,
    item: &ZoteroItem,
) -> Result<String, ZoteroMcpError> {
    let mut parts = Vec::new();
    if let Some(title) = &item.data.title {
        if !title.trim().is_empty() {
            parts.push(title.clone());
        }
    }
    if let Some(abstract_note) = &item.data.abstract_note {
        if !abstract_note.trim().is_empty() {
            parts.push(abstract_note.clone());
        }
    }
    let children =
        client.get_item_children(&item.key).await.unwrap_or_default();
    for child in &children {
        if child.data.item_type != ItemType::Attachment {
            continue;
        }
        if let Ok(text) = client.get_item_fulltext(&child.key).await {
            if !text.trim().is_empty() {
                parts.push(text);
                break;
            }
        }
    }
    Ok(parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        state::AppState,
        zotero::test_http::{MockServer, http_response},
    };

    /// Deterministic test [`EmbeddingProvider`]: every text embeds to the
    /// same fixed vector. `index_library`'s tests only assert on
    /// [`IndexReport`] counts, never on similarity scores, so vector
    /// content is irrelevant — no ONNX/network involved.
    #[derive(Debug)]
    struct FakeProvider;

    impl EmbeddingProvider for FakeProvider {
        fn embed(
            &self,
            texts: &[String],
        ) -> Result<Vec<Vec<f32>>, ZoteroMcpError> {
            Ok(texts.iter().map(|_| vec![1.0, 0.0, 0.0, 0.0]).collect())
        }
    }

    fn test_state(zotero_api_url: String) -> AppState {
        AppState {
            zotero_api_url,
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: String::new(),
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: false,
            ..AppState::from_env()
        }
    }

    fn item_json(
        key: &str,
        title: &str,
        abstract_note: &str,
        date_modified: &str,
    ) -> String {
        format!(
            r#"{{"key":"{key}","version":1,"data":{{"key":"{key}","version":1,"itemType":"journalArticle","title":"{title}","abstractNote":"{abstract_note}","dateModified":"{date_modified}"}}}}"#
        )
    }

    #[tokio::test]
    async fn indexes_new_items_with_title_and_abstract() {
        let items = format!(
            "[{}]",
            item_json(
                "ITEM1",
                "A Paper",
                "An abstract about testing.",
                "2024-01-01"
            )
        );
        // Requests: items page, then empty children page for ITEM1.
        let server = MockServer::new(vec![
            http_response("200 OK", &items),
            http_response("200 OK", "[]"),
        ]);
        let state = test_state(server.url().to_owned());
        let client = ZoteroClient::new(&state);
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(FakeProvider);

        let report =
            index_library(&client, &index, &provider, false).await.unwrap();

        assert_eq!(report.items_scanned, 1);
        assert_eq!(report.items_indexed, 1);
        assert_eq!(report.items_skipped_unchanged, 0);
        assert!(report.chunks_written >= 1);

        let index_stats = index.stats().await.unwrap();
        assert_eq!(index_stats.indexed_items, 1);
    }

    #[tokio::test]
    async fn skips_unchanged_items_on_second_run() {
        let items = format!(
            "[{}]",
            item_json(
                "ITEM1",
                "A Paper",
                "An abstract about testing.",
                "2024-01-01"
            )
        );
        let server = MockServer::new(vec![
            http_response("200 OK", &items),
            http_response("200 OK", "[]"),
            http_response("200 OK", &items),
        ]);
        let state = test_state(server.url().to_owned());
        let client = ZoteroClient::new(&state);
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(FakeProvider);

        index_library(&client, &index, &provider, false).await.unwrap();
        let second =
            index_library(&client, &index, &provider, false).await.unwrap();

        assert_eq!(second.items_skipped_unchanged, 1);
        assert_eq!(second.items_indexed, 0);
    }

    #[tokio::test]
    async fn deletes_items_no_longer_in_library() {
        let items = format!(
            "[{}]",
            item_json(
                "ITEM1",
                "A Paper",
                "An abstract about testing.",
                "2024-01-01"
            )
        );
        let server = MockServer::new(vec![
            http_response("200 OK", &items),
            http_response("200 OK", "[]"),
            http_response("200 OK", "[]"),
        ]);
        let state = test_state(server.url().to_owned());
        let client = ZoteroClient::new(&state);
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(FakeProvider);

        index_library(&client, &index, &provider, false).await.unwrap();
        let second =
            index_library(&client, &index, &provider, false).await.unwrap();

        assert_eq!(second.items_deleted, 1);
        let index_stats = index.stats().await.unwrap();
        assert_eq!(index_stats.indexed_items, 0);
    }

    #[tokio::test]
    async fn skips_items_with_no_indexable_text() {
        let items = "[{\"key\":\"ITEM1\",\"version\":1,\"data\":{\"key\":\"\
                     ITEM1\",\"version\":1,\"itemType\":\"journalArticle\"}}]";
        let server = MockServer::new(vec![
            http_response("200 OK", items),
            http_response("200 OK", "[]"),
        ]);
        let state = test_state(server.url().to_owned());
        let client = ZoteroClient::new(&state);
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(FakeProvider);

        let report =
            index_library(&client, &index, &provider, false).await.unwrap();

        assert_eq!(report.items_skipped_empty, 1);
        assert_eq!(report.items_indexed, 0);
    }

    #[test]
    fn index_report_defaults_to_all_zeroes() {
        let report = IndexReport::default();
        assert_eq!(report.items_scanned, 0);
        assert_eq!(report.items_indexed, 0);
        assert_eq!(report.items_skipped_unchanged, 0);
        assert_eq!(report.items_skipped_empty, 0);
        assert_eq!(report.items_deleted, 0);
        assert_eq!(report.chunks_written, 0);
    }
}
