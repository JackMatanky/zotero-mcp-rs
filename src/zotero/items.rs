//! Core item lifecycle operations for the Zotero Local HTTP API.
//!
//! Provides [`ZoteroClient`] methods for reading, creating, updating, deleting,
//! and moving library items into or out of the trash. Called by item management
//! tool handlers in `crate::mcp::zotero::items`.
//!
//! # Main Types
//!
//! - [`TrashAction`]: Requested trash state transition (`MoveToTrash` or
//!   `Restore`).
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::state::AppState;
//! # use zotero_mcp_rs::zotero::client::ZoteroClient;
//! # use zotero_mcp_rs::zotero::ItemKey;
//! # async fn example(
//! #     state: AppState,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let client = ZoteroClient::new(&state);
//! let item_key = ItemKey::from("ABCD1234");
//! let item = client.get_item(&item_key).await?;
//! println!("Item key: {}", item.key,);
//! # Ok(())
//! # }
//! ```

use std::{fmt::Write, path::Path};

use md5::Digest;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::fs;

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        ItemKey, ItemType, LinkMode, ZoteroItem, client::ZoteroClient,
        metadata::ItemDraft,
    },
};

/// Requested trash state transition for a library item.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) enum TrashAction {
    /// Move the item into the trash.
    MoveToTrash,
    /// Restore the item from the trash back to the library.
    Restore,
}

impl TrashAction {
    /// Returns `true` if this action represents a deletion to trash.
    pub(crate) fn is_deleted(self) -> bool {
        matches!(self, Self::MoveToTrash)
    }
}

/// Phase-1 response payload from Zotero's file-upload endpoint.
#[derive(Deserialize)]
struct UploadTicket {
    /// Signed upload URL to `POST` the raw file bytes to.
    url: String,
    /// Upload key replayed in the finalize request.
    #[serde(rename = "uploadKey")]
    upload_key: String,
}

impl ZoteroClient<'_> {
    /// Fetches the `limit` most recently modified library items, excluding
    /// notes.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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
    /// through the whole library with a stable date-modified ordering so page
    /// boundaries are deterministic.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if a response body cannot be decoded.
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
    /// - [`ZoteroMcpError::NotFound`] if the item does not exist.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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
    /// - [`ZoteroMcpError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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
    /// - [`ZoteroMcpError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroMcpError::NotFound`] if the item does not exist.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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

    /// Sets the item's trash state for `item_key` according to `action`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroMcpError::NotFound`] if the item does not exist.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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
    /// - [`ZoteroMcpError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
    pub(crate) async fn create_item_from_metadata(
        &self,
        draft: ItemDraft,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        self.post_json_first(&url, &vec![draft], "Created item array was empty")
            .await
    }

    /// Fetches Zotero's indexed full-text content for `item_key`, returning an
    /// empty string if the item is unindexed or missing text.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code
    /// - [`ZoteroMcpError::Network`] if the HTTP request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded
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

    /// Attaches a linked file or URL to a parent library item.
    ///
    /// # Arguments
    ///
    /// * `parent_item_key` - Key of the parent item to attach to.
    /// * `title` - Display title for the attachment.
    /// * `file_path_or_url` - Local filepath or web URL to link.
    /// * `content_type` - Optional MIME content type (defaults to
    ///   `"application/pdf"`).
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroMcpError::Network`] if the request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if the response body cannot be decoded.
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

    /// Imports a local file into Zotero storage via a three-phase MD5 upload
    /// sequence and returns the created attachment item.
    ///
    /// If Zotero already contains an identical file (matching MD5 checksum),
    /// this method returns the existing attachment without re-uploading raw
    /// bytes.
    ///
    /// # Arguments
    ///
    /// * `parent_item_key` - Parent item to attach to; [`None`] creates a
    ///   top-level attachment.
    /// * `title` - Display title for the attachment.
    /// * `path` - Path to the local file to import.
    /// * `content_type` - Optional MIME content type (defaults to
    ///   `"application/pdf"`).
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroMcpError::InputRejected`] if the filepath has no valid UTF-8
    ///   filename.
    /// - [`ZoteroMcpError::Io`] if reading the local file fails.
    /// - [`ZoteroMcpError::LocalApi`] if Zotero rejects any phase of the
    ///   upload.
    /// - [`ZoteroMcpError::Network`] if a request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroMcpError::Json`] if a response body cannot be decoded.
    pub(crate) async fn import_pdf_file(
        &self,
        parent_item_key: Option<&ItemKey>,
        title: &str,
        path: &Path,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;

        let bytes = fs::read(path).await?;

        let mut hasher = md5::Md5::new();
        hasher.update(&bytes);
        let mut md5 = String::with_capacity(32);
        for byte in hasher.finalize() {
            let _ = write!(md5, "{byte:02x}");
        }

        let filename =
            path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                ZoteroMcpError::InputRejected(
                    "path has no valid UTF-8 filename".into(),
                )
            })?;

        let metadata = fs::metadata(path).await?;
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));

        let mut attachment = serde_json::Map::new();
        attachment.insert("itemType".into(), json!(ItemType::Attachment));
        attachment.insert("title".into(), json!(title));
        attachment.insert("linkMode".into(), json!(LinkMode::ImportedFile));
        attachment.insert("filename".into(), json!(filename));
        attachment.insert(
            "contentType".into(),
            json!(content_type.unwrap_or("application/pdf")),
        );
        if let Some(parent) = parent_item_key {
            attachment.insert("parentItem".into(), json!(parent));
        }
        let create_url = format!("{}/users/0/items", self.state.zotero_api_url);
        let item: ZoteroItem = self
            .post_json_first(
                &create_url,
                &json!([attachment]),
                "Created attachment array was empty",
            )
            .await?;

        let file_url = format!(
            "{}/users/0/items/{}/file",
            self.state.zotero_api_url, item.data.key
        );
        let filesize_text = bytes.len().to_string();
        let mtime_text = modified_ms.to_string();
        let resp = self
            .state
            .client
            .post(&file_url)
            .form(&[
                ("md5", md5.as_str()),
                ("filename", filename),
                ("filesize", filesize_text.as_str()),
                ("mtime", mtime_text.as_str()),
            ])
            .header("If-None-Match", "*")
            .send()
            .await?;
        let body: serde_json::Value =
            self.ensure_success(resp).await?.json().await?;
        if body.as_object().is_some_and(|object| object.contains_key("exists"))
        {
            return Ok(item);
        }
        let ticket: UploadTicket = serde_json::from_value(body)?;

        let upload =
            self.state.client.post(&ticket.url).body(bytes).send().await?;
        if upload.status().as_u16() != 201 {
            return Err(ZoteroMcpError::LocalApi {
                status: upload.status().as_u16(),
                message: upload.text().await.unwrap_or_default(),
            });
        }

        let finalize = self
            .state
            .client
            .post(&file_url)
            .form(&[("upload", ticket.upload_key.as_str())])
            .header("If-None-Match", "*")
            .send()
            .await?;
        self.ensure_success(finalize).await?;
        Ok(item)
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

    mod fulltext {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn returns_content_field() {
            let server = MockServer::new(vec![http_response(
                "200 OK",
                r#"{"content":"paper text"}"#,
            )]);
            let app = state(server.url(), false);

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
            let app = state(server.url(), false);
            let client = ZoteroClient::new(&app);

            let missing =
                client.get_item_fulltext(&ItemKey::from("ITEM0001")).await;
            let non_string =
                client.get_item_fulltext(&ItemKey::from("ITEM0002")).await;

            assert_eq!(missing.ok().as_deref(), Some(""));
            assert_eq!(non_string.ok().as_deref(), Some(""));
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

    mod attachments {
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

        fn state(
            zotero_api_url: impl AsRef<str>,
            write_enabled: bool,
        ) -> AppState {
            AppState {
                zotero_api_url: zotero_api_url.as_ref().to_owned(),
                write_enabled,
                ..AppState::from_env()
            }
        }

        fn created_attachment() -> String {
            json!([{"key":"ATTACH01","version":1,"data":{"key":"ATTACH01","version":1,"itemType":"attachment"}}]).to_string()
        }

        async fn attach_and_body(
            content_type: Option<&str>,
        ) -> serde_json::Value {
            let (server, recorded) =
                MockServer::recording(vec![http_response(
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
            assert_eq!(
                payload.get("contentType"),
                Some(&json!("application/pdf"))
            );
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

        mod import_pdf {
            use pretty_assertions::assert_eq;

            use super::*;

            fn pdf_file() -> (tempfile::TempDir, std::path::PathBuf) {
                let dir = tempfile::tempdir().expect("temp dir");
                let path = dir.path().join("paper.pdf");
                std::fs::write(&path, b"%PDF-1.4\n%%EOF\n").expect("write pdf");
                (dir, path)
            }

            fn phase1_response(upload_url: &str) -> String {
                json!({
                    "url": upload_url,
                    "uploadKey": "uk",
                    "contentType": "application/pdf",
                    "prefix": "",
                    "suffix": "",
                })
                .to_string()
            }

            #[tokio::test]
            async fn imports_pdf_via_three_phase_upload() {
                let (_dir, pdf_path) = pdf_file();
                let (upload_server, upload_recorded) = MockServer::recording(
                    vec![http_response("201 Created", "")],
                );
                let (api_server, recorded) = MockServer::recording(vec![
                    http_response("200 OK", &created_attachment()),
                    http_response(
                        "200 OK",
                        &phase1_response(&format!(
                            "{}/upload",
                            upload_server.url()
                        )),
                    ),
                    http_response("204 No Content", ""),
                ]);
                let app = state(api_server.url(), true);

                let result = ZoteroClient::new(&app)
                    .import_pdf_file(
                        Some(&ItemKey::from("PARENT01")),
                        "Paper",
                        &pdf_path,
                        None,
                    )
                    .await;

                assert!(result.is_ok(), "import should succeed: {result:?}");
                let requests = recorded.lock().expect("request log lock");
                assert_eq!(requests.len(), 3);

                let created =
                    request_body(requests.first().expect("request 0"))
                        .expect("create request json")
                        .get(0)
                        .expect("created item")
                        .clone();
                assert_eq!(created.get("parentItem"), Some(&json!("PARENT01")));
                assert_eq!(
                    created.get("linkMode"),
                    Some(&json!("imported_file"))
                );
                assert_eq!(created.get("filename"), Some(&json!("paper.pdf")));
                assert_eq!(
                    created.get("contentType"),
                    Some(&json!("application/pdf"))
                );

                let phase1 = requests.get(1).expect("request 1").to_lowercase();
                assert!(phase1.contains("md5="));
                assert!(phase1.contains("filename=paper.pdf"));
                assert!(phase1.contains("filesize="));
                assert!(phase1.contains("mtime="));
                assert!(phase1.contains("if-none-match: *"));

                let upload_requests =
                    upload_recorded.lock().expect("upload request log");
                assert_eq!(upload_requests.len(), 1);
                let upload_body = upload_requests
                    .first()
                    .expect("upload request")
                    .split_once("\r\n\r\n")
                    .map_or("", |(_, body)| body);
                assert_eq!(upload_body, "%PDF-1.4\n%%EOF\n");

                let phase3 = requests.get(2).expect("request 2").to_lowercase();
                assert!(phase3.contains("upload=uk"));
                assert!(phase3.contains("if-none-match: *"));
            }

            #[tokio::test]
            async fn short_circuits_when_zotero_already_has_the_file() {
                let (_dir, pdf_path) = pdf_file();
                let (api_server, recorded) = MockServer::recording(vec![
                    http_response("200 OK", &created_attachment()),
                    http_response("200 OK", r#"{"exists": 1}"#),
                ]);
                let app = state(api_server.url(), true);

                let result = ZoteroClient::new(&app)
                    .import_pdf_file(None, "Paper", &pdf_path, None)
                    .await;

                assert!(
                    result.is_ok(),
                    "exists short-circuit should succeed: {result:?}"
                );
                let requests = recorded.lock().expect("request log lock");
                assert_eq!(requests.len(), 2);
                let created =
                    request_body(requests.first().expect("request 0"))
                        .expect("create request json")
                        .get(0)
                        .expect("created item")
                        .clone();
                assert!(
                    created.get("parentItem").is_none(),
                    "top-level import must omit parentItem"
                );
                assert!(requests.get(1).expect("request 1").contains("md5="));
            }

            #[tokio::test]
            async fn denies_writes_when_write_permission_is_disabled() {
                let app = state("http://127.0.0.1:1", false);

                let result = ZoteroClient::new(&app)
                    .import_pdf_file(
                        Some(&ItemKey::from("PARENT01")),
                        "Paper",
                        std::path::Path::new("/tmp/paper.pdf"),
                        None,
                    )
                    .await;

                assert!(
                    matches!(result, Err(ZoteroMcpError::PermissionDenied(_))),
                    "write-disabled import should fail before HTTP: {result:?}"
                );
            }
        }
    }
}
