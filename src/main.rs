mod better_bibtex;
mod better_notes;
mod errors;
mod pdf;
mod state;
mod tools;
mod zotero;

use rmcp::ServiceExt;
use state::AppState;
use tools::ZoteroMcpServer;
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
