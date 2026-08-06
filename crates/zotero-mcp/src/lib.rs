//! Model Context Protocol server exposing a Zotero library over stdio.
//!
//! Wires the [`ZoteroMcpServer`] tool router to three backends: the Zotero
//! Local HTTP API, the Better `BibTeX` JSON-RPC API, and the Better Notes
//! companion bridge. Communicates with MCP clients over standard input and
//! output (stdio) using JSON-RPC ([`rmcp::transport::stdio`]); all diagnostic
//! logging is routed to standard error so it never corrupts the stdio
//! protocol stream.
//!
//! `main.rs` is a thin wrapper that calls [`run`]; all server logic lives
//! here.

mod better_bibtex;
mod better_notes;
mod catalog;
mod resources;
mod response;
mod semantic_search;
mod server;
mod zotero;

use rmcp::ServiceExt;
use server::ZoteroMcpServer;
use tracing_subscriber::EnvFilter;
use zotero_api::AppState;

/// Runs the Zotero MCP server binary.
///
/// Initializes the [`tracing`] subscriber to output strictly to standard error,
/// constructs the shared [`AppState`], builds the [`ZoteroMcpServer`], and
/// connects to MCP clients over stdio.
///
/// # Errors
///
/// - If the server fails to serve over the stdio transport.
#[inline]
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Tracing MUST output strictly to standard error so stdio JSON-RPC stream
    // is clean
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let state = AppState::from_env();
    tracing::info!(
        "Starting zotero-mcp server (write_enabled={})",
        state.is_write_enabled()
    );

    let server = ZoteroMcpServer::new(state);
    let transport = rmcp::transport::stdio();

    server.serve(transport).await?.waiting().await?;

    Ok(())
}
