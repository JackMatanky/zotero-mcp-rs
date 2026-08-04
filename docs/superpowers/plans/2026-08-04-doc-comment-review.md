# Rust Doc Comment Review & Revision Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Revise all module-level and public-item doc comments across every `src/**/*.rs` file to fully follow Rust doc conventions.

**Architecture:** The codebase is already well-documented. Only 3 files need actual edits. The remaining ~55 files are verification-only.

**Tech Stack:** Rust, `cargo doc`, `harper` CLI (grammar checker)

---

## Conventions (per rust-doc skill)

1. **Every doc comment starts with a single-line summary** — no "This function..." prefix
2. **No `# Returns` sections** — describe returns inline in main text
3. **Every `Result`-returning function gets `# Errors`** with intra-doc links to error variants
4. **Intra-doc links** for all type references: [`TypeName`]
5. **No docs on standard trait impls** (`Debug`, `Clone`, `From<String>` for enums, etc.)
6. **Struct fields**: document only non-obvious fields; skip `id`, `name`, `key`, `title`
7. **Enum variants**: document only when behavior is non-obvious
8. **Module docs (`//!`)**: list main types with intra-doc links
9. **Use lists** for readability when >2 items
10. **No em-dashes** unless grammar strictly dictates

---

## Phase 1: Edits

### Task 1: `src/zotero/annotations.rs`

Already has `//!` module doc (line 1). Only needs a doc comment on the private `as_zotero_string()` method.

**Before** (line 18):
```rust
fn as_zotero_string(&self) -> String {
```

**After**:
```rust
/// Serializes the position to the JSON string expected by the Zotero API.
fn as_zotero_string(&self) -> String {
```

No `# Errors` needed (infallible, private). No doc on `From<serde_json::Value>` (standard trait impl).

---

### Task 2: `src/mcp/catalog.rs`

Already has `//!` module doc (line 1). Needs docs on 3 enums, their variants, and `PrimitiveInfo` struct.

**Before** (lines 19-27):
```rust
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PrimitiveKind {
    Tool,
    Resource,
    Prompt,
}
```

**After**:
```rust
/// Kind of MCP primitive (tool, resource, or prompt).
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PrimitiveKind {
    /// A callable MCP tool.
    Tool,
    /// A readable MCP resource.
    Resource,
    /// An MCP prompt template.
    Prompt,
}
```

---

**Before** (lines 29-42):
```rust
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrimitiveDomain {
    Discovery,
    Items,
    Collections,
    Search,
    Notes,
    Sqlite,
    Semantic,
    Prompts,
}
```

**After**:
```rust
/// Functional domain grouping for MCP primitives.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrimitiveDomain {
    /// Discovery and introspection tools.
    Discovery,
    /// Item read/write operations.
    Items,
    /// Collection operations.
    Collections,
    /// Search operations.
    Search,
    /// Note operations.
    Notes,
    /// Direct SQLite database queries.
    Sqlite,
    /// Semantic search operations.
    Semantic,
    /// Prompt templates.
    Prompts,
}
```

---

**Before** (lines 44-52):
```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum EnvGate {
    #[serde(rename = "ZOTERO_WRITE_ENABLED")]
    WriteEnabled,
    #[serde(rename = "ZOTERO_SQLITE_ACCESS")]
    SqliteAccess,
    #[serde(rename = "ZOTERO_SEMANTIC_SEARCH")]
    SemanticSearchEnabled,
}
```

**After**:
```rust
/// Environment variable that gates access to a group of tools.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum EnvGate {
    /// Requires `ZOTERO_WRITE_ENABLED=1` for write tools.
    #[serde(rename = "ZOTERO_WRITE_ENABLED")]
    WriteEnabled,
    /// Requires `ZOTERO_SQLITE_ACCESS=1` for SQLite tools.
    #[serde(rename = "ZOTERO_SQLITE_ACCESS")]
    SqliteAccess,
    /// Requires `ZOTERO_SEMANTIC_SEARCH=1` for semantic tools.
    #[serde(rename = "ZOTERO_SEMANTIC_SEARCH")]
    SemanticSearchEnabled,
}
```

---

**Before** (lines 54-64):
```rust
#[derive(Clone, Copy, Serialize)]
struct PrimitiveInfo {
    name: &'static str,
    kind: PrimitiveKind,
    domain: PrimitiveDomain,
    requires: &'static [EnvGate],
    summary: &'static str,
    example: Option<&'static str>,
    #[serde(skip_serializing)]
    search_text: &'static str,
}
```

**After**:
```rust
/// Metadata for a single discoverable MCP primitive.
#[derive(Clone, Copy, Serialize)]
struct PrimitiveInfo {
    name: &'static str,
    kind: PrimitiveKind,
    domain: PrimitiveDomain,
    requires: &'static [EnvGate],
    summary: &'static str,
    example: Option<&'static str>,
    #[serde(skip_serializing)]
    search_text: &'static str,
}
```

---

### Task 3: `src/mcp/resources.rs`

Already has `//!` module docs (lines 1-19). `json_resource` already has a doc (line 236). Needs docs on 6 private helpers.

**Before** (line 49):
```rust
fn text_resource_template(
```

**After**:
```rust
/// Builds a [`ResourceTemplate`](rmcp::model::ResourceTemplate) for a
/// `zotero://` plain-text resource URI.
fn text_resource_template(
```

---

**Before** (line 61):
```rust
fn note_children(children: Vec<ZoteroItem>) -> Vec<ZoteroItem> {
```

**After**:
```rust
/// Filters child items to only notes.
fn note_children(children: Vec<ZoteroItem>) -> Vec<ZoteroItem> {
```

---

**Before** (line 253):
```rust
fn text_resource(uri: &str, text: &str) -> rmcp::model::ReadResourceResult {
```

**After**:
```rust
/// Wraps plain text in a [`ReadResourceResult`](rmcp::model::ReadResourceResult)
/// for `uri`.
fn text_resource(uri: &str, text: &str) -> rmcp::model::ReadResourceResult {
```

---

**Before** (line 260):
```rust
async fn read_item_resource(
    client: &ZoteroClient<'_>,
    uri: &str,
    rest: &str,
) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
```

**After**:
```rust
/// Reads a single Zotero item by key and returns its JSON as a resource.
///
/// Supports nested sub-resources: `children`, `notes`, `fulltext`,
/// `relations`.
async fn read_item_resource(
    client: &ZoteroClient<'_>,
    uri: &str,
    rest: &str,
) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
```

---

**Before** (line 299):
```rust
async fn read_collection_resource(
    client: &ZoteroClient<'_>,
    uri: &str,
    rest: &str,
) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
```

**After**:
```rust
/// Reads a Zotero collection by key, optionally returning its items.
async fn read_collection_resource(
    client: &ZoteroClient<'_>,
    uri: &str,
    rest: &str,
) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
```

---

**Before** (line 337):
```rust
fn resource_error(error: impl std::fmt::Display) -> rmcp::ErrorData {
```

**After**:
```rust
/// Wraps an error into an [`rmcp::ErrorData`] for resource read failures.
fn resource_error(error: impl std::fmt::Display) -> rmcp::ErrorData {
```

---

**Before** (line 341):
```rust
fn unknown_resource(uri: &str) -> rmcp::ErrorData {
```

**After**:
```rust
/// Returns an [`rmcp::ErrorData`] for unrecognized resource URIs.
fn unknown_resource(uri: &str) -> rmcp::ErrorData {
```

---

## Phase 2: Verification-only files (no edits)

These files already have correct `//!` module docs, `# Errors` sections where needed, no `# Returns` sections, and proper intra-doc links. Verification passes only.

**Top-level modules:**
- `src/main.rs`
- `src/errors.rs`
- `src/state.rs`
- `src/security.rs`
- `src/pdf.rs`

**`src/zotero/` core domain (all have `//!` module docs):**
- `src/zotero/mod.rs`, `client.rs`, `types.rs`, `keys.rs`, `objects.rs`
- `src/zotero/attachments.rs`, `collections.rs`, `duplicates.rs`, `fulltext.rs`
- `src/zotero/items.rs`, `metadata.rs`, `notes.rs`, `relations.rs`
- `src/zotero/search.rs`, `src/zotero/sqlite.rs`, `src/zotero/tags.rs`

**`src/better_bibtex/` bridge:**
- `src/better_bibtex/mod.rs`, `client.rs`, `models.rs`

**`src/better_notes/` bridge:**
- `src/better_notes/mod.rs`, `client.rs`, `models.rs`

**`src/semantic_search/` embedding layer:**
- `src/semantic_search/mod.rs`, `chunking.rs`, `embedding.rs`, `index.rs`, `search.rs`, `store.rs`

**`src/mcp/` server layer:**
- `src/mcp/mod.rs`, `server.rs`, `connector_tools.rs`, `pdf.rs`
- `src/mcp/better_bibtex.rs`, `better_notes.rs`, `semantic_search.rs`
- `src/mcp/zotero.rs`
- `src/mcp/zotero/status.rs`, `items.rs`, `collections.rs`, `annotations.rs`
- `src/mcp/zotero/attachments.rs`, `notes.rs`, `tags.rs`, `relations.rs`
- `src/mcp/zotero/search.rs`, `sqlite.rs`, `duplicates.rs`, `fulltext.rs`
- `src/mcp/zotero/metadata.rs`, `pdf.rs`, `coverage.rs`

---

## Phase 3: Verification

### Task 4: `cargo doc`
- [ ] Run: `cargo doc --no-deps --all-features 2>&1`
- [ ] Fix any broken intra-doc links

### Task 5: `harper` grammar check
- [ ] Run: `harper check src/zotero/annotations.rs src/mcp/catalog.rs src/mcp/resources.rs`
- [ ] Fix any grammar issues flagged

### Task 6: Spot-check 5 random files
- [ ] Module docs exist
- [ ] Public functions have `# Errors` sections (if fallible)
- [ ] No `# Returns` sections
- [ ] Intra-doc links present for type references
- [ ] No em-dashes unless grammar requires

---

## Summary

| File                     | Change                                                                                         |
| ------------------------ | ---------------------------------------------------------------------------------------------- |
| `src/zotero/annotations.rs` | Add doc to private `as_zotero_string()`                                                         |
| `src/mcp/catalog.rs`        | Add docs to 3 enums (`PrimitiveKind`, `PrimitiveDomain`, `EnvGate`) and their variants + `PrimitiveInfo` |
| `src/mcp/resources.rs`      | Add docs to 6 private helpers                                                                  |
