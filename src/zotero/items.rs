//! Core item lifecycle operations for the Zotero Local HTTP API.
//!
//! Adds [`ZoteroClient`] methods for item reads, metadata-created item writes,
//! field updates, trash/restore, and deletion.

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{ItemKey, ZoteroItem, client::ZoteroClient, metadata::ItemDraft},
};

/// Requested trash state transition for an item.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) enum TrashAction {
    MoveToTrash,
    Restore,
}

impl TrashAction {
    pub(crate) fn is_deleted(self) -> bool {
        matches!(self, Self::MoveToTrash)
    }
}

impl ZoteroClient<'_> {
    /// Fetches the `limit` most recently modified library items, excluding
    /// notes.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_recent_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items?limit={}&sort=dateModified&direction=desc&\
             itemType=-note",
            self.state.zotero_api_url, limit
        );
        self.get_json(&url).await
    }

    /// Fetches every top-level library item (notes excluded), paginating
    /// through the whole library with a stable date-modified ordering so
    /// page boundaries are deterministic.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if a response cannot be decoded
    pub(crate) async fn get_all_items(
        &self,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items?itemType=-note&sort=dateModified&direction=desc",
            self.state.zotero_api_url
        );
        self.get_all_json(&url, 100).await
    }

    /// Fetches the item identified by `item_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::NotFound`] if the item does not exist
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_item(
        &self,
        item_key: &ItemKey,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        let url =
            format!("{}/users/0/items/{}", self.state.zotero_api_url, item_key);
        let resp =
            self.state.send_with_retry(self.state.client.get(&url)).await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ZoteroMcpError::NotFound(format!("Item {item_key}")));
        }
        Ok(self.ensure_success(resp).await?.json().await?)
    }

    /// Lists top-level items not belonging to any collection, up to `limit`
    /// items.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_unfiled_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/top?limit={}",
            self.state.zotero_api_url, limit
        );
        let items: Vec<ZoteroItem> = self.get_json(&url).await?;
        Ok(items
            .into_iter()
            .filter(|i| i.data.collections.is_empty())
            .collect())
    }

    /// Fetches the child items (notes and attachments) of `item_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_item_children(
        &self,
        item_key: &ItemKey,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/children",
            self.state.zotero_api_url, item_key
        );
        self.get_json(&url).await
    }

    /// Updates fields of an existing item identified by `item_key` with JSON
    /// `fields`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn update_item(
        &self,
        item_key: &ItemKey,
        fields: serde_json::Value,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url =
            format!("{}/users/0/items/{}", self.state.zotero_api_url, item_key);
        let resp = self
            .state
            .send_with_retry(self.state.client.patch(&url).json(&fields))
            .await?;
        let resp = self.ensure_success(resp).await?;
        match resp.json::<ZoteroItem>().await {
            Ok(item) => Ok(item),
            Err(_) => self.get_item(item_key).await,
        }
    }

    /// Permanently deletes the item identified by `item_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::NotFound`] if the item does not exist
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn delete_item(
        &self,
        item_key: &ItemKey,
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let item = self.get_item(item_key).await?;
        let url =
            format!("{}/users/0/items/{}", self.state.zotero_api_url, item_key);
        self.delete(&url, item.version).await
    }

    /// Sets the item's trash state for `item_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::NotFound`] if the item does not exist
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn set_item_deleted(
        &self,
        item_key: &ItemKey,
        action: TrashAction,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let item = self.get_item(item_key).await?;
        self.update_item(
            item_key,
            serde_json::json!({"deleted": action.is_deleted(), "version": item.version}),
        )
        .await
    }

    /// Creates a new item from a resolved metadata `draft` (as returned by
    /// [`crate::zotero::metadata::resolve_metadata`]).
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn create_item_from_metadata(
        &self,
        draft: ItemDraft,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        self.post_json_first(&url, &vec![draft], "Created item array was empty")
            .await
    }
}

#[cfg(test)]
mod tests {
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
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: String::new(),
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled,
            ..AppState::from_env()
        }
    }

    fn item_json(
        key: &str,
        deleted: bool,
        collections: &serde_json::Value,
    ) -> String {
        json!({
            "key": key,
            "version": 7,
            "data": {
                "key": key,
                "version": 7,
                "itemType": "journalArticle",
                "deleted": deleted,
                "collections": collections.clone(),
            },
        })
        .to_string()
    }

    mod trash_action {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn returns_true_for_move_to_trash() {
            assert!(
                TrashAction::MoveToTrash.is_deleted(),
                "move-to-trash maps to deleted=true"
            );
        }

        #[test]
        fn returns_false_for_restore() {
            assert_eq!(
                TrashAction::Restore.is_deleted(),
                false,
                "restore maps to deleted=false"
            );
        }
    }

    mod get_item {
        use super::*;

        #[tokio::test]
        async fn returns_not_found_on_404() {
            let server =
                MockServer::new(vec![http_response("404 Not Found", "")]);
            let app = state(server.url(), false);

            let result = ZoteroClient::new(&app)
                .get_item(&ItemKey::from("ITEM0001"))
                .await;

            assert!(
                matches!(result, Err(ZoteroMcpError::NotFound(_))),
                "404 should map to NotFound: {result:?}"
            );
        }
    }

    mod get_unfiled_items {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn filters_items_with_collections() {
            let body = format!(
                "[{},{}]",
                item_json("ITEM0001", false, &json!([])),
                item_json("ITEM0002", false, &json!(["COLL0001"]))
            );
            let server = MockServer::new(vec![http_response("200 OK", &body)]);
            let app = state(server.url(), false);

            let result = ZoteroClient::new(&app).get_unfiled_items(10).await;

            assert!(
                result.is_ok(),
                "unfiled items response should decode: {result:?}"
            );
            let items = result.unwrap_or_default();
            assert_eq!(
                items.iter().map(|item| item.key.as_str()).collect::<Vec<_>>(),
                vec!["ITEM0001"]
            );
        }
    }

    mod update_item {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn refetches_item_when_patch_response_is_empty() {
            let (server, recorded) = MockServer::recording(vec![
                http_response("200 OK", ""),
                http_response(
                    "200 OK",
                    &item_json("ITEM0001", false, &json!([])),
                ),
            ]);
            let app = state(server.url(), true);

            let result = ZoteroClient::new(&app)
                .update_item(
                    &ItemKey::from("ITEM0001"),
                    json!({"title": "Updated"}),
                )
                .await;

            assert!(
                result.is_ok(),
                "empty PATCH response should refetch item: {result:?}"
            );
            assert_eq!(result.expect("asserted Ok above").key, "ITEM0001");
            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 2);
            assert!(
                requests.first().is_some_and(|request| request
                    .starts_with("PATCH /users/0/items/ITEM0001")),
                "first request should PATCH item: {requests:?}"
            );
            assert!(
                requests.get(1).is_some_and(|request| request
                    .starts_with("GET /users/0/items/ITEM0001")),
                "second request should refetch item: {requests:?}"
            );
        }
    }

    mod set_item_deleted {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn sends_deleted_flag_for_move_to_trash_and_restore() {
            let (server, recorded) = MockServer::recording(vec![
                http_response(
                    "200 OK",
                    &item_json("ITEM0001", false, &json!([])),
                ),
                http_response(
                    "200 OK",
                    &item_json("ITEM0001", true, &json!([])),
                ),
                http_response(
                    "200 OK",
                    &item_json("ITEM0001", true, &json!([])),
                ),
                http_response(
                    "200 OK",
                    &item_json("ITEM0001", false, &json!([])),
                ),
            ]);
            let app = state(server.url(), true);
            let client = ZoteroClient::new(&app);

            let trashed = client
                .set_item_deleted(
                    &ItemKey::from("ITEM0001"),
                    TrashAction::MoveToTrash,
                )
                .await;
            let restored = client
                .set_item_deleted(
                    &ItemKey::from("ITEM0001"),
                    TrashAction::Restore,
                )
                .await;

            assert!(
                trashed.is_ok(),
                "move-to-trash should succeed: {trashed:?}"
            );
            assert!(restored.is_ok(), "restore should succeed: {restored:?}");
            let requests = recorded.lock().expect("request log lock");
            let trash_body = requests
                .get(1)
                .and_then(|request| request_body(request).ok())
                .unwrap_or_default();
            let restore_body = requests
                .get(3)
                .and_then(|request| request_body(request).ok())
                .unwrap_or_default();
            assert_eq!(trash_body.get("deleted"), Some(&json!(true)));
            assert_eq!(restore_body.get("deleted"), Some(&json!(false)));
            assert_eq!(trash_body.get("version"), Some(&json!(7)));
            assert_eq!(restore_body.get("version"), Some(&json!(7)));
        }
    }
}
