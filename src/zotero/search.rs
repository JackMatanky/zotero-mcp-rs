//! Search and query operations for the Zotero Local HTTP API.
//!
//! Implements methods on [`ZoteroClient`] for free-text search, tag queries,
//! citation key matching, and structured multi-condition advanced search.
//!
//! # Key Types & Operations
//!
//! - [`ZoteroClient::search_items`] - Free-text search matching title, creator,
//!   year, or fulltext
//! - [`ZoteroClient::search_by_citation_key`] - Lookup by native or legacy
//!   citation key
//! - [`ZoteroClient::advanced_search`] - Multi-condition search using
//!   [`SearchCondition`], [`SearchField`], and [`SearchOperator`]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        client::ZoteroClient,
        models::{CitationKey, CollectionKey, TagName, ZoteroItem},
    },
};

/// Searchable item field in structured searches.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SearchField {
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

/// Comparison operator in structured searches.
#[derive(
    Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SearchOperator {
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

/// Structured search condition matching a specific item field.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct SearchCondition {
    pub(crate) field: SearchField,
    #[serde(default)]
    pub(crate) operator: SearchOperator,
    pub(crate) value: String,
}

/// How multiple conditions are combined: `all` (AND, default) or `any` (OR).
#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Deserialize,
    Serialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum JoinMode {
    #[default]
    All,
    Any,
}

/// Item field to sort results by.
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SortField {
    DateAdded,
    DateModified,
    Title,
    Date,
    Creator,
}

/// Sort direction.
#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Deserialize,
    Serialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SortDirection {
    #[default]
    Asc,
    Desc,
}

impl ZoteroClient<'_> {
    /// Searches library items matching a query string, excluding notes.
    ///
    /// # Arguments
    ///
    /// * `query` - Free-text query matching title, creator, year, or fulltext
    /// * `collection_key` - Optional collection key to scope the search
    /// * `limit` - Maximum number of items to return
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn search_items(
        &self,
        query: &str,
        collection_key: Option<&CollectionKey>,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let base = match collection_key {
            Some(col) => format!(
                "{}/users/0/collections/{}/items",
                self.state.zotero_api_url, col
            ),
            None => format!("{}/users/0/items", self.state.zotero_api_url),
        };
        let encoded_q = urlencoding::encode(query);
        let url = format!("{base}?q={encoded_q}&limit={limit}&itemType=-note");

        self.get_json(&url).await
    }

    /// Searches items by `tag` name, returning at most `limit` items (excluding
    /// notes).
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn search_by_tag(
        &self,
        tag: &TagName,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let encoded_tag = urlencoding::encode(tag.as_str());
        let url = format!(
            "{}/users/0/items?tag={}&limit={}&itemType=-note",
            self.state.zotero_api_url, encoded_tag, limit
        );
        self.get_json(&url).await
    }

    /// Searches items by citation key.
    ///
    /// Matches Zotero's native `citationKey` item field first (Zotero 9+).
    /// Falls back to scanning the legacy `extra` field for items with no native
    /// citation key.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn search_by_citation_key(
        &self,
        citekey: &CitationKey,
    ) -> Result<Option<ZoteroItem>, ZoteroMcpError> {
        let items = self.search_items(citekey.as_str(), None, 20).await?;
        let citekey_lc = citekey.as_str().to_lowercase();
        for item in items {
            if let Some(native) = &item.data.citation_key {
                if native.to_lowercase() == citekey_lc {
                    return Ok(Some(item));
                }
                continue;
            }
            if let Some(extra) = &item.data.extra {
                let extra_lc = extra.to_lowercase();
                if extra_lc.contains(&format!("citation key: {citekey_lc}"))
                    || extra_lc.contains(&format!("citationkey: {citekey_lc}"))
                    || extra_lc.contains(&citekey_lc)
                {
                    return Ok(Some(item));
                }
            }
        }
        Ok(None)
    }

    /// Executes an advanced multi-condition structured search over item fields.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn advanced_search(
        &self,
        conditions: Vec<SearchCondition>,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let items = self.get_all_items().await?;
        let results = items
            .into_iter()
            .filter(|item| {
                conditions.iter().all(|cond| match_condition(item, cond))
            })
            .take(limit)
            .collect();
        Ok(results)
    }
}

/// Evaluates whether `item` satisfies a single search `cond`.
fn match_condition(item: &ZoteroItem, cond: &SearchCondition) -> bool {
    let val = cond.value.to_lowercase();
    let matches_str = |s: &str| match cond.operator {
        SearchOperator::Is => s.to_lowercase() == val,
        SearchOperator::IsNot => s.to_lowercase() != val,
        SearchOperator::StartsWith => s.to_lowercase().starts_with(&val),
        SearchOperator::EndsWith => s.to_lowercase().ends_with(&val),
        SearchOperator::DoesNotContain => !s.to_lowercase().contains(&val),
        SearchOperator::Contains | SearchOperator::Other(_) => {
            s.to_lowercase().contains(&val)
        }
        SearchOperator::IsGreaterThan | SearchOperator::IsAfter => {
            compare_dates(s, &cond.value).is_gt()
        }
        SearchOperator::IsLessThan | SearchOperator::IsBefore => {
            compare_dates(s, &cond.value).is_lt()
        }
    };

    match &cond.field {
        SearchField::Title => {
            item.data.title.as_deref().is_some_and(matches_str)
        }
        SearchField::Creator => item.data.creators.iter().any(|c| {
            let full = format!(
                "{} {}",
                c.first_name.as_deref().unwrap_or(""),
                c.last_name.as_deref().unwrap_or("")
            );
            matches_str(&full) || c.name.as_deref().is_some_and(matches_str)
        }),
        SearchField::Date => item.data.date.as_deref().is_some_and(matches_str),
        SearchField::Year => item.data.date.as_deref().is_some_and(|d| {
            matches_str(&d.chars().take(4).collect::<String>())
        }),
        SearchField::ItemType => matches_str(item.data.item_type.as_str()),
        SearchField::Tag => item.data.tags.iter().any(|t| matches_str(&t.tag)),
        SearchField::Extra => {
            item.data.extra.as_deref().is_some_and(matches_str)
        }
        SearchField::Doi => item.data.doi.as_deref().is_some_and(matches_str),
        SearchField::Other(field_name) => match field_name.as_str() {
            "title" => item.data.title.as_deref().is_some_and(matches_str),
            "doi" => item.data.doi.as_deref().is_some_and(matches_str),
            _ => false,
        },
    }
}

/// Compares two date-or-year strings (`YYYY`, `YYYY-MM`, `YYYY-MM-DD`) by
/// their leading numeric components. Missing components compare as zero.
fn compare_dates(a: &str, b: &str) -> std::cmp::Ordering {
    date_key(a).cmp(&date_key(b))
}

/// Splits `s` into `(year, month, day)` numeric components.
fn date_key(s: &str) -> (u32, u32, u32) {
    let mut parts = s.split('-').filter(|p| !p.is_empty());
    let next = |it: &mut dyn Iterator<Item = &str>| {
        it.next().and_then(|p| p.parse::<u32>().ok()).unwrap_or(0)
    };
    (next(&mut parts), next(&mut parts), next(&mut parts))
}

/// Sorts `items` in place-order by `field` in `direction` and returns them.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "used by MCP search/sort tools in a later task"
    )
)]
fn sort_items(
    items: Vec<ZoteroItem>,
    field: SortField,
    direction: SortDirection,
) -> Vec<ZoteroItem> {
    let mut items = items;
    items.sort_by(|a, b| {
        let ord = sort_key(a, field).cmp(&sort_key(b, field));
        match direction {
            SortDirection::Asc => ord,
            SortDirection::Desc => ord.reverse(),
        }
    });
    items
}

/// Returns the sort key string for `item` under `field`.
fn sort_key(item: &ZoteroItem, field: SortField) -> String {
    match field {
        SortField::Title => item.data.title.clone().unwrap_or_default(),
        SortField::Date => item.data.date.clone().unwrap_or_default(),
        SortField::DateAdded => {
            item.data.date_added.clone().unwrap_or_default()
        }
        SortField::DateModified => {
            item.data.date_modified.clone().unwrap_or_default()
        }
        SortField::Creator => {
            item.data.creators.first().map_or_else(String::new, |c| {
                c.name.clone().unwrap_or_else(|| {
                    format!(
                        "{} {}",
                        c.first_name.as_deref().unwrap_or(""),
                        c.last_name.as_deref().unwrap_or("")
                    )
                })
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zotero::models::{ItemKey, ItemType, ZoteroItemData};

    mod deserialization {
        use pretty_assertions::assert_eq;

        use super::*;
        #[test]
        fn deserializes_search_condition_json() {
            let json = serde_json::json!({
                "field": "title",
                "operator": "contains",
                "value": "Rust"
            });
            let cond: SearchCondition = serde_json::from_value(json).unwrap();
            assert_eq!(cond.field, SearchField::Title);
            assert_eq!(cond.operator, SearchOperator::Contains);
            assert_eq!(cond.value, "Rust");
        }

        #[test]
        fn deserializes_unknown_field_into_other_variant() {
            let json = serde_json::json!({
                "field": "customField",
                "operator": "is",
                "value": "val"
            });
            let cond: SearchCondition = serde_json::from_value(json).unwrap();
            assert_eq!(
                cond.field,
                SearchField::Other("customField".to_owned())
            );
        }
    }

    mod match_condition {

        use super::*;

        fn make_item(
            title: Option<&str>,
            date: Option<&str>,
            doi: Option<&str>,
        ) -> ZoteroItem {
            ZoteroItem {
                key: ItemKey::from("ITEM0001"),
                version: 1,
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("ITEM0001"),
                    version: 1,
                    item_type: ItemType::JournalArticle,
                    title: title.map(ToOwned::to_owned),
                    date: date.map(ToOwned::to_owned),
                    doi: doi.map(ToOwned::to_owned),
                    ..Default::default()
                },
            }
        }

        #[test]
        fn matches_title_field_with_contains_operator_case_insensitively() {
            let item = make_item(Some("Programming in Rust"), None, None);
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::Contains,
                value: "rust".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_date_field_with_is_operator() {
            let item = make_item(None, Some("2024"), None);
            let cond = SearchCondition {
                field: SearchField::Date,
                operator: SearchOperator::Is,
                value: "2024".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_doi_field_with_starts_with_operator() {
            let item = make_item(None, None, Some("10.1000/182"));
            let cond = SearchCondition {
                field: SearchField::Doi,
                operator: SearchOperator::StartsWith,
                value: "10.1000/".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn returns_false_when_condition_value_does_not_match() {
            let item = make_item(Some("Learning Go"), None, None);
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::Contains,
                value: "Rust".to_owned(),
            };
            assert!(!match_condition(&item, &cond));
        }

        #[test]
        fn matches_is_not_operator() {
            let item = make_item(Some("Learning Go"), None, None);
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::IsNot,
                value: "rust".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_does_not_contain_operator() {
            let item = make_item(Some("Learning Go"), None, None);
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::DoesNotContain,
                value: "rust".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_is_greater_than_on_year() {
            let item = make_item(None, Some("2024"), None);
            let cond = SearchCondition {
                field: SearchField::Year,
                operator: SearchOperator::IsGreaterThan,
                value: "2020".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_is_less_than_on_date() {
            let item = make_item(None, Some("2024-02-15"), None);
            let cond = SearchCondition {
                field: SearchField::Date,
                operator: SearchOperator::IsLessThan,
                value: "2025-01-01".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_is_after_on_date() {
            let item = make_item(None, Some("2024-02-15"), None);
            let cond = SearchCondition {
                field: SearchField::Date,
                operator: SearchOperator::IsAfter,
                value: "2024-01-01".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }
    }

    mod sort_items {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::zotero::models::{ItemKey, ItemType, ZoteroItemData};

        fn item(key: &str, title: &str, date: &str) -> ZoteroItem {
            ZoteroItem {
                key: ItemKey::from(key),
                version: 1,
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from(key),
                    version: 1,
                    item_type: ItemType::JournalArticle,
                    title: Some(title.to_owned()),
                    date: Some(date.to_owned()),
                    ..Default::default()
                },
            }
        }

        #[test]
        fn sorts_by_title_ascending() {
            let items = vec![
                item("K3", "Zeta", "2024"),
                item("K1", "Alpha", "2024"),
                item("K2", "Beta", "2024"),
            ];
            let sorted =
                sort_items(items, SortField::Title, SortDirection::Asc);
            let titles: Vec<&str> = sorted
                .iter()
                .map(|i| i.data.title.as_deref().unwrap_or_default())
                .collect();
            assert_eq!(titles, vec!["Alpha", "Beta", "Zeta"]);
        }

        #[test]
        fn sorts_by_date_descending() {
            let items = vec![
                item("K1", "A", "2022"),
                item("K2", "B", "2025"),
                item("K3", "C", "2023"),
            ];
            let sorted =
                sort_items(items, SortField::Date, SortDirection::Desc);
            let keys: Vec<&str> =
                sorted.iter().map(|i| i.key.as_str()).collect();
            assert_eq!(keys, vec!["K2", "K3", "K1"]);
        }
    }
}
