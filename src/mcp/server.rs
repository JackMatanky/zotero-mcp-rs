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
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

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
    pub(crate) domain: Option<String>,
    pub(crate) include_disabled: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GroupedToolArgs {
    pub(crate) action: String,
    #[serde(flatten)]
    pub(crate) args: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Copy, Serialize)]
struct CapabilityInfo {
    name: &'static str,
    kind: &'static str,
    domain: &'static str,
    requires: &'static [&'static str],
    summary: &'static str,
    example: Option<&'static str>,
}

static CAPABILITIES: &[CapabilityInfo] = &[
    CapabilityInfo {
        name: "zotero_discover",
        kind: "tool",
        domain: "discovery",
        requires: &[],
        summary: "Find Zotero tools, resources, prompts, env gates, and \
                  examples",
        example: Some(r#"{"query":"notes"}"#),
    },
    CapabilityInfo {
        name: "zotero://items/{item_key}",
        kind: "resource",
        domain: "items",
        requires: &[],
        summary: "Read one Zotero item by key",
        example: Some("zotero://items/ITEMKEY"),
    },
    CapabilityInfo {
        name: "zotero://collections/{collection_key}/items",
        kind: "resource",
        domain: "collections",
        requires: &[],
        summary: "Read collection items",
        example: Some("zotero://collections/COLKEY/items"),
    },
    CapabilityInfo {
        name: "zotero_search",
        kind: "tool",
        domain: "search",
        requires: &[],
        summary: "Grouped search actions: items, tag, citation_key, advanced, \
                  duplicates, coverage",
        example: Some(r#"{"action":"items","query":"rust","limit":10}"#),
    },
    CapabilityInfo {
        name: "zotero_items",
        kind: "tool",
        domain: "items",
        requires: &[],
        summary: "Grouped item read actions: recent, get, metadata, children, \
                  fulltext",
        example: Some(r#"{"action":"get","item_key":"ITEMKEY"}"#),
    },
    CapabilityInfo {
        name: "zotero_notes",
        kind: "tool",
        domain: "notes",
        requires: &[],
        summary: "Grouped note read actions: list, synthesize",
        example: Some(r#"{"action":"list","item_key":"ITEMKEY"}"#),
    },
    CapabilityInfo {
        name: "zotero_items_write",
        kind: "tool",
        domain: "items",
        requires: &["ZOTERO_WRITE_ENABLED"],
        summary: "Grouped item write actions: update, delete, trash, restore, \
                  add_by_identifier, attach_file",
        example: Some(r#"{"action":"trash","item_key":"ITEMKEY"}"#),
    },
    CapabilityInfo {
        name: "zotero_notes_write",
        kind: "tool",
        domain: "notes",
        requires: &["ZOTERO_WRITE_ENABLED"],
        summary: "Grouped note write actions: create, annotation",
        example: Some(
            r##"{"action":"create","parent_key":"ITEMKEY","markdown":"# Note"}"##,
        ),
    },
    CapabilityInfo {
        name: "zotero_local_search",
        kind: "tool",
        domain: "local",
        requires: &["ZOTERO_SQLITE_ACCESS"],
        summary: "Grouped local SQLite search actions: fulltext, \
                  notes_annotations",
        example: Some(r#"{"action":"fulltext","query":"borrow checker"}"#),
    },
    CapabilityInfo {
        name: "zotero_literature_review",
        kind: "prompt",
        domain: "prompts",
        requires: &[],
        summary: "Generate a literature review prompt for a collection",
        example: Some(r#"{"collection_key":"COLKEY"}"#),
    },
];

const COMPACT_BASE_TOOLS: &[&str] = &[
    "zotero_discover",
    "zotero_status",
    "zotero_search",
    "zotero_pdf",
    "zotero_notes",
    "zotero_collections",
    "zotero_items",
    "zotero_tags",
    "zotero_relations",
    "better_bibtex",
    "better_notes",
    "search",
    "fetch",
];

const COMPACT_SQLITE_TOOLS: &[&str] = &["zotero_local_search"];

const COMPACT_WRITE_TOOLS: &[&str] = &[
    "zotero_notes_write",
    "zotero_collections_write",
    "zotero_items_write",
    "zotero_tags_write",
    "zotero_relations_write",
];

const SQLITE_TOOLS: &[&str] = &[
    "zotero_local_search",
    "zotero_fulltext_search",
    "zotero_search_notes_annotations",
];

const WRITE_TOOLS: &[&str] = &[
    "zotero_notes_write",
    "zotero_collections_write",
    "zotero_items_write",
    "zotero_tags_write",
    "zotero_relations_write",
    "zotero_create_note",
    "zotero_create_collection",
    "zotero_manage_collections",
    "zotero_update_item",
    "zotero_attach_file",
    "zotero_batch_update_tags",
    "zotero_add_item_relation",
    "zotero_remove_item_relation",
    "zotero_delete_item",
    "zotero_trash_item",
    "zotero_restore_item",
    "zotero_delete_collection",
    "zotero_create_annotation",
    "zotero_add_by_identifier",
    "zotero_update_collection",
    "zotero_rename_tag",
    "zotero_delete_tags",
    "better_bibtex_regenerate_citekeys",
    "better_bibtex_autoexport_add",
];

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
            ToolExposureMode::Filtered => Self::is_filtered_tool(state, name),
            ToolExposureMode::Compact => Self::is_compact_tool(state, name),
        }
    }

    fn is_filtered_tool(state: &AppState, name: &str) -> bool {
        (state.write_enabled || !WRITE_TOOLS.contains(&name))
            && (state.sqlite_access || !SQLITE_TOOLS.contains(&name))
    }

    fn is_compact_tool(state: &AppState, name: &str) -> bool {
        COMPACT_BASE_TOOLS.contains(&name)
            || (state.sqlite_access && COMPACT_SQLITE_TOOLS.contains(&name))
            || (state.write_enabled && COMPACT_WRITE_TOOLS.contains(&name))
    }

    fn discover_capabilities(
        &self,
        args: &DiscoverArgs,
    ) -> Vec<CapabilityInfo> {
        let query = args.query.as_ref().map(|value| value.to_lowercase());
        let domain = args.domain.as_ref().map(|value| value.to_lowercase());
        CAPABILITIES
            .iter()
            .copied()
            .filter(|capability| {
                args.include_disabled == Some(true)
                    || self.is_capability_enabled(capability)
            })
            .filter(|capability| {
                domain.as_deref().is_none_or(|domain| {
                    capability.domain.eq_ignore_ascii_case(domain)
                })
            })
            .filter(|capability| {
                query.as_deref().is_none_or(|query| {
                    capability.name.to_lowercase().contains(query)
                        || capability.domain.to_lowercase().contains(query)
                        || capability.summary.to_lowercase().contains(query)
                })
            })
            .collect()
    }

    fn is_capability_enabled(&self, capability: &CapabilityInfo) -> bool {
        !capability.requires.iter().any(|requirement| {
            (*requirement == "ZOTERO_WRITE_ENABLED"
                && !self.state.write_enabled)
                || (*requirement == "ZOTERO_SQLITE_ACCESS"
                    && !self.state.sqlite_access)
        })
    }

    fn grouped_args<T>(args: &GroupedToolArgs) -> Result<T, rmcp::ErrorData>
    where
        T: DeserializeOwned,
    {
        serde_json::from_value(serde_json::Value::Object(args.args.clone()))
            .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))
    }

    fn invalid_group_action(tool: &str, action: &str) -> rmcp::ErrorData {
        rmcp::ErrorData::invalid_params(
            format!("Unknown {tool} action: {action}"),
            None,
        )
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
            instructions: Some(SERVER_INSTRUCTIONS.to_owned()),
        }
    }

    fn list_tools(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<rmcp::model::ListToolsResult, rmcp::ErrorData>>
    {
        std::future::ready(Ok(rmcp::model::ListToolsResult {
            tools: Self::visible_tools_for_state(&self.state),
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

    fn list_resource_templates(
        &self,
        _param: Option<rmcp::model::PaginatedRequestParam>,
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
        name = "zotero_discover",
        description = "Discover Zotero tools, resource templates, prompts, \
                       required env flags, and examples without loading every \
                       detailed tool schema"
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
                       citation_key, advanced, duplicates, coverage"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_search(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "items" => {
                self.zotero_search_items_impl(Self::grouped_args(&args)?).await
            }
            "tag" => {
                self.zotero_search_by_tag_impl(Self::grouped_args(&args)?).await
            }
            "citation_key" => {
                self.zotero_search_by_citation_key_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            "advanced" => {
                self.zotero_advanced_search_impl(Self::grouped_args(&args)?)
                    .await
            }
            "duplicates" => {
                self.zotero_find_duplicates_impl(Self::grouped_args(&args)?)
                    .await
            }
            "coverage" => {
                self.zotero_library_coverage_impl(Self::grouped_args(&args)?)
                    .await
            }
            action => Err(Self::invalid_group_action("zotero_search", action)),
        }
    }

    #[tool(
        name = "zotero_local_search",
        description = "Grouped local SQLite search router. action: fulltext, \
                       notes_annotations"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_local_search(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "fulltext" => {
                self.zotero_fulltext_search_impl(Self::grouped_args(&args)?)
                    .await
            }
            "notes_annotations" => {
                self.zotero_search_notes_annotations_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            action => {
                Err(Self::invalid_group_action("zotero_local_search", action))
            }
        }
    }

    #[tool(
        name = "zotero_pdf",
        description = "Grouped Zotero PDF router. action: path, read_pages, \
                       outline"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_pdf(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "path" => {
                self.zotero_get_pdf_path_impl(Self::grouped_args(&args)?).await
            }
            "read_pages" => {
                self.zotero_read_pdf_pages_impl(Self::grouped_args(&args)?)
                    .await
            }
            "outline" => {
                self.zotero_get_pdf_outline_impl(Self::grouped_args(&args)?)
                    .await
            }
            action => Err(Self::invalid_group_action("zotero_pdf", action)),
        }
    }

    #[tool(
        name = "zotero_notes",
        description = "Grouped Zotero notes read router. action: list, \
                       synthesize"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_notes(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "list" => {
                self.zotero_get_notes_impl(Self::grouped_args(&args)?).await
            }
            "synthesize" => {
                self.zotero_synthesize_annotations_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            action => Err(Self::invalid_group_action("zotero_notes", action)),
        }
    }

    #[tool(
        name = "zotero_notes_write",
        description = "Grouped Zotero notes write router. action: create, \
                       annotation"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_notes_write(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "create" => {
                self.zotero_create_note_impl(Self::grouped_args(&args)?).await
            }
            "annotation" => {
                self.zotero_create_annotation_impl(Self::grouped_args(&args)?)
                    .await
            }
            action => {
                Err(Self::invalid_group_action("zotero_notes_write", action))
            }
        }
    }

    #[tool(
        name = "zotero_collections",
        description = "Grouped Zotero collection read router. action: items, \
                       search, unfiled"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_collections(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "items" => {
                self.zotero_get_collection_items_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            "search" => {
                self.zotero_search_collections_impl(Self::grouped_args(&args)?)
                    .await
            }
            "unfiled" => {
                self.zotero_get_unfiled_items_impl(Self::grouped_args(&args)?)
                    .await
            }
            action => {
                Err(Self::invalid_group_action("zotero_collections", action))
            }
        }
    }

    #[tool(
        name = "zotero_collections_write",
        description = "Grouped Zotero collection write router. action: \
                       create, manage, update, delete"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_collections_write(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "create" => {
                self.zotero_create_collection_impl(Self::grouped_args(&args)?)
                    .await
            }
            "manage" => {
                self.zotero_manage_collections_impl(Self::grouped_args(&args)?)
                    .await
            }
            "update" => {
                self.zotero_update_collection_impl(Self::grouped_args(&args)?)
                    .await
            }
            "delete" => {
                self.zotero_delete_collection_impl(Self::grouped_args(&args)?)
                    .await
            }
            action => Err(Self::invalid_group_action(
                "zotero_collections_write",
                action,
            )),
        }
    }

    #[tool(
        name = "zotero_items",
        description = "Grouped Zotero item read router. action: recent, get, \
                       metadata, children, fulltext"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_items(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "recent" => {
                self.zotero_get_recent_impl(Self::grouped_args(&args)?).await
            }
            "get" => {
                self.zotero_get_item_impl(Self::grouped_args(&args)?).await
            }
            "metadata" => {
                self.zotero_get_item_metadata_impl(Self::grouped_args(&args)?)
                    .await
            }
            "children" => {
                self.zotero_get_item_children_impl(Self::grouped_args(&args)?)
                    .await
            }
            "fulltext" => {
                self.zotero_get_item_fulltext_impl(Self::grouped_args(&args)?)
                    .await
            }
            action => Err(Self::invalid_group_action("zotero_items", action)),
        }
    }

    #[tool(
        name = "zotero_items_write",
        description = "Grouped Zotero item write router. action: update, \
                       delete, trash, restore, add_by_identifier, attach_file"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_items_write(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "update" => {
                self.zotero_update_item_impl(Self::grouped_args(&args)?).await
            }
            "delete" => {
                self.zotero_delete_item_impl(Self::grouped_args(&args)?).await
            }
            "trash" => {
                self.zotero_trash_item_impl(Self::grouped_args(&args)?).await
            }
            "restore" => {
                self.zotero_restore_item_impl(Self::grouped_args(&args)?).await
            }
            "add_by_identifier" => {
                self.zotero_add_by_identifier_impl(Self::grouped_args(&args)?)
                    .await
            }
            "attach_file" => {
                self.zotero_attach_file_impl(Self::grouped_args(&args)?).await
            }
            action => {
                Err(Self::invalid_group_action("zotero_items_write", action))
            }
        }
    }

    #[tool(
        name = "zotero_tags",
        description = "Grouped Zotero tag read router. action: list, search"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_tags(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "list" => {
                self.zotero_list_tags_impl(Self::grouped_args(&args)?).await
            }
            "search" => {
                self.zotero_search_by_tag_impl(Self::grouped_args(&args)?).await
            }
            action => Err(Self::invalid_group_action("zotero_tags", action)),
        }
    }

    #[tool(
        name = "zotero_tags_write",
        description = "Grouped Zotero tag write router. action: batch_update, \
                       rename, delete"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_tags_write(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "batch_update" => {
                self.zotero_batch_update_tags_impl(Self::grouped_args(&args)?)
                    .await
            }
            "rename" => {
                self.zotero_rename_tag_impl(Self::grouped_args(&args)?).await
            }
            "delete" => {
                self.zotero_delete_tags_impl(Self::grouped_args(&args)?).await
            }
            action => {
                Err(Self::invalid_group_action("zotero_tags_write", action))
            }
        }
    }

    #[tool(
        name = "zotero_relations",
        description = "Grouped Zotero relation read router. action: get"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_relations(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "get" => {
                self.zotero_get_related_items_impl(Self::grouped_args(&args)?)
                    .await
            }
            action => {
                Err(Self::invalid_group_action("zotero_relations", action))
            }
        }
    }

    #[tool(
        name = "zotero_relations_write",
        description = "Grouped Zotero relation write router. action: add, \
                       remove"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn zotero_relations_write(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "add" => {
                self.zotero_add_item_relation_impl(Self::grouped_args(&args)?)
                    .await
            }
            "remove" => {
                self.zotero_remove_item_relation_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            action => Err(Self::invalid_group_action(
                "zotero_relations_write",
                action,
            )),
        }
    }

    #[tool(
        name = "better_bibtex",
        description = "Grouped Better BibTeX router. action: citekeys, \
                       regenerate, export, bibliography, scan_aux, \
                       pandoc_filter, autoexport_add, search"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn better_bibtex(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "citekeys" => {
                self.better_bibtex_get_citekeys_impl(Self::grouped_args(&args)?)
                    .await
            }
            "regenerate" => {
                self.better_bibtex_regenerate_citekeys_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            "export" => {
                self.better_bibtex_export_items_impl(Self::grouped_args(&args)?)
                    .await
            }
            "bibliography" => {
                self.better_bibtex_format_bibliography_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            "scan_aux" => {
                self.better_bibtex_scan_aux_impl(Self::grouped_args(&args)?)
                    .await
            }
            "pandoc_filter" => {
                self.better_bibtex_pandoc_filter_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            "autoexport_add" => {
                self.better_bibtex_autoexport_add_impl(Self::grouped_args(
                    &args,
                )?)
                .await
            }
            "search" => {
                self.better_bibtex_search_impl(Self::grouped_args(&args)?).await
            }
            action => Err(Self::invalid_group_action("better_bibtex", action)),
        }
    }

    #[tool(
        name = "better_notes",
        description = "Grouped Better Notes router. action: export, \
                       from_markdown, run_template, relations, tree"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn better_notes(
        &self,
        Parameters(args): Parameters<GroupedToolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args.action.as_str() {
            "export" => {
                self.better_notes_export_impl(Self::grouped_args(&args)?).await
            }
            "from_markdown" => {
                self.better_notes_from_markdown_impl(Self::grouped_args(&args)?)
                    .await
            }
            "run_template" => {
                self.better_notes_run_template_impl(Self::grouped_args(&args)?)
                    .await
            }
            "relations" => {
                self.better_notes_get_relations_impl(Self::grouped_args(&args)?)
                    .await
            }
            "tree" => {
                self.better_notes_get_tree_impl(Self::grouped_args(&args)?)
                    .await
            }
            action => Err(Self::invalid_group_action("better_notes", action)),
        }
    }

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
        name = "zotero_get_related_items",
        description = "Get items related to an item via Zotero's dc:relation \
                       links"
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
                       (requires write permission)"
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
                       dc:relation) (requires write permission)"
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
        name = "zotero_fulltext_search",
        description = "Search Zotero's local sqlite database for full-text \
                       matches across titles, creators, and indexed PDF text \
                       (requires ZOTERO_SQLITE_ACCESS=1)"
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
                       annotation text (requires ZOTERO_SQLITE_ACCESS=1)"
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
            assert_eq!(info.server_info.version, "0.1.0");
            assert!(info.capabilities.tools.is_some());
            assert!(info.capabilities.resources.is_some());
            assert!(info.capabilities.prompts.is_some());
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
        fn filtered_mode_hides_disabled_write_and_sqlite_tools() {
            let mut state = AppState::from_env();
            state.tool_mode = ToolExposureMode::Filtered;
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

            assert!(names.contains(&"zotero_local_search".to_owned()));
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
