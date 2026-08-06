//! MCP tool handlers and argument models for Zotero item relations.
//!
//! Handles `zotero_relations` (read-only) and `zotero_relations_write`
//! (mutation) grouped-router tool calls. Converts incoming MCP tool parameters
//! into calls on [`ZoteroClient`] for inspecting `dc:relation` links, adding
//! bidirectional item relations, and removing existing relations.
//!
//! # Main Types
//!
//! - [`ZoteroRelationsCommand`]: Grouped-router command for read-only relation
//!   actions.
//! - [`ZoteroRelationsWriteCommand`]: Grouped-router command for write relation
//!   actions.
//! - [`GetRelatedItemsArgs`]: Arguments for fetching items related to a Zotero
//!   item key.
//! - [`AddItemRelationArgs`]: Arguments for linking two Zotero items
//!   bidirectionally.
//! - [`RemoveItemRelationArgs`]: Arguments for unlinking two Zotero items.
//!
//! # Examples
//!
//! ```ignore
//! # use rmcp::handler::server::wrapper::Parameters;
//! # use zotero_api::AppState;
//! # use zotero_mcp::ZoteroMcpServer;
//! # use zotero_zotero::relations::{
//! #     ZoteroRelationsCommand,
//! #     GetRelatedItemsArgs,
//! # };
//! # async fn run() -> Result<
//! #     (),
//! #     Box<dyn std::error::Error>,
//! # > {
//! let server = ZoteroMcpServer::new(AppState::from_env());
//! let args = ZoteroRelationsCommand::Get(GetRelatedItemsArgs {
//!     item_key: "ITEM0001".into(),
//! });
//! let result = server.zotero_relations(Parameters(args)).await?;
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::{ItemKey, ZoteroClient};

use crate::{
    ZoteroMcpServer,
    response::{json_result, text_error, text_success},
};

/// Arguments for the `get` action of `zotero_relations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetRelatedItemsArgs {
    /// Zotero item key ([`ItemKey`]) whose related items to list.
    item_key: String,
}

/// Arguments for the `add` action of `zotero_relations_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddItemRelationArgs {
    /// Zotero item key ([`ItemKey`]) of the first item to link (bidirectional,
    /// order-independent).
    item_key: String,
    /// Zotero item key ([`ItemKey`]) of the second item to link
    /// (bidirectional, order-independent).
    related_item_key: String,
}

/// Arguments for the `remove` action of `zotero_relations_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RemoveItemRelationArgs {
    /// Zotero item key ([`ItemKey`]) of the first item to unlink
    /// (bidirectional, order-independent).
    item_key: String,
    /// Zotero item key ([`ItemKey`]) of the second item to unlink
    /// (bidirectional, order-independent).
    related_item_key: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Read commands dispatched by the `zotero_relations` MCP tool router.
pub(crate) enum ZoteroRelationsCommand {
    /// Get items related to a given item key.
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
    /// Dispatches read-only relation tool commands to internal handlers.
    ///
    /// Receives parsed `args` wrapped in [`Parameters`], routing the `get`
    /// action to retrieve items linked to a given Zotero item key.
    ///
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
    /// Dispatches relation mutation tool commands to internal handlers.
    ///
    /// Receives parsed `args` wrapped in [`Parameters`], routing `add` or
    /// `remove` actions to mutate bidirectional Zotero item relations.
    ///
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
    /// Handles Zotero related-item listing tool calls.
    ///
    /// Fetches items linked to `args.item_key` via
    /// [`ZoteroClient::get_related_items`] and formats the response as JSON
    /// tool output. # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_get_related_items_impl(
        &self,
        args: GetRelatedItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client.get_related_items(&ItemKey::from(args.item_key)).await,
        ))
    }

    /// Handles Zotero related-item linking tool calls.
    ///
    /// Creates a bidirectional link between `args.item_key` and
    /// `args.related_item_key` via [`ZoteroClient::add_item_relation`].
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
            .add_item_relation(
                &ItemKey::from(args.item_key),
                &ItemKey::from(args.related_item_key),
            )
            .await
        {
            Ok(()) => Ok(text_success("Item relation added")),
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero related-item unlinking tool calls.
    ///
    /// Removes the bidirectional relation between `args.item_key` and
    /// `args.related_item_key` via [`ZoteroClient::remove_item_relation`].
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
            .remove_item_relation(
                &ItemKey::from(args.item_key),
                &ItemKey::from(args.related_item_key),
            )
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
    use zotero_api::AppState;

    use super::*;
    use crate::{ZoteroMcpServer, zotero::fixtures::*};

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
            let server = ZoteroMcpServer::new(
                AppState::test_default().with_write_enabled(false),
            );

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
