//! Read-only queries for the Zotero Local HTTP API.

use reqwest::StatusCode;

use crate::{
    errors::ZoteroMcpError,
    zotero::{ZoteroClient, ZoteroCollection, ZoteroItem},
};

impl ZoteroClient<'_> {
    /// Fetches the `limit` most recently modified library items, excluding
    /// notes.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_recent_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items?limit={}&sort=dateModified&direction=desc&\
             itemType=-note",
            self.state.zotero_api_url, limit
        );
        self.get_json(&url).await
    }

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
        collection_key: Option<&str>,
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

    /// Fetches the item identified by `item_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::NotFound`] if the item does not exist
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_item(
        &self,
        item_key: &str,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        let url =
            format!("{}/users/0/items/{}", self.state.zotero_api_url, item_key);
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ZoteroMcpError::NotFound(format!("Item {item_key}")));
        }
        Ok(self.ensure_success(resp).await?.json().await?)
    }

    /// Fetches every collection in the library.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_collections(
        &self,
    ) -> Result<Vec<ZoteroCollection>, ZoteroMcpError> {
        let url = format!("{}/users/0/collections", self.state.zotero_api_url);
        self.get_json(&url).await
    }

    /// Searches collections by `query`, matching collection names
    /// case-insensitively.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn search_collections(
        &self,
        query: &str,
    ) -> Result<Vec<ZoteroCollection>, ZoteroMcpError> {
        let collections = self.get_collections().await?;
        let query_lc = query.to_lowercase();
        let filtered = collections
            .into_iter()
            .filter(|c| c.data.name.to_lowercase().contains(&query_lc))
            .collect();
        Ok(filtered)
    }

    /// Fetches every item inside the collection identified by
    /// `collection_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_collection_items(
        &self,
        collection_key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/collections/{}/items",
            self.state.zotero_api_url, collection_key
        );
        self.get_json(&url).await
    }

    /// Fetches the child items (notes and attachments) of `item_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_item_children(
        &self,
        item_key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/children",
            self.state.zotero_api_url, item_key
        );
        self.get_json(&url).await
    }

    /// Fetches Zotero's indexed fulltext content for `item_key`, returning an
    /// empty string if unindexed.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_item_fulltext(
        &self,
        item_key: &str,
    ) -> Result<String, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/fulltext",
            self.state.zotero_api_url, item_key
        );
        let val: serde_json::Value = self.get_json(&url).await?;
        let content = val
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        Ok(content)
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
        tag: &str,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let encoded_tag = urlencoding::encode(tag);
        let url = format!(
            "{}/users/0/items?tag={}&limit={}&itemType=-note",
            self.state.zotero_api_url, encoded_tag, limit
        );
        self.get_json(&url).await
    }

    /// Searches items by citation key.
    ///
    /// Matches Zotero's native `citationKey` item field first (Zotero 9+;
    /// authoritative when present, and already part of what `search_items`
    /// finds server-side via Zotero's quicksearch). Falls back to scanning
    /// the legacy `extra` field for items with no native citation key --
    /// libraries still on Zotero <9, or a Better `BibTeX` install that only
    /// ever wrote `Citation Key: ...` to `extra`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn search_by_citation_key(
        &self,
        citekey: &str,
    ) -> Result<Option<ZoteroItem>, ZoteroMcpError> {
        let items = self.search_items(citekey, None, 20).await?;
        let citekey_lc = citekey.to_lowercase();
        for item in items {
            if let Some(ref native) = item.data.citation_key {
                if native.to_lowercase() == citekey_lc {
                    return Ok(Some(item));
                }
                continue;
            }
            if let Some(ref extra) = item.data.extra {
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
        conditions: Vec<serde_json::Value>,
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

    /// Finds potential duplicate items in the library or optional
    /// `collection_key` by matching title or DOI.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn find_duplicates(
        &self,
        collection_key: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, ZoteroMcpError> {
        let items = if let Some(col) = collection_key {
            self.get_collection_items(col).await?
        } else {
            let url = format!(
                "{}/users/0/items?limit=100",
                self.state.zotero_api_url
            );
            self.get_json(&url).await?
        };

        Ok(find_duplicate_groups(&items))
    }

    /// Computes library or optional `collection_key` coverage statistics for
    /// PDF, DOI, and notes.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_library_coverage(
        &self,
        collection_key: Option<&str>,
    ) -> Result<serde_json::Value, ZoteroMcpError> {
        let items = match collection_key {
            Some(col) => self.get_collection_items(col).await?,
            None => self.get_recent_items(100).await?,
        };

        let mut flags = Vec::with_capacity(items.len());
        for item in &items {
            let children =
                self.get_item_children(&item.key).await.unwrap_or_default();
            flags.push(coverage_flags(item, &children));
        }

        Ok(classify_coverage(&flags))
    }

    /// Extracts and synthesizes annotations and notes for `item_key` into
    /// structured Markdown.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn synthesize_annotations(
        &self,
        item_key: &str,
    ) -> Result<String, ZoteroMcpError> {
        use std::fmt::Write as _;

        let item = self.get_item(item_key).await?;
        let children =
            self.get_item_children(item_key).await.unwrap_or_default();

        let mut md = String::new();
        let title = item.data.title.as_deref().unwrap_or(item_key);
        let _ = writeln!(md, "# Annotations & Notes: {title}\n");

        if let Some(ref doi) = item.data.doi {
            let _ = writeln!(md, "**DOI:** {doi}");
        }
        if let Some(ref date) = item.data.date {
            let _ = writeln!(md, "**Date:** {date}");
        }
        md.push('\n');
        md.push_str(&format_annotations_section(&children));
        md.push_str(&format_notes_section(&item, &children));

        Ok(md)
    }

    /// Lists all tag names in the library, returning up to `limit` tags.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn list_tags(
        &self,
        limit: usize,
    ) -> Result<Vec<String>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/tags?limit={}",
            self.state.zotero_api_url, limit
        );
        let raw: Vec<serde_json::Value> = self.get_json(&url).await?;
        Ok(raw
            .into_iter()
            .filter_map(|v| {
                v.get("tag").and_then(|t| t.as_str()).map(str::to_owned)
            })
            .collect())
    }

    /// Lists top-level items not belonging to any collection, up to `limit`
    /// items.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_unfiled_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/top?limit={}",
            self.state.zotero_api_url, limit
        );
        let items: Vec<ZoteroItem> = self.get_json(&url).await?;
        Ok(items
            .into_iter()
            .filter(|i| i.data.collections.is_empty())
            .collect())
    }
}

fn match_condition(item: &ZoteroItem, cond: &serde_json::Value) -> bool {
    let field = cond.get("field").and_then(|v| v.as_str()).unwrap_or("");
    let op =
        cond.get("operator").and_then(|v| v.as_str()).unwrap_or("contains");
    let val =
        cond.get("value").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();

    match field {
        "tag" => {
            let has_tag = item
                .data
                .tags
                .iter()
                .any(|t| t.tag.to_lowercase().contains(&val));
            if op == "is_not" {
                !has_tag
            } else {
                has_tag
            }
        }
        "creator" | "author" => {
            let has_creator = item.data.creators.iter().any(|c| {
                let first = c.first_name.as_deref().unwrap_or("");
                let last = c.last_name.as_deref().unwrap_or("");
                format!("{first} {last}").to_lowercase().contains(&val)
            });
            if op == "is_not" {
                !has_creator
            } else {
                has_creator
            }
        }
        _ => {
            let val_matched = match field {
                "itemType" | "item_type" => item.data.item_type.to_lowercase(),
                "doi" => item.data.doi.as_deref().unwrap_or("").to_lowercase(),
                "citationKey" | "citekey" => item
                    .data
                    .citation_key
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase(),
                "year" | "date" => {
                    item.data.date.as_deref().unwrap_or("").to_lowercase()
                }
                "abstract" | "abstractNote" => item
                    .data
                    .abstract_note
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase(),
                _ => item.data.title.as_deref().unwrap_or("").to_lowercase(),
            };
            match op {
                "equals" => val_matched == val,
                "is_not" => !val_matched.contains(&val),
                _ => val_matched.contains(&val),
            }
        }
    }
}

fn find_duplicate_groups(items: &[ZoteroItem]) -> Vec<serde_json::Value> {
    let mut doi_map: std::collections::BTreeMap<String, Vec<&ZoteroItem>> =
        std::collections::BTreeMap::new();
    let mut title_map: std::collections::BTreeMap<String, Vec<&ZoteroItem>> =
        std::collections::BTreeMap::new();

    for item in items {
        if let Some(ref doi) = item.data.doi {
            let clean_doi = doi.trim().to_lowercase();
            if !clean_doi.is_empty() {
                doi_map.entry(clean_doi).or_default().push(item);
            }
        }
        if let Some(ref title) = item.data.title {
            let clean_title: String = title
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            if clean_title.len() >= 3 {
                title_map.entry(clean_title).or_default().push(item);
            }
        }
    }

    let mut duplicates = Vec::new();
    for (doi, grouped) in doi_map {
        if grouped.len() > 1 {
            duplicates.push(serde_json::json!({
                "reason": "matching_doi",
                "match_key": doi,
                "count": grouped.len(),
                "items": grouped,
            }));
        }
    }
    for (title, grouped) in title_map {
        if grouped.len() > 1 {
            duplicates.push(serde_json::json!({
                "reason": "matching_title",
                "match_key": title,
                "count": grouped.len(),
                "items": grouped,
            }));
        }
    }

    duplicates
}

type CoverageFlags = (bool, bool, bool);

fn coverage_flags(item: &ZoteroItem, children: &[ZoteroItem]) -> CoverageFlags {
    let has_doi =
        item.data.doi.as_deref().is_some_and(|d| !d.trim().is_empty());
    let has_pdf = children.iter().any(|child| {
        child.data.item_type == "attachment"
            && child
                .data
                .content_type
                .as_deref()
                .is_some_and(|ct| ct.contains("pdf"))
    });
    let has_notes = children.iter().any(|child| child.data.item_type == "note");

    (has_doi, has_pdf, has_notes)
}

fn classify_coverage(flags: &[CoverageFlags]) -> serde_json::Value {
    let total_items = flags.len();
    let items_with_doi =
        flags.iter().filter(|(has_doi, _, _)| *has_doi).count();
    let items_with_pdf =
        flags.iter().filter(|(_, has_pdf, _)| *has_pdf).count();
    let items_with_notes =
        flags.iter().filter(|(_, _, has_notes)| *has_notes).count();

    serde_json::json!({
        "total_items": total_items,
        "items_with_doi": items_with_doi,
        "doi_coverage_pct": compute_percentage(items_with_doi, total_items),
        "items_with_pdf": items_with_pdf,
        "pdf_coverage_pct": compute_percentage(items_with_pdf, total_items),
        "items_with_notes": items_with_notes,
        "notes_coverage_pct": compute_percentage(items_with_notes, total_items),
    })
}

fn compute_percentage(count: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let count = f64::from(u32::try_from(count).map_or(u32::MAX, |n| n));
    let total = f64::from(u32::try_from(total).map_or(u32::MAX, |n| n));
    (count / total) * 100.0
}

fn format_annotations_section(children: &[ZoteroItem]) -> String {
    use std::fmt::Write as _;

    let mut md = String::from("## Highlights & Annotations\n\n");
    let mut has_annotations = false;
    for child in children {
        if child.data.item_type == "annotation" {
            has_annotations = true;
            let page =
                child.data.annotation_page_label.as_deref().unwrap_or("?");
            if let Some(ref text) = child.data.annotation_text {
                let _ = writeln!(md, "> \"{text}\" (p. {page})\n");
            }
            if let Some(ref comment) = child.data.annotation_comment {
                let _ = writeln!(md, "**Comment:** {comment}\n");
            }
        }
    }
    if !has_annotations {
        let _ = writeln!(md, "*No annotations found.*\n");
    }
    md
}

fn format_notes_section(item: &ZoteroItem, children: &[ZoteroItem]) -> String {
    use std::fmt::Write as _;

    let mut md = String::from("## Notes\n\n");
    let mut has_notes = false;
    if let Some(ref note) = item.data.note {
        has_notes = true;
        let _ = writeln!(md, "{note}\n");
    }
    for child in children {
        if child.data.item_type == "note" {
            if let Some(ref note) = child.data.note {
                has_notes = true;
                let _ = writeln!(md, "{note}\n");
            }
        }
    }
    if !has_notes {
        let _ = writeln!(md, "*No notes found.*\n");
    }
    md
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::{
        super::client::tests::fixtures::{
            http_response, mock_server, test_state,
        },
        *,
    };

    #[tokio::test]
    async fn get_recent_items_deserializes_correctly() {
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Test Title" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let res = ZoteroClient::new(&state).get_recent_items(10).await.unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res.first().expect("item").key, "ITEM1");
    }

    #[tokio::test]
    async fn get_item_returns_not_found_on_404() {
        let base = mock_server(vec![http_response("404 Not Found", "")]);
        let state = test_state(base, false);

        let err = ZoteroClient::new(&state)
            .get_item("NONEXISTENT")
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::NotFound(_)));
    }

    #[tokio::test]
    async fn get_item_fulltext_returns_empty_when_no_content_field() {
        let base = mock_server(vec![http_response("200 OK", "{}")]);
        let state = test_state(base, false);

        let text =
            ZoteroClient::new(&state).get_item_fulltext("ITEM1").await.unwrap();
        assert_eq!(text, "");
    }

    #[tokio::test]
    async fn search_by_tag_filters_items() {
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Tagged Item", "tags": [{ "tag": "quantum" }] }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let res = ZoteroClient::new(&state)
            .search_by_tag("quantum", 10)
            .await
            .unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res.first().expect("item").key, "ITEM1");
    }

    #[tokio::test]
    async fn search_by_citation_key_matches_extra() {
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Citekey Item", "extra": "Citation Key: smith2020deep" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let res = ZoteroClient::new(&state)
            .search_by_citation_key("smith2020deep")
            .await
            .unwrap();
        assert_eq!(res.expect("item found").key, "ITEM1");
    }

    #[tokio::test]
    async fn search_by_citation_key_matches_native_field() {
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Citekey Item", "citationKey": "smith2020deep" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let res = ZoteroClient::new(&state)
            .search_by_citation_key("smith2020deep")
            .await
            .unwrap();
        assert_eq!(res.expect("item found").key, "ITEM1");
    }

    #[tokio::test]
    async fn search_by_citation_key_native_field_takes_precedence_over_stale_extra()
     {
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": {
                "key": "ITEM1",
                "itemType": "journalArticle",
                "title": "Citekey Item",
                "citationKey": "other2019",
                "extra": "Citation Key: smith2020deep"
            }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let res = ZoteroClient::new(&state)
            .search_by_citation_key("smith2020deep")
            .await
            .unwrap();
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn advanced_search_filters_items_by_conditions() {
        let items = json!([
            {
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Computing" }
            },
            {
                "key": "ITEM2",
                "version": 1,
                "data": { "key": "ITEM2", "itemType": "book", "title": "Classical Mechanics" }
            }
        ]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let conds = vec![
            json!({"field": "title", "operator": "contains", "value": "quantum"}),
        ];
        let res =
            ZoteroClient::new(&state).advanced_search(conds, 10).await.unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res.first().expect("item").key, "ITEM1");
    }

    #[tokio::test]
    async fn advanced_search_filters_items_by_citation_key() {
        let items = json!([
            {
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Computing", "citationKey": "smith2020deep" }
            },
            {
                "key": "ITEM2",
                "version": 1,
                "data": { "key": "ITEM2", "itemType": "book", "title": "Classical Mechanics", "citationKey": "jones2019classical" }
            }
        ]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let conds = vec![
            json!({"field": "citationKey", "operator": "equals", "value": "smith2020deep"}),
        ];
        let res =
            ZoteroClient::new(&state).advanced_search(conds, 10).await.unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res.first().expect("item").key, "ITEM1");
    }

    #[tokio::test]
    async fn get_library_coverage_computes_metrics() {
        let items = json!([
            {
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Paper 1", "doi": "10.1234/test" }
            }
        ]);
        let children = json!([
            {
                "key": "ATT1",
                "version": 1,
                "data": { "key": "ATT1", "itemType": "attachment", "contentType": "application/pdf" }
            },
            {
                "key": "NOTE1",
                "version": 1,
                "data": { "key": "NOTE1", "itemType": "note", "note": "some note" }
            }
        ]);
        let base = mock_server(vec![
            http_response("200 OK", &items.to_string()),
            http_response("200 OK", &children.to_string()),
        ]);
        let state = test_state(base, false);

        let coverage =
            ZoteroClient::new(&state).get_library_coverage(None).await.unwrap();
        assert_eq!(
            coverage.get("total_items").and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            coverage.get("items_with_doi").and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            coverage.get("items_with_pdf").and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            coverage
                .get("items_with_notes")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
    }

    #[tokio::test]
    async fn synthesize_annotations_highlights_and_notes_to_markdown() {
        let item = json!({
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Quantum Physics Paper", "doi": "10.1234/q1" }
        });
        let children = json!([
            {
                "key": "ANN1",
                "version": 1,
                "data": {
                    "key": "ANN1",
                    "itemType": "annotation",
                    "annotationPageLabel": "12",
                    "annotationText": "Key discovery in quantum state",
                    "annotationComment": "Important finding"
                }
            },
            {
                "key": "NOTE1",
                "version": 1,
                "data": {
                    "key": "NOTE1",
                    "itemType": "note",
                    "note": "Summary of paper methods"
                }
            }
        ]);
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &children.to_string()),
        ]);
        let state = test_state(base, false);

        let md = ZoteroClient::new(&state)
            .synthesize_annotations("ITEM1")
            .await
            .unwrap();
        assert!(md.contains("# Annotations & Notes: Quantum Physics Paper"));
        assert!(md.contains("> \"Key discovery in quantum state\" (p. 12)"));
        assert!(md.contains("**Comment:** Important finding"));
        assert!(md.contains("Summary of paper methods"));
    }
    #[tokio::test]
    async fn search_collections_returns_matching_items() {
        let collections = json!([
            { "key": "C1", "version": 1, "data": { "key": "C1", "name": "Quantum Physics" } },
            { "key": "C2", "version": 1, "data": { "key": "C2", "name": "Quantum Mechanics" } },
            { "key": "C3", "version": 1, "data": { "key": "C3", "name": "Biology" } }
        ]);
        let base = mock_server(vec![http_response(
            "200 OK",
            &collections.to_string(),
        )]);
        let state = test_state(base, false);

        let results = ZoteroClient::new(&state)
            .search_collections("quantum")
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn find_duplicates_detects_duplicates_by_title_and_doi() {
        let items = json!([
            {
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Unique Article Title", "doi": "10.1234/unique" }
            },
            {
                "key": "ITEM2",
                "version": 1,
                "data": { "key": "ITEM2", "itemType": "journalArticle", "title": "Unique Article Title", "doi": "10.1234/unique" }
            }
        ]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let duplicates =
            ZoteroClient::new(&state).find_duplicates(None).await.unwrap();
        assert_eq!(duplicates.len(), 2);
    }

    #[tokio::test]
    async fn list_tags_returns_tag_names() {
        let tags = json!([{"tag": "quantum", "meta": {"numItems": 3}}]);
        let base =
            mock_server(vec![http_response("200 OK", &tags.to_string())]);
        let state = test_state(base, false);

        let result = ZoteroClient::new(&state).list_tags(100).await.unwrap();
        assert_eq!(result, vec!["quantum".to_owned()]);
    }

    #[tokio::test]
    async fn get_unfiled_items_filters_out_items_with_collections() {
        let items = json!([
            {
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "itemType": "journalArticle", "title": "Filed Item", "collections": ["COL1"] }
            },
            {
                "key": "ITEM2",
                "version": 1,
                "data": { "key": "ITEM2", "itemType": "journalArticle", "title": "Unfiled Item", "collections": [] }
            }
        ]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let state = test_state(base, false);

        let result =
            ZoteroClient::new(&state).get_unfiled_items(100).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.first().expect("item").key, "ITEM2");
    }
}
