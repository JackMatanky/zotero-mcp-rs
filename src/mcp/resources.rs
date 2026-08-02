//! MCP resource and prompt handlers.
//!
//! This module implements handler logic for exposing Zotero library resources
//! and prompt templates via MCP.
//!
//! Exposed resources:
//! - `zotero://collections`: Returns all collection metadata in JSON format.
//! - `zotero://tags`: Returns all library tags in JSON format.
//!
//! Exposed resource templates:
//! - `zotero://items/{item_key}`: Returns item data for a specific Zotero item.
//!
//! Exposed prompts:
//! - `zotero_literature_review`: Generates a structured literature review
//!   prompt for a collection.

use rmcp::model::{
    GetPromptResult, PromptMessage, ReadResourceResult, ResourceContents, Role,
};
use serde::Serialize;

use crate::{
    ZoteroMcpServer,
    zotero::{CollectionKey, ItemKey, ItemType, ZoteroClient, ZoteroItem},
};

/// Builds a resource template where `name` is the programmatic identifier and
/// `title` is the human-readable display string.
///
/// MCP treats `name` as a logical identifier and `title` as UI copy, falling
/// back to `name` only when `title` is absent.
fn resource_template(
    uri_template: &str,
    name: &str,
    title: &str,
    description: &str,
) -> rmcp::model::ResourceTemplate {
    rmcp::model::ResourceTemplate::new(uri_template, name)
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

fn note_children(children: Vec<ZoteroItem>) -> Vec<ZoteroItem> {
    children
        .into_iter()
        .filter(|child| child.data.item_type == ItemType::Note)
        .collect()
}

impl ZoteroMcpServer {
    /// Lists MCP resources exposed by the server as a [`ListResourcesResult`].
    ///
    /// [`ListResourcesResult`]: rmcp::model::ListResourcesResult
    pub(crate) fn list_resources_impl() -> rmcp::model::ListResourcesResult {
        let collections =
            rmcp::model::Resource::new("zotero://collections", "collections")
                .with_title("Zotero Collections")
                .with_description("List of all collections in Zotero library")
                .with_mime_type("application/json");
        let tags = rmcp::model::Resource::new("zotero://tags", "tags")
            .with_title("Zotero Tags")
            .with_description("List of all tags in Zotero library")
            .with_mime_type("application/json");
        rmcp::model::ListResourcesResult::with_all_items(vec![
            collections,
            tags,
        ])
    }

    pub(crate) fn list_resource_templates_impl()
    -> rmcp::model::ListResourceTemplatesResult {
        rmcp::model::ListResourceTemplatesResult::with_all_items(vec![
            resource_template(
                "zotero://items/{item_key}",
                "item",
                "Zotero Item",
                "Read one Zotero item by key",
            ),
            resource_template(
                "zotero://items/{item_key}/children",
                "item_children",
                "Zotero Item Children",
                "Read child notes, attachments, and annotations for an item",
            ),
            resource_template(
                "zotero://items/{item_key}/notes",
                "item_notes",
                "Zotero Item Notes",
                "Read child notes for an item",
            ),
            resource_template(
                "zotero://items/{item_key}/relations",
                "item_relations",
                "Zotero Item Relations",
                "Read related items for an item",
            ),
            resource_template(
                "zotero://collections/{collection_key}/items",
                "collection_items",
                "Zotero Collection Items",
                "Read items in a collection",
            ),
        ])
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
            return client
                .get_collections()
                .await
                .map(|collections| json_resource(uri, &collections))
                .map_err(|e| {
                    rmcp::ErrorData::internal_error(e.to_string(), None)
                });
        }
        if uri == "zotero://tags" {
            return client
                .list_tags(500)
                .await
                .map(|tags| json_resource(uri, &tags))
                .map_err(|e| {
                    rmcp::ErrorData::internal_error(e.to_string(), None)
                });
        }
        if let Some(rest) = uri.strip_prefix("zotero://items/") {
            let mut parts = rest.split('/');
            let item_key = parts.next().unwrap_or_default();
            let nested = parts.next();
            if item_key.is_empty() || parts.next().is_some() {
                return Err(rmcp::ErrorData::invalid_params(
                    format!("Unknown resource URI: {uri}"),
                    None,
                ));
            }
            let item_key = ItemKey::from(item_key);
            return match nested {
                None => client
                    .get_item(&item_key)
                    .await
                    .map(|item| json_resource(uri, &item)),
                Some("children") => client
                    .get_item_children(&item_key)
                    .await
                    .map(|children| json_resource(uri, &children)),
                Some("notes") => {
                    client.get_item_children(&item_key).await.map(|children| {
                        json_resource(uri, &note_children(children))
                    })
                }
                Some("relations") => client
                    .get_related_items(&item_key)
                    .await
                    .map(|relations| json_resource(uri, &relations)),
                Some(_) => {
                    return Err(rmcp::ErrorData::invalid_params(
                        format!("Unknown resource URI: {uri}"),
                        None,
                    ));
                }
            }
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None));
        }
        if let Some(rest) = uri.strip_prefix("zotero://collections/") {
            match rest.strip_suffix("/items") {
                Some(collection_key) if !collection_key.contains('/') => {
                    let collection_key = CollectionKey::from(collection_key);
                    return client
                        .get_collection_items(&collection_key)
                        .await
                        .map(|items| json_resource(uri, &items))
                        .map_err(|e| {
                            rmcp::ErrorData::internal_error(e.to_string(), None)
                        });
                }
                _ => {}
            }
            return Err(rmcp::ErrorData::invalid_params(
                format!("Unknown resource URI: {uri}"),
                None,
            ));
        }
        Err(rmcp::ErrorData::invalid_params(
            format!("Unknown resource URI: {uri}"),
            None,
        ))
    }

    /// Lists MCP prompts exposed by the server as a [`ListPromptsResult`].
    ///
    /// [`ListPromptsResult`]: rmcp::model::ListPromptsResult
    pub(crate) fn list_prompts_impl() -> rmcp::model::ListPromptsResult {
        let argument = rmcp::model::PromptArgument::new("collection_key")
            .with_title("Collection Key")
            .with_description("Key of the Zotero collection")
            .with_required(true);
        let prompt = rmcp::model::Prompt::new(
            "zotero_literature_review",
            Some("Generate a literature review prompt for a Zotero collection"),
            Some(vec![argument]),
        )
        .with_title("Literature Review");
        rmcp::model::ListPromptsResult::with_all_items(vec![prompt])
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
            Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                Role::User,
                format!(
                    "Please perform a structured literature review of all \
                     paper items in Zotero collection key '{col_key}'."
                ),
            )])
            .with_description("Synthesize literature review from Zotero items"))
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
    ReadResourceResult::new(vec![
        ResourceContents::text(
            serde_json::to_string_pretty(value).unwrap_or_default(),
            uri.to_owned(),
        )
        .with_mime_type("application/json"),
    ])
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
        fn list_resources_returns_static_zotero_uris() {
            // Act
            let res = ZoteroMcpServer::list_resources_impl();

            // Assert
            let uris: Vec<&str> = res
                .resources
                .iter()
                .map(|resource| resource.uri.as_str())
                .collect();
            assert_eq!(uris, ["zotero://collections", "zotero://tags"]);
        }

        #[test]
        fn resource_names_are_identifiers_with_display_titles() {
            // Act
            let res = ZoteroMcpServer::list_resources_impl();

            // Assert
            let collections = res.resources.first().expect("resource");
            assert_eq!(collections.name, "collections");
            assert_eq!(
                collections.title.as_deref(),
                Some("Zotero Collections")
            );
            for resource in &res.resources {
                assert!(
                    !resource.name.contains(' '),
                    "resource name must be a programmatic identifier"
                );
            }
        }

        #[test]
        fn lists_item_and_collection_resource_templates() {
            let res = ZoteroMcpServer::list_resource_templates_impl();
            let templates: Vec<&str> = res
                .resource_templates
                .iter()
                .map(|template| template.uri_template.as_str())
                .collect();

            assert!(templates.contains(&"zotero://items/{item_key}"));
            assert!(
                templates
                    .contains(&"zotero://collections/{collection_key}/items")
            );
        }

        #[test]
        fn resource_templates_all_declare_a_uri_variable() {
            let res = ZoteroMcpServer::list_resource_templates_impl();

            for template in &res.resource_templates {
                assert!(
                    template.uri_template.contains('{'),
                    "{} is static and belongs in resources/list",
                    template.uri_template
                );
                assert!(!template.name.contains(' '));
                assert!(template.title.is_some());
            }
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
        async fn reads_item_children_resource_uri() {
            let children = json!([{
                "key": "CHILD1",
                "version": 1,
                "data": { "key": "CHILD1", "itemType": "note", "title": "Child Note" }
            }]);
            let base = mock_server(vec![http_response(
                "200 OK",
                &children.to_string(),
            )]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            let res = server
                .read_resource_impl("zotero://items/ITEM123/children")
                .await
                .expect("read item children");

            let content = res.contents.first().expect("resource content");
            let is_text = matches!(
                content,
                rmcp::model::ResourceContents::TextResourceContents { text, .. }
                if text.contains("Child Note")
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

        #[tokio::test]
        async fn unknown_nested_resource_uri_returns_invalid_params() {
            let server = ZoteroMcpServer::new(zotero_state(String::new()));

            let err = server
                .read_resource_impl("zotero://items/ITEM123/unknown")
                .await
                .expect_err("should fail");

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
            let text = msg.content.as_text().expect("text content");
            assert!(text.text.contains("COL123"));
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
