//! Note item operations for the Zotero Local HTTP API.
//!
//! No types defined — functionality is exposed via [`ZoteroClient`] methods.

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
