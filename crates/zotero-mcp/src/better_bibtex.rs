//! MCP tool handlers and argument models for Better `BibTeX` integration.
//!
//! This module provides handlers for interacting with the Zotero Better
//! `BibTeX` plugin. Supported operations include:
//! - Retrieving and regenerating citation keys ([`GetCitekeysArgs`],
//!   [`RegenerateKeysArgs`])
//! - Exporting library items in `BibTeX`/`BibLaTeX` formats
//!   ([`ExportItemsArgs`])
//! - Formatting bibliographies ([`BibliographyArgs`])
//! - Scanning `LaTeX` `.aux` files ([`ScanAuxArgs`])
//! - Pandoc filter integration ([`PandocFilterArgs`])
//! - Configuring auto-exports ([`AutoExportAddArgs`])
//! - Performing quick search queries ([`BetterBibtexSearchArgs`])
//!
//! # Examples
//!
//! ```ignore
//! # use zotero_mcp::ZoteroMcpServer;
//! # use zotero_api::AppState;
//! # use zotero_mcp::better_bibtex::GetCitekeysArgs;
//! # use zotero_api::ItemKey;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let server = ZoteroMcpServer::new(AppState::from_env());
//! let args = GetCitekeysArgs {
//!     item_keys: vec![ItemKey::from("ABCD1234")],
//! };
//! let result = server.better_bibtex_get_citekeys_impl(args).await?;
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::{
    AuxFilePath, BetterBibtexClient, CitationKey, CollectionPath, ItemKey,
    SearchQuery, TranslatorName,
};

use crate::ZoteroMcpServer;

/// Mirrors `zotero_api::BibliographyContentType` for MCP argument schemas.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BibliographyContentTypeArg {
    Html,
    #[default]
    Text,
}
impl From<BibliographyContentTypeArg> for zotero_api::BibliographyContentType {
    #[inline]
    fn from(value: BibliographyContentTypeArg) -> Self {
        match value {
            BibliographyContentTypeArg::Html => Self::Html,
            BibliographyContentTypeArg::Text => Self::Text,
        }
    }
}

/// Mirrors `zotero_api::BibliographyFormat` for MCP argument schemas.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BibliographyFormatArgs {
    #[serde(default)]
    content_type: Option<BibliographyContentTypeArg>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    locale: Option<String>,
    #[serde(default)]
    quick_copy: Option<bool>,
}
impl From<BibliographyFormatArgs> for zotero_api::BibliographyFormat {
    #[inline]
    fn from(value: BibliographyFormatArgs) -> Self {
        Self {
            content_type: value.content_type.map(Into::into),
            id: value.id.map(Into::into),
            locale: value.locale.map(Into::into),
            quick_copy: value.quick_copy,
        }
    }
}

/// Mirrors `zotero_api::AutoExportAddRequest` for MCP argument schemas.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub(crate) struct AutoExportAddArgs {
    collection: String,
    translator: String,
    path: String,
    #[serde(default)]
    display_options: Option<std::collections::HashMap<String, bool>>,
    #[serde(default)]
    replace: Option<bool>,
}
impl From<AutoExportAddArgs> for zotero_api::AutoExportAddRequest {
    #[inline]
    fn from(value: AutoExportAddArgs) -> Self {
        Self {
            collection: value.collection.into(),
            translator: value.translator.into(),
            path: value.path.into(),
            display_options: value.display_options,
            replace: value.replace,
        }
    }
}

// --- Argument Schemas ---

/// Arguments for the `citekeys` action of `better_bibtex`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCitekeysArgs {
    /// Zotero item keys ([`ItemKey`]) to look up.
    pub(crate) item_keys: Vec<String>,
}

/// Arguments for the `regenerate` action of `better_bibtex`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RegenerateKeysArgs {
    /// Better `BibTeX` citation keys ([`CitationKey`]) to regenerate.
    pub(crate) citekeys: Vec<String>,
}

/// Arguments for the `export` action of `better_bibtex`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ExportItemsArgs {
    /// Better `BibTeX` citation keys ([`CitationKey`]) to export.
    pub(crate) citekeys: Vec<String>,
    /// Translator name or GUID ([`TranslatorName`]).
    pub(crate) translator: String,
}

/// Arguments for the `bibliography` action of `better_bibtex`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BibliographyArgs {
    /// Citation keys ([`CitationKey`]) to format.
    pub(crate) citekeys: Vec<String>,
    /// Optional Better `BibTeX` bibliography format settings
    /// ([`BibliographyFormat`]).
    pub(crate) format: Option<BibliographyFormatArgs>,
}

/// Arguments for the `scan_aux` action of `better_bibtex`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ScanAuxArgs {
    /// Better `BibTeX` collection path ([`CollectionPath`]) to import
    /// references into. Defaults to `//`, the personal library root.
    pub(crate) collection: Option<String>,
    /// Absolute path to the `LaTeX` `.aux` file ([`AuxFilePath`]).
    pub(crate) aux_path: String,
}

/// Arguments for the `pandoc_filter` action of `better_bibtex`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct PandocFilterArgs {
    /// Citation keys ([`CitationKey`]) to filter.
    pub(crate) citekeys: Vec<String>,
}

/// Arguments for the `search` action of `better_bibtex`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BetterBibtexSearchArgs {
    /// Better `BibTeX` quick-search query ([`SearchQuery`]).
    pub(crate) query: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    /// Retrieves Better `BibTeX` citation keys using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if citekey lookup fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_bibtex_get_citekeys_impl(
        &self,
        args: GetCitekeysArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let item_keys: Vec<ItemKey> =
            args.item_keys.into_iter().map(Into::into).collect();
        Ok(crate::response::json_result(client.get_citekeys(&item_keys).await))
    }

    /// Regenerates Better `BibTeX` citation keys using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if citekey regeneration fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_bibtex_regenerate_citekeys_impl(
        &self,
        args: RegenerateKeysArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let citekeys: Vec<CitationKey> =
            args.citekeys.into_iter().map(Into::into).collect();
        match client.regenerate_keys(&citekeys).await {
            Ok(_) => Ok(crate::response::text_success(
                "Citation keys regenerated successfully",
            )),
            Err(e) => Ok(crate::response::text_error(&e)),
        }
    }

    /// Exports citekeys in the requested translator format using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if item export fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_bibtex_export_items_impl(
        &self,
        args: ExportItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let citekeys: Vec<CitationKey> =
            args.citekeys.into_iter().map(Into::into).collect();
        let translator = TranslatorName::from(args.translator);
        Ok(crate::response::text_result(
            client.export_items(&citekeys, &translator).await,
        ))
    }

    /// Formats a bibliography from citation keys using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if bibliography formatting fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_bibtex_format_bibliography_impl(
        &self,
        args: BibliographyArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let citekeys: Vec<CitationKey> =
            args.citekeys.into_iter().map(Into::into).collect();
        let format = args.format.map(Into::into);
        Ok(crate::response::text_result(
            client.bibliography(&citekeys, format.as_ref()).await,
        ))
    }

    /// Imports references from a `LaTeX` `.aux` file using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if `.aux` file scanning fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_bibtex_scan_aux_impl(
        &self,
        args: ScanAuxArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let collection = args.collection.map_or_else(
            CollectionPath::personal_library,
            CollectionPath::from,
        );
        let aux_path = AuxFilePath::from(args.aux_path);
        Ok(crate::response::json_result(
            client.scan_aux(&collection, &aux_path).await,
        ))
    }

    /// Processes citation keys through the Better `BibTeX` Pandoc filter using
    /// `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if Pandoc filter processing fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_bibtex_pandoc_filter_impl(
        &self,
        args: PandocFilterArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let citekeys: Vec<CitationKey> =
            args.citekeys.into_iter().map(Into::into).collect();
        Ok(crate::response::json_result(
            client.pandoc_filter(&citekeys, true).await,
        ))
    }

    /// Registers a Better `BibTeX` auto-export target using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if auto-export configuration fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_bibtex_autoexport_add_impl(
        &self,
        args: AutoExportAddArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        match client.autoexport_add(&args.into()).await {
            Ok(_) => Ok(crate::response::text_success(
                "Auto-export configured successfully",
            )),
            Err(e) => Ok(crate::response::text_error(&e)),
        }
    }

    /// Searches Better `BibTeX` items using `args`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if search fails at the protocol level
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn better_bibtex_search_impl(
        &self,
        args: BetterBibtexSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        Ok(crate::response::json_result(
            client.search(&SearchQuery::from(args.query)).await,
        ))
    }
}

/// Commands dispatched by the `better_bibtex` MCP tool router.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum BetterBibtexCommand {
    /// Retrieve citation keys for items.
    Citekeys(GetCitekeysArgs),
    /// Regenerate citation keys for items.
    Regenerate(RegenerateKeysArgs),
    /// Export items in BibTeX/BibLaTeX format.
    Export(ExportItemsArgs),
    /// Format a bibliography from citation keys.
    Bibliography(BibliographyArgs),
    /// Import references from a `LaTeX` .aux file.
    ScanAux(ScanAuxArgs),
    /// Process citation keys through the Pandoc filter.
    PandocFilter(PandocFilterArgs),
    /// Configure an auto-export target.
    AutoexportAdd(AutoExportAddArgs),
    /// Search items by query.
    Search(BetterBibtexSearchArgs),
}

#[tool_router(router = better_bibtex_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "better_bibtex",
        description = "Grouped Better BibTeX router. action: citekeys, \
                       regenerate, export, bibliography, scan_aux, \
                       pandoc_filter, autoexport_add, search",
        annotations(
            title = "Better BibTeX",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn better_bibtex(
        &self,
        Parameters(args): Parameters<BetterBibtexCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            BetterBibtexCommand::Citekeys(args) => {
                self.better_bibtex_get_citekeys_impl(args).await
            }
            BetterBibtexCommand::Regenerate(args) => {
                self.better_bibtex_regenerate_citekeys_impl(args).await
            }
            BetterBibtexCommand::Export(args) => {
                self.better_bibtex_export_items_impl(args).await
            }
            BetterBibtexCommand::Bibliography(args) => {
                self.better_bibtex_format_bibliography_impl(args).await
            }
            BetterBibtexCommand::ScanAux(args) => {
                self.better_bibtex_scan_aux_impl(args).await
            }
            BetterBibtexCommand::PandocFilter(args) => {
                self.better_bibtex_pandoc_filter_impl(args).await
            }
            BetterBibtexCommand::AutoexportAdd(args) => {
                self.better_bibtex_autoexport_add_impl(args).await
            }
            BetterBibtexCommand::Search(args) => {
                self.better_bibtex_search_impl(args).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zotero_api::AppState;

    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        pub(super) fn better_bibtex_state(
            better_bibtex_url: String,
        ) -> AppState {
            AppState::test_default()
                .with_better_bibtex_url(better_bibtex_url)
                .with_write_enabled(true)
        }

        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }
    }

    use fixtures::*;

    mod citekeys {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn gets_citekeys_for_item_keys() {
            // Arrange
            let body = json!({
                "jsonrpc": "2.0",
                "result": { "ITEM1": "citekey1" }
            });
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(better_bibtex_state(base));

            // Act
            let res = server
                .better_bibtex_get_citekeys_impl(GetCitekeysArgs {
                    item_keys: vec!["ITEM1".into()],
                })
                .await
                .expect("get citekeys ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }

        #[tokio::test]
        async fn regenerates_citekeys_for_keys() {
            // Arrange
            let body = json!({
                "jsonrpc": "2.0",
                "result": { "KEY1": "newkey1" }
            });
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(better_bibtex_state(base));

            // Act
            let res = server
                .better_bibtex_regenerate_citekeys_impl(RegenerateKeysArgs {
                    citekeys: vec!["KEY1".into()],
                })
                .await
                .expect("regenerate ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    mod export {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn exports_items_to_requested_translator() {
            // Arrange
            let body = json!({
                "jsonrpc": "2.0",
                "result": "@article{foo, author={Smith}}"
            });
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(better_bibtex_state(base));

            // Act
            let res = server
                .better_bibtex_export_items_impl(ExportItemsArgs {
                    citekeys: vec!["foo".into()],
                    translator: "Better BibLaTeX".to_owned(),
                })
                .await
                .expect("export ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }

        #[tokio::test]
        async fn formats_bibliography_from_citekeys() {
            // Arrange
            let body = json!({
                "jsonrpc": "2.0",
                "result": "Formatted Bibliography"
            });
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(better_bibtex_state(base));

            // Act
            let res = server
                .better_bibtex_format_bibliography_impl(BibliographyArgs {
                    citekeys: vec!["foo".into()],
                    format: None,
                })
                .await
                .expect("bibliography ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    mod search {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn searches_library_with_query() {
            // Arrange
            let body = json!({
                "jsonrpc": "2.0",
                "result": [{ "citekey": "foo", "title": "Test" }]
            });
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let server = ZoteroMcpServer::new(better_bibtex_state(base));

            // Act
            let res = server
                .better_bibtex_search_impl(BetterBibtexSearchArgs {
                    query: "Smith".to_owned(),
                })
                .await
                .expect("search ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    /// Reverse-exhaustive match on `zotero_api::BibliographyContentType`: if
    /// a variant is added there, this fails to compile until
    /// `BibliographyContentTypeArg` (and its `From` impl above) is updated
    /// too, catching schema drift a one-directional match cannot.
    mod arg_mirrors {
        use super::*;

        #[test]
        fn bibliography_content_type_arg_covers_every_variant() {
            fn to_arg(
                content_type: zotero_api::BibliographyContentType,
            ) -> BibliographyContentTypeArg {
                match content_type {
                    zotero_api::BibliographyContentType::Html => {
                        BibliographyContentTypeArg::Html
                    }
                    zotero_api::BibliographyContentType::Text => {
                        BibliographyContentTypeArg::Text
                    }
                }
            }
            let _ = to_arg;
        }
    }
}
