//! Read-only queries for the Zotero Local HTTP API.

use reqwest::StatusCode;

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        client::ZoteroClient,
        models::{ZoteroCollection, ZoteroItem},
    },
};

impl ZoteroClient<'_> {
    /// Fetches the `limit` most recently modified library items (notes
    /// excluded).
    pub(crate) async fn get_recent_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items?limit={}&sort=dateModified&direction=desc&\
             itemType=-note",
            self.state.zotero_api_url, limit
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Searches library items by `query` (title, creator, year, or
    /// fulltext), optionally scoped to `collection_key`, returning at most
    /// `limit` results. Notes are excluded.
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

        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Fetches the item identified by `item_key`.
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
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let item: ZoteroItem = resp.json().await?;
        Ok(item)
    }

    /// Fetches every collection in the library.
    pub(crate) async fn get_collections(
        &self,
    ) -> Result<Vec<ZoteroCollection>, ZoteroMcpError> {
        let url = format!("{}/users/0/collections", self.state.zotero_api_url);
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let collections: Vec<ZoteroCollection> = resp.json().await?;
        Ok(collections)
    }

    /// Fetches every item inside the collection identified by
    /// `collection_key`.
    pub(crate) async fn get_collection_items(
        &self,
        collection_key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/collections/{}/items",
            self.state.zotero_api_url, collection_key
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Fetches the child items (notes and attachments) of `item_key`.
    pub(crate) async fn get_item_children(
        &self,
        item_key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/children",
            self.state.zotero_api_url, item_key
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Fetches Zotero's indexed fulltext content for `item_key`, or an
    /// empty string if none has been indexed.
    pub(crate) async fn get_item_fulltext(
        &self,
        item_key: &str,
    ) -> Result<String, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/fulltext",
            self.state.zotero_api_url, item_key
        );
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let val: serde_json::Value = resp.json().await?;
        let content = val
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        Ok(content)
    }

    /// Searches items by tag name.
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
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    /// Searches items by citation key in `extra` field or query.
    pub(crate) async fn search_by_citation_key(
        &self,
        citekey: &str,
    ) -> Result<Option<ZoteroItem>, ZoteroMcpError> {
        let items = self.search_items(citekey, None, 20).await?;
        let citekey_lc = citekey.to_lowercase();
        for item in items {
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

    /// Advanced multi-condition structured search over item fields.
    #[allow(
        clippy::cognitive_complexity,
        clippy::excessive_nesting,
        reason = "search matching logic"
    )]
    pub(crate) async fn advanced_search(
        &self,
        conditions: Vec<serde_json::Value>,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let items = self.get_recent_items(100).await?;
        let mut results = Vec::new();

        for item in items {
            let mut matches_all = true;
            for cond in &conditions {
                let field =
                    cond.get("field").and_then(|v| v.as_str()).unwrap_or("");
                let op = cond
                    .get("operator")
                    .and_then(|v| v.as_str())
                    .unwrap_or("contains");
                let val = cond
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();

                let cond_pass = match field {
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
                            format!("{first} {last}")
                                .to_lowercase()
                                .contains(&val)
                        });
                        if op == "is_not" {
                            !has_creator
                        } else {
                            has_creator
                        }
                    }
                    _ => {
                        let val_matched = match field {
                            "itemType" | "item_type" => {
                                item.data.item_type.to_lowercase()
                            }
                            "doi" => item
                                .data
                                .doi
                                .as_deref()
                                .unwrap_or("")
                                .to_lowercase(),
                            "year" | "date" => item
                                .data
                                .date
                                .as_deref()
                                .unwrap_or("")
                                .to_lowercase(),
                            "abstract" | "abstractNote" => item
                                .data
                                .abstract_note
                                .as_deref()
                                .unwrap_or("")
                                .to_lowercase(),
                            _ => item
                                .data
                                .title
                                .as_deref()
                                .unwrap_or("")
                                .to_lowercase(),
                        };
                        match op {
                            "equals" => val_matched == val,
                            "is_not" => !val_matched.contains(&val),
                            _ => val_matched.contains(&val),
                        }
                    }
                };

                if !cond_pass {
                    matches_all = false;
                    break;
                }
            }

            if matches_all {
                results.push(item);
                if results.len() >= limit {
                    break;
                }
            }
        }

        Ok(results)
    }

    /// Computes library or collection coverage statistics (PDF, DOI, Notes).
    pub(crate) async fn get_library_coverage(
        &self,
        collection_key: Option<&str>,
    ) -> Result<serde_json::Value, ZoteroMcpError> {
        let items = match collection_key {
            Some(col) => self.get_collection_items(col).await?,
            None => self.get_recent_items(100).await?,
        };

        let total_items = items.len();
        let mut items_with_doi: usize = 0;
        let mut items_with_pdf: usize = 0;
        let mut items_with_notes: usize = 0;

        for item in &items {
            if item.data.doi.as_deref().is_some_and(|d| !d.trim().is_empty()) {
                items_with_doi = items_with_doi.saturating_add(1);
            }

            if let Ok(children) = self.get_item_children(&item.key).await {
                let has_pdf = children.iter().any(|c| {
                    c.data.item_type == "attachment"
                        && c.data
                            .content_type
                            .as_deref()
                            .is_some_and(|ct| ct.contains("pdf"))
                });
                if has_pdf {
                    items_with_pdf = items_with_pdf.saturating_add(1);
                }

                let has_note =
                    children.iter().any(|c| c.data.item_type == "note");
                if has_note {
                    items_with_notes = items_with_notes.saturating_add(1);
                }
            }
        }

        #[allow(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_lossless,
            reason = "coverage percentage calculation"
        )]
        let total_f = total_items as f64;
        #[allow(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_lossless,
            reason = "coverage percentage calculation"
        )]
        let doi_pct = if total_items > 0 {
            (items_with_doi as f64 / total_f) * 100.0
        } else {
            0.0
        };
        #[allow(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_lossless,
            reason = "coverage percentage calculation"
        )]
        let pdf_pct = if total_items > 0 {
            (items_with_pdf as f64 / total_f) * 100.0
        } else {
            0.0
        };
        #[allow(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_lossless,
            reason = "coverage percentage calculation"
        )]
        let notes_pct = if total_items > 0 {
            (items_with_notes as f64 / total_f) * 100.0
        } else {
            0.0
        };

        Ok(serde_json::json!({
            "total_items": total_items,
            "items_with_doi": items_with_doi,
            "doi_coverage_pct": doi_pct,
            "items_with_pdf": items_with_pdf,
            "pdf_coverage_pct": pdf_pct,
            "items_with_notes": items_with_notes,
            "notes_coverage_pct": notes_pct,
        }))
    }

    /// Extracts and synthesizes annotations and notes into structured Markdown.
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

        let mut has_annotations = false;
        let _ = writeln!(md, "## Highlights & Annotations\n");
        for child in &children {
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

        let mut has_notes = false;
        let _ = writeln!(md, "## Notes\n");
        if let Some(ref note) = item.data.note {
            has_notes = true;
            let _ = writeln!(md, "{note}\n");
        }
        for child in &children {
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

        Ok(md)
    }
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
    async fn fn_filters_items_by_conditions() {
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
}
