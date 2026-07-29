//! Wires every MCP tool to the Zotero, Better `BibTeX`, and Better Notes
//! clients.
//!
//! Each `#[tool(description = "...")]` attribute below *is* the
//! documentation surfaced to MCP clients, so individual tool methods
//! intentionally carry no separate `///` rustdoc — adding one would just
//! duplicate the `description` string.

use std::path::Path;

use rmcp::{
    ServerHandler,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, Content, Implementation, InitializeResult,
        ProtocolVersion, ServerCapabilities,
    },
    tool, tool_router,
};
use serde_json::json;

use crate::{
    better_bibtex::BetterBibtexClient,
    better_notes::BetterNotesClient,
    pdf::extract_pdf_pages,
    state::AppState,
    tools::models::{
        AutoexportAddArgs, BetterBibtexSearchArgs, BibliographyArgs,
        CreateNoteArgs, EmptyArgs, ExportItemsArgs, FromMarkdownArgs,
        GetCitekeysArgs, GetCollectionItemsArgs, GetItemArgs,
        GetItemChildrenArgs, GetItemFulltextArgs, GetItemMetadataArgs,
        GetNotesArgs, GetPdfPathArgs, GetRecentArgs, NoteRelationsArgs,
        NoteTreeArgs, PandocFilterArgs, ReadPdfPagesArgs, RegenerateKeysArgs,
        RunTemplateArgs, ScanAuxArgs, SearchItemsArgs, ToMarkdownArgs,
    },
    zotero::ZoteroClient,
};

/// The MCP tool router: holds the shared [`AppState`] and implements
/// [`ServerHandler`], hosting every `#[tool]` method below.
pub(crate) struct ZoteroMcpServer {
    pub(crate) state: AppState,
}

impl ZoteroMcpServer {
    pub(crate) fn new(state: AppState) -> Self {
        Self {
            state,
        }
    }
}

impl ServerHandler for ZoteroMcpServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "zotero-mcp-rs".to_owned(),
                version: "0.1.0".to_owned(),
                title: None,
                icons: None,
                website_url: None,
            },
            instructions: None,
        }
    }

    async fn list_tools(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: Self::tool_router().list_all(),
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        param: rmcp::model::CallToolRequestParam,
        context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let ctx = rmcp::handler::server::tool::ToolCallContext::new(
            self, param, context,
        );
        Self::tool_router().call(ctx).await
    }
}

#[tool_router]
impl ZoteroMcpServer {
    // --- Diagnostics & Status ---

    #[tool(
        name = "zotero_status",
        description = "Check diagnostic status of Zotero Local API, Better \
                       BibTeX, and Better Notes bridge"
    )]
    pub(crate) async fn zotero_status(
        &self,
        _params: Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let zotero_client = ZoteroClient::new(&self.state);
        let bbt_client = BetterBibtexClient::new(&self.state);
        let bn_client = BetterNotesClient::new(&self.state);

        let z_status = zotero_client.check_status().await;
        let bbt_status = bbt_client.check_status().await;
        let bn_status = bn_client.check_status().await;

        let status = json!({
            "write_enabled": self.state.write_enabled,
            "zotero_local_api": z_status,
            "better_bibtex": bbt_status,
            "better_notes_bridge": bn_status,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&status).unwrap_or_default(),
        )]))
    }

    // --- Core Zotero Tools ---

    #[tool(
        name = "zotero_get_recent",
        description = "Fetch most recently modified items from Zotero library"
    )]
    pub(crate) async fn zotero_get_recent(
        &self,
        Parameters(args): Parameters<GetRecentArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let limit = args.limit.unwrap_or(10).min(100);
        match client.get_recent_items(limit).await {
            Ok(items) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&items).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "zotero_search_items",
        description = "Search Zotero items by query across title, creators, \
                       year, or collection"
    )]
    pub(crate) async fn zotero_search_items(
        &self,
        Parameters(args): Parameters<SearchItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let limit = args.limit.unwrap_or(20);
        match client
            .search_items(&args.query, args.collection_key.as_deref(), limit)
            .await
        {
            Ok(items) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&items).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "zotero_get_item",
        description = "Fetch item details by Zotero item key"
    )]
    pub(crate) async fn zotero_get_item(
        &self,
        Parameters(args): Parameters<GetItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item(&args.item_key).await {
            Ok(item) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&item).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "zotero_get_item_metadata",
        description = "Get metadata for an item as JSON or formatted BibTeX \
                       string"
    )]
    pub(crate) async fn zotero_get_item_metadata(
        &self,
        Parameters(args): Parameters<GetItemMetadataArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let format = args.format.as_deref().unwrap_or("json");
        if format.eq_ignore_ascii_case("bibtex") {
            let bbt_client = BetterBibtexClient::new(&self.state);
            let item_keys = vec![args.item_key.as_str()];
            match bbt_client.export_items(&item_keys, "Better BibTeX").await {
                Ok(bibtex) => {
                    Ok(CallToolResult::success(vec![Content::text(bibtex)]))
                }
                Err(e) => Ok(CallToolResult::error(vec![Content::text(
                    e.to_string(),
                )])),
            }
        } else {
            let client = ZoteroClient::new(&self.state);
            match client.get_item(&args.item_key).await {
                Ok(item) => Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&item.data)
                        .unwrap_or_default(),
                )])),
                Err(e) => Ok(CallToolResult::error(vec![Content::text(
                    e.to_string(),
                )])),
            }
        }
    }

    #[tool(
        name = "zotero_get_collections",
        description = "Get list of all Zotero collections in library"
    )]
    pub(crate) async fn zotero_get_collections(
        &self,
        _params: Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_collections().await {
            Ok(collections) => {
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&collections)
                        .unwrap_or_default(),
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "zotero_get_collection_items",
        description = "Fetch items inside a specific Zotero collection"
    )]
    pub(crate) async fn zotero_get_collection_items(
        &self,
        Parameters(args): Parameters<GetCollectionItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_collection_items(&args.collection_key).await {
            Ok(items) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&items).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "zotero_get_item_children",
        description = "Get child items (notes, attachments) for a given item \
                       key"
    )]
    pub(crate) async fn zotero_get_item_children(
        &self,
        Parameters(args): Parameters<GetItemChildrenArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item_children(&args.item_key).await {
            Ok(children) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&children).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "zotero_get_item_fulltext",
        description = "Fetch Zotero indexed attachment text for an item key"
    )]
    pub(crate) async fn zotero_get_item_fulltext(
        &self,
        Parameters(args): Parameters<GetItemFulltextArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client.get_item_fulltext(&args.item_key).await {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "zotero_get_pdf_path",
        description = "Resolve absolute local PDF file path for an attachment \
                       item or parent item"
    )]
    pub(crate) async fn zotero_get_pdf_path(
        &self,
        Parameters(args): Parameters<GetPdfPathArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let item = match client.get_item(&args.item_key).await {
            Ok(it) => it,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(
                    e.to_string(),
                )]));
            }
        };

        if item.data.item_type == "attachment" {
            if let Some(path) = item.data.path {
                return Ok(CallToolResult::success(vec![Content::text(path)]));
            }
        }

        // Try getting children for parent item
        match client.get_item_children(&args.item_key).await {
            Ok(children) => {
                for child in children {
                    if child.data.item_type == "attachment" {
                        if let Some(ct) = &child.data.content_type {
                            if ct.contains("pdf") {
                                if let Some(p) = child.data.path {
                                    return Ok(CallToolResult::success(vec![
                                        Content::text(p),
                                    ]));
                                }
                            }
                        }
                    }
                }
                Ok(CallToolResult::error(vec![Content::text(
                    "No local PDF path found for item",
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "zotero_read_pdf_pages",
        description = "Extract exact page ranges from a PDF file path using \
                       local PDF reader"
    )]
    pub(crate) async fn zotero_read_pdf_pages(
        &self,
        Parameters(args): Parameters<ReadPdfPagesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let target_path = if args.item_key_or_path.starts_with('/') {
            args.item_key_or_path.clone()
        } else {
            let client = ZoteroClient::new(&self.state);
            match client.get_item(&args.item_key_or_path).await {
                Ok(item) => {
                    let attachment_path = if item.data.item_type == "attachment"
                    {
                        item.data.path
                    } else {
                        None
                    };
                    if let Some(path) = attachment_path {
                        path
                    } else {
                        match client
                            .get_item_children(&args.item_key_or_path)
                            .await
                        {
                            Ok(children) => {
                                let mut pdf_p = None;
                                for c in children {
                                    if c.data.item_type == "attachment" {
                                        if let Some(p) = c.data.path {
                                            if p.to_lowercase()
                                                .ends_with(".pdf")
                                            {
                                                pdf_p = Some(p);
                                                break;
                                            }
                                        }
                                    }
                                }
                                match pdf_p {
                                    Some(p) => p,
                                    None => {
                                        return Ok(CallToolResult::error(
                                            vec![Content::text(
                                                "PDF attachment path not \
                                             found for item",
                                            )],
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                return Ok(CallToolResult::error(vec![
                                    Content::text(e.to_string()),
                                ]));
                            }
                        }
                    }
                }
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        e.to_string(),
                    )]));
                }
            }
        };

        let pages = args.pages;
        let res = tokio::task::spawn_blocking(move || {
            let path_obj = Path::new(&target_path);
            extract_pdf_pages(path_obj, pages.as_deref())
        })
        .await;

        match res {
            Ok(Ok(text)) => {
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Ok(Err(e)) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
            Err(join_err) => Ok(CallToolResult::error(vec![Content::text(
                join_err.to_string(),
            )])),
        }
    }

    #[tool(
        name = "zotero_get_notes",
        description = "Fetch notes for an item key (formatted via Better \
                       Notes if bridge available)"
    )]
    pub(crate) async fn zotero_get_notes(
        &self,
        Parameters(args): Parameters<GetNotesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let children = match client.get_item_children(&args.item_key).await {
            Ok(ch) => ch,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(
                    e.to_string(),
                )]));
            }
        };

        let notes: Vec<_> = children
            .into_iter()
            .filter(|c| c.data.item_type == "note")
            .collect();
        let bn_client = BetterNotesClient::new(&self.state);

        let mut output_notes = Vec::new();
        for note_item in notes {
            let note_key = note_item.key.clone();
            let note_html = note_item.data.note.unwrap_or_default();
            let markdown =
                match bn_client.to_markdown(Some(&note_key), None).await {
                    Ok(md) => md,
                    Err(_) => note_html.clone(),
                };
            output_notes.push(json!({
                "key": note_key,
                "html": note_html,
                "markdown": markdown,
            }));
        }

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output_notes).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "zotero_create_note",
        description = "Create a note attached to a parent item (requires \
                       write permission)"
    )]
    pub(crate) async fn zotero_create_note(
        &self,
        Parameters(args): Parameters<CreateNoteArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client
            .create_note(&args.parent_item_key, &args.note_content)
            .await
        {
            Ok(item) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&item).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    // --- Better BibTeX Tools ---

    #[tool(
        name = "better_bibtex_status",
        description = "Check Better BibTeX JSON-RPC API readiness"
    )]
    pub(crate) async fn better_bibtex_status(
        &self,
        _params: Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        let status = bbt_client.check_status().await;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&status).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "better_bibtex_get_citekeys",
        description = "Map Zotero item keys to Better BibTeX citation keys"
    )]
    pub(crate) async fn better_bibtex_get_citekeys(
        &self,
        Parameters(args): Parameters<GetCitekeysArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        let keys_ref: Vec<&str> =
            args.item_keys.iter().map(String::as_str).collect();
        match bbt_client.get_citekeys(&keys_ref).await {
            Ok(map) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&map).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_bibtex_export_items",
        description = "Export items by keys/citekeys using format: Better \
                       BibTeX, Better BibLaTeX, or CSL JSON"
    )]
    pub(crate) async fn better_bibtex_export_items(
        &self,
        Parameters(args): Parameters<ExportItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        let keys_ref: Vec<&str> =
            args.item_keys.iter().map(String::as_str).collect();
        let translator = args.translator.as_deref().unwrap_or("Better BibTeX");
        match bbt_client.export_items(&keys_ref, translator).await {
            Ok(exported) => {
                Ok(CallToolResult::success(vec![Content::text(exported)]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_bibtex_bibliography",
        description = "Generate formatted bibliography for citation keys"
    )]
    pub(crate) async fn better_bibtex_bibliography(
        &self,
        Parameters(args): Parameters<BibliographyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        let keys_ref: Vec<&str> =
            args.item_keys.iter().map(String::as_str).collect();
        match bbt_client
            .bibliography(
                &keys_ref,
                args.style.as_deref(),
                args.locale.as_deref(),
            )
            .await
        {
            Ok(bib) => Ok(CallToolResult::success(vec![Content::text(bib)])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_bibtex_search",
        description = "High-precision search using Better BibTeX search engine"
    )]
    pub(crate) async fn better_bibtex_search(
        &self,
        Parameters(args): Parameters<BetterBibtexSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        match bbt_client.search(&args.query).await {
            Ok(res) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&res).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_bibtex_pandoc_filter",
        description = "Get Pandoc citeproc filter metadata for items"
    )]
    pub(crate) async fn better_bibtex_pandoc_filter(
        &self,
        Parameters(args): Parameters<PandocFilterArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        let keys_ref: Vec<&str> =
            args.item_keys.iter().map(String::as_str).collect();
        let as_csl = args.as_csl.unwrap_or(true);
        match bbt_client.pandoc_filter(&keys_ref, as_csl).await {
            Ok(res) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&res).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_bibtex_regenerate_keys",
        description = "Regenerate citekeys for items (requires write \
                       permission)"
    )]
    pub(crate) async fn better_bibtex_regenerate_keys(
        &self,
        Parameters(args): Parameters<RegenerateKeysArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        let keys_ref: Vec<&str> =
            args.item_keys.iter().map(String::as_str).collect();
        match bbt_client.regenerate_keys(&keys_ref).await {
            Ok(res) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&res).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_bibtex_autoexport_add",
        description = "Add an auto-export job for a collection (requires \
                       write permission)"
    )]
    pub(crate) async fn better_bibtex_autoexport_add(
        &self,
        Parameters(args): Parameters<AutoexportAddArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        match bbt_client
            .autoexport_add(&args.collection_key, &args.translator, &args.path)
            .await
        {
            Ok(res) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&res).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_bibtex_scan_aux",
        description = "Scan LaTeX .aux file to import citations into a \
                       collection (requires write permission)"
    )]
    pub(crate) async fn better_bibtex_scan_aux(
        &self,
        Parameters(args): Parameters<ScanAuxArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bbt_client = BetterBibtexClient::new(&self.state);
        match bbt_client.scan_aux(&args.collection_key, &args.aux_path).await {
            Ok(res) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&res).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    // --- Better Notes Tools ---

    #[tool(
        name = "better_notes_status",
        description = "Check Better Notes bridge plugin status"
    )]
    pub(crate) async fn better_notes_status(
        &self,
        _params: Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bn_client = BetterNotesClient::new(&self.state);
        let status = bn_client.check_status().await;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&status).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "better_notes_to_markdown",
        description = "Convert Zotero note HTML into clean Better Notes \
                       Markdown format"
    )]
    pub(crate) async fn better_notes_to_markdown(
        &self,
        Parameters(args): Parameters<ToMarkdownArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bn_client = BetterNotesClient::new(&self.state);
        match bn_client
            .to_markdown(args.item_key.as_deref(), args.html.as_deref())
            .await
        {
            Ok(md) => Ok(CallToolResult::success(vec![Content::text(md)])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_notes_from_markdown",
        description = "Create a Zotero note from Markdown content via Better \
                       Notes parser (requires write permission)"
    )]
    pub(crate) async fn better_notes_from_markdown(
        &self,
        Parameters(args): Parameters<FromMarkdownArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bn_client = BetterNotesClient::new(&self.state);
        match bn_client
            .convert_from_markdown(&args.parent_key, &args.markdown)
            .await
        {
            Ok(key) => Ok(CallToolResult::success(vec![Content::text(key)])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_notes_run_template",
        description = "Run a named Better Notes template on an item"
    )]
    pub(crate) async fn better_notes_run_template(
        &self,
        Parameters(args): Parameters<RunTemplateArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bn_client = BetterNotesClient::new(&self.state);
        match bn_client.run_template(&args.name, &args.item_key).await {
            Ok(res) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&res).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_notes_get_relations",
        description = "Fetch outlinks, backlinks, and graph relations for a \
                       note"
    )]
    pub(crate) async fn better_notes_get_relations(
        &self,
        Parameters(args): Parameters<NoteRelationsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bn_client = BetterNotesClient::new(&self.state);
        match bn_client.get_relations(&args.item_key).await {
            Ok(res) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&res).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }

    #[tool(
        name = "better_notes_get_tree",
        description = "Retrieve the full Better Notes hierarchy tree for a \
                       note"
    )]
    pub(crate) async fn better_notes_get_tree(
        &self,
        Parameters(args): Parameters<NoteTreeArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bn_client = BetterNotesClient::new(&self.state);
        match bn_client.get_tree(&args.item_key).await {
            Ok(res) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&res).unwrap_or_default(),
            )])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use reqwest::Client;
        use rmcp::model::RawContent;

        use super::{AppState, CallToolResult};

        /// Builds an [`AppState`] pointing each backend URL at its own
        /// fixture server (or `String::new()` when a test doesn't need
        /// that backend).
        pub(super) fn test_state(
            zotero_api_url: String,
            better_bibtex_url: String,
            better_notes_url: String,
        ) -> AppState {
            AppState {
                client: Client::new(),
                zotero_api_url,
                better_bibtex_url,
                better_notes_url,
                write_enabled: false,
            }
        }

        /// [`test_state`] with only `zotero_api_url` set.
        pub(super) fn zotero_state(zotero_api_url: String) -> AppState {
            test_state(zotero_api_url, String::new(), String::new())
        }

        /// Formats a minimal raw HTTP/1.1 response with `status` (e.g.
        /// `"200 OK"`) and a JSON `body`, computing `Content-Length`
        /// automatically.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: \
                 close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Spawns a background thread serving one canned raw HTTP response
        /// (see [`http_response`]) per accepted connection, in order.
        /// Returns the bound `http://host:port` base URL, standing in for
        /// a Zotero/Better BibTeX/Better Notes backend.
        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                let mut it = responses.into_iter();
                while let (Some(resp), Ok((mut stream, _))) =
                    (it.next(), listener.accept())
                {
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(resp.as_bytes());
                }
            });
            format!("http://{addr}")
        }

        /// Extracts the text of the first content block in a
        /// [`CallToolResult`].
        ///
        /// # Panics
        ///
        /// Panics if `result` has no content blocks or the first isn't
        /// text — acceptable in test-only code asserting a specific tool
        /// response shape.
        pub(super) fn result_text(result: &CallToolResult) -> &str {
            if let Some(content) = result.content.first() {
                if let RawContent::Text(ref t) = content.raw {
                    return &t.text;
                }
            }
            ""
        }
    }

    mod get_pdf_path {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, result_text, zotero_state},
        };

        #[tokio::test]
        async fn returns_direct_path_for_an_attachment_item() {
            // Arrange
            let item = json!({
                "key": "ATT1",
                "version": 1,
                "data": {
                    "key": "ATT1",
                    "version": 1,
                    "itemType": "attachment",
                    "path": "/tmp/file.pdf"
                }
            });
            let base =
                mock_server(vec![http_response("200 OK", &item.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let result = server
                .zotero_get_pdf_path(Parameters(GetPdfPathArgs {
                    item_key: "ATT1".to_owned(),
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result.is_error, Some(false));
            assert_eq!(result_text(&result), "/tmp/file.pdf");
        }

        #[tokio::test]
        async fn finds_pdf_attachment_among_parent_items_children() {
            // Arrange
            let parent = json!({
                "key": "PARENT1",
                "version": 1,
                "data": { "key": "PARENT1", "version": 1, "itemType": "journalArticle" }
            });
            let children = json!([
                {
                    "key": "NOTE1",
                    "version": 1,
                    "data": { "key": "NOTE1", "version": 1, "itemType": "note" }
                },
                {
                    "key": "ATT2",
                    "version": 1,
                    "data": {
                        "key": "ATT2",
                        "version": 1,
                        "itemType": "attachment",
                        "contentType": "application/pdf",
                        "path": "/tmp/child.pdf"
                    }
                }
            ]);
            let base = mock_server(vec![
                http_response("200 OK", &parent.to_string()),
                http_response("200 OK", &children.to_string()),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let result = server
                .zotero_get_pdf_path(Parameters(GetPdfPathArgs {
                    item_key: "PARENT1".to_owned(),
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result.is_error, Some(false));
            assert_eq!(result_text(&result), "/tmp/child.pdf");
        }

        #[tokio::test]
        async fn errors_when_no_pdf_attachment_is_found() {
            // Arrange
            let parent = json!({
                "key": "PARENT1",
                "version": 1,
                "data": { "key": "PARENT1", "version": 1, "itemType": "journalArticle" }
            });
            let children = json!([{
                "key": "NOTE1",
                "version": 1,
                "data": { "key": "NOTE1", "version": 1, "itemType": "note" }
            }]);
            let base = mock_server(vec![
                http_response("200 OK", &parent.to_string()),
                http_response("200 OK", &children.to_string()),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let result = server
                .zotero_get_pdf_path(Parameters(GetPdfPathArgs {
                    item_key: "PARENT1".to_owned(),
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result.is_error, Some(true));
            assert_eq!(
                result_text(&result),
                "No local PDF path found for item"
            );
        }

        #[tokio::test]
        async fn propagates_the_item_lookup_error() {
            // Arrange
            let base = mock_server(vec![http_response("404 Not Found", "")]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let result = server
                .zotero_get_pdf_path(Parameters(GetPdfPathArgs {
                    item_key: "MISSING".to_owned(),
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result.is_error, Some(true));
            assert!(result_text(&result).contains("MISSING"));
        }
    }

    mod read_pdf_pages {
        use pretty_assertions::assert_eq;
        use serde_json::json;

        use super::{
            super::*,
            fixtures::{http_response, mock_server, result_text, zotero_state},
        };

        #[tokio::test]
        async fn skips_zotero_lookup_for_an_absolute_path() {
            // Arrange: no mock server — an absolute path never hits the
            // Zotero API.
            let server = ZoteroMcpServer::new(zotero_state(String::new()));

            // Act
            let result = server
                .zotero_read_pdf_pages(Parameters(ReadPdfPagesArgs {
                    item_key_or_path: "/nonexistent/direct.pdf".to_owned(),
                    pages: None,
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result.is_error, Some(true));
            assert!(result_text(&result).contains("/nonexistent/direct.pdf"));
        }

        #[tokio::test]
        async fn resolves_an_attachment_key_to_its_path() {
            // Arrange
            let item = json!({
                "key": "ATT1",
                "version": 1,
                "data": {
                    "key": "ATT1",
                    "version": 1,
                    "itemType": "attachment",
                    "path": "/nonexistent/att.pdf"
                }
            });
            let base =
                mock_server(vec![http_response("200 OK", &item.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let result = server
                .zotero_read_pdf_pages(Parameters(ReadPdfPagesArgs {
                    item_key_or_path: "ATT1".to_owned(),
                    pages: None,
                }))
                .await
                .unwrap();

            // Assert: resolution succeeded (reached the extraction stage
            // against the resolved path); the file itself doesn't exist,
            // which is expected.
            assert_eq!(result.is_error, Some(true));
            assert!(result_text(&result).contains("/nonexistent/att.pdf"));
        }

        #[tokio::test]
        async fn resolves_a_parent_key_to_a_pdf_child_case_insensitively() {
            // Arrange
            let parent = json!({
                "key": "PARENT1",
                "version": 1,
                "data": { "key": "PARENT1", "version": 1, "itemType": "journalArticle" }
            });
            let children = json!([{
                "key": "ATT3",
                "version": 1,
                "data": {
                    "key": "ATT3",
                    "version": 1,
                    "itemType": "attachment",
                    "path": "/nonexistent/child.PDF"
                }
            }]);
            let base = mock_server(vec![
                http_response("200 OK", &parent.to_string()),
                http_response("200 OK", &children.to_string()),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let result = server
                .zotero_read_pdf_pages(Parameters(ReadPdfPagesArgs {
                    item_key_or_path: "PARENT1".to_owned(),
                    pages: None,
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result.is_error, Some(true));
            assert!(result_text(&result).contains("/nonexistent/child.PDF"));
        }

        #[tokio::test]
        async fn errors_when_parent_has_no_pdf_child() {
            // Arrange
            let parent = json!({
                "key": "PARENT1",
                "version": 1,
                "data": { "key": "PARENT1", "version": 1, "itemType": "journalArticle" }
            });
            let children = json!([{
                "key": "NOTE1",
                "version": 1,
                "data": { "key": "NOTE1", "version": 1, "itemType": "note" }
            }]);
            let base = mock_server(vec![
                http_response("200 OK", &parent.to_string()),
                http_response("200 OK", &children.to_string()),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let result = server
                .zotero_read_pdf_pages(Parameters(ReadPdfPagesArgs {
                    item_key_or_path: "PARENT1".to_owned(),
                    pages: None,
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result.is_error, Some(true));
            assert_eq!(
                result_text(&result),
                "PDF attachment path not found for item"
            );
        }
    }

    mod get_item_metadata {
        use pretty_assertions::assert_eq;
        use serde_json::{Value, json};

        use super::{
            super::*,
            fixtures::{http_response, mock_server, result_text, test_state},
        };

        #[tokio::test]
        async fn returns_item_data_for_the_default_json_format() {
            // Arrange
            let item = json!({
                "key": "ITEM1",
                "version": 3,
                "data": {
                    "key": "ITEM1",
                    "version": 3,
                    "itemType": "book",
                    "title": "My Book"
                }
            });
            let base =
                mock_server(vec![http_response("200 OK", &item.to_string())]);
            let server = ZoteroMcpServer::new(test_state(
                base,
                String::new(),
                String::new(),
            ));

            // Act
            let result = server
                .zotero_get_item_metadata(Parameters(GetItemMetadataArgs {
                    item_key: "ITEM1".to_owned(),
                    format: None,
                }))
                .await
                .unwrap();

            // Assert: item.data has no nested "data" field; the full
            // ZoteroItem wrapper does, so this distinguishes the two shapes.
            let parsed: Value =
                serde_json::from_str(result_text(&result)).unwrap();
            assert!(parsed.get("data").is_none());
            assert_eq!(parsed.get("title"), Some(&json!("My Book")));
        }

        #[tokio::test]
        async fn exports_via_better_bibtex_for_the_bibtex_format() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"jsonrpc":"2.0","result":"@book{foo,}"}"#,
            )]);
            let server = ZoteroMcpServer::new(test_state(
                String::new(),
                base,
                String::new(),
            ));

            // Act
            let result = server
                .zotero_get_item_metadata(Parameters(GetItemMetadataArgs {
                    item_key: "ITEM1".to_owned(),
                    format: Some("bibtex".to_owned()),
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result_text(&result), "@book{foo,}");
        }

        #[tokio::test]
        async fn treats_the_bibtex_format_as_case_insensitive() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                r#"{"jsonrpc":"2.0","result":"@book{foo,}"}"#,
            )]);
            let server = ZoteroMcpServer::new(test_state(
                String::new(),
                base,
                String::new(),
            ));

            // Act
            let result = server
                .zotero_get_item_metadata(Parameters(GetItemMetadataArgs {
                    item_key: "ITEM1".to_owned(),
                    format: Some("BibTeX".to_owned()),
                }))
                .await
                .unwrap();

            // Assert
            assert_eq!(result_text(&result), "@book{foo,}");
        }
    }

    mod get_notes {
        use pretty_assertions::assert_eq;
        use serde_json::{Value, json};

        use super::{
            super::*,
            fixtures::{http_response, mock_server, result_text, test_state},
        };

        /// One note child (with HTML body) and one non-note attachment
        /// child, as returned by `GET /items/{key}/children`.
        fn note_and_attachment_children() -> Value {
            json!([
                {
                    "key": "NOTE1",
                    "version": 1,
                    "data": {
                        "key": "NOTE1",
                        "version": 1,
                        "itemType": "note",
                        "note": "<p>Hello</p>"
                    }
                },
                {
                    "key": "ATT1",
                    "version": 1,
                    "data": { "key": "ATT1", "version": 1, "itemType": "attachment" }
                }
            ])
        }

        #[tokio::test]
        async fn filters_to_notes_and_falls_back_to_html_when_bridge_is_down() {
            // Arrange
            let base = mock_server(vec![http_response(
                "200 OK",
                &note_and_attachment_children().to_string(),
            )]);
            // Port 0 is never a live listener: to_markdown always fails.
            let state = test_state(
                base,
                String::new(),
                "http://127.0.0.1:0".to_owned(),
            );
            let server = ZoteroMcpServer::new(state);

            // Act
            let result = server
                .zotero_get_notes(Parameters(GetNotesArgs {
                    item_key: "PARENT1".to_owned(),
                }))
                .await
                .unwrap();

            // Assert
            let parsed: Vec<Value> =
                serde_json::from_str(result_text(&result)).unwrap();
            assert_eq!(
                parsed.len(),
                1,
                "attachment child must be filtered out"
            );
            let first = parsed.first().expect("parsed has element");
            assert_eq!(first["key"], "NOTE1");
            assert_eq!(first["html"], "<p>Hello</p>");
            assert_eq!(first["markdown"], "<p>Hello</p>");
        }

        #[tokio::test]
        async fn uses_better_notes_markdown_when_the_bridge_is_available() {
            // Arrange
            let zotero_base = mock_server(vec![http_response(
                "200 OK",
                &note_and_attachment_children().to_string(),
            )]);
            let bn_base = mock_server(vec![http_response(
                "200 OK",
                r#"{"markdown":"**Hello**"}"#,
            )]);
            let state = test_state(zotero_base, String::new(), bn_base);
            let server = ZoteroMcpServer::new(state);

            // Act
            let result = server
                .zotero_get_notes(Parameters(GetNotesArgs {
                    item_key: "PARENT1".to_owned(),
                }))
                .await
                .unwrap();

            // Assert
            let parsed: Vec<Value> =
                serde_json::from_str(result_text(&result)).unwrap();
            let first = parsed.first().expect("parsed contains items");
            assert_eq!(first["markdown"], "**Hello**");
            assert_eq!(first["html"], "<p>Hello</p>");
        }
    }
}
