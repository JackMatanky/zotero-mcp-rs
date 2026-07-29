# Findings — Full Rust Zotero MCP with Better BibTeX + Better Notes

## Prior decision retained
- Better Notes exposes APIs at `Zotero.BetterNotes.api` for other Zotero plugin developers.
- API modules observed earlier: `workspace`, `sync`, `convert`, `template`, `$export`, `$import`, `editor`, `note`, `relation`, `utils`.
- The Better Notes API is in-process JavaScript inside Zotero; standalone Rust cannot call it directly.
- Source: https://github.com/windingwind/zotero-better-notes#-api and https://github.com/windingwind/zotero-better-notes/blob/master/src/api.ts

## Better BibTeX JSON-RPC
- Official docs: https://retorque.re/zotero-better-bibtex/exporting/json-rpc/
- Endpoint: `http://localhost:23119/better-bibtex/json-rpc`.
- Request format: JSON-RPC 2.0 over POST with JSON headers.
- Useful methods documented:
  - `api.ready()` → `{ betterbibtex, zotero }` versions.
  - `item.citationkey(item_keys | "selected")` → Zotero item keys to citekeys.
  - `item.export(citekeys, translator, libraryID?)` → export string.
  - `item.bibliography(citekeys, format?, library?)` → formatted bibliography.
  - `item.search(terms, library?)` → Zotero search.
  - `item.notes(citekeys)` → notes by citekey.
  - `item.attachments(citekey, library?)` → attachments by citekey.
  - `item.collections(citekeys, includeParents?)` → collections.
  - `item.pandoc_filter(...)` → pandoc zotero filter data.
  - `item.regenerate_key(...)` → mutating key regeneration.
  - `autoexport.add(...)` → mutating auto-export setup.
  - `collection.scanAUX(...)` → mutating AUX scan; clears target collection.
  - `user.groups(...)` → libraries/groups.
  - `viewer.viewPDF(...)` → opens Zotero PDF viewer.
- Important: Better BibTeX can be used directly from Rust; no companion plugin needed.

## Better BibTeX CAYW
- Official docs: https://retorque.re/zotero-better-bibtex/citing/cayw/
- Endpoint: `http://127.0.0.1:23119/better-bibtex/cayw`.
- Programmatic access works through tools like curl; browser access is restricted for security.
- Supports formats including `pandoc`, `natbib`, `biblatex`, `typst`, `json`, `formatted-citation`, and `formatted-bibliography`.
- Use only for explicit interactive picker tools; not autonomous research flows.

## Better BibTeX automatic export
- Official docs: https://retorque.re/zotero-better-bibtex/exporting/auto/
- Auto-export can keep a collection/library export updated.
- It is powerful but should be write-gated; avoid creating many auto-exports.

## Zotero Local API
- Docs: https://www.zotero.org/support/dev/web_api/v3/local_api
- Local API lives at `http://localhost:23119/api/` when enabled.
- Read requests require no authentication.
- Write requests require local authorization via `POST /api/local/authorize`.
- Useful for ordinary Zotero data, not Better Notes APIs.

## Rust crates checked

### rmcp
- Docs: https://docs.rs/rmcp/latest/rmcp/
- Official Rust SDK for Model Context Protocol.
- Supports server/client, stdio, streamable HTTP, macros, JSON schema features.
- Best fit for the MCP layer.

### papers-zotero
- Docs: https://docs.rs/papers-zotero/latest/papers_zotero/
- Async Rust client for Zotero Web API v3.
- Supports items, collections, tags, searches, groups, fulltext, and writes.
- Has local/base URL helpers, but direct `reqwest` may still be simpler for plugin endpoints and local auth.

### pdf-extract
- Docs: https://docs.rs/pdf-extract/latest/pdf_extract/
- Provides `extract_text_by_pages` and memory variants.
- Best lazy choice for `zotero_read_pdf_pages` before considering heavier PDF stacks.

### rusqlite/sqlx
- `rusqlite`: https://docs.rs/rusqlite/latest/rusqlite/
- `sqlx` SQLite: https://docs.rs/sqlx/latest/sqlx/sqlite/index.html
- Direct SQLite should be deferred; Zotero Local API avoids schema coupling and write safety hazards.

## Key decision
Build one Rust MCP server. Use direct HTTP clients for Zotero Local API and Better BibTeX. Add one small Zotero companion plugin only for Better Notes, because Better Notes has no standalone local HTTP API.
