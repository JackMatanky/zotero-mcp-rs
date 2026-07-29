//! Async client for the Better `BibTeX` JSON-RPC API.
//!
//! Every RPC call is routed through [`BetterBibtexClient::call_rpc`], which
//! wraps the JSON-RPC request/response envelope and maps failures to
//! [`ZoteroMcpError::BetterBibTeX`].

use serde::Serialize;
use serde_json::Value;

use crate::{
    better_bibtex::{
        models::{CitekeyMap, JsonRpcRequest, JsonRpcResponse},
        sqlite::{get_default_bbt_db_path, read_bbt_citekeys_sqlite},
    },
    errors::ZoteroMcpError,
    state::AppState,
};

/// Client for the Better `BibTeX` JSON-RPC API, scoped to a single tool call.
pub(crate) struct BetterBibtexClient<'a> {
    state: &'a AppState,
}

impl<'a> BetterBibtexClient<'a> {
    /// Creates a Better `BibTeX` client borrowing shared [`AppState`].
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
        }
    }

    /// Issues a JSON-RPC 2.0 call to `method` with `params`, decoding the
    /// result as `R`.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the HTTP response is non-2xx, the RPC response
    ///   carries an `error` object, or the result is `null`
    /// - [`Network`] if the request fails at the transport level
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    /// [`Network`]: ZoteroMcpError::Network
    async fn call_rpc<P: Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R, ZoteroMcpError> {
        let req_body = JsonRpcRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };

        let resp = self
            .state
            .send_with_retry(
                self.state
                    .client
                    .post(&self.state.better_bibtex_url)
                    .json(&req_body),
            )
            .await?;

        if !resp.status().is_success() {
            return Err(ZoteroMcpError::BetterBibTeX(format!(
                "HTTP {}",
                resp.status()
            )));
        }

        let rpc_resp: JsonRpcResponse<R> = resp.json().await?;
        if rpc_resp.jsonrpc != "2.0" {
            return Err(ZoteroMcpError::BetterBibTeX(format!(
                "Unsupported JSON-RPC version {}",
                rpc_resp.jsonrpc
            )));
        }

        if let Some(err) = rpc_resp.error {
            let detail =
                err.data.map(|d| format!(" (data: {d})")).unwrap_or_default();
            return Err(ZoteroMcpError::BetterBibTeX(format!(
                "RPC error {}: {}{detail}",
                err.code, err.message
            )));
        }

        rpc_resp.result.ok_or_else(|| {
            ZoteroMcpError::BetterBibTeX(
                "JSON-RPC returned null result".to_owned(),
            )
        })
    }

    /// Maps `item_keys` to their Better `BibTeX` citation keys.
    ///
    /// Tries the local `SQLite` citekey cache first (fast path, no HTTP
    /// round trip); falls back to the JSON-RPC `item.citationkey` call if
    /// the cache is missing, unreadable, or yields no matches.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the JSON-RPC fallback fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn get_citekeys(
        &self,
        item_keys: &[&str],
    ) -> Result<CitekeyMap, ZoteroMcpError> {
        // Fast path: Try reading from ~/Zotero/better-bibtex.migrated SQLite DB
        // (~0.01ms)
        let db_path = get_default_bbt_db_path();
        if let Ok(map) = read_bbt_citekeys_sqlite(&db_path, item_keys).await {
            if !map.is_empty() {
                return Ok(map);
            }
        }

        // Fallback: Query Better BibTeX JSON-RPC API
        let params = vec![item_keys];
        self.call_rpc("item.citationkey", params).await
    }

    /// Exports `item_keys` using the named `translator` (e.g. `Better
    /// BibTeX`, `Better BibLaTeX`, `CSL JSON`).
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn export_items(
        &self,
        item_keys: &[&str],
        translator: &str,
    ) -> Result<String, ZoteroMcpError> {
        let params = (item_keys, translator);
        self.call_rpc("item.export", params).await
    }

    /// Generates a formatted bibliography for `item_keys` using the given
    /// citation `style` and `locale`, if provided.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn bibliography(
        &self,
        item_keys: &[&str],
        style: Option<&str>,
        locale: Option<&str>,
    ) -> Result<String, ZoteroMcpError> {
        let params = (item_keys, style, locale);
        self.call_rpc("item.bibliography", params).await
    }

    /// Runs a high-precision Better `BibTeX` search for `terms`.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn search(
        &self,
        terms: &str,
    ) -> Result<Value, ZoteroMcpError> {
        let params = vec![terms];
        self.call_rpc("item.search", params).await
    }

    /// Fetches Pandoc citeproc filter metadata for `item_keys`, as CSL JSON
    /// when `as_csl` is `true`.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn pandoc_filter(
        &self,
        item_keys: &[&str],
        as_csl: bool,
    ) -> Result<Value, ZoteroMcpError> {
        let params = (item_keys, as_csl);
        self.call_rpc("item.pandoc_filter", params).await
    }

    /// Regenerates citation keys for `item_keys`.
    ///
    /// Mutates the Zotero library; assumes the caller has already enforced
    /// [`AppState::check_write_permission`], and re-checks it itself before
    /// issuing the call.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn regenerate_keys(
        &self,
        item_keys: &[&str],
    ) -> Result<Value, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let params = vec![item_keys];
        self.call_rpc("item.regenerate_key", params).await
    }

    /// Registers an auto-export job that writes `collection_key` to `path`
    /// using `translator` whenever the collection changes.
    ///
    /// Mutates Better `BibTeX`'s export configuration; assumes the caller has
    /// already enforced [`AppState::check_write_permission`], and
    /// re-checks it itself before issuing the call.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn autoexport_add(
        &self,
        collection_key: &str,
        translator: &str,
        path: &str,
    ) -> Result<Value, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let params = (collection_key, translator, path);
        self.call_rpc("autoexport.add", params).await
    }

    /// Scans a `LaTeX` `.aux` file at `aux_path` and imports its cited
    /// references into `collection_key`.
    ///
    /// Mutates the Zotero library; assumes the caller has already enforced
    /// [`AppState::check_write_permission`], and re-checks it itself before
    /// issuing the call.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if write operations are disabled
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn scan_aux(
        &self,
        collection_key: &str,
        aux_path: &str,
    ) -> Result<Value, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let params = (collection_key, aux_path);
        self.call_rpc("collection.scanAUX", params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use reqwest::Client;

        use super::AppState;

        /// Builds an [`AppState`] pointing `better_bibtex_url` at a fixture
        /// server, with `write_enabled` set for write-gate tests.
        pub(super) fn test_state(
            better_bibtex_url: String,
            write_enabled: bool,
        ) -> AppState {
            AppState {
                client: Client::new(),
                zotero_api_url: String::new(),
                better_bibtex_url,
                better_notes_url: String::new(),
                write_enabled,
            }
        }

        /// Formats a minimal raw HTTP/1.1 response with `status` (e.g.
        /// `"200 OK"`) and a JSON `body`, computing `Content-Length`
        /// automatically.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: \
                 close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Spawns a background thread serving one canned raw HTTP response
        /// (see [`http_response`]) per accepted connection, in order.
        /// Returns the bound `http://host:port` base URL, standing in for
        /// the Better `BibTeX` JSON-RPC endpoint.
        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                let mut it = responses.into_iter();
                while let (Some(resp), Ok((mut stream, _))) =
                    (it.next(), listener.accept())
                {
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(resp.as_bytes());
                }
            });
            format!("http://{addr}")
        }
    }

    mod call_rpc {
        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        // Exercised indirectly through `export_items`, the simplest caller
        // of the shared `call_rpc` envelope handling.

        #[tokio::test]
        async fn returns_better_bibtex_error_when_http_status_is_non_success() {
            // Arrange
            let base = mock_server(vec![http_response("404 Not Found", "")]);
            let state = test_state(base, false);

            // Act
            let err = BetterBibtexClient::new(&state)
                .export_items(&["KEY1"], "Better BibTeX")
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(
                &err,
                ZoteroMcpError::BetterBibTeX(msg) if msg.contains("404")
            ));
        }

        #[tokio::test]
        async fn returns_better_bibtex_error_when_response_carries_an_rpc_error()
         {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"jsonrpc":"2.0","error":{"code":-32600,"message":"boom"}}"#,
            )]);
            let state = test_state(base, false);

            // Act
            let err = BetterBibtexClient::new(&state)
                .export_items(&["KEY1"], "Better BibTeX")
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(
                &err,
                ZoteroMcpError::BetterBibTeX(msg) if msg.contains("-32600") && msg.contains("boom")
            ));
        }

        #[tokio::test]
        async fn returns_better_bibtex_error_when_result_is_null() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"jsonrpc":"2.0"}"#,
            )]);
            let state = test_state(base, false);

            // Act
            let err = BetterBibtexClient::new(&state)
                .export_items(&["KEY1"], "Better BibTeX")
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(
                &err,
                ZoteroMcpError::BetterBibTeX(msg) if msg.contains("null result")
            ));
        }
    }

    mod export_items {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn returns_exported_string_on_success() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"jsonrpc":"2.0","result":"@article{foo,}"}"#,
            )]);
            let state = test_state(base, false);

            // Act
            let exported = BetterBibtexClient::new(&state)
                .export_items(&["KEY1"], "Better BibTeX")
                .await
                .unwrap();

            // Assert
            assert_eq!(exported, "@article{foo,}");
        }
    }

    mod regenerate_keys {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, test_state},
        };

        #[tokio::test]
        async fn rejects_when_write_is_disabled() {
            // Arrange
            let state = test_state(String::new(), false);

            // Act
            let err = BetterBibtexClient::new(&state)
                .regenerate_keys(&["KEY1"])
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }

        #[tokio::test]
        async fn returns_new_citekeys_when_write_is_enabled() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"jsonrpc":"2.0","result":{"KEY1":"newkey1"}}"#,
            )]);
            let state = test_state(base, true);

            // Act
            let result = BetterBibtexClient::new(&state)
                .regenerate_keys(&["KEY1"])
                .await
                .unwrap();

            // Assert
            assert_eq!(
                result.get("KEY1"),
                Some(&Value::String("newkey1".to_owned()))
            );
        }
    }
}
