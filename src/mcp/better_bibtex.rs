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

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

pub(crate) use crate::better_bibtex::AutoExportAddRequest as AutoExportAddArgs;
use crate::{
    ZoteroMcpServer,
    better_bibtex::{
        AuxFilePath, BetterBibtexClient, BibliographyFormat, CollectionPath,
        SearchQuery, TranslatorName,
    },
    zotero::{CitationKey, ItemKey},
};

// --- Argument Schemas ---

/// Arguments for the `better_bibtex_get_citekeys` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCitekeysArgs {
    /// Zotero item keys ([`ItemKey`]) to look up.
    pub(crate) item_keys: Vec<ItemKey>,
}

/// Arguments for the `better_bibtex_regenerate_citekeys` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RegenerateKeysArgs {
    /// Better `BibTeX` citation keys ([`CitationKey`]) to regenerate.
    pub(crate) citekeys: Vec<CitationKey>,
}

/// Arguments for the `better_bibtex_export_items` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ExportItemsArgs {
    /// Better `BibTeX` citation keys ([`CitationKey`]) to export.
    pub(crate) citekeys: Vec<CitationKey>,
    /// Translator name or GUID ([`TranslatorName`]).
    pub(crate) translator: TranslatorName,
}

/// Arguments for the `better_bibtex_format_bibliography` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BibliographyArgs {
    /// Citation keys ([`CitationKey`]) to format.
    pub(crate) citekeys: Vec<CitationKey>,
    /// Optional Better `BibTeX` bibliography format settings
    /// ([`BibliographyFormat`]).
    pub(crate) format: Option<BibliographyFormat>,
}

/// Arguments for the `better_bibtex_scan_aux` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ScanAuxArgs {
    /// Better `BibTeX` collection path ([`CollectionPath`]) to import
    /// references into. Defaults to `//`, the personal library root.
    pub(crate) collection: Option<CollectionPath>,
    /// Absolute path to the `LaTeX` `.aux` file ([`AuxFilePath`]).
    pub(crate) aux_path: AuxFilePath,
}

/// Arguments for the `better_bibtex_pandoc_filter` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct PandocFilterArgs {
    /// Citation keys ([`CitationKey`]) to filter.
    pub(crate) citekeys: Vec<CitationKey>,
}

/// Arguments for the `better_bibtex_search` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BetterBibtexSearchArgs {
    /// Better `BibTeX` quick-search query ([`SearchQuery`]).
    pub(crate) query: SearchQuery,
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
        Ok(super::json_result(client.get_citekeys(&args.item_keys).await))
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
        match client.regenerate_keys(&args.citekeys).await {
            Ok(_) => Ok(super::text_success(
                "Citation keys regenerated successfully",
            )),
            Err(e) => Ok(super::text_error(&e)),
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
        Ok(super::text_result(
            client.export_items(&args.citekeys, &args.translator).await,
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
        Ok(super::text_result(
            client.bibliography(&args.citekeys, args.format.as_ref()).await,
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
        let collection =
            args.collection.unwrap_or_else(CollectionPath::personal_library);
        Ok(super::json_result(
            client.scan_aux(&collection, &args.aux_path).await,
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
        Ok(super::json_result(client.pandoc_filter(&args.citekeys, true).await))
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
        match client.autoexport_add(&args).await {
            Ok(_) => {
                Ok(super::text_success("Auto-export configured successfully"))
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
