//! Async HTTP client for the Better `BibTeX` Zotero plugin JSON-RPC 2.0 API.
//!
//! Wraps JSON-RPC method dispatch, request/response serialization, error
//! mapping, and security permission checks for all Better `BibTeX` operations.
//! Used by MCP tool handlers in `crate::mcp::better_bibtex`.
//!
//! Main types:
//! - [`BetterBibtexClient`] - JSON-RPC client borrowing [`AppState`]
//!
//! [`AppState`]: crate::state::AppState
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::better_bibtex::BetterBibtexClient;
//! # use zotero_mcp_rs::state::AppState;
//! # async fn example(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
//! let client = BetterBibtexClient::new(state);
//! let search_results = client.search(&"author:smith".into()).await?;
//! # Ok(())
//! # }
//! ```
use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    better_bibtex::models::{
        AutoExportAddRequest, AuxFilePath, BibliographyFormat, CitekeyMap,
        CollectionPath, JsonRpcRequest, JsonRpcResponse, RegenerateKeyMap,
        SearchQuery, TranslatorName,
    },
    errors::ZoteroMcpError,
    state::AppState,
    zotero::{CitationKey, ItemKey},
};

/// Client for issuing JSON-RPC 2.0 requests to the Better `BibTeX` plugin,
/// scoped to a single tool call.
pub(crate) struct BetterBibtexClient<'a> {
    state: &'a AppState,
}

impl<'a> BetterBibtexClient<'a> {
    /// Creates a new [`BetterBibtexClient`] borrowing the shared [`AppState`].
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
        }
    }

    /// Maps Zotero `item_keys` to their current Better `BibTeX` citation keys.
    ///
    /// Issues an `item.citationkey` JSON-RPC request to retrieve citation keys
    /// for the provided item keys.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`]: if the JSON-RPC call fails or returns an RPC error
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn get_citekeys(
        &self,
        item_keys: &[ItemKey],
    ) -> Result<CitekeyMap, ZoteroMcpError> {
        let params = vec![item_keys];
        self.call_rpc("item.citationkey", params).await
    }

    /// Exports items identified by `citekeys` formatted with the specified
    /// `translator`.
    ///
    /// Issues an `item.export` JSON-RPC request using translators such as
    /// `Better BibTeX` or `Better BibLaTeX`.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`]: if the JSON-RPC call fails or the translator is
    ///   invalid
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn export_items(
        &self,
        citekeys: &[CitationKey],
        translator: &TranslatorName,
    ) -> Result<String, ZoteroMcpError> {
        let params = (citekeys, translator);
        self.call_rpc("item.export", params).await
    }

    /// Generates a formatted bibliography string for `citekeys`.
    ///
    /// Issues an `item.bibliography` JSON-RPC request with optional `format`
    /// output options.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`]: if the JSON-RPC call fails or formatting options are
    ///   rejected
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn bibliography(
        &self,
        citekeys: &[CitationKey],
        format: Option<&BibliographyFormat>,
    ) -> Result<String, ZoteroMcpError> {
        let mut params = vec![serde_json::to_value(citekeys)?];
        if let Some(format) = format {
            params.push(serde_json::to_value(format)?);
        }
        self.call_rpc("item.bibliography", params).await
    }

    /// Executes a high-precision Better `BibTeX` search for `terms`.
    ///
    /// Issues an `item.search` JSON-RPC request against the Zotero item
    /// library.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`]: if the JSON-RPC call fails or the search query is
    ///   malformed
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn search(
        &self,
        terms: &SearchQuery,
    ) -> Result<Value, ZoteroMcpError> {
        let params = vec![terms];
        self.call_rpc("item.search", params).await
    }

    /// Fetches Pandoc citeproc filter metadata for `citekeys`.
    ///
    /// Issues an `item.pandoc_filter` JSON-RPC request. When `as_csl` is
    /// `true`, returns the output in CSL JSON format.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`]: if the JSON-RPC call fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn pandoc_filter(
        &self,
        citekeys: &[CitationKey],
        as_csl: bool,
    ) -> Result<Value, ZoteroMcpError> {
        let params = (citekeys, as_csl);
        self.call_rpc("item.pandoc_filter", params).await
    }

    /// Regenerates citation keys for `citekeys`.
    ///
    /// Mutates keys in the Zotero library. Checks write permission via
    /// [`AppState::check_write_permission`] before issuing the
    /// `item.regenerate_key` RPC call.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`]: if write operations are disabled in security
    ///   settings
    /// - [`BetterBibTeX`]: if the JSON-RPC call fails
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn regenerate_keys(
        &self,
        citekeys: &[CitationKey],
    ) -> Result<RegenerateKeyMap, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let params = vec![citekeys];
        self.call_rpc("item.regenerate_key", params).await
    }

    /// Registers an auto-export job for a collection.
    ///
    /// Mutates Better `BibTeX` export settings. Checks write permission via
    /// [`AppState::check_write_permission`], ensures filepath features are
    /// enabled, and validates output path permissions before issuing the
    /// `autoexport.add` RPC call.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`]: if write operations are disabled in security
    ///   settings
    /// - [`InputRejected`]: if filepath features are disabled or the export
    ///   directory is disallowed
    /// - [`BetterBibTeX`]: if the JSON-RPC call fails
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn autoexport_add(
        &self,
        request: &AutoExportAddRequest,
    ) -> Result<Value, ZoteroMcpError> {
        self.state.check_write_permission()?;
        if !self.state.security.file_paths_enabled {
            return Err(ZoteroMcpError::InputRejected(
                "File path features are disabled; set \
                 ZOTERO_MCP_PROFILE=workspace or \
                 ZOTERO_FILE_PATHS_ENABLED=true"
                    .to_owned(),
            ));
        }
        self.state.check_output_path(
            Path::new(request.path.as_ref()),
            &self.state.security.allowed_export_dirs,
            "auto-export output",
        )?;
        let mut params = vec![
            serde_json::to_value(&request.collection)?,
            serde_json::to_value(&request.translator)?,
            serde_json::to_value(&request.path)?,
        ];
        match (&request.display_options, request.replace) {
            (Some(display_options), _) => {
                params.push(serde_json::to_value(display_options)?);
            }
            (None, Some(_)) => params.push(json!({})),
            (None, None) => {}
        }
        if let Some(replace) = request.replace {
            params.push(json!(replace));
        }
        self.call_rpc("autoexport.add", params).await
    }

    /// Scans a `LaTeX` `.aux` file at `aux_path` and imports cited references
    /// into `collection`.
    ///
    /// Mutates the Zotero library by importing missing items. Checks write
    /// permission via [`AppState::check_write_permission`], ensures
    /// filepath features are enabled, and validates the AUX filepath before
    /// issuing the `collection.scanAUX` RPC call.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`]: if write operations are disabled in security
    ///   settings
    /// - [`InputRejected`]: if filepath features are disabled or the AUX
    ///   filepath is disallowed
    /// - [`BetterBibTeX`]: if the JSON-RPC call fails
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn scan_aux(
        &self,
        collection: &CollectionPath,
        aux_path: &AuxFilePath,
    ) -> Result<Value, ZoteroMcpError> {
        self.state.check_write_permission()?;
        if !self.state.security.file_paths_enabled {
            return Err(ZoteroMcpError::InputRejected(
                "File path features are disabled; set \
                 ZOTERO_MCP_PROFILE=workspace or \
                 ZOTERO_FILE_PATHS_ENABLED=true"
                    .to_owned(),
            ));
        }
        self.state.check_existing_read_path(
            Path::new(aux_path.as_ref()),
            &self.state.security.allowed_aux_dirs,
            "AUX scan",
        )?;
        let params = (collection, aux_path);
        self.call_rpc("collection.scanAUX", params).await
    }

    /// Issues a JSON-RPC 2.0 call to `method` with `params`, decoding the
    /// result payload as `R`.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`]: if the HTTP response is non-2xx, RPC response
    ///   carries an error object, or result is null
    /// - [`Network`]: if the transport-level HTTP request fails
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
}

#[cfg(test)]
mod tests {

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::{
                Arc,
                mpsc::{self, Receiver},
            },
        };

        use reqwest::Client;
        use tokio::sync::OnceCell;

        use crate::{security::SecurityConfig, state::AppState};

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
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled,
                sqlite_access: false,
                zotero_db_path: None,
                local_zotero_db: AppState::local_zotero_db_cache(),
                semantic_search_enabled: false,
                semantic_db_path: None,
                semantic_index: Arc::new(OnceCell::new()),
                embedding_provider: Arc::new(OnceCell::new()),
                security: SecurityConfig::default(),
            }
        }

        /// Formats a minimal JSON HTTP response with `status` and `body` for
        /// fixture servers.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Runs a one-shot fixture HTTP server for `responses` and returns its
        /// base URL.
        pub(super) fn mock_server(responses: Vec<String>) -> String {
            mock_server_with_requests(responses).0
        }

        /// Runs a one-shot fixture HTTP server for `responses`, captures each
        /// accepted request, and returns the base URL plus request receiver.
        pub(super) fn mock_server_with_requests(
            responses: Vec<String>,
        ) -> (String, Receiver<String>) {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut buf = vec![0_u8; 4096];
                    let n = stream.read(&mut buf).expect("read request");
                    let _ = tx.send(
                        String::from_utf8_lossy(
                            buf.get(..n).unwrap_or_default(),
                        )
                        .into_owned(),
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            (format!("http://{addr}"), rx)
        }
    }

    fn request_json(request: &str) -> serde_json::Value {
        let body = request.split("\r\n\r\n").nth(1).expect("request body");
        serde_json::from_str(body).expect("json request body")
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
                .export_items(
                    &[CitationKey::from("KEY1")],
                    &TranslatorName::from("Better BibTeX"),
                )
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
                .export_items(
                    &[CitationKey::from("KEY1")],
                    &TranslatorName::from("Better BibTeX"),
                )
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
                .export_items(
                    &[CitationKey::from("KEY1")],
                    &TranslatorName::from("Better BibTeX"),
                )
                .await
                .unwrap_err();

            // Assert
            assert!(matches!(
                &err,
                ZoteroMcpError::BetterBibTeX(msg) if msg.contains("null result")
            ));
        }
    }

    mod get_citekeys {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server_with_requests, test_state},
            request_json,
        };

        #[tokio::test]
        async fn sends_item_keys_to_json_rpc() {
            // Arrange
            let (base, requests) = mock_server_with_requests(vec![
                http_response(
                    "200 OK",
                    r#"{"jsonrpc":"2.0","result":{"ITEM1":"citekey1","MISSING":null}}"#,
                ),
            ]);
            let state = test_state(base, false);

            // Act
            let result = BetterBibtexClient::new(&state)
                .get_citekeys(&[
                    ItemKey::from("ITEM1"),
                    ItemKey::from("MISSING"),
                ])
                .await
                .unwrap();

            // Assert
            assert_eq!(
                result.get(&ItemKey::from("ITEM1")),
                Some(&Some(CitationKey::from("citekey1")))
            );
            assert_eq!(result.get(&ItemKey::from("MISSING")), Some(&None));
            let request = requests.recv().expect("captured request");
            let body = request_json(&request);
            assert_eq!(
                body.get("method"),
                Some(&serde_json::json!("item.citationkey"))
            );
            assert_eq!(
                body.get("params"),
                Some(&serde_json::json!([["ITEM1", "MISSING"]]))
            );
        }
    }

    mod export_items {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{
                http_response, mock_server, mock_server_with_requests,
                test_state,
            },
            request_json,
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
                .export_items(
                    &[CitationKey::from("KEY1")],
                    &TranslatorName::from("Better BibTeX"),
                )
                .await
                .unwrap();

            // Assert
            assert_eq!(exported, "@article{foo,}");
        }

        #[tokio::test]
        async fn sends_citekeys_and_translator() {
            // Arrange
            let (base, requests) =
                mock_server_with_requests(vec![http_response(
                    "200 OK",
                    r#"{"jsonrpc":"2.0","result":"@article{foo,}"}"#,
                )]);
            let state = test_state(base, false);

            // Act
            BetterBibtexClient::new(&state)
                .export_items(
                    &[CitationKey::from("citekey1")],
                    &TranslatorName::from("Better BibTeX"),
                )
                .await
                .unwrap();

            // Assert
            let request = requests.recv().expect("captured request");
            let body = request_json(&request);
            assert_eq!(
                body.get("method"),
                Some(&serde_json::json!("item.export"))
            );
            assert_eq!(
                body.get("params"),
                Some(&serde_json::json!([["citekey1"], "Better BibTeX"]))
            );
        }
    }

    mod bibliography {
        use pretty_assertions::assert_eq;

        use super::{
            super::{
                super::models::{BibliographyContentType, CslStyleId, Locale},
                *,
            },
            fixtures::{http_response, mock_server_with_requests, test_state},
            request_json,
        };

        #[tokio::test]
        async fn sends_bibliography_format_object() {
            // Arrange
            let (base, requests) =
                mock_server_with_requests(vec![http_response(
                    "200 OK",
                    r#"{"jsonrpc":"2.0","result":"Bibliography"}"#,
                )]);
            let state = test_state(base, false);
            let format = BibliographyFormat {
                content_type: Some(BibliographyContentType::Html),
                id: Some(CslStyleId::from("apa")),
                locale: Some(Locale::from("en-US")),
                quick_copy: Some(false),
            };

            // Act
            BetterBibtexClient::new(&state)
                .bibliography(&[CitationKey::from("citekey1")], Some(&format))
                .await
                .unwrap();

            // Assert
            let request = requests.recv().expect("captured request");
            let body = request_json(&request);
            assert_eq!(
                body.get("method"),
                Some(&serde_json::json!("item.bibliography"))
            );
            assert_eq!(
                body.get("params"),
                Some(&serde_json::json!([
                    ["citekey1"],
                    {
                        "contentType": "html",
                        "id": "apa",
                        "locale": "en-US",
                        "quickCopy": false
                    }
                ]))
            );
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
                .regenerate_keys(&[CitationKey::from("KEY1")])
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
                .regenerate_keys(&[CitationKey::from("KEY1")])
                .await
                .unwrap();

            // Assert
            assert_eq!(
                result.get(&CitationKey::from("KEY1")),
                Some(&Some(CitationKey::from("newkey1")))
            );
        }
    }

    mod autoexport_add {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server_with_requests, test_state},
            request_json,
        };
        use crate::better_bibtex::models::ExportFilePath;

        fn request(path: String) -> AutoExportAddRequest {
            AutoExportAddRequest {
                collection: CollectionPath::from("/Library"),
                translator: TranslatorName::from("Better BibTeX"),
                path: ExportFilePath::from(path),
                display_options: None,
                replace: None,
            }
        }

        #[tokio::test]
        async fn autoexport_rejects_output_path_when_file_paths_disabled() {
            let mut state = test_state(String::new(), true);
            state.security.file_paths_enabled = false;

            let err = BetterBibtexClient::new(&state)
                .autoexport_add(&request("/tmp/out.bib".to_owned()))
                .await
                .unwrap_err();

            assert!(matches!(
                err,
                ZoteroMcpError::InputRejected(message)
                    if message.contains("File path features are disabled")
            ));
        }

        #[tokio::test]
        async fn autoexport_rejects_output_path_outside_allowed_export_dir() {
            let allowed = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let output = outside.path().join("out.bib");
            let mut state = test_state(String::new(), true);
            state.security.file_paths_enabled = true;
            state.security.allowed_export_dirs =
                vec![allowed.path().canonicalize().unwrap()];

            let err = BetterBibtexClient::new(&state)
                .autoexport_add(&request(output.display().to_string()))
                .await
                .unwrap_err();

            assert!(matches!(
                err,
                ZoteroMcpError::InputRejected(message)
                    if message.contains("auto-export output")
            ));
        }

        #[tokio::test]
        async fn autoexport_sends_request_when_output_path_parent_is_allowed() {
            let root = tempfile::TempDir::new().unwrap();
            let output = root.path().join("out.bib");
            let (base, requests) =
                mock_server_with_requests(vec![http_response(
                    "200 OK",
                    r#"{"jsonrpc":"2.0","result":{"ok":true}}"#,
                )]);
            let mut state = test_state(base, true);
            state.security.file_paths_enabled = true;
            state.security.allowed_export_dirs =
                vec![root.path().canonicalize().unwrap()];
            let output_str = output.display().to_string();

            BetterBibtexClient::new(&state)
                .autoexport_add(&request(output_str.clone()))
                .await
                .unwrap();

            let request = requests.recv().expect("captured request");
            let body = request_json(&request);
            assert_eq!(
                body.get("method"),
                Some(&serde_json::json!("autoexport.add"))
            );
            assert_eq!(
                body.get("params").and_then(|params| params.get(2)),
                Some(&serde_json::json!(output_str))
            );
        }
    }

    mod scan_aux {
        use pretty_assertions::assert_eq;

        use super::{
            super::*,
            fixtures::{http_response, mock_server_with_requests, test_state},
            request_json,
        };

        #[tokio::test]
        async fn scan_aux_rejects_aux_path_outside_allowed_aux_dir() {
            let allowed = tempfile::TempDir::new().unwrap();
            let outside = tempfile::TempDir::new().unwrap();
            let aux = outside.path().join("paper.aux");
            std::fs::write(&aux, b"\\citation{key}").unwrap();
            let mut state = test_state(String::new(), true);
            state.security.file_paths_enabled = true;
            state.security.allowed_aux_dirs =
                vec![allowed.path().canonicalize().unwrap()];

            let err = BetterBibtexClient::new(&state)
                .scan_aux(
                    &CollectionPath::from("/Library"),
                    &AuxFilePath::from(aux.display().to_string()),
                )
                .await
                .unwrap_err();

            assert!(matches!(
                err,
                ZoteroMcpError::InputRejected(message)
                    if message.contains("AUX scan")
            ));
        }

        #[tokio::test]
        async fn scan_aux_sends_request_when_aux_path_is_allowed() {
            let root = tempfile::TempDir::new().unwrap();
            let aux = root.path().join("paper.aux");
            std::fs::write(&aux, b"\\citation{key}").unwrap();
            let (base, requests) =
                mock_server_with_requests(vec![http_response(
                    "200 OK",
                    r#"{"jsonrpc":"2.0","result":{"ok":true}}"#,
                )]);
            let mut state = test_state(base, true);
            state.security.file_paths_enabled = true;
            state.security.allowed_aux_dirs =
                vec![root.path().canonicalize().unwrap()];
            let aux_str = aux.display().to_string();

            BetterBibtexClient::new(&state)
                .scan_aux(
                    &CollectionPath::from("/Library"),
                    &AuxFilePath::from(aux_str.clone()),
                )
                .await
                .unwrap();

            let request = requests.recv().expect("captured request");
            let body = request_json(&request);
            assert_eq!(
                body.get("method"),
                Some(&serde_json::json!("collection.scanAUX"))
            );
            assert_eq!(
                body.get("params").and_then(|params| params.get(0)),
                Some(&serde_json::json!("/Library"))
            );
            assert_eq!(
                body.get("params").and_then(|params| params.get(1)),
                Some(&serde_json::json!(aux_str))
            );
        }
    }
}
