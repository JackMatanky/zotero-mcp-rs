//! Zotero Local API full-text item content operations.
//!
//! This uses `/items/{itemKey}/fulltext`; direct `zotero.sqlite` full-text
//! search lives in [`crate::zotero::sqlite`].

use crate::{
    errors::ZoteroMcpError,
    zotero::{ItemKey, client::ZoteroClient},
};

impl ZoteroClient<'_> {
    /// Fetches Zotero's indexed fulltext content for `item_key`, returning an
    /// empty string if unindexed.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_item_fulltext(
        &self,
        item_key: &ItemKey,
    ) -> Result<String, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/fulltext",
            self.state.zotero_api_url, item_key
        );
        let val: serde_json::Value = self.get_json(&url).await?;
        let content = val
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        Ok(content)
    }
}
