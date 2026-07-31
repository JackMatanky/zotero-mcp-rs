//! MCP resource and prompt handlers.
//!
//! This module implements handler logic for exposing Zotero library resources
//! and prompt templates via MCP.
//!
//! Exposed resources:
//! - `zotero://collections`: Returns all collection metadata in JSON format.
//! - `zotero://items/{item_key}`: Returns item data for a specific Zotero item.
//!
//! Exposed prompts:
//! - `zotero_literature_review`: Generates a structured literature review
//!   prompt for a collection.

use serde::Serialize;

use crate::{
    ZoteroMcpServer,
    zotero::{ItemKey, ZoteroClient},
};

impl ZoteroMcpServer {
    /// Lists MCP resources exposed by the server as a [`ListResourcesResult`].
    ///
    /// [`ListResourcesResult`]: rmcp::model::ListResourcesResult
    pub(crate) fn list_resources_impl() -> rmcp::model::ListResourcesResult {
        let raw_resource = rmcp::model::RawResource {
            uri: "zotero://collections".to_owned(),
            name: "Zotero Collections".to_owned(),
            title: None,
            description: Some(
                "List of all collections in Zotero library".to_owned(),
            ),
            icons: None,
            mime_type: Some("application/json".to_owned()),
            size: None,
        };
        rmcp::model::ListResourcesResult {
            resources: vec![rmcp::model::Annotated::new(raw_resource, None)],
            next_cursor: None,
        }
    }

    /// Reads a single MCP resource identified by `uri`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if `uri` is unrecognized or resource reading fails
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) async fn read_resource_impl(
        &self,
        uri: &str,
    ) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        if uri == "zotero://collections" {
            match client.get_collections().await {
                Ok(collections) => Ok(json_resource(uri, &collections)),
                Err(e) => {
                    Err(rmcp::ErrorData::internal_error(e.to_string(), None))
                }
            }
        } else if let Some(item_key) = uri.strip_prefix("zotero://items/") {
            let item_key = ItemKey::from(item_key);
            match client.get_item(&item_key).await {
                Ok(item) => Ok(json_resource(uri, &item)),
                Err(e) => {
                    Err(rmcp::ErrorData::internal_error(e.to_string(), None))
                }
            }
        } else {
            Err(rmcp::ErrorData::invalid_params(
                format!("Unknown resource URI: {uri}"),
                None,
            ))
        }
    }

    /// Lists MCP prompts exposed by the server as a [`ListPromptsResult`].
    ///
    /// [`ListPromptsResult`]: rmcp::model::ListPromptsResult
    pub(crate) fn list_prompts_impl() -> rmcp::model::ListPromptsResult {
        let prompt = rmcp::model::Prompt {
            name: "zotero_literature_review".to_owned(),
            title: None,
            description: Some(
                "Generate a literature review prompt for a Zotero collection"
                    .to_owned(),
            ),
            icons: None,
            arguments: Some(vec![rmcp::model::PromptArgument {
                name: "collection_key".to_owned(),
                title: None,
                description: Some("Key of the Zotero collection".to_owned()),
                required: Some(true),
            }]),
        };
        rmcp::model::ListPromptsResult {
            prompts: vec![prompt],
            next_cursor: None,
        }
    }

    /// Builds an MCP prompt response for `name` using `arguments`.
    ///
    /// # Errors
    ///
    /// - [`ErrorData`] if `name` is not a recognized prompt
    ///
    /// [`ErrorData`]: rmcp::ErrorData
    pub(crate) fn get_prompt_impl(
        name: &str,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<rmcp::model::GetPromptResult, rmcp::ErrorData> {
        if name == "zotero_literature_review" {
            let col_key = arguments
                .and_then(|args| args.get("collection_key"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Ok(rmcp::model::GetPromptResult {
                description: Some(
                    "Synthesize literature review from Zotero items".to_owned(),
                ),
                messages: vec![rmcp::model::PromptMessage {
                    role: rmcp::model::PromptMessageRole::User,
                    content: rmcp::model::PromptMessageContent::Text {
                        text: format!(
                            "Please perform a structured literature review of \
                             all paper items in Zotero collection key \
                             '{col_key}'."
                        ),
                    },
                }],
            })
        } else {
            Err(rmcp::ErrorData::invalid_params(
                format!("Unknown prompt: {name}"),
                None,
            ))
        }
    }
}

/// Formats `value` as pretty JSON and constructs a [`ReadResourceResult`] for
/// `uri`.
///
/// [`ReadResourceResult`]: rmcp::model::ReadResourceResult
fn json_resource<T: Serialize>(
    uri: &str,
    value: &T,
) -> rmcp::model::ReadResourceResult {
    rmcp::model::ReadResourceResult {
        contents: vec![rmcp::model::ResourceContents::TextResourceContents {
            uri: uri.to_owned(),
            mime_type: Some("application/json".to_owned()),
            text: serde_json::to_string_pretty(value).unwrap_or_default(),
            meta: None,
        }],
    }
}
