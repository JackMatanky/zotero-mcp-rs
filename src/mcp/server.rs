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
    model::{
        CallToolResponse, GetPromptResponse, Implementation, InitializeResult,
        ProtocolVersion, ReadResourceResponse, ServerCapabilities,
    },
};

use crate::{
    mcp::catalog::tool_visibility,
    state::{AppState, ToolExposureMode},
};

const SERVER_INSTRUCTIONS: &str =
    "Call zotero_discover first to find Zotero tools, resources, prompts, env \
     gates, and examples. Use zotero://... resources for read-only object \
     retrieval, including zotero://items/{item_key}. search and fetch are \
     connector compatibility tools. Write tools require \
     ZOTERO_WRITE_ENABLED=1. SQLite tools require ZOTERO_SQLITE_ACCESS=1. \
     ZOTERO_MCP_MODE=all exposes legacy individual tools.";

/// Holds shared [`AppState`] and implements [`ServerHandler`].
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

    fn tool_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        let mut router = Self::catalog_router();
        router.merge(Self::status_router());
        router.merge(Self::search_router());
        router.merge(Self::duplicates_router());
        router.merge(Self::coverage_router());
        router.merge(Self::sqlite_router());
        router.merge(Self::pdf_router());
        router.merge(Self::notes_router());
        router.merge(Self::annotations_router());
        router.merge(Self::collections_router());
        router.merge(Self::items_router());
        router.merge(Self::metadata_router());
        router.merge(Self::fulltext_router());
        router.merge(Self::attachments_router());
        router.merge(Self::tags_router());
        router.merge(Self::relations_router());
        router.merge(Self::better_bibtex_router());
        router.merge(Self::better_notes_router());
        router.merge(Self::connector_router());
        router
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

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        mcp::catalog::{DiscoverArgs, ToolVisibility, tool_visibility},
        state::{AppState, ToolExposureMode},
    };
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
