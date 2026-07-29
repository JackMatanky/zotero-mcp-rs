//! Async client for the Better `BibTeX` JSON-RPC API.
//!
//! Every RPC call is routed through [`BetterBibtexClient::call_rpc`], which
//! wraps the JSON-RPC request/response envelope and maps failures to
//! [`ZoteroMcpError::BetterBibTeX`].

use crate::better_bibtex::models::{
    BetterBibtexStatus, CitekeyMap, JsonRpcRequest, JsonRpcResponse,
};
use crate::better_bibtex::sqlite::{
    get_default_bbt_db_path, read_bbt_citekeys_sqlite,
};
use crate::errors::ZoteroMcpError;
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;

/// Client for the Better `BibTeX` JSON-RPC API, scoped to a single tool call.
#[expect(dead_code, reason = "Client invoked by MCP tool handlers")]
pub(crate) struct BetterBibtexClient<'a> {
    state: &'a AppState,
}

#[expect(dead_code, reason = "Client methods invoked by MCP tool handlers")]
impl<'a> BetterBibtexClient<'a> {
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
        if let Some(err) = rpc_resp.error {
            return Err(ZoteroMcpError::BetterBibTeX(format!(
                "RPC error {}: {}",
                err.code, err.message
            )));
        }

        rpc_resp.result.ok_or_else(|| {
            ZoteroMcpError::BetterBibTeX(
                "JSON-RPC returned null result".to_string(),
            )
        })
    }

    /// Probes the Better `BibTeX` JSON-RPC endpoint for availability.
    ///
    /// Never returns an error: failures are captured in the returned
    /// [`BetterBibtexStatus::error`] field instead of being propagated.
    pub(crate) async fn check_status(&self) -> BetterBibtexStatus {
        match self.call_rpc::<Vec<&str>, Value>("api.ready", vec![]).await {
            Ok(_) => BetterBibtexStatus {
                ready: true,
                url: self.state.better_bibtex_url.clone(),
                error: None,
            },
            Err(e) => BetterBibtexStatus {
                ready: false,
                url: self.state.better_bibtex_url.clone(),
                error: Some(e.to_string()),
            },
        }
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
        // Fast path: Try reading from ~/Zotero/better-bibtex.migrated SQLite DB (~0.01ms)
        let db_path = get_default_bbt_db_path();
        if let Ok(map) = read_bbt_citekeys_sqlite(&db_path, item_keys) {
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

    /// Fetches notes attached to `item_keys` via Better `BibTeX`.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn get_notes(
        &self,
        item_keys: &[&str],
    ) -> Result<Value, ZoteroMcpError> {
        let params = vec![item_keys];
        self.call_rpc("item.notes", params).await
    }

    /// Fetches attachments for `item_keys` via Better `BibTeX`.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn get_attachments(
        &self,
        item_keys: &[&str],
    ) -> Result<Value, ZoteroMcpError> {
        let params = vec![item_keys];
        self.call_rpc("item.attachments", params).await
    }

    /// Fetches the collections containing `item_keys` via Better `BibTeX`.
    ///
    /// # Errors
    ///
    /// - [`BetterBibTeX`] if the JSON-RPC call fails
    ///
    /// [`BetterBibTeX`]: ZoteroMcpError::BetterBibTeX
    pub(crate) async fn get_collections(
        &self,
        item_keys: &[&str],
    ) -> Result<Value, ZoteroMcpError> {
        let params = vec![item_keys];
        self.call_rpc("item.collections", params).await
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
