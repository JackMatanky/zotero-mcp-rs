//! MCP tool handlers and argument models for core Zotero Local API
//! operations.
//!
//! Each sibling module owns one grouped-router domain exposed to MCP
//! clients:
//! - `items`: `zotero_items` / `zotero_items_write`
//! - `collections`: `zotero_collections` / `zotero_collections_write`
//! - `notes`: `zotero_notes` / `zotero_notes_write`
//! - `tags`: `zotero_tags` / `zotero_tags_write`
//! - `relations`: `zotero_relations` / `zotero_relations_write`
//! - `search`: `zotero_search`
//! - `sqlite`: `zotero_sqlite_search`
//! - `pdf`: `zotero_pdf`
//!
//! This module also hosts the standalone `zotero_status` tool and
//! re-exports every argument type for [`super::server`] and
//! [`super::connector_tools`].

mod collections;
mod items;
mod notes;
mod pdf;
mod relations;
mod search;
mod sqlite;
mod tags;
#[cfg(test)]
mod tests;

pub(crate) use collections::{
    CreateCollectionArgs, DeleteCollectionArgs, GetCollectionItemsArgs,
    GetUnfiledItemsArgs, ManageCollectionsArgs, SearchCollectionsArgs,
    UpdateCollectionArgs,
};
pub(crate) use items::{
    AddByIdentifierArgs, AttachFileArgs, DeleteItemArgs, GetItemArgs,
    GetItemChildrenArgs, GetItemFulltextArgs, GetItemMetadataArgs,
    GetRecentArgs, TrashItemArgs, UpdateItemArgs,
};
pub(crate) use notes::{
    CreateAnnotationArgs, CreateNoteArgs, GetNotesArgs,
    SynthesizeAnnotationsArgs,
};
pub(crate) use pdf::{GetPdfOutlineArgs, GetPdfPathArgs, ReadPdfPagesArgs};
pub(crate) use relations::{
    AddItemRelationArgs, GetRelatedItemsArgs, RemoveItemRelationArgs,
};
use rmcp::model::CallToolResult;
use schemars::JsonSchema;
pub(crate) use search::{
    AdvancedSearchArgs, FindDuplicatesArgs, LibraryCoverageArgs,
    SearchByCitationKeyArgs, SearchByTagArgs, SearchItemsArgs,
};
use serde::Deserialize;
pub(crate) use sqlite::{FulltextSearchArgs, SearchNotesAnnotationsArgs};
pub(crate) use tags::{
    BatchUpdateTagsArgs, DeleteTagsArgs, ListTagsArgs, RenameTagArgs,
};

use crate::{ZoteroMcpServer, mcp::json_success, zotero::ZoteroClient};

/// Arguments for tools that take no parameters.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct EmptyArgs {}

impl ZoteroMcpServer {
    /// Handles Zotero Local API status tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(crate) async fn zotero_status_impl(
        &self,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let status = client.check_status().await;
        Ok(json_success(&status))
    }
}
