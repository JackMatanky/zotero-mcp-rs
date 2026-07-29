//! MCP tool handlers, argument models, and unit tests for Better `BibTeX` tools.

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ZoteroMcpServer, better_bibtex::BetterBibtexClient};

// --- Argument Schemas ---

/// Arguments for `better_bibtex_get_citekeys`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetCitekeysArgs {
    /// List of Zotero item keys
    pub(crate) item_keys: Vec<String>,
}

/// Arguments for `better_bibtex_regenerate_citekeys`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RegenerateKeysArgs {
    /// List of Zotero item keys to regenerate citekeys for
    pub(crate) item_keys: Vec<String>,
}

/// Arguments for `better_bibtex_export_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ExportItemsArgs {
    /// List of Zotero item keys
    pub(crate) item_keys: Vec<String>,
    /// Format string (e.g. "bibtex", "biblatex", "csljson")
    pub(crate) translator: String,
}

/// Arguments for `better_bibtex_format_bibliography`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BibliographyArgs {
    /// List of citation keys
    pub(crate) citekeys: Vec<String>,
    /// CSL style string (e.g. "apa", "ieee")
    pub(crate) style: Option<String>,
}

/// Arguments for `better_bibtex_scan_aux`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ScanAuxArgs {
    /// Target collection key to import references into
    pub(crate) collection_key: Option<String>,
    /// Path to the `LaTeX` .aux file
    pub(crate) aux_path: String,
}

/// Arguments for `better_bibtex_pandoc_filter`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct PandocFilterArgs {
    /// List of citation keys
    pub(crate) citekeys: Vec<String>,
}

/// Arguments for `better_bibtex_autoexport_add`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AutoexportAddArgs {
    /// Zotero collection key or library ID
    pub(crate) collection_key: String,
    /// Destination file path
    pub(crate) path: String,
    /// Format (e.g. "bibtex", "biblatex")
    pub(crate) translator: String,
}

/// Arguments for `better_bibtex_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BetterBibtexSearchArgs {
    /// Search query
    pub(crate) query: String,
}

// --- Handler Implementations ---

impl ZoteroMcpServer {
    pub(crate) async fn better_bibtex_get_citekeys_impl(
        &self,
        args: GetCitekeysArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let keys_str: Vec<&str> =
            args.item_keys.iter().map(String::as_str).collect();
        match client.get_citekeys(&keys_str).await {
            Ok(citekeys) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&citekeys).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_bibtex_regenerate_citekeys_impl(
        &self,
        args: RegenerateKeysArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let keys_str: Vec<&str> =
            args.item_keys.iter().map(String::as_str).collect();
        match client.regenerate_keys(&keys_str).await {
            Ok(_) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    "Citation keys regenerated successfully".to_owned(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_bibtex_export_items_impl(
        &self,
        args: ExportItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let keys_str: Vec<&str> =
            args.item_keys.iter().map(String::as_str).collect();
        match client.export_items(&keys_str, &args.translator).await {
            Ok(output) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    output,
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_bibtex_format_bibliography_impl(
        &self,
        args: BibliographyArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let keys_str: Vec<&str> =
            args.citekeys.iter().map(String::as_str).collect();
        match client.bibliography(&keys_str, args.style.as_deref(), None).await
        {
            Ok(bib) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    bib,
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_bibtex_scan_aux_impl(
        &self,
        args: ScanAuxArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let col = args.collection_key.as_deref().unwrap_or("");
        match client.scan_aux(col, &args.aux_path).await {
            Ok(keys) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&keys).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_bibtex_pandoc_filter_impl(
        &self,
        args: PandocFilterArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        let keys_str: Vec<&str> =
            args.citekeys.iter().map(String::as_str).collect();
        match client.pandoc_filter(&keys_str, true).await {
            Ok(output) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&output).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

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
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    "Autoexport configured successfully".to_owned(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }

    pub(crate) async fn better_bibtex_search_impl(
        &self,
        args: BetterBibtexSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = BetterBibtexClient::new(&self.state);
        match client.search(&args.query).await {
            Ok(results) => {
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    serde_json::to_string_pretty(&results).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    e.to_string(),
                )]))
            }
        }
    }
}
