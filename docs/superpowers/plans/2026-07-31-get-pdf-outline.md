# get_pdf_outline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `zotero_get_pdf_outline` MCP tool that returns a PDF's bookmarks/table of contents (level, title, page) for an item's PDF or a direct path, reusing `lopdf` already in the dependency tree.

**Architecture:** New pure function `extract_pdf_outline` in `src/pdf.rs` (mirrors `extract_pdf_pages`). Tool impl in `src/mcp/zotero.rs` mirrors `zotero_read_pdf_pages_impl`; the shared security path-resolution (~70 lines) is extracted into one `resolve_pdf_path` helper used by both impls. Registered in `src/mcp/server.rs`.

**Tech Stack:** Rust, `lopdf 0.34` (transitive dep today, becomes direct), rmcp, existing `tempfile`/`pretty_assertions` dev-deps, `cargo nextest` + `hk` lint.

---

### Task 1: Add `lopdf` as a direct dependency

**Files:**
- Modify: `Cargo.toml:12-25`

- [ ] **Step 1:** Add to `[dependencies]` (feature set identical to pdf-extract's, so nothing new compiles):

```toml
lopdf = { version = "0.34", default-features = false, features = ["nom_parser"] }
```

- [ ] **Step 2:** Verify no version/feature change:
`cargo tree -i lopdf` — should show `lopdf v0.34.0` used by both `pdf-extract` and `zotero-mcp-rs`, with `nom_parser` only.

- [ ] **Step 3:** Commit (confirm with user first):
```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add lopdf as direct dependency for PDF outline extraction"
```

---

### Task 2: `extract_pdf_outline` in `src/pdf.rs` (pure function, TDD)

**Files:**
- Modify: `src/pdf.rs` (add struct + fn at top level, ~line 77)
- Test: `src/pdf.rs` test modules (new `mod extract_pdf_outline`)

- [ ] **Step 1: Write the failing tests** — add `#[cfg(test)] pub(crate)` fixture helpers and a new test module:

```rust
// top level, next to extract_pdf_pages
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct PdfOutlineEntry {
    pub(crate) level: usize,
    pub(crate) title: String,
    pub(crate) page: usize,
}

/// Writes a minimal 2-page PDF with a 3-entry outline to `path`.
#[cfg(test)]
pub(crate) fn write_pdf_with_outline(path: &Path) {
    use lopdf::{dictionary, Bookmark, Document, Object, Stream};

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
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page1.into(), page2.into()],
        "Count" => 2,
    }));
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
    let outline_id = doc.build_outline().expect("build outline");
    doc.catalog_mut()
        .expect("catalog")
        .set(b"Outlines", outline_id);
    doc.save(path).expect("save pdf");
}

/// Writes a minimal valid 1-page PDF with no outline to `path`.
#[cfg(test)]
pub(crate) fn write_pdf_without_outline(path: &Path) {
    use lopdf::{dictionary, Document, Object, Stream};

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page1 = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page1.into()],
        "Count" => 1,
    }));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).expect("save pdf");
}
```

```rust
// in `#[cfg(test)] mod tests`
mod extract_pdf_outline {
    use std::io::Write;

    use super::*;

    #[test]
    fn returns_not_found_error_when_file_is_missing() {
        let path = Path::new("/nonexistent/file.pdf");
        let result = extract_pdf_outline(path, 50 * 1024 * 1024);
        assert!(matches!(result, Err(ZoteroMcpError::NotFound(_))));
    }

    #[test]
    fn rejects_file_larger_than_max_before_parsing() {
        let mut temp = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        temp.write_all(b"more").unwrap();
        let result = extract_pdf_outline(temp.path(), 3);
        assert!(matches!(result, Err(ZoteroMcpError::InputRejected(_))));
    }

    #[test]
    fn returns_pdf_extract_error_when_file_is_not_a_valid_pdf() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        temp.write_all(b"Not a real PDF file header").unwrap();
        let result = extract_pdf_outline(temp.path(), 50 * 1024 * 1024);
        assert!(matches!(result, Err(ZoteroMcpError::PdfExtract(_))));
    }

    #[test]
    fn returns_empty_outline_when_pdf_has_no_bookmarks() {
        let dir = tempfile::TempDir::new().unwrap();
        let pdf = dir.path().join("plain.pdf");
        write_pdf_without_outline(&pdf);
        let result = extract_pdf_outline(&pdf, 50 * 1024 * 1024).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn returns_entries_with_level_title_and_page() {
        let dir = tempfile::TempDir::new().unwrap();
        let pdf = dir.path().join("outline.pdf");
        write_pdf_with_outline(&pdf);
        let entries = extract_pdf_outline(&pdf, 50 * 1024 * 1024).unwrap();
        assert_eq!(entries, vec![
            PdfOutlineEntry { level: 1, title: "Chapter 1".to_owned(), page: 1 },
            PdfOutlineEntry { level: 1, title: "Chapter 2".to_owned(), page: 2 },
            PdfOutlineEntry { level: 2, title: "Section 2.1".to_owned(), page: 2 },
        ]);
    }
}
```

- [ ] **Step 2: Verify RED**
`cargo nextest run extract_pdf_outline` — fails: `extract_pdf_outline` not found (compile error = valid red). Note: fixture helpers will compile (they only use lopdf).

- [ ] **Step 3: Minimal implementation** (add after `extract_pdf_pages`):

```rust
/// Extracts the bookmark outline (table of contents) from the PDF at
/// `file_path` as a flat list of entries with 1-based `page` numbers.
///
/// A PDF without bookmarks yields an empty [`Vec`]; `get_toc()` reporting no
/// outline is treated as no-op rather than an error.
///
/// # Errors
///
/// - [`NotFound`] if `file_path` does not exist
/// - [`InputRejected`] if `file_path` is larger than `max_pdf_bytes`
/// - [`PdfExtract`] if the file cannot be parsed as a PDF
///
/// [`InputRejected`]: ZoteroMcpError::InputRejected
/// [`NotFound`]: ZoteroMcpError::NotFound
/// [`PdfExtract`]: ZoteroMcpError::PdfExtract
pub(crate) fn extract_pdf_outline(
    file_path: &Path,
    max_pdf_bytes: u64,
) -> Result<Vec<PdfOutlineEntry>, ZoteroMcpError> {
    if !file_path.exists() {
        return Err(ZoteroMcpError::NotFound(format!(
            "PDF file not found: {}",
            file_path.display()
        )));
    }
    let len = std::fs::metadata(file_path)?.len();
    if len > max_pdf_bytes {
        return Err(ZoteroMcpError::InputRejected(format!(
            "PDF file {} exceeds {max_pdf_bytes} bytes",
            file_path.display()
        )));
    }
    let doc = lopdf::Document::load(file_path)
        .map_err(|e| ZoteroMcpError::PdfExtract(e.to_string()))?;
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
```

- [ ] **Step 4: Verify GREEN** — `cargo nextest run extract_pdf_outline` passes all 5 tests; `mise run test` still green.

- [ ] **Step 5:** Commit (confirm with user first).

---

### Task 3: Extract shared `resolve_pdf_path` helper (refactor, guarded by existing tests)

**Files:**
- Modify: `src/mcp/zotero.rs` — add helper method + rewrite `zotero_read_pdf_pages_impl` (currently lines 551-626)

- [ ] **Step 1:** Add the helper to `impl ZoteroMcpServer` (behavior-identical extraction of the read_pdf_pages path-resolution block):

```rust
/// Resolves and security-validates the PDF file path for `item_key_or_path`,
/// which may be an item key (parent or attachment) or a direct filesystem path.
///
/// # Errors
///
/// - [`ZoteroMcpError::LocalApi`] if the item or its children cannot be fetched
/// - [`ZoteroMcpError::NotFound`] if no PDF attachment exists for the item
/// - [`ZoteroMcpError::InputRejected`] if the path fails security checks
pub(crate) async fn resolve_pdf_path(
    &self,
    item_key_or_path: &str,
) -> Result<PathBuf, ZoteroMcpError> {
    let bridge_roots = self.fetch_bridge_pdf_roots().await;
    if Path::new(item_key_or_path).exists() {
        return self.validate_pdf_read_path(
            Path::new(item_key_or_path),
            &bridge_roots,
            true,
        );
    }

    let client = ZoteroClient::new(&self.state);
    let item_key = ItemKey::from(item_key_or_path);
    let item = client.get_item(&item_key).await?;

    let resolved = if item.data.item_type == ItemType::Attachment {
        resolve_attachment_pdf_path(&item, &bridge_roots)
    } else {
        client
            .get_item_children(&item_key)
            .await
            .ok()
            .and_then(|children| find_pdf_path(&children, &bridge_roots))
    };
    let Some(resolved) = resolved else {
        return Err(ZoteroMcpError::NotFound(format!(
            "No PDF file path found for key: {item_key_or_path}"
        )));
    };

    if resolved.requires_root_check {
        self.validate_pdf_read_path(&resolved.path, &bridge_roots, false)
    } else {
        let checked = canonicalize_existing_path(&resolved.path)?;
        self.state.check_pdf_file(&checked)?;
        Ok(checked)
    }
}
```

- [ ] **Step 2:** Rewrite `zotero_read_pdf_pages_impl` body to:

```rust
pub(crate) async fn zotero_read_pdf_pages_impl(
    &self,
    args: ReadPdfPagesArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let pdf_path = match self.resolve_pdf_path(&args.item_key_or_path).await {
        Ok(path) => path,
        Err(e) => return Ok(super::text_error(&e)),
    };
    let pages_ref = args.pages.as_deref();
    Ok(super::json_result(extract_pdf_pages(
        &pdf_path,
        pages_ref,
        self.state.security.max_pdf_bytes,
    )))
}
```

- [ ] **Step 3:** Verify existing tests stay green: `cargo nextest run pdf_pages` then `mise run test`.
- [ ] **Step 4:** Commit (confirm with user first).

---

### Task 4: `zotero_get_pdf_outline` tool (impl + args, TDD)

**Files:**
- Modify: `src/mcp/zotero.rs` — `GetPdfOutlineArgs` struct (after `ReadPdfPagesArgs`, line ~128) + `zotero_get_pdf_outline_impl` (after `zotero_read_pdf_pages_impl`) + tests
- Modify: `src/mcp/server.rs` — import `GetPdfOutlineArgs`, add `#[tool]` method after `zotero_read_pdf_pages` (line ~310)

- [ ] **Step 1: Write the failing tests** — new `mod pdf_outline` after `mod pdf_pages`:

```rust
mod pdf_outline {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn rejects_direct_path_by_default() {
        let temp = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let server = ZoteroMcpServer::new(AppState {
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        let res = server
            .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                item_key_or_path: temp.path().display().to_string(),
            })
            .await
            .expect("get pdf outline result");

        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("Direct file paths are disabled"));
    }

    #[tokio::test]
    async fn returns_outline_for_direct_path_inside_configured_root() {
        let root = tempfile::TempDir::new().unwrap();
        let pdf = root.path().join("outline.pdf");
        crate::pdf::write_pdf_with_outline(&pdf);
        let mut security = SecurityConfig::default();
        security.direct_file_paths = true;
        security.allowed_read_dirs = vec![root.path().canonicalize().unwrap()];
        let server = ZoteroMcpServer::new(AppState {
            security,
            ..AppState::from_env()
        });

        let res = server
            .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                item_key_or_path: pdf.display().to_string(),
            })
            .await
            .expect("get pdf outline result");

        assert_eq!(res.is_error, Some(false));
        let text = tool_text(&res);
        assert!(text.contains("Chapter 1"));
        assert!(text.contains("Section 2.1"));
    }

    #[tokio::test]
    async fn returns_empty_outline_for_pdf_without_bookmarks() {
        let root = tempfile::TempDir::new().unwrap();
        let pdf = root.path().join("plain.pdf");
        crate::pdf::write_pdf_without_outline(&pdf);
        let mut security = SecurityConfig::default();
        security.direct_file_paths = true;
        security.allowed_read_dirs = vec![root.path().canonicalize().unwrap()];
        let server = ZoteroMcpServer::new(AppState {
            security,
            ..AppState::from_env()
        });

        let res = server
            .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                item_key_or_path: pdf.display().to_string(),
            })
            .await
            .expect("get pdf outline result");

        assert_eq!(res.is_error, Some(false));
        assert!(tool_text(&res).contains("[]"));
    }

    #[tokio::test]
    async fn reads_imported_attachment_enclosure_outline() {
        let pdf = tempfile::NamedTempFile::new().unwrap();
        crate::pdf::write_pdf_with_outline(pdf.path());
        let file_url = url::Url::from_file_path(pdf.path()).unwrap().to_string();
        let children = json!([{
            "key": "PDF00001",
            "version": 1,
            "links": {
                "enclosure": {
                    "href": file_url,
                    "type": "application/pdf",
                    "title": "outline.pdf",
                },
            },
            "data": {
                "key": "PDF00001",
                "version": 1,
                "itemType": "attachment",
                "linkMode": "imported_file",
                "contentType": "application/pdf",
                "filename": "outline.pdf",
            },
        }]);
        let zotero_base = zotero_pdf_server(children);
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
            security: security_with_pdf_limit(1024 * 1024),
            ..AppState::from_env()
        });

        let res = server
            .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                item_key_or_path: "ITEM0001".to_owned(),
            })
            .await
            .expect("get pdf outline result");

        assert_eq!(res.is_error, Some(false));
        assert!(tool_text(&res).contains("Chapter 1"));
    }
}
```

- [ ] **Step 2: Verify RED**
`cargo nextest run pdf_outline` — fails: `zotero_get_pdf_outline_impl` and `GetPdfOutlineArgs` not found.

- [ ] **Step 3: Args struct** (after `ReadPdfPagesArgs`, ~line 128):

```rust
/// Arguments for `zotero_get_pdf_outline`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetPdfOutlineArgs {
    /// Zotero item key; direct PDF paths must resolve under configured or
    /// Zotero-reported PDF roots, otherwise direct-path opt-in is required.
    pub(crate) item_key_or_path: String,
}
```

- [ ] **Step 4: Impl** (after `zotero_read_pdf_pages_impl`):

```rust
/// Handles PDF outline extraction tool calls.
///
/// # Errors
///
/// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
/// failures are returned as MCP error content.
pub(crate) async fn zotero_get_pdf_outline_impl(
    &self,
    args: GetPdfOutlineArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let pdf_path = match self.resolve_pdf_path(&args.item_key_or_path).await {
        Ok(path) => path,
        Err(e) => return Ok(super::text_error(&e)),
    };
    Ok(super::json_result(extract_pdf_outline(
        &pdf_path,
        self.state.security.max_pdf_bytes,
    )))
}
```

- [ ] **Step 5: Verify GREEN** — `cargo nextest run pdf_outline` passes; `mise run test` green.

- [ ] **Step 6: Register tool in `src/mcp/server.rs`** — add `GetPdfOutlineArgs` to the `mcp::zotero::{...}` import and this method after `zotero_read_pdf_pages`:

```rust
#[tool(
    name = "zotero_get_pdf_outline",
    description = "Extract the PDF outline (table of contents/bookmarks) for \
                   an item's PDF attachment or a direct PDF path"
)]
/// # Errors
///
/// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
/// failures are returned as MCP error content.
pub(crate) async fn zotero_get_pdf_outline(
    &self,
    Parameters(args): Parameters<GetPdfOutlineArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    self.zotero_get_pdf_outline_impl(args).await
}
```

- [ ] **Step 7:** `mise run test`, then `mise run clippy`, then `mise run lint`. Verify the tool appears: `cargo nextest run tool_router_lists_all_registered_tools`.
- [ ] **Step 8:** Commit (confirm with user first).

---

### Task 5: Final verification

- [ ] `mise run ci` (check + clippy + test + audit)
- [ ] Run `gitnexus detect_changes` before the final commit (index was rebuilding earlier — re-run `node .gitnexus/run.cjs analyze` if still stale), confirm only `src/pdf.rs`, `src/mcp/zotero.rs`, `src/mcp/server.rs`, `Cargo.toml`/`Cargo.lock` changed.

---

**Self-review notes:**
- Spec coverage: pure fn (T2), impl + args (T4), registration (T4), refactor dedup (T3), deps (T1), verification (T5). All covered.
- Placeholders: none — all code concrete.
- Type consistency: `PdfOutlineEntry{level,title,page}`, `GetPdfOutlineArgs{item_key_or_path}`, `resolve_pdf_path(&self, &str) -> Result<PathBuf, ZoteroMcpError>`, `extract_pdf_outline(&Path, u64) -> Result<Vec<PdfOutlineEntry>, ZoteroMcpError>` — consistent across tasks.
- `zotero_get_pdf_path_impl` (line 510) is left as-is; it stays a lightweight display-string helper, the new tool does proper security validation.
- **Skipped:** nested outline tree (`get_outlines`) — flat `level` is LLM-friendlier; `.docx`/PDF-writing — out of scope. Add when a caller needs hierarchy.
