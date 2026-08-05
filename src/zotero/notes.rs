//! Note item operations for the Zotero Local HTTP API.
//!
//! Provides methods on [`ZoteroClient`] for creating child note items attached
//! to library items. This module is called by note MCP tool handlers in
//! `crate::mcp::zotero`.
//!
//! No types defined; functionality is exposed via [`ZoteroClient`] methods.
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::errors::ZoteroMcpError;
//! # use zotero_mcp_rs::state::AppState;
//! # use zotero_mcp_rs::zotero::{ItemKey, ZoteroClient};
//! # async fn example() -> Result<(), ZoteroMcpError> {
//! let state = AppState::from_env();
//! let client = ZoteroClient::new(&state);
//! let parent_key = ItemKey::from("PARENT01");
//! let note = client.create_note(&parent_key, "<p>Meeting notes</p>").await?;
//! println!("Created note item: {}", note.key);
//! # Ok(())
//! # }
//! ```

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
    /// - [`ZoteroMcpError::PermissionDenied`] if write operations are disabled
    ///   in application state
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code
    /// - [`ZoteroMcpError::Network`] if the HTTP request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::{
        state::AppState,
        zotero::{
            client::ZoteroClient,
            test_http::{MockServer, http_response, request_body},
        },
    };

    fn state(zotero_api_url: impl AsRef<str>, write_enabled: bool) -> AppState {
        AppState {
            zotero_api_url: zotero_api_url.as_ref().to_owned(),
            write_enabled,
            ..AppState::from_env()
        }
    }

    #[tokio::test]
    async fn posts_note_payload_for_parent_item() {
        let response = json!([{"key":"NOTE0001","version":1,"data":{"key":"NOTE0001","version":1,"itemType":"note"}}]).to_string();
        let (server, recorded) =
            MockServer::recording(vec![http_response("200 OK", &response)]);
        let app = state(server.url(), true);

        let result = ZoteroClient::new(&app)
            .create_note(&ItemKey::from("PARENT01"), "<p>Note</p>")
            .await;

        assert!(result.is_ok(), "note creation should succeed: {result:?}");
        let requests = recorded.lock().expect("request log lock");
        let payload = requests
            .first()
            .and_then(|request| request_body(request).ok())
            .and_then(|body| {
                body.as_array().and_then(|array| array.first()).cloned()
            })
            .unwrap_or_default();
        assert_eq!(payload.get("itemType"), Some(&json!("note")));
        assert_eq!(payload.get("parentItem"), Some(&json!("PARENT01")));
        assert_eq!(payload.get("note"), Some(&json!("<p>Note</p>")));
    }

    #[tokio::test]
    async fn denies_writes_when_write_permission_is_disabled() {
        let app = state("http://127.0.0.1:1", false);

        let result = ZoteroClient::new(&app)
            .create_note(&ItemKey::from("PARENT01"), "<p>Note</p>")
            .await;

        assert!(
            matches!(result, Err(ZoteroMcpError::PermissionDenied(_))),
            "write-disabled note should fail before HTTP: {result:?}"
        );
    }
}
