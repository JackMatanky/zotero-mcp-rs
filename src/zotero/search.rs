//! Search and query operations for the Zotero Local HTTP API.

use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        client::ZoteroClient,
        models::{CitationKey, CollectionKey, TagName, ZoteroItem},
    },
};

/// Searchable item field in structured searches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SearchOperator {
    #[default]
    Contains,
    Is,
    StartsWith,
    EndsWith,
    #[serde(untagged)]
    Other(String),
}

/// Structured search condition matching a specific item field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SearchCondition {
    pub(crate) field: SearchField,
    #[serde(default)]
    pub(crate) operator: SearchOperator,
    pub(crate) value: String,
}

impl ZoteroClient<'_> {
    /// Searches library items matching a query string, excluding notes.
    ///
    /// # Arguments
    ///
    /// - `query`: Free-text query matching title, creator, year, or fulltext.
    /// - `collection_key`: Optional collection key to scope the search.
    /// - `limit`: Maximum number of items to return.
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
        let items = self.get_recent_items(100).await?;
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

fn match_condition(item: &ZoteroItem, cond: &SearchCondition) -> bool {
    let val = cond.value.to_lowercase();
    let matches_str = |s: &str| match cond.operator {
        SearchOperator::Is => s.to_lowercase() == val,
        SearchOperator::StartsWith => s.to_lowercase().starts_with(&val),
        SearchOperator::EndsWith => s.to_lowercase().ends_with(&val),
        SearchOperator::Contains | SearchOperator::Other(_) => {
            s.to_lowercase().contains(&val)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_condition_deserialize() {
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
}
