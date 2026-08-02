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
        CallToolResponse, CallToolResult, GetPromptResponse, Implementation,
        InitializeResult, ProtocolVersion, ReadResourceResponse,
        ServerCapabilities,
    },
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
            AddByIdentifierArgs, AddItemRelationArgs, AdvancedSearchArgs,
            AttachFileArgs, BatchUpdateTagsArgs, CreateAnnotationArgs,
            CreateCollectionArgs, CreateNoteArgs, DeleteCollectionArgs,
            DeleteItemArgs, DeleteTagsArgs, EmptyArgs, FindDuplicatesArgs,
            FulltextSearchArgs, GetCollectionItemsArgs, GetItemArgs,
            GetItemChildrenArgs, GetItemFulltextArgs, GetItemMetadataArgs,
            GetNotesArgs, GetPdfOutlineArgs, GetPdfPathArgs, GetRecentArgs,
            GetRelatedItemsArgs, GetUnfiledItemsArgs, LibraryCoverageArgs,
            ListTagsArgs, ManageCollectionsArgs, ReadPdfPagesArgs,
            RemoveItemRelationArgs, RenameTagArgs, SearchByCitationKeyArgs,
            SearchByTagArgs, SearchCollectionsArgs, SearchItemsArgs,
            SearchNotesAnnotationsArgs, SynthesizeAnnotationsArgs,
            TrashItemArgs, UpdateCollectionArgs, UpdateItemArgs,
        },
    },
    state::{AppState, ToolExposureMode},
};

const SERVER_INSTRUCTIONS: &str =
    "Call zotero_discover first to find Zotero tools, resources, prompts, env \
     gates, and examples. Use zotero://... resources for read-only object \
     retrieval, including zotero://items/{item_key}. search and fetch are \
     connector compatibility tools. Write tools require \
     ZOTERO_WRITE_ENABLED=1. SQLite tools require ZOTERO_SQLITE_ACCESS=1. \
     ZOTERO_MCP_MODE=all exposes legacy individual tools.";

#[derive(Deserialize, JsonSchema)]
pub(crate) struct DiscoverArgs {
    pub(crate) query: Option<String>,
    pub(crate) domain: Option<CapabilityDomain>,
    pub(crate) include_disabled: Option<bool>,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CapabilityKind {
    Tool,
    Resource,
    Prompt,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CapabilityDomain {
    Discovery,
    Items,
    Collections,
    Search,
    Notes,
    Sqlite,
    Prompts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum CapabilityGate {
    #[serde(rename = "ZOTERO_WRITE_ENABLED")]
    WriteEnabled,
    #[serde(rename = "ZOTERO_SQLITE_ACCESS")]
    SqliteAccess,
}

#[derive(Clone, Copy, Serialize)]
struct CapabilityInfo {
    name: &'static str,
    kind: CapabilityKind,
    domain: CapabilityDomain,
    requires: &'static [CapabilityGate],
    summary: &'static str,
    example: Option<&'static str>,
    #[serde(skip_serializing)]
    search_text: &'static str,
}

static CAPABILITIES: &[CapabilityInfo] = &[
    CapabilityInfo {
        name: "zotero_discover",
        kind: CapabilityKind::Tool,
        domain: CapabilityDomain::Discovery,
        requires: &[],
        summary: "Find Zotero tools, resources, prompts, env gates, and \
                  examples",
        example: Some(r#"{"query":"notes"}"#),
        search_text: "zotero_discover discovery find zotero tools resources \
                      prompts env gates and examples",
    },
    CapabilityInfo {
        name: "zotero://items/{item_key}",
        kind: CapabilityKind::Resource,
        domain: CapabilityDomain::Items,
        requires: &[],
        summary: "Read one Zotero item by key",
        example: Some("zotero://items/ITEMKEY"),
        search_text: "zotero://items/{item_key} items read one zotero item by \
                      key",
    },
    CapabilityInfo {
        name: "zotero://collections/{collection_key}/items",
        kind: CapabilityKind::Resource,
        domain: CapabilityDomain::Collections,
        requires: &[],
        summary: "Read collection items",
        example: Some("zotero://collections/COLKEY/items"),
        search_text: "zotero://collections/{collection_key}/items collections \
                      read collection items",
    },
    CapabilityInfo {
        name: "zotero_search",
        kind: CapabilityKind::Tool,
        domain: CapabilityDomain::Search,
        requires: &[],
        summary: "Grouped search actions: items, tag, citation_key, advanced, \
                  duplicates, coverage",
        example: Some(r#"{"action":"items","query":"rust","limit":10}"#),
        search_text: "zotero_search search grouped search actions items tag \
                      citation_key advanced duplicates coverage",
    },
    CapabilityInfo {
        name: "zotero_items",
        kind: CapabilityKind::Tool,
        domain: CapabilityDomain::Items,
        requires: &[],
        summary: "Grouped item read actions: recent, get, metadata, children, \
                  fulltext",
        example: Some(r#"{"action":"get","item_key":"ITEMKEY"}"#),
        search_text: "zotero_items items grouped item read actions recent get \
                      metadata children fulltext",
    },
    CapabilityInfo {
        name: "zotero_notes",
        kind: CapabilityKind::Tool,
        domain: CapabilityDomain::Notes,
        requires: &[],
        summary: "Grouped note read actions: list, synthesize",
        example: Some(r#"{"action":"list","item_key":"ITEMKEY"}"#),
        search_text: "zotero_notes notes grouped note read actions list \
                      synthesize",
    },
    CapabilityInfo {
        name: "zotero_items_write",
        kind: CapabilityKind::Tool,
        domain: CapabilityDomain::Items,
        requires: &[CapabilityGate::WriteEnabled],
        summary: "Grouped item write actions: update, delete, trash, restore, \
                  add_by_identifier, attach_file",
        example: Some(r#"{"action":"trash","item_key":"ITEMKEY"}"#),
        search_text: "zotero_items_write items grouped item write actions \
                      update delete trash restore add_by_identifier \
                      attach_file zotero_write_enabled",
    },
    CapabilityInfo {
        name: "zotero_notes_write",
        kind: CapabilityKind::Tool,
        domain: CapabilityDomain::Notes,
        requires: &[CapabilityGate::WriteEnabled],
        summary: "Grouped note write actions: create, annotation",
        example: Some(
            r##"{"action":"create","parent_key":"ITEMKEY","markdown":"# Note"}"##,
        ),
        search_text: "zotero_notes_write notes grouped note write actions \
                      create annotation zotero_write_enabled",
    },
    CapabilityInfo {
        name: "zotero_sqlite_search",
        kind: CapabilityKind::Tool,
        domain: CapabilityDomain::Sqlite,
        requires: &[CapabilityGate::SqliteAccess],
        summary: "Grouped local SQLite search actions: fulltext, \
                  notes_annotations",
        example: Some(r#"{"action":"fulltext","query":"borrow checker"}"#),
        search_text: "zotero_sqlite_search sqlite grouped local sqlite search \
                      actions fulltext notes_annotations zotero_sqlite_access",
    },
    CapabilityInfo {
        name: "zotero_literature_review",
        kind: CapabilityKind::Prompt,
        domain: CapabilityDomain::Prompts,
        requires: &[],
        summary: "Generate a literature review prompt for a collection",
        example: Some(r#"{"collection_key":"COLKEY"}"#),
        search_text: "zotero_literature_review prompts generate a literature \
                      review prompt for a collection",
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ToolVisibility {
    CompactUngated,
    CompactSqlite,
    CompactWrite,
    LegacyUngated,
    LegacySqlite,
    LegacyWrite,
}

impl ToolVisibility {
    fn is_compact_visible(self, state: &AppState) -> bool {
        match self {
            Self::CompactUngated => true,
            Self::CompactSqlite => state.sqlite_access,
            Self::CompactWrite => state.write_enabled,
            Self::LegacyUngated | Self::LegacySqlite | Self::LegacyWrite => {
                false
            }
        }
    }

    fn is_gated_visible(self, state: &AppState) -> bool {
        match self {
            Self::CompactSqlite | Self::LegacySqlite => state.sqlite_access,
            Self::CompactWrite | Self::LegacyWrite => state.write_enabled,
            Self::CompactUngated | Self::LegacyUngated => true,
        }
    }
}

fn tool_visibility(name: &str) -> ToolVisibility {
    match name {
        "zotero_discover" | "zotero_status" | "zotero_search"
        | "zotero_pdf" | "zotero_notes" | "zotero_collections"
        | "zotero_items" | "zotero_tags" | "zotero_relations"
        | "better_bibtex" | "better_notes" | "search" | "fetch" => {
            ToolVisibility::CompactUngated
        }
        "zotero_sqlite_search" => ToolVisibility::CompactSqlite,
        "zotero_notes_write"
        | "zotero_collections_write"
        | "zotero_items_write"
        | "zotero_tags_write"
        | "zotero_relations_write" => ToolVisibility::CompactWrite,
        "zotero_fulltext_search" | "zotero_search_notes_annotations" => {
            ToolVisibility::LegacySqlite
        }
        "zotero_create_note"
        | "zotero_create_collection"
        | "zotero_manage_collections"
        | "zotero_update_item"
        | "zotero_attach_file"
        | "zotero_batch_update_tags"
        | "zotero_add_item_relation"
        | "zotero_remove_item_relation"
        | "zotero_delete_item"
        | "zotero_trash_item"
        | "zotero_restore_item"
        | "zotero_delete_collection"
        | "zotero_create_annotation"
        | "zotero_add_by_identifier"
        | "zotero_update_collection"
        | "zotero_rename_tag"
        | "zotero_delete_tags"
        | "better_bibtex_regenerate_citekeys"
        | "better_bibtex_autoexport_add" => ToolVisibility::LegacyWrite,
        _ => ToolVisibility::LegacyUngated,
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroSearchCommand {
    Items(SearchItemsArgs),
    Tag(SearchByTagArgs),
    CitationKey(SearchByCitationKeyArgs),
    Advanced(AdvancedSearchArgs),
    Duplicates(FindDuplicatesArgs),
    Coverage(LibraryCoverageArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroSqliteSearchCommand {
    Fulltext(FulltextSearchArgs),
    NotesAnnotations(SearchNotesAnnotationsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroPdfCommand {
    Path(GetPdfPathArgs),
    ReadPages(ReadPdfPagesArgs),
    Outline(GetPdfOutlineArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroNotesCommand {
    List(GetNotesArgs),
    Synthesize(SynthesizeAnnotationsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroNotesWriteCommand {
    Create(CreateNoteArgs),
    Annotation(CreateAnnotationArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroCollectionsCommand {
    Items(GetCollectionItemsArgs),
    Search(SearchCollectionsArgs),
    Unfiled(GetUnfiledItemsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroCollectionsWriteCommand {
    Create(CreateCollectionArgs),
    Manage(ManageCollectionsArgs),
    Update(UpdateCollectionArgs),
    Delete(DeleteCollectionArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroItemsCommand {
    Recent(GetRecentArgs),
    Get(GetItemArgs),
    Metadata(GetItemMetadataArgs),
    Children(GetItemChildrenArgs),
    Fulltext(GetItemFulltextArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroItemsWriteCommand {
    Update(UpdateItemArgs),
    Delete(DeleteItemArgs),
    Trash(TrashItemArgs),
    Restore(TrashItemArgs),
    AddByIdentifier(AddByIdentifierArgs),
    AttachFile(AttachFileArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroTagsCommand {
    List(ListTagsArgs),
    Search(SearchByTagArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroTagsWriteCommand {
    BatchUpdate(BatchUpdateTagsArgs),
    Rename(RenameTagArgs),
    Delete(DeleteTagsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroRelationsCommand {
    Get(GetRelatedItemsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum ZoteroRelationsWriteCommand {
    Add(AddItemRelationArgs),
    Remove(RemoveItemRelationArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum BetterBibtexCommand {
    Citekeys(GetCitekeysArgs),
    Regenerate(RegenerateKeysArgs),
    Export(ExportItemsArgs),
    Bibliography(BibliographyArgs),
    ScanAux(ScanAuxArgs),
    PandocFilter(PandocFilterArgs),
    AutoexportAdd(AutoExportAddArgs),
    Search(BetterBibtexSearchArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
pub(crate) enum BetterNotesCommand {
    Export(NoteExportArgs),
    FromMarkdown(FromMarkdownArgs),
    RunTemplate(RunTemplateArgs),
    Relations(NoteRelationsArgs),
    Tree(NoteTreeArgs),
}

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

    pub(crate) fn visible_tools_for_state(
        state: &AppState,
    ) -> Vec<rmcp::model::Tool> {
        let mut tools = Self::tool_router().list_all();
        tools.retain(|tool| Self::is_visible_tool(state, tool.name.as_ref()));
        tools
    }

    fn is_visible_tool(state: &AppState, name: &str) -> bool {
        match state.tool_mode {
            ToolExposureMode::All => true,
            ToolExposureMode::Gated => Self::is_gated_tool(state, name),
            ToolExposureMode::Compact => Self::is_compact_tool(state, name),
        }
    }

    fn is_gated_tool(state: &AppState, name: &str) -> bool {
        tool_visibility(name).is_gated_visible(state)
    }

    fn is_compact_tool(state: &AppState, name: &str) -> bool {
        tool_visibility(name).is_compact_visible(state)
    }

    fn discover_capabilities(
        &self,
        args: &DiscoverArgs,
    ) -> Vec<CapabilityInfo> {
        let query = args.query.as_ref().map(|value| value.to_lowercase());
        CAPABILITIES
            .iter()
            .copied()
            .filter(|capability| {
                args.include_disabled == Some(true)
                    || self.is_capability_enabled(*capability)
            })
            .filter(|capability| {
                args.domain.is_none_or(|domain| capability.domain == domain)
            })
            .filter(|capability| {
                query
                    .as_deref()
                    .is_none_or(|query| capability.search_text.contains(query))
            })
            .collect()
    }

    fn is_capability_enabled(&self, capability: CapabilityInfo) -> bool {
        !capability.requires.iter().any(|requirement| {
            (*requirement == CapabilityGate::WriteEnabled
                && !self.state.write_enabled)
                || (*requirement == CapabilityGate::SqliteAccess
                    && !self.state.sqlite_access)
        })
    }

    pub(crate) fn zotero_discover_impl(
        &self,
        args: &DiscoverArgs,
    ) -> CallToolResult {
        #[derive(Serialize)]
        struct DiscoveryResponse {
            capabilities: Vec<CapabilityInfo>,
        }

        crate::mcp::json_success(&DiscoveryResponse {
            capabilities: self.discover_capabilities(args),
        })
    }
}

impl ServerHandler for ZoteroMcpServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        // 2025-06-18 is the first revision defining `title` on tools,
        // resources, and prompts, and `_meta` on resource contents.
        .with_protocol_version(ProtocolVersion::V_2025_06_18)
        .with_server_info(
            Implementation::new("zotero-mcp-rs", env!("CARGO_PKG_VERSION"))
                .with_title("Zotero"),
        )
        .with_instructions(SERVER_INSTRUCTIONS)
    }

    fn list_tools(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<rmcp::model::ListToolsResult, rmcp::ErrorData>>
    {
        std::future::ready(Ok(rmcp::model::ListToolsResult::with_all_items(
            Self::visible_tools_for_state(&self.state),
        )))
    }

    async fn call_tool(
        &self,
        param: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        let ctx = rmcp::handler::server::tool::ToolCallContext::new(
            self, param, context,
        );
        Self::tool_router().call(ctx).await
    }

    fn list_resources(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<
        Output = Result<rmcp::model::ListResourcesResult, rmcp::ErrorData>,
    > {
        std::future::ready(Ok(Self::list_resources_impl()))
    }

    fn list_resource_templates(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<
        Output = Result<
            rmcp::model::ListResourceTemplatesResult,
            rmcp::ErrorData,
        >,
    > {
        std::future::ready(Ok(Self::list_resource_templates_impl()))
    }

    async fn read_resource(
        &self,
        param: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ReadResourceResponse, rmcp::ErrorData> {
        self.read_resource_impl(&param.uri).await.map(Into::into)
    }

    fn list_prompts(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<rmcp::model::ListPromptsResult, rmcp::ErrorData>>
    {
        std::future::ready(Ok(Self::list_prompts_impl()))
    }

    fn get_prompt(
        &self,
        param: rmcp::model::GetPromptRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResponse, rmcp::ErrorData>> {
        std::future::ready(
            Self::get_prompt_impl(&param.name, param.arguments.as_ref())
                .map(Into::into),
        )
    }
}

/// Hosts all `#[tool]` router forwarder methods.
#[tool_router]
impl ZoteroMcpServer {
    // --- Zotero Diagnostics & Status ---

    #[tool(
        name = "zotero_discover",
        description = "Discover Zotero tools, resource templates, prompts, \
                       required env flags, and examples without loading every \
                       detailed tool schema",
        annotations(
            title = "Discover Zotero Capabilities",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_discover(
        &self,
        Parameters(args): Parameters<DiscoverArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.zotero_discover_impl(&args))
    }

    #[tool(
        name = "zotero_search",
        description = "Grouped Zotero search router. action: items, tag, \
                       citation_key, advanced, duplicates, coverage",
        annotations(
            title = "Search Zotero Library",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_search(
        &self,
        Parameters(args): Parameters<ZoteroSearchCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroSearchCommand::Items(args) => {
                self.zotero_search_items_impl(args).await
            }
            ZoteroSearchCommand::Tag(args) => {
                self.zotero_search_by_tag_impl(args).await
            }
            ZoteroSearchCommand::CitationKey(args) => {
                self.zotero_search_by_citation_key_impl(args).await
            }
            ZoteroSearchCommand::Advanced(args) => {
                self.zotero_advanced_search_impl(args).await
            }
            ZoteroSearchCommand::Duplicates(args) => {
                self.zotero_find_duplicates_impl(args).await
            }
            ZoteroSearchCommand::Coverage(args) => {
                self.zotero_library_coverage_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_sqlite_search",
        description = "Grouped local SQLite search router. action: fulltext, \
                       notes_annotations",
        annotations(
            title = "Search Zotero SQLite Database",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_sqlite_search(
        &self,
        Parameters(args): Parameters<ZoteroSqliteSearchCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroSqliteSearchCommand::Fulltext(args) => {
                self.zotero_fulltext_search_impl(args).await
            }
            ZoteroSqliteSearchCommand::NotesAnnotations(args) => {
                self.zotero_search_notes_annotations_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_pdf",
        description = "Grouped Zotero PDF router. action: path, read_pages, \
                       outline",
        annotations(
            title = "Read Zotero PDFs",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_pdf(
        &self,
        Parameters(args): Parameters<ZoteroPdfCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroPdfCommand::Path(args) => {
                self.zotero_get_pdf_path_impl(args).await
            }
            ZoteroPdfCommand::ReadPages(args) => {
                self.zotero_read_pdf_pages_impl(args).await
            }
            ZoteroPdfCommand::Outline(args) => {
                self.zotero_get_pdf_outline_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_notes",
        description = "Grouped Zotero notes read router. action: list, \
                       synthesize",
        annotations(
            title = "Read Zotero Notes",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_notes(
        &self,
        Parameters(args): Parameters<ZoteroNotesCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroNotesCommand::List(args) => {
                self.zotero_get_notes_impl(args).await
            }
            ZoteroNotesCommand::Synthesize(args) => {
                self.zotero_synthesize_annotations_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_notes_write",
        description = "Grouped Zotero notes write router. action: create, \
                       annotation",
        annotations(
            title = "Write Zotero Notes",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_notes_write(
        &self,
        Parameters(args): Parameters<ZoteroNotesWriteCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroNotesWriteCommand::Create(args) => {
                self.zotero_create_note_impl(args).await
            }
            ZoteroNotesWriteCommand::Annotation(args) => {
                self.zotero_create_annotation_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_collections",
        description = "Grouped Zotero collection read router. action: items, \
                       search, unfiled",
        annotations(
            title = "Read Zotero Collections",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_collections(
        &self,
        Parameters(args): Parameters<ZoteroCollectionsCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroCollectionsCommand::Items(args) => {
                self.zotero_get_collection_items_impl(args).await
            }
            ZoteroCollectionsCommand::Search(args) => {
                self.zotero_search_collections_impl(args).await
            }
            ZoteroCollectionsCommand::Unfiled(args) => {
                self.zotero_get_unfiled_items_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_collections_write",
        description = "Grouped Zotero collection write router. action: \
                       create, manage, update, delete",
        annotations(
            title = "Write Zotero Collections",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_collections_write(
        &self,
        Parameters(args): Parameters<ZoteroCollectionsWriteCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroCollectionsWriteCommand::Create(args) => {
                self.zotero_create_collection_impl(args).await
            }
            ZoteroCollectionsWriteCommand::Manage(args) => {
                self.zotero_manage_collections_impl(args).await
            }
            ZoteroCollectionsWriteCommand::Update(args) => {
                self.zotero_update_collection_impl(args).await
            }
            ZoteroCollectionsWriteCommand::Delete(args) => {
                self.zotero_delete_collection_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_items",
        description = "Grouped Zotero item read router. action: recent, get, \
                       metadata, children, fulltext",
        annotations(
            title = "Read Zotero Items",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_items(
        &self,
        Parameters(args): Parameters<ZoteroItemsCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroItemsCommand::Recent(args) => {
                self.zotero_get_recent_impl(args).await
            }
            ZoteroItemsCommand::Get(args) => {
                self.zotero_get_item_impl(args).await
            }
            ZoteroItemsCommand::Metadata(args) => {
                self.zotero_get_item_metadata_impl(args).await
            }
            ZoteroItemsCommand::Children(args) => {
                self.zotero_get_item_children_impl(args).await
            }
            ZoteroItemsCommand::Fulltext(args) => {
                self.zotero_get_item_fulltext_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_items_write",
        description = "Grouped Zotero item write router. action: update, \
                       delete, trash, restore, add_by_identifier, attach_file",
        annotations(
            title = "Write Zotero Items",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_items_write(
        &self,
        Parameters(args): Parameters<ZoteroItemsWriteCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroItemsWriteCommand::Update(args) => {
                self.zotero_update_item_impl(args).await
            }
            ZoteroItemsWriteCommand::Delete(args) => {
                self.zotero_delete_item_impl(args).await
            }
            ZoteroItemsWriteCommand::Trash(args) => {
                self.zotero_trash_item_impl(args).await
            }
            ZoteroItemsWriteCommand::Restore(args) => {
                self.zotero_restore_item_impl(args).await
            }
            ZoteroItemsWriteCommand::AddByIdentifier(args) => {
                self.zotero_add_by_identifier_impl(args).await
            }
            ZoteroItemsWriteCommand::AttachFile(args) => {
                self.zotero_attach_file_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_tags",
        description = "Grouped Zotero tag read router. action: list, search",
        annotations(
            title = "Read Zotero Tags",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_tags(
        &self,
        Parameters(args): Parameters<ZoteroTagsCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroTagsCommand::List(args) => {
                self.zotero_list_tags_impl(args).await
            }
            ZoteroTagsCommand::Search(args) => {
                self.zotero_search_by_tag_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_tags_write",
        description = "Grouped Zotero tag write router. action: batch_update, \
                       rename, delete",
        annotations(
            title = "Write Zotero Tags",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_tags_write(
        &self,
        Parameters(args): Parameters<ZoteroTagsWriteCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroTagsWriteCommand::BatchUpdate(args) => {
                self.zotero_batch_update_tags_impl(args).await
            }
            ZoteroTagsWriteCommand::Rename(args) => {
                self.zotero_rename_tag_impl(args).await
            }
            ZoteroTagsWriteCommand::Delete(args) => {
                self.zotero_delete_tags_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_relations",
        description = "Grouped Zotero relation read router. action: get",
        annotations(
            title = "Read Zotero Item Relations",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_relations(
        &self,
        Parameters(args): Parameters<ZoteroRelationsCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroRelationsCommand::Get(args) => {
                self.zotero_get_related_items_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_relations_write",
        description = "Grouped Zotero relation write router. action: add, \
                       remove",
        annotations(
            title = "Write Zotero Item Relations",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_relations_write(
        &self,
        Parameters(args): Parameters<ZoteroRelationsWriteCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroRelationsWriteCommand::Add(args) => {
                self.zotero_add_item_relation_impl(args).await
            }
            ZoteroRelationsWriteCommand::Remove(args) => {
                self.zotero_remove_item_relation_impl(args).await
            }
        }
    }

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

    #[tool(
        name = "better_notes",
        description = "Grouped Better Notes router. action: export, \
                       from_markdown, run_template, relations, tree",
        annotations(
            title = "Better Notes",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn better_notes(
        &self,
        Parameters(args): Parameters<BetterNotesCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            BetterNotesCommand::Export(args) => {
                self.better_notes_export_impl(args).await
            }
            BetterNotesCommand::FromMarkdown(args) => {
                self.better_notes_from_markdown_impl(args).await
            }
            BetterNotesCommand::RunTemplate(args) => {
                self.better_notes_run_template_impl(args).await
            }
            BetterNotesCommand::Relations(args) => {
                self.better_notes_get_relations_impl(args).await
            }
            BetterNotesCommand::Tree(args) => {
                self.better_notes_get_tree_impl(args).await
            }
        }
    }

    #[tool(
        name = "zotero_status",
        description = "Check Zotero Local API availability, version, and \
                       connectivity",
        annotations(
            title = "Check Zotero Connection",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Fetch recently modified library items (notes excluded)",
        annotations(
            title = "Recently Modified Items",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Search items by title, creator, year, or fulltext query",
        annotations(
            title = "Search Items",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Fetch a single Zotero item by its key",
        annotations(
            title = "Get Item",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       string",
        annotations(
            title = "Get Item Metadata",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Fetch items inside a specific Zotero collection",
        annotations(
            title = "Get Collection Items",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       key",
        annotations(
            title = "Get Item Children",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Get Zotero's indexed fulltext for an item",
        annotations(
            title = "Get Item Full Text",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       attachment",
        annotations(
            title = "Locate Item PDF",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Extract raw text from specific 1-based pages of a PDF",
        annotations(
            title = "Read PDF Pages",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       for an item's PDF attachment or a direct PDF path",
        annotations(
            title = "Get PDF Outline",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Fetch all note child items for a given item key",
        annotations(
            title = "Get Item Notes",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       permission)",
        annotations(
            title = "Create Note",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
                       permission)",
        annotations(
            title = "Create Collection",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        description = "Search collections by collection name query",
        annotations(
            title = "Search Collections",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       write permission)",
        annotations(
            title = "Add or Remove Collection Items",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
                       (requires write permission)",
        annotations(
            title = "Update Item",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
                       permission)",
        annotations(
            title = "Attach File to Item",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
                       permission)",
        annotations(
            title = "Batch Update Item Tags",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "zotero_get_related_items",
        description = "Get items related to an item via Zotero's dc:relation \
                       links",
        annotations(
            title = "Get Related Items",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_related_items(
        &self,
        Parameters(args): Parameters<GetRelatedItemsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_related_items_impl(args).await
    }

    #[tool(
        name = "zotero_add_item_relation",
        description = "Link two items as related (bidirectional, dc:relation) \
                       (requires write permission)",
        annotations(
            title = "Add Item Relation",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_add_item_relation(
        &self,
        Parameters(args): Parameters<AddItemRelationArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_add_item_relation_impl(args).await
    }

    #[tool(
        name = "zotero_remove_item_relation",
        description = "Remove the relation between two items (bidirectional, \
                       dc:relation) (requires write permission)",
        annotations(
            title = "Remove Item Relation",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_remove_item_relation(
        &self,
        Parameters(args): Parameters<RemoveItemRelationArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_remove_item_relation_impl(args).await
    }

    #[tool(
        name = "zotero_delete_item",
        description = "Permanently delete an item (article, note, annotation, \
                       or attachment) (requires write permission)",
        annotations(
            title = "Delete Item Permanently",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
                       permission)",
        annotations(
            title = "Move Item to Trash",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        description = "Restore an item from trash (requires write permission)",
        annotations(
            title = "Restore Item from Trash",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
                       deleted (requires write permission)",
        annotations(
            title = "Delete Collection",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
                       collection by matching title or DOI",
        annotations(
            title = "Find Duplicate Items",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Search Zotero items by tag string",
        annotations(
            title = "Search Items by Tag",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Search Zotero items by citation key string",
        annotations(
            title = "Search Items by Citation Key",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       fields",
        annotations(
            title = "Advanced Item Search",
            read_only_hint = true,
            open_world_hint = false
        )
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
        name = "zotero_fulltext_search",
        description = "Search Zotero's local sqlite database for full-text \
                       matches across titles, creators, and indexed PDF text \
                       (requires ZOTERO_SQLITE_ACCESS=1)",
        annotations(
            title = "Full-Text Search (SQLite)",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_fulltext_search(
        &self,
        Parameters(args): Parameters<FulltextSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_fulltext_search_impl(args).await
    }

    #[tool(
        name = "zotero_search_notes_annotations",
        description = "Search Zotero's local sqlite database for note and PDF \
                       annotation text (requires ZOTERO_SQLITE_ACCESS=1)",
        annotations(
            title = "Search Notes and Annotations",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_notes_annotations(
        &self,
        Parameters(args): Parameters<SearchNotesAnnotationsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_notes_annotations_impl(args).await
    }

    #[tool(
        name = "zotero_library_coverage",
        description = "Analyze library or collection statistics for PDF, DOI, \
                       and note coverage",
        annotations(
            title = "Library Coverage Report",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "List top-level items not in any collection",
        annotations(
            title = "List Unfiled Items",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       structured Markdown",
        annotations(
            title = "Synthesize Annotations",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       attachment (requires write permission)",
        annotations(
            title = "Create PDF Annotation",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
                       match is already present) (requires write permission)",
        annotations(
            title = "Add Item by Identifier",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
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
                       write permission)",
        annotations(
            title = "Rename or Move Collection",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        description = "List all tag names in the library",
        annotations(
            title = "List Tags",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       it (requires write permission)",
        annotations(
            title = "Rename Tag",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
                       write permission)",
        annotations(
            title = "Delete Tags",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        description = "Fetch citation keys for Zotero items via Better BibTeX",
        annotations(
            title = "Get Citation Keys",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       permission)",
        annotations(
            title = "Regenerate Citation Keys",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        description = "Export citekeys using a Better BibTeX translator",
        annotations(
            title = "Export Items (Better BibTeX)",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Format a bibliography for citekeys with Better BibTeX",
        annotations(
            title = "Format Bibliography",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       BibTeX",
        annotations(
            title = "Scan LaTeX Aux File",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        description = "Process citekeys through Better BibTeX's Pandoc filter",
        annotations(
            title = "Pandoc Citation Filter",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       path (requires write permission)",
        annotations(
            title = "Register Auto-Export",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        description = "Search items using Better BibTeX's query engine",
        annotations(
            title = "Search (Better BibTeX)",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       Better Notes",
        annotations(
            title = "Export Note",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       via Better Notes",
        annotations(
            title = "Create Note from Markdown",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        description = "Execute a Better Notes template against an item",
        annotations(
            title = "Run Note Template",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       Better Notes",
        annotations(
            title = "Get Note Relations",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       via Better Notes",
        annotations(
            title = "Get Note Tree",
            read_only_hint = true,
            open_world_hint = false
        )
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
        description = "Connector search tool - search Zotero items by query",
        annotations(
            title = "Search Zotero",
            read_only_hint = true,
            open_world_hint = false
        )
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
                       item ID/key",
        annotations(
            title = "Fetch Zotero Item",
            read_only_hint = true,
            open_world_hint = false
        )
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
    use crate::state::{AppState, ToolExposureMode};
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
            assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
            assert_eq!(info.server_info.title.as_deref(), Some("Zotero"));
            assert!(info.capabilities.tools.is_some());
            assert!(info.capabilities.resources.is_some());
            assert!(info.capabilities.prompts.is_some());
        }

        #[test]
        fn get_info_advertises_title_capable_protocol_revision() {
            // Arrange
            let server = ZoteroMcpServer::new(AppState::from_env());

            // Act
            let info = server.get_info();

            // Assert
            assert_eq!(
                info.protocol_version,
                rmcp::model::ProtocolVersion::V_2025_06_18
            );
        }

        #[test]
        fn get_info_instructions_describe_compact_navigation() {
            let server = ZoteroMcpServer::new(AppState::from_env());

            let instructions = server.get_info().instructions.unwrap();

            assert!(instructions.contains("zotero_discover"));
            assert!(instructions.contains("zotero://items/{item_key}"));
            assert!(instructions.contains("ZOTERO_WRITE_ENABLED"));
        }

        #[test]
        fn tool_router_lists_all_registered_tools() {
            // Act
            let tools = ZoteroMcpServer::tool_router().list_all();

            // Assert
            assert!(!tools.is_empty());
        }

        #[test]
        fn every_tool_declares_behaviour_annotations() {
            // Act
            let tools = ZoteroMcpServer::tool_router().list_all();

            // Assert
            for tool in &tools {
                assert!(
                    tool.annotations.is_some(),
                    "{} is missing annotations",
                    tool.name
                );
                let annotations =
                    tool.annotations.as_ref().expect("annotations");
                assert!(
                    annotations.title.is_some(),
                    "{} is missing a display title",
                    tool.name
                );
                assert!(
                    annotations.read_only_hint.is_some(),
                    "{} is missing read_only_hint",
                    tool.name
                );
                assert!(
                    annotations.open_world_hint.is_some(),
                    "{} is missing open_world_hint",
                    tool.name
                );
            }
        }

        #[test]
        fn mutating_tools_are_annotated_as_writes() {
            // Arrange
            let tools = ZoteroMcpServer::tool_router().list_all();

            // Assert
            for tool in &tools {
                let annotations =
                    tool.annotations.as_ref().expect("annotations");
                let read_only =
                    annotations.read_only_hint.expect("read_only_hint");
                let mutates = matches!(
                    tool_visibility(tool.name.as_ref()),
                    ToolVisibility::CompactWrite | ToolVisibility::LegacyWrite
                );
                if mutates {
                    assert!(
                        !read_only,
                        "{} mutates but is annotated read-only",
                        tool.name
                    );
                }
                if !read_only {
                    assert!(
                        annotations.destructive_hint.is_some()
                            && annotations.idempotent_hint.is_some(),
                        "{} is a write tool and must declare destructive and \
                         idempotent hints",
                        tool.name
                    );
                }
            }
        }

        fn visible_tool_names(state: &AppState) -> Vec<String> {
            let mut names: Vec<_> =
                ZoteroMcpServer::visible_tools_for_state(state)
                    .into_iter()
                    .map(|tool| tool.name.to_string())
                    .collect();
            names.sort();
            names
        }

        #[test]
        fn all_mode_keeps_registered_legacy_tools() {
            let mut state = AppState::from_env();
            state.tool_mode = ToolExposureMode::All;

            let names = visible_tool_names(&state);

            assert!(names.contains(&"zotero_get_item".to_owned()));
            assert!(names.contains(&"zotero_create_note".to_owned()));
            assert!(names.contains(&"zotero_fulltext_search".to_owned()));
        }

        #[test]
        fn gated_mode_hides_disabled_write_and_sqlite_tools() {
            let mut state = AppState::from_env();
            state.tool_mode = ToolExposureMode::Gated;
            state.write_enabled = false;
            state.sqlite_access = false;

            let names = visible_tool_names(&state);

            assert!(names.contains(&"zotero_get_item".to_owned()));
            assert!(!names.contains(&"zotero_create_note".to_owned()));
            assert!(!names.contains(&"zotero_fulltext_search".to_owned()));
            assert!(!names.contains(&"zotero_notes_write".to_owned()));
        }

        #[test]
        fn compact_mode_lists_base_grouped_tools_only() {
            let mut state = AppState::from_env();
            state.tool_mode = ToolExposureMode::Compact;
            state.write_enabled = false;
            state.sqlite_access = false;

            let names = visible_tool_names(&state);

            assert_eq!(names, [
                "better_bibtex",
                "better_notes",
                "fetch",
                "search",
                "zotero_collections",
                "zotero_discover",
                "zotero_items",
                "zotero_notes",
                "zotero_pdf",
                "zotero_relations",
                "zotero_search",
                "zotero_status",
                "zotero_tags",
            ]);
            assert!(!names.contains(&"zotero_get_item".to_owned()));
            assert!(!names.contains(&"zotero_create_note".to_owned()));
            assert!(!names.contains(&"zotero_fulltext_search".to_owned()));
        }

        #[test]
        fn compact_mode_adds_sqlite_group_when_enabled() {
            let mut state = AppState::from_env();
            state.tool_mode = ToolExposureMode::Compact;
            state.sqlite_access = true;

            let names = visible_tool_names(&state);

            assert!(names.contains(&"zotero_sqlite_search".to_owned()));
            assert!(!names.contains(&"zotero_fulltext_search".to_owned()));
        }

        #[test]
        fn compact_mode_adds_write_groups_when_enabled() {
            let mut state = AppState::from_env();
            state.tool_mode = ToolExposureMode::Compact;
            state.write_enabled = true;

            let names = visible_tool_names(&state);

            assert!(names.contains(&"zotero_notes_write".to_owned()));
            assert!(names.contains(&"zotero_items_write".to_owned()));
            assert!(!names.contains(&"zotero_create_note".to_owned()));
        }

        #[test]
        fn grouped_routers_and_legacy_tools_are_registered() {
            let names: Vec<_> = ZoteroMcpServer::tool_router()
                .list_all()
                .into_iter()
                .map(|tool| tool.name.to_string())
                .collect();

            assert!(names.contains(&"zotero_search".to_owned()));
            assert!(names.contains(&"zotero_items_write".to_owned()));
            assert!(names.contains(&"better_notes".to_owned()));
            assert!(names.contains(&"zotero_search_items".to_owned()));
        }

        fn discover_json(
            server: &ZoteroMcpServer,
            args: DiscoverArgs,
        ) -> serde_json::Value {
            let res = server.zotero_discover_impl(&args);
            let text = res
                .content
                .first()
                .and_then(|content| content.as_text())
                .map(|text| text.text.as_str())
                .unwrap_or_default();
            serde_json::from_str(text).unwrap()
        }

        #[test]
        fn discover_omits_write_capabilities_by_default() {
            let mut state = AppState::from_env();
            state.write_enabled = false;
            state.sqlite_access = false;
            let server = ZoteroMcpServer::new(state);

            let json = discover_json(&server, DiscoverArgs {
                query: None,
                domain: None,
                include_disabled: None,
            });
            let capabilities =
                json["capabilities"].as_array().expect("capabilities array");

            assert!(!capabilities.iter().any(|capability| {
                capability["requires"]
                    .as_array()
                    .expect("requires")
                    .iter()
                    .any(|requirement| requirement == "ZOTERO_WRITE_ENABLED")
            }));
        }

        #[test]
        fn discover_can_include_disabled_capabilities() {
            let mut state = AppState::from_env();
            state.write_enabled = false;
            state.sqlite_access = false;
            let server = ZoteroMcpServer::new(state);

            let json = discover_json(&server, DiscoverArgs {
                query: None,
                domain: None,
                include_disabled: Some(true),
            });
            let capabilities =
                json["capabilities"].as_array().expect("capabilities array");

            assert!(capabilities.iter().any(|capability| {
                capability["requires"]
                    .as_array()
                    .expect("requires")
                    .iter()
                    .any(|requirement| requirement == "ZOTERO_WRITE_ENABLED")
            }));
        }
    }
}
