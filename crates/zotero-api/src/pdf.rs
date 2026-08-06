//! Local PDF extraction module.
//!
//! Extracts plain text (with optional page filtering) and bookmark outlines
//! (tables of contents) from local PDF files.
//!
//! # Main Functions
//!
//! - [`extract_pdf_pages`]: Extract plain text from PDF pages.
//! - [`extract_pdf_outline`]: Read bookmark outline and table of contents
//!   entries.
//!
//! ```no_run
//! use std::path::Path;
//!
//! use zotero_api::pdf::extract_pdf_pages;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let path = Path::new("/path/to/document.pdf");
//! let text = extract_pdf_pages(path, Some(&[1, 2]), 50 * 1024 * 1024)?;
//! # Ok(())
//! # }
//! ```

use std::path::Path;

use crate::errors::ZoteroApiError;
/// Extracts text from the PDF at `file_path`, optionally restricted to
/// `page_numbers`.
///
/// `pdf-extract` delimits pages with a form-feed (`\x0C`) character in its
/// output. When `page_numbers` is [`Some`], only the matching 1-based pages are
/// kept, rejoined with `\x0C`; when it's [`None`], the full extracted text is
/// returned unmodified. An empty `page_numbers` slice returns an empty string.
///
/// # Arguments
///
/// * `file_path` - Path to the PDF file on disk.
/// * `page_numbers` - Optional 1-based page numbers to extract.
/// * `max_pdf_bytes` - Maximum allowed PDF size in bytes.
/// # Errors
///
/// - [`ZoteroApiError::NotFound`]: If `file_path` does not exist.
/// - [`ZoteroApiError::InputRejected`]: If `file_path` is larger than
///   `max_pdf_bytes`.
/// - [`ZoteroApiError::PdfExtract`]: If text extraction fails.
#[inline]
pub fn extract_pdf_pages(
    file_path: &Path,
    page_numbers: Option<&[usize]>,
    max_pdf_bytes: u64,
) -> Result<String, ZoteroApiError> {
    if !file_path.exists() {
        return Err(ZoteroApiError::NotFound(format!(
            "PDF file not found: {}",
            file_path.display()
        )));
    }
    let len = std::fs::metadata(file_path)?.len();
    if len > max_pdf_bytes {
        return Err(ZoteroApiError::InputRejected(format!(
            "PDF file {} exceeds {max_pdf_bytes} bytes",
            file_path.display()
        )));
    }
    let full_text = pdf_extract::extract_text(file_path)
        .map_err(|e| ZoteroApiError::PdfExtract(e.to_string()))?;

    Ok(filter_pages(&full_text, page_numbers))
}

/// Filters form-feed-delimited `full_text` down to `page_numbers` (1-based).
///
/// Pages extracted by `pdf-extract` are delimited by a form-feed (`\x0C`)
/// character. `page_numbers` of [`None`] returns `full_text` unmodified; an
/// empty (but [`Some`]) slice returns an empty string; out-of-range page
/// numbers are silently skipped.
fn filter_pages(full_text: &str, page_numbers: Option<&[usize]>) -> String {
    let Some(pages) = page_numbers else {
        return full_text.to_owned();
    };
    if pages.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    for (idx, page_content) in full_text.split('\x0C').enumerate() {
        let page_num = idx.saturating_add(1);
        if pages.contains(&page_num) {
            if !output.is_empty() {
                output.push('\x0C');
            }
            output.push_str(page_content);
        }
    }
    output
}

/// Extracts the bookmark outline (table of contents) from the PDF at
/// `file_path` as a flat list of entries with 1-based `page` numbers.
///
/// A PDF without bookmarks yields an empty [`Vec`]; `get_toc()` reporting no
/// outline is treated as no-op rather than an error.
///
/// # Errors
///
/// - [`ZoteroApiError::NotFound`]: If `file_path` does not exist.
/// - [`ZoteroApiError::InputRejected`]: If `file_path` is larger than
///   `max_pdf_bytes`.
/// - [`ZoteroApiError::PdfExtract`]: If outline extraction fails.
#[inline]
pub fn extract_pdf_outline(
    file_path: &Path,
    max_pdf_bytes: u64,
) -> Result<Vec<PdfOutlineEntry>, ZoteroApiError> {
    if !file_path.exists() {
        return Err(ZoteroApiError::NotFound(format!(
            "PDF file not found: {}",
            file_path.display()
        )));
    }
    let len = std::fs::metadata(file_path)?.len();
    if len > max_pdf_bytes {
        return Err(ZoteroApiError::InputRejected(format!(
            "PDF file {} exceeds {max_pdf_bytes} bytes",
            file_path.display()
        )));
    }
    let doc = lopdf::Document::load(file_path)
        .map_err(|e| ZoteroApiError::PdfExtract(e.to_string()))?;
    // ponytail: any get_toc() failure → empty outline; load already validated
    // the file, and a corrupt outline silently becomes "no outline"
    Ok(doc
        .get_toc()
        .map(|toc| toc.toc)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| PdfOutlineEntry {
            level: entry.level,
            title: entry.title,
            page: entry.page,
        })
        .collect())
}

/// A single entry in a PDF document outline (table of contents).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PdfOutlineEntry {
    /// 1-based nesting level of the outline entry (1 for top-level chapters).
    pub(crate) level: usize,
    /// Display title of the section or bookmark.
    pub(crate) title: String,
    /// 1-based page number where the outline entry points.
    pub(crate) page: usize,
}

/// Writes a minimal 2-page PDF with a 3-entry outline to `path`.
#[cfg(any(test, feature = "test-util"))]
#[inline]
#[expect(
    clippy::missing_panics_doc,
    reason = "test-only fixture writer; panicking on setup failure is intended"
)]
pub fn write_pdf_with_outline(path: &Path) {
    use lopdf::{Bookmark, Document, Object, Stream, dictionary};

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page1 = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    });
    let page2 = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page1.into(), page2.into()],
            "Count" => 2,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    doc.add_bookmark(
        Bookmark::new("Chapter 1".to_owned(), [0.0, 0.0, 0.0], 0, page1),
        None,
    );
    let ch2 = doc.add_bookmark(
        Bookmark::new("Chapter 2".to_owned(), [0.0, 0.0, 0.0], 0, page2),
        None,
    );
    doc.add_bookmark(
        Bookmark::new("Section 2.1".to_owned(), [0.0, 0.0, 0.0], 0, page2),
        Some(ch2),
    );
    #[expect(
        clippy::expect_used,
        reason = "test-only fixture writer; panicking on setup failure is \
                  intended"
    )]
    let outline_id = doc.build_outline().expect("build outline");
    #[expect(
        clippy::expect_used,
        reason = "test-only fixture writer; panicking on setup failure is \
                  intended"
    )]
    let catalog = doc.catalog_mut().expect("catalog");
    catalog.set(b"Outlines", outline_id);
    #[expect(
        clippy::expect_used,
        reason = "test-only fixture writer; panicking on setup failure is \
                  intended"
    )]
    doc.save(path).expect("save pdf");
}

/// Writes a minimal valid 1-page PDF with no outline to `path`.
#[cfg(any(test, feature = "test-util"))]
#[inline]
#[expect(
    clippy::missing_panics_doc,
    reason = "test-only fixture writer; panicking on setup failure is intended"
)]
pub fn write_pdf_without_outline(path: &Path) {
    use lopdf::{Document, Object, Stream, dictionary};

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page1 = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page1.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    #[expect(
        clippy::expect_used,
        reason = "test-only fixture writer; panicking on setup failure is \
                  intended"
    )]
    doc.save(path).expect("save pdf");
}

#[cfg(test)]
mod tests {
    use super::*;

    mod extract_pdf_pages {
        use std::io::Write;

        use super::*;

        #[test]
        fn returns_not_found_error_when_file_is_missing() {
            // Arrange
            let path = Path::new("/nonexistent/file.pdf");

            // Act
            let result = extract_pdf_pages(path, None, 50 * 1024 * 1024);

            // Assert
            assert!(matches!(result, Err(ZoteroApiError::NotFound(_))));
        }

        #[test]
        fn returns_pdf_extract_error_when_file_is_not_a_valid_pdf() {
            // Arrange
            let mut temp = tempfile::NamedTempFile::new().unwrap();
            temp.write_all(b"Not a real PDF file header").unwrap();

            // Act
            let result = extract_pdf_pages(temp.path(), None, 50 * 1024 * 1024);

            // Assert
            assert!(matches!(result, Err(ZoteroApiError::PdfExtract(_))));
        }

        #[test]
        fn rejects_file_larger_than_max_before_parsing() {
            // Arrange
            let mut temp =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            temp.write_all(b"more").unwrap();

            // Act
            let result = extract_pdf_pages(temp.path(), None, 3);

            // Assert
            assert!(matches!(result, Err(ZoteroApiError::InputRejected(_))));
        }
    }

    mod filter_pages {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn returns_full_text_when_page_numbers_is_none() {
            // Arrange
            let text = "page one\x0Cpage two\x0Cpage three";

            // Act
            let result = filter_pages(text, None);

            // Assert
            assert_eq!(result, text);
        }

        #[test]
        fn returns_empty_string_when_page_numbers_is_empty() {
            // Arrange
            let text = "page one\x0Cpage two";

            // Act
            let result = filter_pages(text, Some(&[]));

            // Assert
            assert_eq!(result, "");
        }

        #[test]
        fn preserves_document_order_regardless_of_requested_order() {
            // Arrange
            let text = "page one\x0Cpage two\x0Cpage three";

            // Act
            let result = filter_pages(text, Some(&[3, 1]));

            // Assert
            assert_eq!(result, "page one\x0Cpage three");
        }

        #[test]
        fn skips_out_of_range_page_numbers() {
            // Arrange
            let text = "page one\x0Cpage two";

            // Act
            let one_valid_res = filter_pages(text, Some(&[1, 99]));
            let none_valid_res = filter_pages(text, Some(&[99]));

            // Assert
            assert_eq!(one_valid_res, "page one");
            assert_eq!(none_valid_res, "");
        }

        #[test]
        fn returns_whole_text_for_page_one_of_an_undelimited_document() {
            // Arrange
            let text = "only page";

            // Act
            let result = filter_pages(text, Some(&[1]));

            // Assert
            assert_eq!(result, "only page");
        }

        #[test]
        fn returns_empty_string_for_out_of_range_page_of_an_undelimited_document()
         {
            // Arrange
            let text = "only page";

            // Act
            let result = filter_pages(text, Some(&[2]));

            // Assert
            assert_eq!(result, "");
        }
    }

    mod extract_pdf_outline {
        use std::io::Write;

        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn returns_not_found_error_when_file_is_missing() {
            // Arrange
            let path = Path::new("/nonexistent/file.pdf");

            // Act
            let result = extract_pdf_outline(path, 50 * 1024 * 1024);

            // Assert
            assert!(matches!(result, Err(ZoteroApiError::NotFound(_))));
        }

        #[test]
        fn rejects_file_larger_than_max_before_parsing() {
            // Arrange
            let mut temp =
                tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
            temp.write_all(b"more").unwrap();

            // Act
            let result = extract_pdf_outline(temp.path(), 3);

            // Assert
            assert!(matches!(result, Err(ZoteroApiError::InputRejected(_))));
        }

        #[test]
        fn returns_pdf_extract_error_when_file_is_not_a_valid_pdf() {
            // Arrange
            let mut temp = tempfile::NamedTempFile::new().unwrap();
            temp.write_all(b"Not a real PDF file header").unwrap();

            // Act
            let result = extract_pdf_outline(temp.path(), 50 * 1024 * 1024);

            // Assert
            assert!(matches!(result, Err(ZoteroApiError::PdfExtract(_))));
        }

        #[test]
        fn returns_empty_outline_when_pdf_has_no_bookmarks() {
            // Arrange
            let dir = tempfile::TempDir::new().unwrap();
            let pdf = dir.path().join("plain.pdf");
            write_pdf_without_outline(&pdf);

            // Act
            let result = extract_pdf_outline(&pdf, 50 * 1024 * 1024).unwrap();

            // Assert
            assert!(result.is_empty());
        }

        #[test]
        fn returns_entries_with_level_title_and_page() {
            // Arrange
            let dir = tempfile::TempDir::new().unwrap();
            let pdf = dir.path().join("outline.pdf");
            write_pdf_with_outline(&pdf);

            // Act
            let entries = extract_pdf_outline(&pdf, 50 * 1024 * 1024).unwrap();

            // Assert
            assert_eq!(entries, vec![
                PdfOutlineEntry {
                    level: 1,
                    title: "Chapter 1".to_owned(),
                    page: 1
                },
                PdfOutlineEntry {
                    level: 1,
                    title: "Chapter 2".to_owned(),
                    page: 2
                },
                PdfOutlineEntry {
                    level: 2,
                    title: "Section 2.1".to_owned(),
                    page: 2
                },
            ]);
        }
    }
}
