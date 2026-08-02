//! Analytics, duplicate detection, and annotation synthesis operations.
//!
//! Adds [`ZoteroClient`] methods for duplicate scans, library coverage metrics,
//! and formatted annotation summaries.
//!
//! # Key operations
//!
//! - [`ZoteroClient::find_duplicates`]: groups potential [`DuplicateGroup`]
//!   matches by DOI or normalized title.
//! - [`ZoteroClient::get_library_coverage`]: computes [`LibraryCoverage`]
//!   statistics for PDF, DOI, and note availability.
//! - [`ZoteroClient::synthesize_annotations`]: renders item annotations and
//!   notes as Markdown.

use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        client::{ZoteroClient, add_pagination},
        models::{CollectionKey, ItemKey, ItemType, ZoteroItem},
        search::PaginationInfo,
    },
};

/// Type of duplication criterion matched.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DuplicateType {
    Doi,
    Title,
}

/// Group of items identified as potential duplicates.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DuplicateGroup {
    pub(crate) match_type: DuplicateType,
    pub(crate) match_value: String,
    pub(crate) item_keys: Vec<ItemKey>,
}

/// Coverage indicators for a single library item.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "domain model tracks 3 distinct boolean flags"
)]
pub(crate) struct ItemCoverageFlags {
    pub(crate) has_pdf: bool,
    pub(crate) has_doi: bool,
    pub(crate) has_notes: bool,
}

/// Library or collection statistics for PDF, DOI, and note coverage.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LibraryCoverage {
    pub(crate) total_items: usize,
    pub(crate) with_pdf: usize,
    pub(crate) with_doi: usize,
    pub(crate) with_notes: usize,
    pub(crate) pdf_percentage: f64,
    pub(crate) doi_percentage: f64,
    pub(crate) notes_percentage: f64,
}

/// One page of library coverage results plus pagination metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LibraryCoveragePage {
    pub(crate) coverage: LibraryCoverage,
    pub(crate) pagination: PaginationInfo,
}

impl ZoteroClient<'_> {
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
        collection_key: Option<&CollectionKey>,
    ) -> Result<Vec<DuplicateGroup>, ZoteroMcpError> {
        let items = if let Some(col) = collection_key {
            self.get_collection_items(col).await?
        } else {
            self.get_all_items().await?
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
        collection_key: Option<&CollectionKey>,
        offset: usize,
        limit: usize,
    ) -> Result<LibraryCoveragePage, ZoteroMcpError> {
        let base = match collection_key {
            Some(col) => format!(
                "{}/users/0/collections/{}/items",
                self.state.zotero_api_url, col
            ),
            None => format!(
                "{}/users/0/items?itemType=-note&sort=dateModified&\
                 direction=desc",
                self.state.zotero_api_url
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
        item_key: &ItemKey,
    ) -> Result<String, ZoteroMcpError> {
        use std::fmt::Write as _;

        let item = self.get_item(item_key).await?;
        let children =
            self.get_item_children(item_key).await.unwrap_or_default();

        let mut md = String::new();
        let title = item.data.title.as_deref().unwrap_or(item_key.as_str());
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

#[allow(
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

/// Formats PDF annotations attached to child items into a Markdown section.
fn format_annotations_section(children: &[ZoteroItem]) -> String {
    use std::fmt::Write as _;

    let mut section = String::new();
    let annotations: Vec<_> = children
        .iter()
        .filter(|c| c.data.item_type == ItemType::Annotation)
        .collect();

    if annotations.is_empty() {
        return section;
    }

    let _ = writeln!(section, "## PDF Annotations\n");
    for ann in annotations {
        let text = ann.data.annotation_text.as_deref().unwrap_or("");
        let comment = ann.data.annotation_comment.as_deref().unwrap_or("");
        let page = ann.data.annotation_page_label.as_deref().unwrap_or("");

        if !text.is_empty() {
            if page.is_empty() {
                let _ = writeln!(section, "> \"{text}\"");
            } else {
                let _ = writeln!(section, "> \"{text}\" (p. {page})");
            }
        }
        if !comment.is_empty() {
            let _ = writeln!(section, "Comment: {comment}");
        }
        section.push('\n');
    }

    section
}

/// Formats child notes and standalone item notes into a Markdown section.
fn format_notes_section(item: &ZoteroItem, children: &[ZoteroItem]) -> String {
    use std::fmt::Write as _;

    let mut section = String::new();
    let child_notes: Vec<_> = children
        .iter()
        .filter(|c| c.data.item_type == ItemType::Note)
        .collect();

    if item.data.item_type == ItemType::Note {
        if let Some(ref note) = item.data.note {
            let _ = writeln!(section, "## Note Content\n\n{note}\n");
        }
    }

    if !child_notes.is_empty() {
        let _ = writeln!(section, "## Child Notes\n");
        for (idx, note_item) in child_notes.iter().enumerate() {
            if let Some(ref body) = note_item.data.note {
                let num = idx.saturating_add(1);
                let _ = writeln!(section, "### Note {num}\n\n{body}\n");
            }
        }
    }

    section
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zotero::models::{
        AnnotationType, LibraryVersion, ZoteroItemData,
    };

    mod compute_percentage {
        use pretty_assertions::assert_eq;

        use super::*;
        #[test]
        #[allow(clippy::float_cmp, reason = "exact float percentages in test")]
        fn returns_percentage_ratio_for_given_counts() {
            assert_eq!(compute_percentage(1, 2), 50.0);
            assert_eq!(compute_percentage(3, 4), 75.0);
        }

        #[test]
        #[allow(clippy::float_cmp, reason = "exact float percentages in test")]
        fn returns_zero_when_total_is_zero() {
            assert_eq!(compute_percentage(0, 0), 0.0);
            assert_eq!(compute_percentage(5, 0), 0.0);
        }
    }
    mod duplicate_groups {
        use pretty_assertions::assert_eq;

        use super::*;

        fn make_item(
            key: &str,
            title: Option<&str>,
            doi: Option<&str>,
        ) -> ZoteroItem {
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

            let groups = find_duplicate_groups(&vec![item1, item2]);
            assert_eq!(groups.len(), 1);
            let first_group = groups.first().expect("group exists");
            assert_eq!(first_group.match_type, DuplicateType::Doi);
            assert_eq!(first_group.match_value, "10.1234/xyz");
            assert_eq!(first_group.item_keys, vec![
                ItemKey::from("ITEM0001"),
                ItemKey::from("ITEM0002")
            ]);
        }

        #[test]
        fn groups_items_by_matching_title_case_insensitively() {
            let item1 =
                make_item("ITEM0001", Some("Quantum Computing Advances"), None);
            let item2 =
                make_item("ITEM0002", Some("quantum computing advances"), None);

            let groups = find_duplicate_groups(&vec![item1, item2]);
            assert_eq!(groups.len(), 1);
            let first_group = groups.first().expect("group exists");
            assert_eq!(first_group.match_type, DuplicateType::Title);
            assert_eq!(first_group.match_value, "quantum computing advances");
        }

        #[test]
        fn returns_empty_when_no_duplicates_exist() {
            let item1 =
                make_item("ITEM0001", Some("Paper 1"), Some("10.1000/1"));
            let item2 =
                make_item("ITEM0002", Some("Paper 2"), Some("10.1000/2"));

            let groups = find_duplicate_groups(&vec![item1, item2]);
            assert!(groups.is_empty());
        }
    }

    mod coverage {
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
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
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
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
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
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
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
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("NOTE0001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::Note,
                    note: Some("Child note".to_owned()),
                    ..Default::default()
                },
            };

            let flags = coverage_flags(&item, &[attachment, note]);
            assert!(flags.has_doi);
            assert!(flags.has_pdf);
            assert!(flags.has_notes);
        }

        #[test]
        #[allow(clippy::float_cmp, reason = "exact float percentages in test")]
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
            assert_eq!(coverage.pdf_percentage, 50.0);
            assert_eq!(coverage.doi_percentage, 100.0);
            assert_eq!(coverage.notes_percentage, 50.0);
        }

        #[test]
        fn coverage_pagination_uses_server_total_for_has_more() {
            let pagination = coverage_pagination(0, 2, 2, Some(3));

            assert_eq!(pagination.total, 3);
            assert!(pagination.has_more);
        }

        #[test]
        fn coverage_pagination_clamps_offset_when_total_is_known() {
            let pagination = coverage_pagination(99, 10, 0, Some(3));

            assert_eq!(pagination.offset, 3);
        }

        #[test]
        fn library_coverage_page_classifies_only_selected_items() {
            let selected = [
                item("ITEM0001", ItemType::JournalArticle, Some("10.1000/1")),
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
            let page =
                classify_coverage_page(&selected, &children_by_idx, pagination);

            assert_eq!(page.coverage.total_items, 2);
            assert_eq!(page.coverage.with_doi, 1);
            assert_eq!(page.coverage.with_notes, 1);
            assert_eq!(page.pagination.total, 3);
            assert!(page.pagination.has_more);
        }
    }

    mod formatting {
        use super::*;
        #[test]
        fn formats_annotations_section_with_highlights_and_notes() {
            let annotation = ZoteroItem {
                key: ItemKey::from("ANN00001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("ANN00001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::Annotation,
                    annotation_type: Some(AnnotationType::Highlight),
                    annotation_text: Some("Important concept".to_owned()),
                    annotation_comment: Some("Check this out".to_owned()),
                    annotation_page_label: Some("42".to_owned()),
                    ..Default::default()
                },
            };

            let result = format_annotations_section(&vec![annotation]);
            assert!(result.contains("## PDF Annotations"));
            assert!(result.contains("> \"Important concept\" (p. 42)"));
            assert!(result.contains("Comment: Check this out"));
        }

        #[test]
        fn formats_standalone_note_section() {
            let note_item = ZoteroItem {
                key: ItemKey::from("NOTE0001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("NOTE0001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::Note,
                    note: Some("<p>Main note text</p>".to_owned()),
                    ..Default::default()
                },
            };

            let result = format_notes_section(&note_item, &[]);
            assert!(result.contains("## Note Content"));
            assert!(result.contains("<p>Main note text</p>"));
        }

        #[test]
        fn formats_child_notes_section() {
            let main_item = ZoteroItem {
                key: ItemKey::from("ITEM0001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("ITEM0001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::JournalArticle,
                    ..Default::default()
                },
            };
            let child_note = ZoteroItem {
                key: ItemKey::from("NOTE0001"),
                version: LibraryVersion(1),
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from("NOTE0001"),
                    version: LibraryVersion(1),
                    item_type: ItemType::Note,
                    note: Some("<p>Child note text</p>".to_owned()),
                    ..Default::default()
                },
            };

            let result = format_notes_section(&main_item, &vec![child_note]);
            assert!(result.contains("## Child Notes"));
            assert!(result.contains("### Note 1"));
            assert!(result.contains("<p>Child note text</p>"));
        }
    }

    mod scan_pagination {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::zotero::client::ZoteroClient;

        fn item_json(key: &str, title: &str) -> String {
            format!(
                r#"{{"key":"{key}","version":1,"data":{{"key":"{key}","version":1,"itemType":"journalArticle","title":"{title}"}}}}"#
            )
        }

        fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        fn mock_server(responses: Vec<String>) -> String {
            use std::{
                io::{Read, Write},
                net::TcpListener,
            };
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) = listener.accept().expect("accept");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }

        fn test_state(zotero_api_url: String) -> crate::state::AppState {
            crate::state::AppState {
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: false,
                ..crate::state::AppState::from_env()
            }
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

            let base = mock_server(vec![
                http_response("200 OK", &page1),
                http_response("200 OK", &page2),
            ]);
            let state = test_state(base);

            let groups =
                ZoteroClient::new(&state).find_duplicates(None).await.unwrap();

            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].match_type, DuplicateType::Title);
            assert_eq!(groups[0].item_keys.len(), 2);
        }
    }
}
