//! MCP tool handlers and argument models for Zotero tag administration.
//!
//! Covers `zotero_tags` / `zotero_tags_write` grouped-router actions: tag
//! listing, batch add/remove across items, renaming, and deletion.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, text_error, text_success},
    zotero::{ItemKey, TagName, ZoteroClient},
};

/// Arguments for `zotero_list_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ListTagsArgs {
    /// Maximum number of tags to return (default: 100).
    limit: Option<usize>,
}

/// Arguments for `zotero_search_by_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByTagArgs {
    /// Tag name ([`TagName`]) to search for.
    tag: TagName,
    /// Maximum number of items to return (default: 20).
    limit: Option<usize>,
}

/// Arguments for `zotero_batch_update_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BatchUpdateTagsArgs {
    /// List of item keys ([`ItemKey`]).
    item_keys: Vec<ItemKey>,
    /// Tags ([`TagName`]) to add.
    add_tags: Option<Vec<TagName>>,
    /// Tags ([`TagName`]) to remove.
    remove_tags: Option<Vec<TagName>>,
}

/// Arguments for `zotero_rename_tag`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct RenameTagArgs {
    /// Existing tag name ([`TagName`]).
    old_tag: TagName,
    /// New tag name ([`TagName`]).
    new_tag: TagName,
}

/// Arguments for `zotero_delete_tags`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct DeleteTagsArgs {
    /// Tag names ([`TagName`]) to delete from the library (up to 50).
    tags: Vec<TagName>,
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
}

impl ZoteroMcpServer {
    /// Handles Zotero tag listing tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
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
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_by_tag_impl(
        &self,
        args: SearchByTagArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(client.search_by_tag(&args.tag, limit).await))
    }

    /// Handles Zotero batch tag update tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_batch_update_tags_impl(
        &self,
        args: BatchUpdateTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let add = args.add_tags.unwrap_or_default();
        let rem = args.remove_tags.unwrap_or_default();
        match client.batch_update_tags(&args.item_keys, &add, &rem).await {
            Ok(count) => {
                Ok(text_success(format!("Batch updated tags on {count} items")))
            }
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero tag rename tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_rename_tag_impl(
        &self,
        args: RenameTagArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let old_tag = args.old_tag;
        let new_tag = args.new_tag;
        match client.rename_tag(&old_tag, &new_tag).await {
            Ok(count) => {
                Ok(text_success(format!("Renamed tag on {count} items")))
            }
            Err(e) => Ok(text_error(&e)),
        }
    }

    /// Handles Zotero tag deletion tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    async fn zotero_delete_tags_impl(
        &self,
        args: DeleteTagsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let tags = args.tags;
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
    use crate::{ZoteroMcpServer, mcp::zotero::fixtures::*};

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
