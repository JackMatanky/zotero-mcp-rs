//! MCP tool handlers and argument models for Better `BibTeX` integration.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    better_bibtex::BetterBibtexClient,
    zotero::{CitationKey, CollectionKey, ItemKey},
};

// --- Argument Schemas ---

/// Arguments for `better_bibtex_get_citekeys`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCitekeysArgs {
    /// Zotero item keys to look up.
    pub(crate) item_keys: Vec<ItemKey>,
}

/// Arguments for `better_bibtex_regenerate_citekeys`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RegenerateKeysArgs {
    /// Zotero item keys to regenerate citation keys for.
    pub(crate) item_keys: Vec<ItemKey>,
}

/// Arguments for `better_bibtex_export_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ExportItemsArgs {
    /// Zotero item keys to export.
    pub(crate) item_keys: Vec<ItemKey>,
    /// Translator format string (e.g. `"bibtex"`, `"biblatex"`, `"csljson"`).
    pub(crate) translator: String,
}

/// Arguments for `better_bibtex_format_bibliography`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BibliographyArgs {
    /// Citation keys to format.
    pub(crate) citekeys: Vec<CitationKey>,
    /// Optional CSL style string (e.g. `"apa"`, `"ieee"`).
    pub(crate) style: Option<String>,
}

/// Arguments for `better_bibtex_scan_aux`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ScanAuxArgs {
    /// Target Zotero collection key to import references into.
    pub(crate) collection_key: Option<CollectionKey>,
    /// Path to the `LaTeX` `.aux` file.
    pub(crate) aux_path: String,
}

/// Arguments for `better_bibtex_pandoc_filter`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct PandocFilterArgs {
    /// Citation keys to filter.
    pub(crate) citekeys: Vec<CitationKey>,
}

/// Arguments for `better_bibtex_autoexport_add`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AutoexportAddArgs {
    /// Zotero collection key or library ID.
    pub(crate) collection_key: CollectionKey,
    /// Destination export file path.
    pub(crate) path: String,
    /// Format translator string (e.g. `"bibtex"`, `"biblatex"`).
    pub(crate) translator: String,
}

/// Arguments for `better_bibtex_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BetterBibtexSearchArgs {
    /// Search query string.
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
        let keys_str: Vec<&str> =
            args.item_keys.iter().map(ItemKey::as_str).collect();
        Ok(super::json_result(client.get_citekeys(&keys_str).await))
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
        let keys_str: Vec<&str> =
            args.item_keys.iter().map(ItemKey::as_str).collect();
        match client.regenerate_keys(&keys_str).await {
            Ok(_) => Ok(super::text_success(
                "Citation keys regenerated successfully",
            )),
            Err(e) => Ok(super::text_error(&e)),
        }
    }

    /// Exports Zotero items in the requested translator format using `args`.
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
        let keys_str: Vec<&str> =
            args.item_keys.iter().map(ItemKey::as_str).collect();
        Ok(super::text_result(
            client.export_items(&keys_str, &args.translator).await,
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
        let keys_str: Vec<&str> =
            args.citekeys.iter().map(CitationKey::as_str).collect();
        Ok(super::text_result(
            client.bibliography(&keys_str, args.style.as_deref(), None).await,
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
        let fallback = CollectionKey::from("");
        let col = args.collection_key.as_ref().unwrap_or(&fallback);
        Ok(super::json_result(client.scan_aux(col, &args.aux_path).await))
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
        let keys_str: Vec<&str> =
            args.citekeys.iter().map(CitationKey::as_str).collect();
        Ok(super::json_result(client.pandoc_filter(&keys_str, true).await))
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
        args: AutoexportAddArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        match client
            .autoexport_add(&args.collection_key, &args.translator, &args.path)
            .await
        {
            Ok(_) => {
                Ok(super::text_success("Autoexport configured successfully"))
            }
            Err(e) => Ok(super::text_error(&e)),
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
        Ok(super::json_result(client.search(&args.query).await))
    }
}
