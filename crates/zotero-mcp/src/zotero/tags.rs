//! MCP tool handlers and argument models for Zotero tag administration.
//!
//! Exposes the `zotero_tags` and `zotero_tags_write` MCP tool routers. These
//! handlers enable reading, searching, batch updating, renaming, and deleting
//! tags across a Zotero library.
//!
//! # Main Types
//!
//! - [`ZoteroTagsCommand`] - Grouped-router command for read-only tag actions
//! - [`ZoteroTagsWriteCommand`] - Grouped-router command for write tag actions
//! - [`ListTagsArgs`] - Arguments for listing library tags
//! - [`SearchByTagArgs`] - Arguments for searching items by tag
//! - [`BatchUpdateTagsArgs`] - Arguments for batch tag additions and removals
//! - [`RenameTagArgs`] - Arguments for library-wide tag renaming
//! - [`DeleteTagsArgs`] - Arguments for deleting tags from a library
//!
//! # Examples
//!
//! ```ignore
//! # use rmcp::handler::server::wrapper::Parameters;
//! # use zotero_mcp::ZoteroMcpServer;
//! # use zotero_zotero::tags::{
//! #     ZoteroTagsCommand,
//! #     ListTagsArgs,
//! # };
//! # async fn run(
//! #     server: ZoteroMcpServer,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let args = Parameters(ZoteroTagsCommand::List(ListTagsArgs {
//!     limit: Some(50),
//! }));
//! let result = server.zotero_tags(args).await?;
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::{ItemKey, TagName, ZoteroClient};

use crate::{
    ZoteroMcpServer,
    response::{json_result, text_error, text_success},
};

/// Arguments for the `list` action of `zotero_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ListTagsArgs {
    /// Maximum number of tags to return (default: 100).
    limit: Option<usize>,
}

/// Arguments for the `search` action of `zotero_tags` and the `tag` action
/// of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByTagArgs {
    /// Tag name ([`TagName`]) to search for.
    tag: String,
    /// Maximum number of items to return (default: 20).
    limit: Option<usize>,
}

/// Arguments for the `batch_update` action of `zotero_tags_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BatchUpdateTagsArgs {
    /// List of item keys ([`ItemKey`]).
    item_keys: Vec<String>,
    /// Tags ([`TagName`]) to add.
    add_tags: Option<Vec<String>>,
    /// Tags ([`TagName`]) to remove.
    remove_tags: Option<Vec<String>>,
}

/// Arguments for the `rename` action of `zotero_tags_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RenameTagArgs {
    /// Existing tag name ([`TagName`]).
    old_tag: String,
    /// New tag name ([`TagName`]).
    new_tag: String,
}

/// Arguments for the `delete` action of `zotero_tags_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteTagsArgs {
    /// Tag names ([`TagName`]) to delete from the library (up to 50).
    tags: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Read commands dispatched by the `zotero_tags` MCP tool router.
pub(crate) enum ZoteroTagsCommand {
    /// List all tags in a library.
    List(ListTagsArgs),
    /// Find items with a specific tag.
    Search(SearchByTagArgs),
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Write commands dispatched by the `zotero_tags` MCP tool router.
pub(crate) enum ZoteroTagsWriteCommand {
    /// Add or remove tags on items in bulk.
    BatchUpdate(BatchUpdateTagsArgs),
    /// Rename a tag across the entire library.
    Rename(RenameTagArgs),
    /// Delete tags from the library.
    Delete(DeleteTagsArgs),
}

#[tool_router(router = tags_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_tags",
        description = "Grouped Zotero tag read router. action: list, search",
        annotations(
            title = "Read Zotero Tags",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// Dispatches read-only tag tool calls.
    ///
    /// Accepts a [`Parameters<ZoteroTagsCommand>`] containing the specific
    /// action and parameters, routing it to internal tag read handlers.
    ///
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
    /// Dispatches tag modification and deletion tool calls.
    ///
    /// Accepts a [`Parameters<ZoteroTagsWriteCommand>`] containing the specific
    /// action and parameters, routing it to internal tag write handlers.
    ///
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
}

impl ZoteroMcpServer {
    /// Handles Zotero tag listing tool calls.
    ///
    /// Queries the Zotero API using [`ListTagsArgs`] parameters and returns all
    /// tags in the library as MCP JSON content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::list_tags`] are returned as MCP error
    /// content.
    async fn zotero_list_tags_impl(
        &self,
        args: ListTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(100);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.list_tags(limit).await))
    }

    /// Handles Zotero tag search tool calls.
    ///
    /// Queries the Zotero API using [`SearchByTagArgs`] parameters and returns
    /// matching items for the specified tag as MCP JSON content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::search_by_tag`] are returned as MCP error
    /// content.
    pub(crate) async fn zotero_search_by_tag_impl(
        &self,
        args: SearchByTagArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        let tag: TagName = args.tag.into();
        Ok(json_result(client.search_by_tag(&tag, limit).await))
    }

    /// Handles Zotero batch tag update tool calls.
    ///
    /// Updates tags across multiple items using [`BatchUpdateTagsArgs`]
    /// parameters.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::batch_update_tags`] are returned as MCP
    /// error content.
    async fn zotero_batch_update_tags_impl(
        &self,
        args: BatchUpdateTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let item_keys: Vec<ItemKey> =
            args.item_keys.into_iter().map(Into::into).collect();
        let add: Vec<TagName> = args
            .add_tags
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        let rem: Vec<TagName> = args
            .remove_tags
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        match client.batch_update_tags(&item_keys, &add, &rem).await {
            Ok(count) => {
                Ok(text_success(format!("Batch updated tags on {count} items")))
            }
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero tag rename tool calls.
    ///
    /// Renames a tag library-wide using [`RenameTagArgs`] parameters.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::rename_tag`] are returned as MCP error
    /// content.
    async fn zotero_rename_tag_impl(
        &self,
        args: RenameTagArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let old_tag: TagName = args.old_tag.into();
        let new_tag: TagName = args.new_tag.into();
        match client.rename_tag(&old_tag, &new_tag).await {
            Ok(count) => {
                Ok(text_success(format!("Renamed tag on {count} items")))
            }
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero tag deletion tool calls.
    ///
    /// Deletes specified tags from the library using [`DeleteTagsArgs`]
    /// parameters.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures from [`ZoteroClient::delete_tags`] are returned as MCP error
    /// content.
    async fn zotero_delete_tags_impl(
        &self,
        args: DeleteTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let tags: Vec<TagName> =
            args.tags.into_iter().map(Into::into).collect();
        match client.delete_tags(&tags).await {
            Ok(()) => Ok(text_success("Tags deleted")),
            Err(e) => Ok(text_error(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ZoteroMcpServer, zotero::fixtures::*};

    mod read_operations {

        use super::*;

        #[tokio::test]
        async fn list_tags_returns_tags() {
            // Arrange
            let tags = json!([{"tag": "quantum", "meta": {"numItems": 3}}]);
            let base =
                mock_server(vec![http_response("200 OK", &tags.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_list_tags_impl(ListTagsArgs {
                    limit: Some(50),
                })
                .await
                .expect("list tags ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }

    mod write_operations {

        use super::*;

        #[tokio::test]
        async fn rename_tag_patches_item_tags() {
            // Arrange
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

            // Act
            let res = server
                .zotero_rename_tag_impl(RenameTagArgs {
                    old_tag: "old_tag".into(),
                    new_tag: "new_tag".into(),
                })
                .await
                .expect("rename tag ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
        #[tokio::test]
        async fn delete_tags_removes_tags() {
            // Arrange
            let base = mock_server(vec![
                http_response_with_headers(
                    "200 OK",
                    &[("Last-Modified-Version", "9")],
                    "[]",
                ),
                http_response("204 No Content", ""),
            ]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .zotero_delete_tags_impl(DeleteTagsArgs {
                    tags: vec!["old_tag".into()],
                })
                .await
                .expect("delete tags ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }
    }
}
