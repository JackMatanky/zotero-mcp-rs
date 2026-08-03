# Semantic Search Refactors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply the eight code-quality findings from the `semantic_search/` review — newtypes (`ItemKey`, `Embedding`), an outcome enum, `Default`-derived report struct, two deduplications against `zotero/`, best-effort fulltext error logging, and a SQLite path robustness fix.

**Architecture:** The `semantic_search/` module owns a SQLite side-car index (`store.rs`), ONNX embedding via `fastembed` behind the `EmbeddingProvider` trait (`embedding.rs`), paragraph-aware chunking (`chunking.rs`), a whole-library indexer (`index.rs`), and a MaxSim cosine search (`search.rs`). Most changes are type-level: replace raw `String`/`Vec<f32>` identities with the crate's existing newtype idiom (`ItemKey` from `zotero/keys.rs`, a new `Embedding` type), deduplicate two predicates already implemented in `zotero/`, and tighten a few structural warts. Each task compiles and passes tests on its own.

**Tech Stack:** Rust (nightly via mise), `tokio`, `sqlx` 0.8 (SQLite), `fastembed`, `thiserror`, `serde`/`schemars`, `tracing`. Tests use inline `#[cfg(test)] mod tests` with `pretty_assertions`, `tempfile`, and the crate's `MockServer` HTTP test harness.

---

## Conventions & How to Work (read first)

- **Task runner.** All commands run through `mise`. `cargo`/`rustup` binaries come from `mise` (nightly toolchain). If `cargo` is not on your PATH, prefix with `mise exec -- cargo ...`.
- **Verification gate.** The project's actual pre-commit bar is `mise run verify` (fmt-check + clippy across all targets/features with `-D warnings` + full test suite incl. doctests). Run it at the end of every task before committing. During a task use the faster targeted `cargo nextest run -E 'test(<name>)'` loop.
- **Formatting.** `mise run fmt` formats everything (nightly rustfmt).
- **Visibility.** Almost everything in this crate is `pub(crate)`. Keep it that way.
- **Doc comments.** Every public item and every fallible `fn` has `///` docs including a `# Errors` section listing error variants. Match the surrounding style — these are real docs, not placeholders.
- **Commits.** Conventional Commits (`refactor:`, `fix:`), staged explicitly (`git add <exact files>`). Commit per task.
- **Tests live inline** in the same file as the code under `#[cfg(test)] mod tests`, using `use super::*;` and `pretty_assertions::assert_eq`. A test helper `MockServer` (in `zotero/mod.rs`) serves canned HTTP responses in order — it does **not** inspect request paths, so URL changes in tests are invisible.
- `item_key` strings in DB APIs are a **storage detail**; the typed boundary is at the Rust API. `ItemKey` implements `as_str()`, `From<String>`, `From<&str>`, `Display`, `Clone`, `Hash`, `Ord`, `PartialEq<str>`, `PartialEq<&str>` (see `zotero/keys.rs`).

---

### Task 1: Deduplicate the "indexable item type" predicate onto `ItemType::is_indexable`

**Finding:** `semantic_search/index.rs::is_indexable_item` is a verbatim copy of `zotero/search.rs::is_searchable_item`. The predicate belongs on the `ItemType` enum.

**Files:**
- Modify: `src/zotero/types.rs` (add method + unit test)
- Modify: `src/zotero/search.rs:567-573` (reuse method)
- Modify: `src/semantic_search/index.rs:70,161-170` (reuse method, drop helper)

- [ ] **Step 1: Write the failing test**

Add to `src/zotero/types.rs`, inside the existing `mod item_type { ... }` test module (after `defaults_to_other_variant`):

```rust
        #[test]
        fn is_indexable_excludes_attachments_notes_and_annotations() {
            for item_type in [
                ItemType::Attachment,
                ItemType::Note,
                ItemType::Annotation,
            ] {
                assert!(
                    !item_type.is_indexable(),
                    "{item_type:?} must not be indexable"
                );
            }
            assert!(ItemType::JournalArticle.is_indexable());
            assert!(ItemType::Other("webpage".to_owned()).is_indexable());
        }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -E 'test(is_indexable_excludes_attachments)'`
Expected: FAIL — compile error, `no method named \`is_indexable\` found for enum \`ItemType\``.

- [ ] **Step 3: Implement `ItemType::is_indexable`**

In `src/zotero/types.rs`, add this method inside the existing `impl ItemType` block (next to `as_str`, after it):

```rust
    /// Returns `true` for item types eligible for search and embedding
    /// indexing: everything except attachments, notes, and annotations.
    #[inline]
    pub(crate) fn is_indexable(&self) -> bool {
        !matches!(self, Self::Attachment | Self::Note | Self::Annotation)
    }
```

- [ ] **Step 4: Reuse it in `zotero/search.rs`**

In `src/zotero/search.rs`, replace the whole `is_searchable_item` body (currently at line 568):

```rust
/// Returns true for items that are not attachments, notes, or annotations.
fn is_searchable_item(item: &ZoteroItem) -> bool {
    item.data.item_type.is_indexable()
}
```

- [ ] **Step 5: Reuse it in `semantic_search/index.rs` and drop the copy**

In `src/semantic_search/index.rs`:

1. Replace the call site at line 70:

```rust
        if item.data.deleted || !is_indexable_item(&item.data.item_type) {
```

with:

```rust
        if item.data.deleted || !item.data.item_type.is_indexable() {
```

2. Delete the `is_indexable_item` helper (lines 161-170, including its doc comment).

3. Update the `zotero::` import — `ItemType` is no longer referenced, so change

```rust
    zotero::{ItemType, ZoteroClient, ZoteroItem},
```

to:

```rust
    zotero::{ZoteroClient, ZoteroItem},
```

- [ ] **Step 6: Run the full gate**

Run: `mise run fmt && mise run verify`
Expected: PASS — formatting clean, clippy clean, all tests pass (the pre-existing `excludes_notes_attachments_and_annotations_on_slow_path` in `search.rs` and `skips_items_with_no_indexable_text` in `index.rs` now exercise the shared method).

- [ ] **Step 7: Commit**

```bash
git add src/zotero/types.rs src/zotero/search.rs src/semantic_search/index.rs
git commit -m "refactor(semantic-search): share ItemType::is_indexable predicate"
```

---

### Task 2: Reuse `ZoteroClient::get_all_items` for the library scan

**Finding:** `index_library` hand-builds `{base_url}/users/0/items` and paginates with its own `SCAN_PAGE_SIZE`, duplicating `ZoteroClient::get_all_items` (`zotero/items.rs:59`, currently `pub(super)` so invisible to the sibling module). `get_all_items` also excludes notes server-side, which the current scan already filters out in code.

**Files:**
- Modify: `src/zotero/items.rs:59` (visibility `pub(super)` → `pub(crate)`)
- Modify: `src/semantic_search/index.rs:20,55-57` (reuse; drop const + URL)

- [ ] **Step 1: Widen visibility of `get_all_items`**

In `src/zotero/items.rs:59`, change `pub(super) async fn get_all_items` to `pub(crate) async fn get_all_items`.

- [ ] **Step 2: Replace the manual scan in `index_library`**

In `src/semantic_search/index.rs`:

1. Delete the `SCAN_PAGE_SIZE` const (line 20).

2. Replace lines 55-57:

```rust
    let url = format!("{}/users/0/items", client.base_url());
    let all_items: Vec<ZoteroItem> =
        client.get_all_json(&url, SCAN_PAGE_SIZE).await?;
```

with:

```rust
    let all_items: Vec<ZoteroItem> = client.get_all_items().await?;
```

- [ ] **Step 3: Run the index tests**

Run: `cargo nextest run -E 'test(indexes_new_items_with_title_and_abstract)' && cargo nextest run -E 'test(skips_unchanged_items_on_second_run)' && cargo nextest run -E 'test(deletes_items_no_longer_in_library)'`
Expected: PASS. Behavior is unchanged (MockServer ignores query strings; `get_all_items` pages with the same 100-item page size).

- [ ] **Step 4: Run the full gate**

Run: `mise run fmt && mise run verify`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/zotero/items.rs src/semantic_search/index.rs
git commit -m "refactor(semantic-search): reuse ZoteroClient::get_all_items for the scan"
```

---

### Task 3: Derive `Default` for `IndexReport`

**Finding:** `index_library` hand-zeroes six `usize` counters (all start at 0). `IndexReport` already derives `Clone, Debug, Serialize`.

**Files:**
- Modify: `src/semantic_search/index.rs:23-31,59-66`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `src/semantic_search/index.rs`:

```rust
    #[test]
    fn index_report_defaults_to_all_zeroes() {
        let report = IndexReport::default();
        assert_eq!(report.items_scanned, 0);
        assert_eq!(report.items_indexed, 0);
        assert_eq!(report.items_skipped_unchanged, 0);
        assert_eq!(report.items_skipped_empty, 0);
        assert_eq!(report.items_deleted, 0);
        assert_eq!(report.chunks_written, 0);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -E 'test(index_report_defaults_to_all_zeroes)'`
Expected: FAIL — `the trait \`Default\` is not implemented for \`IndexReport\``.

- [ ] **Step 3: Derive `Default` and use `::default()`**

In `src/semantic_search/index.rs`:

1. Change the derive on `IndexReport` (line 23):

```rust
#[derive(Clone, Debug, Default, Serialize)]
```

2. Replace the manual initializer (lines 59-66):

```rust
    let mut report = IndexReport {
        items_scanned: 0,
        items_indexed: 0,
        items_skipped_unchanged: 0,
        items_skipped_empty: 0,
        items_deleted: 0,
        chunks_written: 0,
    };
```

with:

```rust
    let mut report = IndexReport::default();
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -E 'test(index_report_defaults_to_all_zeroes)'`
Expected: PASS.

- [ ] **Step 5: Run the full gate**

Run: `mise run fmt && mise run verify`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/semantic_search/index.rs
git commit -m "refactor(semantic-search): derive Default for IndexReport"
```

---

### Task 4: Model per-item index outcome as an enum

**Finding:** `index_one_item` returns `Result<bool, _>` (`false` = "no indexable text"), while "unchanged" is a separate early check bumping a different counter. Three mutually exclusive outcomes (`Indexed` / `SkippedUnchanged` / `SkippedEmpty`) are spread across a `bool` return plus two branches.

**Files:**
- Modify: `src/semantic_search/index.rs:69-95,109-159`

- [ ] **Step 1: Add the `IndexOutcome` enum**

In `src/semantic_search/index.rs`, add at module level (after the `SCAN_PAGE_SIZE`-free imports / before `IndexReport`):

```rust
/// Per-item outcome of the library scan, used to bump exactly one
/// `IndexReport` counter per item.
enum IndexOutcome {
    Indexed,
    SkippedUnchanged,
    SkippedEmpty,
}
```

- [ ] **Step 2: Change `index_one_item` to return the outcome**

Replace the `index_one_item` function (lines 109-159) with this version — same body, but it returns `IndexOutcome`, and it now also increments `chunks_written` for both success paths where relevant (the unchanged `SkippedEmpty` early-returns stay early):

```rust
async fn index_one_item(
    client: &ZoteroClient<'_>,
    index: &SemanticIndex,
    provider: &Arc<dyn EmbeddingProvider>,
    item: &ZoteroItem,
    report: &mut IndexReport,
) -> Result<IndexOutcome, ZoteroMcpError> {
    let text = assemble_item_text(client, item).await?;
    let text = if text.chars().count() > MAX_INDEXABLE_CHARS {
        text.chars().take(MAX_INDEXABLE_CHARS).collect()
    } else {
        text
    };
    if text.trim().is_empty() {
        return Ok(IndexOutcome::SkippedEmpty);
    }

    let pieces = chunk_text(&text, MAX_CHUNK_CHARS);
    if pieces.is_empty() {
        return Ok(IndexOutcome::SkippedEmpty);
    }
    let mut vectors = provider.embed(&pieces)?;
    for vector in &mut vectors {
        normalize(vector);
    }
    let new_chunks: Vec<NewChunk> = pieces
        .into_iter()
        .zip(vectors)
        .enumerate()
        .map(|(idx, (chunk_text, embedding))| NewChunk {
            chunk_index: i64::try_from(idx).unwrap_or(i64::MAX),
            chunk_text,
            embedding,
        })
        .collect();
    report.chunks_written =
        report.chunks_written.saturating_add(new_chunks.len());
    index
        .upsert_item(
            item.key.as_str(),
            item.data.title.as_deref(),
            item.data.date_modified.as_deref(),
            &new_chunks,
        )
        .await?;
    Ok(IndexOutcome::Indexed)
}
```

- [ ] **Step 3: Rewrite the scan loop to match on the outcome**

In `src/semantic_search/index.rs`, replace the loop body (lines 69-86) with:

```rust
    for item in &all_items {
        if item.data.deleted || !item.data.item_type.is_indexable() {
            continue;
        }
        report.items_scanned = report.items_scanned.saturating_add(1);
        current_keys.insert(item.key.to_string());

        let outcome = if !force && is_unchanged(index, item).await? {
            IndexOutcome::SkippedUnchanged
        } else {
            index_one_item(client, index, provider, item, &mut report).await?
        };
        match outcome {
            IndexOutcome::Indexed => {
                report.items_indexed =
                    report.items_indexed.saturating_add(1)
            }
            IndexOutcome::SkippedUnchanged => {
                report.items_skipped_unchanged =
                    report.items_skipped_unchanged.saturating_add(1)
            }
            IndexOutcome::SkippedEmpty => {
                report.items_skipped_empty =
                    report.items_skipped_empty.saturating_add(1)
            }
        }
    }
```

- [ ] **Step 4: Update the doc comment on `index_one_item`**

The function currently documents its `bool` contract. Replace it with:

```rust
/// Assembles, chunks, embeds, and stores one item's text, returning the
/// [`IndexOutcome`] so the caller bumps exactly one counter.
```

- [ ] **Step 5: Run the outcome coverage tests**

Run: `cargo nextest run -E 'test(skips_unchanged_items_on_second_run)' && cargo nextest run -E 'test(skips_items_with_no_indexable_text)' && cargo nextest run -E 'test(indexes_new_items_with_title_and_abstract)'`
Expected: PASS. These three existing tests cover `SkippedUnchanged`, `SkippedEmpty`, and `Indexed` respectively.

- [ ] **Step 6: Run the full gate**

Run: `mise run fmt && mise run verify`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/semantic_search/index.rs
git commit -m "refactor(semantic-search): model per-item index outcome as an enum"
```

---

### Task 5: Store API takes `&ItemKey` instead of `&str`

**Finding:** every other module passes the typed `ItemKey`; `semantic_search/store.rs` re-stringifies. This task converts the store's read/write API to `&ItemKey` and fixes all callers. `StoredChunk.item_key` stays `String` until Task 6.

**Files:**
- Modify: `src/semantic_search/store.rs:116,139,192,206-213`
- Modify: `src/semantic_search/index.rs:74,76,100-107,151`
- Modify: `src/mcp/semantic_search.rs:234,261` (test call sites + import)

- [ ] **Step 1: Add the `ItemKey` import to `store.rs`**

In `src/semantic_search/store.rs`, change the `crate::` import block:

```rust
use crate::{
    errors::ZoteroMcpError,
    semantic_search::embedding::{decode_embedding, encode_embedding},
};
```

to:

```rust
use crate::{
    errors::ZoteroMcpError,
    semantic_search::embedding::{decode_embedding, encode_embedding},
    zotero::ItemKey,
};
```

- [ ] **Step 2: Change the four store method signatures**

In `src/semantic_search/store.rs`, make these signature + bind changes:

`stored_date_modified`:

```rust
    pub(crate) async fn stored_date_modified(
        &self,
        item_key: &ItemKey,
    ) -> Result<Option<String>, ZoteroMcpError> {
        let row =
            sqlx::query("SELECT date_modified FROM items WHERE item_key = ?")
                .bind(item_key.as_str())
```

`upsert_item` (param only; the body's `.bind(item_key)` must become `.bind(item_key.as_str())`):

```rust
    pub(crate) async fn upsert_item(
        &self,
        item_key: &ItemKey,
        title: Option<&str>,
        date_modified: Option<&str>,
        chunks: &[NewChunk],
    ) -> Result<(), ZoteroMcpError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO items (item_key, title, date_modified, indexed_at)
             VALUES (?, ?, ?, strftime('%s','now'))
             ON CONFLICT(item_key) DO UPDATE SET
                title = excluded.title,
                date_modified = excluded.date_modified,
                indexed_at = excluded.indexed_at",
        )
        .bind(item_key.as_str())
```

`delete_item`:

```rust
    pub(crate) async fn delete_item(
        &self,
        item_key: &ItemKey,
    ) -> Result<(), ZoteroMcpError> {
        sqlx::query("DELETE FROM items WHERE item_key = ?")
            .bind(item_key.as_str())
```

`all_item_keys` (return type changes to `Vec<ItemKey>`):

```rust
    pub(crate) async fn all_item_keys(
        &self,
    ) -> Result<Vec<ItemKey>, ZoteroMcpError> {
        let rows = sqlx::query("SELECT item_key FROM items")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|r| Ok(ItemKey::from(r.try_get::<String, _>("item_key")?)))
            .collect()
    }
```

- [ ] **Step 3: Update `index.rs` call sites**

In `src/semantic_search/index.rs`:

1. In `is_unchanged` (currently line 104), change:

```rust
    let stored = index.stored_date_modified(item.key.as_str()).await?;
```

to:

```rust
    let stored = index.stored_date_modified(&item.key).await?;
```

2. In `index_one_item`, change the `upsert_item` call:

```rust
    index
        .upsert_item(
            item.key.as_str(),
```

to:

```rust
    index
        .upsert_item(
            &item.key,
```

3. In `index_library`, change the `current_keys` declaration and insert (lines 67, 74):

```rust
    let mut current_keys = std::collections::HashSet::new();
```
stays, but the insert:

```rust
        current_keys.insert(item.key.to_string());
```

becomes:

```rust
        current_keys.insert(item.key.clone());
```

(`current_keys` is now inferred as `HashSet<ItemKey>`; the stale-key loop already passes `&stale_key`, which matches `delete_item(&ItemKey)`.)

- [ ] **Step 4: Update `store.rs` tests**

In `src/semantic_search/store.rs`, in `mod tests`:

- `upsert_item("ITEM1", ...)` → `upsert_item(&ItemKey::from("ITEM1"), ...)` (5 occurrences: lines 290, 317, 345, 349, 370)
- `index.delete_item("ITEM1")` → `index.delete_item(&ItemKey::from("ITEM1"))` (lines 353, 380)
- `index.stored_date_modified("ITEM1")` → `index.stored_date_modified(&ItemKey::from("ITEM1"))` (line 333)
- `index.all_item_keys().await.unwrap()` assertions become:

```rust
        assert_eq!(index.all_item_keys().await.unwrap(), vec![
            ItemKey::from("ITEM2")
        ]);
```

- [ ] **Step 5: Update `mcp/semantic_search.rs` tests**

In `src/mcp/semantic_search.rs`, in `mod tests`:

1. Update the import block:

```rust
    use crate::{
        semantic_search::{EmbeddingProvider, SemanticIndex},
        state::AppState,
        zotero::test_http::{MockServer, http_response},
    };
```

to:

```rust
    use crate::{
        semantic_search::{EmbeddingProvider, SemanticIndex},
        state::AppState,
        zotero::{
            ItemKey, test_http::{MockServer, http_response},
        },
    };
```

2. Change both direct `upsert_item` calls (lines 234, 261) from `"ITEM1"` to `&ItemKey::from("ITEM1")`.

- [ ] **Step 6: Run tests**

Run: `cargo nextest run -E 'test(upsert_then_load_round_trips_chunks)' && cargo nextest run -E 'test(status_action_reports_indexed_counts)' && cargo nextest run -E 'test(search_action_returns_matching_hit)'`
Expected: PASS.

- [ ] **Step 7: Run the full gate**

Run: `mise run fmt && mise run verify`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/semantic_search/store.rs src/semantic_search/index.rs src/mcp/semantic_search.rs
git commit -m "refactor(semantic-search): store API takes &ItemKey"
```

---

### Task 6: `StoredChunk` and `SemanticSearchHit` carry `ItemKey`

**Finding (cont.):** with the store API typed, the in-memory records still stringify the key: `StoredChunk.item_key: String` forces `search.rs` to clone into `SemanticSearchHit.item_key: String` and key its `HashMap` by `&str`. Convert both to `ItemKey`.

**Files:**
- Modify: `src/semantic_search/store.rs:17-24,233-240`
- Modify: `src/semantic_search/search.rs:8-14,19-25,58-73`
- Modify: `src/semantic_search/store.rs` tests (assertions only)

- [ ] **Step 1: Change `StoredChunk` and `load_all_chunks` in `store.rs`**

In `src/semantic_search/store.rs`:

1. Change the struct field:

```rust
/// One stored chunk, decoded, ready for a cosine scan.
#[derive(Clone, Debug)]
pub(crate) struct StoredChunk {
    pub(crate) item_key: ItemKey,
```

2. In `load_all_chunks`, change the row mapping:

```rust
            chunks.push(StoredChunk {
                item_key: row.try_get("item_key")?,
```

to:

```rust
            chunks.push(StoredChunk {
                item_key: ItemKey::from(row.try_get::<String, _>("item_key")?),
```

- [ ] **Step 2: Update `store.rs` test assertions**

`assert_eq!(first.item_key, "ITEM1")` and `assert_eq!(remaining.first().unwrap().item_key, "ITEM2")` still compile and pass unchanged — `ItemKey` implements `PartialEq<&str>`. No edits needed in the test bodies.

- [ ] **Step 3: Add the `ItemKey` import to `search.rs`**

In `src/semantic_search/search.rs`, change the import block:

```rust
use crate::{
    errors::ZoteroMcpError,
    semantic_search::{
        EmbeddingProvider,
        embedding::{cosine_similarity, normalize},
        store::StoredChunk,
    },
};
```

to:

```rust
use crate::{
    errors::ZoteroMcpError,
    semantic_search::{
        EmbeddingProvider,
        embedding::{cosine_similarity, normalize},
        store::StoredChunk,
    },
    zotero::ItemKey,
};
```

- [ ] **Step 4: Change `SemanticSearchHit` and the aggregation**

In `src/semantic_search/search.rs`:

1. Change the struct:

```rust
/// One semantic search result: the best-matching chunk for its item.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct SemanticSearchHit {
    pub(crate) item_key: ItemKey,
```

2. Change the aggregation (lines 58-74):

```rust
    let mut best_per_item: HashMap<&ItemKey, SemanticSearchHit> =
        HashMap::new();
    for chunk in all_chunks {
        let score = cosine_similarity(&query_vector, &chunk.embedding);
        if score < min_similarity {
            continue;
        }
        let entry = best_per_item.get(&chunk.item_key);
        if entry.is_none_or(|existing| score > existing.similarity) {
            best_per_item.insert(&chunk.item_key, SemanticSearchHit {
                item_key: chunk.item_key.clone(),
                title: chunk.title.clone(),
                similarity: score,
                chunk_index: chunk.chunk_index,
                chunk_text: chunk.chunk_text.clone(),
            });
        }
    }
```

- [ ] **Step 5: Update `search.rs` test helper**

In `src/semantic_search/search.rs`, in `mod tests`, change the `stored` helper:

```rust
    fn stored(
        item_key: &str,
        chunk_index: i64,
        embedding: Vec<f32>,
    ) -> StoredChunk {
        StoredChunk {
            item_key: ItemKey::from(item_key),
            title: Some(format!("Title {item_key}")),
            chunk_index,
            chunk_text: format!("chunk {chunk_index} of {item_key}"),
            embedding,
        }
    }
```

Test assertions like `assert_eq!(hit.item_key, "ITEM1")` keep working via `PartialEq<&str>`.

- [ ] **Step 6: Run tests**

Run: `cargo nextest run -E 'test(returns_best_chunk_per_item_above_min_similarity)' && cargo nextest run -E 'test(upsert_then_load_round_trips_chunks)'`
Expected: PASS.

- [ ] **Step 7: Run the full gate**

Run: `mise run fmt && mise run verify`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/semantic_search/store.rs src/semantic_search/search.rs
git commit -m "refactor(semantic-search): carry ItemKey in chunk and hit records"
```

---

### Task 7: Introduce the `Embedding` newtype for vectors

**Finding:** embeddings flow as bare `Vec<f32>` through `normalize`/`encode_embedding`/`decode_embedding`/`cosine_similarity`, with no dimensionality invariants. Add an `Embedding(Vec<f32>)` newtype that owns those operations, and make `EmbeddingProvider::embed` return `Vec<Embedding>` so the trait boundary enforces the type.

**Files:**
- Modify: `src/semantic_search/embedding.rs` (replace free functions with the type)
- Modify: `src/semantic_search/mod.rs:15,41-51` (re-export `Embedding`, retype trait)
- Modify: `src/semantic_search/store.rs:12-14,26-31,168-179,233-240`
- Modify: `src/semantic_search/search.rs:35-80`
- Modify: `src/semantic_search/index.rs:129-146`
- Modify: `src/mcp/semantic_search.rs` (tests: `FixedProvider`, `new_chunk`)

- [ ] **Step 1: Write the failing tests**

Replace the entire `#[cfg(test)] mod tests` in `src/semantic_search/embedding.rs` with:

```rust
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn encode_decode_round_trips_including_negative_and_zero() {
        let original =
            Embedding::from(vec![0.0, -1.5, 3.25, -0.000_1, 42.0]);
        let encoded = original.encode();
        let decoded = Embedding::try_from(encoded.as_slice()).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn decode_rejects_non_multiple_of_four_length() {
        let bytes = vec![0_u8, 1, 2];
        assert!(Embedding::try_from(bytes.as_slice()).is_err());
    }

    #[test]
    fn normalize_leaves_zero_vector_unchanged() {
        let mut vector = Embedding::from(vec![0.0_f32, 0.0, 0.0]);
        vector.normalize();
        assert_eq!(vector, Embedding::from(vec![0.0, 0.0, 0.0]));
    }

    #[test]
    fn normalized_self_similarity_is_approximately_one() {
        let mut vector = Embedding::from(vec![1.0_f32, 2.0, 3.0, -4.0]);
        vector.normalize();
        let similarity = vector.dot(&vector);
        assert!(
            (similarity - 1.0).abs() < 1e-6,
            "expected ~1.0, got {similarity}"
        );
    }

    #[test]
    fn dot_mismatched_lengths_returns_zero() {
        let a = Embedding::from(vec![1.0_f32, 2.0]);
        let b = Embedding::from(vec![1.0_f32, 2.0, 3.0]);
        assert!(a.dot(&b).abs() < f32::EPSILON);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(encode_decode_round_trips_including_negative_and_zero)'`
Expected: FAIL — compile error, `Embedding` is not defined.

- [ ] **Step 3: Define the type and delete the free functions**

In `src/semantic_search/embedding.rs`, replace everything from the `/// L2-normalizes ...` doc comment (line 69) through the end of `decode_embedding` (line 126) with:

```rust
/// A dense embedding vector produced by the model and stored in the index.
///
/// Newtype over `Vec<f32>` so dimensionality and normalization are handled
/// at typed boundaries (BLOB decode, dot products) rather than as free
/// `Vec<f32>` bookkeeping.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Embedding(Vec<f32>);

impl Embedding {
    /// L2-normalizes in place. A zero vector is left unchanged.
    pub(crate) fn normalize(&mut self) {
        let norm_sq: f32 = self.0.iter().map(|x| x * x).sum();
        if norm_sq <= 0.0 {
            return;
        }
        let norm = norm_sq.sqrt();
        for x in self.0.iter_mut() {
            *x /= norm;
        }
    }

    /// Dot product of two equal-length, pre-normalized vectors — equal to
    /// their cosine similarity. Returns `0.0` if lengths differ (defensive:
    /// should never happen since only one model/dimensionality is stored).
    pub(crate) fn dot(&self, other: &Embedding) -> f32 {
        if self.0.len() != other.0.len() {
            return 0.0;
        }
        self.0.iter().zip(&other.0).map(|(x, y)| x * y).sum()
    }

    /// Encodes the vector as little-endian `f32` bytes for `BLOB` storage.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.0.len().saturating_mul(4));
        for value in &self.0 {
            buf.extend_from_slice(&value.to_le_bytes());
        }
        buf
    }
}

impl From<Vec<f32>> for Embedding {
    fn from(values: Vec<f32>) -> Self {
        Self(values)
    }
}

impl TryFrom<&[u8]> for Embedding {
    type Error = ZoteroMcpError;

    /// Decodes little-endian `f32` bytes back into an embedding.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Embedding`] if `bytes.len()` is not a multiple of 4
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut chunks = bytes.chunks_exact(4);
        let mut values = Vec::with_capacity(chunks.len());
        for chunk in &mut chunks {
            let array: [u8; 4] = chunk.try_into().map_err(|_| {
                ZoteroMcpError::Embedding(
                    "corrupt embedding blob: chunk is not 4 bytes".to_owned(),
                )
            })?;
            values.push(f32::from_le_bytes(array));
        }
        if !chunks.remainder().is_empty() {
            return Err(ZoteroMcpError::Embedding(
                "corrupt embedding blob: length is not a multiple of 4"
                    .to_owned(),
            ));
        }
        Ok(Self(values))
    }
}
```

- [ ] **Step 4: Update `mod.rs` — re-export `Embedding` and retype the trait**

In `src/semantic_search/mod.rs`:

1. Change the re-export (line 15):

```rust
pub(crate) use embedding::{Embedding, FastEmbedProvider};
```

2. Retype the trait method (line 50):

```rust
    /// Embeds a batch of texts, returning one vector per input in the same
    /// order. Vectors are NOT required to be normalized; callers normalize.
    ///
    /// # Errors
    ///
    /// Returns [`ZoteroMcpError::Embedding`] if inference fails.
    fn embed(&self, texts: &[String]) -> Result<Vec<Embedding>, ZoteroMcpError>;
```

(`Embedding` is now in scope in `mod.rs` via the re-export.)

- [ ] **Step 5: Update `store.rs` to use `Embedding`**

In `src/semantic_search/store.rs`:

1. Change the import:

```rust
use crate::{
    errors::ZoteroMcpError,
    semantic_search::embedding::{decode_embedding, encode_embedding},
    zotero::ItemKey,
};
```

to:

```rust
use crate::{errors::ZoteroMcpError, semantic_search::Embedding, zotero::ItemKey};
```

2. Change the struct fields:

```rust
/// A chunk to insert, with its already-normalized embedding.
pub(crate) struct NewChunk {
    pub(crate) chunk_index: i64,
    pub(crate) chunk_text: String,
    pub(crate) embedding: Embedding,
}
```

and:

```rust
    pub(crate) embedding: Embedding,
```

(the `StoredChunk` field).

3. In `upsert_item`, change the bind:

```rust
            .bind(encode_embedding(&chunk.embedding))
```

to:

```rust
            .bind(chunk.embedding.encode())
```

4. In `load_all_chunks`, change the decode:

```rust
            chunks.push(StoredChunk {
                item_key: ItemKey::from(row.try_get::<String, _>("item_key")?),
                title: row.try_get("title")?,
                chunk_index: row.try_get("chunk_index")?,
                chunk_text: row.try_get("chunk_text")?,
                embedding: Embedding::try_from(&embedding_bytes)?,
            });
```

- [ ] **Step 6: Update `store.rs` tests**

In `src/semantic_search/store.rs`, `mod tests`, change the `chunk` helper:

```rust
    fn chunk(idx: i64, text: &str, value: f32) -> NewChunk {
        NewChunk {
            chunk_index: idx,
            chunk_text: text.to_owned(),
            embedding: Embedding::from(vec![value, value, value]),
        }
    }
```

and the round-trip assertions:

```rust
        assert_eq!(first.embedding, Embedding::from(vec![0.5, 0.5, 0.5]));
        let second = loaded.get(1).unwrap();
        assert_eq!(second.chunk_text, "second chunk");
        assert_eq!(second.embedding, Embedding::from(vec![-0.5, -0.5, -0.5]));
```

(`use super::*;` brings `Embedding` into scope via the parent's import.)

- [ ] **Step 7: Update `search.rs`**

In `src/semantic_search/search.rs`:

1. Remove the now-unused imports (`cosine_similarity`, `normalize`):

```rust
use crate::{
    errors::ZoteroMcpError,
    semantic_search::{EmbeddingProvider, store::StoredChunk},
    zotero::ItemKey,
};
```

2. Rework the query embedding + scoring (lines 42-74):

```rust
    let provider = Arc::clone(provider);
    let query_owned = query.to_owned();
    let mut query_embedding = tokio::task::spawn_blocking(move || {
        provider.embed(&[query_owned]).and_then(|mut v| {
            v.pop().ok_or_else(|| {
                ZoteroMcpError::Embedding(
                    "embedding provider returned no vector for the query"
                        .to_owned(),
                )
            })
        })
    })
    .await
    .map_err(|e| ZoteroMcpError::Embedding(e.to_string()))??;
    query_embedding.normalize();

    let mut best_per_item: HashMap<&ItemKey, SemanticSearchHit> =
        HashMap::new();
    for chunk in all_chunks {
        let score = query_embedding.dot(&chunk.embedding);
        if score < min_similarity {
            continue;
        }
        let entry = best_per_item.get(&chunk.item_key);
        if entry.is_none_or(|existing| score > existing.similarity) {
            best_per_item.insert(&chunk.item_key, SemanticSearchHit {
                item_key: chunk.item_key.clone(),
                title: chunk.title.clone(),
                similarity: score,
                chunk_index: chunk.chunk_index,
                chunk_text: chunk.chunk_text.clone(),
            });
        }
    }
```

3. In `mod tests`, update `FixedProvider` and `stored`:

```rust
    #[derive(Debug)]
    struct FixedProvider {
        vector: Vec<f32>,
    }

    impl EmbeddingProvider for FixedProvider {
        fn embed(
            &self,
            texts: &[String],
        ) -> Result<Vec<Embedding>, ZoteroMcpError> {
            Ok(texts
                .iter()
                .map(|_| Embedding::from(self.vector.clone()))
                .collect())
        }
    }

    fn stored(
        item_key: &str,
        chunk_index: i64,
        embedding: Vec<f32>,
    ) -> StoredChunk {
        StoredChunk {
            item_key: ItemKey::from(item_key),
            title: Some(format!("Title {item_key}")),
            chunk_index,
            chunk_text: format!("chunk {chunk_index} of {item_key}"),
            embedding: Embedding::from(embedding),
        }
    }
```

Add the test import alongside `use super::*;`:

```rust
    use super::*;
    use crate::semantic_search::Embedding;
```

- [ ] **Step 8: Update `index.rs`**

In `src/semantic_search/index.rs`:

1. Change the `semantic_search::` imports — remove `embedding::normalize`:

```rust
    semantic_search::{
        EmbeddingProvider, MAX_CHUNK_CHARS, MAX_INDEXABLE_CHARS,
        chunking::chunk_text,
        store::{NewChunk, SemanticIndex},
    },
```

2. Rework the embed/normalize/collect block (lines 133-146):

```rust
    let mut vectors = provider.embed(&pieces)?;
    for vector in &mut vectors {
        vector.normalize();
    }
    let new_chunks: Vec<NewChunk> = pieces
        .into_iter()
        .zip(vectors)
        .enumerate()
        .map(|(idx, (chunk_text, embedding))| NewChunk {
            chunk_index: i64::try_from(idx).unwrap_or(i64::MAX),
            chunk_text,
            embedding,
        })
        .collect();
```

3. In `mod tests`, update `FakeProvider` and add the import:

```rust
    use super::*;
    use crate::{
        semantic_search::Embedding,
        state::AppState,
        zotero::test_http::{MockServer, http_response},
    };
```

```rust
    impl EmbeddingProvider for FakeProvider {
        fn embed(
            &self,
            texts: &[String],
        ) -> Result<Vec<Embedding>, ZoteroMcpError> {
            Ok(texts
                .iter()
                .map(|_| Embedding::from(vec![1.0, 0.0, 0.0, 0.0]))
                .collect())
        }
    }
```

- [ ] **Step 9: Update `mcp/semantic_search.rs` tests**

In `src/mcp/semantic_search.rs`, `mod tests`:

1. Import `Embedding`:

```rust
    use crate::{
        semantic_search::{Embedding, EmbeddingProvider, SemanticIndex},
        state::AppState,
        zotero::{
            ItemKey, test_http::{MockServer, http_response},
        },
    };
```

2. Update `FixedProvider`:

```rust
    impl EmbeddingProvider for FixedProvider {
        fn embed(
            &self,
            texts: &[String],
        ) -> Result<Vec<Embedding>, crate::errors::ZoteroMcpError> {
            Ok(texts
                .iter()
                .map(|_| Embedding::from(self.vector.clone()))
                .collect())
        }
    }
```

3. Update `new_chunk`:

```rust
    fn new_chunk(
        chunk_index: i64,
        text: &str,
        value: f32,
    ) -> crate::semantic_search::NewChunk {
        crate::semantic_search::NewChunk {
            chunk_index,
            chunk_text: text.to_owned(),
            embedding: Embedding::from(vec![value]),
        }
    }
```

- [ ] **Step 10: Run the full gate**

Run: `mise run fmt && mise run verify`
Expected: PASS. (If a stale `use` import remains, clippy's `-D warnings` will name the exact file/line to delete.)

- [ ] **Step 11: Commit**

```bash
git add src/semantic_search/embedding.rs src/semantic_search/mod.rs src/semantic_search/store.rs src/semantic_search/search.rs src/semantic_search/index.rs src/mcp/semantic_search.rs
git commit -m "refactor(semantic-search): introduce Embedding newtype"
```

---

### Task 8: Log (don't silently swallow) fulltext-fetch failures

**Finding:** `assemble_item_text` drops every children/fulltext fetch error with `unwrap_or_default()` / `if let Ok`, so a flaky Local API silently yields an index missing fulltext, still counted as `items_indexed`.

**Files:**
- Modify: `src/semantic_search/index.rs:191-203`

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src/semantic_search/index.rs`:

```rust
    #[tokio::test]
    async fn indexes_item_when_children_fetch_fails() {
        let items = format!(
            "[{}]",
            item_json(
                "ITEM1",
                "A Paper",
                "An abstract about testing.",
                "2024-01-01"
            )
        );
        let server = MockServer::new(vec![
            http_response("200 OK", &items),
            http_response("500 Internal Server Error", ""),
        ]);
        let state = test_state(server.url().to_owned());
        let client = ZoteroClient::new(&state);
        let dir = tempfile::tempdir().unwrap();
        let index = SemanticIndex::open(&dir.path().join("embeddings.sqlite"))
            .await
            .unwrap();
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(FakeProvider);

        let report =
            index_library(&client, &index, &provider, false).await.unwrap();

        assert_eq!(report.items_indexed, 1);
        assert_eq!(report.items_skipped_empty, 0);
    }
```

This test asserts the *behavior*: a failed children fetch must not abort the scan or count the item as empty — it is still indexed from title + abstract. The `tracing::warn!` emission is a side effect the test cannot observe, and that is fine.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -E 'test(indexes_item_when_children_fetch_fails)'`
Expected: FAIL. Why: `MockServer` has only 2 queued responses and the test expects the scan to complete; the current code already tolerates the 500 via `unwrap_or_default()`, so this test may *pass* before the change. That means the real red→green signal here is the **logging behavior**, which is not observable. Run the test now to confirm it passes (green baseline), then proceed — Step 3's value is the logging, verified by code review and by the suite staying green. If the test unexpectedly fails, stop and investigate before continuing.

- [ ] **Step 3: Add best-effort `tracing::warn!` on swallowed errors**

In `src/semantic_search/index.rs`, replace the children/fulltext block inside `assemble_item_text` (lines 191-203):

```rust
    let children = match client.get_item_children(&item.key).await {
        Ok(children) => children,
        Err(err) => {
            tracing::warn!(
                key = item.key.as_str(),
                error = %err,
                "failed to fetch item children during semantic indexing; \
                 indexing without attachment fulltext"
            );
            Vec::new()
        }
    };
    for child in &children {
        if child.data.item_type != ItemType::Attachment {
            continue;
        }
        match client.get_item_fulltext(&child.key).await {
            Ok(text) if !text.trim().is_empty() => {
                parts.push(text);
                break;
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    key = child.key.as_str(),
                    error = %err,
                    "failed to fetch attachment fulltext during semantic \
                     indexing"
                );
            }
        }
    }
```

Note this re-introduces the `ItemType` reference removed in Task 1. Restore it in the `zotero::` import:

```rust
    zotero::{ItemType, ZoteroClient, ZoteroItem},
```

- [ ] **Step 4: Run the test and the gate**

Run: `cargo nextest run -E 'test(indexes_item_when_children_fetch_fails)' && cargo nextest run -E 'test(skips_items_with_no_indexable_text)'`
Expected: PASS.

Run: `mise run fmt && mise run verify`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/semantic_search/index.rs
git commit -m "fix(semantic-search): warn on fulltext fetch failures instead of silently dropping"
```

---

### Task 9: Open the SQLite index via `filename()`, not a hand-built URL

**Finding:** `store.rs` builds `format!("sqlite://{}", db_path.display())`. A path containing `?`, `#`, or spaces (legal on macOS/Linux) produces a wrong URL. `SqliteConnectOptions::filename()` takes the path directly.

**Files:**
- Modify: `src/semantic_search/store.rs:4,60-67`

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src/semantic_search/store.rs`:

```rust
    #[tokio::test]
    async fn open_handles_paths_with_spaces_and_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("my index #1/embeddings.sqlite");
        let index = SemanticIndex::open(&db_path).await.unwrap();
        index
            .upsert_item(&ItemKey::from("ITEM1"), None, None, &[chunk(
                0, "a", 1.0,
            )])
            .await
            .unwrap();
        assert_eq!(index.stats().await.unwrap().indexed_items, 1);
        assert!(db_path.exists(), "db must be created at the exact path");
    }
```

The final `assert!(db_path.exists(), ...)` is the point of the test: the URL form mis-parses `#` as a URL fragment (and spaces are not valid URL path chars), so the file is opened at a *wrong* path and this assertion fails. `filename()` opens exactly `db_path`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -E 'test(open_handles_paths_with_spaces_and_special_chars)'`
Expected: FAIL — `SemanticIndex::open(...).unwrap()` panics (sqlx's URL parser rejects or mangles the path) **or** the `db_path.exists()` assertion panics because the db landed at a truncated path. Either way the test must go red.

- [ ] **Step 3: Switch to `SqliteConnectOptions::filename`**

In `src/semantic_search/store.rs`:

1. Remove the now-unused `str::FromStr` import — change:

```rust
use std::{path::Path, str::FromStr, time::Duration};
```

to:

```rust
use std::{path::Path, time::Duration};
```

2. Replace the options construction in `open` (lines 60-67):

```rust
        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -E 'test(open_handles_paths_with_spaces_and_special_chars)'`
Expected: PASS.

- [ ] **Step 5: Run the full gate**

Run: `mise run fmt && mise run verify`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/semantic_search/store.rs
git commit -m "fix(semantic-search): open index db via SqliteConnectOptions::filename"
```

---

## Self-Review

**Spec coverage — all eight review findings map to a task:**
1. `ItemKey` newtype through the store/chunk/hit layer → Task 5 (API) + Task 6 (records)
2. Per-item outcome enum → Task 4
3. `IndexReport` `Default` → Task 3
4. Reuse `get_all_items` → Task 2
5. `ItemType::is_indexable` dedupe → Task 1
6. Fulltext-fetch failures logged → Task 8
7. `Embedding` newtype → Task 7
8. SQLite path robustness → Task 9

**Deferred intentionally (documented, not forgotten):**
- `chunk_index: i64` and the `i64::try_from(idx).unwrap_or(i64::MAX)` guard (`index.rs`) — kept as-is; a `ChunkIndex` newtype was judged YAGNI, and the guard is harmless on all supported targets.
- `mod.rs` `default_semantic_data_dir` vs `zotero::sqlite::profiles_dirs` — target directories differ (`zotero-mcp-rs` vs Zotero's own data dir); unifying would need parameterization that isn't worth it.

**Placeholder scan:** no TBD/TODO placeholders; every code step carries complete, compileable code.

**Type consistency:** `ItemKey` (via `as_str`, `From<String>`, `From<&str>`, `PartialEq<&str>`) and `Embedding` (via `From<Vec<f32>>`, `TryFrom<&[u8]>`, `normalize`, `dot`, `encode`) are used with matching signatures across Tasks 5-7; Task 8 correctly re-imports `ItemType` that Task 1 removed. Test helpers in `search.rs`, `index.rs`, and `mcp/semantic_search.rs` all construct `Embedding`/`ItemKey` identically.
