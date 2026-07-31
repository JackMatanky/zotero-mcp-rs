//! MCP server implementation and tool router dispatch for Zotero integration.
//!
//! This module defines [`ZoteroMcpServer`], which implements the `rmcp`
//! [`ServerHandler`] trait to serve tool calls, resources, and prompts to
//! connected MCP clients.
//!
//! Tools are routed using the `#[tool_router]` macro, delegating logic to the
//! underlying Zotero Local API, Better `BibTeX`, and Better Notes handlers.

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
            AutoExportAddArgs, BetterBibtexSearchArgs, BibliographyArgs,
            ExportItemsArgs, GetCitekeysArgs, PandocFilterArgs,
            RegenerateKeysArgs, ScanAuxArgs,
        },
        better_notes::{
            FromMarkdownArgs, NoteExportArgs, NoteRelationsArgs, NoteTreeArgs,
            RunTemplateArgs,
        },
        connector_tools::{FetchArgs, SearchArgs},
        zotero::{
            AddByIdentifierArgs, AdvancedSearchArgs, AttachFileArgs,
            BatchUpdateTagsArgs, CreateAnnotationArgs, CreateCollectionArgs,
            CreateNoteArgs, DeleteCollectionArgs, DeleteItemArgs,
            DeleteTagsArgs, EmptyArgs, FindDuplicatesArgs,
            GetCollectionItemsArgs, GetItemArgs, GetItemChildrenArgs,
            GetItemFulltextArgs, GetItemMetadataArgs, GetNotesArgs,
            GetPdfOutlineArgs, GetPdfPathArgs, GetRecentArgs,
            GetUnfiledItemsArgs, LibraryCoverageArgs, ListTagsArgs,
            ManageCollectionsArgs, ReadPdfPagesArgs, RenameTagArgs,
            SearchByCitationKeyArgs, SearchByTagArgs, SearchCollectionsArgs,
            SearchItemsArgs, SynthesizeAnnotationsArgs, TrashItemArgs,
            UpdateCollectionArgs, UpdateItemArgs,
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
        name = "zotero_get_pdf_outline",
        description = "Extract the PDF outline (table of contents/bookmarks) \
                       for an item's PDF attachment or a direct PDF path"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_pdf_outline(
        &self,
        Parameters(args): Parameters<GetPdfOutlineArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_pdf_outline_impl(args).await
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
        description = "Regenerate Better BibTeX citation keys (requires write \
                       permission)"
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
        description = "Export citekeys using a Better BibTeX translator"
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
        description = "Format a bibliography for citekeys with Better BibTeX"
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
        description = "Configure Better BibTeX auto-export for a collection \
                       path (requires write permission)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn better_bibtex_autoexport_add(
        &self,
        Parameters(args): Parameters<AutoExportAddArgs>,
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

    // --- Connector-Compatible Tools ---

    #[tool(
        name = "search",
        description = "Connector search tool - search Zotero items by query"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn connector_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.connector_search_impl(args).await
    }

    #[tool(
        name = "fetch",
        description = "Connector fetch tool - get Zotero item metadata by \
                       item ID/key"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn connector_fetch(
        &self,
        Parameters(args): Parameters<FetchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.connector_fetch_impl(args).await
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::state::AppState;

    mod server_handler {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn get_info_returns_server_metadata_and_capabilities() {
            // Arrange
            let server = ZoteroMcpServer::new(AppState::from_env());

            // Act
            let info = server.get_info();

            // Assert
            assert_eq!(info.server_info.name, "zotero-mcp-rs");
            assert_eq!(info.server_info.version, "0.1.0");
            assert!(info.capabilities.tools.is_some());
            assert!(info.capabilities.resources.is_some());
            assert!(info.capabilities.prompts.is_some());
        }

        #[test]
        fn tool_router_lists_all_registered_tools() {
            // Act
            let tools = ZoteroMcpServer::tool_router().list_all();

            // Assert
            assert!(!tools.is_empty());
        }
    }
}
