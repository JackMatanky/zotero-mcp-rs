//! Read-only access to Zotero's local `zotero.sqlite` database.
//!
//! Locates the database using the same order as Zotero desktop integrations:
//!
//! 1. `ZOTERO_DB_PATH`
//! 2. The `prefs.js` `dataDir` preference
//! 3. The per-user default Zotero data directory
//!
//! The database is opened with `SQLite` `immutable=1` and read-only flags so a
//! running Zotero instance does not block reads. Queries inspect Zotero's
//! `itemData`, `fulltextWords`, `itemNotes`, and `itemAnnotations` tables
//! directly.
//!
//! Every method is gated at the MCP tool layer by
//! [`AppState::check_sqlite_access`](crate::state::AppState::check_sqlite_access).

use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};

use crate::{errors::ZoteroMcpError, zotero::ItemKey};

/// Maximum rows to scan before full-text results are filtered in Rust.
const FULLTEXT_SCAN_CAP: usize = 2000;

/// Opens Zotero's local `SQLite` database in immutable read-only mode.
#[derive(Clone, Debug)]
pub(crate) struct LocalZoteroDb {
    pool: SqlitePool,
}

/// Kind of a local search hit.
#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    PartialEq,
    Deserialize,
    Serialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HitKind {
    /// A note child of a parent item.
    Note,
    /// A PDF annotation.
    Annotation,
}

/// A single full-text search hit.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct FulltextHit {
    pub(crate) key: ItemKey,
    pub(crate) item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) doi: Option<String>,
    pub(crate) creators: String,
    pub(crate) snippet: String,
}

/// A single note or annotation search hit.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct NoteAnnotationHit {
    pub(crate) kind: HitKind,
    pub(crate) key: ItemKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parent_key: Option<ItemKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parent_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) page_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) color: Option<String>,
}

impl LocalZoteroDb {
    /// Opens `path` with `SQLite` `immutable=1` and read-only semantics.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] if `path` cannot be opened read-only
    /// - [`ZoteroMcpError::LocalDb`] if the database is not a Zotero database
    pub(crate) async fn open(path: &Path) -> Result<Self, ZoteroMcpError> {
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            path.display()
        ))?
        .read_only(true)
        .immutable(true)
        .busy_timeout(Duration::from_secs(2));
        let pool = SqlitePool::connect_with(opts).await?;
        let db = Self {
            pool,
        };
        db.probe_schema().await?;
        Ok(db)
    }

    /// Verifies that the opened database contains Zotero's `items` table.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] if the schema probe query fails
    /// - [`ZoteroMcpError::LocalDb`] if the `items` table is missing
    async fn probe_schema(&self) -> Result<(), ZoteroMcpError> {
        let row = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND \
             name='items'",
        )
        .fetch_optional(&self.pool)
        .await?;
        if row.is_none() {
            return Err(ZoteroMcpError::LocalDb(
                "Not a Zotero database: 'items' table not found".to_owned(),
            ));
        }
        Ok(())
    }

    /// Searches the library for `query` across title, DOI, extra, and indexed
    /// fulltext, returning at most `limit` hits.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] if a query or row read fails
    #[expect(
        clippy::too_many_lines,
        reason = "SQL spans are long; mirrors digest query shape"
    )]
    pub(crate) async fn search_fulltext(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FulltextHit>, ZoteroMcpError> {
        let query_lc = query.to_lowercase();
        let query_tokens: Vec<String> =
            query_lc.split_whitespace().map(str::to_owned).collect();
        let result_cap =
            limit.saturating_mul(5).max(limit).min(FULLTEXT_SCAN_CAP);
        let mut token_placeholders = "?,".repeat(query_tokens.len());
        token_placeholders.pop();
        let word_hits_join = if query_tokens.is_empty() {
            String::new()
        } else {
            format!(
                r"
            LEFT JOIN (
                SELECT ia.parentItemID AS itemID,
                       COUNT(DISTINCT fw.word) AS matched_words
                FROM fulltextItemWords fiw
                JOIN fulltextWords fw ON fw.wordID = fiw.wordID
                JOIN itemAttachments ia ON ia.itemID = fiw.itemID
                WHERE ia.parentItemID IS NOT NULL
                  AND fw.word IN ({token_placeholders})
                GROUP BY ia.parentItemID
            ) word_hits ON word_hits.itemID = i.itemID"
            )
        };
        let words_join = if query_tokens.is_empty() {
            String::new()
        } else {
            format!(
                r"
            LEFT JOIN (
                SELECT ia.parentItemID AS itemID,
                       GROUP_CONCAT(fw.word, ' ') AS words
                FROM fulltextItemWords fiw
                JOIN fulltextWords fw ON fw.wordID = fiw.wordID
                JOIN itemAttachments ia ON ia.itemID = fiw.itemID
                WHERE ia.parentItemID IS NOT NULL
                  AND fw.word IN ({token_placeholders})
                GROUP BY ia.parentItemID
            ) words ON words.itemID = i.itemID"
            )
        };
        let words_select = if query_tokens.is_empty() {
            "'' AS words"
        } else {
            "words.words AS words"
        };
        let fulltext_predicate = if query_tokens.is_empty() {
            "0".to_owned()
        } else {
            format!(
                "COALESCE(word_hits.matched_words, 0) = {}",
                query_tokens.len()
            )
        };
        let sql = format!(
            r"
            SELECT i.key, it.typeName AS item_type,
                   title.value AS title, doi.value AS doi,
                   extra.value AS extra,
                   creators.creators AS creators,
                   {words_select}
            FROM items i
            JOIN itemTypes it ON i.itemTypeID = it.itemTypeID
            LEFT JOIN itemData title_data
                ON title_data.itemID = i.itemID AND title_data.fieldID = 1
            LEFT JOIN itemDataValues title
                ON title.valueID = title_data.valueID
            LEFT JOIN fields doi_field
                ON doi_field.fieldName = 'DOI'
            LEFT JOIN itemData doi_data
                ON doi_data.itemID = i.itemID
               AND doi_data.fieldID = doi_field.fieldID
            LEFT JOIN itemDataValues doi
                ON doi.valueID = doi_data.valueID
            LEFT JOIN itemData extra_data
                ON extra_data.itemID = i.itemID AND extra_data.fieldID = 16
            LEFT JOIN itemDataValues extra
                ON extra.valueID = extra_data.valueID
            LEFT JOIN (
                SELECT ic.itemID, GROUP_CONCAT(
                    CASE
                        WHEN c.firstName IS NOT NULL AND c.lastName IS NOT NULL
                        THEN c.lastName || ', ' || c.firstName
                        WHEN c.lastName IS NOT NULL
                        THEN c.lastName
                        ELSE NULL
                    END, '; '
                ) AS creators
                FROM itemCreators ic
                JOIN creators c ON c.creatorID = ic.creatorID
                GROUP BY ic.itemID
            ) creators ON creators.itemID = i.itemID
            {words_join}
            {word_hits_join}
            WHERE it.typeName NOT IN ('attachment', 'note', 'annotation')
              AND i.itemID NOT IN (SELECT itemID FROM deletedItems)
              AND (
                lower(COALESCE(title.value, '')) LIKE ?
                OR lower(COALESCE(doi.value, '')) LIKE ?
                OR lower(COALESCE(extra.value, '')) LIKE ?
                OR lower(COALESCE(creators.creators, '')) LIKE ?
                OR {fulltext_predicate}
              )
            LIMIT ?
            "
        );
        let mut query_builder = sqlx::query(&sql);
        for token in &query_tokens {
            query_builder = query_builder.bind(token);
        }
        for token in &query_tokens {
            query_builder = query_builder.bind(token);
        }
        let pattern = format!("%{query_lc}%");
        query_builder = query_builder
            .bind(pattern.as_str())
            .bind(pattern.as_str())
            .bind(pattern.as_str())
            .bind(pattern.as_str())
            .bind(i64::try_from(result_cap).unwrap_or(2000));
        let rows = query_builder.fetch_all(&self.pool).await?;

        let mut hits = Vec::new();
        for row in rows {
            let key: String = row.try_get("key")?;
            let item_type: String = row.try_get("item_type")?;
            let title: Option<String> = row.try_get("title")?;
            let doi: Option<String> = row.try_get("doi")?;
            let creators: Option<String> = row.try_get("creators")?;
            let words: Option<String> = row.try_get("words")?;
            hits.push(FulltextHit {
                key: ItemKey::from(key),
                item_type,
                title,
                doi,
                creators: creators.unwrap_or_default(),
                snippet: words
                    .as_deref()
                    .map(|w| w.chars().take(400).collect())
                    .unwrap_or_default(),
            });
        }
        hits.truncate(limit);
        Ok(hits)
    }

    /// Searches child notes and PDF annotations for `query`, returning at most
    /// `limit` hits.
    ///
    /// Mirrors the digest's `search_notes_local` and `search_annotations_local`
    /// queries.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Sqlite`] if a query or row read fails
    pub(crate) async fn search_notes_annotations(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<NoteAnnotationHit>, ZoteroMcpError> {
        let query_lc = query.to_lowercase();
        let pattern = format!("%{query}%");
        let fetch_limit = limit.saturating_mul(5).max(limit).min(500);
        let note_rows = sqlx::query(
            r"
            SELECT i.key, n.note, n.title,
                   pi.key AS parentKey, pdv.value AS parentTitle
            FROM itemNotes n
            JOIN items i ON n.itemID = i.itemID
            LEFT JOIN items pi ON n.parentItemID = pi.itemID
            LEFT JOIN itemData pd
                ON pd.itemID = pi.itemID AND pd.fieldID = 1
            LEFT JOIN itemDataValues pdv ON pd.valueID = pdv.valueID
            WHERE n.note LIKE ?
              AND i.itemID NOT IN (SELECT itemID FROM deletedItems)
            LIMIT ?
            ",
        )
        .bind(pattern.as_str())
        .bind(i64::try_from(fetch_limit).unwrap_or(20))
        .fetch_all(&self.pool)
        .await?;

        let ann_rows = sqlx::query(
            r"
            SELECT i.key, ia.text, ia.comment, ia.type, ia.color,
                   ia.pageLabel, att.key AS attachmentKey,
                   gpi.key AS parentKey, gpdv.value AS parentTitle
            FROM itemAnnotations ia
            JOIN items i ON ia.itemID = i.itemID
            LEFT JOIN items att ON ia.parentItemID = att.itemID
            LEFT JOIN itemAttachments iatt ON ia.parentItemID = iatt.itemID
            LEFT JOIN items gpi ON iatt.parentItemID = gpi.itemID
            LEFT JOIN itemData gpd
                ON gpd.itemID = gpi.itemID AND gpd.fieldID = 1
            LEFT JOIN itemDataValues gpdv ON gpd.valueID = gpdv.valueID
            WHERE (ia.text LIKE ? OR ia.comment LIKE ?)
              AND i.itemID NOT IN (SELECT itemID FROM deletedItems)
            LIMIT ?
            ",
        )
        .bind(pattern.as_str())
        .bind(pattern.as_str())
        .bind(i64::try_from(limit).unwrap_or(20))
        .fetch_all(&self.pool)
        .await?;

        let mut hits = Vec::new();
        for row in note_rows {
            let note: Option<String> = row.try_get("note")?;
            let clean = strip_html(note.as_deref().unwrap_or(""));
            if !clean.to_lowercase().contains(&query_lc) {
                continue;
            }
            hits.push(NoteAnnotationHit {
                kind: HitKind::Note,
                key: ItemKey::from(row.try_get::<String, _>("key")?),
                text: note,
                comment: None,
                parent_key: row
                    .try_get::<Option<String>, _>("parentKey")?
                    .map(ItemKey::from),
                parent_title: row.try_get("parentTitle")?,
                page_label: None,
                color: None,
            });
        }
        for row in ann_rows {
            hits.push(NoteAnnotationHit {
                kind: HitKind::Annotation,
                key: ItemKey::from(row.try_get::<String, _>("key")?),
                text: row.try_get("text")?,
                comment: row.try_get("comment")?,
                parent_key: row
                    .try_get::<Option<String>, _>("parentKey")?
                    .map(ItemKey::from),
                parent_title: row.try_get("parentTitle")?,
                page_label: row.try_get("pageLabel")?,
                color: row.try_get("color")?,
            });
        }
        hits.truncate(limit);
        Ok(hits)
    }
}

/// Locates `zotero.sqlite` using `override_path`, `ZOTERO_DB_PATH`, profile
/// preferences, or the per-user default, in that order.
pub(crate) fn find_zotero_db(override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return Some(path.to_path_buf());
    }
    if let Some(path) = env::var_os("ZOTERO_DB_PATH").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    for dir in profiles_dirs() {
        if let Some(db) = db_in_profile(&dir) {
            return Some(db);
        }
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|home| db_in_dir(&home.join("Zotero")))
}

/// Looks up the `dataDir` preference in `prefs.js`, then falls back to
/// `profile_dir` itself.
fn db_in_profile(profile_dir: &Path) -> Option<PathBuf> {
    let prefs = profile_dir.join("prefs.js");
    if prefs.is_file() {
        if let Some(data_dir) =
            read_string_pref(&prefs, "extensions.zotero.dataDir")
        {
            if let Some(db) = db_in_dir(&PathBuf::from(data_dir)) {
                return Some(db);
            }
        }
    }
    db_in_dir(profile_dir)
}

/// Returns `dir/zotero.sqlite` if it exists.
fn db_in_dir(dir: &Path) -> Option<PathBuf> {
    let db = dir.join("zotero.sqlite");
    db.is_file().then_some(db)
}

/// Returns candidate profile directories for the current operating system.
fn profiles_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(appdata) = env::var_os("APPDATA") {
        dirs.push(
            PathBuf::from(appdata)
                .join("Zotero")
                .join("Zotero")
                .join("Profiles"),
        );
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        dirs.push(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Zotero")
                .join("Profiles"),
        );
    }
    #[cfg(target_os = "linux")]
    if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".zotero").join("zotero"));
    }
    dirs
}

/// Parses `user_pref("key", "value");` from Zotero's `prefs.js`, returning
/// the unquoted value.
fn read_string_pref(prefs: &Path, key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(prefs).ok()?;
    let needle = format!("user_pref(\"{key}\",");
    contents.lines().find_map(|line| {
        let line = line.trim();
        if !line.starts_with(&needle) {
            return None;
        }
        let rest = line.trim_start_matches(&needle).trim_start();
        let mut value = String::new();
        let mut escaped = false;
        for ch in rest.strip_prefix('"')?.chars() {
            if escaped {
                value.push(match ch {
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                break;
            } else {
                value.push(ch);
            }
        }
        Some(value)
    })
}

/// Strips HTML tags from Zotero note HTML.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pretty_assertions::assert_eq;

    use super::*;

    #[expect(
        clippy::too_many_lines,
        reason = "seeds a realistic Zotero schema across many tables"
    )]
    async fn seed_db(path: &Path) -> Result<(), sqlx::Error> {
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            path.display()
        ))?
        .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();

        sqlx::query(
            "CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName \
             TEXT)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE items (itemID INTEGER PRIMARY KEY, key TEXT, \
             itemTypeID INTEGER, dateAdded TEXT, dateModified TEXT)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE fields (fieldID INTEGER PRIMARY KEY, fieldName TEXT)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE itemData (itemID INTEGER, fieldID INTEGER, valueID \
             INTEGER)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, value \
             TEXT)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, firstName \
             TEXT, lastName TEXT, fieldMode INT)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE itemCreators (itemID INTEGER, creatorID INTEGER)",
        )
        .execute(&pool)
        .await?;
        sqlx::query("CREATE TABLE deletedItems (itemID INTEGER)")
            .execute(&pool)
            .await?;
        sqlx::query(
            "CREATE TABLE fulltextWords (wordID INTEGER PRIMARY KEY, word \
             TEXT UNIQUE)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE fulltextItemWords (wordID INT, itemID INT, PRIMARY \
             KEY (wordID, itemID))",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE itemNotes (itemID INTEGER, parentItemID INTEGER, \
             note TEXT, title TEXT)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE itemAnnotations (itemID INTEGER, parentItemID \
             INTEGER, text TEXT, comment TEXT, type INTEGER, color TEXT, \
             pageLabel TEXT)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE itemAttachments (itemID INTEGER, parentItemID \
             INTEGER, path TEXT, contentType TEXT)",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO fields (fieldID, fieldName) VALUES (1, 'title'), \
             (16, 'extra'), (7, 'DOI')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO itemTypes (itemTypeID, typeName) VALUES (1, \
             'journalArticle'), (2, 'note'), (3, 'attachment')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
             dateModified) VALUES (1, 'K00001', 1, '2024-01-01', '2024-02-01')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO itemData (itemID, fieldID, valueID) VALUES (1, 1, \
             100), (1, 7, 101)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO itemDataValues (valueID, value) VALUES (100, 'Rust \
             in Action'), (101, '10.1000/rust')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
             dateModified) VALUES (3, 'A00001', 3, '2024-01-02', '2024-02-02')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO itemAttachments (itemID, parentItemID, path, \
             contentType) VALUES (3, 1, 'storage:K00001.pdf', \
             'application/pdf')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO fulltextWords (wordID, word) VALUES (1, 'the'), (2, \
             'borrow'), (3, 'checker'), (4, 'ensures'), (5, 'memory'), (6, \
             'safety')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO fulltextItemWords (wordID, itemID) VALUES (1, 3), \
             (2, 3), (3, 3), (4, 3), (5, 3), (6, 3)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO creators (creatorID, firstName, lastName, fieldMode) \
             VALUES (1, 'Jon', 'Gjengset', 0)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO itemCreators (itemID, creatorID) VALUES (1, 1)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
             dateModified) VALUES (2, 'N00001', 2, '2024-03-01', '2024-03-01')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO itemNotes (itemID, parentItemID, note, title) VALUES \
             (2, 1, '<p>Ownership summary</p>', 'summary')",
        )
        .execute(&pool)
        .await?;

        pool.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn opens_read_only_immutable_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let seeded = seed_db(&db_path).await;
        assert!(seeded.is_ok(), "seed database should be created: {seeded:?}");

        let db = LocalZoteroDb::open(&db_path).await.unwrap();
        let hits = db.search_fulltext("safety", 5).await.unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn rejects_non_zotero_database() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other.sqlite");
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            other.display()
        ))
        .unwrap()
        .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();
        sqlx::query("CREATE TABLE anything (x INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let err = LocalZoteroDb::open(&other).await.unwrap_err();
        assert!(matches!(err, ZoteroMcpError::LocalDb(_)));
    }

    #[tokio::test]
    async fn searches_fulltext_across_items() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let seeded = seed_db(&db_path).await;
        assert!(seeded.is_ok(), "seed database should be created: {seeded:?}");
        let db = LocalZoteroDb::open(&db_path).await.unwrap();

        let hits = db.search_fulltext("borrow checker", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        let first = hits.first().unwrap();
        assert_eq!(first.title.as_deref(), Some("Rust in Action"));
        assert!(first.snippet.contains("borrow checker"));

        let none = db.search_fulltext("nothing matches", 10).await.unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn fulltext_search_matches_metadata_or_all_fulltext_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let seeded = seed_db(&db_path).await;
        assert!(seeded.is_ok(), "seed database should be created: {seeded:?}");
        let db = LocalZoteroDb::open(&db_path).await.unwrap();

        let metadata_hits =
            db.search_fulltext("Rust in Action", 10).await.unwrap();
        assert_eq!(metadata_hits.len(), 1);
        assert_eq!(
            metadata_hits.first().and_then(|hit| hit.title.as_deref()),
            Some("Rust in Action")
        );

        let fulltext_hits =
            db.search_fulltext("borrow checker", 10).await.unwrap();
        assert_eq!(fulltext_hits.len(), 1);
        assert_eq!(
            fulltext_hits.first().and_then(|hit| hit.title.as_deref()),
            Some("Rust in Action")
        );
    }

    #[tokio::test]
    async fn searches_notes_and_annotations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let seeded = seed_db(&db_path).await;
        assert!(seeded.is_ok(), "seed database should be created: {seeded:?}");
        let db = LocalZoteroDb::open(&db_path).await.unwrap();

        let hits = db.search_notes_annotations("ownership", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        let hit = hits.first().unwrap();
        assert_eq!(hit.kind, HitKind::Note);
        assert_eq!(
            hit.parent_key.as_ref().map(ItemKey::as_str),
            Some("K00001")
        );
    }

    #[tokio::test]
    async fn searches_notes_past_html_false_positives_before_limit() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        let seeded = seed_db(&db_path).await;
        assert!(seeded.is_ok(), "seed database should be created: {seeded:?}");
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            db_path.display()
        ))
        .unwrap();
        let pool = SqlitePool::connect_with(opts).await.unwrap();
        sqlx::query(
            "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
             dateModified) VALUES (4, 'N00002', 2, '2024-03-02', \
             '2024-03-02'), (5, 'N00003', 2, '2024-03-03', '2024-03-03')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemNotes (itemID, parentItemID, note, title) VALUES \
             (4, 1, '<span data-query=\"visible\"></span>', 'hidden'), (5, 1, \
             '<p>visible note</p>', 'visible')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let db = LocalZoteroDb::open(&db_path).await.unwrap();
        let hits = db.search_notes_annotations("visible", 1).await.unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits.first().map(|hit| hit.key.as_str()).unwrap_or_default(),
            "N00003"
        );
    }

    #[test]
    fn read_string_pref_unescapes_paths_and_quotes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let prefs = dir.path().join("prefs.js");
        std::fs::write(
            &prefs,
            r#"user_pref("extensions.zotero.dataDir", "/Users/jack/Zotero \"Library\"");"#,
        )
        .expect("write prefs");

        let value = read_string_pref(&prefs, "extensions.zotero.dataDir");

        assert_eq!(value.as_deref(), Some("/Users/jack/Zotero \"Library\""));
    }

    #[test]
    fn db_in_dir_returns_zotero_sqlite_when_present() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = dir.path().join("zotero.sqlite");
        std::fs::write(&db, "").expect("write sqlite placeholder");

        let found = db_in_dir(dir.path());

        assert_eq!(found.as_deref(), Some(db.as_path()));
    }

    #[test]
    fn strip_html_removes_tags_but_keeps_visible_text() {
        let stripped = strip_html("<p>Hello <strong>visible</strong></p>");

        assert_eq!(stripped, "Hello visible");
    }
}
