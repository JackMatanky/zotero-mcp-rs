//! Read-only access to Zotero's local `zotero.sqlite` database.
//!
//! Mirrors the discovery and SQL approach documented in
//! `docs/54yyyu-zotero-mcp-digest.txt` (lines 6712-7779): locate the
//! database via `ZOTERO_DB_PATH`, the `prefs.js` `dataDir` preference, or the
//! per-user default; open it immutable/read-only so a running Zotero does not
//! block reads; and query the `itemData`/`fulltextItems`/`itemNotes`/
//! `itemAnnotations` tables directly.
//!
//! Every method is gated at the MCP tool layer by
//! [`AppState::check_sqlite_access`].
//!
//! [`AppState`]: crate::state::AppState

use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};

use crate::{errors::ZoteroMcpError, zotero::models::ItemKey};

/// Max rows to pull from the full-text scan before filtering in Rust.
const FULLTEXT_SCAN_CAP: usize = 2000;

/// Opens Zotero's local sqlite database in immutable read-only mode.
#[derive(Clone, Debug)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by local sqlite tools in a later task")
)]
pub(crate) struct LocalZoteroDb {
    pool: SqlitePool,
}

impl LocalZoteroDb {
    /// Opens `path` read-only with `immutable=1` semantics (mirrors the
    /// digest's `_get_connection`). Fails with [`ZoteroMcpError::LocalDb`] if
    /// `path` is unreadable or is not a Zotero database.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalDb`] if the path cannot be opened read-only or
    ///   the database is not a Zotero database
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "used by local sqlite tools in a later task"
        )
    )]
    pub(crate) async fn open(path: &Path) -> Result<Self, ZoteroMcpError> {
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            path.display()
        ))
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?
        .read_only(true)
        .immutable(true)
        .busy_timeout(Duration::from_secs(2));
        let pool = SqlitePool::connect_with(opts)
            .await
            .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
        let db = Self {
            pool,
        };
        db.probe_schema().await?;
        Ok(db)
    }

    /// Verifies the `items` table exists, confirming this is a Zotero db.
    async fn probe_schema(&self) -> Result<(), ZoteroMcpError> {
        let row = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND \
             name='items'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
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
    /// - [`ZoteroMcpError::LocalDb`] if a query or row read fails
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "used by local sqlite tools in a later task"
        )
    )]
    pub(crate) async fn search_fulltext(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FulltextHit>, ZoteroMcpError> {
        let rows = sqlx::query(
            r"
            SELECT i.key, it.typeName AS item_type,
                   title.value AS title, doi.value AS doi,
                   extra.value AS extra,
                   creators.creators AS creators,
                   ft.content AS fulltext
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
                    COALESCE(
                        c.name,
                        c.lastName || ' ' || c.firstName
                    ),
                    '; '
                ) AS creators
                FROM itemCreators ic
                JOIN creators c ON c.creatorID = ic.creatorID
                GROUP BY ic.itemID
            ) creators ON creators.itemID = i.itemID
            LEFT JOIN fulltextItems ft ON ft.itemID = i.itemID
            WHERE it.typeName NOT IN ('attachment', 'note', 'annotation')
              AND i.itemID NOT IN (SELECT itemID FROM deletedItems)
            ",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;

        let query_lc = query.to_lowercase();
        let mut hits = Vec::new();
        for row in rows {
            if hits.len() >= FULLTEXT_SCAN_CAP {
                break;
            }
            let title: Option<String> = row
                .try_get("title")
                .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let doi: Option<String> = row
                .try_get("doi")
                .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let extra: Option<String> = row
                .try_get("extra")
                .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let creators: Option<String> = row
                .try_get("creators")
                .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let fulltext: Option<String> = row
                .try_get("fulltext")
                .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;

            let haystack = format!(
                "{} {} {} {} {}",
                title.as_deref().unwrap_or(""),
                creators.as_deref().unwrap_or(""),
                doi.as_deref().unwrap_or(""),
                extra.as_deref().unwrap_or(""),
                fulltext.as_deref().unwrap_or("")
            );
            if !haystack.to_lowercase().contains(&query_lc) {
                continue;
            }
            let key: String = row
                .try_get("key")
                .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let item_type: String = row
                .try_get("item_type")
                .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            hits.push(FulltextHit {
                key: ItemKey::from(key),
                item_type,
                title,
                doi,
                creators: creators.unwrap_or_default(),
                snippet: fulltext
                    .as_deref()
                    .map(|f| f.chars().take(400).collect())
                    .unwrap_or_default(),
            });
            if hits.len() >= limit {
                break;
            }
        }
        Ok(hits)
    }

    /// Searches child notes and PDF annotations for `query`, returning at most
    /// `limit` hits. Mirrors the digest's `search_notes_local` /
    /// `search_annotations_local`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalDb`] if a query or row read fails
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "used by local sqlite tools in a later task"
        )
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "SQL spans are long; mirrors digest query shape"
    )]
    pub(crate) async fn search_notes_annotations(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<NoteAnnotationHit>, ZoteroMcpError> {
        let pattern = format!("%{query}%");
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
        .bind(i64::try_from(limit).unwrap_or(20))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;

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
        .await
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;

        let mut hits = Vec::new();
        for row in note_rows {
            let note: Option<String> = row
                .try_get("note")
                .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let clean = strip_html(note.as_deref().unwrap_or(""));
            if !clean.to_lowercase().contains(&query.to_lowercase()) {
                continue;
            }
            hits.push(NoteAnnotationHit {
                kind: "note".to_owned(),
                key: ItemKey::from(
                    row.try_get::<String, _>("key")
                        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                ),
                text: note,
                comment: None,
                parent_key: row
                    .try_get::<Option<String>, _>("parentKey")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?
                    .map(ItemKey::from),
                parent_title: row
                    .try_get("parentTitle")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                page_label: None,
                color: None,
            });
        }
        for row in ann_rows {
            hits.push(NoteAnnotationHit {
                kind: "annotation".to_owned(),
                key: ItemKey::from(
                    row.try_get::<String, _>("key")
                        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                ),
                text: row
                    .try_get("text")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                comment: row
                    .try_get("comment")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                parent_key: row
                    .try_get::<Option<String>, _>("parentKey")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?
                    .map(ItemKey::from),
                parent_title: row
                    .try_get("parentTitle")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                page_label: row
                    .try_get("pageLabel")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                color: row
                    .try_get("color")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
            });
        }
        hits.truncate(limit);
        Ok(hits)
    }
}

/// A single full-text search hit.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by local sqlite tools in a later task")
)]
pub(crate) struct FulltextHit {
    pub(crate) key: ItemKey,
    pub(crate) item_type: String,
    pub(crate) title: Option<String>,
    pub(crate) doi: Option<String>,
    pub(crate) creators: String,
    pub(crate) snippet: String,
}

/// A single note or annotation search hit.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by local sqlite tools in a later task")
)]
pub(crate) struct NoteAnnotationHit {
    pub(crate) kind: String,
    pub(crate) key: ItemKey,
    pub(crate) text: Option<String>,
    pub(crate) comment: Option<String>,
    pub(crate) parent_key: Option<ItemKey>,
    pub(crate) parent_title: Option<String>,
    pub(crate) page_label: Option<String>,
    pub(crate) color: Option<String>,
}

/// Locates `zotero.sqlite` via `ZOTERO_DB_PATH`, the `prefs.js` `dataDir`
/// preference in any profile dir, or the per-user default, in that order.
#[expect(dead_code, reason = "used by local sqlite tools in a later task")]
pub(crate) fn find_zotero_db() -> Option<PathBuf> {
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

/// Looks up the `dataDir` pref in `prefs.js`, then falls back to the profile
/// dir itself.
#[expect(dead_code, reason = "used by local sqlite tools in a later task")]
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
#[expect(dead_code, reason = "used by local sqlite tools in a later task")]
fn db_in_dir(dir: &Path) -> Option<PathBuf> {
    let db = dir.join("zotero.sqlite");
    db.is_file().then_some(db)
}

/// Candidate profile directories, per-OS (mirrors the digest's
/// `_zotero_profiles_dirs`).
#[expect(dead_code, reason = "used by local sqlite tools in a later task")]
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
#[expect(dead_code, reason = "used by local sqlite tools in a later task")]
fn read_string_pref(prefs: &Path, key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(prefs).ok()?;
    let needle = format!("user_pref(\"{key}\",");
    contents.lines().find_map(|line| {
        let line = line.trim();
        if !line.starts_with(&needle) {
            return None;
        }
        let rest = line.trim_start_matches(&needle);
        let value = rest.strip_prefix('"')?.split('"').next()?;
        Some(value.replace("\\\"", "\"").replace("\\\\", "\\"))
    })
}

/// Strips HTML tags from Zotero note HTML.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by local sqlite tools in a later task")
)]
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

        // Items + itemTypes
        sqlx::query(
            "CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName \
             TEXT)",
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
            "CREATE TABLE fields (fieldID INTEGER PRIMARY KEY, fieldName TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemData (itemID INTEGER, fieldID INTEGER, valueID \
             INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, value \
             TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, firstName \
             TEXT, lastName TEXT, name TEXT)",
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
            "CREATE TABLE fulltextItems (itemID INTEGER, content TEXT, \
             indexedChars INTEGER, totalChars INTEGER, version INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemNotes (itemID INTEGER, parentItemID INTEGER, \
             note TEXT, title TEXT)",
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

        // fields: title=1, extra=16, DOI
        sqlx::query(
            "INSERT INTO fields (fieldID, fieldName) VALUES (1, 'title'), \
             (16, 'extra'), (7, 'DOI')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemTypes (itemTypeID, typeName) VALUES (1, \
             'journalArticle'), (2, 'note')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // item 1: "Rust in Action" with fulltext mentioning "borrow checker"
        sqlx::query(
            "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
             dateModified) VALUES (1, 'K00001', 1, '2024-01-01', '2024-02-01')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemData (itemID, fieldID, valueID) VALUES (1, 1, \
             100), (1, 7, 101)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemDataValues (valueID, value) VALUES (100, 'Rust \
             in Action'), (101, '10.1000/rust')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO fulltextItems (itemID, content) VALUES (1, 'The \
             borrow checker ensures memory safety.')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO creators (creatorID, firstName, lastName, name) \
             VALUES (1, 'Jon', 'Gjengset', NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemCreators (itemID, creatorID) VALUES (1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // item 2: a note child of item 1
        sqlx::query(
            "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
             dateModified) VALUES (2, 'N00001', 2, '2024-03-01', '2024-03-01')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemNotes (itemID, parentItemID, note, title) VALUES \
             (2, 1, '<p>Ownership summary</p>', 'summary')",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool.close().await;
    }

    #[tokio::test]
    async fn opens_read_only_immutable_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        seed_db(&db_path).await;

        // open() succeeding means probe_schema passed. Prove the pool is
        // usable with a real query rather than asserting pool.size() (which
        // is 0 until a connection is actually used).
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
        seed_db(&db_path).await;
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
    async fn searches_notes_and_annotations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        seed_db(&db_path).await;
        let db = LocalZoteroDb::open(&db_path).await.unwrap();

        let hits = db.search_notes_annotations("ownership", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        let hit = hits.first().unwrap();
        assert_eq!(hit.kind, "note");
        assert_eq!(
            hit.parent_key.as_ref().map(ItemKey::as_str),
            Some("K00001")
        );
    }
}
