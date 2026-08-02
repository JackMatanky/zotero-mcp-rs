//! Search and query operations for the Zotero Local HTTP API.
//!
//! Adds [`ZoteroClient`] methods for free-text search, tag search, citation key
//! lookup, and structured multi-condition search.
//!
//! # Key types and operations
//!
//! - [`ZoteroClient::search_items`]: free-text search over title, creator,
//!   year, or fulltext.
//! - [`ZoteroClient::search_by_citation_key`]: lookup by native Zotero citation
//!   key or legacy Better `BibTeX` metadata.
//! - [`ZoteroClient::advanced_search`]: structured search with
//!   [`SearchCondition`], [`SearchField`], and [`SearchOperator`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        CitationKey, CollectionKey, ItemType, TagName, ZoteroItem,
        client::ZoteroClient, objects::ZoteroCreator,
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

/// Pagination metadata returned with every search result page.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) struct PaginationInfo {
    pub(crate) limit: usize,
    pub(crate) offset: usize,
    pub(crate) total: usize,
    pub(crate) has_more: bool,
}

/// A page of search results plus its pagination metadata.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) struct SearchPage<T> {
    pub(crate) items: Vec<T>,
    pub(crate) pagination: PaginationInfo,
}

impl ZoteroClient<'_> {
    /// Searches library items matching `query`, excluding notes, and returns a
    /// paginated page.
    ///
    /// # Arguments
    ///
    /// * `query` - Free-text query matching title, creator, year, or fulltext
    /// * `collection_key` - Optional collection key to scope the search
    /// * `offset` - 0-based offset into the full result set
    /// * `limit` - Maximum number of items to return
    ///
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
        offset: usize,
        limit: usize,
    ) -> Result<SearchPage<ZoteroItem>, ZoteroMcpError> {
        let base = match collection_key {
            Some(col) => format!(
                "{}/users/0/collections/{}/items",
                self.state.zotero_api_url, col
            ),
            None => format!("{}/users/0/items", self.state.zotero_api_url),
        };
        let encoded_q = urlencoding::encode(query);
        let url = format!(
            "{base}?q={encoded_q}&start={offset}&limit={limit}&itemType=-note"
        );
        let page = self.get_items_with_total(&url).await?;
        Ok(finish_page(page.items, page.total, offset, limit))
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

    /// Searches items by native or legacy citation key.
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
        let page = self.search_items(citekey.as_str(), None, 0, 20).await?;
        let citekey_lc = citekey.as_str().to_lowercase();
        for item in page.items {
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
    /// Returns a paginated page. When `join_mode` is `All` and every condition
    /// is expressible as a Zotero quick-search parameter, the search is pushed
    /// down to the server; otherwise the whole library is scanned and filtered
    /// client-side.
    ///
    /// # Arguments
    ///
    /// * `conditions` - List of conditions to match against item fields
    /// * `join_mode` - `All` (AND) or `Any` (OR)
    /// * `sort` - Optional field to sort results by
    /// * `sort_direction` - Sort order for `sort`
    /// * `offset` - 0-based offset into the full result set
    /// * `limit` - Maximum number of items to return
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    #[allow(
        clippy::too_many_arguments,
        reason = "six orthogonal search parameters; a params struct adds \
                  indirection without removing them"
    )]
    pub(crate) async fn advanced_search(
        &self,
        conditions: Vec<SearchCondition>,
        join_mode: JoinMode,
        sort: Option<SortField>,
        sort_direction: SortDirection,
        offset: usize,
        limit: usize,
    ) -> Result<SearchPage<ZoteroItem>, ZoteroMcpError> {
        if join_mode == JoinMode::All && sort.is_none() {
            if let Some(url) = self.pushdown_url(&conditions) {
                let full_url = format!("{url}&start={offset}&limit={limit}");
                let page = self.get_items_with_total(&full_url).await?;
                return Ok(finish_page(page.items, page.total, offset, limit));
            }
        }

        let items = self.get_all_items().await?;
        let prepared: Vec<_> =
            conditions.iter().map(PreparedCondition::from).collect();
        if let Some(field) = sort {
            let matches: Vec<ZoteroItem> = items
                .into_iter()
                .filter(|item| {
                    is_searchable_item(item)
                        && item_matches_conditions(item, &prepared, join_mode)
                })
                .collect();
            return Ok(paginate(
                sort_items(matches, field, sort_direction),
                offset,
                limit,
            ));
        }

        let mut page = PageAccumulator::new(offset, limit);
        for item in items {
            if is_searchable_item(&item)
                && item_matches_conditions(&item, &prepared, join_mode)
            {
                page.push_match(item);
            }
        }
        Ok(page.into_page())
    }

    /// Builds a server-search URL for `conditions` when they are fully
    /// expressible as Zotero quick-search parameters, or `None` to fall back
    /// to a client-side scan.
    ///
    /// The emitted URL always carries a single merged `itemType` parameter
    /// excluding notes, attachments, and annotations (mirroring
    /// [`is_searchable_item`]), with any positive item-type condition merged
    /// into the same parameter.
    fn pushdown_url(&self, conditions: &[SearchCondition]) -> Option<String> {
        if conditions.is_empty() {
            return None;
        }
        let mut q: Option<&str> = None;
        let mut qmode = "titleCreatorYear";
        let mut item_type: Option<&str> = None;
        let mut tag: Option<&str> = None;

        for cond in conditions {
            let value = cond.value.as_str();
            let operator_pushable = matches!(
                cond.operator,
                SearchOperator::Contains
                    | SearchOperator::Is
                    | SearchOperator::StartsWith
            );
            if !operator_pushable {
                return None;
            }
            match &cond.field {
                SearchField::Creator
                | SearchField::Year
                | SearchField::Date => {
                    if q.is_some() {
                        return None; // only one free-text term
                    }
                    q = Some(value);
                    qmode = if cond.field == SearchField::Creator {
                        "creator"
                    } else {
                        "year"
                    };
                }
                SearchField::ItemType
                    if cond.operator == SearchOperator::Is =>
                {
                    if item_type.is_some() {
                        return None;
                    }
                    item_type = Some(value);
                }
                SearchField::Tag if cond.operator == SearchOperator::Is => {
                    if tag.is_some() {
                        return None;
                    }
                    tag = Some(value);
                }
                _ => return None,
            }
        }

        let mut url = format!("{}/users/0/items", self.state.zotero_api_url);
        let mut params = Vec::new();
        if let Some(q) = q {
            params.push(format!("q={}", urlencoding::encode(q)));
            params.push(format!("qmode={qmode}"));
        }
        if let Some(item_type) = item_type {
            params.push(format!(
                "itemType={item_type},-note,-attachment,-annotation"
            ));
        } else {
            // exclusion only; merged into the same param so the fast path
            // cannot append a second itemType key
            params.push("itemType=-note,-attachment,-annotation".to_owned());
        }
        if let Some(tag) = tag {
            params.push(format!("tag={}", urlencoding::encode(tag)));
        }
        url.push('?');
        url.push_str(&params.join("&"));
        Some(url)
    }
}

/// Search condition prepared once for client-side scans.
struct PreparedCondition<'a> {
    field: &'a SearchField,
    operator: &'a SearchOperator,
    value: &'a str,
    value_lc: String,
}

impl<'a> From<&'a SearchCondition> for PreparedCondition<'a> {
    fn from(cond: &'a SearchCondition) -> Self {
        Self {
            field: &cond.field,
            operator: &cond.operator,
            value: cond.value.as_str(),
            value_lc: cond.value.to_lowercase(),
        }
    }
}

impl PreparedCondition<'_> {
    fn matches_str(&self, s: &str) -> bool {
        match self.operator {
            SearchOperator::Is => s.to_lowercase() == self.value_lc,
            SearchOperator::IsNot => s.to_lowercase() != self.value_lc,
            SearchOperator::StartsWith => {
                s.to_lowercase().starts_with(&self.value_lc)
            }
            SearchOperator::EndsWith => {
                s.to_lowercase().ends_with(&self.value_lc)
            }
            SearchOperator::DoesNotContain => {
                !s.to_lowercase().contains(&self.value_lc)
            }
            SearchOperator::Contains | SearchOperator::Other(_) => {
                s.to_lowercase().contains(&self.value_lc)
            }
            SearchOperator::IsGreaterThan | SearchOperator::IsAfter => {
                compare_dates(s, self.value).is_gt()
            }
            SearchOperator::IsLessThan | SearchOperator::IsBefore => {
                compare_dates(s, self.value).is_lt()
            }
        }
    }

    fn matches_item(&self, item: &ZoteroItem) -> bool {
        match self.field {
            SearchField::Title => {
                item.data.title.as_deref().is_some_and(|s| self.matches_str(s))
            }
            SearchField::Creator => item.data.creators.iter().any(|c| {
                c.name.as_deref().is_some_and(|s| self.matches_str(s))
                    || c.first_name
                        .as_deref()
                        .is_some_and(|s| self.matches_str(s))
                    || c.last_name
                        .as_deref()
                        .is_some_and(|s| self.matches_str(s))
                    || matches_creator_full_name(c, self)
            }),
            SearchField::Date => {
                item.data.date.as_deref().is_some_and(|s| self.matches_str(s))
            }
            SearchField::Year => item.data.date.as_deref().is_some_and(|d| {
                self.matches_str(d.split('-').next().unwrap_or(d))
            }),
            SearchField::ItemType => {
                self.matches_str(item.data.item_type.as_str())
            }
            SearchField::Tag => {
                item.data.tags.iter().any(|t| self.matches_str(t.tag.as_str()))
            }
            SearchField::Extra => {
                item.data.extra.as_deref().is_some_and(|s| self.matches_str(s))
            }
            SearchField::Doi => {
                item.data.doi.as_deref().is_some_and(|s| self.matches_str(s))
            }
            SearchField::Other(field_name) => match field_name.as_str() {
                "title" => item
                    .data
                    .title
                    .as_deref()
                    .is_some_and(|s| self.matches_str(s)),
                "doi" => item
                    .data
                    .doi
                    .as_deref()
                    .is_some_and(|s| self.matches_str(s)),
                _ => false,
            },
        }
    }
}

/// Accumulates only the requested page while still counting all matches.
struct PageAccumulator<T> {
    offset: usize,
    limit: usize,
    total: usize,
    items: Vec<T>,
}

impl<T> PageAccumulator<T> {
    fn new(offset: usize, limit: usize) -> Self {
        Self {
            offset,
            limit,
            total: 0,
            items: Vec::with_capacity(limit),
        }
    }

    fn push_match(&mut self, item: T) {
        if self.total >= self.offset && self.items.len() < self.limit {
            self.items.push(item);
        }
        self.total = self.total.saturating_add(1);
    }

    fn into_page(self) -> SearchPage<T> {
        let offset = self.offset.min(self.total);
        let returned = self.items.len();
        SearchPage {
            items: self.items,
            pagination: PaginationInfo {
                limit: self.limit,
                offset,
                total: self.total,
                has_more: offset.saturating_add(returned) < self.total,
            },
        }
    }
}

/// Returns a `{items, pagination}` page slicing `results` at `offset`/`limit`.
fn paginate<T>(results: Vec<T>, offset: usize, limit: usize) -> SearchPage<T> {
    let total = results.len();
    let skip = offset.min(total);
    let items: Vec<T> = results.into_iter().skip(skip).take(limit).collect();
    SearchPage {
        items,
        pagination: PaginationInfo {
            limit,
            offset: skip,
            total,
            has_more: skip.saturating_add(limit) < total,
        },
    }
}

/// Wraps a server-fetched page, falling back to `offset + items.len()` when
/// the server reports no total.
fn finish_page(
    items: Vec<ZoteroItem>,
    server_total: Option<usize>,
    offset: usize,
    limit: usize,
) -> SearchPage<ZoteroItem> {
    let returned = items.len();
    let total = server_total.unwrap_or_else(|| offset.saturating_add(returned));
    let has_more = server_total.map_or(returned == limit, |exact| {
        offset.saturating_add(returned) < exact
    });

    SearchPage {
        items,
        pagination: PaginationInfo {
            limit,
            offset,
            total,
            has_more,
        },
    }
}

/// Returns true for items that are not attachments, notes, or annotations.
fn is_searchable_item(item: &ZoteroItem) -> bool {
    !matches!(
        item.data.item_type,
        ItemType::Attachment | ItemType::Note | ItemType::Annotation
    )
}

fn matches_creator_full_name(
    creator: &ZoteroCreator,
    cond: &PreparedCondition<'_>,
) -> bool {
    let (Some(first), Some(last)) =
        (creator.first_name.as_deref(), creator.last_name.as_deref())
    else {
        return false;
    };
    let mut full = String::with_capacity(
        first.len().saturating_add(1).saturating_add(last.len()),
    );
    full.push_str(first);
    full.push(' ');
    full.push_str(last);
    cond.matches_str(&full)
}

fn item_matches_conditions(
    item: &ZoteroItem,
    conditions: &[PreparedCondition<'_>],
    join_mode: JoinMode,
) -> bool {
    match join_mode {
        JoinMode::All => conditions.iter().all(|cond| cond.matches_item(item)),
        JoinMode::Any => conditions.iter().any(|cond| cond.matches_item(item)),
    }
}

#[cfg(test)]
/// Evaluates whether `item` satisfies a single search `cond`.
fn match_condition(item: &ZoteroItem, cond: &SearchCondition) -> bool {
    PreparedCondition::from(cond).matches_item(item)
}

/// Compares two date-or-year strings (`YYYY`, `YYYY-MM`, `YYYY-MM-DD`) by
/// their leading numeric components. Missing components compare as zero.
fn compare_dates(a: &str, b: &str) -> std::cmp::Ordering {
    date_key(a).cmp(&date_key(b))
}

/// Splits `s` into `(year, month, day)` numeric components.
fn date_key(s: &str) -> (u32, u32, u32) {
    let mut parts = s.split('-').filter(|p| !p.is_empty());
    (
        next_date_part(&mut parts),
        next_date_part(&mut parts),
        next_date_part(&mut parts),
    )
}

/// Parses the next `-`-separated component of a date string as `u32`, or zero
/// when absent or non-numeric.
fn next_date_part<'a>(parts: &mut impl Iterator<Item = &'a str>) -> u32 {
    parts.next().and_then(|p| p.parse::<u32>().ok()).unwrap_or(0)
}

/// Sorts `items` by `field` in `direction` and returns the sorted items.
fn sort_items(
    items: Vec<ZoteroItem>,
    field: SortField,
    direction: SortDirection,
) -> Vec<ZoteroItem> {
    let mut keyed: Vec<(String, ZoteroItem)> = items
        .into_iter()
        .map(|item| {
            let key = sort_key(&item, field);
            (key, item)
        })
        .collect();
    match direction {
        SortDirection::Asc => keyed.sort_by(|a, b| a.0.cmp(&b.0)),
        SortDirection::Desc => keyed.sort_by(|a, b| b.0.cmp(&a.0)),
    }
    keyed.into_iter().map(|(_, item)| item).collect()
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
    use crate::zotero::{
        ItemKey, ItemType, LibraryVersion, objects::ZoteroItemData,
    };

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
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("ITEM0001"),
                    version: LibraryVersion(1),
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
        fn is_not_returns_false_when_value_matches() {
            let item = make_item(Some("Learning Go"), None, None);
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::IsNot,
                value: "learning go".to_owned(),
            };
            assert!(!match_condition(&item, &cond));
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
        use crate::zotero::{ItemKey, ItemType, objects::ZoteroItemData};

        fn item(key: &str, title: &str, date: &str) -> ZoteroItem {
            ZoteroItem {
                key: ItemKey::from(key),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from(key),
                    version: LibraryVersion(1),
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

    mod advanced_search {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::{state::AppState, zotero::client::ZoteroClient};

        fn items_page(items: &[serde_json::Value]) -> String {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }

        fn zotero_item(
            key: &str,
            title: &str,
            extra: Option<&str>,
        ) -> serde_json::Value {
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": "journalArticle",
                    "title": title,
                    "extra": extra,
                    "dateAdded": "2024-01-01T00:00:00Z",
                    "dateModified": "2024-01-01T00:00:00Z",
                },
            })
        }

        fn zotero_item_with_creator(
            key: &str,
            title: &str,
            creator: &str,
        ) -> serde_json::Value {
            let mut item = zotero_item(key, title, None);
            item["data"]["creators"] = serde_json::json!([{ "name": creator }]);
            item
        }

        fn title_contains(value: &str) -> SearchCondition {
            SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::Contains,
                value: value.to_owned(),
            }
        }

        fn creator_contains(value: &str) -> SearchCondition {
            SearchCondition {
                field: SearchField::Creator,
                operator: SearchOperator::Contains,
                value: value.to_owned(),
            }
        }

        fn zotero_state(zotero_api_url: String) -> AppState {
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

        fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        fn http_response_with_headers(
            status: &str,
            headers: &[(&str, &str)],
            body: &str,
        ) -> String {
            let hdrs = headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}\r\n"))
                .collect::<Vec<_>>()
                .join("");
            format!(
                "HTTP/1.1 {status}\r\n{hdrs}Content-Length: \
                 {}\r\nContent-Type: application/json\r\nConnection: \
                 close\r\n\r\n{body}",
                body.len()
            )
        }

        fn mock_server(responses: Vec<String>) -> String {
            use std::{
                io::{Read, Write},
                net::TcpListener,
            };
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

        fn mock_server_capturing(
            responses: Vec<String>,
        ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
            use std::{
                io::{BufRead, Write},
                net::TcpListener,
                sync::{Arc, Mutex},
            };
            let requests = Arc::new(Mutex::new(Vec::new()));
            let captured = Arc::clone(&requests);
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut line = String::new();
                    let mut reader = std::io::BufReader::new(&mut stream);
                    let _ = reader.read_line(&mut line);
                    drop(reader);
                    captured.lock().expect("lock requests").push(line);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            (format!("http://{addr}"), requests)
        }

        #[tokio::test]
        async fn slow_path_filters_full_library_and_paginates() {
            let item1 = zotero_item("K1", "Rust in Action", Some("book"));
            let item2 = zotero_item("K2", "Rust for Beginners", Some("book"));
            let item3 = zotero_item("K3", "Rust Essentials", Some("talk"));
            let base = mock_server(vec![http_response(
                "200 OK",
                &items_page(&[item1, item2, item3]),
            )]);
            let state = zotero_state(base);

            let cond_extra = SearchCondition {
                field: SearchField::Extra,
                operator: SearchOperator::Is,
                value: "book".to_owned(),
            };
            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust"), cond_extra],
                    JoinMode::All,
                    Some(SortField::Title),
                    SortDirection::Asc,
                    0,
                    1,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].key.as_str(), "K2");
            assert_eq!(page.pagination.limit, 1);
            assert_eq!(page.pagination.offset, 0);
            assert_eq!(page.pagination.total, 2);
            assert!(page.pagination.has_more);
        }

        #[tokio::test]
        async fn sort_request_uses_slow_path_so_sort_is_applied() {
            let item1 = zotero_item("K1", "A Rust Book", None);
            let item2 = zotero_item("K2", "Z Rust Book", None);
            let (base, requests) = mock_server_capturing(vec![http_response(
                "200 OK",
                &items_page(&[item1, item2]),
            )]);
            let state = zotero_state(base);

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust")],
                    JoinMode::All,
                    Some(SortField::Title),
                    SortDirection::Desc,
                    0,
                    10,
                )
                .await
                .unwrap();

            let titles: Vec<&str> = page
                .items
                .iter()
                .map(|item| item.data.title.as_deref().unwrap_or_default())
                .collect();
            assert_eq!(titles, vec!["Z Rust Book", "A Rust Book"]);
            let requests = requests.lock().expect("lock requests");
            let request_line = requests.first().expect("one request captured");
            assert!(
                !request_line.contains("qmode=titleCreatorYear"),
                "request: {request_line}"
            );
        }

        #[tokio::test]
        async fn strict_title_search_uses_slow_path_to_avoid_title_creator_year_broadening()
         {
            let item1 = zotero_item("K1", "Rust Patterns", None);
            let item2 =
                zotero_item_with_creator("K2", "Memory Safety", "Rustacean");
            let base = mock_server(vec![http_response(
                "200 OK",
                &items_page(&[item1, item2]),
            )]);
            let state = zotero_state(base);

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust")],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    10,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 1);
            assert_eq!(
                page.items[0].data.title.as_deref(),
                Some("Rust Patterns")
            );
        }

        #[tokio::test]
        async fn fast_path_uses_server_side_search_and_total_header() {
            let item1 = zotero_item("K1", "Rust in Action", None);
            let base = mock_server(vec![http_response_with_headers(
                "200 OK",
                &[("Total-Results", "17")],
                &items_page(&[item1]),
            )]);
            let state = zotero_state(base);

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![creator_contains("Ferris")],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    10,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 1);
            assert_eq!(page.pagination.total, 17);
            assert_eq!(page.pagination.offset, 0);
            assert!(page.pagination.has_more);
        }

        #[tokio::test]
        async fn fast_path_without_total_marks_has_more_when_page_is_full() {
            let item1 = zotero_item("K1", "Rust in Action", None);
            let base = mock_server(vec![http_response(
                "200 OK",
                &items_page(&[item1]),
            )]);
            let state = zotero_state(base);
            let cond_type = SearchCondition {
                field: SearchField::ItemType,
                operator: SearchOperator::Is,
                value: "journalArticle".to_owned(),
            };

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![cond_type],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    1,
                )
                .await
                .unwrap();

            assert_eq!(page.pagination.total, 1);
            assert!(page.pagination.has_more);
        }

        #[tokio::test]
        async fn fast_path_without_total_marks_done_when_page_is_short() {
            let item1 = zotero_item("K1", "Rust in Action", None);
            let base = mock_server(vec![http_response(
                "200 OK",
                &items_page(&[item1]),
            )]);
            let state = zotero_state(base);
            let cond_type = SearchCondition {
                field: SearchField::ItemType,
                operator: SearchOperator::Is,
                value: "journalArticle".to_owned(),
            };

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![cond_type],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    2,
                )
                .await
                .unwrap();

            assert_eq!(page.pagination.total, 1);
            assert!(!page.pagination.has_more);
        }

        #[tokio::test]
        async fn creator_fast_path_builds_single_merged_itemtype_param() {
            let item1 = zotero_item("K1", "Rust in Action", None);
            let (base, requests) =
                mock_server_capturing(vec![http_response_with_headers(
                    "200 OK",
                    &[("Total-Results", "1")],
                    &items_page(&[item1]),
                )]);
            let state = zotero_state(base);

            ZoteroClient::new(&state)
                .advanced_search(
                    vec![creator_contains("Ferris")],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    10,
                )
                .await
                .unwrap();

            let requests = requests.lock().expect("lock requests");
            let request_line = requests.first().expect("one request captured");
            assert!(
                request_line.contains("qmode=creator"),
                "request: {request_line}"
            );
            assert!(
                request_line.contains("itemType=-note,-attachment,-annotation"),
                "request: {request_line}"
            );
            assert_eq!(
                request_line.matches("itemType=").count(),
                1,
                "request must carry a single itemType key: {request_line}"
            );
        }

        #[tokio::test]
        async fn fast_path_merges_positive_itemtype_with_exclusions() {
            let item1 = zotero_item("K1", "Rust in Action", None);
            let (base, requests) =
                mock_server_capturing(vec![http_response_with_headers(
                    "200 OK",
                    &[("Total-Results", "1")],
                    &items_page(&[item1]),
                )]);
            let state = zotero_state(base);

            let cond_type = SearchCondition {
                field: SearchField::ItemType,
                operator: SearchOperator::Is,
                value: "journalArticle".to_owned(),
            };
            ZoteroClient::new(&state)
                .advanced_search(
                    vec![cond_type],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    10,
                )
                .await
                .unwrap();

            let requests = requests.lock().expect("lock requests");
            let request_line = requests.first().expect("one request captured");
            assert_eq!(
                request_line.matches("itemType=").count(),
                1,
                "request must carry a single itemType key: {request_line}"
            );
            assert!(
                request_line.contains(
                    "itemType=journalArticle,-note,-attachment,-annotation"
                ),
                "request: {request_line}"
            );
        }

        #[test]
        fn pushdown_url_encodes_free_text_creator() {
            let state = zotero_state("http://127.0.0.1:23119/api".to_owned());
            let client = ZoteroClient::new(&state);
            let url = client
                .pushdown_url(&[creator_contains("Ferris Crab")])
                .unwrap();
            assert!(url.contains("q=Ferris%20Crab"));
            assert!(url.contains("qmode=creator"));
        }

        #[test]
        fn pushdown_url_refuses_title() {
            let state = zotero_state("http://127.0.0.1:23119/api".to_owned());
            let client = ZoteroClient::new(&state);
            assert!(client.pushdown_url(&[title_contains("Rust")]).is_none());
        }

        #[test]
        fn pushdown_url_refuses_non_pushable_operator() {
            let state = zotero_state("http://127.0.0.1:23119/api".to_owned());
            let client = ZoteroClient::new(&state);
            let cond = SearchCondition {
                field: SearchField::Creator,
                operator: SearchOperator::DoesNotContain,
                value: "Rust".to_owned(),
            };
            assert!(client.pushdown_url(&[cond]).is_none());
        }

        #[test]
        fn pushdown_url_encodes_item_type_and_tag() {
            let state = zotero_state("http://127.0.0.1:23119/api".to_owned());
            let client = ZoteroClient::new(&state);
            let conds = vec![
                SearchCondition {
                    field: SearchField::ItemType,
                    operator: SearchOperator::Is,
                    value: "conferencePaper".to_owned(),
                },
                SearchCondition {
                    field: SearchField::Tag,
                    operator: SearchOperator::Is,
                    value: "methods".to_owned(),
                },
            ];
            let url = client.pushdown_url(&conds).unwrap();
            assert!(url.contains("itemType=conferencePaper"));
            assert!(url.contains("-note,-attachment,-annotation"));
            assert!(url.contains("tag=methods"));
        }
    }
}
