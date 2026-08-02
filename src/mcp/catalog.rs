//! MCP primitive catalog and discovery tool routing.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ZoteroMcpServer, state::AppState};

#[derive(Deserialize, JsonSchema)]
pub(crate) struct DiscoverArgs {
    pub(crate) query: Option<String>,
    pub(crate) domain: Option<PrimitiveDomain>,
    pub(crate) include_disabled: Option<bool>,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PrimitiveKind {
    Tool,
    Resource,
    Prompt,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrimitiveDomain {
    Discovery,
    Items,
    Collections,
    Search,
    Notes,
    Sqlite,
    Prompts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum EnvGate {
    #[serde(rename = "ZOTERO_WRITE_ENABLED")]
    WriteEnabled,
    #[serde(rename = "ZOTERO_SQLITE_ACCESS")]
    SqliteAccess,
}

#[derive(Clone, Copy, Serialize)]
struct PrimitiveInfo {
    name: &'static str,
    kind: PrimitiveKind,
    domain: PrimitiveDomain,
    requires: &'static [EnvGate],
    summary: &'static str,
    example: Option<&'static str>,
    #[serde(skip_serializing)]
    search_text: &'static str,
}

static PRIMITIVES: &[PrimitiveInfo] = &[
    PrimitiveInfo {
        name: "zotero_discover",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Discovery,
        requires: &[],
        summary: "Find Zotero tools, resources, prompts, env gates, and \
                  examples",
        example: Some(r#"{"query":"notes"}"#),
        search_text: "zotero_discover discovery find zotero tools resources \
                      prompts env gates and examples",
    },
    PrimitiveInfo {
        name: "zotero://items/{item_key}",
        kind: PrimitiveKind::Resource,
        domain: PrimitiveDomain::Items,
        requires: &[],
        summary: "Read one Zotero item by key",
        example: Some("zotero://items/ITEMKEY"),
        search_text: "zotero://items/{item_key} items read one zotero item by \
                      key",
    },
    PrimitiveInfo {
        name: "zotero://collections/{collection_key}/items",
        kind: PrimitiveKind::Resource,
        domain: PrimitiveDomain::Collections,
        requires: &[],
        summary: "Read collection items",
        example: Some("zotero://collections/COLKEY/items"),
        search_text: "zotero://collections/{collection_key}/items collections \
                      read collection items",
    },
    PrimitiveInfo {
        name: "zotero_search",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Search,
        requires: &[],
        summary: "Grouped search actions: items, tag, citation_key, advanced, \
                  duplicates, coverage",
        example: Some(r#"{"action":"items","query":"rust","limit":10}"#),
        search_text: "zotero_search search grouped search actions items tag \
                      citation_key advanced duplicates coverage",
    },
    PrimitiveInfo {
        name: "zotero_items",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Items,
        requires: &[],
        summary: "Grouped item read actions: recent, get, metadata, children, \
                  fulltext",
        example: Some(r#"{"action":"get","item_key":"ITEMKEY"}"#),
        search_text: "zotero_items items grouped item read actions recent get \
                      metadata children fulltext",
    },
    PrimitiveInfo {
        name: "zotero_notes",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Notes,
        requires: &[],
        summary: "Grouped note read actions: list, synthesize",
        example: Some(r#"{"action":"list","item_key":"ITEMKEY"}"#),
        search_text: "zotero_notes notes grouped note read actions list \
                      synthesize",
    },
    PrimitiveInfo {
        name: "zotero_items_write",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Items,
        requires: &[EnvGate::WriteEnabled],
        summary: "Grouped item write actions: update, delete, trash, restore, \
                  add_by_identifier, attach_file",
        example: Some(r#"{"action":"trash","item_key":"ITEMKEY"}"#),
        search_text: "zotero_items_write items grouped item write actions \
                      update delete trash restore add_by_identifier \
                      attach_file zotero_write_enabled",
    },
    PrimitiveInfo {
        name: "zotero_notes_write",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Notes,
        requires: &[EnvGate::WriteEnabled],
        summary: "Grouped note write actions: create, annotation",
        example: Some(
            r##"{"action":"create","parent_key":"ITEMKEY","markdown":"# Note"}"##,
        ),
        search_text: "zotero_notes_write notes grouped note write actions \
                      create annotation zotero_write_enabled",
    },
    PrimitiveInfo {
        name: "zotero_sqlite_search",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Sqlite,
        requires: &[EnvGate::SqliteAccess],
        summary: "Grouped local SQLite search actions: fulltext, \
                  notes_annotations",
        example: Some(r#"{"action":"fulltext","query":"borrow checker"}"#),
        search_text: "zotero_sqlite_search sqlite grouped local sqlite search \
                      actions fulltext notes_annotations zotero_sqlite_access",
    },
    PrimitiveInfo {
        name: "zotero_literature_review",
        kind: PrimitiveKind::Prompt,
        domain: PrimitiveDomain::Prompts,
        requires: &[],
        summary: "Generate a literature review prompt for a collection",
        example: Some(r#"{"collection_key":"COLKEY"}"#),
        search_text: "zotero_literature_review prompts generate a literature \
                      review prompt for a collection",
    },
];

/// Returns true if `name` is a write (mutating) tool, gated behind
/// `ZOTERO_WRITE_ENABLED`.
pub(crate) fn is_write_tool(name: &str) -> bool {
    matches!(
        name,
        "zotero_notes_write"
            | "zotero_collections_write"
            | "zotero_items_write"
            | "zotero_tags_write"
            | "zotero_relations_write"
    )
}

/// Returns true if `name` is currently advertised to MCP clients given
/// `state`'s write/`SQLite` gates.
pub(crate) fn is_tool_visible(state: &AppState, name: &str) -> bool {
    if is_write_tool(name) {
        return state.write_enabled;
    }
    if name == "zotero_sqlite_search" {
        return state.sqlite_access;
    }
    matches!(
        name,
        "zotero_discover"
            | "zotero_status"
            | "zotero_search"
            | "zotero_pdf"
            | "zotero_notes"
            | "zotero_collections"
            | "zotero_items"
            | "zotero_tags"
            | "zotero_relations"
            | "better_bibtex"
            | "better_notes"
            | "search"
            | "fetch"
    )
}

impl ZoteroMcpServer {
    fn discover_primitives(&self, args: &DiscoverArgs) -> Vec<PrimitiveInfo> {
        let query = args.query.as_ref().map(|value| value.to_lowercase());
        PRIMITIVES
            .iter()
            .copied()
            .filter(|primitive| {
                args.include_disabled == Some(true)
                    || self.is_primitive_enabled(*primitive)
            })
            .filter(|primitive| {
                args.domain.is_none_or(|domain| primitive.domain == domain)
            })
            .filter(|primitive| {
                query
                    .as_deref()
                    .is_none_or(|query| primitive.search_text.contains(query))
            })
            .collect()
    }

    fn is_primitive_enabled(&self, primitive: PrimitiveInfo) -> bool {
        !primitive.requires.iter().any(|requirement| {
            (*requirement == EnvGate::WriteEnabled && !self.state.write_enabled)
                || (*requirement == EnvGate::SqliteAccess
                    && !self.state.sqlite_access)
        })
    }

    pub(crate) fn zotero_discover_impl(
        &self,
        args: &DiscoverArgs,
    ) -> CallToolResult {
        #[derive(Serialize)]
        struct DiscoveryResponse {
            capabilities: Vec<PrimitiveInfo>,
        }

        crate::mcp::json_success(&DiscoveryResponse {
            capabilities: self.discover_primitives(args),
        })
    }
}

#[tool_router(router = catalog_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
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
}
