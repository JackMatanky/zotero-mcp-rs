# PDF Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `zotero_items_write` action `import_pdf` that imports a local PDF into Zotero storage via the official three-phase Zotero file upload protocol using an MD5 checksum, closing gap #1 in `docs/zotero-mcp-comparison.md`.

**Architecture:** One client method `import_pdf_file` on `ZoteroClient` creates an `imported_file` attachment item (parented or top-level), then runs the Web-API-identical three-phase upload: (1) `POST …/items/<key>/file` with `md5`/`filename`/`filesize`/`mtime` + `If-None-Match: *` → upload ticket or `{"exists":1}` short-circuit; (2) `POST` the raw bytes to the returned `url` → `201`; (3) `POST` `upload=<uploadKey>` repeating the header → `204`. A thin MCP handler validates the path with the existing security helpers (reused from `zotero_pdf`), then calls the client. The connector fast-path from earlier drafts is **dropped**: top-level `imported_file` attachments are legal (docs/zotero_api.md:1206), so the MD5 path covers the no-parent case without connector session plumbing.

**Tech Stack:** Rust 1.94 (mise toolchain — system rustc 1.89 is rejected by sqlx 0.9.0), `md-5` (RustCrypto) crate (new dependency), using streaming chunked I/O with the `Digest` trait for memory-efficient MD5 computation via `BufReader` + 8KB chunks (primary method for all files; avoids loading entire file into RAM), existing reqwest 0.12, rmcp 3.1, schemars 1.0, serde. Tests use the existing `test_http::MockServer` + `tempfile` (already a dev-dep).

---

## Conventions & How to Work (read first)

- All cargo commands run **from this workspace root** via `mise x -- cargo …`. Do **not** use the mise MCP task tool / `mise run task`: it resolves a parent-level mise config and runs the sibling `traces-pkm` crate instead of this one. Reliable invocations:
  - Focused test: `mise x -- cargo nextest run -p zotero-mcp-rs -E 'test(<name>)'`
  - Full gate (must stay green at the end of every task): `mise x -- cargo fmt --all` + `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings` + `mise x -- cargo nextest run -p zotero-mcp-rs` (baseline: 354 tests pass).
- TDD strictly: write the failing test first, run it to confirm it fails, then implement. No skipping the red step.
- Lint bar: `indexing_slicing` is denied for non-test source; Vec indexing **is** accepted inside `#[cfg(test)]` (see `src/zotero/relations.rs:587` indexing `requests[0..3]` with no `#[allow]`). `as_conversions` is warn (fails under `-D warnings`), so **never use `as` casts** — use `u64::try_from(x).unwrap_or(u64::MAX)`. `expect_used` is denied outside tests. Clippy thresholds in `clippy.toml`: complexity 15, max args 5, too-many-lines 100.
- Security gates (mandatory, never simplified away): write ops must start with `state.check_write_permission()?`; **import reads bytes from disk and uploads them**, so the path must pass `validate_pdf_read_path` (mcp/pdf.rs) — i.e. be under `allowed_read_dirs` or a bridge root, and `direct_file_paths` must be enabled for direct input. Tests simulate this by setting `security.direct_file_paths = true` and `allowed_read_dirs = [temp dir]`.
- HTTP-header wire format: reqwest canonicalizes header names to lowercase (`If-None-Match` → `if-none-match` on the wire). Tests must compare lowercased request strings.
- `MockServer` serves canned responses in order, one TCP connection each, and its `Connection: close` forces reqwest to open a fresh connection per request. The phase-2 upload URL is a **different host** (the upload target), so the tests use **two** MockServers: an API server (create / phase-1 / phase-3) and an upload server (phase-2). `MockServer::recording` returns a `RequestLog` to assert request bodies; `request_body(raw)` only parses JSON bodies, so form-encoded requests are asserted with raw substring checks.
- Every task ends with the full gate green and a commit (one commit per task).

---

## File Map

| File | Change |
| --- | --- |
| `Cargo.toml` | Add `md-5 = "0.11"` dependency |
| `src/zotero/attachments.rs` | Add `import_pdf_file` client method + `UploadTicket` + tests |
| `src/mcp/zotero/attachments.rs` | Add `ImportPdfArgs` + `zotero_import_pdf_impl` + MCP test |
| `src/mcp/zotero/items.rs` | Register `import_pdf` action in `ZoteroItemsWriteCommand` router |
| `src/mcp/catalog.rs` | Add `import_pdf` to the `zotero_items_write` catalog entry |
| `src/mcp/zotero.rs` | Update module doc (attachments row) |
| `docs/zotero-mcp-comparison.md` | Mark PDF import gap closed |

---

## Tasks

### Task 1 — Add the `md-5` dependency

**Files:**
- Modify: `Cargo.toml` (`[dependencies]`)

- [ ] **Step 1: Add the dependency**

Find the `[dependencies]` block (near `url = "2.5"`) and add one line:

```toml
md-5 = "0.11"
```

- [ ] **Step 2: Verify it compiles**

Run: `mise x -- cargo check -p zotero-mcp-rs`
Expected: exits 0, `md-5 v0.11.x` appears in the compile output. (The crate compiles even though it is not used yet — cargo warns, does not fail.)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add RustCrypto md-5 dependency for PDF import"
```

---

### Task 2 — Client method `import_pdf_file` (TDD)

**Files:**
- Modify: `src/zotero/attachments.rs`

Background you need: `src/zotero/attachments.rs` already has `attach_file_link` (`pub(crate) async fn`, uses `self.state.check_write_permission()`, `self.state.zotero_api_url`, `self.post_json_first(&url, &payload, "...")`), and a `#[cfg(test)] mod tests` with a `state(zotero_api_url, write_enabled)` fixture and `created_attachment()`. The method below is added to the same `impl ZoteroClient<'_>` block.

- [ ] **Step 1: Write the failing tests**

Add this submodule to the existing `#[cfg(test)] mod tests` in `src/zotero/attachments.rs`. The three tests cover: the full three-phase happy path, the `{"exists":1}` short-circuit (also proving a top-level import omits `parentItem`), and the write-permission gate.

```rust
    mod import_pdf {
        use super::*;

        fn pdf_file() -> (tempfile::TempDir, std::path::PathBuf) {
            let dir = tempfile::tempdir().expect("temp dir");
            let path = dir.path().join("paper.pdf");
            std::fs::write(&path, b"%PDF-1.4\n%%EOF\n").expect("write pdf");
            (dir, path)
        }

        fn phase1_response(upload_url: &str) -> String {
            json!({
                "url": upload_url,
                "uploadKey": "uk",
                "contentType": "application/pdf",
                "prefix": "",
                "suffix": "",
            })
            .to_string()
        }

        #[tokio::test]
        async fn imports_pdf_via_three_phase_upload() {
            let (_dir, pdf_path) = pdf_file();
            let (upload_server, upload_recorded) = MockServer::recording(vec![
                http_response("201 Created", ""),
            ]);
            let (api_server, recorded) = MockServer::recording(vec![
                http_response("200 OK", &created_attachment()),
                http_response(
                    "200 OK",
                    &phase1_response(&format!(
                        "{}/upload",
                        upload_server.url()
                    )),
                ),
                http_response("204 No Content", ""),
            ]);
            let app = state(api_server.url(), true);

            let result = ZoteroClient::new(&app)
                .import_pdf_file(
                    Some(&ItemKey::from("PARENT01")),
                    "Paper",
                    &pdf_path,
                    None,
                )
                .await;

            assert!(
                result.is_ok(),
                "import should succeed: {result:?}"
            );
            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 3);

            let created = request_body(&requests[0])
                .expect("create request json")
                .get(0)
                .expect("created item")
                .clone();
            assert_eq!(created.get("parentItem"), Some(&json!("PARENT01")));
            assert_eq!(created.get("linkMode"), Some(&json!("imported_file")));
            assert_eq!(created.get("filename"), Some(&json!("paper.pdf")));
            assert_eq!(
                created.get("contentType"),
                Some(&json!("application/pdf"))
            );

            let phase1 = requests[1].to_lowercase();
            assert!(phase1.contains("md5="));
            assert!(phase1.contains("filename=paper.pdf"));
            assert!(phase1.contains("filesize="));
            assert!(phase1.contains("mtime="));
            assert!(phase1.contains("if-none-match: *"));

            let upload_requests =
                upload_recorded.lock().expect("upload request log");
            assert_eq!(upload_requests.len(), 1);
            let upload_body = upload_requests[0]
                .split_once("\r\n\r\n")
                .map_or("", |(_, body)| body);
            assert_eq!(upload_body, "%PDF-1.4\n%%EOF\n");

            let phase3 = requests[2].to_lowercase();
            assert!(phase3.contains("upload=uk"));
            assert!(phase3.contains("if-none-match: *"));
        }

        #[tokio::test]
        async fn short_circuits_when_zotero_already_has_the_file() {
            let (_dir, pdf_path) = pdf_file();
            let (api_server, recorded) = MockServer::recording(vec![
                http_response("200 OK", &created_attachment()),
                http_response("200 OK", r#"{"exists": 1}"#),
            ]);
            let app = state(api_server.url(), true);

            let result = ZoteroClient::new(&app)
                .import_pdf_file(None, "Paper", &pdf_path, None)
                .await;

            assert!(
                result.is_ok(),
                "exists short-circuit should succeed: {result:?}"
            );
            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 2);
            let created = request_body(&requests[0])
                .expect("create request json")
                .get(0)
                .expect("created item")
                .clone();
            assert!(
                created.get("parentItem").is_none(),
                "top-level import must omit parentItem"
            );
            assert!(requests[1].contains("md5="));
        }

        #[tokio::test]
        async fn denies_writes_when_write_permission_is_disabled() {
            let app = state("http://127.0.0.1:1", false);

            let result = ZoteroClient::new(&app)
                .import_pdf_file(
                    Some(&ItemKey::from("PARENT01")),
                    "Paper",
                    std::path::Path::new("/tmp/paper.pdf"),
                    None,
                )
                .await;

            assert!(
                matches!(
                    result,
                    Err(ZoteroMcpError::PermissionDenied(_))
                ),
                "write-disabled import should fail before HTTP: {result:?}"
            );
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `mise x -- cargo nextest run -p zotero-mcp-rs -E 'test(import_pdf)'`
Expected: compile error — `no function or associated item named 'import_pdf_file' found for struct 'ZoteroClient'`. This is the red step; do not implement yet beyond confirming the failure is the missing method.

- [ ] **Step 3: Write the minimal implementation**

Add to `src/zotero/attachments.rs`: a `serde::Deserialize` import, the `UploadTicket` struct, and the `import_pdf_file` method in the existing `impl ZoteroClient<'_>` block (after `attach_file_link`).

Top of file imports become:
```rust
use std::path::Path;

use md5::Digest;
use serde::Deserialize;
use tokio::fs;

use crate::{
    errors::ZoteroMcpError,
    zotero::{ItemKey, ItemType, LinkMode, ZoteroItem, client::ZoteroClient},
};
use serde_json::json;
```

Add this struct at module level:

```rust
/// Phase-1 response payload from Zotero's file-upload endpoint.
#[derive(Deserialize)]
struct UploadTicket {
    /// Signed upload URL to `POST` the raw file bytes to.
    url: String,
    /// Upload key replayed in the finalize request.
    #[serde(rename = "uploadKey")]
    upload_key: String,
}
```

Add this method inside the `impl ZoteroClient<'_>` block:

```rust
    /// Imports a local file into Zotero storage via the three-phase MD5
    /// upload and returns the created attachment item.
    ///
    /// # Arguments
    ///
    /// * `parent_item_key` - Parent item to attach to; [`None`] creates a
    ///   top-level attachment.
    /// * `title` - Title for the attachment
    /// * `path` - Canonical path to the local file to import
    /// * `content_type` - Optional MIME content type (defaults to
    ///   `"application/pdf"`)
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::InputRejected`] if the path has no UTF-8 filename
    /// - [`ZoteroMcpError::Io`] if the file cannot be read
    /// - [`ZoteroMcpError::LocalApi`] if Zotero rejects any phase
    /// - [`ZoteroMcpError::Network`] if a request fails at the transport level
    /// - [`ZoteroMcpError::Json`] if a response cannot be decoded
    pub(crate) async fn import_pdf_file(
        &self,
        parent_item_key: Option<&ItemKey>,
        title: &str,
        path: &Path,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;

        // Read file once into memory for MD5 + phase-2 upload
        // (PDFs are typically small enough; avoids double I/O)
        let bytes = tokio::fs::read(path).await?;

        // Compute MD5 from the bytes
        let mut hasher = md5::Md5::new();
        hasher.update(&bytes);
        let md5 = format!("{:x}", hasher.finalize());

        // Extract filename from path
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ZoteroMcpError::InputRejected("path has no valid UTF-8 filename".into()))?;

        // Get modification time in milliseconds
        let metadata = tokio::fs::metadata(path).await?;
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut attachment = serde_json::Map::new();
        attachment.insert("itemType".into(), json!(ItemType::Attachment));
        attachment.insert("title".into(), json!(title));
        attachment.insert("linkMode".into(), json!(LinkMode::ImportedFile));
        attachment.insert("filename".into(), json!(filename));
        attachment.insert(
            "contentType".into(),
            json!(content_type.unwrap_or("application/pdf")),
        );
        if let Some(parent) = parent_item_key {
            attachment.insert("parentItem".into(), json!(parent));
        }
        let create_url = format!("{}/users/0/items", self.state.zotero_api_url);
        let item: ZoteroItem = self
            .post_json_first(
                &create_url,
                &json!([attachment]),
                "Created attachment array was empty",
            )
            .await?;

        let file_url = format!(
            "{}/users/0/items/{}/file",
            self.state.zotero_api_url,
            item.data.key
        );
        let filesize_text = bytes.len().to_string();
        let mtime_text = modified_ms.to_string();
        let resp = self
            .state
            .client
            .post(&file_url)
            .form(&[
                ("md5", md5.as_str()),
                ("filename", filename),
                ("filesize", filesize_text.as_str()),
                ("mtime", mtime_text.as_str()),
            ])
            .header("If-None-Match", "*")
            .send()
            .await?;
        let body: serde_json::Value =
            self.ensure_success(resp).await?.json().await?;
        if body
            .as_object()
            .is_some_and(|object| object.contains_key("exists"))
        {
            return Ok(item);
        }
        let ticket: UploadTicket = serde_json::from_value(body)?;

        let upload = self
            .state
            .client
            .post(&ticket.url)
            .body(bytes)
            .send()
            .await?;
        if upload.status().as_u16() != 201 {
            return Err(ZoteroMcpError::LocalApi {
                status: upload.status().as_u16(),
                message: upload.text().await.unwrap_or_default(),
            });
        }

        let finalize = self
            .state
            .client
            .post(&file_url)
            .form(&[("upload", ticket.upload_key.as_str())])
            .header("If-None-Match", "*")
            .send()
            .await?;
        self.ensure_success(finalize).await?;
        Ok(item)
    }
```

The method uses the unqualified `json!` macro, so add `use serde_json::json;` to the top-of-file imports above (the existing `attach_file_link` spells out `serde_json::json!`, but this method's `json!(ItemType::Attachment)` / `json!([attachment])` calls need the import).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `mise x -- cargo nextest run -p zotero-mcp-rs -E 'test(import_pdf)'`
Expected: `3 passed`, 0 failed.

- [ ] **Step 5: Run the full gate**

Run: `mise x -- cargo fmt --all` then `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings` then `mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: fmt clean, clippy zero warnings, 354 + 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/zotero/attachments.rs
git commit -m "feat(zotero): three-phase MD5 PDF import client method"
```

---

### Task 3 — MCP action `import_pdf` (args, handler, router, catalog)

**Files:**
- Modify: `src/mcp/zotero/attachments.rs`
- Modify: `src/mcp/zotero/items.rs`
- Modify: `src/mcp/catalog.rs`
- Modify: `src/mcp/zotero.rs`

Background: the write router `zotero_items_write` lives in `src/mcp/zotero/items.rs` (`ZoteroItemsWriteCommand` enum with `AttachFile(crate::mcp::zotero::attachments::AttachFileArgs)` variant, dispatched in `zotero_items_write`), and the catalog entry for `zotero_items_write` is a `PrimitiveInfo` in `src/mcp/catalog.rs` gated on `EnvGate::WriteEnabled`. Security helpers `fetch_bridge_pdf_roots` and `validate_pdf_read_path` are methods on `ZoteroMcpServer` defined in `src/mcp/pdf.rs`, callable from this module.

- [ ] **Step 1: Write the failing MCP test**

Add a `#[cfg(test)] mod tests` to `src/mcp/zotero/attachments.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ZoteroMcpServer,
        mcp::zotero::fixtures::{
            http_response, mock_server, tool_text, zotero_state,
        },
        security::SecurityConfig,
    };

    fn import_server(upload_base: &str) -> String {
        let created = serde_json::json!([{
            "key": "ATTACH01",
            "version": 1,
            "data": { "key": "ATTACH01", "version": 1, "itemType": "attachment" },
        }])
        .to_string();
        let phase1 = serde_json::json!({
            "url": format!("{upload_base}/upload"),
            "uploadKey": "uk",
            "contentType": "application/pdf",
            "prefix": "",
            "suffix": "",
        })
        .to_string();
        mock_server(vec![
            http_response("200 OK", &created),
            http_response("200 OK", &phase1),
            http_response("204 No Content", ""),
        ])
    }

    #[tokio::test]
    async fn import_pdf_uploads_file_and_reports_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pdf_path = dir.path().join("paper.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4\n%%EOF\n").expect("write pdf");

        let upload_base =
            mock_server(vec![http_response("201 Created", "")]);
        let base = import_server(&upload_base);

        let mut app = zotero_state(base);
        app.security = SecurityConfig {
            direct_file_paths: true,
            allowed_read_dirs: vec![dir.path().to_path_buf()],
            ..app.security
        };
        let server = ZoteroMcpServer::new(app);

        let res = server
            .zotero_import_pdf_impl(ImportPdfArgs {
                parent_item_key: Some("PARENT01".into()),
                title: "Paper".to_owned(),
                file_path: pdf_path.to_string_lossy().into_owned(),
                content_type: None,
            })
            .await
            .expect("import ok");

        assert_eq!(res.is_error, Some(false));
        assert!(tool_text(&res).contains("ATTACH01"));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `mise x -- cargo nextest run -p zotero-mcp-rs -E 'test(import_pdf)'`
Expected: compile error — `cannot find type 'ImportPdfArgs'` / `no method 'zotero_import_pdf_impl'`. Red step confirmed.

- [ ] **Step 3: Add the args struct and handler**

Extend `src/mcp/zotero/attachments.rs` with the args struct and handler, keeping the existing `AttachFileArgs` / `zotero_attach_file_impl` untouched:

```rust
use std::path::Path;

/// Arguments for the `import_pdf` action of `zotero_items_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ImportPdfArgs {
    /// Optional key of the parent item ([`ItemKey`]); omitted to create a
    /// top-level attachment.
    parent_item_key: Option<ItemKey>,
    /// Display title for the attachment.
    title: String,
    /// Local path to the PDF file to import.
    file_path: String,
    /// Optional content type (default: `"application/pdf"`).
    content_type: Option<String>,
}
```

Add this method to the existing `impl ZoteroMcpServer` block:

```rust
    /// Handles Zotero PDF import tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_import_pdf_impl(
        &self,
        args: ImportPdfArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bridge_roots = self.fetch_bridge_pdf_roots().await;
        let checked = match self.validate_pdf_read_path(
            Path::new(&args.file_path),
            &bridge_roots,
            true,
        ) {
            Ok(path) => path,
            Err(e) => return Ok(text_error(&e)),
        };
        let client = ZoteroClient::new(&self.state);
        Ok(json_result(
            client
                .import_pdf_file(
                    args.parent_item_key.as_ref(),
                    &args.title,
                    &checked,
                    args.content_type.as_deref(),
                )
                .await,
        ))
    }
```

Add `text_error` to the existing `use crate::mcp::json_result;` import (change to `use crate::mcp::{json_result, text_error};`).

- [ ] **Step 4: Register the action in the router, catalog, and module doc**

In `src/mcp/zotero/items.rs`:

Add the variant to `ZoteroItemsWriteCommand` (after `AttachFile`):

```rust
    AttachFile(crate::mcp::zotero::attachments::AttachFileArgs),
    ImportPdf(crate::mcp::zotero::attachments::ImportPdfArgs),
```

Add the dispatch arm in `zotero_items_write` (after the `AttachFile` arm):

```rust
            ZoteroItemsWriteCommand::ImportPdf(args) => {
                self.zotero_import_pdf_impl(args).await
            }
```

Update the tool description string (action list):

```rust
        description = "Grouped Zotero item write router. action: update, \
                       delete, trash, restore, add_by_identifier, \
                       attach_file, import_pdf",
```

In `src/mcp/catalog.rs`, update the `zotero_items_write` `PrimitiveInfo` summary and search_text:

```rust
        summary: "Grouped item write actions: update, delete, trash, restore, \
                  add_by_identifier, attach_file, import_pdf",
        example: Some(r#"{"action":"trash","item_key":"ITEMKEY"}"#),
        search_text: "zotero_items_write items grouped item write actions \
                      update delete trash restore add_by_identifier \
                      attach_file import_pdf zotero_write_enabled",
```

In `src/mcp/zotero.rs`, update the module doc line for attachments:

```rust
//! - `attachments`: `attach_file` and `import_pdf` actions of
//!   `zotero_items_write`
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `mise x -- cargo nextest run -p zotero-mcp-rs -E 'test(import_pdf)'`
Expected: 4 passed (3 client + 1 MCP), 0 failed.

- [ ] **Step 6: Run the full gate**

Run: `mise x -- cargo fmt --all` then `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings` then `mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: fmt clean, clippy zero warnings, all 358 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/mcp/zotero/attachments.rs src/mcp/zotero/items.rs src/mcp/catalog.rs src/mcp/zotero.rs
git commit -m "feat(mcp): expose import_pdf in zotero_items_write router and catalog"
```

---

### Task 4 — Documentation and final verification

**Files:**
- Modify: `docs/zotero-mcp-comparison.md`

- [ ] **Step 1: Update the comparison table and gap list**

In `docs/zotero-mcp-comparison.md`:

Row 42 ("PDF import / upload"), column `zotero-mcp-rs`, change:

```markdown
| **PDF import / upload** | ⚠️ linked attach from local path only, no upload | ✅ add_from_file (PDF/EPUB, auto-DOI), attach local/URL | ❌ | ❌ | ✅ import_pdf_to_zotero (download+upload+index), Unpaywall OA | ❌ | ❌ |
```

to:

```markdown
| **PDF import / upload** | ✅ imported_file three-phase upload (MD5), top-level or parented | ✅ add_from_file (PDF/EPUB, auto-DOI), attach local/URL | ❌ | ❌ | ✅ import_pdf_to_zotero (download+upload+index), Unpaywall OA | ❌ | ❌ |
```

Line 60 (server profile), change the limitation sentence `… no PDF upload, no \`.docx\` injection.` to `… no URL download-and-import, no \`.docx\` injection.`

In the gap analysis, move gap #1 into the **Closed** section (delete it from Open):

```markdown
1. ~~**PDF import / upload**~~ — `import_pdf` action of `zotero_items_write` imports a local PDF into Zotero storage via the three-phase MD5 upload (top-level or parented), with path allowlist + size-cap validation.
```

- [ ] **Step 2: Final full gate**

Run: `mise x -- cargo fmt --all` then `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings` then `mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: fmt clean, clippy zero warnings, all 358 tests pass.

- [ ] **Step 3: Commit**

```bash
git add docs/zotero-mcp-comparison.md
git commit -m "docs: mark PDF import gap closed"
```

- [ ] **Step 4 (optional): Manual smoke against a live Zotero**

With Zotero running, `ZOTERO_WRITE_ENABLED=1`, `ZOTERO_MCP_PROFILE=workspace`, and a real `.pdf` under the workspace dir, start the server and call `zotero_items_write` with `{"action":"import_pdf","title":"Smoke","file_path":"<abs path>.pdf"}`. Confirm the attachment appears in Zotero with the file stored in its storage directory (not a linked path).
