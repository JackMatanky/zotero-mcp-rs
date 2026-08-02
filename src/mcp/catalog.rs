//! MCP capability catalog and discovery tool routing.

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
pub(crate) enum ToolVisibility {
    CompactUngated,
    CompactSqlite,
    CompactWrite,
    LegacyUngated,
    LegacySqlite,
    LegacyWrite,
}

impl ToolVisibility {
    pub(crate) fn is_compact_visible(self, state: &AppState) -> bool {
        match self {
            Self::CompactUngated => true,
            Self::CompactSqlite => state.sqlite_access,
            Self::CompactWrite => state.write_enabled,
            Self::LegacyUngated | Self::LegacySqlite | Self::LegacyWrite => {
                false
            }
        }
    }

    pub(crate) fn is_gated_visible(self, state: &AppState) -> bool {
        match self {
            Self::CompactSqlite | Self::LegacySqlite => state.sqlite_access,
            Self::CompactWrite | Self::LegacyWrite => state.write_enabled,
            Self::CompactUngated | Self::LegacyUngated => true,
        }
    }
}

pub(crate) fn tool_visibility(name: &str) -> ToolVisibility {
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

impl ZoteroMcpServer {
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
