use crate::errors::ZoteroMcpError;
use std::path::Path;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Function called by spawn_blocking in MCP tool handler"
    )
)]
pub fn extract_pdf_pages(
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

    // Pages extracted by pdf-extract are typically delimited by form-feed '\x0C'
    if let Some(pages) = page_numbers {
        if pages.is_empty() {
            return Ok(String::new());
        }

        let mut output = String::new();
        for (idx, page_content) in full_text.split('\x0C').enumerate() {
            let page_num = idx + 1;
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
    use super::*;
    use std::io::Write;
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
