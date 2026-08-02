//! Attachment item operations for the Zotero Local HTTP API.

use crate::{
    errors::ZoteroMcpError,
    zotero::{ItemKey, ItemType, LinkMode, ZoteroItem, client::ZoteroClient},
};

impl ZoteroClient<'_> {
    /// Attaches a linked file to a parent item.
    ///
    /// # Arguments
    ///
    /// * `parent_item_key` - Key of the parent item to attach to
    /// * `title` - Title for the attachment
    /// * `file_path_or_url` - File path or URL to link
    /// * `content_type` - Optional MIME content type (defaults to
    ///   `"application/pdf"`)
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn attach_file_link(
        &self,
        parent_item_key: &ItemKey,
        title: &str,
        file_path_or_url: &str,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": ItemType::Attachment,
            "parentItem": parent_item_key,
            "title": title,
            "linkMode": LinkMode::ImportedFile,
            "path": file_path_or_url,
            "contentType": content_type.unwrap_or("application/pdf"),
        }]);

        self.post_json_first(
            &url,
            &payload,
            "Created attachment array was empty",
        )
        .await
    }
}
