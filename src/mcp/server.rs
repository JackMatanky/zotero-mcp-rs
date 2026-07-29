//! Wires every MCP tool to the Zotero, Better `BibTeX`, and Better Notes
//! clients.

use rmcp::{
    ServerHandler,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, Implementation, InitializeResult, ProtocolVersion,
        ServerCapabilities,
    },
    tool, tool_router,
};

use crate::{
    mcp::{
        better_bibtex::{
            AutoexportAddArgs, BetterBibtexSearchArgs, BibliographyArgs,
            ExportItemsArgs, GetCitekeysArgs, PandocFilterArgs,
            RegenerateKeysArgs, ScanAuxArgs,
        },
        better_notes::{
            FromMarkdownArgs, NoteRelationsArgs, NoteTreeArgs, RunTemplateArgs,
            ToMarkdownArgs,
        },
        chatgpt::{FetchArgs, SearchArgs},
        zotero::{
            AdvancedSearchArgs, AttachFileArgs, BatchUpdateTagsArgs,
            CreateCollectionArgs, CreateNoteArgs, EmptyArgs,
            FindDuplicatesArgs, GetCollectionItemsArgs, GetItemArgs,
            GetItemChildrenArgs, GetItemFulltextArgs, GetItemMetadataArgs,
            GetNotesArgs, GetPdfPathArgs, GetRecentArgs, LibraryCoverageArgs,
            ManageCollectionsArgs, ReadPdfPagesArgs, SearchByCitationKeyArgs,
            SearchByTagArgs, SearchCollectionsArgs, SearchItemsArgs,
            SynthesizeAnnotationsArgs, UpdateItemArgs,
        },
    },
    state::AppState,
};

/// The MCP tool router: holds the shared [`AppState`] and implements
/// [`ServerHandler`], hosting every `#[tool]` method below.
pub(crate) struct ZoteroMcpServer {
    /// Shared configuration and HTTP client state.
    pub(crate) state: AppState,
}

impl ZoteroMcpServer {
    /// Creates an MCP server using shared [`AppState`].
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
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
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

    async fn list_resources(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        self.list_resources_impl()
    }

    async fn read_resource(
        &self,
        param: rmcp::model::ReadResourceRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
        self.read_resource_impl(&param.uri).await
    }

    async fn list_prompts(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ListPromptsResult, rmcp::ErrorData> {
        self.list_prompts_impl()
    }

    async fn get_prompt(
        &self,
        param: rmcp::model::GetPromptRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::GetPromptResult, rmcp::ErrorData> {
        self.get_prompt_impl(&param.name, param.arguments.as_ref())
    }
}

#[tool_router]
impl ZoteroMcpServer {
    // --- Zotero Diagnostics & Status ---

    #[tool(
        name = "zotero_status",
        description = "Check Zotero Local API availability, version, and \
                       connectivity"
    )]
    /// Routes `zotero_status` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_status(
        &self,
        Parameters(_args): Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_status_impl().await
    }

    // --- Zotero Read Operations ---

    #[tool(
        name = "zotero_get_recent",
        description = "Fetch recently modified library items (notes excluded)"
    )]
    /// Routes `zotero_get_recent` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_recent(
        &self,
        Parameters(args): Parameters<GetRecentArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_recent_impl(args).await
    }

    #[tool(
        name = "zotero_search_items",
        description = "Search items by title, creator, year, or fulltext query"
    )]
    /// Routes `zotero_search_items` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_items(
        &self,
        Parameters(args): Parameters<SearchItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_items_impl(args).await
    }

    #[tool(
        name = "zotero_get_item",
        description = "Fetch a single Zotero item by its key"
    )]
    /// Routes `zotero_get_item` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item(
        &self,
        Parameters(args): Parameters<GetItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_impl(args).await
    }

    #[tool(
        name = "zotero_get_item_metadata",
        description = "Get metadata for an item as JSON or formatted BibTeX \
                       string"
    )]
    /// Routes `zotero_get_item_metadata` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_metadata(
        &self,
        Parameters(args): Parameters<GetItemMetadataArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_metadata_impl(args).await
    }

    #[tool(
        name = "zotero_get_collection_items",
        description = "Fetch items inside a specific Zotero collection"
    )]
    /// Routes `zotero_get_collection_items` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_collection_items(
        &self,
        Parameters(args): Parameters<GetCollectionItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_collection_items_impl(args).await
    }

    #[tool(
        name = "zotero_get_item_children",
        description = "Get child items (notes, attachments) for a given item \
                       key"
    )]
    /// Routes `zotero_get_item_children` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_children(
        &self,
        Parameters(args): Parameters<GetItemChildrenArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_children_impl(args).await
    }

    #[tool(
        name = "zotero_get_item_fulltext",
        description = "Get Zotero's indexed fulltext for an item"
    )]
    /// Routes `zotero_get_item_fulltext` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_fulltext(
        &self,
        Parameters(args): Parameters<GetItemFulltextArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_fulltext_impl(args).await
    }

    #[tool(
        name = "zotero_get_pdf_path",
        description = "Locate the local PDF file path for an item or its \
                       attachment"
    )]
    /// Routes `zotero_get_pdf_path` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_pdf_path(
        &self,
        Parameters(args): Parameters<GetPdfPathArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_pdf_path_impl(args).await
    }

    #[tool(
        name = "zotero_read_pdf_pages",
        description = "Extract raw text from specific 1-based pages of a PDF"
    )]
    /// Routes `zotero_read_pdf_pages` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_read_pdf_pages(
        &self,
        Parameters(args): Parameters<ReadPdfPagesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_read_pdf_pages_impl(args).await
    }

    #[tool(
        name = "zotero_get_notes",
        description = "Fetch all note child items for a given item key"
    )]
    /// Routes `zotero_get_notes` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_notes(
        &self,
        Parameters(args): Parameters<GetNotesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_notes_impl(args).await
    }

    // --- Zotero Write Operations ---

    #[tool(
        name = "zotero_create_note",
        description = "Attach a new note to an item (requires write \
                       permission)"
    )]
    /// Routes `zotero_create_note` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_note(
        &self,
        Parameters(args): Parameters<CreateNoteArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_create_note_impl(args).await
    }

    #[tool(
        name = "zotero_create_collection",
        description = "Create a new Zotero collection (requires write \
                       permission)"
    )]
    /// Routes `zotero_create_collection` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_collection(
        &self,
        Parameters(args): Parameters<CreateCollectionArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_create_collection_impl(args).await
    }

    #[tool(
        name = "zotero_search_collections",
        description = "Search collections by collection name query"
    )]
    /// Routes `zotero_search_collections` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_collections(
        &self,
        Parameters(args): Parameters<SearchCollectionsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_collections_impl(args).await
    }

    #[tool(
        name = "zotero_manage_collections",
        description = "Add or remove items to/from a collection (requires \
                       write permission)"
    )]
    /// Routes `zotero_manage_collections` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_manage_collections(
        &self,
        Parameters(args): Parameters<ManageCollectionsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_manage_collections_impl(args).await
    }

    #[tool(
        name = "zotero_update_item",
        description = "Update fields of an existing item using PATCH \
                       (requires write permission)"
    )]
    /// Routes `zotero_update_item` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_update_item(
        &self,
        Parameters(args): Parameters<UpdateItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_update_item_impl(args).await
    }

    #[tool(
        name = "zotero_attach_file",
        description = "Attach a file link to a parent item (requires write \
                       permission)"
    )]
    /// Routes `zotero_attach_file` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_attach_file(
        &self,
        Parameters(args): Parameters<AttachFileArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_attach_file_impl(args).await
    }

    #[tool(
        name = "zotero_batch_update_tags",
        description = "Batch add/remove tags across items (requires write \
                       permission)"
    )]
    /// Routes `zotero_batch_update_tags` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_batch_update_tags(
        &self,
        Parameters(args): Parameters<BatchUpdateTagsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_batch_update_tags_impl(args).await
    }

    #[tool(
        name = "zotero_find_duplicates",
        description = "Finds potential duplicate items in library or \
                       collection by matching title or DOI"
    )]
    /// Routes `zotero_find_duplicates` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_find_duplicates(
        &self,
        Parameters(args): Parameters<FindDuplicatesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_find_duplicates_impl(args).await
    }

    #[tool(
        name = "zotero_search_by_tag",
        description = "Search Zotero items by tag string"
    )]
    /// Routes `zotero_search_by_tag` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_tag(
        &self,
        Parameters(args): Parameters<SearchByTagArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_by_tag_impl(args).await
    }

    #[tool(
        name = "zotero_search_by_citation_key",
        description = "Search Zotero items by citation key string"
    )]
    /// Routes `zotero_search_by_citation_key` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_citation_key(
        &self,
        Parameters(args): Parameters<SearchByCitationKeyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_by_citation_key_impl(args).await
    }

    #[tool(
        name = "zotero_advanced_search",
        description = "Advanced multi-condition structured search over item \
                       fields"
    )]
    /// Routes `zotero_advanced_search` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_advanced_search(
        &self,
        Parameters(args): Parameters<AdvancedSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_advanced_search_impl(args).await
    }

    #[tool(
        name = "zotero_library_coverage",
        description = "Analyze library or collection statistics for PDF, DOI, \
                       and note coverage"
    )]
    /// Routes `zotero_library_coverage` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_library_coverage(
        &self,
        Parameters(args): Parameters<LibraryCoverageArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_library_coverage_impl(args).await
    }

    #[tool(
        name = "zotero_synthesize_annotations",
        description = "Extract and synthesize annotations and notes into \
                       structured Markdown"
    )]
    /// Routes `zotero_synthesize_annotations` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_synthesize_annotations(
        &self,
        Parameters(args): Parameters<SynthesizeAnnotationsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_synthesize_annotations_impl(args).await
    }

    // --- Better BibTeX Operations ---

    #[tool(
        name = "better_bibtex_get_citekeys",
        description = "Fetch citation keys for Zotero items via Better BibTeX"
    )]
    /// Routes `better_bibtex_get_citekeys` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_get_citekeys(
        &self,
        Parameters(args): Parameters<GetCitekeysArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_bibtex_get_citekeys_impl(args).await
    }

    #[tool(
        name = "better_bibtex_regenerate_citekeys",
        description = "Regenerate citation keys for items via Better BibTeX \
                       (requires write permission)"
    )]
    /// Routes `better_bibtex_regenerate_citekeys` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_regenerate_citekeys(
        &self,
        Parameters(args): Parameters<RegenerateKeysArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_bibtex_regenerate_citekeys_impl(args).await
    }

    #[tool(
        name = "better_bibtex_export_items",
        description = "Export items using a Better BibTeX translator"
    )]
    /// Routes `better_bibtex_export_items` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_export_items(
        &self,
        Parameters(args): Parameters<ExportItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_bibtex_export_items_impl(args).await
    }

    #[tool(
        name = "better_bibtex_format_bibliography",
        description = "Format a bibliography for citekeys in a given CSL style"
    )]
    /// Routes `better_bibtex_format_bibliography` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_format_bibliography(
        &self,
        Parameters(args): Parameters<BibliographyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_bibtex_format_bibliography_impl(args).await
    }

    #[tool(
        name = "better_bibtex_scan_aux",
        description = "Extract citekeys from a LaTeX .aux file via Better \
                       BibTeX"
    )]
    /// Routes `better_bibtex_scan_aux` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_scan_aux(
        &self,
        Parameters(args): Parameters<ScanAuxArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_bibtex_scan_aux_impl(args).await
    }

    #[tool(
        name = "better_bibtex_pandoc_filter",
        description = "Process citekeys through Better BibTeX's Pandoc filter"
    )]
    /// Routes `better_bibtex_pandoc_filter` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_pandoc_filter(
        &self,
        Parameters(args): Parameters<PandocFilterArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_bibtex_pandoc_filter_impl(args).await
    }

    #[tool(
        name = "better_bibtex_autoexport_add",
        description = "Configure auto-export for a collection/library \
                       (requires write permission)"
    )]
    /// Routes `better_bibtex_autoexport_add` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_autoexport_add(
        &self,
        Parameters(args): Parameters<AutoexportAddArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_bibtex_autoexport_add_impl(args).await
    }

    #[tool(
        name = "better_bibtex_search",
        description = "Search items using Better BibTeX's query engine"
    )]
    /// Routes `better_bibtex_search` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_search(
        &self,
        Parameters(args): Parameters<BetterBibtexSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_bibtex_search_impl(args).await
    }

    // --- Better Notes Operations ---

    #[tool(
        name = "better_notes_to_markdown",
        description = "Convert a Zotero note item to Markdown via Better Notes"
    )]
    /// Routes `better_notes_to_markdown` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_to_markdown(
        &self,
        Parameters(args): Parameters<ToMarkdownArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_notes_to_markdown_impl(args).await
    }

    #[tool(
        name = "better_notes_from_markdown",
        description = "Convert Markdown to HTML formatted for Zotero notes \
                       via Better Notes"
    )]
    /// Routes `better_notes_from_markdown` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_from_markdown(
        &self,
        Parameters(args): Parameters<FromMarkdownArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_notes_from_markdown_impl(args).await
    }

    #[tool(
        name = "better_notes_run_template",
        description = "Execute a Better Notes template against an item"
    )]
    /// Routes `better_notes_run_template` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_run_template(
        &self,
        Parameters(args): Parameters<RunTemplateArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_notes_run_template_impl(args).await
    }

    #[tool(
        name = "better_notes_get_relations",
        description = "Fetch linked items / note network for a note via \
                       Better Notes"
    )]
    /// Routes `better_notes_get_relations` MCP tool calls to the domain
    /// handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_get_relations(
        &self,
        Parameters(args): Parameters<NoteRelationsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_notes_get_relations_impl(args).await
    }

    #[tool(
        name = "better_notes_get_tree",
        description = "Fetch the hierarchical note outline/tree for a note \
                       via Better Notes"
    )]
    /// Routes `better_notes_get_tree` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_get_tree(
        &self,
        Parameters(args): Parameters<NoteTreeArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_notes_get_tree_impl(args).await
    }

    // --- ChatGPT Connector Compatibility Tools ---

    #[tool(
        name = "search",
        description = "ChatGPT Connector search tool - search Zotero items by \
                       query"
    )]
    /// Routes `chatgpt_search` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn chatgpt_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.chatgpt_search_impl(args).await
    }

    #[tool(
        name = "fetch",
        description = "ChatGPT Connector fetch tool - get item metadata by \
                       item ID/key"
    )]
    /// Routes `chatgpt_fetch` MCP tool calls to the domain handler.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn chatgpt_fetch(
        &self,
        Parameters(args): Parameters<FetchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.chatgpt_fetch_impl(args).await
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        /// Builds an [`AppState`] fixture for Zotero MCP handler tests.
        pub(super) fn zotero_state(zotero_api_url: String) -> AppState {
            AppState {
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            }
        }

        /// Formats a minimal JSON HTTP response for fixture servers.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\n\r\n{body}",
                body.len()
            )
        }

        #[expect(
            clippy::excessive_nesting,
            reason = "mock HTTP server thread loop"
        )]
        /// Runs a one-shot fixture HTTP server and returns its base URL.
        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let Ok((mut stream, _)) = listener.accept() else {
                        continue;
                    };
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }
    }

    use fixtures::*;

    #[tokio::test]
    async fn zotero_get_recent_tool_returns_success() {
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Test Title" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_get_recent(Parameters(GetRecentArgs {
                limit: Some(10),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn list_resources_returns_collections_uri() {
        let server = ZoteroMcpServer::new(zotero_state(String::new()));
        let res = server.list_resources_impl().unwrap();
        assert_eq!(res.resources.len(), 1);
        assert_eq!(
            res.resources.first().expect("resource").raw.uri,
            "zotero://collections"
        );
    }

    #[tokio::test]
    async fn read_resource_returns_item_json() {
        let item = json!({
            "key": "ITEM123",
            "version": 1,
            "data": { "key": "ITEM123", "itemType": "journalArticle", "title": "Resource Test Paper" }
        });
        let base =
            mock_server(vec![http_response("200 OK", &item.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res =
            server.read_resource_impl("zotero://items/ITEM123").await.unwrap();
        assert_eq!(res.contents.len(), 1);
        let content = res.contents.first().expect("resource content");
        let is_text = matches!(content, rmcp::model::ResourceContents::TextResourceContents { text, .. } if text.contains("Resource Test Paper"));
        assert!(is_text);
    }

    #[tokio::test]
    async fn list_and_get_prompts_work() {
        let server = ZoteroMcpServer::new(zotero_state(String::new()));
        let list = server.list_prompts_impl().unwrap();
        assert_eq!(list.prompts.len(), 1);
        assert_eq!(
            list.prompts.first().expect("prompt").name,
            "zotero_literature_review"
        );

        let mut args = serde_json::Map::new();
        args.insert("collection_key".to_owned(), json!("COL123"));
        let prompt = server
            .get_prompt_impl("zotero_literature_review", Some(&args))
            .unwrap();
        assert_eq!(prompt.messages.len(), 1);
    }

    #[tokio::test]
    async fn chatgpt_connector_search_and_fetch_tools() {
        let item = json!({
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Physics Paper" }
        });
        let base = mock_server(vec![
            http_response("200 OK", &json!([item]).to_string()),
            http_response("200 OK", &item.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let search_res = server
            .chatgpt_search(Parameters(SearchArgs {
                query: "quantum".to_owned(),
            }))
            .await
            .unwrap();
        assert!(!search_res.is_error.unwrap_or(false));

        let fetch_res = server
            .chatgpt_fetch(Parameters(FetchArgs {
                id: "ITEM1".to_owned(),
            }))
            .await
            .unwrap();
        assert!(!fetch_res.is_error.unwrap_or(false));
    }
}
