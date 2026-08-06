//! MCP tool handlers and argument models for Zotero library search.
//!
//! Exposes the `zotero_search` MCP tool router, allowing clients to search
//! items by full text, tag, citation key, structured multi-condition
//! queries, duplicate detection, and library coverage analysis.
//!
//! # Main Types
//!
//! - [`ZoteroSearchCommand`] - Grouped-router command for search actions
//! - [`SearchItemsArgs`] - Arguments for full-text search across item fields
//! - [`SearchByCitationKeyArgs`] - Arguments for searching items by citation
//!   key
//! - [`AdvancedSearchArgs`] - Arguments for structured multi-condition search
//! - [`FindDuplicatesArgs`] - Arguments for duplicate detection action
//! - [`LibraryCoverageArgs`] - Arguments for library coverage metrics action
//!
//! # Examples
//!
//! ```ignore
//! # use rmcp::handler::server::wrapper::Parameters;
//! # use zotero_mcp::ZoteroMcpServer;
//! # use zotero_zotero::search::{
//! #     ZoteroSearchCommand,
//! #     SearchItemsArgs,
//! # };
//! # async fn run(
//! #     server: ZoteroMcpServer,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let args = Parameters(ZoteroSearchCommand::Items(
//!     SearchItemsArgs::for_connector("quantum computing".to_string()),
//! ));
//! let result = server.zotero_search(args).await?;
//! # Ok(())
//! # }
//! ```

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use zotero_api::{CitationKey, CollectionKey, ZoteroClient};

use super::tags::SearchByTagArgs;
use crate::{ZoteroMcpServer, response::json_result};

/// Mirrors `zotero_api::SearchField` for MCP argument schemas.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SearchFieldArg {
    Title,
    Creator,
    Date,
    Year,
    ItemType,
    Tag,
    Extra,
    Doi,
    #[serde(untagged)]
    Other(String),
}
impl From<SearchFieldArg> for zotero_api::SearchField {
    #[inline]
    fn from(value: SearchFieldArg) -> Self {
        match value {
            SearchFieldArg::Title => Self::Title,
            SearchFieldArg::Creator => Self::Creator,
            SearchFieldArg::Date => Self::Date,
            SearchFieldArg::Year => Self::Year,
            SearchFieldArg::ItemType => Self::ItemType,
            SearchFieldArg::Tag => Self::Tag,
            SearchFieldArg::Extra => Self::Extra,
            SearchFieldArg::Doi => Self::Doi,
            SearchFieldArg::Other(s) => Self::Other(s),
        }
    }
}

/// Mirrors `zotero_api::SearchOperator` for MCP argument schemas.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SearchOperatorArg {
    #[default]
    Contains,
    Is,
    StartsWith,
    EndsWith,
    IsNot,
    DoesNotContain,
    IsGreaterThan,
    IsLessThan,
    IsBefore,
    IsAfter,
    #[serde(untagged)]
    Other(String),
}
impl From<SearchOperatorArg> for zotero_api::SearchOperator {
    #[inline]
    fn from(value: SearchOperatorArg) -> Self {
        match value {
            SearchOperatorArg::Contains => Self::Contains,
            SearchOperatorArg::Is => Self::Is,
            SearchOperatorArg::StartsWith => Self::StartsWith,
            SearchOperatorArg::EndsWith => Self::EndsWith,
            SearchOperatorArg::IsNot => Self::IsNot,
            SearchOperatorArg::DoesNotContain => Self::DoesNotContain,
            SearchOperatorArg::IsGreaterThan => Self::IsGreaterThan,
            SearchOperatorArg::IsLessThan => Self::IsLessThan,
            SearchOperatorArg::IsBefore => Self::IsBefore,
            SearchOperatorArg::IsAfter => Self::IsAfter,
            SearchOperatorArg::Other(s) => Self::Other(s),
        }
    }
}

/// Mirrors `zotero_api::SearchCondition` for MCP argument schemas.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub(crate) struct SearchConditionArg {
    field: SearchFieldArg,
    #[serde(default)]
    operator: SearchOperatorArg,
    value: String,
}
impl From<SearchConditionArg> for zotero_api::SearchCondition {
    #[inline]
    fn from(value: SearchConditionArg) -> Self {
        Self {
            field: value.field.into(),
            operator: value.operator.into(),
            value: value.value,
        }
    }
}

/// Mirrors `zotero_api::JoinMode` for MCP argument schemas.
#[derive(Copy, Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum JoinModeArg {
    #[default]
    All,
    Any,
}
impl From<JoinModeArg> for zotero_api::JoinMode {
    #[inline]
    fn from(value: JoinModeArg) -> Self {
        match value {
            JoinModeArg::All => Self::All,
            JoinModeArg::Any => Self::Any,
        }
    }
}

/// Mirrors `zotero_api::SortField` for MCP argument schemas.
#[derive(Copy, Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SortFieldArg {
    DateAdded,
    DateModified,
    Title,
    Date,
    Creator,
}
impl From<SortFieldArg> for zotero_api::SortField {
    #[inline]
    fn from(value: SortFieldArg) -> Self {
        match value {
            SortFieldArg::DateAdded => Self::DateAdded,
            SortFieldArg::DateModified => Self::DateModified,
            SortFieldArg::Title => Self::Title,
            SortFieldArg::Date => Self::Date,
            SortFieldArg::Creator => Self::Creator,
        }
    }
}

/// Mirrors `zotero_api::SortDirection` for MCP argument schemas.
#[derive(Copy, Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SortDirectionArg {
    #[default]
    Asc,
    Desc,
}
impl From<SortDirectionArg> for zotero_api::SortDirection {
    #[inline]
    fn from(value: SortDirectionArg) -> Self {
        match value {
            SortDirectionArg::Asc => Self::Asc,
            SortDirectionArg::Desc => Self::Desc,
        }
    }
}

/// Arguments for the connector-compatible `search` tool.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ConnectorSearchArgs {
    /// Search query string matched against title, creator, or metadata
    /// fields.
    pub(crate) query: String,
}

/// Arguments for the `items` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query matched against title, creator, year, or full-text
    /// content.
    query: String,
    /// Optional collection key ([`CollectionKey`]) to search within.
    collection_key: Option<String>,
    /// Zero-based offset into the full result set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    limit: Option<usize>,
}

impl SearchItemsArgs {
    /// Constructs full-text search arguments with default offset and limit.
    pub(crate) fn for_connector(query: String) -> Self {
        Self {
            query,
            collection_key: None,
            start: None,
            limit: Some(20),
        }
    }
}

/// Arguments for the `citation_key` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchByCitationKeyArgs {
    /// Citation key ([`CitationKey`]) to match.
    citekey: String,
}

/// Arguments for the `advanced` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions
    /// ([`SearchCondition`](zotero_api::SearchCondition)).
    conditions: Vec<SearchConditionArg>,
    /// Match mode: `"all"` ([`JoinMode::All`](zotero_api::JoinMode::All), AND,
    /// default) or `"any"` ([`JoinMode::Any`](zotero_api::JoinMode::Any),
    /// OR).
    join_mode: Option<JoinModeArg>,
    /// Sort field ([`SortField`](zotero_api::SortField)): `"dateAdded"`,
    /// `"dateModified"`, `"title"`, `"date"`, or `"creator"`.
    sort_by: Option<SortFieldArg>,
    /// Sort direction: `"asc"` or `"desc"`
    /// ([`SortDirection`](zotero_api::SortDirection), default: `"asc"`).
    sort_direction: Option<SortDirectionArg>,
    /// Zero-based offset into the full result set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    limit: Option<usize>,
}

/// Arguments for the `duplicates` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindDuplicatesArgs {
    /// Optional collection key ([`CollectionKey`]) to scope duplicate search.
    collection_key: Option<String>,
}

/// Arguments for the `coverage` action of `zotero_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct LibraryCoverageArgs {
    /// Optional collection key ([`CollectionKey`]) to scope coverage analysis.
    collection_key: Option<String>,
    /// Zero-based offset into the item set (default: 0).
    start: Option<usize>,
    /// Maximum number of items to analyze (default: 100, max: 500).
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[schemars(extend("type" = "object"))]
/// Search commands dispatched by the `zotero_search` MCP tool router.
pub(crate) enum ZoteroSearchCommand {
    /// Full-text search across item fields.
    Items(SearchItemsArgs),
    /// Find items by tag name.
    Tag(SearchByTagArgs),
    /// Find items by `BibTeX` citation key.
    CitationKey(SearchByCitationKeyArgs),
    /// Run a structured search with multiple conditions.
    Advanced(AdvancedSearchArgs),
    /// Find potential duplicate items in a library.
    Duplicates(FindDuplicatesArgs),
    /// Report coverage statistics for a library.
    Coverage(LibraryCoverageArgs),
}

#[tool_router(router = search_router, vis = "pub(crate)")]
impl ZoteroMcpServer {
    #[tool(
        name = "zotero_search",
        description = "Grouped Zotero search router. action: items, tag, \
                       citation_key, advanced, duplicates, coverage",
        annotations(
            title = "Search Zotero Library",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// Dispatches search requests to the appropriate search handler.
    ///
    /// Accepts a [`Parameters<ZoteroSearchCommand>`] containing the specific
    /// action and parameters, routing it to internal search handlers.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if the underlying tool handler fails or
    /// returns an error.
    pub(crate) async fn zotero_search(
        &self,
        Parameters(args): Parameters<ZoteroSearchCommand>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match args {
            ZoteroSearchCommand::Items(args) => {
                self.zotero_search_items_impl(args).await
            }
            ZoteroSearchCommand::Tag(args) => {
                self.zotero_search_by_tag_impl(args).await
            }
            ZoteroSearchCommand::CitationKey(args) => {
                self.zotero_search_by_citation_key_impl(args).await
            }
            ZoteroSearchCommand::Advanced(args) => {
                self.zotero_advanced_search_impl(args).await
            }
            ZoteroSearchCommand::Duplicates(args) => {
                self.zotero_find_duplicates_impl(args).await
            }
            ZoteroSearchCommand::Coverage(args) => {
                self.zotero_library_coverage_impl(args).await
            }
        }
    }

    #[tool(
        name = "search",
        description = "Connector search tool - search Zotero items by query",
        annotations(
            title = "Search Zotero",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn connector_search(
        &self,
        Parameters(args): Parameters<ConnectorSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_items_impl(SearchItemsArgs::for_connector(
            args.query,
        ))
        .await
    }
}

impl ZoteroMcpServer {
    /// Handles Zotero item search tool calls.
    ///
    /// Queries the Zotero API using the provided [`SearchItemsArgs`] parameters
    /// and returns matching items as MCP JSON content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if protocol-level failures occur. Backend
    /// failures from [`ZoteroClient::search_items`] are formatted as MCP JSON
    /// error responses.
    pub(crate) async fn zotero_search_items_impl(
        &self,
        args: SearchItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .search_items(
                    &args.query,
                    args.collection_key.map(CollectionKey::from).as_ref(),
                    offset,
                    limit,
                )
                .await,
        ))
    }

    /// Handles Zotero citation-key search tool calls.
    ///
    /// Queries the Zotero API using the provided [`SearchByCitationKeyArgs`]
    /// parameters and returns matching items as MCP JSON content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if protocol-level failures occur. Backend
    /// failures from [`ZoteroClient::search_by_citation_key`] are formatted as
    /// MCP JSON error responses.
    async fn zotero_search_by_citation_key_impl(
        &self,
        args: SearchByCitationKeyArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .search_by_citation_key(&CitationKey::from(args.citekey))
                .await,
        ))
    }

    /// Handles Zotero structured search tool calls.
    ///
    /// Executes a multi-condition query against the Zotero API using
    /// [`AdvancedSearchArgs`] and returns matching items as MCP JSON
    /// content.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] if protocol-level failures occur. Backend
    /// failures from [`ZoteroClient::advanced_search`] are formatted as MCP
    /// JSON error responses.
    async fn zotero_advanced_search_impl(
        &self,
        args: AdvancedSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .advanced_search(
                    args.conditions.into_iter().map(Into::into).collect(),
                    args.join_mode.map(Into::into).unwrap_or_default(),
                    args.sort_by.map(Into::into),
                    args.sort_direction.map(Into::into).unwrap_or_default(),
                    offset,
                    limit,
                )
                .await,
        ))
    }

    /// Handles Zotero duplicate detection tool calls.
    ///
    /// Scans for potential duplicate items in the library or optional
    /// collection specified by `args` using [`ZoteroClient`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_find_duplicates_impl(
        &self,
        args: FindDuplicatesArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .find_duplicates(
                    args.collection_key.map(CollectionKey::from).as_ref(),
                )
                .await,
        ))
    }

    /// Handles Zotero library coverage analysis tool calls.
    ///
    /// Analyzes library coverage metrics for the requested range using
    /// [`ZoteroClient`].
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::zotero) async fn zotero_library_coverage_impl(
        &self,
        args: LibraryCoverageArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(100).min(500);
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .get_library_coverage(
                    args.collection_key.map(CollectionKey::from).as_ref(),
                    offset,
                    limit,
                )
                .await,
        ))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ZoteroMcpServer, zotero::fixtures::*};

    mod connector_operations {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn connector_search_returns_matching_items() {
            let item = json!({
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Physics Paper" }
            });
            let base = mock_server(vec![http_response(
                "200 OK",
                &json!([item]).to_string(),
            )]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            let res = server
                .connector_search(Parameters(ConnectorSearchArgs {
                    query: "quantum".to_owned(),
                }))
                .await
                .expect("search succeeded");

            assert_eq!(res.is_error, Some(false));
        }
    }

    mod duplicates {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn find_duplicates_returns_success() {
            let base = mock_server(vec![http_response("200 OK", "[]")]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            let res = server
                .zotero_find_duplicates_impl(FindDuplicatesArgs {
                    collection_key: None,
                })
                .await
                .expect("duplicates succeeded");

            assert_eq!(res.is_error, Some(false));
        }
    }

    mod coverage {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn library_coverage_returns_success() {
            let base = mock_server(vec![http_response_with_headers(
                "200 OK",
                &[("Total-Results", "0")],
                "[]",
            )]);
            let server = ZoteroMcpServer::new(zotero_state(base));

            let res = server
                .zotero_library_coverage_impl(LibraryCoverageArgs {
                    collection_key: None,
                    start: Some(0),
                    limit: Some(10),
                })
                .await
                .expect("coverage succeeded");

            assert_eq!(res.is_error, Some(false));
        }
    }

    /// Reverse-exhaustive matches on each `zotero_api` domain enum: if a
    /// variant is added there, these fail to compile until the matching
    /// `*Arg` mirror (and its `From` impl above) is updated too, catching
    /// schema drift that a one-directional `From<Arg> for Domain` match
    /// cannot.
    mod arg_mirrors {
        use super::*;

        #[test]
        fn search_field_arg_covers_every_search_field_variant() {
            fn to_arg(field: zotero_api::SearchField) -> SearchFieldArg {
                match field {
                    zotero_api::SearchField::Title => SearchFieldArg::Title,
                    zotero_api::SearchField::Creator => SearchFieldArg::Creator,
                    zotero_api::SearchField::Date => SearchFieldArg::Date,
                    zotero_api::SearchField::Year => SearchFieldArg::Year,
                    zotero_api::SearchField::ItemType => {
                        SearchFieldArg::ItemType
                    }
                    zotero_api::SearchField::Tag => SearchFieldArg::Tag,
                    zotero_api::SearchField::Extra => SearchFieldArg::Extra,
                    zotero_api::SearchField::Doi => SearchFieldArg::Doi,
                    zotero_api::SearchField::Other(s) => {
                        SearchFieldArg::Other(s)
                    }
                }
            }
            let _ = to_arg;
        }

        #[test]
        fn search_operator_arg_covers_every_search_operator_variant() {
            fn to_arg(op: zotero_api::SearchOperator) -> SearchOperatorArg {
                match op {
                    zotero_api::SearchOperator::Contains => {
                        SearchOperatorArg::Contains
                    }
                    zotero_api::SearchOperator::Is => SearchOperatorArg::Is,
                    zotero_api::SearchOperator::StartsWith => {
                        SearchOperatorArg::StartsWith
                    }
                    zotero_api::SearchOperator::EndsWith => {
                        SearchOperatorArg::EndsWith
                    }
                    zotero_api::SearchOperator::IsNot => {
                        SearchOperatorArg::IsNot
                    }
                    zotero_api::SearchOperator::DoesNotContain => {
                        SearchOperatorArg::DoesNotContain
                    }
                    zotero_api::SearchOperator::IsGreaterThan => {
                        SearchOperatorArg::IsGreaterThan
                    }
                    zotero_api::SearchOperator::IsLessThan => {
                        SearchOperatorArg::IsLessThan
                    }
                    zotero_api::SearchOperator::IsBefore => {
                        SearchOperatorArg::IsBefore
                    }
                    zotero_api::SearchOperator::IsAfter => {
                        SearchOperatorArg::IsAfter
                    }
                    zotero_api::SearchOperator::Other(s) => {
                        SearchOperatorArg::Other(s)
                    }
                }
            }
            let _ = to_arg;
        }

        #[test]
        fn join_mode_arg_covers_every_join_mode_variant() {
            fn to_arg(mode: zotero_api::JoinMode) -> JoinModeArg {
                match mode {
                    zotero_api::JoinMode::All => JoinModeArg::All,
                    zotero_api::JoinMode::Any => JoinModeArg::Any,
                }
            }
            let _ = to_arg;
        }

        #[test]
        fn sort_field_arg_covers_every_sort_field_variant() {
            fn to_arg(field: zotero_api::SortField) -> SortFieldArg {
                match field {
                    zotero_api::SortField::DateAdded => SortFieldArg::DateAdded,
                    zotero_api::SortField::DateModified => {
                        SortFieldArg::DateModified
                    }
                    zotero_api::SortField::Title => SortFieldArg::Title,
                    zotero_api::SortField::Date => SortFieldArg::Date,
                    zotero_api::SortField::Creator => SortFieldArg::Creator,
                }
            }
            let _ = to_arg;
        }

        #[test]
        fn sort_direction_arg_covers_every_sort_direction_variant() {
            fn to_arg(dir: zotero_api::SortDirection) -> SortDirectionArg {
                match dir {
                    zotero_api::SortDirection::Asc => SortDirectionArg::Asc,
                    zotero_api::SortDirection::Desc => SortDirectionArg::Desc,
                }
            }
            let _ = to_arg;
        }
    }
}
