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

    fn created_attachment() -> String {
        json!([{"key":"ATTACH01","version":1,"data":{"key":"ATTACH01","version":1,"itemType":"attachment"}}]).to_string()
    }

    async fn attach_and_body(content_type: Option<&str>) -> serde_json::Value {
        let (server, recorded) = MockServer::recording(vec![http_response(
            "200 OK",
            &created_attachment(),
        )]);
        let app = state(server.url(), true);
        let result = ZoteroClient::new(&app)
            .attach_file_link(
                &ItemKey::from("PARENT01"),
                "PDF",
                "/tmp/paper.pdf",
                content_type,
            )
            .await;
        assert!(
            result.is_ok(),
            "attachment creation should succeed: {result:?}"
        );
        let requests = recorded.lock().expect("request log lock");
        let body = requests
            .first()
            .and_then(|request| request_body(request).ok())
            .unwrap_or_default();
        body.as_array()
            .and_then(|array| array.first())
            .cloned()
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn uses_pdf_content_type_by_default() {
        let payload = attach_and_body(None).await;

        assert_eq!(payload.get("itemType"), Some(&json!("attachment")));
        assert_eq!(payload.get("parentItem"), Some(&json!("PARENT01")));
        assert_eq!(payload.get("linkMode"), Some(&json!("imported_file")));
        assert_eq!(payload.get("path"), Some(&json!("/tmp/paper.pdf")));
        assert_eq!(payload.get("contentType"), Some(&json!("application/pdf")));
    }

    #[tokio::test]
    async fn uses_explicit_content_type() {
        let payload = attach_and_body(Some("text/plain")).await;

        assert_eq!(payload.get("contentType"), Some(&json!("text/plain")));
    }

    #[tokio::test]
    async fn denies_writes_when_write_permission_is_disabled() {
        let app = state("http://127.0.0.1:1", false);

        let result = ZoteroClient::new(&app)
            .attach_file_link(
                &ItemKey::from("PARENT01"),
                "PDF",
                "/tmp/paper.pdf",
                None,
            )
            .await;

        assert!(
            matches!(result, Err(ZoteroMcpError::PermissionDenied(_))),
            "write-disabled attachment should fail before HTTP: {result:?}"
        );
    }
}
