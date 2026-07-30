//! Wires every MCP tool to the Zotero, Better `BibTeX`, and Better Notes
//! clients.

use std::future::Future;

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
            FromMarkdownArgs, NoteExportArgs, NoteRelationsArgs, NoteTreeArgs,
            RunTemplateArgs,
        },
        chatgpt::{FetchArgs, SearchArgs},
        zotero::{
            AddByIdentifierArgs, AdvancedSearchArgs, AttachFileArgs,
            BatchUpdateTagsArgs, CreateAnnotationArgs, CreateCollectionArgs,
            CreateNoteArgs, DeleteCollectionArgs, DeleteItemArgs,
            DeleteTagsArgs, EmptyArgs, FindDuplicatesArgs,
            GetCollectionItemsArgs, GetItemArgs, GetItemChildrenArgs,
            GetItemFulltextArgs, GetItemMetadataArgs, GetNotesArgs,
            GetPdfPathArgs, GetRecentArgs, GetUnfiledItemsArgs,
            LibraryCoverageArgs, ListTagsArgs, ManageCollectionsArgs,
            ReadPdfPagesArgs, RenameTagArgs, SearchByCitationKeyArgs,
            SearchByTagArgs, SearchCollectionsArgs, SearchItemsArgs,
            SynthesizeAnnotationsArgs, TrashItemArgs, UpdateCollectionArgs,
            UpdateItemArgs,
        },
    },
    state::AppState,
};

/// Holds shared [`AppState`] and implements [`ServerHandler`], hosting every
/// `#[tool]` method below.
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

    fn list_tools(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<rmcp::model::ListToolsResult, rmcp::ErrorData>>
    {
        std::future::ready(Ok(rmcp::model::ListToolsResult {
            tools: Self::tool_router().list_all(),
            next_cursor: None,
        }))
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

    fn list_resources(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<
        Output = Result<rmcp::model::ListResourcesResult, rmcp::ErrorData>,
    > {
        std::future::ready(Ok(Self::list_resources_impl()))
    }

    async fn read_resource(
        &self,
        param: rmcp::model::ReadResourceRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
        self.read_resource_impl(&param.uri).await
    }

    fn list_prompts(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<rmcp::model::ListPromptsResult, rmcp::ErrorData>>
    {
        std::future::ready(Ok(Self::list_prompts_impl()))
    }

    fn get_prompt(
        &self,
        param: rmcp::model::GetPromptRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<rmcp::model::GetPromptResult, rmcp::ErrorData>>
    {
        std::future::ready(Self::get_prompt_impl(
            &param.name,
            param.arguments.as_ref(),
        ))
    }
}

/// Hosts all `#[tool]` router forwarder methods.
#[tool_router]
impl ZoteroMcpServer {
    // --- Zotero Diagnostics & Status ---

    #[tool(
        name = "zotero_status",
        description = "Check Zotero Local API availability, version, and \
                       connectivity"
    )]
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
        name = "zotero_delete_item",
        description = "Permanently delete an item (article, note, annotation, \
                       or attachment) (requires write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_delete_item(
        &self,
        Parameters(args): Parameters<DeleteItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_delete_item_impl(args).await
    }

    #[tool(
        name = "zotero_trash_item",
        description = "Move an item to trash, reversible (requires write \
                       permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_trash_item(
        &self,
        Parameters(args): Parameters<TrashItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_trash_item_impl(args).await
    }

    #[tool(
        name = "zotero_restore_item",
        description = "Restore an item from trash (requires write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_restore_item(
        &self,
        Parameters(args): Parameters<TrashItemArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_restore_item_impl(args).await
    }

    #[tool(
        name = "zotero_delete_collection",
        description = "Permanently delete a collection; items inside are not \
                       deleted (requires write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_delete_collection(
        &self,
        Parameters(args): Parameters<DeleteCollectionArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_delete_collection_impl(args).await
    }

    #[tool(
        name = "zotero_find_duplicates",
        description = "Finds potential duplicate items in library or \
                       collection by matching title or DOI"
    )]
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
        name = "zotero_get_unfiled_items",
        description = "List top-level items not in any collection"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_unfiled_items(
        &self,
        Parameters(args): Parameters<GetUnfiledItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_unfiled_items_impl(args).await
    }

    #[tool(
        name = "zotero_synthesize_annotations",
        description = "Extract and synthesize annotations and notes into \
                       structured Markdown"
    )]
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

    #[tool(
        name = "zotero_create_annotation",
        description = "Create a PDF highlight/underline/note annotation on an \
                       attachment (requires write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_create_annotation(
        &self,
        Parameters(args): Parameters<CreateAnnotationArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_create_annotation_impl(args).await
    }

    #[tool(
        name = "zotero_add_by_identifier",
        description = "Resolve a DOI, arXiv ID, or ISBN via public metadata \
                       APIs and add it to the library (returns the existing \
                       item instead of creating a duplicate if an exact title \
                       match is already present) (requires write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_add_by_identifier(
        &self,
        Parameters(args): Parameters<AddByIdentifierArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_add_by_identifier_impl(args).await
    }

    #[tool(
        name = "zotero_update_collection",
        description = "Rename and/or move a collection (pass an empty string \
                       for parent_key to move to the top level) (requires \
                       write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_update_collection(
        &self,
        Parameters(args): Parameters<UpdateCollectionArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_update_collection_impl(args).await
    }

    #[tool(
        name = "zotero_list_tags",
        description = "List all tag names in the library"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_list_tags(
        &self,
        Parameters(args): Parameters<ListTagsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_list_tags_impl(args).await
    }

    #[tool(
        name = "zotero_rename_tag",
        description = "Rename a tag across every item in the library that has \
                       it (requires write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_rename_tag(
        &self,
        Parameters(args): Parameters<RenameTagArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_rename_tag_impl(args).await
    }

    #[tool(
        name = "zotero_delete_tags",
        description = "Delete up to 50 tags from the entire library (requires \
                       write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_delete_tags(
        &self,
        Parameters(args): Parameters<DeleteTagsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_delete_tags_impl(args).await
    }

    // --- Better BibTeX Operations ---

    #[tool(
        name = "better_bibtex_get_citekeys",
        description = "Fetch citation keys for Zotero items via Better BibTeX"
    )]
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
        name = "better_notes_export",
        description = "Export a Zotero note item as Markdown or HTML via \
                       Better Notes"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_notes_export(
        &self,
        Parameters(args): Parameters<NoteExportArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.better_notes_export_impl(args).await
    }

    #[tool(
        name = "better_notes_from_markdown",
        description = "Convert Markdown to HTML formatted for Zotero notes \
                       via Better Notes"
    )]
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
    use crate::{
        better_notes::NoteExportFormat, mcp::better_notes::NoteExportArgs,
        zotero::AnnotationType,
    };

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
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            }
        }

        /// Builds an [`AppState`] fixture for Better Notes MCP handler tests.
        pub(super) fn better_notes_state(better_notes_url: String) -> AppState {
            AppState {
                zotero_api_url: String::new(),
                better_bibtex_url: String::new(),
                better_notes_url,
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            }
        }

        /// Formats a minimal JSON HTTP response for fixture servers.
        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        /// Runs a one-shot fixture HTTP server and returns its base URL.
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

    #[tokio::test]
    async fn better_notes_export_tool_returns_markdown_success() {
        let base = mock_server(vec![http_response(
            "200 OK",
            r##"{"content":"# Exported"}"##,
        )]);
        let server = ZoteroMcpServer::new(better_notes_state(base));

        let res = server
            .better_notes_export(Parameters(NoteExportArgs {
                item_key: "NOTE1".into(),
                format: Some(NoteExportFormat::Markdown),
            }))
            .await
            .unwrap();

        assert_eq!(res.is_error, Some(false));
        assert_eq!(
            res.content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str()),
            Some("# Exported")
        );
    }

    #[tokio::test]
    async fn better_notes_export_tool_returns_html_success() {
        let base = mock_server(vec![http_response(
            "200 OK",
            r#"{"content":"<h1>Exported</h1>"}"#,
        )]);
        let server = ZoteroMcpServer::new(better_notes_state(base));

        let res = server
            .better_notes_export(Parameters(NoteExportArgs {
                item_key: "NOTE1".into(),
                format: Some(NoteExportFormat::Html),
            }))
            .await
            .unwrap();

        assert_eq!(res.is_error, Some(false));
        assert_eq!(
            res.content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str()),
            Some("<h1>Exported</h1>")
        );
    }

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
    async fn zotero_get_unfiled_items_tool_returns_success() {
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Unfiled Item", "collections": [] }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_get_unfiled_items(Parameters(GetUnfiledItemsArgs {
                limit: Some(50),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_delete_item_tool_returns_success() {
        let item = json!({
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle" }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("204 No Content", ""),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_delete_item(Parameters(DeleteItemArgs {
                item_key: "ITEM1".into(),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_trash_item_tool_returns_success() {
        let item = json!({
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle" }
        });
        let updated = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "deleted": true }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_trash_item(Parameters(TrashItemArgs {
                item_key: "ITEM1".into(),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_restore_item_tool_returns_success() {
        let item = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "deleted": true }
        });
        let updated = json!({
            "key": "ITEM1",
            "version": 3,
            "data": { "key": "ITEM1", "version": 3, "itemType": "journalArticle", "deleted": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_restore_item(Parameters(TrashItemArgs {
                item_key: "ITEM1".into(),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_delete_collection_tool_returns_success() {
        let collection = json!({
            "key": "COL1",
            "version": 1,
            "data": { "key": "COL1", "name": "Old Collection", "parentCollection": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &collection.to_string()),
            http_response("204 No Content", ""),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_delete_collection(Parameters(DeleteCollectionArgs {
                collection_key: "COL1".into(),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_delete_item_tool_returns_error_when_write_disabled() {
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: String::new(),
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            write_enabled: false,
            ..AppState::from_env()
        });

        let res = server
            .zotero_delete_item(Parameters(DeleteItemArgs {
                item_key: "ITEM1".into(),
            }))
            .await
            .unwrap();
        assert_eq!(res.is_error, Some(true));
    }

    #[tokio::test]
    async fn zotero_create_annotation_tool_returns_success() {
        let created = json!([{
            "key": "ANNOT1",
            "version": 1,
            "data": { "key": "ANNOT1", "version": 1, "itemType": "annotation", "annotationType": "highlight" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_create_annotation(Parameters(CreateAnnotationArgs {
                parent_attachment_key: "ATT1".into(),
                annotation_type: AnnotationType::Highlight,
                text: Some("selected text".to_owned()),
                comment: None,
                color: None,
                page_label: None,
                position: json!({"pageIndex": 0, "rects": [[100, 200, 300, 220]]}),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_add_by_identifier_tool_returns_success() {
        let crossref_body = json!({"message": {
            "title": ["A Great Paper"],
            "author": [{"given": "Sam", "family": "McAuthor"}],
            "published": {"date-parts": [[2021]]},
            "DOI": "10.1/xyz",
            "URL": "https://doi.org/10.1/xyz",
            "container-title": ["Journal of Things"]
        }});
        let crossref_base = mock_server(vec![http_response(
            "200 OK",
            &crossref_body.to_string(),
        )]);
        let created = json!([{
            "key": "NEWITEM1",
            "version": 1,
            "data": { "key": "NEWITEM1", "version": 1, "itemType": "journalArticle", "title": "A Great Paper" }
        }]);
        let zotero_base = mock_server(vec![
            http_response("200 OK", "[]"),
            http_response("200 OK", &created.to_string()),
        ]);
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: crossref_base,
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: true,
            ..AppState::from_env()
        });

        let res = server
            .zotero_add_by_identifier(Parameters(AddByIdentifierArgs {
                kind: crate::zotero::IdentifierKind::Doi,
                identifier: "10.1/xyz".to_owned(),
                collection_key: None,
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_add_by_identifier_tool_returns_existing_item_when_duplicate_found()
     {
        let crossref_body = json!({"message": {
            "title": ["A Great Paper"],
            "author": [{"given": "Sam", "family": "McAuthor"}],
            "published": {"date-parts": [[2021]]},
            "DOI": "10.1/xyz",
            "URL": "https://doi.org/10.1/xyz",
            "container-title": ["Journal of Things"]
        }});
        let crossref_base = mock_server(vec![http_response(
            "200 OK",
            &crossref_body.to_string(),
        )]);
        let existing = json!([{
            "key": "EXISTING1",
            "version": 1,
            "data": { "key": "EXISTING1", "version": 1, "itemType": "journalArticle", "title": "A Great Paper" }
        }]);
        let zotero_base =
            mock_server(vec![http_response("200 OK", &existing.to_string())]);
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: crossref_base,
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: true,
            ..AppState::from_env()
        });

        let res = server
            .zotero_add_by_identifier(Parameters(AddByIdentifierArgs {
                kind: crate::zotero::IdentifierKind::Doi,
                identifier: "10.1/xyz".to_owned(),
                collection_key: None,
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
        let text = match res.content.first().and_then(|c| c.as_text()) {
            Some(t) => t.text.clone(),
            None => String::new(),
        };
        assert!(text.contains("EXISTING1"));
    }

    #[tokio::test]
    async fn zotero_update_collection_tool_returns_success() {
        let current = json!({
            "key": "COL1",
            "version": 3,
            "data": { "key": "COL1", "name": "Old Name", "parentCollection": false }
        });
        let updated = json!({
            "key": "COL1",
            "version": 4,
            "data": { "key": "COL1", "name": "New Name", "parentCollection": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &current.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_update_collection(Parameters(UpdateCollectionArgs {
                collection_key: "COL1".into(),
                name: Some("New Name".to_owned()),
                parent_key: None,
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_list_tags_tool_returns_success() {
        let tags = json!([{"tag": "quantum", "meta": {"numItems": 3}}]);
        let base =
            mock_server(vec![http_response("200 OK", &tags.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_list_tags(Parameters(ListTagsArgs {
                limit: Some(50),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_rename_tag_tool_returns_success() {
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "tags": [{ "tag": "old_tag" }] }
        }]);
        let patched = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "tags": [{ "tag": "new_tag" }] }
        });
        let base = mock_server(vec![
            http_response("200 OK", &items.to_string()),
            http_response("200 OK", &patched.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        let res = server
            .zotero_rename_tag(Parameters(RenameTagArgs {
                old_tag: "old_tag".into(),
                new_tag: "new_tag".into(),
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn zotero_delete_tags_tool_returns_success() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        std::thread::spawn(move || {
            let (mut stream, _) =
                listener.accept().expect("accept version request");
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);
            let version_resp = "HTTP/1.1 200 OK\r\nContent-Length: \
                                2\r\nContent-Type: \
                                application/json\r\nLast-Modified-Version: \
                                9\r\nConnection: close\r\n\r\n[]";
            let _ = stream.write_all(version_resp.as_bytes());

            let (mut stream2, _) =
                listener.accept().expect("accept delete request");
            let mut buf2 = [0_u8; 1024];
            let _ = stream2.read(&mut buf2);
            let _ = stream2.write_all(
                "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"
                    .as_bytes(),
            );
        });
        let server =
            ZoteroMcpServer::new(zotero_state(format!("http://{addr}")));

        let res = server
            .zotero_delete_tags(Parameters(DeleteTagsArgs {
                tags: vec!["old_tag".into()],
            }))
            .await
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn list_resources_returns_collections_uri() {
        let res = ZoteroMcpServer::list_resources_impl();
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
        let _server = ZoteroMcpServer::new(zotero_state(String::new()));
        let list = ZoteroMcpServer::list_prompts_impl();
        assert_eq!(list.prompts.len(), 1);
        assert_eq!(
            list.prompts.first().expect("prompt").name,
            "zotero_literature_review"
        );

        let mut args = serde_json::Map::new();
        args.insert("collection_key".to_owned(), json!("COL123"));
        let prompt = ZoteroMcpServer::get_prompt_impl(
            "zotero_literature_review",
            Some(&args),
        )
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
