//! Model Context Protocol server exposing a Zotero library over stdio.
//!
//! Wires the [`ZoteroMcpServer`] tool router to three backends: the Zotero
//! Local HTTP API, the Better `BibTeX` JSON-RPC API, and the Better Notes
//! companion bridge. Communicates with MCP clients over standard input and
//! output (stdio) using JSON-RPC ([`rmcp::transport::stdio`]); all diagnostic
//! logging is routed to standard error so it never corrupts the stdio
//! protocol stream.
//!
//! # Examples
//!
//! ```no_run
//! use zotero_mcp_rs::{mcp::ZoteroMcpServer, state::AppState};
//!
//! let state = AppState::from_env();
//! let server = ZoteroMcpServer::new(state);
//! ```

mod better_bibtex;
mod better_notes;
mod errors;
#[macro_use]
mod macros;
mod mcp;
mod pdf;
mod security;
mod semantic_search;
mod state;
mod zotero;

use mcp::ZoteroMcpServer;
use rmcp::ServiceExt;
use state::AppState;
use tracing_subscriber::EnvFilter;

/// Runs the Zotero MCP server binary.
///
/// Initializes the [`tracing`] subscriber to output strictly to standard error,
/// constructs the shared [`AppState`], builds the [`ZoteroMcpServer`], and
/// connects to MCP clients over stdio.
///
/// # Errors
///
/// - If the server fails to serve over the stdio transport.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        "Starting zotero-mcp-rs server (write_enabled={})",
        state.write_enabled
    );

    let server = ZoteroMcpServer::new(state);
    let transport = rmcp::transport::stdio();

    server.serve(transport).await?.waiting().await?;

    Ok(())
}
