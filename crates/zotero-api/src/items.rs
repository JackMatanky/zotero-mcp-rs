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
//! # use zotero_api::AppState;
//! # use zotero_api::{ItemKey, ZoteroClient};
//! # async fn example(
//! #     state: AppState,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let client = ZoteroClient::new(&state);
//! let item_key = ItemKey::from("ABCD1234");
//! let item = client.get_item(&item_key).await?;
//! let _serialized = serde_json::to_string(&item)?;
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
    client::ZoteroClient,
    errors::ZoteroApiError,
    keys::{ItemKey, LibraryVersion},
    metadata::ItemDraft,
    objects::{BatchWriteResponse, ZoteroItem},
    types::{ItemType, LinkMode},
};

/// Requested trash state transition for a library item.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum TrashAction {
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
    /// Fetches the `limit` most recently modified items in the target library,
    /// excluding notes.
    ///
    /// Issues a `GET` request against `<prefix>/items` with
    /// `sort=dateModified&direction=desc` and `itemType=-note`. Results are
    /// returned as a [`Vec<ZoteroItem>`] sorted by modification
    /// timestamp in descending order.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of recent items to fetch.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if the Zotero Local API returns a non-2xx
    ///   HTTP status.
    /// - [`ZoteroApiError::Network`] if a network transport error occurs.
    /// - [`ZoteroApiError::Json`] if the HTTP response payload cannot be
    ///   decoded.
    #[inline]
    pub async fn get_recent_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroApiError> {
        let url = format!(
            "{}{}/items?limit={}&sort=dateModified&direction=desc&\
             itemType=-note",
            self.state.zotero_api_url(),
            self.target_prefix(),
            limit
        );
        self.get_json(&url).await
    }

    /// Fetches every top-level item across the entire library (excluding
    /// notes), automatically paginating.
    ///
    /// Queries `<prefix>/items` in pages of 100 items using a stable
    /// `dateModified` descending sort. Continues issuing page requests
    /// until a page returns fewer items than requested or an empty list,
    /// ensuring deterministic, loss-free library iteration.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status on any
    ///   page request.
    /// - [`ZoteroApiError::Network`] if transport failures occur during
    ///   pagination.
    /// - [`ZoteroApiError::Json`] if any page payload fails JSON
    ///   deserialization.
    #[inline]
    pub async fn get_all_items(
        &self,
    ) -> Result<Vec<ZoteroItem>, ZoteroApiError> {
        let url = format!(
            "{}{}/items?itemType=-note&sort=dateModified&direction=desc",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.get_all_json(&url, 100).await
    }

    /// Fetches a single library item by its unique [`ItemKey`].
    ///
    /// Issues a `GET` request against `<prefix>/items/<item_key>`. If Zotero
    /// returns a 404 status, this method maps it directly to
    /// [`ZoteroApiError::NotFound`].
    ///
    /// # Arguments
    ///
    /// * `item_key` - Eight-character alphanumeric identifier of the target
    ///   item.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::NotFound`] if no item with `item_key` exists in the
    ///   library target.
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx HTTP status
    ///   code other than 404.
    /// - [`ZoteroApiError::Network`] if a transport-level error occurs.
    /// - [`ZoteroApiError::Json`] if the returned item representation cannot be
    ///   parsed.
    #[inline]
    pub async fn get_item(
        &self,
        item_key: &ItemKey,
    ) -> Result<ZoteroItem, ZoteroApiError> {
        let url = format!(
            "{}{}/items/{}",
            self.state.zotero_api_url(),
            self.target_prefix(),
            item_key
        );
        let resp =
            self.state.send_with_retry(self.state.client().get(&url)).await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ZoteroApiError::NotFound(format!("Item {item_key}")));
        }
        Ok(self.ensure_success(resp).await?.json().await?)
    }

    /// Retrieves top-level items that do not belong to any collection.
    ///
    /// Queries `<prefix>/items/top` for top-level library items up to `limit`,
    /// filtering out items whose `collections` key array is non-empty.
    /// Useful for locating unorganized references.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of top-level items to retrieve before unfiled
    ///   filtering.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status.
    /// - [`ZoteroApiError::Network`] if a network transport failure occurs.
    /// - [`ZoteroApiError::Json`] if response decoding fails.
    #[inline]
    pub async fn get_unfiled_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroApiError> {
        let url = format!(
            "{}{}/items/top?limit={}",
            self.state.zotero_api_url(),
            self.target_prefix(),
            limit
        );
        let items: Vec<ZoteroItem> = self.get_json(&url).await?;
        Ok(items
            .into_iter()
            .filter(|i| i.data.collections.is_empty())
            .collect())
    }

    /// Lists all child items (such as HTML notes and file attachments)
    /// belonging to `item_key`.
    ///
    /// Issues a `GET` request to `<prefix>/items/<item_key>/children`. Returns
    /// a list of [`ZoteroItem`] objects whose `parent_item` metadata
    /// matches `item_key`.
    ///
    /// # Arguments
    ///
    /// * `item_key` - Key of the parent item whose children to fetch.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroApiError::Network`] if the transport request fails.
    /// - [`ZoteroApiError::Json`] if the children array cannot be decoded.
    #[inline]
    pub async fn get_item_children(
        &self,
        item_key: &ItemKey,
    ) -> Result<Vec<ZoteroItem>, ZoteroApiError> {
        let url = format!(
            "{}{}/items/{}/children",
            self.state.zotero_api_url(),
            self.target_prefix(),
            item_key
        );
        self.get_json(&url).await
    }

    /// Updates specific metadata fields of an existing library item via JSON
    /// patch fields.
    ///
    /// Checks write permissions via
    /// [`AppState::check_write_permission`](crate::state::AppState::check_write_permission)
    /// before sending a `PATCH` request to `<prefix>/items/<item_key>` with
    /// the supplied JSON object. If Zotero returns an empty response body
    /// (common in local write sync), this method refetches the item via
    /// [`get_item`](Self::get_item).
    ///
    /// # Arguments
    ///
    /// * `item_key` - Key of the item to modify.
    /// * `fields` - JSON payload containing the fields to update (e.g.
    ///   `{"title": "New Title", "version": 1}`).
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::NotFound`] if the item key does not exist.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the patch payload with
    ///   a non-2xx status.
    /// - [`ZoteroApiError::Network`] if the HTTP request fails at the transport
    ///   level.
    /// - [`ZoteroApiError::Json`] if response payload decoding fails.
    #[inline]
    pub async fn update_item(
        &self,
        item_key: &ItemKey,
        fields: serde_json::Value,
    ) -> Result<ZoteroItem, ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/items/{}",
            self.state.zotero_api_url(),
            self.target_prefix(),
            item_key
        );
        let resp = self
            .state
            .send_with_retry(self.state.client().patch(&url).json(&fields))
            .await?;
        let resp = self.ensure_success(resp).await?;
        match resp.json::<ZoteroItem>().await {
            Ok(item) => Ok(item),
            Err(_) => self.get_item(item_key).await,
        }
    }

    /// Permanently deletes an item from the library using optimistic
    /// concurrency checks.
    ///
    /// Verifies write permissions, fetches the current item version to populate
    /// the `If-Unmodified-Since-Version` header, and issues a `DELETE`
    /// request to `<prefix>/items/<item_key>`.
    ///
    /// # Arguments
    ///
    /// * `item_key` - Key of the item to permanently delete.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::NotFound`] if no item exists with `item_key`.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the deletion request
    ///   (e.g. version conflict 412).
    /// - [`ZoteroApiError::Network`] if a transport-level error occurs.
    #[inline]
    pub async fn delete_item(
        &self,
        item_key: &ItemKey,
    ) -> Result<(), ZoteroApiError> {
        self.state.check_write_permission()?;
        let item = self.get_item(item_key).await?;
        let url = format!(
            "{}{}/items/{}",
            self.state.zotero_api_url(),
            self.target_prefix(),
            item_key
        );
        self.delete(&url, item.version).await
    }

    /// Moves an item to the Zotero trash or restores it back to the library.
    ///
    /// Fetches the target item's version, updates its `deleted` property
    /// according to `action` ([`TrashAction::MoveToTrash`] set `deleted:
    /// true`; [`TrashAction::Restore`] set `deleted: false`), and patches
    /// the item in Zotero.
    ///
    /// # Arguments
    ///
    /// * `item_key` - Key of the target item.
    /// * `action` - Target trash state transition ([`TrashAction::MoveToTrash`]
    ///   or [`TrashAction::Restore`]).
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::NotFound`] if no item exists with `item_key`.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the patch payload.
    /// - [`ZoteroApiError::Network`] if the transport request fails.
    #[inline]
    pub async fn set_item_deleted(
        &self,
        item_key: &ItemKey,
        action: TrashAction,
    ) -> Result<ZoteroItem, ZoteroApiError> {
        self.state.check_write_permission()?;
        let item = self.get_item(item_key).await?;
        self.update_item(
            item_key,
            serde_json::json!({
                "deleted": action.is_deleted(),
                "version": item.version
            }),
        )
        .await
    }

    /// Creates a new library item from a resolved metadata draft.
    ///
    /// Verifies write permissions and issues a `POST` request to
    /// `<prefix>/items` with a single-element array containing the
    /// [`ItemDraft`]. Returns the newly created [`ZoteroItem`].
    ///
    /// # Arguments
    ///
    /// * `draft` - Typed metadata draft returned by metadata resolution or
    ///   manually constructed.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if the created item response payload cannot
    ///   be parsed.
    #[inline]
    pub async fn create_item_from_metadata(
        &self,
        draft: ItemDraft,
    ) -> Result<ZoteroItem, ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/items",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.post_json_first(&url, &vec![draft], "Created item array was empty")
            .await
    }

    /// Batch-creates multiple items in a single request via `POST
    /// <prefix>/items`.
    ///
    /// Sends an array of item JSON payloads to Zotero's batch creation
    /// endpoint. Returns a [`BatchWriteResponse`] mapping created item keys
    /// and index positions.
    ///
    /// # Arguments
    ///
    /// * `items` - Slice of raw JSON item objects to create in Zotero.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the batch payload.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if the batch response payload cannot be
    ///   decoded.
    #[inline]
    pub async fn create_items(
        &self,
        items: &[serde_json::Value],
    ) -> Result<BatchWriteResponse, ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/items",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let req =
            self.apply_auth_headers(self.state.client().post(&url).json(items));
        let resp = self.state.send_with_retry(req).await?;
        Ok(self.ensure_success(resp).await?.json().await?)
    }

    /// Batch-updates multiple items in a single request via `POST
    /// <prefix>/items`.
    ///
    /// Submits patch payloads containing item keys and updated fields to
    /// Zotero's batch endpoint.
    ///
    /// # Arguments
    ///
    /// * `items` - Slice of JSON patch objects containing item keys and fields
    ///   to modify.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the batch update
    ///   payload.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if the batch response payload cannot be
    ///   decoded.
    #[inline]
    pub async fn update_items(
        &self,
        items: &[serde_json::Value],
    ) -> Result<BatchWriteResponse, ZoteroApiError> {
        self.create_items(items).await
    }

    /// Batch-deletes multiple items by key in a single request via `DELETE
    /// <prefix>/items?itemKey=K1,K2,...`.
    ///
    /// Verifies write permissions and issues a comma-separated key deletion
    /// query with optimistic version header validation
    /// (`If-Unmodified-Since-Version`).
    ///
    /// # Arguments
    ///
    /// * `keys` - Slice of item keys to delete.
    /// * `version` - Current library version required for concurrency
    ///   protection.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code
    ///   (e.g. 412 version conflict).
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn delete_items(
        &self,
        keys: &[ItemKey],
        version: LibraryVersion,
    ) -> Result<(), ZoteroApiError> {
        self.state.check_write_permission()?;
        let keys_str =
            keys.iter().map(ItemKey::as_str).collect::<Vec<_>>().join(",");
        let url = format!(
            "{}{}/items?itemKey={keys_str}",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.delete(&url, version).await
    }

    /// Retrieves the local file view URL for an attachment item.
    /// Queries `GET <prefix>/items/<key>/file/view/url`. Returns the local
    /// web/protocol URL string used by Zotero desktop or local clients to
    /// view attachment content.
    ///
    /// # Arguments
    ///
    /// * `key` - Key of the target attachment item.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn get_item_file_view_url(
        &self,
        key: &ItemKey,
    ) -> Result<String, ZoteroApiError> {
        let url = format!(
            "{}{}/items/{}/file/view/url",
            self.state.zotero_api_url(),
            self.target_prefix(),
            key
        );
        let req = self.apply_auth_headers(self.state.client().get(&url));
        let resp = self.state.send_with_retry(req).await?;
        let resp = self.ensure_success(resp).await?;
        Ok(resp.text().await?)
    }

    /// Fetches Zotero's indexed full-text content for an item, returning an
    /// empty string if unindexed.
    ///
    /// Queries `GET <prefix>/items/<item_key>/fulltext`. Extracts the `content`
    /// field string if present, or defaults to an empty string.
    ///
    /// # Arguments
    ///
    /// * `item_key` - Key of the item whose full-text index content to
    ///   retrieve.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if the full-text payload cannot be decoded.
    #[inline]
    pub async fn get_item_fulltext(
        &self,
        item_key: &ItemKey,
    ) -> Result<String, ZoteroApiError> {
        let url = format!(
            "{}{}/items/{}/fulltext",
            self.state.zotero_api_url(),
            self.target_prefix(),
            item_key
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
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    #[inline]
    pub async fn attach_file_link(
        &self,
        parent_item_key: &ItemKey,
        title: &str,
        file_path_or_url: &str,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/items",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
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
    /// - [`ZoteroApiError::PermissionDenied`] if write access is disabled.
    /// - [`ZoteroApiError::InputRejected`] if the filepath has no valid UTF-8
    ///   filename.
    /// - [`ZoteroApiError::Io`] if reading the local file fails.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects any phase of the
    ///   upload.
    /// - [`ZoteroApiError::Network`] if a request fails at the HTTP transport
    ///   level.
    /// - [`ZoteroApiError::Json`] if a response body cannot be decoded.
    #[inline]
    pub async fn import_pdf_file(
        &self,
        parent_item_key: Option<&ItemKey>,
        title: &str,
        path: &Path,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroApiError> {
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
                ZoteroApiError::InputRejected(
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
        let create_url = format!(
            "{}{}/items",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let item: ZoteroItem = self
            .post_json_first(
                &create_url,
                &json!([attachment]),
                "Created attachment array was empty",
            )
            .await?;

        let file_url = format!(
            "{}{}/items/{}/file",
            self.state.zotero_api_url(),
            self.target_prefix(),
            item.data.key
        );
        let filesize_text = bytes.len().to_string();
        let mtime_text = modified_ms.to_string();
        let resp = self
            .state
            .client()
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
            self.state.client().post(&ticket.url).body(bytes).send().await?;
        if upload.status().as_u16() != 201 {
            return Err(ZoteroApiError::LocalApi {
                status: upload.status().as_u16(),
                message: upload.text().await.unwrap_or_default(),
            });
        }

        let finalize = self
            .state
            .client()
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
        client::{
            ZoteroClient,
            test_http::{MockServer, http_response, request_body},
        },
        state::AppState,
    };

    fn state(zotero_api_url: impl AsRef<str>, write_enabled: bool) -> AppState {
        AppState::test_default()
            .with_zotero_api_url(zotero_api_url.as_ref())
            .with_write_enabled(write_enabled)
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
                matches!(result, Err(ZoteroApiError::NotFound(_))),
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
            client::{
                ZoteroClient,
                test_http::{MockServer, http_response, request_body},
            },
            state::AppState,
        };

        fn state(
            zotero_api_url: impl AsRef<str>,
            write_enabled: bool,
        ) -> AppState {
            AppState::test_default()
                .with_zotero_api_url(zotero_api_url.as_ref())
                .with_write_enabled(write_enabled)
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
                matches!(result, Err(ZoteroApiError::PermissionDenied(_))),
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
                    matches!(result, Err(ZoteroApiError::PermissionDenied(_))),
                    "write-disabled import should fail before HTTP: {result:?}"
                );
            }
        }
    }
    mod batch {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn creates_items_in_batch() {
            let json_resp = serde_json::json!({
                "successful": {"0": "KEY00001"},
                "unchanged": {},
                "failed": {}
            })
            .to_string();

            let (server, recorded) =
                MockServer::recording(vec![http_response(
                    "200 OK", &json_resp,
                )]);
            let app = state(server.url(), true);
            let client = ZoteroClient::new(&app);

            let res = client
                .create_items(&[serde_json::json!({"itemType": "book"})])
                .await;
            assert!(res.is_ok());
            let batch_res = res.unwrap();
            assert_eq!(
                batch_res.successful.get("0"),
                Some(&serde_json::json!("KEY00001"))
            );

            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 1);
            assert!(
                requests.first().unwrap().starts_with("POST /users/0/items")
            );
        }

        #[tokio::test]
        async fn deletes_items_in_batch() {
            let (server, recorded) =
                MockServer::recording(vec![http_response(
                    "204 No Content",
                    "",
                )]);
            let app = state(server.url(), true);
            let client = ZoteroClient::new(&app);

            let keys =
                vec![ItemKey::from("K1000001"), ItemKey::from("K2000002")];
            let res = client.delete_items(&keys, LibraryVersion(5)).await;
            assert!(res.is_ok());

            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 1);
            assert!(requests.first().unwrap().starts_with(
                "DELETE /users/0/items?itemKey=K1000001,K2000002"
            ));
        }

        #[tokio::test]
        async fn fetches_item_file_view_url() {
            let (server, recorded) =
                MockServer::recording(vec![http_response(
                    "200 OK",
                    "http://zotero.org/view/pdf",
                )]);
            let app = state(server.url(), false);
            let client = ZoteroClient::new(&app);

            let url = client
                .get_item_file_view_url(&ItemKey::from("ITEM0001"))
                .await
                .unwrap();
            assert_eq!(url, "http://zotero.org/view/pdf");

            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 1);
            assert!(
                requests
                    .first()
                    .unwrap()
                    .starts_with("GET /users/0/items/ITEM0001/file/view/url")
            );
        }
    }
}
