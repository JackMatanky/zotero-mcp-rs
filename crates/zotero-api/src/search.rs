//! Search and query operations for the Zotero Local HTTP API.
//!
//! Provides [`ZoteroClient`] methods for free-text item search, tag searching,
//! citation key resolution, and multi-condition structured queries against
//! Zotero's local HTTP API endpoints.
//!
//! # Key Types and Operations
//!
//! - [`ZoteroClient::search_items`]: Free-text search over title, creator,
//!   year, or fulltext.
//! - [`ZoteroClient::search_by_tag`]: Filter library items matching a specific
//!   [`TagName`].
//! - [`ZoteroClient::search_by_citation_key`]: Look up an item by citation key.
//! - [`ZoteroClient::advanced_search`]: Multi-condition search using
//!   [`SearchCondition`], [`SearchField`], [`SearchOperator`], and
//!   [`JoinMode`].
//! - [`SearchPage`]: Paginated container holding returned items and
//!   [`PaginationInfo`].
//! # Examples
//!
//! Performing a free-text item search using [`ZoteroClient`]:
//!
//! ```no_run
//! # use zotero_api::errors::ZoteroApiError;
//! # use zotero_api::ZoteroClient;
//! # async fn run(
//! #     client: &ZoteroClient<'_>,
//! # ) -> Result<(), ZoteroApiError> {
//! let page = client.search_items("quantum mechanics", None, 0, 10).await?;
//! println!("Found {} items", page.pagination.total);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};

use crate::{
    client::{ZoteroClient, add_pagination},
    errors::ZoteroApiError,
    keys::{CitationKey, CollectionKey, ItemKey, TagName},
    objects::{ZoteroCreator, ZoteroItem},
    types::ItemType,
};

/// Searchable item field in structured searches.
///
/// Defines the specific metadata field targeted by a [`SearchCondition`].
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchField {
    /// Item title metadata.
    Title,
    /// Creator name (first, last, or single name).
    Creator,
    /// Full date string.
    Date,
    /// Publication year component extracted from date metadata.
    Year,
    /// Zotero item type (e.g. `journalArticle`, `book`).
    ItemType,
    /// Tag attached to the item.
    Tag,
    /// Miscellaneous extra metadata field.
    Extra,
    /// Digital Object Identifier (DOI).
    Doi,
    /// Custom or unrecognized field name.
    #[serde(untagged)]
    Other(String),
}

/// Comparison operator in structured searches.
///
/// Specifies how a [`SearchCondition`]'s target field is evaluated against its
/// query value.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchOperator {
    /// Substring match (case-insensitive).
    #[default]
    Contains,
    /// Exact match (case-insensitive).
    Is,
    /// Prefix match (case-insensitive).
    StartsWith,
    /// Suffix match (case-insensitive).
    EndsWith,
    /// Inequality match.
    IsNot,
    /// Negative substring match.
    DoesNotContain,
    /// Greater-than comparison for numeric or date values.
    IsGreaterThan,
    /// Less-than comparison for numeric or date values.
    IsLessThan,
    /// Date comparison matching dates prior to target value.
    IsBefore,
    /// Date comparison matching dates after target value.
    IsAfter,
    /// Custom or unrecognized operator string.
    #[serde(untagged)]
    Other(String),
}

/// Structured search condition matching a specific item field.
///
/// Combines a target [`SearchField`], a comparison [`SearchOperator`], and a
/// search string value.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchCondition {
    /// Target metadata field to evaluate.
    pub field: SearchField,
    /// Operator used to evaluate the field against `value`.
    #[serde(default)]
    pub operator: SearchOperator,
    /// Query string value to match against.
    pub value: String,
}

/// Logical combination mode for multiple search conditions.
///
/// Determines whether all conditions must match ([`JoinMode::All`], logical
/// AND) or any single condition can match ([`JoinMode::Any`], logical OR).
#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub enum JoinMode {
    /// Logical AND: all conditions must evaluate to true.
    #[default]
    All,
    /// Logical OR: at least one condition must evaluate to true.
    Any,
}

/// Item field used to order search results.
///
/// Specifies which metadata property determines the relative order of returned
/// items.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SortField {
    /// Creation timestamp of the item in the library.
    DateAdded,
    /// Modification timestamp of the item.
    DateModified,
    /// Primary title string.
    Title,
    /// Publication date.
    Date,
    /// First creator's name.
    Creator,
}

/// Direction for ordering search results.
#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    /// Ascending order (A to Z, oldest to newest).
    #[default]
    Asc,
    /// Descending order (Z to A, newest to oldest).
    Desc,
}

/// Pagination metadata returned alongside search result pages.
///
/// Contains offset, limit, total match count, and a flag indicating whether
/// additional pages remain.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaginationInfo {
    /// Maximum number of items requested for this page.
    pub limit: usize,
    /// 0-based offset into the total result set.
    pub offset: usize,
    /// Total number of items matching the search criteria across all pages.
    pub total: usize,
    /// Indicates whether additional matching items exist past this page
    /// (`offset + limit < total`).
    pub has_more: bool,
}

/// Paginated result container wrapping items and pagination metadata.
///
/// Pairs a collection of items of type `T` with associated [`PaginationInfo`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchPage<T> {
    /// Items included in this page.
    pub items: Vec<T>,
    /// Associated pagination metadata.
    pub pagination: PaginationInfo,
}

impl ZoteroClient<'_> {
    /// Searches library items matching `query`, excluding notes, and returns a
    /// paginated page.
    ///
    /// Executes a free-text search across item title, creator, year, and
    /// fulltext content.
    ///
    /// Issues `GET <prefix>/items?q=<query>&start=<offset>&limit=<limit>` (or
    /// `<prefix>/collections/<key>/items` if `collection_key` is provided).
    /// Returns a [`SearchPage`] containing matched items and total hit counts.
    ///
    /// # Arguments
    ///
    /// * `query` - Free-text search string.
    /// * `collection_key` - Optional collection key to scope the search.
    /// * `offset` - 0-based result offset for pagination.
    /// * `limit` - Maximum number of items to return.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero responds with a non-2xx HTTP
    ///   status.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if response payload decoding fails.
    #[inline]
    pub async fn search_items(
        &self,
        query: &str,
        collection_key: Option<&CollectionKey>,
        offset: usize,
        limit: usize,
    ) -> Result<SearchPage<ZoteroItem>, ZoteroApiError> {
        let base = match collection_key {
            Some(col) => format!(
                "{}{}/collections/{}/items",
                self.state.zotero_api_url(),
                self.target_prefix(),
                col
            ),
            None => format!(
                "{}{}/items",
                self.state.zotero_api_url(),
                self.target_prefix()
            ),
        };
        let encoded_q = urlencoding::encode(query);
        let url = format!(
            "{base}?q={encoded_q}&start={offset}&limit={limit}&itemType=-note"
        );
        let page = self.get_items_with_total(&url).await?;
        Ok(finish_page(page.items, page.total, offset, limit))
    }

    /// Searches items tagged with `tag`, returning up to `limit` items
    /// (excluding notes).
    ///
    /// Queries `GET <prefix>/items?tag=<tag>&limit=<limit>&itemType=-note`.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag name to match.
    /// * `limit` - Maximum number of items to return.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if JSON decoding fails.
    #[inline]
    pub async fn search_by_tag(
        &self,
        tag: &TagName,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroApiError> {
        let encoded_tag = urlencoding::encode(tag.as_str());
        let url = format!(
            "{}{}/items?tag={}&limit={}&itemType=-note",
            self.state.zotero_api_url(),
            self.target_prefix(),
            encoded_tag,
            limit
        );
        self.get_json(&url).await
    }

    /// Searches items by native `citationKey` or legacy `extra` field citation
    /// key.
    ///
    /// Performs a initial quick-search for `citekey`, then inspects candidate
    /// items for a matching native `citationKey` property or a legacy
    /// `Citation Key: ...` string in the `extra` field.
    /// Returns `Some(ZoteroItem)` if found, or `None` if no item matches.
    ///
    /// # Arguments
    ///
    /// * `citekey` - Citation key identifier (e.g. `Smith2024`).
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if response decoding fails.
    #[inline]
    pub async fn search_by_citation_key(
        &self,
        citekey: &CitationKey,
    ) -> Result<Option<ZoteroItem>, ZoteroApiError> {
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
    /// If `join_mode` is [`JoinMode::All`] and all conditions can be converted
    /// into Zotero server query parameters, the search is pushed down
    /// directly to Zotero's Local API server. Otherwise, the library
    /// is fetched and filtered client-side with evaluation of all conditions.
    ///
    /// # Arguments
    ///
    /// * `conditions` - Vector of [`SearchCondition`] filters (field, operator,
    ///   value).
    /// * `join_mode` - [`JoinMode::All`] (AND) or [`JoinMode::Any`] (OR).
    /// * `sort` - Optional field to sort results by.
    /// * `sort_direction` - [`SortOrder::Asc`] or [`SortOrder::Desc`].
    /// * `offset` - 0-based result offset for pagination.
    /// * `limit` - Maximum number of items to return.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if response decoding fails.
    #[expect(
        clippy::too_many_arguments,
        reason = "six orthogonal search parameters; a params struct adds \
                  indirection without removing them"
    )]
    #[inline]
    pub async fn advanced_search(
        &self,
        conditions: Vec<SearchCondition>,
        join_mode: JoinMode,
        sort: Option<SortField>,
        sort_direction: SortOrder,
        offset: usize,
        limit: usize,
    ) -> Result<SearchPage<ZoteroItem>, ZoteroApiError> {
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
    /// expressible as Zotero quick-search parameters, or `None` to fall back to
    /// a client-side scan.
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

        let mut url = format!(
            "{}{}/items",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
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
            // Exclusion only; merged into the same parameter so the fast path
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
    item.data.item_type.is_indexable()
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
    direction: SortOrder,
) -> Vec<ZoteroItem> {
    let mut keyed: Vec<(String, ZoteroItem)> = items
        .into_iter()
        .map(|item| {
            let key = sort_key(&item, field);
            (key, item)
        })
        .collect();
    match direction {
        SortOrder::Asc => keyed.sort_by(|a, b| a.0.cmp(&b.0)),
        SortOrder::Desc => keyed.sort_by(|a, b| b.0.cmp(&a.0)),
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

/// Coverage indicators for a single library item.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "domain model tracks 3 distinct boolean flags"
)]
pub(crate) struct ItemCoverageFlags {
    /// Whether the item has at least one PDF attachment.
    pub(crate) has_pdf: bool,
    /// Whether the item has a nonempty DOI field.
    pub(crate) has_doi: bool,
    /// Whether the item has at least one child note.
    pub(crate) has_notes: bool,
}

/// Aggregate PDF, DOI, and note coverage statistics for a library or
/// collection.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LibraryCoverage {
    /// Total number of top-level items evaluated.
    pub(crate) total_items: usize,
    /// Number of items with at least one PDF attachment.
    pub(crate) with_pdf: usize,
    /// Number of items with a nonempty DOI field.
    pub(crate) with_doi: usize,
    /// Number of items with at least one child note.
    pub(crate) with_notes: usize,
    /// Percentage of items with at least one PDF attachment.
    pub(crate) pdf_percentage: f64,
    /// Percentage of items with a nonempty DOI field.
    pub(crate) doi_percentage: f64,
    /// Percentage of items with at least one child note.
    pub(crate) notes_percentage: f64,
}

/// One page of library coverage results alongside pagination metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryCoveragePage {
    /// Aggregate coverage statistics for items in this page.
    pub(crate) coverage: LibraryCoverage,
    /// Pagination metadata describing the current offset, limit, and total
    /// count.
    pub(crate) pagination: PaginationInfo,
}

impl ZoteroClient<'_> {
    /// Computes library or optional `collection_key` coverage statistics for
    /// PDF attachments, DOIs, and child notes.
    ///
    /// # Arguments
    ///
    /// * `collection_key` - Optional collection key to filter coverage metrics;
    ///   [`None`] evaluates the entire library.
    /// * `offset` - Zero-based pagination offset.
    /// * `limit` - Maximum number of library items to evaluate in this page.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroApiError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroApiError::Json`] if the response body cannot be decoded.
    #[inline]
    pub async fn get_library_coverage(
        &self,
        collection_key: Option<&CollectionKey>,
        offset: usize,
        limit: usize,
    ) -> Result<LibraryCoveragePage, ZoteroApiError> {
        let base = match collection_key {
            Some(col) => format!(
                "{}{}/collections/{}/items",
                self.state.zotero_api_url(),
                self.target_prefix(),
                col
            ),
            None => format!(
                "{}{}/items?itemType=-note&sort=dateModified&direction=desc",
                self.state.zotero_api_url(),
                self.target_prefix()
            ),
        };
        let page_url = add_pagination(&base, offset, limit);
        let page = self.get_items_with_total(&page_url).await?;
        let pagination =
            coverage_pagination(offset, limit, page.items.len(), page.total);

        let mut children_by_idx = Vec::with_capacity(page.items.len());
        for item in &page.items {
            children_by_idx.push(
                self.get_item_children(&item.key).await.unwrap_or_default(),
            );
        }

        Ok(classify_coverage_page(&page.items, &children_by_idx, pagination))
    }
}

/// Evaluates PDF, DOI, and note availability flags for a single `item`.
fn coverage_flags(
    item: &ZoteroItem,
    children: &[ZoteroItem],
) -> ItemCoverageFlags {
    let has_doi = item.data.doi.as_ref().is_some_and(|d| !d.trim().is_empty());
    let has_pdf = children.iter().any(|child| {
        child.data.item_type == ItemType::Attachment
            && child
                .data
                .content_type
                .as_deref()
                .is_some_and(|ct| ct.contains("pdf"))
    });
    let has_notes =
        children.iter().any(|child| child.data.item_type == ItemType::Note);

    ItemCoverageFlags {
        has_pdf,
        has_doi,
        has_notes,
    }
}

fn coverage_pagination(
    offset: usize,
    limit: usize,
    returned: usize,
    server_total: Option<usize>,
) -> PaginationInfo {
    let total = server_total.unwrap_or_else(|| offset.saturating_add(returned));
    let page_offset =
        server_total.map_or(offset, |known_total| offset.min(known_total));
    PaginationInfo {
        limit,
        offset: page_offset,
        total,
        has_more: server_total.map_or(returned == limit, |known_total| {
            page_offset.saturating_add(returned) < known_total
        }),
    }
}

fn classify_coverage_page(
    selected: &[ZoteroItem],
    children_by_idx: &[Vec<ZoteroItem>],
    pagination: PaginationInfo,
) -> LibraryCoveragePage {
    let mut flags = Vec::with_capacity(selected.len());
    for (item, children) in selected.iter().zip(children_by_idx) {
        flags.push(coverage_flags(item, children));
    }
    LibraryCoveragePage {
        coverage: classify_coverage(&flags),
        pagination,
    }
}

/// Aggregates coverage flags across library items into [`LibraryCoverage`].
fn classify_coverage(flags: &[ItemCoverageFlags]) -> LibraryCoverage {
    let total = flags.len();
    if total == 0 {
        return LibraryCoverage {
            total_items: 0,
            with_pdf: 0,
            with_doi: 0,
            with_notes: 0,
            pdf_percentage: 0.0,
            doi_percentage: 0.0,
            notes_percentage: 0.0,
        };
    }

    let with_pdf = flags.iter().filter(|f| f.has_pdf).count();
    let with_doi = flags.iter().filter(|f| f.has_doi).count();
    let with_notes = flags.iter().filter(|f| f.has_notes).count();

    LibraryCoverage {
        total_items: total,
        with_pdf,
        with_doi,
        with_notes,
        pdf_percentage: compute_percentage(with_pdf, total),
        doi_percentage: compute_percentage(with_doi, total),
        notes_percentage: compute_percentage(with_notes, total),
    }
}

#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "percentages calculation requires float conversion"
)]
/// Calculates percentage ratio of `count` out of `total`.
fn compute_percentage(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (count as f64 / total as f64) * 100.0
    }
}

/// Type of duplication criterion matched.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DuplicateType {
    /// Matched by DOI string.
    Doi,
    /// Matched by normalized title string.
    Title,
}

/// Group of items identified as potential duplicates.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DuplicateGroup {
    /// Duplication criterion matched (`Doi` or `Title`).
    pub(crate) match_type: DuplicateType,
    /// Matched DOI or normalized title string.
    pub(crate) match_value: String,
    /// Item keys belonging to this duplicate group.
    pub(crate) item_keys: Vec<ItemKey>,
}

impl ZoteroClient<'_> {
    /// Scans the library target or optional `collection_key` for potential
    /// duplicate items matching by title or DOI.
    ///
    /// Grouping is performed by normalizing titles (case-insensitively, with
    /// title length greater than 5 characters) and DOIs. Returns a list of
    /// [`DuplicateGroup`] structures listing matched keys.
    ///
    /// # Arguments
    /// * `collection_key` - Optional collection key to scope duplicate
    ///   detection; [`None`] scans the whole library.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if fetching library items fails.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if response decoding fails.
    #[inline]
    pub async fn find_duplicates(
        &self,
        collection_key: Option<&CollectionKey>,
    ) -> Result<Vec<DuplicateGroup>, ZoteroApiError> {
        let items = if let Some(col) = collection_key {
            self.get_collection_items(col).await?
        } else {
            self.get_all_items().await?
        };

        Ok(find_duplicate_groups(&items))
    }
}

/// Group items by matching DOI or title to identify potential duplicate items.
fn find_duplicate_groups(items: &[ZoteroItem]) -> Vec<DuplicateGroup> {
    let mut doi_map: std::collections::BTreeMap<String, Vec<&ZoteroItem>> =
        std::collections::BTreeMap::new();
    let mut title_map: std::collections::BTreeMap<String, Vec<&ZoteroItem>> =
        std::collections::BTreeMap::new();

    for item in items {
        if let Some(ref doi) = item.data.doi {
            if !doi.trim().is_empty() {
                doi_map
                    .entry(doi.trim().to_lowercase())
                    .or_default()
                    .push(item);
            }
        }
        if let Some(ref title) = item.data.title {
            let t = title.trim().to_lowercase();
            if t.len() > 5 {
                title_map.entry(t).or_default().push(item);
            }
        }
    }

    let mut duplicates = Vec::new();
    for (doi, grouped) in doi_map {
        if grouped.len() > 1 {
            duplicates.push(DuplicateGroup {
                match_type: DuplicateType::Doi,
                match_value: doi,
                item_keys: grouped.iter().map(|i| i.key.clone()).collect(),
            });
        }
    }
    for (title, grouped) in title_map {
        if grouped.len() > 1 {
            duplicates.push(DuplicateGroup {
                match_type: DuplicateType::Title,
                match_value: title,
                item_keys: grouped.iter().map(|i| i.key.clone()).collect(),
            });
        }
    }

    duplicates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        keys::{ItemKey, LibraryVersion},
        objects::{ZoteroItemData, ZoteroTag},
        types::{ItemType, TagOrigin},
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
                library: None,
                links: None,
                meta: None,
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
        fn matches_creator_name_first_last_and_full_name() {
            let mut item = make_item(None, None, None);
            item.data.creators = vec![ZoteroCreator {
                creator_type: None,
                first_name: Some("Ada".to_owned()),
                last_name: Some("Lovelace".to_owned()),
                name: Some("Countess".to_owned()),
            }];
            for value in ["Ada", "Lovelace", "Ada Lovelace", "Countess"] {
                let cond = SearchCondition {
                    field: SearchField::Creator,
                    operator: SearchOperator::Contains,
                    value: value.to_owned(),
                };
                assert!(match_condition(&item, &cond), "creator case {value}");
            }
        }

        #[test]
        fn matches_item_type_tag_extra_and_other_mapped_fields() {
            let mut item =
                make_item(Some("Mapped Title"), None, Some("10.1000/x"));
            item.data.extra = Some("Citation Key: mapped2024".to_owned());
            item.data.tags = vec![ZoteroTag {
                tag: TagName::from("Methods"),
                origin: TagOrigin::User,
            }];
            let cases = [
                (SearchField::ItemType, "journalArticle"),
                (SearchField::Tag, "methods"),
                (SearchField::Extra, "mapped2024"),
                (SearchField::Other("title".to_owned()), "mapped title"),
                (SearchField::Other("doi".to_owned()), "10.1000/x"),
            ];
            for (field, value) in cases {
                let cond = SearchCondition {
                    field,
                    operator: SearchOperator::Contains,
                    value: value.to_owned(),
                };
                assert!(
                    match_condition(&item, &cond),
                    "mapped field case {value}"
                );
            }
        }

        #[test]
        fn returns_false_for_unknown_other_field_and_missing_values() {
            let item = make_item(None, None, None);
            let unknown = SearchCondition {
                field: SearchField::Other("unknown".to_owned()),
                operator: SearchOperator::Contains,
                value: "anything".to_owned(),
            };
            let missing_title = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::Contains,
                value: "anything".to_owned(),
            };

            assert!(
                !match_condition(&item, &unknown),
                "unknown other field must not match"
            );
            assert!(
                !match_condition(&item, &missing_title),
                "missing title must not match"
            );
        }

        #[test]
        fn matches_ends_with_and_is_before() {
            let item =
                make_item(Some("Learning Rust"), Some("2024-02-15"), None);
            let ends_with = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::EndsWith,
                value: "rust".to_owned(),
            };
            let before = SearchCondition {
                field: SearchField::Date,
                operator: SearchOperator::IsBefore,
                value: "2025-01-01".to_owned(),
            };

            assert!(
                match_condition(&item, &ends_with),
                "title should match EndsWith"
            );
            assert!(
                match_condition(&item, &before),
                "date should match IsBefore"
            );
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

    mod pagination {
        use pretty_assertions::assert_eq;

        use super::*;

        fn item(key: &str) -> ZoteroItem {
            ZoteroItem {
                key: ItemKey::from(key),
                version: LibraryVersion(1),
                library: None,
                links: None,
                meta: None,
                data: ZoteroItemData {
                    key: ItemKey::from(key),
                    version: LibraryVersion(1),
                    item_type: ItemType::JournalArticle,
                    ..Default::default()
                },
            }
        }

        #[test]
        fn accumulates_offset_limit_total_and_has_more() {
            let mut accumulator = PageAccumulator::new(1, 2);
            for key in ["K1", "K2", "K3", "K4"] {
                accumulator.push_match(item(key));
            }

            let page = accumulator.into_page();

            assert_eq!(page.pagination.offset, 1);
            assert_eq!(page.pagination.limit, 2);
            assert_eq!(page.pagination.total, 4);
            assert!(
                page.pagination.has_more,
                "one matching item remains after this page"
            );
            assert_eq!(
                page.items
                    .iter()
                    .map(|item| item.key.as_str())
                    .collect::<Vec<_>>(),
                vec!["K2", "K3"]
            );
        }

        #[test]
        fn clamps_offset_when_paginating_past_end() {
            let page = paginate(vec![item("K1")], 10, 5);

            assert_eq!(page.pagination.offset, 1);
            assert_eq!(page.pagination.total, 1);
            assert_eq!(page.items.len(), 0);
            assert_eq!(
                page.pagination.has_more, false,
                "offset past end has no next page"
            );
        }
    }

    mod sort_items {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::{keys::ItemKey, objects::ZoteroItemData, types::ItemType};

        fn item(key: &str, title: &str, date: &str) -> ZoteroItem {
            ZoteroItem {
                key: ItemKey::from(key),
                version: LibraryVersion(1),
                library: None,
                links: None,
                meta: None,
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
            let sorted = sort_items(items, SortField::Title, SortOrder::Asc);
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
            let sorted = sort_items(items, SortField::Date, SortOrder::Desc);
            let keys: Vec<&str> =
                sorted.iter().map(|i| i.key.as_str()).collect();
            assert_eq!(keys, vec!["K2", "K3", "K1"]);
        }
    }

    mod advanced_search {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::{
            client::{
                ZoteroClient,
                test_http::{
                    MockServer, http_response, http_response_with_headers,
                },
            },
            state::AppState,
        };

        fn items_page(items: &[serde_json::Value]) -> String {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(std::string::ToString::to_string)
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

        fn zotero_item_of_type(
            key: &str,
            title: &str,
            item_type: &str,
        ) -> serde_json::Value {
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": item_type,
                    "title": title,
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
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": "journalArticle",
                    "title": title,
                    "extra": null,
                    "creators": [{ "name": creator }],
                    "dateAdded": "2024-01-01T00:00:00Z",
                    "dateModified": "2024-01-01T00:00:00Z",
                },
            })
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

        fn zotero_state(zotero_api_url: impl AsRef<str>) -> AppState {
            AppState::test_default()
                .with_zotero_api_url(zotero_api_url.as_ref())
        }

        #[tokio::test]
        async fn uses_any_join_mode_on_slow_path() {
            let item1 = zotero_item("K1", "Rust Book", Some("nomatch"));
            let item2 = zotero_item("K2", "Go Book", Some("needle"));
            let server = MockServer::new(vec![http_response(
                "200 OK",
                &items_page(&[item1, item2]),
            )]);
            let base = server.url();
            let state = zotero_state(base);
            let cond_extra = SearchCondition {
                field: SearchField::Extra,
                operator: SearchOperator::Contains,
                value: "needle".to_owned(),
            };

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust"), cond_extra],
                    JoinMode::Any,
                    None,
                    SortOrder::Asc,
                    0,
                    10,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 2);
        }

        #[tokio::test]
        async fn excludes_notes_attachments_and_annotations_on_slow_path() {
            let article = zotero_item("K1", "Rust", None);
            let note = zotero_item_of_type("N1", "Rust", "note");
            let attachment = zotero_item_of_type("A1", "Rust", "attachment");
            let annotation = zotero_item_of_type("AN1", "Rust", "annotation");
            let server = MockServer::new(vec![http_response(
                "200 OK",
                &items_page(&[article, note, attachment, annotation]),
            )]);
            let base = server.url();
            let state = zotero_state(base);

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust")],
                    JoinMode::All,
                    Some(SortField::Title),
                    SortOrder::Asc,
                    0,
                    10,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 1);
            assert_eq!(
                page.items.first().map(|item| item.key.as_str()),
                Some("K1")
            );
        }

        #[test]
        fn pushdown_url_refuses_empty_conditions() {
            let state = zotero_state("http://127.0.0.1:23119/api");
            let client = ZoteroClient::new(&state);

            assert!(
                client.pushdown_url(&[]).is_none(),
                "empty conditions require slow path"
            );
        }

        #[test]
        fn pushdown_url_refuses_multiple_free_text_conditions() {
            let state = zotero_state("http://127.0.0.1:23119/api");
            let client = ZoteroClient::new(&state);
            let year = SearchCondition {
                field: SearchField::Year,
                operator: SearchOperator::Is,
                value: "2024".to_owned(),
            };

            assert!(
                client.pushdown_url(&[creator_contains("Ada"), year]).is_none(),
                "only one q/qmode pair can be pushed down"
            );
        }

        #[tokio::test]
        async fn slow_path_filters_full_library_and_paginates() {
            let item1 = zotero_item("K1", "Rust in Action", Some("book"));
            let item2 = zotero_item("K2", "Rust for Beginners", Some("book"));
            let item3 = zotero_item("K3", "Rust Essentials", Some("talk"));
            let server = MockServer::new(vec![http_response(
                "200 OK",
                &items_page(&[item1, item2, item3]),
            )]);
            let base = server.url();
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
                    SortOrder::Asc,
                    0,
                    1,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 1);
            assert_eq!(
                page.items
                    .first()
                    .map(|item| item.key.as_str())
                    .unwrap_or_default(),
                "K2"
            );
            assert_eq!(page.pagination.limit, 1);
            assert_eq!(page.pagination.offset, 0);
            assert_eq!(page.pagination.total, 2);
            assert!(page.pagination.has_more);
        }

        #[tokio::test]
        async fn sort_request_uses_slow_path_so_sort_is_applied() {
            let item1 = zotero_item("K1", "A Rust Book", None);
            let item2 = zotero_item("K2", "Z Rust Book", None);
            let (server, requests) =
                MockServer::recording(vec![http_response(
                    "200 OK",
                    &items_page(&[item1, item2]),
                )]);
            let base = server.url();
            let state = zotero_state(base);

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust")],
                    JoinMode::All,
                    Some(SortField::Title),
                    SortOrder::Desc,
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
            let server = MockServer::new(vec![http_response(
                "200 OK",
                &items_page(&[item1, item2]),
            )]);
            let base = server.url();
            let state = zotero_state(base);

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust")],
                    JoinMode::All,
                    None,
                    SortOrder::Asc,
                    0,
                    10,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 1);
            assert_eq!(
                page.items.first().and_then(|item| item.data.title.as_deref()),
                Some("Rust Patterns")
            );
        }

        #[tokio::test]
        async fn fast_path_uses_server_side_search_and_total_header() {
            let item1 = zotero_item("K1", "Rust in Action", None);
            let server = MockServer::new(vec![http_response_with_headers(
                "200 OK",
                &[("Total-Results", "17")],
                &items_page(&[item1]),
            )]);
            let base = server.url();
            let state = zotero_state(base);

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![creator_contains("Ferris")],
                    JoinMode::All,
                    None,
                    SortOrder::Asc,
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
            let server = MockServer::new(vec![http_response(
                "200 OK",
                &items_page(&[item1]),
            )]);
            let base = server.url();
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
                    SortOrder::Asc,
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
            let server = MockServer::new(vec![http_response(
                "200 OK",
                &items_page(&[item1]),
            )]);
            let base = server.url();
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
                    SortOrder::Asc,
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
            let (server, requests) =
                MockServer::recording(vec![http_response_with_headers(
                    "200 OK",
                    &[("Total-Results", "1")],
                    &items_page(&[item1]),
                )]);
            let base = server.url();
            let state = zotero_state(base);

            ZoteroClient::new(&state)
                .advanced_search(
                    vec![creator_contains("Ferris")],
                    JoinMode::All,
                    None,
                    SortOrder::Asc,
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
            let (server, requests) =
                MockServer::recording(vec![http_response_with_headers(
                    "200 OK",
                    &[("Total-Results", "1")],
                    &items_page(&[item1]),
                )]);
            let base = server.url();
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
                    SortOrder::Asc,
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
            let state = zotero_state("http://127.0.0.1:23119/api");
            let client = ZoteroClient::new(&state);
            let url = client
                .pushdown_url(&[creator_contains("Ferris Crab")])
                .unwrap();
            assert!(url.contains("q=Ferris%20Crab"));
            assert!(url.contains("qmode=creator"));
        }

        #[test]
        fn pushdown_url_refuses_title() {
            let state = zotero_state("http://127.0.0.1:23119/api");
            let client = ZoteroClient::new(&state);
            assert!(client.pushdown_url(&[title_contains("Rust")]).is_none());
        }

        #[test]
        fn pushdown_url_refuses_non_pushable_operator() {
            let state = zotero_state("http://127.0.0.1:23119/api");
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
            let state = zotero_state("http://127.0.0.1:23119/api");
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

    mod coverage {
        use super::*;

        mod compute_percentage {

            use super::*;
            #[test]
            fn returns_percentage_ratio_for_given_counts() {
                assert!((compute_percentage(1, 2) - 50.0).abs() < f64::EPSILON);
                assert!((compute_percentage(3, 4) - 75.0).abs() < f64::EPSILON);
            }

            #[test]
            fn returns_zero_when_total_is_zero() {
                assert!((compute_percentage(0, 0) - 0.0).abs() < f64::EPSILON);
                assert!((compute_percentage(5, 0) - 0.0).abs() < f64::EPSILON);
            }
        }

        mod coverage_flags {
            use pretty_assertions::assert_eq;

            use super::*;

            fn item(
                key: &str,
                item_type: ItemType,
                doi: Option<&str>,
            ) -> ZoteroItem {
                ZoteroItem {
                    key: ItemKey::from(key),
                    version: LibraryVersion(1),
                    library: None,
                    links: None,
                    meta: None,
                    data: ZoteroItemData {
                        key: ItemKey::from(key),
                        version: LibraryVersion(1),
                        item_type,
                        doi: doi.map(ToOwned::to_owned),
                        ..Default::default()
                    },
                }
            }

            #[test]
            fn evaluates_item_coverage_flags_correctly() {
                let item = ZoteroItem {
                    key: ItemKey::from("ITEM0001"),
                    version: LibraryVersion(1),
                    library: None,
                    links: None,
                    meta: None,
                    data: ZoteroItemData {
                        key: ItemKey::from("ITEM0001"),
                        version: LibraryVersion(1),
                        item_type: ItemType::JournalArticle,
                        doi: Some("10.1000/1".to_owned()),
                        note: Some("Self note".to_owned()),
                        ..Default::default()
                    },
                };
                let attachment = ZoteroItem {
                    key: ItemKey::from("ATTACH01"),
                    version: LibraryVersion(1),
                    library: None,
                    links: None,
                    meta: None,
                    data: ZoteroItemData {
                        key: ItemKey::from("ATTACH01"),
                        version: LibraryVersion(1),
                        item_type: ItemType::Attachment,
                        content_type: Some("application/pdf".to_owned()),
                        ..Default::default()
                    },
                };
                let note = ZoteroItem {
                    key: ItemKey::from("NOTE0001"),
                    version: LibraryVersion(1),
                    library: None,
                    links: None,
                    meta: None,
                    data: ZoteroItemData {
                        key: ItemKey::from("NOTE0001"),
                        version: LibraryVersion(1),
                        item_type: ItemType::Note,
                        note: Some("Child note".to_owned()),
                        ..Default::default()
                    },
                };

                let children = vec![attachment, note];
                let flags = coverage_flags(&item, &children);
                assert!(flags.has_doi, "expected DOI flag for nonblank DOI");
                assert!(flags.has_pdf, "expected PDF flag for PDF attachment");
                assert!(flags.has_notes, "expected notes flag for child note");
            }

            #[test]
            fn aggregates_library_coverage_statistics_and_percentages() {
                let flags1 = ItemCoverageFlags {
                    has_pdf: true,
                    has_doi: true,
                    has_notes: false,
                };
                let flags2 = ItemCoverageFlags {
                    has_pdf: false,
                    has_doi: true,
                    has_notes: true,
                };

                let coverage = classify_coverage(&[flags1, flags2]);
                assert_eq!(coverage.total_items, 2);
                assert_eq!(coverage.with_pdf, 1);
                assert_eq!(coverage.with_doi, 2);
                assert_eq!(coverage.with_notes, 1);
                assert!((coverage.pdf_percentage - 50.0).abs() < f64::EPSILON);
                assert!((coverage.doi_percentage - 100.0).abs() < f64::EPSILON);
                assert!(
                    (coverage.notes_percentage - 50.0).abs() < f64::EPSILON
                );
            }

            #[test]
            fn classify_coverage_returns_zeroed_stats_for_empty_input() {
                let coverage = classify_coverage(&[]);

                assert_eq!(coverage.total_items, 0);
                assert_eq!(coverage.with_pdf, 0);
                assert_eq!(coverage.with_doi, 0);
                assert_eq!(coverage.with_notes, 0);
                assert!((coverage.pdf_percentage - 0.0).abs() < f64::EPSILON);
                assert!((coverage.doi_percentage - 0.0).abs() < f64::EPSILON);
                assert!((coverage.notes_percentage - 0.0).abs() < f64::EPSILON);
            }

            #[test]
            fn coverage_flags_ignores_blank_doi_and_non_pdf_attachment() {
                let item =
                    item("ITEM0001", ItemType::JournalArticle, Some("  "));
                let attachment = ZoteroItem {
                    key: ItemKey::from("ATTACH01"),
                    version: LibraryVersion(1),
                    library: None,
                    links: None,
                    meta: None,
                    data: ZoteroItemData {
                        key: ItemKey::from("ATTACH01"),
                        version: LibraryVersion(1),
                        item_type: ItemType::Attachment,
                        content_type: Some("text/plain".to_owned()),
                        ..Default::default()
                    },
                };

                let children = vec![attachment];
                let flags = coverage_flags(&item, &children);

                assert!(!flags.has_doi, "blank DOI must not count as coverage");
                assert!(
                    !flags.has_pdf,
                    "non-PDF attachment must not count as PDF"
                );
                assert!(!flags.has_notes, "no note children were arranged");
            }

            #[test]
            fn coverage_pagination_without_total_marks_full_page_as_more_and_short_page_as_done()
             {
                let full_page = coverage_pagination(20, 10, 10, None);
                let short_page = coverage_pagination(20, 10, 3, None);

                assert_eq!(full_page.total, 30);
                assert!(
                    full_page.has_more,
                    "unknown total and full page implies more"
                );
                assert_eq!(short_page.total, 23);
                assert!(
                    !short_page.has_more,
                    "unknown total and short page means pagination is done"
                );
            }

            #[test]
            fn coverage_pagination_uses_server_total_for_has_more() {
                let pagination = coverage_pagination(0, 2, 2, Some(3));

                assert_eq!(pagination.total, 3);
                assert!(
                    pagination.has_more,
                    "known total exceeds returned page"
                );
            }

            #[test]
            fn coverage_pagination_clamps_offset_when_total_is_known() {
                let pagination = coverage_pagination(99, 10, 0, Some(3));

                assert_eq!(pagination.offset, 3);
            }

            #[test]
            fn library_coverage_page_classifies_only_selected_items() {
                let selected = vec![
                    item(
                        "ITEM0001",
                        ItemType::JournalArticle,
                        Some("10.1000/1"),
                    ),
                    item("ITEM0002", ItemType::JournalArticle, None),
                ];
                let children_by_idx = vec![
                    vec![item("PDF0001", ItemType::Attachment, None)],
                    vec![item("NOTE0001", ItemType::Note, None)],
                ];

                let pagination = PaginationInfo {
                    limit: 2,
                    offset: 0,
                    total: 3,
                    has_more: true,
                };
                let page = classify_coverage_page(
                    &selected,
                    &children_by_idx,
                    pagination,
                );

                assert_eq!(page.coverage.total_items, 2);
                assert_eq!(page.coverage.with_doi, 1);
                assert_eq!(page.coverage.with_notes, 1);
                assert_eq!(page.pagination.total, 3);
                assert!(
                    page.pagination.has_more,
                    "known total exceeds selected page"
                );
            }
        }
    }

    mod duplicates {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::client::{
            ZoteroClient,
            test_http::{MockServer, http_response},
        };

        fn make_item(
            key: &str,
            title: Option<&str>,
            doi: Option<&str>,
        ) -> ZoteroItem {
            ZoteroItem {
                key: ItemKey::from(key),
                version: LibraryVersion(1),
                library: None,
                links: None,
                meta: None,
                data: ZoteroItemData {
                    key: ItemKey::from(key),
                    version: LibraryVersion(1),
                    item_type: ItemType::JournalArticle,
                    title: title.map(ToOwned::to_owned),
                    doi: doi.map(ToOwned::to_owned),
                    ..Default::default()
                },
            }
        }

        #[test]
        fn groups_items_by_matching_doi_case_insensitively() {
            let item1 =
                make_item("ITEM0001", Some("Paper A"), Some("10.1234/XYZ"));
            let item2 =
                make_item("ITEM0002", Some("Paper B"), Some("10.1234/xyz "));

            let items = vec![item1, item2];
            let groups = find_duplicate_groups(&items);
            assert_eq!(groups.len(), 1);
            assert_eq!(
                groups.first().map(|group| group.match_type),
                Some(DuplicateType::Doi)
            );
            assert_eq!(
                groups.first().map(|group| group.match_value.as_str()),
                Some("10.1234/xyz")
            );
            assert_eq!(
                groups.first().map(|group| group.item_keys.as_slice()),
                Some(
                    &[ItemKey::from("ITEM0001"), ItemKey::from("ITEM0002")][..]
                )
            );
        }

        #[test]
        fn groups_items_by_matching_title_case_insensitively() {
            let item1 =
                make_item("ITEM0001", Some("Quantum Computing Advances"), None);
            let item2 =
                make_item("ITEM0002", Some("quantum computing advances"), None);

            let items = vec![item1, item2];
            let groups = find_duplicate_groups(&items);
            assert_eq!(groups.len(), 1);
            assert_eq!(
                groups.first().map(|group| group.match_type),
                Some(DuplicateType::Title)
            );
            assert_eq!(
                groups.first().map(|group| group.match_value.as_str()),
                Some("quantum computing advances")
            );
        }

        #[test]
        fn ignores_blank_doi_and_titles_that_are_too_short() {
            let item1 = make_item("ITEM0001", Some("AI"), Some("  "));
            let item2 = make_item("ITEM0002", Some("ai"), Some(""));

            let items = vec![item1, item2];
            let groups = find_duplicate_groups(&items);

            assert!(
                groups.is_empty(),
                "blank DOI and titles shorter than three characters must be \
                 ignored"
            );
        }

        #[test]
        fn returns_empty_when_no_duplicates_exist() {
            let item1 =
                make_item("ITEM0001", Some("Paper 1"), Some("10.1000/1"));
            let item2 =
                make_item("ITEM0002", Some("Paper 2"), Some("10.1000/2"));

            let items = vec![item1, item2];
            let groups = find_duplicate_groups(&items);
            assert!(
                groups.is_empty(),
                "distinct DOI/title pairs must not group"
            );
        }

        fn item_json(key: &str, title: &str) -> String {
            format!(
                r#"{{"key":"{key}","version":1,"data":{{"key":"{key}","version":1,"itemType":"journalArticle","title":"{title}"}}}}"#
            )
        }

        fn test_state(zotero_api_url: String) -> crate::state::AppState {
            crate::state::AppState::test_default()
                .with_zotero_api_url(zotero_api_url)
        }

        #[tokio::test]
        async fn find_duplicates_scans_more_than_one_hundred_items() {
            // 120 unique items => 2 pages (100 + 20). Two items share a title
            // ("Shared Title") so exactly one duplicate group must be found.
            let mut bodies = Vec::new();
            for i in 0..100 {
                bodies.push(item_json(
                    &format!("K{i:04}"),
                    &format!("Title {i}"),
                ));
            }
            bodies.push(item_json("K9900", "Shared Title"));
            bodies.push(item_json("K9901", "Shared Title"));
            for i in 102..120 {
                bodies.push(item_json(
                    &format!("K{i:04}"),
                    &format!("Title {i}"),
                ));
            }
            // Use take/skip (not `bodies[a..b]` slicing) to avoid the
            // `indexing_slicing` deny lint.
            let page1 = format!(
                "[{}]",
                bodies
                    .iter()
                    .take(100)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let page2 = format!(
                "[{}]",
                bodies
                    .iter()
                    .skip(100)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            );

            let server = MockServer::new(vec![
                http_response("200 OK", &page1),
                http_response("200 OK", &page2),
            ]);
            let base = server.url();
            let state = test_state(base.to_owned());

            let groups =
                ZoteroClient::new(&state).find_duplicates(None).await.unwrap();

            assert_eq!(groups.len(), 1);
            assert_eq!(
                groups.first().map(|group| group.match_type),
                Some(DuplicateType::Title)
            );
            assert_eq!(
                groups.first().map(|group| group.item_keys.len()),
                Some(2)
            );
        }
    }
}
