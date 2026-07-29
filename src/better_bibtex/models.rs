use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcRequest<'a, T: Serialize> {
    pub(crate) jsonrpc: &'static str,
    pub(crate) method: &'a str,
    pub(crate) params: T,
    pub(crate) id: u64,
}

#[expect(
    dead_code,
    reason = "Deserialized from Better BibTeX JSON-RPC response"
)]
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcResponse<T> {
    pub(crate) jsonrpc: String,
    pub(crate) result: Option<T>,
    pub(crate) error: Option<JsonRpcError>,
    pub(crate) id: Option<u64>,
}

#[expect(dead_code, reason = "Deserialized from Better BibTeX JSON-RPC error")]
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
    pub(crate) data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BetterBibtexStatus {
    pub(crate) ready: bool,
    pub(crate) url: String,
    pub(crate) error: Option<String>,
}

pub(crate) type CitekeyMap = HashMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_json_rpc_request_and_response() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "item.citationkey",
            params: vec!["KEY1"],
            id: 1,
        };
        let val = serde_json::to_value(&req).unwrap();
        assert_eq!(val["method"], "item.citationkey");
        assert_eq!(val["id"], 1);

        let resp_json = serde_json::json!({
            "jsonrpc": "2.0",
            "result": { "KEY1": "citekey1" },
            "error": {
                "code": -32600,
                "message": "Invalid request",
            "data": "extra detail"
            },
            "id": 1
        });

        let resp: JsonRpcResponse<serde_json::Value> = serde_json::from_value(resp_json).unwrap();
        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, Some(1));
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.data, Some(serde_json::json!("extra detail")));
    }
}
