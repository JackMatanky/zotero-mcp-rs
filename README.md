# zotero-mcp-rs

A [Model Context Protocol](https://modelcontextprotocol.io) server for Zotero, written in Rust.

## Build & run

```bash
cargo run
```

## Configuration

Only grouped, `action`-dispatched router tools are exposed (e.g. `zotero_items`, `zotero_search`).

`ZOTERO_WRITE_ENABLED=1` exposes write tools (create/update/delete). `ZOTERO_SQLITE_ACCESS=1` exposes direct local-SQLite-database tools (`zotero_sqlite_search` and friends), distinct from the Zotero Local HTTP API used by `zotero_search`.

## Validate

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## License

MIT OR Apache-2.0
