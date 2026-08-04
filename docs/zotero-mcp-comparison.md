# Zotero MCP Server Comparison

Comparison of zotero-mcp-rs against popular Zotero MCP servers (research, 2026-07).

**Legend:** ✅ full support · ⚠️ partial/limited · ❌ not available

**Primitive types** (MCP): **tools** = callable functions that read/write Zotero (counted per grouped router; each router dispatches 2–8 `action` subcommands); **resources** = parameterizable read-only `zotero://` URIs (attach as context); **prompts** = reusable slash-command templates.

### Platform & primitives

| | **zotero-mcp-rs** (this repo) | **54yyyu/zotero-mcp** | **cookjohn/zotero-mcp** | **introfini/ZotSeek** | **Xevos117/mcp-zotero** | **stephenstubbs/zotero-mcp** | **masaki39/zotero-mcp** |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Stars** | — | 4.5k | 1.1k | 175 | 33 | 4 | 3 |
| **Stack / arch** | Rust + rmcp, stdio | Python (FastMCP), PyPI | Zotero plugin + built-in MCP, TS | Zotero plugin (TS, Transformers.js) + built-in MCP | TypeScript, npx | Rust, stdio | Python (PyPI/uv) |
| **Transport** | stdio | stdio + Streamable HTTP + SSE | Streamable HTTP | Streamable HTTP (`:23119/zotseek/mcp`) | stdio | stdio | stdio |
| **Zotero connection** | Local HTTP API (`/users/0` only) + BBT/BetterNotes bridges | Local + Web + hybrid (local reads / web writes), group libs, WebDAV | In-process (plugin) | In-process (plugin), 100% local; group libs opt-in | Web API v3 (API key + user ID) | Own plugin HTTP API (23119/mcp) | Local HTTP API |
| **# Tools** | **61** (17 write-gated) | **~40** | 20 | search-focused, read-only | 15 | 8 | 5 |
| **Resources** | 3 static (`zotero://collections`, `zotero://items/recent`, `zotero://tags`) + 7 templates (`zotero://items/{key}` · `/fulltext` · `/children` · `/notes` · `/relations`; `zotero://collections/{key}` · `/items`) | 1 static + 2 templates (`zotero://items/{key}`, `zotero://collections/{key}/items`), Markdown | — | — (results carry `zotero://` deep links, not MCP resources) | — | — | — |
| **Prompts** | 1 (`zotero_literature_review`) | 4 (lit review, synthesize notes, contradicting evidence, expand from paper) | — | — | workflow `instructions` + Claude skill (not MCP prompts) | — | — |
| **Extra tooling** | BBT/BetterNotes bridges, `zotero_status`, analytics, `zotero_discover` | standalone `zotero-cli`, Docker, setup/update cmds, 294 tests | client config generator, fulltext DB stats | auto-index, find-similar, PDF-selection search, status column, crash-resilient | — | `/read` slash command, color scheme | — |

### Search & reading

| | **zotero-mcp-rs** | **54yyyu** | **cookjohn** | **ZotSeek** | **Xevos117** | **stephenstubbs** | **masaki39** |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Search** | quicksearch, tag, citekey; advanced whole-library, operators/join/sort, paginated `{items, pagination}` | ✅ keyword, advanced multi-criteria, tags, citekey, notes/annotations | ✅ boolean, filters, relevance, offsets | ✅ hybrid semantic+keyword (RRF), multi-query AND/OR, section-aware, passage previews | ✅ by query or sorted field | citekey lookup only | author/title only, 30 items |
| **Semantic search** | ✅ **local-only** ONNX (fastembed `bge-small-en-v1.5`, 384-d), sidecar SQLite, chunked title+abstract+fulltext, cosine; `search`/`index`/`status` — gated `ZOTERO_SEMANTIC_SEARCH` | ✅ ChromaDB + sentence-transformers / OpenAI / Gemini / Ollama, auto-update | ✅ OpenAI/Ollama + SQLite-vec | ✅ **100% local** (nomic-embed-text-v1.5, 4 curated models, WebGPU-ready) | ❌ | ❌ | ❌ |
| **Full-text read** | ✅ Zotero index (`get_item_fulltext`) | ✅ fulltext + direct PDF processing | ✅ cached fulltext DB (list/search/get/stats) | ⚠️ semantic chunks w/ page numbers; no raw fulltext endpoint | ✅ `get_item_fulltext` | ✅ MuPDF by pages or sections | ✅ `read_pdf` (per page, multi-attachment) |
| **PDF outline / bookmarks** | ✅ `zotero_pdf` → `outline` | ✅ `get_pdf_outline` (PyMuPDF extra) | ❌ | ❌ | ❌ | ✅ `get_pdf_outline` | ❌ |
| **Annotations read** | ✅ synthesize to Markdown | ✅ direct PDF extraction (pdfannots), search, image anns, layout detection | ✅ search by color/tags/type | ❌ | ❌ | — | ❌ |

### Writing & library management

| | **zotero-mcp-rs** | **54yyyu** | **cookjohn** | **ZotSeek** | **Xevos117** | **stephenstubbs** | **masaki39** |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Annotations write** | ✅ highlight/underline/note | ⚠️ create/update notes + annotations; `create_note` (beta) | ⚠️ via plugin UI only | ❌ | ❌ | ✅ highlight + area, semantic colors | ❌ |
| **Notes** | ✅ read + create + BetterNotes (export/template/relations/tree) | ✅ get/search/create notes | ✅ create/update/append, MD→HTML | ❌ | ❌ | ❌ | ❌ |
| **Tags / metadata** | ✅ batch tags, update item, rename/delete tags | ✅ update item, batch-update tags | ✅ write_tag, write_metadata | ❌ | ⚠️ metadata via add/edit only | ❌ | ❌ |
| **Create items** | ✅ by DOI/arXiv/ISBN | ✅ by DOI/URL/ISBN/BibTeX/CSL-JSON/file, OA PDF cascade, idempotent `if_exists` | ✅ create + reparent | ❌ | ✅ all 37 types, batch, by DOI | ❌ | ✅ by DOI (dedupe) |
| **Collections** | ✅ CRUD + manage items | ✅ create/search/manage (+ names & `parent/child` paths) | ✅ browse/search/subcollections | ⚠️ save results via plugin UI (not MCP) | ✅ list/create/delete + items | ❌ | ❌ |
| **Delete / trash** | ✅ delete, trash, restore | ⚠️ none exposed (only find/merge) | ⚠️ | ❌ | ✅ delete_items/collection — gated by `UNSAFE_OPERATIONS` (default **none**) | ❌ | ❌ |
| **PDF import / upload** | ⚠️ linked attach from local path only, no upload | ✅ add_from_file (PDF/EPUB, auto-DOI), attach local/URL | ❌ | ❌ | ✅ import_pdf_to_zotero (download+upload+index), Unpaywall OA | ❌ | ❌ |
| **DOI / identifiers** | ✅ DOI + arXiv + ISBN (Crossref/S2/OpenLibrary) | ✅ DOI + URL + ISBN (Open Library+Google Books) + BibTeX/CSL-JSON | ❌ | ❌ | ✅ DOI + Unpaywall OA | ❌ | ✅ DOI (CrossRef + confirm) |
| **BibTeX / citations** | ✅ BBT: citekeys, export, bibliography, pandoc, scanAUX, autoexport | ✅ BBT citekeys, BibTeX/markdown/JSON export, BBT annotation access | ❌ | ❌ | ❌ | ⚠️ BBT citekey lookup only | ❌ |
| **.docx citation injection** | ❌ | ❌ | ❌ | ❌ | ✅ inject_citations (5 styles) | ❌ | ❌ |
| **Duplicates / analytics** | ✅ find_duplicates, library_coverage (whole-library scan) | ✅ find **+ merge** (consolidates children, dry-run) | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Citation intelligence** | ❌ | ✅ Scite tallies + retraction alerts (optional extra) | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Related-item relations** | ✅ get/add/remove (dc:relation) | ✅ get/add/remove (dc:relation, owl:sameAs) | ❌ | ❌ | ❌ | ❌ | ❌ |

### Safety

| | **zotero-mcp-rs** | **54yyyu** | **cookjohn** | **ZotSeek** | **Xevos117** | **stephenstubbs** | **masaki39** |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Write safety default** | ✅ **read-only** until `ZOTERO_WRITE_ENABLED=1`; path allowlist profiles | ⚠️ local mode = read-only; writes need API key (hybrid) | ⚠️ write tools can be disabled in prefs | ✅ MCP opt-in & read-only; plugin UI can create collections | ✅ no deletions unless `UNSAFE_OPERATIONS` set | ✅ annotations only | ⚠️ add-by-DOI always on |
| **Security model** | loopback-only + canonicalize-prefix path checks, size caps, 4 profiles | API key (web/hybrid), local read-only, WebDAV creds | local-only plugin | 100% local/offline, zero network for search/index, no API keys | API key auth; deletions opt-in | loopback plugin | local API, no key |

## Server profiles

### zotero-mcp-rs (this repo)
Rust MCP server (rmcp, 61 tools / 17 write-gated, 3 resources + 7 resource templates, 1 prompt) speaking to Zotero's **Local HTTP API** with optional Better BibTeX (JSON-RPC), Better Notes (companion bridge), and local semantic search (fastembed ONNX, sidecar SQLite) integrations. Read-only by default (`ZOTERO_WRITE_ENABLED=1` opts into write tools); hardened path allowlists (4 security profiles), response size caps, and symlink-safe canonicalization. Strengths: write breadth (tags, notes, collections, annotations, item lifecycle, delete/trash/restore), BBT citation toolchain (citekeys, bibliography, pandoc filter, scanAUX, autoexport), identifier ingestion (DOI/arXiv/ISBN), whole-library advanced search + duplicates and library-coverage analytics, local sqlite full-text + note/annotation search, and now a fully-local semantic search (search/index/status, gated by `ZOTERO_SEMANTIC_SEARCH`). Limitations: local user library only (no web API/groups), no PDF upload, no `.docx` injection.

### [54yyyu/zotero-mcp](https://github.com/54yyyu/zotero-mcp) (4.5k★) — closest competitor
Python FastMCP server, installable via PyPI (`zotero-mcp-server`). Most feature-complete of the set: local/web/hybrid modes, semantic search (ChromaDB, multiple embedding backends), duplicate find **+ merge**, PDF import with auto-DOI, five add-by-identifier paths, Scite citation intelligence, item relations, PDF outline, page-layout detection, standalone CLI + Docker. The reference for zotero-mcp-rs's biggest feature gaps.

### [cookjohn/zotero-mcp](https://github.com/cookjohn/zotero-mcp) (1.1k★)
Zotero plugin with an **integrated MCP server** (Streamable HTTP) — no separate process. Strongest discovery UX: boolean/relevance-scored search, semantic search (SQLite-vec), cached fulltext database, annotation search by color/tag. Write scope is narrower (4 write tools). Requires installing the `.xpi` plugin and enabling the server in Zotero preferences.

### [introfini/ZotSeek](https://github.com/introfini/ZotSeek) (175★)
Zotero plugin (Zotero 8/9) with **semantic search + built-in read-only MCP server**. 100% local: Transformers.js runs the bundled nomic-embed-text-v1.5 (4 curated models) in a ChromeWorker, embeddings stored in `zotseek.sqlite`, zero network traffic. Search is hybrid (semantic + keyword via RRF), section-aware, multi-query AND/OR, with passage-level results that deep-link into the exact PDF page. MCP is opt-in and strictly read-only; the plugin UI additionally does find-similar, PDF-selection search, auto-indexing, and save-results-as-collection. Not a general library tool — search only, no write/metadata/citation tools.

### [Xevos117/mcp-zotero](https://github.com/Xevos117/mcp-zotero) (33★)
TypeScript server targeting the **Zotero Web API** (API key + user ID) — the only one that works against remote libraries. Standouts: PDF import with fulltext indexing, Unpaywall OA discovery, `inject_citations` for `.docx` (5 styles), deletions gated behind `UNSAFE_OPERATIONS` (default none).

### [stephenstubbs/zotero-mcp](https://github.com/stephenstubbs/zotero-mcp) (4★)
Rust server + Zotero plugin exposing a custom HTTP API (23119/mcp). Narrow scope: critical reading — citekey lookup, PDF outline, page/section reading (MuPDF), highlight + area annotations with a semantic color scheme. Effectively a subset of zotero-mcp-rs.

### [masaki39/zotero-mcp](https://github.com/masaki39/zotero-mcp) (3★)
Minimal Python server (5 tools): search, get item, read PDF, confirm DOI, add by DOI. Simplest setup (`uvx masaki39-zotero-mcp@latest`). A strict subset of zotero-mcp-rs.

## Gap analysis: zotero-mcp-rs vs. the field

Closed gaps first, then open ones ranked by value. Each open gap names which competitors lead and the shape of a fix.

### Closed

1. ~~**Whole-library advanced search / pagination**~~ — server-side pushdown (no 100-item ceiling), operators/join/sort, paginated `{items, pagination}` results, plus two gated sqlite-backed actions `zotero_sqlite_search` → `fulltext` / `notes_annotations` for full-text + note/annotation search.
2. ~~**Semantic search**~~ — local-only ONNX embeddings (fastembed `bge-small-en-v1.5`, 384-d) in a sidecar SQLite index, chunked title+abstract+fulltext, brute-force cosine, gated by `ZOTERO_SEMANTIC_SEARCH`. Functional, but the minimal variant: single fixed model (no OpenAI/Gemini/Ollama backends like 54yyyu), no hybrid keyword+RFF fusion or multi-query AND/OR (ZotSeek), no reranker/auto-update.

### Open

1. **PDF import / upload** — `zotero_attach_file` links an existing path/URL only; no multipart upload or download-and-import. Competitors: 54yyyu `add_from_file` (PDF/EPUB, auto-DOI, local/URL attach), Xevos117 `import_pdf_to_zotero` (download → storage → fulltext index, Unpaywall OA). Highest-value gap: closes the "add a paper I have on disk" workflow end-to-end.
2. **Web API / group library support** — everything is hardcoded to local `users/0`, so no remote libraries, no group libraries, no API-key auth. Competitors: Xevos117 (Web API v3 only), 54yyyu (local/web/hybrid + groups + WebDAV). Bigger than a feature: changes the connection model, auth, and the security story.
3. **Duplicate merging** (beyond detection) — 54yyyu consolidates children with a dry-run preview and moves the loser to Trash (not permanent). zotero-mcp-rs detects; merging needs the consolidating logic + the existing delete/trash tools.
4. **`.docx` citation injection** — exclusive to Xevos117 (`inject_citations`, 5 styles, real Zotero field codes). The only major capability no competitor of zotero-mcp-rs shares; low overlap with the BBT/citation toolchain already present.

### Where zotero-mcp-rs leads (no gap)

- **Write breadth**: only server with delete/trash/restore + full collection CRUD + annotation writes. 54yyyu deliberately exposes no deletes; cookjohn and ZotSeek are read-only or plugin-gated.
- **BBT citation toolchain**: pandoc filter, scanAUX, autoexport go beyond any competitor's citekey/export support.
- **Safety defaults**: read-only until opt-in, with path allowlists and size caps — the strictest write gate of the set.
