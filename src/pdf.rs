//! Local PDF text extraction backing the `zotero_read_pdf_pages` tool.
//!
//! Wraps the [`pdf_extract`] crate to pull plain text out of a PDF file on
//! disk, with optional page-range filtering.

use std::path::Path;

use crate::errors::ZoteroMcpError;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Function called by spawn_blocking in MCP tool handler"
    )
)]
/// Extracts text from the PDF at `file_path`, optionally restricted to
/// `page_numbers`.
///
/// `pdf-extract` delimits pages with a form-feed (`\x0C`) character in its
/// output. When `page_numbers` is `Some`, only the matching 1-based pages
/// are kept, rejoined with `\x0C`; when it's `None`, the full extracted text
/// is returned unmodified. An empty `page_numbers` slice returns an empty
/// string.
///
/// # Errors
///
/// - [`NotFound`] if `file_path` does not exist
/// - [`PdfExtract`] if `pdf-extract` fails to parse the file
///
/// [`NotFound`]: ZoteroMcpError::NotFound
/// [`PdfExtract`]: ZoteroMcpError::PdfExtract
pub(crate) fn extract_pdf_pages(
    file_path: &Path,
    page_numbers: Option<&[usize]>,
) -> Result<String, ZoteroMcpError> {
    if !file_path.exists() {
        return Err(ZoteroMcpError::NotFound(format!(
            "PDF file not found: {}",
            file_path.display()
        )));
    }
    let full_text = pdf_extract::extract_text(file_path)
        .map_err(|e| ZoteroMcpError::PdfExtract(e.to_string()))?;

    Ok(filter_pages(&full_text, page_numbers))
}

/// Filters form-feed-delimited `full_text` down to `page_numbers` (1-based).
///
/// Pages extracted by `pdf-extract` are delimited by a form-feed (`\x0C`)
/// character. `page_numbers` of `None` returns `full_text` unmodified; an
/// empty (but `Some`) slice returns an empty string; out-of-range page
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
            let result = extract_pdf_pages(path, None);

            // Assert
            assert!(matches!(result, Err(ZoteroMcpError::NotFound(_))));
        }

        #[test]
        fn returns_pdf_extract_error_when_file_is_not_a_valid_pdf() {
            // Arrange
            let mut temp = tempfile::NamedTempFile::new().unwrap();
            temp.write_all(b"Not a real PDF file header").unwrap();

            // Act
            let result = extract_pdf_pages(temp.path(), None);

            // Assert
            assert!(matches!(result, Err(ZoteroMcpError::PdfExtract(_))));
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
            let with_one_valid = filter_pages(text, Some(&[1, 99]));
            let with_none_valid = filter_pages(text, Some(&[99]));

            // Assert
            assert_eq!(with_one_valid, "page one");
            assert_eq!(with_none_valid, "");
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
}
