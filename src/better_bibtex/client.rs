use crate::better_bibtex::models::{
    BetterBibtexStatus, CitekeyMap, JsonRpcRequest, JsonRpcResponse,
};
use crate::errors::ZoteroMcpError;
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;

#[expect(dead_code, reason = "Client invoked by MCP tool handlers")]
pub(crate) struct BetterBibtexClient<'a> {
    state: &'a AppState,
}

#[expect(dead_code, reason = "Client methods invoked by MCP tool handlers")]
impl<'a> BetterBibtexClient<'a> {
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self { state }
    }

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
            .client
            .post(&self.state.better_bibtex_url)
            .json(&req_body)
            .send()
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
            ZoteroMcpError::BetterBibTeX("JSON-RPC returned null result".to_string())
        })
    }

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

    pub(crate) async fn get_citekeys(
        &self,
        item_keys: &[&str],
    ) -> Result<CitekeyMap, ZoteroMcpError> {
        let params = vec![item_keys];
        self.call_rpc("item.citationkey", params).await
    }

    pub(crate) async fn export_items(
        &self,
        item_keys: &[&str],
        translator: &str,
    ) -> Result<String, ZoteroMcpError> {
        let params = (item_keys, translator);
        self.call_rpc("item.export", params).await
    }

    pub(crate) async fn bibliography(
        &self,
        item_keys: &[&str],
        style: Option<&str>,
        locale: Option<&str>,
    ) -> Result<String, ZoteroMcpError> {
        let params = (item_keys, style, locale);
        self.call_rpc("item.bibliography", params).await
    }

    pub(crate) async fn search(&self, terms: &str) -> Result<Value, ZoteroMcpError> {
        let params = vec![terms];
        self.call_rpc("item.search", params).await
    }

    pub(crate) async fn get_notes(&self, item_keys: &[&str]) -> Result<Value, ZoteroMcpError> {
        let params = vec![item_keys];
        self.call_rpc("item.notes", params).await
    }

    pub(crate) async fn get_attachments(
        &self,
        item_keys: &[&str],
    ) -> Result<Value, ZoteroMcpError> {
        let params = vec![item_keys];
        self.call_rpc("item.attachments", params).await
    }

    pub(crate) async fn get_collections(
        &self,
        item_keys: &[&str],
    ) -> Result<Value, ZoteroMcpError> {
        let params = vec![item_keys];
        self.call_rpc("item.collections", params).await
    }

    pub(crate) async fn pandoc_filter(
        &self,
        item_keys: &[&str],
        as_csl: bool,
    ) -> Result<Value, ZoteroMcpError> {
        let params = (item_keys, as_csl);
        self.call_rpc("item.pandoc_filter", params).await
    }

    pub(crate) async fn regenerate_keys(
        &self,
        item_keys: &[&str],
    ) -> Result<Value, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let params = vec![item_keys];
        self.call_rpc("item.regenerate_key", params).await
    }

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
