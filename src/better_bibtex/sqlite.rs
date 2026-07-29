//! Read-only `SQLite` access to Better `BibTeX`'s citekey cache database.
//!
//! Used as a fast, offline fallback for citekey lookups that avoids a
//! round trip through the JSON-RPC API.

use std::{collections::HashMap, path::Path};

use rusqlite::{Connection, OpenFlags};

use crate::{better_bibtex::models::CitekeyMap, errors::ZoteroMcpError};

/// Reads citation keys for `item_keys` from the Better `BibTeX` `SQLite`
/// database at `db_path`.
///
/// An empty `item_keys` slice fetches every citekey in the database.
/// Returns a map from Zotero item key to citation key; items with no
/// pinned citekey are simply absent from the result, not an error.
///
/// # Errors
///
/// - [`NotFound`] if `db_path` does not exist
/// - [`Sqlite`] if the database cannot be opened or queried
///
/// [`NotFound`]: ZoteroMcpError::NotFound
/// [`Sqlite`]: ZoteroMcpError::Sqlite
pub(crate) fn read_bbt_citekeys_sqlite(
    db_path: &Path,
    item_keys: &[&str],
) -> Result<CitekeyMap, ZoteroMcpError> {
    if !db_path.exists() {
        return Err(ZoteroMcpError::NotFound(format!(
            "BBT SQLite database not found: {}",
            db_path.display()
        )));
    }

    let uri = format!("file:{}?mode=ro", db_path.display());
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;

    let mut map = HashMap::new();
    if item_keys.is_empty() {
        // Fetch all citekeys
        let mut stmt = conn.prepare_cached(
            "SELECT itemKey, citationKey FROM citationkey WHERE citationKey \
             IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for (ik, ck) in rows.flatten() {
            map.insert(ik, ck);
        }
    } else {
        let mut stmt = conn.prepare_cached(
            "SELECT citationKey FROM citationkey WHERE itemKey = ?1 AND \
             citationKey IS NOT NULL",
        )?;
        for &k in item_keys {
            if let Ok(ck) = stmt.query_row([k], |row| row.get::<_, String>(0)) {
                map.insert(k.to_owned(), ck);
            }
        }
    }

    Ok(map)
}

/// Resolves the default path to Better `BibTeX`'s `better-bibtex.migrated`
/// `SQLite` database.
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
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_sqlite_bbt_reader() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("better-bibtex.migrated");

        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE citationkey (itemID INTEGER, itemKey TEXT, \
             libraryID INTEGER, citationKey TEXT, pinned INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO citationkey (itemID, itemKey, libraryID, \
             citationKey, pinned) VALUES (1, 'ITEMKEY1', 1, 'citekey1', 0)",
            [],
        )
        .unwrap();

        let map = read_bbt_citekeys_sqlite(&db_path, &["ITEMKEY1"]).unwrap();
        assert_eq!(map.get("ITEMKEY1").unwrap(), "citekey1");
    }
}
