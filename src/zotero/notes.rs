//! Note item operations for the Zotero Local HTTP API.

use crate::{
    errors::ZoteroMcpError,
    zotero::{ItemKey, ItemType, ZoteroItem, client::ZoteroClient},
};

impl ZoteroClient<'_> {
    /// Creates a note item attached to `parent_item_key` with body
    /// `note_content`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn create_note(
        &self,
        parent_item_key: &ItemKey,
        note_content: &str,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": ItemType::Note,
            "parentItem": parent_item_key,
            "note": note_content,
        }]);

        self.post_json_first(&url, &payload, "Created note array was empty")
            .await
    }
}
