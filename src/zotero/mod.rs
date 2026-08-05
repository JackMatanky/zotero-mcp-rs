//! Zotero Local API client and flat domain layer.
//!
//! Provides [`ZoteroClient`] and underlying domain submodules for Zotero key
//! types, API object shapes, endpoint wrappers, metadata lookup, and direct
//! `SQLite` access. This module is called by the higher-level MCP tool handlers
//! in [`crate::mcp`] to interact with Zotero.
//!
//! For client usage examples, see the [`ZoteroClient`] documentation in
//! [`client`].
//!
//! # Main Types
//!
//! - [`ZoteroClient`] - HTTP client for the Zotero Local API
//! - [`ZoteroItem`] - A single library item
//! - [`ZoteroCollection`] - A collection hierarchy node
//! - [`ItemKey`] - 8-character alphanumeric item identifier
//! - [`CollectionKey`] - 8-character alphanumeric collection identifier
//! - [`LibraryVersion`] - Library version counter
//! - [`ItemType`] - Item kind (`journalArticle`, `book`, etc.)
//! - [`AnnotationType`] - PDF annotation kind
//! - [`LocalZoteroDb`] - Read-only `zotero.sqlite` access

mod annotations;
mod attachments;
mod client;
mod collections;
mod coverage;
mod duplicates;
mod fulltext;
mod items;
mod keys;
pub(crate) mod metadata;
mod notes;
mod objects;
mod relations;
mod search;
mod sqlite;
mod tags;
mod types;

pub(crate) use annotations::{AnnotationDraft, AnnotationPosition};
pub(crate) use client::ZoteroClient;
pub(crate) use collections::CollectionItemAction;
pub(crate) use items::TrashAction;
pub(crate) use keys::{
    CitationKey, CollectionKey, ItemKey, LibraryVersion, TagName,
};
pub(crate) use metadata::IdentifierKind;
pub(crate) use objects::{ZoteroCollection, ZoteroItem};
pub(crate) use search::{
    JoinMode, SearchCondition, SearchField, SearchOperator, SortDirection,
    SortField,
};
pub(crate) use sqlite::{LocalZoteroDb, find_zotero_db};
pub(crate) use types::{AnnotationType, CollectionParent, ItemType, LinkMode};

#[cfg(test)]
pub(crate) mod test_http {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread::{self, JoinHandle},
    };

    pub(crate) type RequestLog = Arc<Mutex<Vec<String>>>;

    pub(crate) struct MockServer {
        base_url: String,
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockServer {
        pub(crate) fn new(responses: Vec<String>) -> Self {
            Self::with_log(responses, None)
        }

        pub(crate) fn recording(responses: Vec<String>) -> (Self, RequestLog) {
            let recorded = Arc::new(Mutex::new(Vec::new()));
            let server = Self::with_log(responses, Some(Arc::clone(&recorded)));
            (server, recorded)
        }

        pub(crate) fn url(&self) -> &str {
            &self.base_url
        }

        fn with_log(
            responses: Vec<String>,
            recorded: Option<RequestLog>,
        ) -> Self {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind test listener");
            let addr = listener.local_addr().expect("test listener address");
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let handle = thread::spawn(move || {
                serve_responses(
                    &listener,
                    &responses,
                    recorded.as_ref(),
                    &thread_stop,
                );
            });

            Self {
                base_url: format!("http://{addr}"),
                stop,
                handle: Some(handle),
            }
        }
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(addr) = self.base_url.strip_prefix("http://") {
                let _ = TcpStream::connect(addr);
            }
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    pub(crate) fn http_response(status: &str, body: &str) -> String {
        http_response_with_headers(status, &[], body)
    }

    pub(crate) fn http_response_with_headers(
        status: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> String {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
             application/json\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }
        response.push_str("Connection: close\r\n\r\n");
        response.push_str(body);
        response
    }
    fn serve_responses(
        listener: &TcpListener,
        responses: &[String],
        recorded: Option<&RequestLog>,
        stop: &AtomicBool,
    ) {
        for response in responses {
            if !serve_response(listener, response, recorded, stop) {
                break;
            }
        }
    }

    fn serve_response(
        listener: &TcpListener,
        response: &str,
        recorded: Option<&RequestLog>,
        stop: &AtomicBool,
    ) -> bool {
        let Ok((mut stream, _)) = listener.accept() else {
            return false;
        };
        if stop.load(Ordering::Acquire) {
            return false;
        }
        record_or_drain_request(&mut stream, recorded);
        let _ = stream.write_all(response.as_bytes());
        true
    }

    fn record_or_drain_request(
        stream: &mut TcpStream,
        recorded: Option<&RequestLog>,
    ) {
        if let Some(recorded) = recorded {
            recorded
                .lock()
                .expect("request log lock")
                .push(read_request(stream));
            return;
        }
        let mut buf = [0_u8; 1024];
        let _ = stream.read(&mut buf);
    }

    pub(crate) fn request_body(
        raw: &str,
    ) -> Result<serde_json::Value, serde_json::Error> {
        let body = raw.split_once("\r\n\r\n").map_or("", |(_, body)| body);
        serde_json::from_str(body)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut buf = [0_u8; 1024];
        let mut data = Vec::new();
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            data.extend_from_slice(buf.get(..n).unwrap_or_default());
            if request_complete(&data) {
                break;
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    fn request_complete(data: &[u8]) -> bool {
        let Some((head_end, content_length)) = request_meta(data) else {
            return false;
        };
        data.len() >= head_end.saturating_add(content_length)
    }

    fn request_meta(data: &[u8]) -> Option<(usize, usize)> {
        let head_end =
            data.windows(4).position(|w| w == b"\r\n\r\n")?.saturating_add(4);
        let head =
            String::from_utf8_lossy(data.get(..head_end).unwrap_or_default());
        let content_length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        Some((head_end, content_length))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn drop_stops_server_with_unconsumed_responses() {
            let server = MockServer::new(vec![http_response("200 OK", "{}")]);

            drop(server);
        }
    }
}

#[cfg(test)]
pub(crate) mod test_sqlite {
    use std::{path::Path, str::FromStr};

    use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

    /// Seeds a minimal-but-realistic Zotero schema covering the fulltext,
    /// note, and annotation search paths used by `LocalZoteroDb`.
    #[expect(
        clippy::too_many_lines,
        reason = "seeds a realistic Zotero schema across many tables"
    )]
    pub(crate) async fn seed_zotero_db(path: &Path) -> Result<(), sqlx::Error> {
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            path.display()
        ))?
        .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await?;

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
}
