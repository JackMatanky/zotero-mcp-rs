//! Binary entry point for the Zotero MCP server.
//!
//! All server logic lives in [`zotero_mcp::run`]; this binary is a thin
//! wrapper that starts the Tokio runtime and calls it.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    zotero_mcp::run().await
}
