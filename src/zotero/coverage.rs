//! Library and collection coverage metrics for the Zotero Local HTTP API.
//!
//! Computes aggregate and per-item availability statistics for PDF attachments,
//! DOIs, and child notes across a user library or specific collection. Called
//! by MCP tool handlers in `crate::mcp::zotero::coverage`.
//!
//! # Main Types
//!
//! - [`LibraryCoverage`]: Aggregate PDF, DOI, and note coverage statistics.
//! - [`LibraryCoveragePage`]: One page of coverage results with pagination
//!   metadata.
//! - [`ItemCoverageFlags`]: Per-item PDF, DOI, and note availability
//!   indicators.
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::state::AppState;
//! # use zotero_mcp_rs::zotero::client::ZoteroClient;
//! # async fn example(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
//! let client = ZoteroClient::new(&state);
//! let page = client.get_library_coverage(None, 0, 50).await?;
//! println!("Total items: {}", page.coverage.total_items);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        CollectionKey, ItemType, ZoteroItem,
        client::{ZoteroClient, add_pagination},
        search::PaginationInfo,
    },
};

/// Coverage indicators for a single library item.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[allow(
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
pub(crate) struct LibraryCoveragePage {
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
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zotero::{ItemKey, LibraryVersion, objects::ZoteroItemData};

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

            let children = vec![attachment, note];
            let flags = coverage_flags(&item, &children);
            assert!(flags.has_doi, "expected DOI flag for nonblank DOI");
            assert!(flags.has_pdf, "expected PDF flag for PDF attachment");
            assert!(flags.has_notes, "expected notes flag for child note");
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
        #[allow(clippy::float_cmp, reason = "exact zero percentages in test")]
        fn classify_coverage_returns_zeroed_stats_for_empty_input() {
            let coverage = classify_coverage(&[]);

            assert_eq!(coverage.total_items, 0);
            assert_eq!(coverage.with_pdf, 0);
            assert_eq!(coverage.with_doi, 0);
            assert_eq!(coverage.with_notes, 0);
            assert_eq!(coverage.pdf_percentage, 0.0);
            assert_eq!(coverage.doi_percentage, 0.0);
            assert_eq!(coverage.notes_percentage, 0.0);
        }

        #[test]
        fn coverage_flags_ignores_blank_doi_and_non_pdf_attachment() {
            let item = item("ITEM0001", ItemType::JournalArticle, Some("  "));
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
                    content_type: Some("text/plain".to_owned()),
                    ..Default::default()
                },
            };

            let children = vec![attachment];
            let flags = coverage_flags(&item, &children);

            assert!(!flags.has_doi, "blank DOI must not count as coverage");
            assert!(!flags.has_pdf, "non-PDF attachment must not count as PDF");
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
            assert!(pagination.has_more, "known total exceeds returned page");
        }

        #[test]
        fn coverage_pagination_clamps_offset_when_total_is_known() {
            let pagination = coverage_pagination(99, 10, 0, Some(3));

            assert_eq!(pagination.offset, 3);
        }

        #[test]
        fn library_coverage_page_classifies_only_selected_items() {
            let selected = vec![
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
            assert!(
                page.pagination.has_more,
                "known total exceeds selected page"
            );
        }
    }
}
