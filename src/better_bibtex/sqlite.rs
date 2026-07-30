//! Read-only `SQLite` access to Better `BibTeX`'s citekey cache database.
//!
//! Used as a fast, offline fallback for citekey lookups that avoids a round
//! trip through the JSON-RPC API.

use std::{collections::HashMap, path::Path};

use sqlx::{
    Connection, Row,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};

use crate::{
    better_bibtex::models::CitekeyMap,
    errors::ZoteroMcpError,
    zotero::{CitationKey, ItemKey},
};

/// A row from Better `BibTeX`'s citekey cache table, mapping one Zotero item
/// key to its pinned citation key.
#[derive(Debug, sqlx::FromRow)]
struct CitationRow {
    #[sqlx(rename = "itemKey")]
    item_key: String,
    #[sqlx(rename = "citationKey")]
    citation_key: String,
}

/// Reads citation keys for `item_keys` from the Better `BibTeX` `SQLite`
/// database at `db_path`.
///
/// An empty `item_keys` slice fetches every citekey in the database.
/// Returns a [`CitekeyMap`] from Zotero item key to citation key; items with no
/// pinned citekey are returned with a `None` value, not an error.
///
/// # Errors
///
/// - [`NotFound`] if `db_path` does not exist
/// - [`Sqlite`] if the database cannot be opened or queried
///
/// [`NotFound`]: ZoteroMcpError::NotFound
/// [`Sqlite`]: ZoteroMcpError::Sqlite
pub(crate) async fn read_bbt_citekeys_sqlite(
    db_path: &Path,
    item_keys: &[ItemKey],
) -> Result<CitekeyMap, ZoteroMcpError> {
    if !db_path.exists() {
        return Err(ZoteroMcpError::NotFound(format!(
            "BBT SQLite database not found: {}",
            db_path.display()
        )));
    }

    let mut conn = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(db_path)
            .read_only(true)
            .create_if_missing(false),
    )
    .await?;

    let mut map = HashMap::new();
    if item_keys.is_empty() {
        let rows: Vec<CitationRow> = sqlx::query_as(
            "SELECT itemKey, citationKey FROM citationkey WHERE citationKey \
             IS NOT NULL",
        )
        .fetch_all(&mut conn)
        .await?;

        for row in rows {
            map.insert(
                ItemKey::from(row.item_key),
                Some(CitationKey::from(row.citation_key)),
            );
        }
    } else {
        for key in item_keys {
            let row = sqlx::query(
                "SELECT citationKey FROM citationkey WHERE itemKey = ?1 AND \
                 citationKey IS NOT NULL",
            )
            .bind(key.as_str())
            .fetch_optional(&mut conn)
            .await?;

            let citation_key = row
                .map(|row| row.try_get::<String, _>("citationKey"))
                .transpose()?
                .map(CitationKey::from);
            map.insert(key.clone(), citation_key);
        }
    }

    Ok(map)
}

/// Resolves the default [`PathBuf`](std::path::PathBuf) path to Better
/// `BibTeX`'s `better-bibtex.migrated` `SQLite` database.
///
/// Probes `~/Zotero/better-bibtex.migrated`, then
/// `~/Zotero/zotero/better-bibtex.migrated`, returning the first path that
/// exists. Falls back to a relative `Zotero/better-bibtex.migrated` path if
/// `$HOME` is unset.
pub(crate) fn get_default_bbt_db_path() -> std::path::PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let path =
            std::path::Path::new(&home).join("Zotero/better-bibtex.migrated");
        if path.exists() {
            return path;
        }
        let profile_path = std::path::Path::new(&home)
            .join("Zotero/zotero/better-bibtex.migrated");
        if profile_path.exists() {
            return profile_path;
        }
        path
    } else {
        std::path::PathBuf::from("Zotero/better-bibtex.migrated")
    }
}

#[cfg(test)]
mod tests {
    mod fixtures {
        use std::path::PathBuf;

        use sqlx::{
            Connection,
            sqlite::{SqliteConnectOptions, SqliteConnection},
        };
        use tempfile::TempDir;

        /// Creates a temp Better `BibTeX` `citationkey` table seeded with
        /// `insert_sql` (a full `INSERT INTO citationkey (...) VALUES (...)`
        /// statement). Returns the backing [`TempDir`] — keep it alive for
        /// the test's duration — and the database [`PathBuf`] path.
        pub(super) async fn seeded_db(insert_sql: &str) -> (TempDir, PathBuf) {
            let temp_dir = tempfile::tempdir().unwrap();
            let db_path = temp_dir.path().join("better-bibtex.migrated");
            let mut conn = SqliteConnection::connect_with(
                &SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE citationkey (itemID INTEGER, itemKey TEXT, \
                 libraryID INTEGER, citationKey TEXT, pinned INTEGER)",
            )
            .execute(&mut conn)
            .await
            .unwrap();
            sqlx::query(insert_sql).execute(&mut conn).await.unwrap();
            (temp_dir, db_path)
        }
    }

    mod read_bbt_citekeys_sqlite {
        use pretty_assertions::assert_eq;

        use super::{super::*, fixtures::seeded_db};

        #[tokio::test]
        async fn returns_citekey_for_matching_item_key() {
            // Arrange
            let (_temp_dir, db_path) = seeded_db(
                "INSERT INTO citationkey (itemID, itemKey, libraryID, \
                 citationKey, pinned) VALUES (1, 'ITEMKEY1', 1, 'citekey1', 0)",
            )
            .await;

            let map = read_bbt_citekeys_sqlite(&db_path, &[ItemKey::from(
                "ITEMKEY1",
            )])
            .await
            .unwrap();

            // Assert
            assert_eq!(
                map.get(&ItemKey::from("ITEMKEY1")),
                Some(&Some(CitationKey::from("citekey1")))
            );
        }

        #[tokio::test]
        async fn returns_not_found_error_when_database_is_missing() {
            // Arrange
            let temp_dir = tempfile::tempdir().unwrap();
            let db_path = temp_dir.path().join("does-not-exist.migrated");

            // Act
            let err = read_bbt_citekeys_sqlite(&db_path, &[ItemKey::from(
                "ITEMKEY1",
            )])
            .await
            .unwrap_err();

            // Assert
            assert!(matches!(err, ZoteroMcpError::NotFound(_)));
        }

        #[tokio::test]
        async fn returns_every_pinned_citekey_when_item_keys_is_empty() {
            // Arrange
            let (_temp_dir, db_path) = seeded_db(
                "INSERT INTO citationkey (itemID, itemKey, libraryID, \
                 citationKey, pinned) VALUES (1, 'ITEMKEY1', 1, 'citekey1', \
                 0), (2, 'ITEMKEY2', 1, 'citekey2', 0), (3, 'ITEMKEY3', 1, \
                 NULL, 0)",
            )
            .await;

            // Act
            let map = read_bbt_citekeys_sqlite(&db_path, &[]).await.unwrap();

            // Assert
            assert_eq!(map.len(), 2);
            assert_eq!(
                map.get(&ItemKey::from("ITEMKEY1")),
                Some(&Some(CitationKey::from("citekey1")))
            );
            assert_eq!(
                map.get(&ItemKey::from("ITEMKEY2")),
                Some(&Some(CitationKey::from("citekey2")))
            );
            assert!(!map.contains_key(&ItemKey::from("ITEMKEY3")));
        }

        #[tokio::test]
        async fn returns_none_for_requested_items_without_a_pinned_citekey() {
            // Arrange
            let (_temp_dir, db_path) = seeded_db(
                "INSERT INTO citationkey (itemID, itemKey, libraryID, \
                 citationKey, pinned) VALUES (1, 'ITEMKEY1', 1, NULL, 0)",
            )
            .await;

            // Act
            let map = read_bbt_citekeys_sqlite(&db_path, &[
                ItemKey::from("ITEMKEY1"),
                ItemKey::from("MISSING"),
            ])
            .await
            .unwrap();

            // Assert
            assert_eq!(map.get(&ItemKey::from("ITEMKEY1")), Some(&None));
            assert_eq!(map.get(&ItemKey::from("MISSING")), Some(&None));
        }
    }
}
