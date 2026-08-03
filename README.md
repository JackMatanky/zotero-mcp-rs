# zotero-mcp-rs

A [Model Context Protocol](https://modelcontextprotocol.io) server for Zotero, written in Rust.

## Build & run

```bash
cargo run
```

## Configuration

Only grouped, `action`-dispatched router tools are exposed (e.g. `zotero_items`, `zotero_search`).

`ZOTERO_WRITE_ENABLED=1` exposes write tools (create/update/delete). `ZOTERO_SQLITE_ACCESS=1` exposes direct local-SQLite-database tools (`zotero_sqlite_search` and friends), distinct from the Zotero Local HTTP API used by `zotero_search`. `ZOTERO_SEMANTIC_SEARCH=1` exposes `zotero_semantic_search` (local embedding index + search over a side-car `SQLite` database, distinct from Zotero's own database). By default its index file lives in a per-user app data directory; set `ZOTERO_SEMANTIC_DB_PATH=/path/to/embeddings.sqlite` (absolute or relative to the server's working directory) to keep the vector database local to a specific project instead.

## Validate

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## License

MIT OR Apache-2.0
