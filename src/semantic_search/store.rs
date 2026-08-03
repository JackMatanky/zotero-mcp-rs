//! Owns the writable side-car `SQLite` database (`embeddings.sqlite`) storing
//! chunk text and embedding BLOBs, independent of Zotero's own database.

use std::{path::Path, str::FromStr, time::Duration};

use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};

use crate::{
    errors::ZoteroMcpError,
    semantic_search::embedding::{decode_embedding, encode_embedding},
};

/// One stored chunk, decoded, ready for a cosine scan.
#[derive(Clone, Debug)]
pub(crate) struct StoredChunk {
    pub(crate) item_key: String,
    pub(crate) title: Option<String>,
    pub(crate) chunk_index: i64,
    pub(crate) chunk_text: String,
    pub(crate) embedding: Vec<f32>,
}

/// A chunk to insert, with its already-normalized embedding.
pub(crate) struct NewChunk {
    pub(crate) chunk_index: i64,
    pub(crate) chunk_text: String,
    pub(crate) embedding: Vec<f32>,
}

/// Aggregate stats for the `status` action of `zotero_semantic_search`.
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct SemanticIndexStats {
    pub(crate) indexed_items: i64,
    pub(crate) indexed_chunks: i64,
}

/// Writable handle to the semantic search side-car database.
#[derive(Clone, Debug)]
pub(crate) struct SemanticIndex {
    pool: SqlitePool,
}

impl SemanticIndex {
    /// Opens (creating if missing, including parent directories) the `SQLite`
    /// database at `db_path`, in WAL mode with foreign keys enabled, and
    /// ensures the schema exists.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Io`] if the parent directory cannot be created
    /// - [`ZoteroMcpError::Sqlite`] if the database cannot be opened or the
    ///   schema cannot be created
    pub(crate) async fn open(db_path: &Path) -> Result<Self, ZoteroMcpError> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            db_path.display()
        ))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePool::connect_with(opts).await?;
        let store = Self {
            pool,
        };
        store.create_schema().await?;
        Ok(store)
    }

    async fn create_schema(&self) -> Result<(), ZoteroMcpError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS items (
                item_pk INTEGER PRIMARY KEY AUTOINCREMENT,
                item_key TEXT NOT NULL UNIQUE,
                title TEXT,
                date_modified TEXT,
                indexed_at INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS chunks (
                item_pk INTEGER NOT NULL REFERENCES items(item_pk) ON DELETE \
             CASCADE,
                chunk_index INTEGER NOT NULL,
                chunk_text TEXT NOT NULL,
                embedding BLOB NOT NULL,
                PRIMARY KEY (item_pk, chunk_index)
            )",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_chunks_item_pk ON chunks(item_pk)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Returns the stored `date_modified` for `item_key`, or [`None`] if the
    /// item is not indexed.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] on query failure
    pub(crate) async fn stored_date_modified(
        &self,
        item_key: &str,
    ) -> Result<Option<String>, ZoteroMcpError> {
        let row =
            sqlx::query("SELECT date_modified FROM items WHERE item_key = ?")
                .bind(item_key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|r| {
            r.try_get::<Option<String>, _>("date_modified").ok().flatten()
        }))
    }

    /// Replaces all chunks for `item_key` with `chunks` in one transaction:
    /// upserts the `items` row, deletes any existing `chunks` rows for it
    /// (`ON DELETE CASCADE` via re-inserting the `items` row would also
    /// work, but an explicit delete is clearer here since we UPDATE, not
    /// replace, the `items` row), then inserts `chunks`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] on query or transaction failure
    pub(crate) async fn upsert_item(
        &self,
        item_key: &str,
        title: Option<&str>,
        date_modified: Option<&str>,
        chunks: &[NewChunk],
    ) -> Result<(), ZoteroMcpError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO items (item_key, title, date_modified, indexed_at)
             VALUES (?, ?, ?, strftime('%s','now'))
             ON CONFLICT(item_key) DO UPDATE SET
                title = excluded.title,
                date_modified = excluded.date_modified,
                indexed_at = excluded.indexed_at",
        )
        .bind(item_key)
        .bind(title)
        .bind(date_modified)
        .execute(&mut *tx)
        .await?;
        let item_pk: i64 =
            sqlx::query("SELECT item_pk FROM items WHERE item_key = ?")
                .bind(item_key)
                .fetch_one(&mut *tx)
                .await?
                .try_get("item_pk")?;
        sqlx::query("DELETE FROM chunks WHERE item_pk = ?")
            .bind(item_pk)
            .execute(&mut *tx)
            .await?;
        for chunk in chunks {
            sqlx::query(
                "INSERT INTO chunks (item_pk, chunk_index, chunk_text, \
                 embedding)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(item_pk)
            .bind(chunk.chunk_index)
            .bind(&chunk.chunk_text)
            .bind(encode_embedding(&chunk.embedding))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Deletes `item_key` and its chunks (cascades via foreign key).
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] on query failure
    pub(crate) async fn delete_item(
        &self,
        item_key: &str,
    ) -> Result<(), ZoteroMcpError> {
        sqlx::query("DELETE FROM items WHERE item_key = ?")
            .bind(item_key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Returns every currently-indexed item key.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] on query failure
    pub(crate) async fn all_item_keys(
        &self,
    ) -> Result<Vec<String>, ZoteroMcpError> {
        let rows = sqlx::query("SELECT item_key FROM items")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(|r| Ok(r.try_get("item_key")?)).collect()
    }

    /// Loads every stored chunk, decoded and ready for a cosine scan.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] on query failure
    /// - [`ZoteroMcpError::Embedding`] if a stored embedding BLOB is corrupt
    pub(crate) async fn load_all_chunks(
        &self,
    ) -> Result<Vec<StoredChunk>, ZoteroMcpError> {
        let rows = sqlx::query(
            "SELECT i.item_key, i.title, c.chunk_index, c.chunk_text, \
             c.embedding
             FROM chunks c JOIN items i ON i.item_pk = c.item_pk",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut chunks = Vec::with_capacity(rows.len());
        for row in rows {
            let embedding_bytes: Vec<u8> = row.try_get("embedding")?;
            chunks.push(StoredChunk {
                item_key: row.try_get("item_key")?,
                title: row.try_get("title")?,
                chunk_index: row.try_get("chunk_index")?,
                chunk_text: row.try_get("chunk_text")?,
                embedding: decode_embedding(&embedding_bytes)?,
            });
        }
        Ok(chunks)
    }

    /// Returns aggregate item/chunk counts.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] on query failure
    pub(crate) async fn stats(
        &self,
    ) -> Result<SemanticIndexStats, ZoteroMcpError> {
        let indexed_items: i64 = sqlx::query("SELECT COUNT(*) AS c FROM items")
            .fetch_one(&self.pool)
            .await?
            .try_get("c")?;
        let indexed_chunks: i64 =
            sqlx::query("SELECT COUNT(*) AS c FROM chunks")
                .fetch_one(&self.pool)
                .await?
                .try_get("c")?;
        Ok(SemanticIndexStats {
            indexed_items,
            indexed_chunks,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn chunk(idx: i64, text: &str, value: f32) -> NewChunk {
        NewChunk {
            chunk_index: idx,
            chunk_text: text.to_owned(),
            embedding: vec![value, value, value],
        }
    }

    #[tokio::test]
    async fn upsert_then_load_round_trips_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        index
            .upsert_item("ITEM1", Some("Title 1"), Some("2024-01-01"), &[
                chunk(0, "first chunk", 0.5),
                chunk(1, "second chunk", -0.5),
            ])
            .await
            .unwrap();

        let mut loaded = index.load_all_chunks().await.unwrap();
        loaded.sort_by_key(|c| c.chunk_index);
        assert_eq!(loaded.len(), 2);
        let first = loaded.first().unwrap();
        assert_eq!(first.item_key, "ITEM1");
        assert_eq!(first.title, Some("Title 1".to_owned()));
        assert_eq!(first.chunk_text, "first chunk");
        assert_eq!(first.embedding, vec![0.5, 0.5, 0.5]);
        let second = loaded.get(1).unwrap();
        assert_eq!(second.chunk_text, "second chunk");
        assert_eq!(second.embedding, vec![-0.5, -0.5, -0.5]);
    }

    #[tokio::test]
    async fn re_upsert_replaces_rather_than_duplicates_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        index
            .upsert_item("ITEM1", Some("Title"), Some("v1"), &[chunk(
                0, "a", 1.0,
            )])
            .await
            .unwrap();
        index
            .upsert_item("ITEM1", Some("Title"), Some("v2"), &[
                chunk(0, "b", 2.0),
                chunk(1, "c", 3.0),
            ])
            .await
            .unwrap();

        let loaded = index.load_all_chunks().await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(
            index.stored_date_modified("ITEM1").await.unwrap(),
            Some("v2".to_owned())
        );
    }

    #[tokio::test]
    async fn delete_item_removes_item_and_its_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        index
            .upsert_item("ITEM1", None, None, &[chunk(0, "a", 1.0)])
            .await
            .unwrap();
        index
            .upsert_item("ITEM2", None, None, &[chunk(0, "b", 2.0)])
            .await
            .unwrap();

        index.delete_item("ITEM1").await.unwrap();

        assert_eq!(index.all_item_keys().await.unwrap(), vec![
            "ITEM2".to_owned()
        ]);
        let remaining = index.load_all_chunks().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining.first().unwrap().item_key, "ITEM2");
    }

    #[tokio::test]
    async fn stats_reflects_inserts_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        index
            .upsert_item("ITEM1", None, None, &[
                chunk(0, "a", 1.0),
                chunk(1, "b", 2.0),
            ])
            .await
            .unwrap();
        let stats = index.stats().await.unwrap();
        assert_eq!(stats.indexed_items, 1);
        assert_eq!(stats.indexed_chunks, 2);

        index.delete_item("ITEM1").await.unwrap();
        let stats_after_delete = index.stats().await.unwrap();
        assert_eq!(stats_after_delete.indexed_items, 0);
        assert_eq!(stats_after_delete.indexed_chunks, 0);
    }
}
