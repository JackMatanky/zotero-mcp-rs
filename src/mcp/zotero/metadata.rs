//! MCP tool handlers for Zotero item metadata and metadata lookup.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    better_bibtex::{BetterBibtexClient, TranslatorName},
    errors::ZoteroMcpError,
    mcp::{json_result, json_success, text_error, text_result},
    zotero::{
        CollectionKey, ItemKey, JoinMode, SearchCondition, SearchField,
        SearchOperator, SortDirection, ZoteroClient,
    },
};

#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(in crate::mcp::zotero) enum MetadataFormat {
    /// Return Zotero item metadata as JSON.
    #[default]
    Json,
    /// Return item metadata as Better `BibTeX`.
    Bibtex,
}
/// Arguments for `zotero_get_item_metadata`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetItemMetadataArgs {
    /// Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
    /// Format: `"json"` or `"bibtex"` ([`MetadataFormat`]), defaulting to
    /// `"json"`.
    format: Option<MetadataFormat>,
}
impl GetItemMetadataArgs {
    pub(crate) fn json(item_key: ItemKey) -> Self {
        Self {
            item_key,
            format: None,
        }
    }
}

/// Arguments for `zotero_add_by_identifier`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AddByIdentifierArgs {
    /// Kind of identifier ([`IdentifierKind`](crate::zotero::IdentifierKind)).
    kind: crate::zotero::IdentifierKind,
    /// The DOI, arXiv ID, or ISBN to resolve.
    identifier: String,
    /// Optional collection key ([`CollectionKey`]) to file the new item into.
    collection_key: Option<CollectionKey>,
}

#[tool_router(router = metadata_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_get_item_metadata",
        description = "Get metadata for an item as JSON or formatted BibTeX \
                       string",
        annotations(
            title = "Get Item Metadata",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_metadata(
        &self,
        Parameters(args): Parameters<GetItemMetadataArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_metadata_impl(args).await
    }

    #[tool(
        name = "zotero_add_by_identifier",
        description = "Resolve a DOI, arXiv ID, or ISBN via public metadata \
                       APIs and add it to the library (returns the existing \
                       item instead of creating a duplicate if an exact title \
                       match is already present) (requires write permission)",
        annotations(
            title = "Add Item by Identifier",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_add_by_identifier(
        &self,
        Parameters(args): Parameters<AddByIdentifierArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_add_by_identifier_impl(args).await
    }
}

impl ZoteroMcpServer {
    /// Handles Zotero item metadata formatting tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_get_item_metadata_impl(
        &self,
        args: GetItemMetadataArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if args.format.unwrap_or_default() == MetadataFormat::Bibtex {
            let bbt_client = BetterBibtexClient::new(&self.state);
            let translator = TranslatorName::from("bibtex");
            let result = async {
                let citekeys = bbt_client
                    .get_citekeys(std::slice::from_ref(&args.item_key))
                    .await?;
                let citekey = citekeys
                    .get(&args.item_key)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        ZoteroMcpError::BetterBibTeX(format!(
                            "no citation key for item {}",
                            args.item_key
                        ))
                    })?;
                bbt_client
                    .export_items(std::slice::from_ref(citekey), &translator)
                    .await
            }
            .await;
            Ok(text_result(result))
        } else {
            let client = ZoteroClient::new(&self.state);
            Ok(json_result(client.get_item(&args.item_key).await))
        }
    }

    /// Handles Zotero add-by-identifier tool calls using `args`.
    ///
    /// Resolves the identifier via a public metadata API and creates the item,
    /// returning the existing item instead if an exact title match is already
    /// present in the library.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_add_by_identifier_impl(
        &self,
        args: AddByIdentifierArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let mut draft = match crate::zotero::metadata::resolve_metadata(
            &self.state,
            args.kind,
            &args.identifier,
        )
        .await
        {
            Ok(d) => d,
            Err(e) => return Ok(text_error(&e)),
        };

        if !draft.title.is_empty() {
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::Is,
                value: draft.title.clone(),
            };
            let existing = client
                .advanced_search(
                    vec![cond],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    1,
                )
                .await;
            if let Ok(page) = existing {
                if let Some(found) = page.items.into_iter().next() {
                    return Ok(json_success(&found));
                }
            }
        }

        if let Some(col) = args.collection_key {
            draft.collections.push(col);
        }
        Ok(json_result(client.create_item_from_metadata(draft).await))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ZoteroMcpServer, mcp::zotero::fixtures::*, state::AppState};

    mod metadata {

        use super::*;

        #[tokio::test]
        async fn add_by_identifier_creates_new_item() {
            // Arrange
            let crossref_body = json!({"message": {
                "title": ["A Great Paper"],
                "author": [{"given": "Sam", "family": "McAuthor"}],
                "published": {"date-parts": [[2021]]},
                "DOI": "10.1/xyz",
                "URL": "https://doi.org/10.1/xyz",
                "container-title": ["Journal of Things"]
            }});
            let crossref_base = mock_server(vec![http_response(
                "200 OK",
                &crossref_body.to_string(),
            )]);
            let created = json!([{
                "key": "NEWITEM1",
                "version": 1,
                "data": { "key": "NEWITEM1", "version": 1, "itemType": "journalArticle", "title": "A Great Paper" }
            }]);
            let zotero_base = mock_server(vec![
                http_response("200 OK", "[]"),
                http_response("200 OK", &created.to_string()),
            ]);
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: crossref_base,
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_add_by_identifier_impl(AddByIdentifierArgs {
                    kind: crate::zotero::IdentifierKind::Doi,
                    identifier: "10.1/xyz".to_owned(),
                    collection_key: None,
                })
                .await
                .expect("add by identifier ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
        }

        #[tokio::test]
        async fn add_by_identifier_returns_existing_item_when_duplicate_found()
        {
            // Arrange
            let crossref_body = json!({"message": {
                "title": ["A Great Paper"],
                "author": [{"given": "Sam", "family": "McAuthor"}],
                "published": {"date-parts": [[2021]]},
                "DOI": "10.1/xyz",
                "URL": "https://doi.org/10.1/xyz",
                "container-title": ["Journal of Things"]
            }});
            let crossref_base = mock_server(vec![http_response(
                "200 OK",
                &crossref_body.to_string(),
            )]);
            let existing = json!([{
                "key": "EXISTING1",
                "version": 1,
                "data": { "key": "EXISTING1", "version": 1, "itemType": "journalArticle", "title": "A Great Paper" }
            }]);
            let zotero_base = mock_server(vec![http_response(
                "200 OK",
                &existing.to_string(),
            )]);
            let server = ZoteroMcpServer::new(AppState {
                zotero_api_url: zotero_base,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: crossref_base,
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: true,
                ..AppState::from_env()
            });

            // Act
            let res = server
                .zotero_add_by_identifier_impl(AddByIdentifierArgs {
                    kind: crate::zotero::IdentifierKind::Doi,
                    identifier: "10.1/xyz".to_owned(),
                    collection_key: None,
                })
                .await
                .expect("add by identifier duplicate ok");

            // Assert
            assert_eq!(res.is_error, Some(false));
            let text = tool_text(&res);
            assert!(text.contains("EXISTING1"));
        }
    }
}
