//! JSON-RPC 2.0 envelopes and response shapes for the Better `BibTeX` API.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::zotero::{CitationKey, ItemKey};

/// Generates a `String`-backed Better `BibTeX` newtype with the conversions
/// needed for JSON-RPC argument boundaries.
macro_rules! string_value {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
            Clone,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            schemars::JsonSchema,
            Serialize,
        )]
        #[serde(transparent)]
        pub(crate) struct $name(pub(crate) String);

        impl std::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            #[inline]
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            #[inline]
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

string_value!(
    CollectionPath,
    concat!(
        "Better `BibTeX` collection path: a forward-slash separated \
         collection ",
        "path, where `//` targets the user's personal library root. Distinct ",
        "from Zotero collection keys."
    )
);

impl CollectionPath {
    /// Personal-library root path used by Better `BibTeX` collection APIs.
    pub(crate) fn personal_library() -> Self {
        Self("//".to_owned())
    }
}

string_value!(
    TranslatorName,
    concat!(
        "Better `BibTeX` translator name or GUID, e.g. `Better BibTeX`, ",
        "`Better BibLaTeX`, or `Better CSL JSON`."
    )
);
string_value!(AuxFilePath, "Absolute path to a `LaTeX` `.aux` file.");
string_value!(
    ExportFilePath,
    "Absolute path for a Better `BibTeX` auto-export output file."
);
string_value!(
    CslStyleId,
    "CSL style identifier accepted by Zotero, e.g. `apa` or a full style URI."
);
string_value!(
    Locale,
    "CSL locale identifier accepted by Zotero, e.g. `en-US`."
);
string_value!(SearchQuery, "Better `BibTeX` quick-search query string.");

/// Bibliography output content type accepted by Better `BibTeX`.
#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    PartialEq,
    schemars::JsonSchema,
    Serialize,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BibliographyContentType {
    /// Render bibliography as HTML.
    Html,
    /// Render bibliography as plain text.
    #[default]
    Text,
}

/// Format object passed to `item.bibliography`.
#[derive(
    Clone, Debug, Default, Deserialize, schemars::JsonSchema, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BibliographyFormat {
    /// Output content type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) content_type: Option<BibliographyContentType>,
    /// CSL style identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<CslStyleId>,
    /// CSL locale identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) locale: Option<Locale>,
    /// Use Zotero quick-copy settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) quick_copy: Option<bool>,
}

/// Request payload for `autoexport.add`.
#[derive(Clone, Debug, Default, Deserialize, schemars::JsonSchema)]
pub(crate) struct AutoExportAddRequest {
    /// Better `BibTeX` collection path.
    pub(crate) collection: CollectionPath,
    /// Translator name or GUID.
    pub(crate) translator: TranslatorName,
    /// Destination export file path.
    pub(crate) path: ExportFilePath,
    /// Interactive export display options.
    pub(crate) display_options: Option<HashMap<String, bool>>,
    /// Replace an existing auto-export with incompatible parameters.
    pub(crate) replace: Option<bool>,
}

/// A JSON-RPC 2.0 request envelope.
#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcRequest<'a, T: Serialize> {
    /// JSON-RPC version string (always `"2.0"`).
    pub(crate) jsonrpc: &'static str,
    /// RPC method name to call.
    pub(crate) method: &'a str,
    /// Parameter payload passed to the method.
    pub(crate) params: T,
    /// Request identifier.
    pub(crate) id: u64,
}

/// A JSON-RPC 2.0 response envelope, carrying either `result` payload or
/// [`JsonRpcError`].
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcResponse<T> {
    /// JSON-RPC version string (expected `"2.0"`).
    pub(crate) jsonrpc: String,
    /// Result payload if call succeeded.
    pub(crate) result: Option<T>,
    /// Error object if call failed.
    pub(crate) error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcError {
    /// Numeric error code.
    pub(crate) code: i64,
    /// Human-readable error message.
    pub(crate) message: String,
    /// Additional error detail.
    pub(crate) data: Option<serde_json::Value>,
}

/// Maps a Zotero item key to its Better `BibTeX` citation key.
pub(crate) type CitekeyMap = HashMap<ItemKey, Option<CitationKey>>;

/// Maps an old Better `BibTeX` citation key to its regenerated citation key.
pub(crate) type RegenerateKeyMap = HashMap<CitationKey, Option<CitationKey>>;

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
