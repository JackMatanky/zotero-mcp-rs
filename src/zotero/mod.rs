//! Zotero Local API client and flat domain layer.
//!
//! Provides [`ZoteroClient`] plus domain modules for Zotero keys, controlled
//! vocabulary types, API objects, resource endpoints, item subdomains,
//! metadata lookup, derived views, and direct `zotero.sqlite` access.

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
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use coverage::{ItemCoverageFlags, LibraryCoverage};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use duplicates::{DuplicateGroup, DuplicateType};
pub(crate) use items::TrashAction;
pub(crate) use keys::{
    CitationKey, CollectionKey, ItemKey, LibraryVersion, TagName,
};
pub(crate) use metadata::IdentifierKind;
pub(crate) use objects::{ZoteroCollection, ZoteroItem};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use relations::RelatedItem;
pub(crate) use search::{
    JoinMode, SearchCondition, SearchField, SearchOperator, SortDirection,
    SortField,
};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
pub(crate) use sqlite::{
    FulltextHit, LocalZoteroDb, NoteAnnotationHit, find_zotero_db,
};
#[allow(unused_imports, reason = "facade re-exports for crate consumers")]
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
