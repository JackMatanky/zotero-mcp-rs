//! JSON-RPC 2.0 envelopes and response shapes for the Better `BibTeX` API.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A JSON-RPC 2.0 request envelope.
#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcRequest<'a, T: Serialize> {
    pub(crate) jsonrpc: &'static str,
    pub(crate) method: &'a str,
    pub(crate) params: T,
    pub(crate) id: u64,
}

#[derive(Debug, Deserialize)]
/// A JSON-RPC 2.0 response envelope, carrying either `result` or `error`.
pub(crate) struct JsonRpcResponse<T> {
    pub(crate) jsonrpc: String,
    pub(crate) result: Option<T>,
    pub(crate) error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
    pub(crate) data: Option<serde_json::Value>,
}

/// Result of probing the Better `BibTeX` JSON-RPC endpoint for availability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BetterBibtexStatus {
    pub(crate) ready: bool,
    pub(crate) url: String,
    pub(crate) error: Option<String>,
}

/// Maps a Zotero item key to its Better `BibTeX` citation key.
pub(crate) type CitekeyMap = HashMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;

    mod json_rpc_request {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn serializes_method_and_id() {
            // Arrange
            let req = JsonRpcRequest {
                jsonrpc: "2.0",
                method: "item.citationkey",
                params: vec!["KEY1"],
                id: 1,
            };

            // Act
            let val = serde_json::to_value(&req).unwrap();

            // Assert
            assert_eq!(
                val.get("method"),
                Some(&serde_json::json!("item.citationkey"))
            );
            assert_eq!(val.get("id"), Some(&serde_json::json!(1)));
        }
    }

    mod json_rpc_response {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn deserializes_result_and_error_object() {
            // Arrange
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

            // Act
            let resp: JsonRpcResponse<serde_json::Value> =
                serde_json::from_value(resp_json).unwrap();

            // Assert
            assert_eq!(resp.jsonrpc, "2.0");
            let err = resp.error.unwrap();
            assert_eq!(err.code, -32600);
            assert_eq!(err.data, Some(serde_json::json!("extra detail")));
        }
    }
}
