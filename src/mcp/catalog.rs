//! MCP primitive catalog and discovery tool routing.
//!
//! Provides capability discovery and metadata for MCP primitives (tools,
//! resources, and prompts) registered with the server, including environment
//! variable gates and example invocations.
//!
//! # Main Types
//!
//! - [`PrimitiveKind`]: Kind of MCP primitive (tool, resource, or prompt).
//! - [`PrimitiveDomain`]: Functional domain grouping for MCP primitives.
//! - [`EnvGate`]: Environment variable gate controlling primitive visibility.
//! - [`PrimitiveInfo`]: Metadata for a single discoverable MCP primitive.
//! - [`DiscoverArgs`]: Arguments for the `zotero_discover` discovery tool.
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::state::AppState;
//! # use zotero_mcp_rs::ZoteroMcpServer;
//! # async fn example() -> Result<(), Box<
//! #     dyn std::error::Error,
//! # >> {
//! let state = AppState::from_env();
//! let server = ZoteroMcpServer::new(state);
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ZoteroMcpServer, state::AppState};

/// Arguments for the `zotero_discover` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DiscoverArgs {
    /// Optional search term matched against primitive names, summaries, and
    /// tags.
    pub(crate) query: Option<String>,
    /// Optional domain filter to restrict results to one functional domain.
    pub(crate) domain: Option<PrimitiveDomain>,
    /// Whether to include disabled primitives in the response (defaults to
    /// `false`).
    pub(crate) include_disabled: Option<bool>,
}

/// Kind of MCP primitive (tool, resource, or prompt).
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PrimitiveKind {
    /// A callable MCP tool.
    Tool,
    /// A readable MCP resource.
    Resource,
    /// An MCP prompt template.
    Prompt,
}

/// Functional domain grouping for MCP primitives.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrimitiveDomain {
    /// Discovery and introspection tools.
    Discovery,
    /// Item read/write operations.
    Items,
    /// Collection operations.
    Collections,
    /// Search operations.
    Search,
    /// Note operations.
    Notes,
    /// Direct `SQLite` database queries.
    Sqlite,
    /// Semantic search operations.
    Semantic,
    /// Prompt templates.
    Prompts,
}

/// Environment variable that gates access to a group of tools.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum EnvGate {
    /// Requires `ZOTERO_WRITE_ENABLED=1` for write tools.
    #[serde(rename = "ZOTERO_WRITE_ENABLED")]
    WriteEnabled,
    /// Requires `ZOTERO_SQLITE_ACCESS=1` for `SQLite` tools.
    #[serde(rename = "ZOTERO_SQLITE_ACCESS")]
    SqliteAccess,
    /// Requires `ZOTERO_SEMANTIC_SEARCH=1` for semantic tools.
    #[serde(rename = "ZOTERO_SEMANTIC_SEARCH")]
    SemanticSearchEnabled,
    /// Requires `ZOTERO_CONNECTOR_COMPAT=1` for single-purpose connector tools.
    #[serde(rename = "ZOTERO_CONNECTOR_COMPAT")]
    ConnectorCompat,
}

/// Metadata for a single discoverable MCP primitive.
#[derive(Clone, Copy, Serialize)]
struct PrimitiveInfo {
    name: &'static str,
    kind: PrimitiveKind,
    domain: PrimitiveDomain,
    /// Environment variables that must be set for this primitive to be
    /// visible.
    requires: &'static [EnvGate],
    summary: &'static str,
    /// Example invocation shown in discovery output.
    example: Option<&'static str>,
    /// Internal search text used for filtering discovery results, omitted from
    /// JSON output.
    #[serde(skip)]
    search_text: &'static str,
}

static PRIMITIVES: &[PrimitiveInfo] = &[
    PrimitiveInfo {
        name: "search",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Search,
        requires: &[EnvGate::ConnectorCompat],
        summary: "Connector compatibility: search Zotero items by query",
        example: Some(r#"{"query":"rust"}"#),
        search_text: "search connector compatibility zotero items query",
    },
    PrimitiveInfo {
        name: "fetch",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Items,
        requires: &[EnvGate::ConnectorCompat],
        summary: "Connector compatibility: get Zotero item metadata by key",
        example: Some(r#"{"id":"ITEMKEY"}"#),
        search_text: "fetch connector compatibility zotero item metadata key",
    },
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
        name: "zotero_status",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Discovery,
        requires: &[],
        summary: "Show server status and configuration",
        example: None,
        search_text: "zotero_status status server configuration",
    },
    PrimitiveInfo {
        name: "zotero_collections",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Collections,
        requires: &[],
        summary: "Grouped collection actions: list, items",
        example: Some(r#"{"action":"list"}"#),
        search_text: "zotero_collections collections grouped actions list \
                      items",
    },
    PrimitiveInfo {
        name: "zotero_tags",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Items,
        requires: &[],
        summary: "Grouped tag actions: list, add, remove",
        example: Some(r#"{"action":"list","item_key":"ITEMKEY"}"#),
        search_text: "zotero_tags tags grouped actions list add remove",
    },
    PrimitiveInfo {
        name: "zotero_relations",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Items,
        requires: &[],
        summary: "Grouped relation actions: list, add, remove",
        example: Some(r#"{"action":"list","item_key":"ITEMKEY"}"#),
        search_text: "zotero_relations relations grouped actions list add \
                      remove",
    },
    PrimitiveInfo {
        name: "zotero_pdf",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Items,
        requires: &[],
        summary: "Grouped PDF actions: attachment, fulltext, annotation",
        example: Some(r#"{"action":"attachment","item_key":"ITEMKEY"}"#),
        search_text: "zotero_pdf pdf grouped actions attachment fulltext \
                      annotation",
    },
    PrimitiveInfo {
        name: "better_bibtex",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Items,
        requires: &[],
        summary: "Better BibTeX export and citation key operations",
        example: Some(r#"{"action":"export","item_key":"ITEMKEY"}"#),
        search_text: "better_bibtex bibtex export citation key operations",
    },
    PrimitiveInfo {
        name: "better_notes",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Notes,
        requires: &[],
        summary: "Better Notes template and synchronization operations",
        example: Some(r#"{"action":"list_templates"}"#),
        search_text: "better_notes notes template synchronization operations",
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
                  add_by_identifier, attach_file, import_pdf",
        example: Some(r#"{"action":"trash","item_key":"ITEMKEY"}"#),
        search_text: "zotero_items_write items grouped item write actions \
                      update delete trash restore add_by_identifier \
                      attach_file import_pdf zotero_write_enabled",
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
        name: "zotero_semantic_search",
        kind: PrimitiveKind::Tool,
        domain: PrimitiveDomain::Semantic,
        requires: &[EnvGate::SemanticSearchEnabled],
        summary: "Grouped semantic search actions: search, index, status",
        example: Some(r#"{"action":"search","query":"attention mechanisms"}"#),
        search_text: "zotero_semantic_search semantic vector embedding \
                      grouped actions search index status \
                      zotero_semantic_search_enabled",
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

/// Returns the env gates for a tool by name, or `None` if not found in the
/// catalog.
fn tool_gates(name: &str) -> Option<&'static [EnvGate]> {
    PRIMITIVES
        .iter()
        .find(|p| p.kind == PrimitiveKind::Tool && p.name == name)
        .map(|p| p.requires)
}

/// Returns `true` if `name` is a write (mutating) tool gated behind
/// `ZOTERO_WRITE_ENABLED`.
#[allow(dead_code, reason = "used in tests; kept as public API for callers")]
pub(crate) fn is_write_tool(name: &str) -> bool {
    tool_gates(name).is_some_and(|gates| gates.contains(&EnvGate::WriteEnabled))
}

/// Returns `true` if `name` is currently advertised to MCP clients given
/// `state`'s feature gates.
///
/// Evaluates environment gates specified for `name` in the primitive catalog
/// against the provided [`AppState`].
pub(crate) fn is_tool_visible(state: &AppState, name: &str) -> bool {
    let Some(gates) = tool_gates(name) else {
        return false;
    };
    gates.iter().all(|gate| match gate {
        EnvGate::WriteEnabled => state.write_enabled,
        EnvGate::SqliteAccess => state.sqlite_access,
        EnvGate::SemanticSearchEnabled => state.semantic_search_enabled,
        EnvGate::ConnectorCompat => state.connector_compat,
    })
}

impl ZoteroMcpServer {
    /// Returns primitives matching the query, domain, and enabled state.
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

    /// Returns `true` if all env gates for `primitive` are satisfied.
    fn is_primitive_enabled(&self, primitive: PrimitiveInfo) -> bool {
        primitive.requires.iter().all(|gate| match gate {
            EnvGate::WriteEnabled => self.state.write_enabled,
            EnvGate::SqliteAccess => self.state.sqlite_access,
            EnvGate::SemanticSearchEnabled => {
                self.state.semantic_search_enabled
            }
            EnvGate::ConnectorCompat => self.state.connector_compat,
        })
    }

    /// Builds a JSON [`CallToolResult`] listing matching capabilities.
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
    /// Discovers Zotero capabilities matching the given query and filters.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if response serialization fails or
    /// protocol-level errors occur.
    pub(crate) async fn zotero_discover(
        &self,
        Parameters(args): Parameters<DiscoverArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.zotero_discover_impl(&args))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    mod discovery {
        use super::*;

        #[test]
        fn omits_search_text_from_serialized_payload() {
            let server = ZoteroMcpServer::new(AppState::from_env());
            let res = server.zotero_discover_impl(&DiscoverArgs {
                query: Some("items".to_owned()),
                domain: None,
                include_disabled: None,
            });

            let json = serde_json::to_string(&res)
                .expect("serialize discovery response");
            assert!(!json.contains("search_text"));
        }
    }

    mod env_gate {
        use pretty_assertions::assert_eq;

        use super::*;
        #[test]
        fn serializes_connector_compat_variant_as_env_var_name() {
            let json = serde_json::to_string(&EnvGate::ConnectorCompat)
                .expect("serialize EnvGate");
            assert_eq!(json, "\"ZOTERO_CONNECTOR_COMPAT\"");
        }
    }
}
