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

    // Pages extracted by pdf-extract are typically delimited by form-feed
    // '\x0C'
    if let Some(pages) = page_numbers {
        if pages.is_empty() {
            return Ok(String::new());
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
        Ok(output)
    } else {
        Ok(full_text)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    #[test]
    fn test_missing_pdf_file() {
        let path = Path::new("/nonexistent/file.pdf");
        let result = extract_pdf_pages(path, None);
        assert!(matches!(result, Err(ZoteroMcpError::NotFound(_))));
    }

    #[test]
    fn test_invalid_pdf_file_error() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        temp.write_all(b"Not a real PDF file header").unwrap();
        let result = extract_pdf_pages(temp.path(), None);
        assert!(matches!(result, Err(ZoteroMcpError::PdfExtract(_))));
    }
}
