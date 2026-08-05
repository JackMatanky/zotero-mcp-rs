//! Serialization models and JSON-RPC 2.0 envelopes for the Better `BibTeX` API.
//!
//! Defines the request and response shapes, content types, and new types used
//! when serializing RPC calls for [`BetterBibtexClient`] and deserializing
//! plugin output.
//!
//! [`BetterBibtexClient`]: crate::better_bibtex::BetterBibtexClient
//!
//! # Main Types
//!
//! - [`JsonRpcRequest`] - Outbound JSON-RPC 2.0 request envelope.
//! - [`JsonRpcResponse`] - Inbound JSON-RPC 2.0 response envelope.
//! - [`JsonRpcError`] - Error payload returned by failed RPC calls.
//! - [`BibliographyFormat`] - Output formatting configuration for
//!   bibliographies.
//! - [`BibliographyContentType`] - Content format (`Html` vs `Text`).
//! - [`AutoExportAddRequest`] - Parameters for registering an auto-export job.
//! - [`CollectionPath`] - Collection path representation (`"//"` for root).
//! - [`CitekeyMap`] - Mapping from Zotero item keys to citation keys.
//! - [`RegenerateKeyMap`] - Mapping from old citation keys to regenerated keys.
//!
//! # Examples
//!
//! ```no_run
//! use zotero_mcp_rs::better_bibtex::{
//!     BibliographyContentType, BibliographyFormat,
//! };
//!
//! let format = BibliographyFormat {
//!     content_type: Some(BibliographyContentType::Html),
//!     id: None,
//!     locale: None,
//!     quick_copy: None,
//! };
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::zotero::{CitationKey, ItemKey};

/// Generates a `String`-backed Better `BibTeX` new type wrapper with
/// conversions for JSON-RPC argument boundaries.
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
        "Better `BibTeX` collection path, represented as \
         forward-slash-separated ",
        "collections where `//` targets the user's personal library root. ",
        "Distinct from Zotero collection keys."
    )
);

impl CollectionPath {
    /// Returns the personal library root path (`"//"`) used by Better `BibTeX`
    /// collection APIs.
    pub(crate) fn personal_library() -> Self {
        Self("//".to_owned())
    }
}

string_value!(
    TranslatorName,
    concat!(
        "Better `BibTeX` translator name or GUID, such as `Better BibTeX`, ",
        "`Better BibLaTeX`, or `Better CSL JSON`."
    )
);
string_value!(
    AuxFilePath,
    "Absolute filesystem path to a `LaTeX` `.aux` file."
);
string_value!(
    ExportFilePath,
    "Absolute filesystem path for a Better `BibTeX` auto-export output file."
);
string_value!(
    CslStyleId,
    "CSL style identifier accepted by Zotero, such as `apa` or a full style \
     URI."
);
string_value!(
    Locale,
    "CSL locale identifier accepted by Zotero, such as `en-US`."
);
string_value!(SearchQuery, "Better `BibTeX` quick-search query string.");

/// Content type format for generated bibliography output.
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
    /// Renders the bibliography as HTML.
    Html,
    /// Renders the bibliography as plain text.
    #[default]
    Text,
}

/// Formatting configuration passed to the `item.bibliography` RPC method.
///
/// Controls the output content type, CSL style, locale, and quick-copy
/// defaults.
#[derive(
    Clone, Debug, Default, Deserialize, schemars::JsonSchema, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BibliographyFormat {
    /// Output content type (`Html` or `Text`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) content_type: Option<BibliographyContentType>,
    /// CSL style identifier (for example, `"apa"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<CslStyleId>,
    /// CSL locale identifier (for example, `"en-US"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) locale: Option<Locale>,
    /// Whether to apply Zotero quick-copy preferences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) quick_copy: Option<bool>,
}

/// Request payload for registering an auto-export job via `autoexport.add`.
///
/// Defines the collection, translator, destination filepath, and export
/// options.
#[derive(Clone, Debug, Default, Deserialize, schemars::JsonSchema)]
pub(crate) struct AutoExportAddRequest {
    /// Target collection path to export.
    pub(crate) collection: CollectionPath,
    /// Better `BibTeX` translator name or GUID.
    pub(crate) translator: TranslatorName,
    /// Destination output filepath; requires filepath features enabled and an
    /// allowed export directory.
    pub(crate) path: ExportFilePath,
    /// Optional display options key-value mapping.
    pub(crate) display_options: Option<HashMap<String, bool>>,
    /// Whether to replace an existing auto-export configuration.
    pub(crate) replace: Option<bool>,
}

/// Outbound JSON-RPC 2.0 request envelope sent to Better `BibTeX`.
#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcRequest<'a, T: Serialize> {
    /// JSON-RPC protocol version (always `"2.0"`).
    pub(crate) jsonrpc: &'static str,
    /// Remote RPC method identifier.
    pub(crate) method: &'a str,
    /// Parameter payload passed to the method.
    pub(crate) params: T,
    /// Unique request sequence identifier.
    pub(crate) id: u64,
}

/// Inbound JSON-RPC 2.0 response envelope returned by Better `BibTeX`.
///
/// Carries either a successful `result` payload or a [`JsonRpcError`].
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcResponse<T> {
    /// JSON-RPC protocol version (expected `"2.0"`).
    pub(crate) jsonrpc: String,
    /// Successful result payload, if the RPC succeeded.
    pub(crate) result: Option<T>,
    /// Error payload object, if the RPC failed.
    pub(crate) error: Option<JsonRpcError>,
}

/// Error object returned in a JSON-RPC 2.0 response when an RPC fails.
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcError {
    /// Numeric RPC error status code.
    pub(crate) code: i64,
    /// Human-readable error message describing the failure.
    pub(crate) message: String,
    /// Optional additional error detail object.
    pub(crate) data: Option<serde_json::Value>,
}

/// Maps a Zotero [`ItemKey`] to its assigned Better `BibTeX` [`CitationKey`].
///
/// A value of `None` indicates the item has no generated citation key.
///
/// [`ItemKey`]: crate::zotero::ItemKey
/// [`CitationKey`]: crate::zotero::CitationKey
pub(crate) type CitekeyMap = HashMap<ItemKey, Option<CitationKey>>;

/// Maps a current [`CitationKey`] to its newly regenerated [`CitationKey`].
///
/// A value of `None` indicates the citation key could not be regenerated.
///
/// [`CitationKey`]: crate::zotero::CitationKey
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
