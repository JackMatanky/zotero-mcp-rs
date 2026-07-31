# Related-Item Relations — TDD Design

Status: proposed
Date: 2026-07-31

## Context

Zotero items can be linked via "Related Items" (`dc:relation`), stored in each
item's `relations` map as arrays of item URIs:

```json
{
  "dc:relation": [
    "http://zotero.org/users/0/items/ITEM123",
    "http://zotero.org/groups/1/items/GROUP1"
  ]
}
```

The shared model already deserializes `relations` (`src/zotero/models.rs:484`)
but treats it as opaque `serde_json::Value`; no tool surfaces or mutates it.
This design adds read + bidirectional write support for `dc:relation` links,
mirroring the reference `54yyyu/zotero-mcp` server (`zotero_add_item_relation`
/ `zotero_remove_item_relation`).

Scope is deliberately `dc:relation` only. Other predicates (`owl:sameAs`,
`dc:replaces`) are preserved on write but not interpreted.

## Decision

Add a `RelationUri` newtype and a `src/zotero/relations.rs` module of pure
helpers plus `ZoteroClient` methods, and three MCP tools. Typed only at the
tool/module boundary — the shared `ZoteroItemData.relations` stays
`serde_json::Value` to keep the blast radius minimal (Approach A, chosen over
typing the model directly or inlining in `mcp/zotero.rs`).

## Components

### 1. `RelationUri` newtype (`src/zotero/models.rs`)

Created with the existing `string_key!` macro (gives `Display`, `AsRef<str>`,
`From<String>/From<&str>`, serde transparent). Base format constant:

```
http://zotero.org/users/0/items/{ITEM_KEY}
```

Traits:

- `impl From<&ItemKey> for RelationUri` — builds the base-form URI from an item key.
- `impl TryFrom<&RelationUri> for ItemKey` — extracts the trailing key segment.
  - `Ok(key)` for any URI whose last segment is a valid item key shape
    (`http://zotero.org/.../items/{KEY}`, including group-library URIs).
  - `Err(RelationUriError)` for anything that is not a Zotero item URI (e.g.
    a bare `"ITEM123"` string or a malformed URI).
  - Group URIs *succeed* here (they carry a key) and are later dropped in the
    read flow when `get_item` 404s (the `/users/0`-only client can't resolve
    group-library keys).
- `Error = RelationUriError` (unit struct, `Display` impl) so `TryFrom` works
  without leaking a full error type.

Note the distinction: **`ItemKey` is never a URI** — it is the bare 8-char
alphanumeric key everywhere in the codebase. URIs exist only inside
`relations` values. `RelationUri` is the bridge.

### 2. Pure helpers (`src/zotero/relations.rs`)

| Function | Signature | Behavior |
| --- | --- | --- |
| `parse_relation_keys` | `(&serde_json::Value) -> Vec<RelationUri>` | Reads `relations["dc:relation"]`, keeps all string entries as `RelationUri` (newtype is string-backed, so no shape filtering here — the real filter is `ItemKey::try_from` at consumption time). Ignores missing/empty/malformed (non-string) entries. |
| `apply_relations` | `(current: &serde_json::Value, add: &[RelationUri], remove: &[RelationUri]) -> serde_json::Value` | Set-based idempotent add/remove on the `dc:relation` array; preserves all other predicates (`owl:sameAs`, `dc:replaces`, ...). Returns `{"dc:relation": [...]}` merged into the input. |

Both are pure (no `&self`), unit-tested like `diff_tags`
(`src/zotero/tags.rs:144`).

### 3. Client methods (`ZoteroClient` in `src/zotero/relations.rs`)

| Method | Signature | Behavior |
| --- | --- | --- |
| `get_related_items` | `(&self, &ItemKey) -> Result<Vec<RelatedItem>, ZoteroMcpError>` | Fetches the item, parses `dc:relation` URIs, resolves each key via `get_item`, skipping keys that 404 (e.g. group-library items invisible to the `/users/0` client). |
| `add_item_relation` | `(&self, a: &ItemKey, b: &ItemKey) -> Result<(), ZoteroMcpError>` | Rejects `a == b` with `InputRejected`. Fetches A and B, patches **both** items' `dc:relation` with each other's URI (bidirectional). |
| `remove_item_relation` | `(&self, a: &ItemKey, b: &ItemKey) -> Result<(), ZoteroMcpError>` | Fetches A and B, patches **both** items' `dc:relation` removing each other's URI (bidirectional). |

Output struct `RelatedItem { key: ItemKey, title: Option<String>, item_type: ItemType }`
(Serialize).

### 4. MCP tools (`src/mcp/zotero.rs` + `src/mcp/server.rs`)

Args structs (each `#[derive(Deserialize, JsonSchema)]`, `ItemKey` fields):

- `GetRelatedItemsArgs { item_key }`
- `AddItemRelationArgs { item_key, related_item_key }`
- `RemoveItemRelationArgs { item_key, related_item_key }`

Impl fns delegate to client methods and wrap results via existing
`json_result`/`text_result` (`src/mcp/mod.rs:43`). Registered in `server.rs`
after `zotero_batch_update_tags` (~line 449).

Tool list (order of registration):

1. `zotero_get_related_items` — read
2. `zotero_add_item_relation` — write
3. `zotero_remove_item_relation` — write

Write tools call `check_write_permission()` (already in client methods, same
pattern as `update_item` at `src/zotero/items.rs:211`).

## Data flow

- **Read:** args.item_key → `get_item` → parse `dc:relation` URIs → per-key
  `get_item` → map to `RelatedItem { key, title, item_type }`.
- **Write (add/remove):** fetch A, fetch B → `apply_relations` on each →
  PATCH both with `{"relations": <merged>, "version": <item.version>}`.

Version preconditions are handled exactly like `batch_update_tags`
(`src/zotero/tags.rs:64-82`): read item, diff, PATCH with its `version`.

## Error handling

| Condition | Error |
| --- | --- |
| Write disabled | `PermissionDenied` (via `check_write_permission`) |
| Self-relation (`a == b`) | `InputRejected` |
| Item not found (get/read/write) | `NotFound` |
| Zotero non-2xx on PATCH/GET | `LocalApi` |
| Transport | `Network` |
| JSON decode | `Json` |

**Partial-write caveat (documented, accepted):** the two PATCHes in add/remove
are not transactional. If the second fails, the first already landed. Both
operations are idempotent (set-based), so a retry is safe. This mirrors the
existing per-item loop in `batch_update_tags`.

## Testing strategy (TDD)

All tests follow the repo's existing patterns: pure-helper unit tests in the
module, `mock_server` canned-response tool tests in `mcp/zotero.rs` tests mod.

### Red phase (write first)

1. `RelationUri` — `From<&ItemKey>` round-trips; `TryFrom` returns key for
   `http://zotero.org/users/0/items/{KEY}`, errors on bare `"ITEM123"` and
   malformed strings.
2. `parse_relation_keys` — extracts URI strings from a populated `dc:relation`
   array; returns empty for missing relations, empty array, or non-string
   entries (numbers/objects).
3. `apply_relations` — adds new URI (idempotent on re-add), removes existing
   URI, preserves other predicates, handles empty current value.

### Green/refactor phase (implement)

4. Client `get_related_items` — mock: item with `dc:relation` → related items
   fetched → titles mapped; unresolvable key skipped.
5. Client `add_item_relation` — mock 4 responses (GET A, GET B, PATCH A,
   PATCH B) → success; `a == b` → `InputRejected`.
6. Client `remove_item_relation` — mock 4 responses → success.

### Tool tests (`mcp/zotero.rs`)

7. `zotero_get_related_items_impl` — success shape (`is_error == false`).
8. `zotero_add_item_relation_impl` — success; write-disabled →
   `PermissionDenied` content.
9. `zotero_remove_item_relation_impl` — success.

## Files touched

| File | Change |
| --- | --- |
| `src/zotero/models.rs` | Add `RelationUri` string_key + `From`/`TryFrom` impls + error type |
| `src/zotero/relations.rs` | **New** — helpers, client methods, `RelatedItem`, unit tests |
| `src/mcp/zotero.rs` | Args structs, 3 impl fns, tool tests |
| `src/mcp/server.rs` | 3 `#[tool]` registrations after `zotero_batch_update_tags` |
| `docs/zotero-mcp-comparison.md` | `# tools` 51→54, add relations row |

No new dependencies.

## Consequences

- **Positive:** related-item graph becomes first-class for LLM agents; pure
  helpers keep the diff small and testable; `RelationUri` prevents
  key/URI transposition at call sites.
- **Negative:** two extra files; partial-write non-transactionality requires
  the idempotency caveat to be respected by callers.
- **Deferred (documented `ponytail:` note):** predicate enum + multi-predicate
  support (`owl:sameAs`, `dc:replaces`) — add when a concrete consumer needs
  them; group-library resolution — requires client support beyond `/users/0`.
