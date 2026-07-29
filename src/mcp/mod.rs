//! Model Context Protocol (MCP) server & tool handlers.

mod better_bibtex;
mod better_notes;
mod chatgpt;
mod resources;
mod server;
mod zotero;

pub(crate) use server::ZoteroMcpServer;
