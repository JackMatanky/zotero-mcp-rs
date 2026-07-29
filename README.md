# zotero-mcp-rs

A [Model Context Protocol](https://modelcontextprotocol.io) server for Zotero, written in Rust.

## Build & run

```bash
cargo run
```

## Validate

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## License

MIT OR Apache-2.0
