//! Zotero Local API full-text item content operations.
//!
//! This uses `/items/{itemKey}/fulltext`; direct `zotero.sqlite` full-text
//! search lives in [`crate::zotero::sqlite`].
//!
//! No types defined — functionality is exposed via [`ZoteroClient`] methods.

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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        state::AppState,
        zotero::{
            client::ZoteroClient,
            test_http::{MockServer, http_response},
        },
    };

    fn state(zotero_api_url: impl AsRef<str>) -> AppState {
        AppState {
            zotero_api_url: zotero_api_url.as_ref().to_owned(),
            ..AppState::from_env()
        }
    }

    #[tokio::test]
    async fn returns_content_field() {
        let server = MockServer::new(vec![http_response(
            "200 OK",
            r#"{"content":"paper text"}"#,
        )]);
        let app = state(server.url());

        let result = ZoteroClient::new(&app)
            .get_item_fulltext(&ItemKey::from("ITEM0001"))
            .await;

        assert_eq!(result.ok().as_deref(), Some("paper text"));
    }

    #[tokio::test]
    async fn returns_empty_string_when_content_field_is_missing_or_not_string()
    {
        let server = MockServer::new(vec![
            http_response("200 OK", r"{}"),
            http_response("200 OK", r#"{"content":42}"#),
        ]);
        let app = state(server.url());
        let client = ZoteroClient::new(&app);

        let missing =
            client.get_item_fulltext(&ItemKey::from("ITEM0001")).await;
        let non_string =
            client.get_item_fulltext(&ItemKey::from("ITEM0002")).await;

        assert_eq!(missing.ok().as_deref(), Some(""));
        assert_eq!(non_string.ok().as_deref(), Some(""));
    }
}
