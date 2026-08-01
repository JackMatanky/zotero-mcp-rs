# Close the Search / Pagination Gap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the two documented gaps in `docs/zotero-mcp-comparison.md` (rows 15 & 30): make server-side advanced search, duplicate detection, and library-coverage scan the **whole library** instead of the first 100 items, add richer search operators/join/sort, return paginated `{items, pagination}` results with a `start` offset, and add two optional sqlite-backed full-text tools (54 → 56 tools).

**Architecture:** A pagination helper `get_all_json` on `ZoteroClient` drives all whole-library scans through repeated paged requests to the Zotero Local HTTP API (short-page stop). `advanced_search` gains an optional server-side pushdown fast path (`q`/`itemType`/`tag` params) and otherwise filters the fully-paged library in Rust. Search tool responses become `{items, pagination:{limit,offset,total,hasMore}}`. A new `src/zotero/local_db.rs` module opens Zotero's `zotero.sqlite` read-only/immutable (via sqlx) to answer full-text and note/annotation queries, gated behind `ZOTERO_SQLITE_ACCESS=1` (default off).

**Tech Stack:** Rust, tokio, reqwest (existing), sqlx 0.8 (new: `runtime-tokio` + `sqlite` features only, manual row mapping — no macros), rmcp 0.8 (tool registration), serde/schemars (existing). Tests reuse the in-file mock HTTP server fixtures (`http_response`, `http_response_with_headers`, `mock_server`, `zotero_state` in `src/mcp/zotero.rs`; `test_state`/`mock_server` in `src/zotero/client.rs` and `src/state.rs`) and sqlx in-memory fixtures for the sqlite layer.

---

## Context: current call sites you must update (verified)

These are every place that calls the functions whose signatures change. Read them before starting:

| Symbol | Defined | Call sites that change |
| ------ | ------- | ---------------------- |
| `search_items` | `src/zotero/search.rs:80` | `search_by_citation_key` (`search.rs:137`), `zotero_search_items_impl` (`src/mcp/zotero.rs:442`) |
| `advanced_search` | `src/zotero/search.rs:167` | `zotero_advanced_search_impl` (`src/mcp/zotero.rs:970`), `zotero_add_by_identifier_impl` (`src/mcp/zotero.rs:1076`) |
| `get_recent_items(100)` scan | `advanced_search` (`search.rs:172`) | → replaced by `get_all_items()` |
| whole-library `get_json` scan | `find_duplicates` (`src/zotero/analytics.rs:83-88`) | → replaced by `get_all_items()` |
| `get_recent_items(100)` scan | `get_library_coverage` (`src/zotero/analytics.rs:108`) | → replaced by `get_all_items()` |
| `SearchItemsArgs` | `src/mcp/zotero.rs:66` | `connector_search_impl` (`src/mcp/connector_tools.rs:50-54`) builds it inline |
| `AdvancedSearchArgs` | `src/mcp/zotero.rs:314` | `zotero_advanced_search_impl` (`src/mcp/zotero.rs:963`) |

**Repo conventions (mandatory):**
- No `src/lib.rs` — binary crate; unit tests live in per-module `#[cfg(test)] mod tests` blocks. No `tests/` dir. Do not create one.
- Strict clippy (via `mise run clippy`): `indexing_slicing`, `expect_used`, `arithmetic_side_effects`, `missing_errors_doc`, `missing_panics_doc` are deny-by-default. No `unwrap()`/`expect()`/`panic!` in non-test code. Prefer `saturating_*` for arithmetic.
- Every public-ish method needs `# Errors` doc section (`missing_errors_doc`).
- Run checks with `mise run check` (cargo check), `mise run test` (cargo nextest run), `mise run clippy` (clippy `-D warnings`), `mise run lint` (hk), `mise run fmt`.
- Commit message style: conventional (`feat:`, `fix:`, `refactor:`, `test:`), lower-case body.

---

## File Structure

- Modify: `Cargo.toml` — add `sqlx` dependency.
- Modify: `src/errors.rs` — add `ZoteroMcpError::LocalDb(String)` variant.
- Modify: `src/state.rs` — add `AppState::sqlite_access: bool` + `check_sqlite_access()`.
- Modify: `src/zotero/client.rs` — add `get_all_json` + `get_items_with_total` + `add_pagination`.
- Modify: `src/zotero/items.rs` — add `get_all_items`.
- Modify: `src/zotero/search.rs` — expand `SearchOperator`, add `JoinMode`/`SortField`/`SortDirection`/`PaginationInfo`/`SearchPage`, rework `advanced_search` (pushdown + full-scan), change `search_items` signature.
- Modify: `src/zotero/analytics.rs` — route `find_duplicates` + `get_library_coverage` through `get_all_items`.
- Create: `src/zotero/local_db.rs` — sqlite reader (`LocalZoteroDb`, discovery, two search queries + result types).
- Modify: `src/zotero/mod.rs` — `mod local_db;` + re-exports.
- Modify: `src/mcp/zotero.rs` — `SearchItemsArgs`/`AdvancedSearchArgs` gain `start` + sort/join fields; `zotero_search_items_impl`, `zotero_advanced_search_impl`, `zotero_add_by_identifier_impl` updated; two new tool impls + arg structs + `#[cfg(test)]` fixtures for sqlite tools.
- Modify: `src/mcp/connector_tools.rs` — `connector_search_impl` passes new `SearchItemsArgs` fields.
- Modify: `src/mcp/server.rs` — register 2 new tools after `zotero_advanced_search`.
- Modify: `src/main.rs` — if `local_db` is not reachable via `zotero` module (verify; `mod` chain already covers it).
- Modify: `docs/zotero-mcp-comparison.md` — rows 15/30, tool count 54 → 56, gap analysis.
- Modify: `README.md` (if it documents `# tools` or search behavior — check first).

---

## Task 1: Add `get_all_json` / `get_items_with_total` pagination helpers to the HTTP client

**Files:**
- Modify: `src/zotero/client.rs` (near `get_json`, after line 117)
- Test: `src/zotero/client.rs` `#[cfg(test)] mod get_all_json` (new test module)

- [ ] **Step 1: Write the failing test**

Append a new test module to `src/zotero/client.rs` (inside the existing `mod tests`). The fixture server must be given the **3 responses** the paginated loop will request: page 1 (2 items), page 2 (1 item → short page stops the loop).

```rust
    mod get_all_json {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{http_response, mock_server, test_state},
            *,
        };

        #[tokio::test]
        async fn fetches_every_page_until_a_short_page() {
            let base = mock_server(vec![
                http_response("200 OK", r#"[{"key":"A"},{"key":"B"}]"#),
                http_response("200 OK", r#"[{"key":"C"}]"#),
            ]);
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url);
            let items: Vec<serde_json::Value> =
                ZoteroClient::new(&state).get_all_json(&url, 2).await.unwrap();

            assert_eq!(items.len(), 3);
            let keys: Vec<&str> = items
                .iter()
                .map(|i| i["key"].as_str().unwrap_or_default())
                .collect();
            assert_eq!(keys, vec!["A", "B", "C"]);
        }

        #[tokio::test]
        async fn single_page_when_first_page_is_short() {
            let base = mock_server(vec![http_response("200 OK", r#"[{"key":"A"}]"#)]);
            let state = test_state(base, false);

            let url = format!("{}/users/0/items", state.zotero_api_url);
            let items: Vec<serde_json::Value> =
                ZoteroClient::new(&state).get_all_json(&url, 2).await.unwrap();

            assert_eq!(items.len(), 1);
        }
    }
```

Note: `test_state` currently builds `AppState { ... }` with a full struct literal. Task 6 adds a new `sqlite_access` field to `AppState`; the literal here must gain `sqlite_access: false,` then. Do it in Task 6, not now (Task 6 tells you every literal to touch).

- [ ] **Step 2: Run test to verify it fails**

Run: `mise run test src::client::tests::get_all_json`
Expected: FAIL — no method `get_all_json`.

- [ ] **Step 3: Write minimal implementation**

Add to `impl ZoteroClient<'_>` in `src/zotero/client.rs`, right after `get_json` (after line 117):

```rust
    /// Fetches every page of a paginated list endpoint, stopping when a page
    /// returns fewer than `page_size` items (Zotero respects `start`/`limit`).
    ///
    /// The `url` is used as-is on the first request; `start`/`limit` query
    /// parameters are appended for each subsequent page.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if Zotero responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if a response body cannot be decoded
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(super) async fn get_all_json<T: DeserializeOwned>(
        &self,
        url: &str,
        page_size: usize,
    ) -> Result<Vec<T>, ZoteroMcpError> {
        let mut all = Vec::new();
        let mut start = 0_usize;
        loop {
            let page_url = add_pagination(url, start, page_size);
            let page: Vec<T> = self.get_json(&page_url).await?;
            let len = page.len();
            all.extend(page);
            if len < page_size {
                break;
            }
            start = start.saturating_add(page_size);
        }
        Ok(all)
    }

    /// Fetches one page of a paginated list endpoint, also returning the
    /// `Total-Results` response header (the full result count) when present.
    ///
    /// Used by server-side search so pagination can report the true total
    /// without scanning every page.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`] if Zotero responds with a non-2xx status
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if the response body cannot be decoded
    ///
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(super) async fn get_items_with_total(
        &self,
        url: &str,
    ) -> Result<(Vec<crate::zotero::models::ZoteroItem>, usize), ZoteroMcpError>
    {
        let resp =
            self.state.send_with_retry(self.state.client.get(url)).await?;
        let resp = self.ensure_success(resp).await?;
        let total = resp
            .headers()
            .get("Total-Results")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let items = resp.json().await?;
        Ok((items, total))
    }
```

And a free function at module bottom (before `mod tests`):

```rust
/// Appends `start`/`limit` query parameters to `url`, preserving any existing
/// query string.
fn add_pagination(url: &str, start: usize, limit: usize) -> String {
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}start={start}&limit={limit}")
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `mise run test src::client::tests::get_all_json`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/zotero/client.rs
git commit -m "feat: add paginated get_all_json client helper"
```

---

## Task 2: Route all three whole-library scans through `get_all_json`

**Files:**
- Modify: `src/zotero/items.rs` (add `get_all_items` near `get_recent_items`)
- Modify: `src/zotero/search.rs:172`
- Modify: `src/zotero/analytics.rs:83-88` and `:108`
- Test: `src/zotero/analytics.rs` new `#[cfg(test)] mod scan_pagination`

- [ ] **Step 1: Write the failing test**

Add `get_all_items` (implementation below) *first*, then a test that proves `find_duplicates` and `get_library_coverage` no longer cap at 100 items. Append to `src/zotero/analytics.rs` test module. The `#[cfg(test)]` fixtures below are private to this file (each test module owns its own copies — that's the repo pattern), so define them inline:

```rust
    mod scan_pagination {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::zotero::client::ZoteroClient;

        fn item_json(key: &str, title: &str) -> String {
            format!(
                r#"{{"key":"{key}","version":1,"data":{{"key":"{key}","version":1,"itemType":"journalArticle","title":"{title}"}}}}"#
            )
        }

        fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        fn mock_server(responses: Vec<String>) -> String {
            use std::io::{Read, Write};
            use std::net::TcpListener;
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) = listener.accept().expect("accept");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }

        fn test_state(zotero_api_url: String) -> crate::state::AppState {
            crate::state::AppState {
                zotero_api_url,
                better_bibtex_url: String::new(),
                better_notes_url: String::new(),
                crossref_url: String::new(),
                semantic_scholar_url: String::new(),
                open_library_url: String::new(),
                write_enabled: false,
                ..crate::state::AppState::from_env()
            }
        }

        #[tokio::test]
        async fn find_duplicates_scans_more_than_one_hundred_items() {
            // 120 unique items => 2 pages (100 + 20). Two items share a title
            // ("Shared Title") so exactly one duplicate group must be found.
            let mut bodies = Vec::new();
            for i in 0..100 {
                bodies.push(item_json(&format!("K{i:04}"), &format!("Title {i}")));
            }
            bodies.push(item_json("K9900", "Shared Title"));
            bodies.push(item_json("K9901", "Shared Title"));
            for i in 102..120 {
                bodies.push(item_json(&format!("K{i:04}"), &format!("Title {i}")));
            }
            // Use take/skip (not `bodies[a..b]` slicing) to avoid the
            // `indexing_slicing` deny lint.
            let page1 = format!(
                "[{}]",
                bodies
                    .iter()
                    .take(100)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let page2 = format!(
                "[{}]",
                bodies
                    .iter()
                    .skip(100)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            );

            let base = mock_server(vec![
                http_response("200 OK", &page1),
                http_response("200 OK", &page2),
            ]);
            let state = test_state(base);

            let groups = ZoteroClient::new(&state)
                .find_duplicates(None)
                .await
                .unwrap();

            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].match_type, DuplicateType::Title);
            assert_eq!(groups[0].item_keys.len(), 2);
        }
    }
```

Note: the fixture helpers above are private to this test module (each test module owns its own copies — that's the repo pattern; they are not importable from `crate::mcp::zotero::tests`).

- [ ] **Step 2: Run test to verify it fails**

Run: `mise run test zotero::analytics::tests::scan_pagination`
Expected: FAIL — the current single `?limit=100` request means the 20 extra items are never seen, and the mock server only has 2 queued responses while only 1 is consumed, so `find_duplicates` returns no groups (or the second response is never read).

- [ ] **Step 3: Write minimal implementation**

In `src/zotero/items.rs`, add (model the doc style on `get_recent_items`):

```rust
    /// Fetches every top-level library item (notes excluded), paginating
    /// through the whole library with a stable date-modified ordering so
    /// page boundaries are deterministic.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if a response cannot be decoded
    pub(super) async fn get_all_items(
        &self,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items?itemType=-note&sort=dateModified&direction=desc",
            self.state.zotero_api_url
        );
        self.get_all_json(&url, 100).await
    }
```

In `src/zotero/search.rs`, replace line 172:

```rust
        let items = self.get_all_items().await?;
```

In `src/zotero/analytics.rs`, replace the `else` branch of `find_duplicates` (lines 82-88):

```rust
        let items = if let Some(col) = collection_key {
            self.get_collection_items(col).await?
        } else {
            self.get_all_items().await?
        };
```

And in `get_library_coverage` replace line 108:

```rust
            None => self.get_all_items().await?,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `mise run test zotero::analytics::tests::scan_pagination`
Expected: PASS (and the whole suite stays green).

- [ ] **Step 5: Commit**

```bash
git add src/zotero/items.rs src/zotero/search.rs src/zotero/analytics.rs
git commit -m "feat: scan whole library for search, duplicates, coverage"
```

---

## Task 3: Expand search operators, join mode, and sort

**Files:**
- Modify: `src/zotero/search.rs`
- Test: `src/zotero/search.rs` `#[cfg(test)] mod match_condition` (extend) + new `mod sort_items`

- [ ] **Step 1: Write the failing test**

Extend the `match_condition` test module in `src/zotero/search.rs`:

```rust
        #[test]
        fn matches_is_not_operator() {
            let item = make_item(Some("Learning Go"), None, None);
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::IsNot,
                value: "rust".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_does_not_contain_operator() {
            let item = make_item(Some("Learning Go"), None, None);
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::DoesNotContain,
                value: "rust".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_is_greater_than_on_year() {
            let item = make_item(None, Some("2024"), None);
            let cond = SearchCondition {
                field: SearchField::Year,
                operator: SearchOperator::IsGreaterThan,
                value: "2020".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_is_less_than_on_date() {
            let item = make_item(None, Some("2024-02-15"), None);
            let cond = SearchCondition {
                field: SearchField::Date,
                operator: SearchOperator::IsLessThan,
                value: "2025-01-01".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }

        #[test]
        fn matches_is_after_on_date() {
            let item = make_item(None, Some("2024-02-15"), None);
            let cond = SearchCondition {
                field: SearchField::Date,
                operator: SearchOperator::IsAfter,
                value: "2024-01-01".to_owned(),
            };
            assert!(match_condition(&item, &cond));
        }
```

And a new `mod sort_items` test:

```rust
    mod sort_items {
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::zotero::models::{ItemKey, ItemType, ZoteroItemData};

        fn item(key: &str, title: &str, date: &str) -> ZoteroItem {
            ZoteroItem {
                key: ItemKey::from(key),
                version: 1,
                library: serde_json::Value::Null,
                links: serde_json::Value::Null,
                meta: serde_json::Value::Null,
                data: ZoteroItemData {
                    key: ItemKey::from(key),
                    version: 1,
                    item_type: ItemType::JournalArticle,
                    title: Some(title.to_owned()),
                    date: Some(date.to_owned()),
                    ..Default::default()
                },
            }
        }

        #[test]
        fn sorts_by_title_ascending() {
            let items = vec![
                item("K3", "Zeta", "2024"),
                item("K1", "Alpha", "2024"),
                item("K2", "Beta", "2024"),
            ];
            let sorted = sort_items(items, SortField::Title, SortDirection::Asc);
            let titles: Vec<&str> = sorted
                .iter()
                .map(|i| i.data.title.as_deref().unwrap_or_default())
                .collect();
            assert_eq!(titles, vec!["Alpha", "Beta", "Zeta"]);
        }

        #[test]
        fn sorts_by_date_descending() {
            let items = vec![
                item("K1", "A", "2022"),
                item("K2", "B", "2025"),
                item("K3", "C", "2023"),
            ];
            let sorted = sort_items(items, SortField::Date, SortDirection::Desc);
            let keys: Vec<&str> = sorted.iter().map(|i| i.key.as_str()).collect();
            assert_eq!(keys, vec!["K2", "K3", "K1"]);
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mise run test src::zotero::search::tests::match_condition src::zotero::search::tests::sort_items`
Expected: FAIL — `SearchOperator::IsNot` / `SortField` / `sort_items` don't exist yet.

- [ ] **Step 3: Write minimal implementation**

Replace the `SearchOperator` enum (lines 43-55) with:

```rust
/// Comparison operator in structured searches.
#[derive(
    Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SearchOperator {
    #[default]
    Contains,
    Is,
    StartsWith,
    EndsWith,
    IsNot,
    DoesNotContain,
    IsGreaterThan,
    IsLessThan,
    IsBefore,
    IsAfter,
    #[serde(untagged)]
    Other(String),
}
```

Add after `SearchCondition` (after line 64):

```rust
/// How multiple conditions are combined: `all` (AND, default) or `any` (OR).
#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum JoinMode {
    #[default]
    All,
    Any,
}

/// Item field to sort results by.
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SortField {
    DateAdded,
    DateModified,
    Title,
    Date,
    Creator,
}

/// Sort direction.
#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SortDirection {
    #[default]
    Asc,
    Desc,
}
```

Replace `match_condition` (lines 185-224) with:

```rust
/// Evaluates whether `item` satisfies a single search `cond`.
fn match_condition(item: &ZoteroItem, cond: &SearchCondition) -> bool {
    let val = cond.value.to_lowercase();
    let matches_str = |s: &str| match cond.operator {
        SearchOperator::Is => s.to_lowercase() == val,
        SearchOperator::IsNot => s.to_lowercase() != val,
        SearchOperator::StartsWith => s.to_lowercase().starts_with(&val),
        SearchOperator::EndsWith => s.to_lowercase().ends_with(&val),
        SearchOperator::DoesNotContain => !s.to_lowercase().contains(&val),
        SearchOperator::Contains | SearchOperator::Other(_) => {
            s.to_lowercase().contains(&val)
        }
        SearchOperator::IsGreaterThan | SearchOperator::IsAfter => {
            compare_dates(s, &cond.value).is_gt()
        }
        SearchOperator::IsLessThan | SearchOperator::IsBefore => {
            compare_dates(s, &cond.value).is_lt()
        }
    };

    match &cond.field {
        SearchField::Title => {
            item.data.title.as_deref().is_some_and(matches_str)
        }
        SearchField::Creator => item.data.creators.iter().any(|c| {
            let full = format!(
                "{} {}",
                c.first_name.as_deref().unwrap_or(""),
                c.last_name.as_deref().unwrap_or("")
            );
            matches_str(&full) || c.name.as_deref().is_some_and(matches_str)
        }),
        SearchField::Date => item.data.date.as_deref().is_some_and(matches_str),
        SearchField::Year => item.data.date.as_deref().is_some_and(|d| {
            matches_str(&d.chars().take(4).collect::<String>())
        }),
        SearchField::ItemType => matches_str(item.data.item_type.as_str()),
        SearchField::Tag => item.data.tags.iter().any(|t| matches_str(&t.tag)),
        SearchField::Extra => {
            item.data.extra.as_deref().is_some_and(matches_str)
        }
        SearchField::Doi => item.data.doi.as_deref().is_some_and(matches_str),
        SearchField::Other(field_name) => match field_name.as_str() {
            "title" => item.data.title.as_deref().is_some_and(matches_str),
            "doi" => item.data.doi.as_deref().is_some_and(matches_str),
            _ => false,
        },
    }
}

/// Compares two date-or-year strings (`YYYY`, `YYYY-MM`, `YYYY-MM-DD`) by
/// their leading numeric components. Missing components compare as zero.
fn compare_dates(a: &str, b: &str) -> std::cmp::Ordering {
    date_key(a).cmp(&date_key(b))
}

/// Splits `s` into `(year, month, day)` numeric components.
fn date_key(s: &str) -> (u32, u32, u32) {
    let mut parts = s.split('-').filter(|p| !p.is_empty());
    let next = |parts: &mut dyn Iterator<Item = &str>| {
        parts
            .next()
            .and_then(|p| p.parse::<u32>().ok())
            .unwrap_or(0)
    };
    (next(&mut parts), next(&mut parts), next(&mut parts))
}
```

Add sort helpers after `match_condition`:

```rust
/// Sorts `items` in place-order by `field` in `direction` and returns them.
fn sort_items(
    items: Vec<ZoteroItem>,
    field: SortField,
    direction: SortDirection,
) -> Vec<ZoteroItem> {
    let mut items = items;
    items.sort_by(|a, b| {
        let ord = sort_key(a, field).cmp(&sort_key(b, field));
        match direction {
            SortDirection::Asc => ord,
            SortDirection::Desc => ord.reverse(),
        }
    });
    items
}

/// Returns the sort key string for `item` under `field`.
fn sort_key(item: &ZoteroItem, field: SortField) -> String {
    match field {
        SortField::Title => item.data.title.clone().unwrap_or_default(),
        SortField::Date => item.data.date.clone().unwrap_or_default(),
        SortField::DateAdded => item.data.date_added.clone().unwrap_or_default(),
        SortField::DateModified => {
            item.data.date_modified.clone().unwrap_or_default()
        }
        SortField::Creator => item.data.creators.first().map_or_else(
            String::new,
            |c| {
                c.name.clone().unwrap_or_else(|| {
                    format!(
                        "{} {}",
                        c.first_name.as_deref().unwrap_or(""),
                        c.last_name.as_deref().unwrap_or("")
                    )
                })
            },
        ),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `mise run test src::zotero::search::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/zotero/search.rs
git commit -m "feat: add advanced search operators, join mode, and sort"
```

---

## Task 4: Paginated response shape — `PaginationInfo` + `SearchPage`, breaking signature change

**Files:**
- Modify: `src/zotero/search.rs` (`PaginationInfo`, `SearchPage`, `search_items`, `advanced_search` — add offset/limit/join/sort params; return page)
- Modify: `src/mcp/zotero.rs` (`SearchItemsArgs`, `AdvancedSearchArgs`; `zotero_search_items_impl:434`, `zotero_advanced_search_impl:963`, `zotero_add_by_identifier_impl:1076`)
- Modify: `src/mcp/connector_tools.rs:50-54`
- Test: `src/zotero/search.rs` new `#[cfg(test)] mod advanced_search`

- [ ] **Step 1: Write the failing test**

Add a test module to `src/zotero/search.rs`. It must exercise both the **slow path** (client-side filter over the full library) and the **fast path** (server pushdown) of the new `advanced_search`. The fast path issues **one** request that returns `Total-Results` and a paged slice; the slow path pages the whole library then filters.

```rust
    mod advanced_search {
        use super::*;
        use crate::zotero::client::ZoteroClient;

        fn items_page(items: &[serde_json::Value]) -> String {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }

        fn zotero_item(
            key: &str,
            title: &str,
            extra: Option<&str>,
        ) -> serde_json::Value {
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": "journalArticle",
                    "title": title,
                    "extra": extra,
                    "dateAdded": "2024-01-01T00:00:00Z",
                    "dateModified": "2024-01-01T00:00:00Z",
                },
            })
        }

        fn title_contains(value: &str) -> SearchCondition {
            SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::Contains,
                value: value.to_owned(),
            }
        }

        #[tokio::test]
        async fn slow_path_filters_full_library_and_paginates() {
            // An `extra`-field condition cannot be expressed as server quick
            // search params, so the slow path (full scan + client filter) runs.
            // Matches: K1 & K2 (extra "book"). Sorted by title ascending:
            // [K2 "Rust for Beginners", K1 "Rust in Action"].
            let item1 = zotero_item("K1", "Rust in Action", Some("book"));
            let item2 = zotero_item("K2", "Rust for Beginners", Some("book"));
            let item3 = zotero_item("K3", "Rust Essentials", Some("talk"));
            let base = mock_server(vec![
                http_response("200 OK", &items_page(&[item1, item2, item3])),
            ]);
            let state = zotero_state(base);

            let cond_extra = SearchCondition {
                field: SearchField::Extra,
                operator: SearchOperator::Is,
                value: "book".to_owned(),
            };
            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust"), cond_extra],
                    JoinMode::All,
                    Some(SortField::Title),
                    SortDirection::Asc,
                    0,
                    1,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].key.as_str(), "K2");
            assert_eq!(page.pagination.limit, 1);
            assert_eq!(page.pagination.offset, 0);
            assert_eq!(page.pagination.total, 2);
            assert!(page.pagination.has_more);
        }

        #[tokio::test]
        async fn fast_path_uses_server_side_search_and_total_header() {
            let item1 = zotero_item("K1", "Rust in Action", None);
            let base = mock_server(vec![
                http_response_with_headers(
                    "200 OK",
                    &[("Total-Results", "17")],
                    &items_page(&[item1]),
                ),
            ]);
            let state = zotero_state(base);

            let page = ZoteroClient::new(&state)
                .advanced_search(
                    vec![title_contains("Rust")],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    10,
                )
                .await
                .unwrap();

            assert_eq!(page.items.len(), 1);
            assert_eq!(page.pagination.total, 17);
            assert_eq!(page.pagination.offset, 0);
            assert!(page.pagination.has_more);
        }
    }
```

Note: same visibility caveat as Task 2 — `crate::mcp::zotero::tests` fixtures are not reachable from `search.rs`. Provide the small `mock_server`/`http_response`/`http_response_with_headers`/`zotero_state` fixture helpers **inside this test module** (copy the pattern from `src/mcp/zotero.rs` tests at lines 1239-1292, with `zotero_state` = the `test_state` variant shown in Task 2). The test bodies above are the spec.

- [ ] **Step 2: Run test to verify it fails**

Run: `mise run test src::zotero::search::tests::advanced_search`
Expected: FAIL — `advanced_search` has the wrong signature (no `join_mode`/`sort`/`offset`), and `ZoteroItem.key` doesn't matter yet — the compile error is the signal.

- [ ] **Step 3: Write minimal implementation**

Add to `src/zotero/search.rs` (after `SearchCondition`):

```rust
/// Pagination metadata returned with every search result page.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) struct PaginationInfo {
    pub(crate) limit: usize,
    pub(crate) offset: usize,
    pub(crate) total: usize,
    pub(crate) has_more: bool,
}

/// A page of search results plus its pagination metadata.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) struct SearchPage<T> {
    pub(crate) items: Vec<T>,
    pub(crate) pagination: PaginationInfo,
}

/// Returns a `{items, pagination}` page slicing `results` at `offset`/`limit`.
fn paginate<T>(results: Vec<T>, offset: usize, limit: usize) -> SearchPage<T> {
    let total = results.len();
    let skip = offset.min(total);
    let items: Vec<T> = results
        .into_iter()
        .skip(skip)
        .take(limit)
        .collect();
    SearchPage {
        items,
        pagination: PaginationInfo {
            limit,
            offset: skip,
            total,
            has_more: skip.saturating_add(limit) < total,
        },
    }
}
```

Replace `search_items` (lines 80-97) with a version that takes `offset` and returns a page:

```rust
    pub(crate) async fn search_items(
        &self,
        query: &str,
        collection_key: Option<&CollectionKey>,
        offset: usize,
        limit: usize,
    ) -> Result<SearchPage<ZoteroItem>, ZoteroMcpError> {
        let base = match collection_key {
            Some(col) => format!(
                "{}/users/0/collections/{}/items",
                self.state.zotero_api_url, col
            ),
            None => format!("{}/users/0/items", self.state.zotero_api_url),
        };
        let encoded_q = urlencoding::encode(query);
        let url = format!(
            "{base}?q={encoded_q}&start={offset}&limit={limit}&itemType=-note"
        );
        let (items, total) = self.get_items_with_total(&url).await?;
        let total = if total == 0 { offset.saturating_add(items.len()) } else { total };
        Ok(SearchPage {
            items,
            pagination: PaginationInfo {
                limit,
                offset,
                total,
                has_more: offset.saturating_add(items.len()) < total,
            },
        })
    }
```

Replace `advanced_search` (lines 167-181) with the pushdown + slow-path version:

```rust
    pub(crate) async fn advanced_search(
        &self,
        conditions: Vec<SearchCondition>,
        join_mode: JoinMode,
        sort: Option<SortField>,
        sort_direction: SortDirection,
        offset: usize,
        limit: usize,
    ) -> Result<SearchPage<ZoteroItem>, ZoteroMcpError> {
        if join_mode == JoinMode::All {
            if let Some(url) = self.pushdown_url(&conditions) {
                let full_url = format!(
                    "{url}&start={offset}&limit={limit}&itemType=-note"
                );
                let (items, total) = self.get_items_with_total(&full_url).await?;
                let total = if total == 0 {
                    offset.saturating_add(items.len())
                } else {
                    total
                };
                return Ok(SearchPage {
                    items,
                    pagination: PaginationInfo {
                        limit,
                        offset,
                        total,
                        has_more: offset.saturating_add(items.len()) < total,
                    },
                });
            }
        }

        let items = self.get_all_items().await?;
        let matches: Vec<ZoteroItem> = items
            .into_iter()
            .filter(|item| {
                let ok = match join_mode {
                    JoinMode::All => conditions
                        .iter()
                        .all(|cond| match_condition(item, cond)),
                    JoinMode::Any => conditions
                        .iter()
                        .any(|cond| match_condition(item, cond)),
                };
                ok && is_searchable_item(item)
            })
            .collect();
        let matches = match sort {
            Some(field) => sort_items(matches, field, sort_direction),
            None => matches,
        };
        Ok(paginate(matches, offset, limit))
    }
```

Add `pushdown_url` and `is_searchable_item` as methods/free functions in `search.rs`:

```rust
impl ZoteroClient<'_> {
    /// Builds a server-search URL for `conditions` when they are fully
    /// expressible as Zotero quick-search parameters, or `None` to fall back
    /// to a client-side scan.
    fn pushdown_url(&self, conditions: &[SearchCondition]) -> Option<String> {
        let mut q: Option<String> = None;
        let mut qmode = "titleCreatorYear".to_owned();
        let mut item_type: Option<String> = None;
        let mut tag: Option<String> = None;

        for cond in conditions {
            let value = &cond.value;
            let operator_pushable = matches!(
                cond.operator,
                SearchOperator::Contains
                    | SearchOperator::Is
                    | SearchOperator::StartsWith
            );
            if !operator_pushable {
                return None;
            }
            match &cond.field {
                SearchField::Title
                | SearchField::Creator
                | SearchField::Year
                | SearchField::Date => {
                    if q.is_some() {
                        return None; // only one free-text term
                    }
                    q = Some(value.clone());
                    qmode = match &cond.field {
                        SearchField::Creator => "creator".to_owned(),
                        SearchField::Year | SearchField::Date => {
                            "year".to_owned()
                        }
                        _ => "titleCreatorYear".to_owned(),
                    };
                }
                SearchField::ItemType if cond.operator == SearchOperator::Is => {
                    if item_type.is_some() {
                        return None;
                    }
                    item_type = Some(value.clone());
                }
                SearchField::Tag if cond.operator == SearchOperator::Is => {
                    if tag.is_some() {
                        return None;
                    }
                    tag = Some(value.clone());
                }
                _ => return None,
            }
        }

        let mut url =
            format!("{}/users/0/items", self.state.zotero_api_url);
        let mut params = Vec::new();
        if let Some(ref q) = q {
            params.push(format!("q={}", urlencoding::encode(q)));
            params.push(format!("qmode={qmode}"));
        }
        if let Some(ref item_type) = item_type {
            params.push(format!("itemType={item_type}"));
        }
        if let Some(ref tag) = tag {
            params.push(format!("tag={}", urlencoding::encode(tag)));
        }
        if params.is_empty() {
            return None;
        }
        url.push('?');
        url.push_str(&params.join("&"));
        Some(url)
    }
}

/// Returns true for items that are not attachments, notes, or annotations.
fn is_searchable_item(item: &ZoteroItem) -> bool {
    !matches!(
        item.data.item_type,
        ItemType::Attachment | ItemType::Note | ItemType::Annotation
    )
}
```

Import `ItemType` in `search.rs` (add to the `use crate::zotero::...` import):

```rust
        models::{CitationKey, CollectionKey, ItemType, TagName, ZoteroItem},
```

Now update all callers:

**`src/zotero/search.rs` `search_by_citation_key` (line 137):**

```rust
        let page = self.search_items(citekey.as_str(), None, 0, 20).await?;
        let citekey_lc = citekey.as_str().to_lowercase();
        for item in page.items {
```

**`src/mcp/zotero.rs` `SearchItemsArgs` (lines 66-73):**

```rust
/// Arguments for `zotero_search_items`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchItemsArgs {
    /// Search query matched against title, creator, year, or fulltext.
    pub(crate) query: String,
    /// Optional collection key ([`CollectionKey`]) to search within.
    pub(crate) collection_key: Option<CollectionKey>,
    /// 0-based offset into the full result set (default: 0).
    pub(crate) start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}
```

**`src/mcp/zotero.rs` `AdvancedSearchArgs` (lines 314-319):**

```rust
/// Arguments for `zotero_advanced_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct AdvancedSearchArgs {
    /// List of search conditions ([`SearchCondition`]).
    pub(crate) conditions: Vec<SearchCondition>,
    /// `"all"` (AND, default) or `"any"` (OR).
    pub(crate) join_mode: Option<JoinMode>,
    /// Sort field: `"dateAdded"`, `"dateModified"`, `"title"`, `"date"`, or
    /// `"creator"`.
    pub(crate) sort_by: Option<SortField>,
    /// Sort direction: `"asc"` or `"desc"` (default: `"asc"`).
    pub(crate) sort_direction: Option<SortDirection>,
    /// 0-based offset into the full result set (default: 0).
    pub(crate) start: Option<usize>,
    /// Maximum number of items to return (default: 20).
    pub(crate) limit: Option<usize>,
}
```

**`src/mcp/zotero.rs` `zotero_search_items_impl` (lines 434-445):**

```rust
    pub(crate) async fn zotero_search_items_impl(
        &self,
        args: SearchItemsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client
                .search_items(
                    &args.query,
                    args.collection_key.as_ref(),
                    offset,
                    limit,
                )
                .await,
        ))
    }
```

**`src/mcp/zotero.rs` `zotero_advanced_search_impl` (lines 963-972):**

```rust
    pub(crate) async fn zotero_advanced_search_impl(
        &self,
        args: AdvancedSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let offset = args.start.unwrap_or(0);
        let limit = args.limit.unwrap_or(20);
        let client = ZoteroClient::new(&self.state);
        Ok(super::json_result(
            client
                .advanced_search(
                    args.conditions,
                    args.join_mode.unwrap_or_default(),
                    args.sort_by,
                    args.sort_direction.unwrap_or_default(),
                    offset,
                    limit,
                )
                .await,
        ))
    }
```

**`src/mcp/zotero.rs` `zotero_add_by_identifier_impl` (line 1076):**

```rust
            let existing = client
                .advanced_search(
                    vec![cond],
                    JoinMode::All,
                    None,
                    SortDirection::Asc,
                    0,
                    1,
                )
                .await;
            if let Ok(page) = existing {
                if let Some(found) = page.items.into_iter().next() {
                    return Ok(super::json_success(&found));
                }
            }
```

Check the `use` list at the top of `src/mcp/zotero.rs` — add `JoinMode`, `SortDirection` from `crate::zotero::search` (and `SortField` if used in the args struct).

**`src/mcp/connector_tools.rs` (around lines 46-56)**: find the `SearchItemsArgs` construction and add `start: None`:

```rust
            SearchItemsArgs {
                query: args.query,
                collection_key: None,
                start: None,
                limit: Some(20),
            }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `mise run check && mise run test && mise run clippy`
Expected: PASS — whole workspace compiles, all tests pass, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/zotero/search.rs src/mcp/zotero.rs src/mcp/connector_tools.rs
git commit -m "feat: paginated search responses with offset and total"
```

---

## Task 5: Server-side pushdown tests

**Files:**
- Modify: `src/zotero/search.rs` (only the `advanced_search` test module from Task 4)

The fast-path (`pushdown_url`) behavior is already wired in Task 4 but only lightly tested. Add focused unit tests for `pushdown_url` itself:

- [ ] **Step 1: Write the failing test**

Append to the `advanced_search` test module in `src/zotero/search.rs`:

```rust
        #[test]
        fn pushdown_url_encodes_free_text_title() {
            let state = zotero_state(
                "http://127.0.0.1:23119/api".to_owned(),
            );
            let client = ZoteroClient::new(&state);
            let url = client
                .pushdown_url(&[title_contains("Rust Programming")])
                .unwrap();
            assert!(url.contains("q=Rust%20Programming"));
            assert!(url.contains("qmode=titleCreatorYear"));
        }

        #[test]
        fn pushdown_url_refuses_non_pushable_operator() {
            let state = zotero_state(
                "http://127.0.0.1:23119/api".to_owned(),
            );
            let client = ZoteroClient::new(&state);
            let cond = SearchCondition {
                field: SearchField::Title,
                operator: SearchOperator::DoesNotContain,
                value: "Rust".to_owned(),
            };
            assert!(client.pushdown_url(&[cond]).is_none());
        }

        #[test]
        fn pushdown_url_encodes_item_type_and_tag() {
            let state = zotero_state(
                "http://127.0.0.1:23119/api".to_owned(),
            );
            let client = ZoteroClient::new(&state);
            let conds = vec![
                SearchCondition {
                    field: SearchField::ItemType,
                    operator: SearchOperator::Is,
                    value: "conferencePaper".to_owned(),
                },
                SearchCondition {
                    field: SearchField::Tag,
                    operator: SearchOperator::Is,
                    value: "methods".to_owned(),
                },
            ];
            let url = client.pushdown_url(&conds).unwrap();
            assert!(url.contains("itemType=conferencePaper"));
            assert!(url.contains("tag=methods"));
        }
```

(Caveat: same fixture reachability note as before — include the fixture helpers locally in this test module.)

- [ ] **Step 2: Run test to verify it fails**

Run: `mise run test src::zotero::search::tests::advanced_search`
Expected: PASS already if Task 4 included `pushdown_url`. If the tests pass immediately, still keep them (they lock behavior). If `pushdown_url` isn't `pub(crate)`-visible from the test module, add `pub(crate)` to it. This task is primarily regression coverage.

- [ ] **Step 3: (no new implementation — behavior already shipped in Task 4)**

- [ ] **Step 4: Run test to verify it passes**

Run: `mise run test src::zotero::search::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/zotero/search.rs
git commit -m "test: lock server-side search pushdown behavior"
```

---

## Task 6: sqlite access gate on `AppState`

**Files:**
- Modify: `src/state.rs`
- Modify: `src/errors.rs`
- Test: `src/state.rs` `#[cfg(test)] mod check_sqlite_access`

- [ ] **Step 1: Write the failing test**

Append to `src/state.rs` tests:

```rust
    mod check_sqlite_access {
        use super::super::*;
        use super::fixtures::test_state;

        #[test]
        fn rejects_when_sqlite_access_is_disabled_by_default() {
            let state = test_state(false);
            assert!(matches!(
                state.check_sqlite_access(),
                Err(ZoteroMcpError::PermissionDenied(_))
            ));
        }

        #[test]
        fn allows_when_sqlite_access_is_enabled() {
            let mut state = test_state(false);
            state.sqlite_access = true;
            assert!(state.check_sqlite_access().is_ok());
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mise run test state::tests::check_sqlite_access`
Expected: FAIL — `check_sqlite_access` / `sqlite_access` don't exist.

- [ ] **Step 3: Write minimal implementation**

In `src/errors.rs`, add a variant (after `Io`):

```rust
    /// Local Zotero sqlite database access failed.
    #[error("Local database error: {0}")]
    LocalDb(String),
```

In `src/state.rs`:

1. Add the field to `AppState` (after `write_enabled`):

```rust
    /// Whether reading Zotero's local sqlite database is allowed. Defaults to
    /// false; enable by setting `ZOTERO_SQLITE_ACCESS`.
    pub(crate) sqlite_access: bool,
```

2. In `from_env`, after the `write_enabled` binding:

```rust
        let sqlite_access = env::var("ZOTERO_SQLITE_ACCESS")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
```

and add `sqlite_access,` to the struct literal.

3. Add the helper (after `check_write_permission`):

```rust
    /// Checks whether reading the local Zotero sqlite database is permitted.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if [`sqlite_access`] is `false` (the default)
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`sqlite_access`]: Self::sqlite_access
    pub(crate) fn check_sqlite_access(&self) -> Result<(), ZoteroMcpError> {
        if self.sqlite_access {
            Ok(())
        } else {
            Err(ZoteroMcpError::PermissionDenied(
                "Local database access rejected: set ZOTERO_SQLITE_ACCESS=1 \
                 to enable reading Zotero's sqlite database"
                    .to_owned(),
            ))
        }
    }
```

4. Update the two remaining full-literal `AppState` constructors (they do not use `..from_env()`), adding `sqlite_access: false,`:
   - `src/state.rs` `test_state` (line ~354)
   - `src/zotero/client.rs` `test_state` (line ~242)

- [ ] **Step 4: Run test to verify it passes**

Run: `mise run test state::tests::check_sqlite_access && mise run check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs src/errors.rs src/zotero/client.rs
git commit -m "feat: gate local sqlite access behind ZOTERO_SQLITE_ACCESS"
```

---

## Task 7: sqlite reader module — discovery, open, full-text and note/annotation search

**Files:**
- Create: `src/zotero/local_db.rs`
- Modify: `src/zotero/mod.rs` (`mod local_db;` + re-exports)
- Modify: `Cargo.toml` (sqlx dep)
- Test: `src/zotero/local_db.rs` `#[cfg(test)]`

- [ ] **Step 1: Add the sqlx dependency to `Cargo.toml`**

Add to the `[dependencies]` block (alphabetical, near `serde`):

```toml
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "sqlite"] }
```

- [ ] **Step 2: Write the failing test**

Create `src/zotero/local_db.rs` with the implementation (Step 3) **and** the following tests. The tests build a real sqlite file with `tempfile`, seed it with Zotero's schema shape, close the writer pool, then open it through the immutable/read-only reader path.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    async fn seed_db(path: &PathBuf) {
        use sqlx::Row;
        let opts = SqliteConnectOptions::from_str(
            &format!("sqlite://{}", path.display()),
        )
        .unwrap()
        .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();

        // Items + itemTypes
        sqlx::query("CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE items (itemID INTEGER PRIMARY KEY, key TEXT, itemTypeID INTEGER, dateAdded TEXT, dateModified TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE fields (fieldID INTEGER PRIMARY KEY, fieldName TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE itemData (itemID INTEGER, fieldID INTEGER, valueID INTEGER)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, value TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, firstName TEXT, lastName TEXT, name TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE itemCreators (itemID INTEGER, creatorID INTEGER)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE deletedItems (itemID INTEGER)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE fulltextItems (itemID INTEGER, content TEXT, indexedChars INTEGER, totalChars INTEGER, version INTEGER)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE itemNotes (itemID INTEGER, parentItemID INTEGER, note TEXT, title TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE itemAnnotations (itemID INTEGER, parentItemID INTEGER, text TEXT, comment TEXT, type INTEGER, color TEXT, pageLabel TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE itemAttachments (itemID INTEGER, parentItemID INTEGER, path TEXT, contentType TEXT)").execute(&pool).await.unwrap();

        // fields: title=1, extra=16, DOI
        sqlx::query("INSERT INTO fields (fieldID, fieldName) VALUES (1, 'title'), (16, 'extra'), (7, 'DOI')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO itemTypes (itemTypeID, typeName) VALUES (1, 'journalArticle'), (2, 'note')").execute(&pool).await.unwrap();

        // item 1: "Rust in Action" with fulltext mentioning "borrow checker"
        sqlx::query("INSERT INTO items (itemID, key, itemTypeID, dateAdded, dateModified) VALUES (1, 'K00001', 1, '2024-01-01', '2024-02-01')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO itemData (itemID, fieldID, valueID) VALUES (1, 1, 100), (1, 7, 101)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO itemDataValues (valueID, value) VALUES (100, 'Rust in Action'), (101, '10.1000/rust')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO fulltextItems (itemID, content) VALUES (1, 'The borrow checker ensures memory safety.')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO creators (creatorID, firstName, lastName, name) VALUES (1, 'Jon', 'Gjengset', NULL)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO itemCreators (itemID, creatorID) VALUES (1, 1)").execute(&pool).await.unwrap();

        // item 2: a note child of item 1
        sqlx::query("INSERT INTO items (itemID, key, itemTypeID, dateAdded, dateModified) VALUES (2, 'N00001', 2, '2024-03-01', '2024-03-01')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO itemNotes (itemID, parentItemID, note, title) VALUES (2, 1, '<p>Ownership summary</p>', 'summary')").execute(&pool).await.unwrap();

        pool.close().await;
    }

    #[tokio::test]
    async fn opens_read_only_immutable_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        seed_db(&db_path).await;

        // open() succeeding means probe_schema passed. Prove the pool is
        // usable with a real query rather than asserting pool.size() (which
        // is 0 until a connection is actually used).
        let db = LocalZoteroDb::open(&db_path).await.unwrap();
        let hits = db.search_fulltext("safety", 5).await.unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn rejects_non_zotero_database() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other.sqlite");
        let opts = SqliteConnectOptions::from_str(
            &format!("sqlite://{}", other.display()),
        )
        .unwrap()
        .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();
        sqlx::query("CREATE TABLE anything (x INTEGER)").execute(&pool).await.unwrap();
        pool.close().await;

        let err = LocalZoteroDb::open(&other).await.unwrap_err();
        assert!(matches!(err, ZoteroMcpError::LocalDb(_)));
    }

    #[tokio::test]
    async fn searches_fulltext_across_items() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        seed_db(&db_path).await;
        let db = LocalZoteroDb::open(&db_path).await.unwrap();

        let hits = db.search_fulltext("borrow checker", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title.as_deref(), Some("Rust in Action"));
        assert!(hits[0].snippet.contains("borrow checker"));

        let none = db.search_fulltext("nothing matches", 10).await.unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn searches_notes_and_annotations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        seed_db(&db_path).await;
        let db = LocalZoteroDb::open(&db_path).await.unwrap();

        let hits = db.search_notes_annotations("ownership", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "note");
        assert_eq!(hits[0].parent_key.as_ref().map(|k| k.as_str()), Some("K00001"));
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `mise run test src::zotero::local_db::tests`
Expected: FAIL — `local_db` module / `LocalZoteroDb` don't exist (module not declared, so compile error). Register the module in `src/zotero/mod.rs` first (see Step 5) so the crate compiles.

- [ ] **Step 4: Write minimal implementation**

Create `src/zotero/local_db.rs`:

```rust
//! Read-only access to Zotero's local `zotero.sqlite` database.
//!
//! Mirrors the discovery and SQL approach documented in
//! `docs/54yyyu-zotero-mcp-digest.txt` (lines 6712-7779): locate the
//! database via `ZOTERO_DB_PATH`, the `prefs.js` `dataDir` preference, or the
//! per-user default; open it immutable/read-only so a running Zotero does not
//! block reads; and query the `itemData`/`fulltextItems`/`itemNotes`/
//! `itemAnnotations` tables directly.
//!
//! Every method is gated at the MCP tool layer by
//! [`AppState::check_sqlite_access`].

use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::{
    errors::ZoteroMcpError,
    zotero::models::ItemKey,
};

/// Max rows to pull from the full-text scan before filtering in Rust.
const FULLTEXT_SCAN_CAP: usize = 2000;

/// Opens Zotero's local sqlite database in immutable read-only mode.
#[derive(Clone)]
pub(crate) struct LocalZoteroDb {
    pool: SqlitePool,
}

impl LocalZoteroDb {
    /// Opens `path` read-only with `immutable=1` semantics (mirrors the
    /// digest's `_get_connection`). Fails with [`ZoteroMcpError::LocalDb`] if
    /// `path` is unreadable or is not a Zotero database.
    pub(crate) async fn open(path: &Path) -> Result<Self, ZoteroMcpError> {
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            path.display()
        ))
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?
        .read_only(true)
        .immutable(true)
        .busy_timeout(Duration::from_secs(2));
        let pool = SqlitePool::connect_with(opts)
            .await
            .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
        let db = Self { pool };
        db.probe_schema().await?;
        Ok(db)
    }

    /// Verifies the `items` table exists, confirming this is a Zotero db.
    async fn probe_schema(&self) -> Result<(), ZoteroMcpError> {
        let row = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='items'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
        if row.is_none() {
            return Err(ZoteroMcpError::LocalDb(
                "Not a Zotero database: 'items' table not found".to_owned(),
            ));
        }
        Ok(())
    }

    /// Searches the library for `query` across title, DOI, extra, and indexed
    /// fulltext, returning at most `limit` hits.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalDb`] if a query or row read fails
    pub(crate) async fn search_fulltext(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FulltextHit>, ZoteroMcpError> {
        let rows = sqlx::query(
            r#"
            SELECT i.key, it.typeName AS item_type,
                   title.value AS title, doi.value AS doi,
                   extra.value AS extra,
                   creators.creators AS creators,
                   ft.content AS fulltext
            FROM items i
            JOIN itemTypes it ON i.itemTypeID = it.itemTypeID
            LEFT JOIN itemData title_data
                ON title_data.itemID = i.itemID AND title_data.fieldID = 1
            LEFT JOIN itemDataValues title
                ON title.valueID = title_data.valueID
            LEFT JOIN fields doi_field
                ON doi_field.fieldName = 'DOI'
            LEFT JOIN itemData doi_data
                ON doi_data.itemID = i.itemID
               AND doi_data.fieldID = doi_field.fieldID
            LEFT JOIN itemDataValues doi
                ON doi.valueID = doi_data.valueID
            LEFT JOIN itemData extra_data
                ON extra_data.itemID = i.itemID AND extra_data.fieldID = 16
            LEFT JOIN itemDataValues extra
                ON extra.valueID = extra_data.valueID
            LEFT JOIN (
                SELECT ic.itemID, GROUP_CONCAT(
                    COALESCE(
                        c.name,
                        c.lastName || ' ' || c.firstName
                    ),
                    '; '
                ) AS creators
                FROM itemCreators ic
                JOIN creators c ON c.creatorID = ic.creatorID
                GROUP BY ic.itemID
            ) creators ON creators.itemID = i.itemID
            LEFT JOIN fulltextItems ft ON ft.itemID = i.itemID
            WHERE it.typeName NOT IN ('attachment', 'note', 'annotation')
              AND i.itemID NOT IN (SELECT itemID FROM deletedItems)
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;

        let query_lc = query.to_lowercase();
        let mut hits = Vec::new();
        for row in rows {
            if hits.len() >= FULLTEXT_SCAN_CAP {
                break;
            }
            let title: Option<String> =
                row.try_get("title").map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let doi: Option<String> =
                row.try_get("doi").map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let extra: Option<String> =
                row.try_get("extra").map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let creators: Option<String> =
                row.try_get("creators").map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let fulltext: Option<String> =
                row.try_get("fulltext").map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;

            let haystack = format!(
                "{} {} {} {}",
                title.as_deref().unwrap_or(""),
                creators.as_deref().unwrap_or(""),
                doi.as_deref().unwrap_or(""),
                fulltext.as_deref().unwrap_or("")
            );
            if !haystack.to_lowercase().contains(&query_lc) {
                continue;
            }
            let key: String =
                row.try_get("key").map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let item_type: String =
                row.try_get("item_type").map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            hits.push(FulltextHit {
                key: ItemKey::from(key),
                item_type,
                title,
                doi,
                creators: creators.unwrap_or_default(),
                snippet: fulltext
                    .as_deref()
                    .map(|f| f.chars().take(400).collect())
                    .unwrap_or_default(),
            });
            if hits.len() >= limit {
                break;
            }
        }
        Ok(hits)
    }

    /// Searches child notes and PDF annotations for `query`, returning at most
    /// `limit` hits. Mirrors the digest's `search_notes_local` /
    /// `search_annotations_local`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalDb`] if a query or row read fails
    pub(crate) async fn search_notes_annotations(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<NoteAnnotationHit>, ZoteroMcpError> {
        let pattern = format!("%{query}%");
        let note_rows = sqlx::query(
            r#"
            SELECT i.key, n.note, n.title,
                   pi.key AS parentKey, pdv.value AS parentTitle
            FROM itemNotes n
            JOIN items i ON n.itemID = i.itemID
            LEFT JOIN items pi ON n.parentItemID = pi.itemID
            LEFT JOIN itemData pd
                ON pd.itemID = pi.itemID AND pd.fieldID = 1
            LEFT JOIN itemDataValues pdv ON pd.valueID = pdv.valueID
            WHERE n.note LIKE ?
              AND i.itemID NOT IN (SELECT itemID FROM deletedItems)
            LIMIT ?
            "#,
        )
        .bind(pattern.as_str())
        .bind(i64::try_from(limit).unwrap_or(20))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;

        let ann_rows = sqlx::query(
            r#"
            SELECT i.key, ia.text, ia.comment, ia.type, ia.color,
                   ia.pageLabel, att.key AS attachmentKey,
                   gpi.key AS parentKey, gpdv.value AS parentTitle
            FROM itemAnnotations ia
            JOIN items i ON ia.itemID = i.itemID
            LEFT JOIN items att ON ia.parentItemID = att.itemID
            LEFT JOIN itemAttachments iatt ON ia.parentItemID = iatt.itemID
            LEFT JOIN items gpi ON iatt.parentItemID = gpi.itemID
            LEFT JOIN itemData gpd
                ON gpd.itemID = gpi.itemID AND gpd.fieldID = 1
            LEFT JOIN itemDataValues gpdv ON gpd.valueID = gpdv.valueID
            WHERE (ia.text LIKE ? OR ia.comment LIKE ?)
              AND i.itemID NOT IN (SELECT itemID FROM deletedItems)
            LIMIT ?
            "#,
        )
        .bind(pattern.as_str())
        .bind(pattern.as_str())
        .bind(i64::try_from(limit).unwrap_or(20))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;

        let mut hits = Vec::new();
        for row in note_rows {
            let note: Option<String> =
                row.try_get("note").map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?;
            let clean = strip_html(note.as_deref().unwrap_or(""));
            if !clean.to_lowercase().contains(&query.to_lowercase()) {
                continue;
            }
            hits.push(NoteAnnotationHit {
                kind: "note".to_owned(),
                key: ItemKey::from(
                    row.try_get::<String, _>("key")
                        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                ),
                text: note,
                comment: None,
                parent_key: row
                    .try_get::<Option<String>, _>("parentKey")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?
                    .map(ItemKey::from),
                parent_title: row
                    .try_get("parentTitle")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                page_label: None,
                color: None,
            });
        }
        for row in ann_rows {
            hits.push(NoteAnnotationHit {
                kind: "annotation".to_owned(),
                key: ItemKey::from(
                    row.try_get::<String, _>("key")
                        .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                ),
                text: row
                    .try_get("text")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                comment: row
                    .try_get("comment")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                parent_key: row
                    .try_get::<Option<String>, _>("parentKey")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?
                    .map(ItemKey::from),
                parent_title: row
                    .try_get("parentTitle")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                page_label: row
                    .try_get("pageLabel")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
                color: row
                    .try_get("color")
                    .map_err(|e| ZoteroMcpError::LocalDb(e.to_string()))?,
            });
        }
        hits.truncate(limit);
        Ok(hits)
    }
}

/// A single full-text search hit.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct FulltextHit {
    pub(crate) key: ItemKey,
    pub(crate) item_type: String,
    pub(crate) title: Option<String>,
    pub(crate) doi: Option<String>,
    pub(crate) creators: String,
    pub(crate) snippet: String,
}

/// A single note or annotation search hit.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct NoteAnnotationHit {
    pub(crate) kind: String,
    pub(crate) key: ItemKey,
    pub(crate) text: Option<String>,
    pub(crate) comment: Option<String>,
    pub(crate) parent_key: Option<ItemKey>,
    pub(crate) parent_title: Option<String>,
    pub(crate) page_label: Option<String>,
    pub(crate) color: Option<String>,
}

/// Locates `zotero.sqlite` via `ZOTERO_DB_PATH`, the `prefs.js` `dataDir`
/// preference in any profile dir, or the per-user default, in that order.
pub(crate) fn find_zotero_db() -> Option<PathBuf> {
    if let Some(path) = env::var_os("ZOTERO_DB_PATH").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    for dir in profiles_dirs() {
        if let Some(db) = db_in_profile(&dir) {
            return Some(db);
        }
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|home| db_in_dir(&home.join("Zotero")))
}

/// Looks up the `dataDir` pref in `prefs.js`, then falls back to the profile
/// dir itself.
fn db_in_profile(profile_dir: &Path) -> Option<PathBuf> {
    let prefs = profile_dir.join("prefs.js");
    if prefs.is_file() {
        if let Some(data_dir) = read_string_pref(&prefs, "extensions.zotero.dataDir")
        {
            if let Some(db) = db_in_dir(&PathBuf::from(data_dir)) {
                return Some(db);
            }
        }
    }
    db_in_dir(profile_dir)
}

/// Returns `dir/zotero.sqlite` if it exists.
fn db_in_dir(dir: &Path) -> Option<PathBuf> {
    let db = dir.join("zotero.sqlite");
    db.is_file().then_some(db)
}

/// Candidate profile directories, per-OS (mirrors the digest's
/// `_zotero_profiles_dirs`).
fn profiles_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(appdata) = env::var_os("APPDATA") {
        dirs.push(
            PathBuf::from(appdata).join("Zotero").join("Zotero").join("Profiles"),
        );
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        dirs.push(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Zotero")
                .join("Profiles"),
        );
    }
    #[cfg(target_os = "linux")]
    if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".zotero").join("zotero"));
    }
    dirs
}

/// Parses `user_pref("key", "value");` from Zotero's `prefs.js`, returning
/// the unquoted value.
fn read_string_pref(prefs: &Path, key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(prefs).ok()?;
    let needle = format!("user_pref(\"{key}\",");
    contents.lines().find_map(|line| {
        let line = line.trim();
        if !line.starts_with(&needle) {
            return None;
        }
        let rest = line.trim_start_matches(&needle);
        let value = rest.strip_prefix('"')?.split('"').next()?;
        Some(value.replace("\\\"", "\"").replace("\\\\", "\\"))
    })
}

/// Strips HTML tags from Zotero note HTML.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}
```

**`src/zotero/mod.rs`** — add `mod local_db;` after `mod items;` and re-export the types:

```rust
pub(crate) use local_db::{FulltextHit, LocalZoteroDb, NoteAnnotationHit};
```

- [ ] **Step 5: Run test to verify it passes**

Run: `mise run check && mise run test src::zotero::local_db::tests && mise run clippy`
Expected: PASS — clippy may demand `# Errors` docs or flag `i64::try_from(limit).unwrap_or(20)` (that's fine: `unwrap_or` on a `Result` is allowed; `expect_used` is the forbidden one). If `missing_errors_doc` fires, add the `# Errors` sections already shown. If `arithmetic_side_effects` fires on `hits.truncate(limit)` or the format strings, re-read and adjust (saturating arithmetic only applies to `+`/`-`).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/zotero/local_db.rs src/zotero/mod.rs
git commit -m "feat: read-only local Zotero sqlite search layer"
```

---

## Task 8: Register the two new tools + wire the gate into handlers

**Files:**
- Modify: `src/mcp/zotero.rs` (arg structs + two `*_impl` handlers + fixtures for sqlite tools)
- Modify: `src/mcp/server.rs` (register tools after `zotero_advanced_search`)
- Test: `src/mcp/zotero.rs` `#[cfg(test)] mod sqlite_tools`

- [ ] **Step 1: Write the failing test**

Add arg structs and impls (Step 3) first so the test compiles, then add a test that proves the gate returns an error when disabled and a real hit when enabled. Use a `tempfile`-seeded sqlite (reuse the seed shape from Task 7; the fixture needs its own `seed_db` since it's in a different module).

Append to the `#[cfg(test)]` module in `src/mcp/zotero.rs`:

```rust
    mod sqlite_tools {
        use super::fixtures::*;
        use std::path::PathBuf;
        use std::str::FromStr;

        use sqlx::{SqliteConnectOptions, SqlitePool};

        async fn seed_db(path: &PathBuf) {
            let opts = SqliteConnectOptions::from_str(
                &format!("sqlite://{}", path.display()),
            )
            .unwrap()
            .create_if_missing(true);
            let pool = SqlitePool::connect_with(opts).await.unwrap();
            sqlx::query("CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName TEXT)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE items (itemID INTEGER PRIMARY KEY, key TEXT, itemTypeID INTEGER, dateAdded TEXT, dateModified TEXT)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE fields (fieldID INTEGER PRIMARY KEY, fieldName TEXT)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE itemData (itemID INTEGER, fieldID INTEGER, valueID INTEGER)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, value TEXT)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, firstName TEXT, lastName TEXT, name TEXT)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE itemCreators (itemID INTEGER, creatorID INTEGER)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE deletedItems (itemID INTEGER)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE fulltextItems (itemID INTEGER, content TEXT, indexedChars INTEGER, totalChars INTEGER, version INTEGER)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE itemNotes (itemID INTEGER, parentItemID INTEGER, note TEXT, title TEXT)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE itemAnnotations (itemID INTEGER, parentItemID INTEGER, text TEXT, comment TEXT, type INTEGER, color TEXT, pageLabel TEXT)").execute(&pool).await.unwrap();
            sqlx::query("CREATE TABLE itemAttachments (itemID INTEGER, parentItemID INTEGER, path TEXT, contentType TEXT)").execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO fields (fieldID, fieldName) VALUES (1, 'title'), (16, 'extra'), (7, 'DOI')").execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO itemTypes (itemTypeID, typeName) VALUES (1, 'journalArticle'), (2, 'note')").execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO items (itemID, key, itemTypeID, dateAdded, dateModified) VALUES (1, 'K00001', 1, '2024-01-01', '2024-02-01')").execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO itemData (itemID, fieldID, valueID) VALUES (1, 1, 100), (1, 7, 101)").execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO itemDataValues (valueID, value) VALUES (100, 'Rust in Action'), (101, '10.1000/rust')").execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO fulltextItems (itemID, content) VALUES (1, 'The borrow checker ensures memory safety.')").execute(&pool).await.unwrap();
            pool.close().await;
        }

        #[tokio::test]
        async fn fulltext_tool_returns_gate_error_when_disabled() {
            let state = zotero_state(String::new());
            let server = ZoteroMcpServer::new(state.clone());
            let res = server
                .zotero_fulltext_search_impl(FulltextSearchArgs {
                    query: "borrow".to_owned(),
                    limit: Some(10),
                })
                .await
                .unwrap();
            let text = tool_text(&res);
            assert!(text.contains("ZOTERO_SQLITE_ACCESS"));
        }

        #[tokio::test]
        async fn fulltext_tool_returns_hits_when_enabled() {
            let dir = tempfile::tempdir().unwrap();
            let db_path = dir.path().join("zotero.sqlite");
            seed_db(&db_path).await;

            let mut state = zotero_state(String::new());
            state.sqlite_access = true;
            std::env::set_var("ZOTERO_DB_PATH", &db_path);
            let server = ZoteroMcpServer::new(state);
            let res = server
                .zotero_fulltext_search_impl(FulltextSearchArgs {
                    query: "borrow checker".to_owned(),
                    limit: Some(10),
                })
                .await
                .unwrap();
            let text = tool_text(&res);
            assert!(text.contains("Rust in Action"));
        }
    }
```

Note: `zotero_fulltext_search_impl` reads `ZOTERO_DB_PATH` from the env (see Step 3). `zotero_state` currently sets `..AppState::from_env()` which runs at construction — so `set_var` must happen before constructing `AppState` **or** the impl must read the env at call time. Implement the impl to read env at call time (Step 3 does this), so `set_var` ordering is safe.

- [ ] **Step 2: Run test to verify it fails**

Run: `mise run test src::mcp::zotero::tests::sqlite_tools`
Expected: FAIL — impls / arg structs don't exist.

- [ ] **Step 3: Write minimal implementation**

**`src/mcp/zotero.rs`** — add arg structs near `AdvancedSearchArgs`:

```rust
/// Arguments for `zotero_fulltext_search`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FulltextSearchArgs {
    /// Free-text query matched against title, creators, DOI, and indexed
    /// fulltext.
    pub(crate) query: String,
    /// Maximum number of results to return (default: 20).
    pub(crate) limit: Option<usize>,
}

/// Arguments for `zotero_search_notes_annotations`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchNotesAnnotationsArgs {
    /// Free-text query matched against note body and annotation text/comment.
    pub(crate) query: String,
    /// Maximum number of results to return (default: 20).
    pub(crate) limit: Option<usize>,
}
```

And the two impls (place after `zotero_advanced_search_impl`):

```rust
    /// Handles local full-text search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_fulltext_search_impl(
        &self,
        args: FulltextSearchArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let state = &self.state;
        let result = async {
            state.check_sqlite_access()?;
            let Some(db_path) = crate::zotero::local_db::find_zotero_db()
            else {
                return Err(ZoteroMcpError::LocalDb(
                    "Zotero sqlite database not found".to_owned(),
                ));
            };
            let db = LocalZoteroDb::open(&db_path).await?;
            db.search_fulltext(&args.query, limit).await
        }
        .await;
        Ok(super::json_result(result))
    }

    /// Handles local note/annotation search tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_notes_annotations_impl(
        &self,
        args: SearchNotesAnnotationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(20);
        let state = &self.state;
        let result = async {
            state.check_sqlite_access()?;
            let Some(db_path) = crate::zotero::local_db::find_zotero_db()
            else {
                return Err(ZoteroMcpError::LocalDb(
                    "Zotero sqlite database not found".to_owned(),
                ));
            };
            let db = LocalZoteroDb::open(&db_path).await?;
            db.search_notes_annotations(&args.query, limit).await
        }
        .await;
        Ok(super::json_result(result))
    }
```

**`src/mcp/server.rs`** — add after the `zotero_advanced_search` tool (after line 628):

```rust
    #[tool(
        name = "zotero_fulltext_search",
        description = "Search Zotero's local sqlite database for full-text \
                       matches across titles, creators, and indexed PDF text \
                       (requires ZOTERO_SQLITE_ACCESS=1)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_fulltext_search(
        &self,
        Parameters(args): Parameters<FulltextSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_fulltext_search_impl(args).await
    }

    #[tool(
        name = "zotero_search_notes_annotations",
        description = "Search Zotero's local sqlite database for note and PDF \
                       annotation text (requires ZOTERO_SQLITE_ACCESS=1)"
    )]
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_search_notes_annotations(
        &self,
        Parameters(args): Parameters<SearchNotesAnnotationsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.zotero_search_notes_annotations_impl(args).await
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `mise run check && mise run test && mise run clippy`
Expected: PASS.

- [ ] **Step 5: Verify tool registration count is 56**

Run: `rg -c 'name = "zotero_|name = "better_bibtex_|name = "better_notes_|name = "search|name = "fetch' src/mcp/server.rs`
Expected: 56 (39 `zotero_*` + 8 `better_bibtex_*` + 5 `better_notes_*` + `search` + `fetch` + 2 new).

- [ ] **Step 6: Commit**

```bash
git add src/mcp/zotero.rs src/mcp/server.rs
git commit -m "feat: add zotero_fulltext_search and zotero_search_notes_annotations tools"
```

---

## Task 9: Update documentation

**Files:**
- Modify: `docs/zotero-mcp-comparison.md`
- Modify: `README.md` (only if it documents the tool count or search behavior — verify first with `rg -n '54|tools|Search|search' README.md`)

- [ ] **Step 1: Write the failing doc assertion (test the doc)**
  No test needed — this is documentation. Verify current state:
  Run: `rg -n '54|100|Search|library-wide' docs/zotero-mcp-comparison.md`
  Expected: the current "54 tools" line 13, "rows 15 & 30" gap markers, and any "capped at 100 items" text.

- [ ] **Step 2: Make the doc changes**

In `docs/zotero-mcp-comparison.md`:
1. Line 13 `# tools` count: `54` → `56`.
2. Row 15 (`zotero_search_items` / Search): change the ⚠️ status to ✅ (now paginated with `start`; whole-library advanced search).
3. Row 30 (`zotero_library_coverage`): change the ⚠️/❌ status to ✅ (now scans the whole library, not 100 items).
4. The profile paragraph near line 40 that says library-wide scans are capped at 100 items: update to reflect full-library pagination.
5. Gap analysis section (lines ~60-69): mark the two addressed gaps as closed and note the two new sqlite tools.

- [ ] **Step 3: Verify the docs build / no dead references**

Run: `rg -n '54 tools|100 items|capped' docs/ README.md`
Expected: no stale "54 tools" or "capped at 100" claims remain (the sqlite schema cap `FULLTEXT_SCAN_CAP` is internal, not a tool cap).

- [ ] **Step 4: Commit**

```bash
git add docs/zotero-mcp-comparison.md README.md
git commit -m "docs: update tool count and search/coverage gaps"
```

---

## Task 10: Final verification pass

**Files:**
- All modified files

- [ ] **Step 1: Full workspace check**

Run: `mise run check && mise run test && mise run clippy && mise run lint && mise run fmt -- --check`
Expected: all green.

- [ ] **Step 2: Detect changes / regression review**

Run: `git diff --stat` and review each hunk against the call-site table at the top of this plan. Confirm no caller of `search_items`/`advanced_search`/`get_recent_items` was missed (grep again):

Run: `rg -n 'search_items\(|advanced_search\(|get_recent_items\(' src/`
Expected: every remaining call site uses the new signatures (0 old-signature calls).

- [ ] **Step 3: Manual smoke test (optional, requires running Zotero)**

If Zotero is running locally, start the server (`mise run run`) and call `zotero_search_items` with a query and `start: 50`, then `zotero_advanced_search` with a title condition + `sortBy: "title"`, then (with `ZOTERO_SQLITE_ACCESS=1`) `zotero_fulltext_search`. Verify the `pagination` object is present in responses.

- [ ] **Step 4: Commit any stragglers**

```bash
git add -A
git commit -m "chore: final verification pass"
```

---

## Self-Review

**1. Spec coverage**
- Gap row 15 (server-side advanced search): ✅ Tasks 1-4 (whole-library scan via `get_all_json`, pushdown, paginated `{items, pagination}`).
- Gap row 30 (library coverage capped at 100): ✅ Task 2 routes `get_library_coverage` and `find_duplicates` through `get_all_items`.
- Richer operators (`IsNot`, `DoesNotContain`, range, before/after): ✅ Task 3.
- Join mode + sort: ✅ Tasks 3-4.
- Skip junk item types: ✅ Task 4 (`is_searchable_item`).
- Full-text + notes/annotations search (54yyyu parity): ✅ Tasks 7-8 (sqlite layer + 2 tools, gated).
- Docs (tool count 54→56, gap status): ✅ Task 9.

**2. Placeholder scan** — every code step carries complete code; no "TBD"/"add error handling" strings. The only "see Task N" references are to identical fixture code that Tasks 2/4/5 tell the engineer to copy locally (deliberate, because `#[cfg(test)]` fixtures are module-private).

**3. Type consistency**
- `SearchPage<T>`/`PaginationInfo` fields (`limit`, `offset`, `total`, `has_more`) are identical across Tasks 4 and their tests.
- `advanced_search` signature `(conditions, join_mode, sort, sort_direction, offset, limit)` is used identically at all three call sites (search.rs test, mcp/zotero.rs:970, mcp/zotero.rs:1076) and in `AdvancedSearchArgs`.
- `search_items(query, collection_key, offset, limit)` used at search.rs:137 and mcp/zotero.rs:442 consistently.
- `AppState::sqlite_access` added in Task 6; all three `AppState` literal constructors (`state.rs:354`, `client.rs:242`) are updated there; `zotero_state` uses `..from_env()` so it needs no edit.
- `ZoteroMcpError::LocalDb(String)` added in Task 6 and used in Tasks 7-8.
- `FulltextHit`/`NoteAnnotationHit` field names match between `local_db.rs` (Task 7), the impls (Task 8), and the doc claims.

**Known deliberate simplifications (ponytail):**
- `get_all_json` relies on Zotero honoring `start`/`limit` (it does) and short-page termination; no hard iteration cap.
- `search_items` falls back to `offset + items.len()` as total when the `Total-Results` header is absent, so pagination never reports a bogus 0.
- The sqlite full-text scan is capped at 2000 candidate rows and filters in Rust (like 54yyyu); a real indexed library with huge fulltext may under-report. Upgrade path: push the `LIKE` into SQL and add a `WHERE fulltextItems.content LIKE` clause if that ever matters.
- `zotero_add_by_identifier_impl` dedup check switches to the slow path by construction (its `Is` title condition IS pushable, so it uses the fast path — verified `operator_pushable` includes `Is`).
- `LocalZoteroDb::open` uses `immutable(true)`; if Zotero is actively writing, reads may be stale but never error (matches the digest's `immutable=1`).

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-01-close-search-pagination-gap.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
