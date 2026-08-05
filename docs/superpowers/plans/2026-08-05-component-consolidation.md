# Component Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce the number of source files and modules by merging small single-purpose modules into their only callers, consolidating duplicate macros, eliminating a redundant compatibility shim, and deduplicating a helper function — all without changing behavior.

**Architecture:** Mechanical moves of code within the same crate. Each step moves code from a small module into its sole caller, removes the original module declaration, and deletes the file. No API changes, no behavior changes. The grouped-tool architecture (22 tools via `#[serde(tag = "action")]`) is preserved.

**Tech Stack:** Rust 1.96, `rmcp` 3.1, `thiserror`, `schemars`, `serde`, `serde_json`. Tests use `pretty_assertions`, `tempfile`, `tokio`, and the existing `test_http::MockServer`.

---

## Conventions & How to Work (read first)

- All cargo commands run **from this workspace root** via `mise x -- cargo …`. Do **not** use the mise MCP task tool / `mise run task`: it resolves a parent-level mise config and runs the sibling `traces-pkm` crate instead of this one. Reliable invocations:
  - Focused test: `mise x -- cargo nextest run -p zotero-mcp-rs -E 'test(<name>)'`
  - Full gate (must stay green at the end of every task): `mise x -- cargo fmt --all` + `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings` + `mise x -- cargo nextest run -p zotero-mcp-rs`
- TDD strictly: write the failing test first, run it to confirm it fails, then implement. No skipping the red step.
- Lint bar: `indexing_slicing` is denied for non-test source; Vec indexing **is** accepted inside `#[cfg(test)]`. `as_conversions` is warn (fails under `-D warnings`), so **never use `as` casts**. `expect_used` is denied outside tests.
- Every task ends with the full gate green and a commit (one commit per task).

---

## File Map

| File | Change |
| --- | --- |
| `src/main.rs` | Add `mod macros;` |
| `src/macros.rs` | **Create** — shared `string_newtype!` macro |
| `src/zotero/keys.rs` | Replace `string_key!` macro with `string_newtype!` |
| `src/better_bibtex/models.rs` | Replace `string_value!` macro with `string_newtype!` |
| `src/better_notes/models.rs` | Replace `TemplateName` manual impls with `string_newtype!` |
| `src/zotero/items.rs` | Absorb `fulltext.rs` and `attachments.rs` |
| `src/zotero/mod.rs` | Remove `mod fulltext;`, `mod attachments;`, update re-exports |
| `src/zotero/fulltext.rs` | **Delete** |
| `src/zotero/attachments.rs` | **Delete** |
| `src/zotero/search.rs` | Absorb `coverage.rs` and `duplicates.rs` |
| `src/zotero/coverage.rs` | **Delete** |
| `src/zotero/duplicates.rs` | **Delete** |
| `src/zotero/notes.rs` | Absorb `annotations.rs` |
| `src/zotero/annotations.rs` | **Delete** |
| `src/mcp/mod.rs` | Remove `mod connector_tools;` |
| `src/mcp/connector_tools.rs` | **Delete** |
| `src/mcp/server.rs` | Remove `connector_router` from `tool_router()` |
| `src/mcp/catalog.rs` | Derive `is_tool_visible`/`is_write_tool` from `PRIMITIVES`; fix `SemanticSearchEnabled` gate bug |
| `src/mcp/resources.rs` | Use shared `filter_notes` helper instead of `note_children` |
| `src/mcp/zotero/notes.rs` | Export `filter_notes` helper |

---

## Tasks

### Task 1: Create shared `string_newtype!` macro in `src/macros.rs`

The `string_key!` macro in `src/zotero/keys.rs:39-126` and `string_value!` in `src/better_bibtex/models.rs:45-92` are nearly identical. `TemplateName` in `src/better_notes/models.rs:41-92` manually implements the same trait set. Consolidate into one macro.

**Files:**
- Create: `src/macros.rs`
- Modify: `src/main.rs:19-27`

- [ ] **Step 1: Create `src/macros.rs`**

```rust
//! Shared newtype macros for string-backed identifier wrappers.

/// Generates a `String`-backed newtype with standard conversions.
///
/// Generates: `Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq,
/// PartialOrd, Serialize`, `#[serde(transparent)]`, `as_str()`, `Display`,
/// `From<String>`, `From<&str>`, `AsRef<str>`, `PartialEq<str>`,
/// `PartialEq<&str>`, `PartialEq<$name> for str`.
///
/// Pass `json` to also derive `schemars::JsonSchema`:
/// ```ignore
/// string_newtype!(MyType, "doc string", json);
/// ```
macro_rules! string_newtype {
    ($name:ident, $doc:expr) => {
        string_newtype!($name, $doc,);
    };
    ($name:ident, $doc:expr, json) => {
        string_newtype!($name, $doc, json,);
    };
    ($name:ident, $doc:expr, $($extra:ident),* $(,)?) => {
        #[doc = $doc]
        #[derive(
            Clone,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
            $($extra,)*
        )]
        #[serde(transparent)]
        pub(crate) struct $name(pub(crate) String);

        impl $name {
            /// Returns the inner string slice.
            #[inline]
            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            #[inline]
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            #[inline]
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            #[inline]
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<$name> for str {
            #[inline]
            fn eq(&self, other: &$name) -> bool {
                self == other.0.as_str()
            }
        }
    };
}
```

- [ ] **Step 2: Register the macro in `src/main.rs`**

Add `mod macros;` before the existing module declarations (before `mod better_bibtex;` on line 19):

```rust
mod macros;
mod better_bibtex;
```

- [ ] **Step 3: Verify it compiles**

Run: `mise x -- cargo check -p zotero-mcp-rs`
Expected: exits 0 with a warning about unused `macros` module (that's fine — no one imports it yet).

- [ ] **Step 4: Commit**

```bash
git add src/macros.rs src/main.rs
git commit -m "refactor: add shared string_newtype! macro"
```

---

### Task 2: Replace `string_key!` in `src/zotero/keys.rs` with `string_newtype!`

**Files:**
- Modify: `src/zotero/keys.rs:39-126` (replace macro definition)

- [ ] **Step 1: Replace the `string_key!` macro definition with `string_newtype!`**

Delete the entire `macro_rules! string_key { ... }` block (lines 39-126). Replace with a one-line re-export:

```rust
use crate::string_newtype;
```

The existing `string_key!(ItemKey, ...)` invocations on lines 128+ now call `string_newtype!` instead.

- [ ] **Step 2: Add `JsonSchema` impl for types that need it**

`keys.rs` types that need `JsonSchema` currently get it from the `string_key!` macro. After switching to `string_newtype!`, add `json` to the invocations for types that need it. Check which ones derive `JsonSchema`:

Looking at the current `string_key!` macro — it **always** includes `schemars::JsonSchema`. So all types from `keys.rs` need `json`. Update each invocation:

```rust
string_newtype!(
    ItemKey,
    "Zotero item key: an 8-character alphanumeric identifier unique within a \
     library. Distinct from [`CollectionKey`] to prevent the two from being \
     transposed at call sites.",
    json
);
string_newtype!(
    CollectionKey,
    "Zotero collection key: an 8-character alphanumeric identifier unique \
     within a library. Distinct from [`ItemKey`] to prevent the two from \
     being transposed at call sites.",
    json
);
string_newtype!(
    TagName,
    "Zotero tag name: wrapper for tag name strings to prevent transposition \
     with free-text query strings or keys.",
    json
);
string_newtype!(
    CitationKey,
    "Zotero citation key: wrapper for citation keys to enforce type safety \
     and key semantics across search and item metadata.",
    json
);
string_newtype!(
    RelationUri,
    "Zotero relation URI: an item URI stored as a value in an item's \
     `relations` map, of the form `http://zotero.org/users/0/items/{KEY}` or \
     `http://zotero.org/groups/{ID}/items/{KEY}`. Bridges [`ItemKey`] and the \
     URI strings Zotero writes for relations: [`From<&ItemKey>`](ItemKey) \
     builds a `/users/0` URI on write, while \
     [`ItemKey::try_from`](ItemKey) recovers the trailing key on read, \
     regardless of the URI prefix.",
    json
);
```

- [ ] **Step 3: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass, no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/zotero/keys.rs
git commit -m "refactor: replace string_key! with shared string_newtype!"
```

---

### Task 3: Replace `string_value!` in `src/better_bibtex/models.rs` with `string_newtype!`

**Files:**
- Modify: `src/better_bibtex/models.rs:45-92` (delete macro), lines 94+ (update invocations)

- [ ] **Step 1: Delete the `string_value!` macro definition**

Delete lines 43-92 (the `/// Generates a String-backed...` doc comment and the entire `macro_rules! string_value { ... }` block).

- [ ] **Step 2: Add import**

At the top of the file, after `use crate::zotero::{CitationKey, ItemKey};`, add:

```rust
use crate::string_newtype;
```

- [ ] **Step 3: Update invocations**

Replace each `string_value!` call with `string_newtype!`. They already pass `json`-equivalent derives (the old `string_value!` included `schemars::JsonSchema`):

```rust
string_newtype!(
    CollectionPath,
    concat!(
        "Better `BibTeX` collection path, represented as \
         forward-slash-separated ",
        "collections where `//` targets the user's personal library root. ",
        "Distinct from Zotero collection keys."
    ),
    json
);
string_newtype!(
    TranslatorName,
    concat!(
        "Better `BibTeX` translator name or GUID, such as `Better BibTeX`, ",
        "`Better BibLaTeX`, or `Better CSL JSON`."
    ),
    json
);
string_newtype!(
    AuxFilePath,
    "Absolute filesystem path to a `LaTeX` `.aux` file.",
    json
);
```

Check all remaining `string_value!` invocations in the file (there are more: `ExportFilePath`, `CslStyleId`, `Locale`, `SearchQuery`, `BibliographyContentType`). Update them all to `string_newtype!(..., json)`.

- [ ] **Step 4: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/better_bibtex/models.rs
git commit -m "refactor: replace string_value! with shared string_newtype!"
```

---

### Task 4: Replace `TemplateName` manual impls in `src/better_notes/models.rs` with `string_newtype!`

**Files:**
- Modify: `src/better_notes/models.rs:39-92`

- [ ] **Step 1: Replace `TemplateName` definition and manual impls**

Delete the `TemplateName` struct definition (lines 41-55) and all manual impls (lines 57-92: `as_str()`, `Display`, `From<String>`, `From<&str>`, `AsRef<str>`, `PartialEq<str>`).

Replace with:

```rust
use crate::string_newtype;

string_newtype!(
    TemplateName,
    "Name of a Better Notes template, such as `\"default\"` or a custom template name.",
    json
);
```

Note: `TemplateName` has `as_str()` in the current code. The `string_newtype!` macro already generates `as_str()`, so this is covered.

Note: `TemplateName` currently lacks `PartialEq<&str>` and `PartialEq<$name> for str` — but `string_newtype!` adds those. This is additive, not breaking.

- [ ] **Step 2: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/better_notes/models.rs
git commit -m "refactor: replace TemplateName manual impls with string_newtype!"
```

---

### Task 5: Merge `zotero/fulltext.rs` into `zotero/items.rs`

**Files:**
- Modify: `src/zotero/items.rs` (add content), `src/zotero/mod.rs` (remove `mod fulltext;`)
- Delete: `src/zotero/fulltext.rs`

- [ ] **Step 1: Read `src/zotero/fulltext.rs` and `src/zotero/items.rs`**

Read both files to understand current content.

- [ ] **Step 2: Move `get_item_fulltext` into `items.rs`**

Append the following to `src/zotero/items.rs` (before the `#[cfg(test)]` module):

```rust
    /// Fetches Zotero's indexed full-text content for `item_key`, returning an
    /// empty string if the item is unindexed or missing text.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code
    /// - [`ZoteroMcpError::Network`] if the HTTP request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded
    pub(crate) async fn get_item_fulltext(
        &self,
        item_key: &ItemKey,
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
```

- [ ] **Step 3: Move tests into `items.rs` test module**

Add the fulltext tests to the existing `#[cfg(test)] mod tests` in `items.rs`:

```rust
    mod fulltext {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::zotero::test_http::{MockServer, http_response};

        fn state(zotero_api_url: impl AsRef<str>) -> AppState {
            AppState {
                zotero_api_url: zotero_api_url.as_ref().to_owned(),
                ..AppState::from_env()
            }
        }

        #[tokio::test]
        async fn returns_content_field() {
            let server = MockServer::new(vec![http_response(
                "200 OK",
                r#"{"content":"paper text"}"#,
            )]);
            let app = state(server.url());

            let result = ZoteroClient::new(&app)
                .get_item_fulltext(&ItemKey::from("ITEM0001"))
                .await;

            assert_eq!(result.ok().as_deref(), Some("paper text"));
        }

        #[tokio::test]
        async fn returns_empty_string_when_content_field_is_missing_or_not_string()
        {
            let server = MockServer::new(vec![
                http_response("200 OK", r"{}"),
                http_response("200 OK", r#"{"content":42}"#),
            ]);
            let app = state(server.url());
            let client = ZoteroClient::new(&app);

            let missing =
                client.get_item_fulltext(&ItemKey::from("ITEM0001")).await;
            let non_string =
                client.get_item_fulltext(&ItemKey::from("ITEM0002")).await;

            assert_eq!(missing.ok().as_deref(), Some(""));
            assert_eq!(non_string.ok().as_deref(), Some(""));
        }
    }
```

- [ ] **Step 4: Remove `mod fulltext;` from `src/zotero/mod.rs`**

In `src/zotero/mod.rs`, delete line 29: `mod fulltext;`

- [ ] **Step 5: Delete `src/zotero/fulltext.rs`**

Run: `rm src/zotero/fulltext.rs`

- [ ] **Step 6: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass (the fulltext tests now run from `items.rs`).

- [ ] **Step 7: Commit**

```bash
git add src/zotero/items.rs src/zotero/mod.rs
git rm src/zotero/fulltext.rs
git commit -m "refactor: merge fulltext.rs into items.rs"
```

---

### Task 6: Merge `zotero/attachments.rs` into `zotero/items.rs`

**Files:**
- Modify: `src/zotero/items.rs`, `src/zotero/mod.rs`
- Delete: `src/zotero/attachments.rs`

- [ ] **Step 1: Read both files**

Read `src/zotero/attachments.rs` (457 lines) and understand its content: `UploadTicket`, `attach_file_link()`, `import_pdf_file()`, and tests.

- [ ] **Step 2: Add imports to `items.rs`**

At the top of `items.rs`, add these imports (check which are already present):

```rust
use std::path::Path;
use md5::Digest;
use serde::Deserialize;
```

- [ ] **Step 3: Move `UploadTicket` struct into `items.rs`**

Add before the `impl ZoteroClient` block:

```rust
/// Phase-1 response payload from Zotero's file-upload endpoint.
#[derive(Deserialize)]
#[allow(dead_code, reason = "used after Task 3 wires the MCP handler")]
struct UploadTicket {
    /// Signed upload URL to `POST` the raw file bytes to.
    url: String,
    /// Upload key replayed in the finalize request.
    #[serde(rename = "uploadKey")]
    upload_key: String,
}
```

- [ ] **Step 4: Move `attach_file_link()` into `items.rs`**

Append to the `impl ZoteroClient<'_>` block:

```rust
    /// Attaches a linked file or URL to a parent library item.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
    pub(crate) async fn attach_file_link(
        &self,
        parent_item_key: &ItemKey,
        title: &str,
        file_path_or_url: &str,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": ItemType::Attachment,
            "parentItem": parent_item_key,
            "title": title,
            "linkMode": LinkMode::ImportedFile,
            "path": file_path_or_url,
            "contentType": content_type.unwrap_or("application/pdf"),
        }]);

        self.post_json_first(
            &url,
            &payload,
            "Created attachment array was empty",
        )
        .await
    }
```

- [ ] **Step 5: Move `import_pdf_file()` into `items.rs`**

Append to the same `impl ZoteroClient<'_>` block:

```rust
    /// Imports a local file into Zotero storage via a three-phase MD5
    /// upload sequence and returns the created attachment item.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroMcpError::InputRejected`] if the filepath has no valid UTF-8 filename.
    /// - [`ZoteroMcpError::Io`] if reading the local file fails.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero rejects any phase of the upload.
    /// - [`ZoteroMcpError::Network`] if a request fails at the HTTP transport level.
    /// - [`ZoteroMcpError::Json`] if a response body cannot be decoded.
    #[allow(dead_code, reason = "wired by MCP handler in Task 3")]
    pub(crate) async fn import_pdf_file(
        &self,
        parent_item_key: Option<&ItemKey>,
        title: &str,
        path: &Path,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;

        let bytes = tokio::fs::read(path).await?;

        let mut hasher = md5::Md5::new();
        hasher.update(&bytes);
        let mut md5_str = String::with_capacity(32);
        for byte in hasher.finalize() {
            use std::fmt::Write;
            let _ = write!(md5_str, "{byte:02x}");
        }

        let filename =
            path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                ZoteroMcpError::InputRejected(
                    "path has no valid UTF-8 filename".into(),
                )
            })?;

        let metadata = tokio::fs::metadata(path).await?;
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));

        let mut attachment = serde_json::Map::new();
        attachment.insert("itemType".into(), serde_json::json!(ItemType::Attachment));
        attachment.insert("title".into(), serde_json::json!(title));
        attachment.insert("linkMode".into(), serde_json::json!(LinkMode::ImportedFile));
        attachment.insert("filename".into(), serde_json::json!(filename));
        attachment.insert(
            "contentType".into(),
            serde_json::json!(content_type.unwrap_or("application/pdf")),
        );
        if let Some(parent) = parent_item_key {
            attachment.insert("parentItem".into(), serde_json::json!(parent));
        }
        let create_url = format!("{}/users/0/items", self.state.zotero_api_url);
        let item: ZoteroItem = self
            .post_json_first(
                &create_url,
                &serde_json::json!([attachment]),
                "Created attachment array was empty",
            )
            .await?;

        let file_url = format!(
            "{}/users/0/items/{}/file",
            self.state.zotero_api_url, item.data.key
        );
        let filesize_text = bytes.len().to_string();
        let mtime_text = modified_ms.to_string();
        let resp = self
            .state
            .client
            .post(&file_url)
            .form(&[
                ("md5", md5_str.as_str()),
                ("filename", filename),
                ("filesize", filesize_text.as_str()),
                ("mtime", mtime_text.as_str()),
            ])
            .header("If-None-Match", "*")
            .send()
            .await?;
        let body: serde_json::Value =
            self.ensure_success(resp).await?.json().await?;
        if body.as_object().is_some_and(|object| object.contains_key("exists"))
        {
            return Ok(item);
        }
        let ticket: UploadTicket = serde_json::from_value(body)?;

        let upload =
            self.state.client.post(&ticket.url).body(bytes).send().await?;
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

- [ ] **Step 6: Move attachment tests into `items.rs` test module**

Add a `mod attachments` test module inside the existing `#[cfg(test)] mod tests` in `items.rs`. Copy all tests from `src/zotero/attachments.rs:226-456` (the `mod tests` block).

- [ ] **Step 7: Remove `mod attachments;` from `src/zotero/mod.rs`**

Delete the line `mod attachments;` (line 24).

- [ ] **Step 8: Delete `src/zotero/attachments.rs`**

Run: `rm src/zotero/attachments.rs`

- [ ] **Step 9: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add src/zotero/items.rs src/zotero/mod.rs
git rm src/zotero/attachments.rs
git commit -m "refactor: merge attachments.rs into items.rs"
```

---

### Task 7: Merge `zotero/annotations.rs` into `zotero/notes.rs`

**Files:**
- Modify: `src/zotero/notes.rs`, `src/zotero/mod.rs`
- Delete: `src/zotero/annotations.rs`

- [ ] **Step 1: Read both files**

Read `src/zotero/annotations.rs` (348 lines) and `src/zotero/notes.rs` (121 lines).

- [ ] **Step 2: Move annotation types into `notes.rs`**

Add these imports to `notes.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::zotero::{AnnotationType, ItemType, ZoteroItem};
```

Move `AnnotationPosition` and `AnnotationDraft` structs (and their impls) into `notes.rs` before the `impl ZoteroClient` block.

- [ ] **Step 3: Move `create_annotation()` and `synthesize_annotations()` into `notes.rs`**

Add both methods to the `impl ZoteroClient<'_>` block in `notes.rs`.

- [ ] **Step 4: Move formatting helpers into `notes.rs`**

Move `format_annotations_section()` and `format_notes_section()` as free functions in `notes.rs`.

- [ ] **Step 5: Move all annotation tests into `notes.rs` test module**

Add a `mod annotations` block inside the existing `#[cfg(test)] mod tests`.

- [ ] **Step 6: Update re-exports in `src/zotero/mod.rs`**

Change line 41 from:

```rust
pub(crate) use annotations::{AnnotationDraft, AnnotationPosition};
```

to:

```rust
pub(crate) use notes::{AnnotationDraft, AnnotationPosition};
```

Remove `mod annotations;` (line 23).

- [ ] **Step 7: Delete `src/zotero/annotations.rs`**

Run: `rm src/zotero/annotations.rs`

- [ ] **Step 8: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/zotero/notes.rs src/zotero/mod.rs
git rm src/zotero/annotations.rs
git commit -m "refactor: merge annotations.rs into notes.rs"
```

---

### Task 8: Merge `zotero/coverage.rs` into `zotero/search.rs`

**Files:**
- Modify: `src/zotero/search.rs`, `src/zotero/mod.rs`
- Delete: `src/zotero/coverage.rs`

- [ ] **Step 1: Read both files**

Read `src/zotero/coverage.rs` (467 lines) and understand its types (`ItemCoverageFlags`, `LibraryCoverage`, `LibraryCoveragePage`) and functions.

- [ ] **Step 2: Move coverage types into `search.rs`**

Add these imports to `search.rs` (check which are already present):

```rust
use crate::zotero::{CollectionKey, ItemType};
```

Move the following into `search.rs` (before the `#[cfg(test)]` module):
- `ItemCoverageFlags` struct
- `LibraryCoverage` struct
- `LibraryCoveragePage` struct
- `coverage_flags()` function
- `coverage_pagination()` function
- `classify_coverage_page()` function
- `classify_coverage()` function
- `compute_percentage()` function
- `get_library_coverage()` method on `ZoteroClient`

- [ ] **Step 3: Move coverage tests into `search.rs` test module**

Add a `mod coverage` block inside the existing `#[cfg(test)] mod tests`.

- [ ] **Step 4: Remove `mod coverage;` from `src/zotero/mod.rs`**

Delete the line `mod coverage;` (line 27).

- [ ] **Step 5: Delete `src/zotero/coverage.rs`**

Run: `rm src/zotero/coverage.rs`

- [ ] **Step 6: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/zotero/search.rs src/zotero/mod.rs
git rm src/zotero/coverage.rs
git commit -m "refactor: merge coverage.rs into search.rs"
```

---

### Task 9: Merge `zotero/duplicates.rs` into `zotero/search.rs`

**Files:**
- Modify: `src/zotero/search.rs`, `src/zotero/mod.rs`
- Delete: `src/zotero/duplicates.rs`

- [ ] **Step 1: Read both files**

Read `src/zotero/duplicates.rs` (326 lines) and understand its types (`DuplicateType`, `DuplicateGroup`) and function (`find_duplicate_groups`).

- [ ] **Step 2: Move duplicate types and functions into `search.rs`**

Move into `search.rs` (before the `#[cfg(test)]` module):
- `DuplicateType` enum
- `DuplicateGroup` struct
- `find_duplicates()` method on `ZoteroClient`
- `find_duplicate_groups()` free function

- [ ] **Step 3: Move duplicate tests into `search.rs` test module**

Add a `mod duplicates` block inside the existing `#[cfg(test)] mod tests`.

- [ ] **Step 4: Remove `mod duplicates;` from `src/zotero/mod.rs`**

Delete the line `mod duplicates;` (line 28).

- [ ] **Step 5: Delete `src/zotero/duplicates.rs`**

Run: `rm src/zotero/duplicates.rs`

- [ ] **Step 6: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/zotero/search.rs src/zotero/mod.rs
git rm src/zotero/duplicates.rs
git commit -m "refactor: merge duplicates.rs into search.rs"
```

---

### Task 10: Eliminate `connector_tools.rs`

**Files:**
- Modify: `src/mcp/mod.rs`, `src/mcp/server.rs`, `src/mcp/catalog.rs`, `src/mcp/zotero.rs`, `src/mcp/zotero/search.rs`, `src/mcp/zotero/items.rs`
- Delete: `src/mcp/connector_tools.rs`

- [ ] **Step 1: Read `src/mcp/connector_tools.rs`**

Understand the two tools: `search` wraps `zotero_search_items_impl`, `fetch` wraps `zotero_get_item_metadata_impl`.

- [ ] **Step 2: Move `search` tool into `src/mcp/zotero/search.rs`**

Move `SearchArgs` struct and the `connector_search` tool definition + `connector_search_impl` into `search.rs`. Add the `#[tool_router]` attribute for `connector_search` using the existing `search_router` (or create a separate inline tool).

Actually, since `search` is a **separate MCP tool** (not an action on `zotero_search`), it needs its own tool definition. The cleanest approach: add the `search` tool definition directly in `search.rs` by adding it to the existing `#[tool_router]` block. The `connector_router` is currently separate — merge it.

In `search.rs`, add after the existing tool definitions:

```rust
    #[tool(
        name = "search",
        description = "Connector search tool - search Zotero items by query",
        annotations(
            title = "Search Zotero",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn connector_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_items_impl(
            crate::mcp::zotero::SearchItemsArgs::for_connector(args.query),
        )
        .await
    }
```

Where `SearchArgs` is:

```rust
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ConnectorSearchArgs {
    pub(crate) query: String,
}
```

- [ ] **Step 3: Move `fetch` tool into `src/mcp/zotero/items.rs`**

Similarly, add the `fetch` tool to `items.rs`:

```rust
    #[tool(
        name = "fetch",
        description = "Connector fetch tool - get Zotero item metadata by item ID/key",
        annotations(
            title = "Fetch Zotero Item",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures.
    pub(crate) async fn connector_fetch(
        &self,
        Parameters(args): Parameters<ConnectorFetchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_get_item_metadata_impl(
            crate::mcp::zotero::GetItemMetadataArgs::json(args.id.into()),
        )
        .await
    }
```

Where:

```rust
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ConnectorFetchArgs {
    pub(crate) id: String,
}
```

- [ ] **Step 4: Remove `connector_router` from `server.rs`**

In `src/mcp/server.rs`, delete line 86: `router.merge(Self::connector_router());`

- [ ] **Step 5: Remove `mod connector_tools;` from `src/mcp/mod.rs`**

Delete line 38: `mod connector_tools;`

- [ ] **Step 6: Update `catalog.rs` `is_tool_visible()`**

The `is_tool_visible()` function (line 248) lists `"search"` and `"fetch"` as always-visible tools. These are now part of `search_router` and `items_router` respectively, so they remain always-visible. No change needed for visibility — but the function is being rewritten in Task 12 anyway.

- [ ] **Step 7: Update `SERVER_INSTRUCTIONS` in `server.rs`**

Remove the sentence about "search and fetch are connector compatibility tools" from `SERVER_INSTRUCTIONS` (line 39). Or keep it — it's still accurate.

- [ ] **Step 8: Delete `src/mcp/connector_tools.rs`**

Run: `rm src/mcp/connector_tools.rs`

- [ ] **Step 9: Update tests in `server.rs`**

The test `visible_tools_lists_base_grouped_tools_only` (line 323) expects `"search"` and `"fetch"` in the visible tools list. They should still appear since they're now part of `search_router` and `items_router`. Verify the test still passes.

- [ ] **Step 10: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass.

- [ ] **Step 11: Commit**

```bash
git add src/mcp/mod.rs src/mcp/server.rs src/mcp/zotero/search.rs src/mcp/zotero/items.rs
git rm src/mcp/connector_tools.rs
git commit -m "refactor: eliminate connector_tools.rs, inline search/fetch into domain modules"
```

---

### Task 11: Deduplicate note-filtering in `mcp/resources.rs`

**Files:**
- Modify: `src/mcp/zotero/notes.rs`, `src/mcp/resources.rs`

- [ ] **Step 1: Read `src/mcp/zotero/notes.rs:159-174` and `src/mcp/resources.rs:108-113`**

Confirm the duplication: `notes.rs` filters children inline, `resources.rs` has `note_children()`.

- [ ] **Step 2: Add `filter_notes` to `src/mcp/zotero/notes.rs`**

Add a public helper function in the `notes.rs` MCP handler module:

```rust
/// Filters child items to only those with `ItemType::Note`.
pub(crate) fn filter_notes(children: Vec<ZoteroItem>) -> Vec<ZoteroItem> {
    children
        .into_iter()
        .filter(|child| child.data.item_type == ItemType::Note)
        .collect()
}
```

- [ ] **Step 3: Use `filter_notes` in `notes.rs` handler**

Replace lines 166-169 in `zotero_get_notes_impl`:

```rust
// Before:
let notes: Vec<_> = children
    .into_iter()
    .filter(|c| c.data.item_type == ItemType::Note)
    .collect();

// After:
let notes = filter_notes(children);
```

- [ ] **Step 4: Use `filter_notes` in `resources.rs`**

Replace the `note_children()` function and its call site:

In `resources.rs`, change line 398-401 from:

```rust
Some("notes") => client
    .get_item_children(&item_key)
    .await
    .map(|children| json_resource(uri, &note_children(children))),
```

to:

```rust
Some("notes") => client
    .get_item_children(&item_key)
    .await
    .map(|children| json_resource(uri, &super::zotero::notes::filter_notes(children))),
```

Delete the `note_children` function (lines 105-113).

- [ ] **Step 5: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/mcp/zotero/notes.rs src/mcp/resources.rs
git commit -m "refactor: deduplicate note-filtering with shared filter_notes helper"
```

---

### Task 12: Derive `is_tool_visible`/`is_write_tool` from `PRIMITIVES`

**Files:**
- Modify: `src/mcp/catalog.rs`

- [ ] **Step 1: Read `src/mcp/catalog.rs` lines 233-305**

Understand `is_write_tool()`, `is_tool_visible()`, and `is_primitive_enabled()`.

- [ ] **Step 2: Add a `tool_gates` lookup function**

Add a helper that looks up a tool's gates from `PRIMITIVES`:

```rust
/// Returns the env gates for a tool by name, or `None` if not found in the
/// catalog.
fn tool_gates(name: &str) -> Option<&'static [EnvGate]> {
    PRIMITIVES
        .iter()
        .find(|p| p.kind == PrimitiveKind::Tool && p.name == name)
        .map(|p| p.requires)
}
```

- [ ] **Step 3: Rewrite `is_write_tool`**

```rust
/// Returns `true` if `name` is a write (mutating) tool gated behind
/// `ZOTERO_WRITE_ENABLED`.
pub(crate) fn is_write_tool(name: &str) -> bool {
    tool_gates(name)
        .is_some_and(|gates| gates.contains(&EnvGate::WriteEnabled))
}
```

- [ ] **Step 4: Rewrite `is_tool_visible`**

```rust
/// Returns `true` if `name` is currently advertised to MCP clients given
/// `state`'s feature gates.
pub(crate) fn is_tool_visible(state: &AppState, name: &str) -> bool {
    let Some(gates) = tool_gates(name) else {
        // Tool not in PRIMITIVES — not a known tool, hide it.
        return false;
    };
    gates.iter().all(|gate| match gate {
        EnvGate::WriteEnabled => state.write_enabled,
        EnvGate::SqliteAccess => state.sqlite_access,
        EnvGate::SemanticSearchEnabled => state.semantic_search_enabled,
    })
}
```

Note: This also **fixes the bug** where `is_primitive_enabled` (line 299) didn't check `SemanticSearchEnabled`.

- [ ] **Step 5: Run the full gate**

Run: `mise x -- cargo clippy --workspace --all-targets --all-features -- -D warnings && mise x -- cargo nextest run -p zotero-mcp-rs`
Expected: all tests pass. The `visible_tools_lists_base_grouped_tools_only` test should now also account for `"search"` and `"fetch"` being visible (they have no gates).

- [ ] **Step 6: Commit**

```bash
git add src/mcp/catalog.rs
git commit -m "refactor: derive tool visibility from PRIMITIVES catalog"
```

---

### Task 13: Update module documentation

**Files:**
- Modify: `src/zotero/mod.rs`, `src/mcp/zotero.rs`, `src/mcp/mod.rs`

- [x] **Step 1: Update `src/zotero/mod.rs` doc comment**

Already accurate: the top-of-file doc never listed submodules by name (no
"# Submodules" section), and tasks 5–9 removed the `mod fulltext;` /
`mod attachments;` / `mod coverage;` / `mod duplicates;` / `mod annotations;`
lines as part of their own commits. No stale references found.

- [x] **Step 2: Update `src/mcp/zotero.rs` doc comment**

No change needed. Its `mod annotations; mod attachments; mod coverage;
mod duplicates; mod fulltext;` are genuine, still-existing MCP **handler**
submodules under `src/mcp/zotero/` — distinct from the deleted **domain**
modules of the same name under `src/zotero/`. The plan conflated the two;
verified by file listing (`src/mcp/zotero/*.rs` still has all 15 files) and
by confirming the domain-layer merges only ever touched `src/zotero/mod.rs`.

- [x] **Step 3: Update `src/mcp/mod.rs` doc comment**

Already accurate: the `connector_tools` doc line and `mod connector_tools;`
were removed together in the Task 10 commit (2006350).

- [x] **Step 4: Run the full gate**

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo nextest run -p zotero-mcp-rs`: all
green, 358 tests passed, zero warnings.

- [x] **Step 5: Commit**

No diff to commit — docs were already correct. No-op, confirmed by
`git status`/`git diff --stat` showing a clean tree.

---

## Summary

| Task | What | Files deleted | Net lines |
|------|------|:---:|---:|
| 1 | Create `macros.rs` | 0 | +80 |
| 2 | Replace `string_key!` | 0 | -40 |
| 3 | Replace `string_value!` | 0 | -30 |
| 4 | Replace `TemplateName` impls | 0 | -30 |
| 5 | Merge `fulltext.rs` → `items.rs` | 1 | -63 |
| 6 | Merge `attachments.rs` → `items.rs` | 1 | -57 |
| 7 | Merge `annotations.rs` → `notes.rs` | 1 | -48 |
| 8 | Merge `coverage.rs` → `search.rs` | 1 | -47 |
| 9 | Merge `duplicates.rs` → `search.rs` | 1 | -46 |
| 10 | Eliminate `connector_tools.rs` | 1 | -206 |
| 11 | Deduplicate note-filtering | 0 | -5 |
| 12 | Derive visibility from `PRIMITIVES` | 0 | -30 |
| 13 | Update module docs | 0 | ~0 |
| **Total** | | **6 files deleted** | **~-422** |

**Result:** 30 → 24 source files, 14 → 11 routers, ~422 lines removed, zero functional changes.
