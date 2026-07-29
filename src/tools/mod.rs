//! The MCP tool-router layer.
//!
//! Re-exports [`ZoteroMcpServer`], which wires every MCP tool to the
//! Zotero, Better `BibTeX`, and Better Notes clients.

mod models;
mod server;

pub(crate) use server::ZoteroMcpServer;
