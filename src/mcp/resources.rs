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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::state::AppState;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        use super::AppState;

        pub(super) fn zotero_state(zotero_api_url: String) -> AppState {
            AppState {
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            }
        }

        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }
    }

    use fixtures::*;

    mod resources {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn list_resources_returns_collections_uri() {
            // Act
            let res = ZoteroMcpServer::list_resources_impl();

            // Assert
            assert_eq!(res.resources.len(), 1);
            assert_eq!(
                res.resources.first().expect("resource").raw.uri,
                "zotero://collections"
            );
        }

        #[tokio::test]
        async fn read_resource_returns_item_json_content() {
            // Arrange
            let item = json!({
                "key": "ITEM123",
                "version": 1,
                "data": { "key": "ITEM123", "itemType": "journalArticle", "title": "Resource Test Paper" }
            });
            let base =
                mock_server(vec![http_response("200 OK", &item.to_string())]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .read_resource_impl("zotero://items/ITEM123")
                .await
                .expect("read resource");

            // Assert
            assert_eq!(res.contents.len(), 1);
            let content = res.contents.first().expect("resource content");
            let is_text = matches!(
                content,
                rmcp::model::ResourceContents::TextResourceContents { text, .. }
                if text.contains("Resource Test Paper")
            );
            assert!(is_text);
        }

        #[tokio::test]
        async fn read_resource_returns_collections_json_content() {
            // Arrange
            let collections = json!([{
                "key": "COL1",
                "version": 1,
                "data": { "key": "COL1", "name": "Physics", "parentCollection": false }
            }]);
            let base = mock_server(vec![http_response(
                "200 OK",
                &collections.to_string(),
            )]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            // Act
            let res = server
                .read_resource_impl("zotero://collections")
                .await
                .expect("read collections");

            // Assert
            assert_eq!(res.contents.len(), 1);
        }

        #[tokio::test]
        async fn read_resource_returns_error_for_unrecognized_uri() {
            // Arrange
            let server = ZoteroMcpServer::new(zotero_state(String::new()));

            // Act
            let err = server
                .read_resource_impl("zotero://unknown_resource")
                .await
                .expect_err("should fail");

            // Assert
            assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        }
    }

    mod prompts {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn list_prompts_returns_literature_review_prompt() {
            // Act
            let list = ZoteroMcpServer::list_prompts_impl();

            // Assert
            assert_eq!(list.prompts.len(), 1);
            assert_eq!(
                list.prompts.first().expect("prompt").name,
                "zotero_literature_review"
            );
        }

        #[test]
        fn get_prompt_renders_literature_review_with_collection_key() {
            // Arrange
            let mut args = serde_json::Map::new();
            args.insert("collection_key".to_owned(), json!("COL123"));

            // Act
            let prompt = ZoteroMcpServer::get_prompt_impl(
                "zotero_literature_review",
                Some(&args),
            )
            .expect("get prompt");

            // Assert
            assert_eq!(prompt.messages.len(), 1);
            let msg = prompt.messages.first().expect("message");
            if let rmcp::model::PromptMessageContent::Text {
                text,
            } = &msg.content
            {
                assert!(text.contains("COL123"));
            } else {
                panic!("expected text content");
            }
        }

        #[test]
        fn get_prompt_returns_error_for_unknown_prompt() {
            // Act
            let err = ZoteroMcpServer::get_prompt_impl("unknown_prompt", None)
                .expect_err("unknown prompt should fail");

            // Assert
            assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        }
    }
}
