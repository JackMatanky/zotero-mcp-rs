# zotero-mcp-rs

A [Model Context Protocol](https://modelcontextprotocol.io) server for Zotero, written in Rust.

## Build & run

```bash
cargo run
```

## Configuration

`ZOTERO_MCP_MODE` controls which MCP tools are advertised (default `compact`):

- `compact` — grouped, `action`-dispatched router tools only (e.g. `zotero_items`, `zotero_search`).
- `gated` — router tools plus every individual legacy tool (e.g. `zotero_get_item`), still hidden behind the gates below.
- `all` — every tool, including gated ones, regardless of the flags below.

`ZOTERO_WRITE_ENABLED=1` exposes write tools (create/update/delete). `ZOTERO_SQLITE_ACCESS=1` exposes direct local-SQLite-database tools (`zotero_sqlite_search` and friends), distinct from the Zotero Local HTTP API used by `zotero_search`.

## Validate

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## License

MIT OR Apache-2.0
