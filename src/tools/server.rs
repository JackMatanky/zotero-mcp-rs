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
    #[expect(
        dead_code,
        reason = "State accessed dynamically by tool router methods"
    )]
    pub(crate) state: AppState,
}

impl ZoteroMcpServer {
    pub(crate) fn new(state: AppState) -> Self {
        Self {
            state,
        }
    }
}

#[async_trait::async_trait]
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
}

#[tool_router]
impl ZoteroMcpServer {
    // --- Diagnostics & Status ---

    #[tool(
        name = "zotero_status",
        description = "Check diagnostic status of Zotero Local API, Better \
                       BibTeX, and Better Notes bridge"
    )]
    async fn zotero_status(
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
    async fn zotero_get_recent(
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
    async fn zotero_search_items(
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
    async fn zotero_get_item(
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
    async fn zotero_get_item_metadata(
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
    async fn zotero_get_collections(
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
    async fn zotero_get_collection_items(
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
    async fn zotero_get_item_children(
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
    async fn zotero_get_item_fulltext(
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
    async fn zotero_get_pdf_path(
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
    async fn zotero_read_pdf_pages(
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
    async fn zotero_get_notes(
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
    async fn zotero_create_note(
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
    async fn better_bibtex_status(
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
    async fn better_bibtex_get_citekeys(
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
    async fn better_bibtex_export_items(
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
    async fn better_bibtex_bibliography(
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
    async fn better_bibtex_search(
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
    async fn better_bibtex_pandoc_filter(
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
    async fn better_bibtex_regenerate_keys(
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
    async fn better_bibtex_autoexport_add(
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
    async fn better_bibtex_scan_aux(
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
    async fn better_notes_status(
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
    async fn better_notes_to_markdown(
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
    async fn better_notes_from_markdown(
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
    async fn better_notes_run_template(
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
    async fn better_notes_get_relations(
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
    async fn better_notes_get_tree(
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
