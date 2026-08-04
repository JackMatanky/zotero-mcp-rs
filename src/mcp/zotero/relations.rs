//! MCP tool handlers and argument models for Zotero item relations.
//!
//! Covers `zotero_relations` / `zotero_relations_write` grouped-router
//! actions: listing an item's `dc:relation` links, and bidirectionally
//! linking or unlinking two items.
//!
//! Main types:
//! - [`ZoteroRelationsCommand`] - Grouped-router command for read-only relation
//!   actions
//! - [`ZoteroRelationsWriteCommand`] - Grouped-router command for write
//!   relation actions

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, text_error, text_success},
    zotero::{ItemKey, ZoteroClient},
};

/// Arguments for the `get` action of `zotero_relations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRelatedItemsArgs {
    /// Zotero item key ([`ItemKey`]) whose related items to list.
    item_key: ItemKey,
}

/// Arguments for the `add` action of `zotero_relations_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddItemRelationArgs {
    /// Zotero item key ([`ItemKey`]) of the first item to link (bidirectional,
    /// order-independent).
    item_key: ItemKey,
    /// Zotero item key ([`ItemKey`]) of the second item to link
    /// (bidirectional, order-independent).
    related_item_key: ItemKey,
}

/// Arguments for the `remove` action of `zotero_relations_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RemoveItemRelationArgs {
    /// Zotero item key ([`ItemKey`]) of the first item to unlink
    /// (bidirectional, order-independent).
    item_key: ItemKey,
    /// Zotero item key ([`ItemKey`]) of the second item to unlink
    /// (bidirectional, order-independent).
    related_item_key: ItemKey,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Read commands dispatched by the `zotero_relations` MCP tool router.
pub(crate) enum ZoteroRelationsCommand {
    /// Get items related to a given item.
    Get(GetRelatedItemsArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Write commands dispatched by the `zotero_relations` MCP tool router.
pub(crate) enum ZoteroRelationsWriteCommand {
    /// Create a bidirectional relation between two items.
    Add(AddItemRelationArgs),
    /// Remove a relation between two items.
    Remove(RemoveItemRelationArgs),
}

#[tool_router(router = relations_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
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
}

impl ZoteroMcpServer {
    /// Handles Zotero related-item listing tool calls, returning the items
    /// linked to `item_key` as `RelatedItem` JSON.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_related_items_impl(
        &self,
        args: GetRelatedItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.get_related_items(&args.item_key).await))
    }

    /// Handles Zotero related-item linking tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_add_item_relation_impl(
        &self,
        args: AddItemRelationArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client
            .add_item_relation(&args.item_key, &args.related_item_key)
            .await
        {
            Ok(()) => Ok(text_success("Item relation added")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero related-item unlinking tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_remove_item_relation_impl(
        &self,
        args: RemoveItemRelationArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        match client
            .remove_item_relation(&args.item_key, &args.related_item_key)
            .await
        {
            Ok(()) => Ok(text_success("Item relation removed")),
            Err(e) => Ok(text_error(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ZoteroMcpServer, mcp::zotero::fixtures::*, state::AppState};

    mod related_items {
        use pretty_assertions::assert_eq;

        use super::*;

        fn item_json(key: &str, relations: &serde_json::Value) -> String {
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": "journalArticle",
                    "relations": relations.clone(),
                },
            })
            .to_string()
        }

        fn related_item_json(key: &str, title: &str) -> String {
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": "journalArticle",
                    "title": title,
                },
            })
            .to_string()
        }

        const URI_A_TO_B: &str = "http://zotero.org/users/0/items/ITEM0002";
        const URI_B_TO_A: &str = "http://zotero.org/users/0/items/ITEM0001";

        #[tokio::test]
        async fn get_related_items_returns_related_items() {
            // Arrange
            let source = item_json(
                "ITEM0001",
                &serde_json::json!({
                    "dc:relation": [URI_A_TO_B],
                }),
            );
            let base = mock_server(vec![
                http_response("200 OK", &source),
                http_response(
                    "200 OK",
                    &related_item_json("ITEM0002", "Related Article"),
                ),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_get_related_items_impl(GetRelatedItemsArgs {
                    item_key: "ITEM0001".into(),
                })
                .await
                .expect("get related items ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = tool_text(&res);
            assert!(text.contains("ITEM0002"));
            assert!(text.contains("Related Article"));
        }

        #[tokio::test]
        async fn add_item_relation_links_items_and_returns_success() {
            // Arrange
            let base = mock_server(vec![
                http_response("200 OK", &item_json("ITEM0001", &json!({}))),
                http_response("200 OK", &item_json("ITEM0002", &json!({}))),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0001",
                        &serde_json::json!({
                            "dc:relation": [URI_A_TO_B],
                        }),
                    ),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0002",
                        &serde_json::json!({
                            "dc:relation": [URI_B_TO_A],
                        }),
                    ),
                ),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_add_item_relation_impl(AddItemRelationArgs {
                    item_key: "ITEM0001".into(),
                    related_item_key: "ITEM0002".into(),
                })
                .await
                .expect("add item relation ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("Item relation added"));
        }

        #[tokio::test]
        async fn add_item_relation_returns_error_when_write_disabled() {
            // Arrange
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: String::new(),
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                write_enabled: false,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_add_item_relation_impl(AddItemRelationArgs {
                    item_key: "ITEM0001".into(),
                    related_item_key: "ITEM0002".into(),
                })
                .await
                .expect("write disabled result");

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert!(tool_text(&res).contains("Permission denied"));
        }

        #[tokio::test]
        async fn remove_item_relation_unlinks_items_and_returns_success() {
            // Arrange
            let base = mock_server(vec![
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0001",
                        &serde_json::json!({
                            "dc:relation": [URI_A_TO_B],
                        }),
                    ),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0002",
                        &serde_json::json!({
                            "dc:relation": [URI_B_TO_A],
                        }),
                    ),
                ),
                http_response("200 OK", &item_json("ITEM0001", &json!({}))),
                http_response("200 OK", &item_json("ITEM0002", &json!({}))),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_remove_item_relation_impl(RemoveItemRelationArgs {
                    item_key: "ITEM0001".into(),
                    related_item_key: "ITEM0002".into(),
                })
                .await
                .expect("remove item relation ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            assert!(tool_text(&res).contains("Item relation removed"));
        }
    }
}
