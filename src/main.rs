//! Model Context Protocol server exposing a Zotero library over stdio.
//!
//! Wires the [`ZoteroMcpServer`] tool router to three backends: the Zotero
//! Local HTTP API, the Better `BibTeX` JSON-RPC API, and the Better Notes
//! companion bridge. Communicates with MCP clients over stdio using JSON-RPC
//! ([`rmcp::transport::stdio`]); all diagnostic logging is routed to stderr
//! so it never corrupts the stdio protocol stream.

mod better_bibtex;
mod better_notes;
mod errors;
mod mcp;
mod pdf;
mod state;
mod zotero;

use mcp::ZoteroMcpServer;
use rmcp::ServiceExt;
use state::AppState;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tracing MUST output strictly to stderr so stdio JSON-RPC stream is clean
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let state = AppState::from_env();
    tracing::info!(
        "Starting zotero-mcp-rs server (write_enabled={})",
        state.write_enabled
    );

    let server = ZoteroMcpServer::new(state);
    let transport = rmcp::transport::stdio();

    server.serve(transport).await?;

    Ok(())
}
