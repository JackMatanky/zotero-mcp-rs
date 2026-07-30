//! Write and mutation operations for the Zotero Local HTTP API.

use crate::{
    errors::ZoteroMcpError,
    zotero::{ZoteroClient, ZoteroCollection, ZoteroItem, models::ZoteroTag},
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
        parent_item_key: &str,
        note_content: &str,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": "note",
            "parentItem": parent_item_key,
            "note": note_content,
        }]);

        self.post_json_first(&url, &payload, "Created note array was empty")
            .await
    }

    /// Creates a new collection with `name` and optional `parent_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn create_collection(
        &self,
        name: &str,
        parent_key: Option<&str>,
    ) -> Result<ZoteroCollection, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/collections", self.state.zotero_api_url);
        let parent_val = match parent_key {
            Some(k) => serde_json::Value::String(k.to_owned()),
            None => serde_json::Value::String("false".to_owned()),
        };
        let payload = serde_json::json!([{
            "name": name,
            "parentCollection": parent_val,
        }]);

        self.post_json_first(
            &url,
            &payload,
            "Created collection array was empty",
        )
        .await
    }

    /// Adds or removes items to or from a collection.
    ///
    /// # Arguments
    ///
    /// - `collection_key`: Key of the target collection.
    /// - `item_keys`: Slice of item keys to add or remove.
    /// - `remove`: `true` to remove items from the collection, `false` to add them.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn manage_collection_items(
        &self,
        collection_key: &str,
        item_keys: &[String],
        remove: bool,
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}/users/0/collections/{}/items",
            self.state.zotero_api_url, collection_key
        );
        let body_str = item_keys.join(" ");

        let req = if remove {
            self.state.client.delete(&url).body(body_str)
        } else {
            self.state.client.post(&url).body(body_str)
        };

        self.ensure_success(self.state.send_with_retry(req).await?).await?;
        Ok(())
    }

    /// Updates fields of an existing item identified by `item_key` with JSON `fields`.
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
        item_key: &str,
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

    /// Attaches a linked file to a parent item.
    ///
    /// # Arguments
    ///
    /// - `parent_item_key`: Key of the parent item to attach to.
    /// - `title`: Display title of the attachment.
    /// - `file_path_or_url`: File path or URL of the file to link.
    /// - `content_type`: Optional MIME content type (defaults to `"application/pdf"`).
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
        parent_item_key: &str,
        title: &str,
        file_path_or_url: &str,
        content_type: Option<&str>,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": "attachment",
            "parentItem": parent_item_key,
            "title": title,
            "linkMode": "imported_file",
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

    /// Batch updates tags across multiple items by adding and removing tags.
    ///
    /// # Arguments
    ///
    /// - `item_keys`: Slice of item keys to update.
    /// - `add_tags`: Slice of tag names to add.
    /// - `remove_tags`: Slice of tag names to remove.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn batch_update_tags(
        &self,
        item_keys: &[String],
        add_tags: &[String],
        remove_tags: &[String],
    ) -> Result<usize, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let mut count: usize = 0;
        for key in item_keys {
            let item = self.get_item(key).await?;
            let new_tags = diff_tags(item.data.tags, add_tags, remove_tags);
            let patch_payload = serde_json::json!({
                "tags": new_tags,
                "version": item.version,
            });
            self.update_item(key, patch_payload).await?;
            count = count.saturating_add(1);
        }
        Ok(count)
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
        item_key: &str,
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let item = self.get_item(item_key).await?;
        let url =
            format!("{}/users/0/items/{}", self.state.zotero_api_url, item_key);
        self.delete(&url, item.version).await
    }

    /// Sets the item's trash state for `item_key`. Setting `deleted` to `true`
    /// moves the item to trash; `false` restores it.
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
        item_key: &str,
        deleted: bool,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let item = self.get_item(item_key).await?;
        self.update_item(
            item_key,
            serde_json::json!({"deleted": deleted, "version": item.version}),
        )
        .await
    }

    /// Permanently deletes the collection identified by `collection_key`. Items
    /// inside the collection are not deleted.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn delete_collection(
        &self,
        collection_key: &str,
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}/users/0/collections/{}",
            self.state.zotero_api_url, collection_key
        );
        let resp = self
            .ensure_success(
                self.state.send_with_retry(self.state.client.get(&url)).await?,
            )
            .await?;
        let collection: ZoteroCollection = resp.json().await?;
        self.delete(&url, collection.version).await
    }

    /// Creates a PDF annotation attached to `parent_attachment_key`.
    ///
    /// # Arguments
    ///
    /// - `parent_attachment_key`: Key of the parent PDF attachment.
    /// - `annotation_type`: Type of annotation (`"highlight"`, `"underline"`, or `"note"`).
    /// - `text`: Optional selected text for highlight or underline.
    /// - `comment`: Optional user comment text.
    /// - `color`: Optional CSS hex color string (defaults to `"#ffd400"`).
    /// - `page_label`: Optional page label string.
    /// - `position_json`: Raw Zotero `annotationPosition` JSON string.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if `position_json` is invalid or response decoding fails
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors Zotero's flat annotation field set; grouping into a struct would only move the same fields one layer down"
    )]
    pub(crate) async fn create_annotation(
        &self,
        parent_attachment_key: &str,
        annotation_type: &str,
        text: Option<&str>,
        comment: Option<&str>,
        color: Option<&str>,
        page_label: Option<&str>,
        position_json: &str,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let position: serde_json::Value = serde_json::from_str(position_json)?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": "annotation",
            "parentItem": parent_attachment_key,
            "annotationType": annotation_type,
            "annotationText": text,
            "annotationComment": comment.unwrap_or(""),
            "annotationColor": color.unwrap_or("#ffd400"),
            "annotationPageLabel": page_label,
            "annotationPosition": position.to_string(),
        }]);
        self.post_json_first(
            &url,
            &payload,
            "Created annotation array was empty",
        )
        .await
    }

    /// Creates a new item from a resolved metadata `draft` (as returned by
    /// [`crate::zotero::identifiers::resolve_metadata`]).
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
        draft: serde_json::Value,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        self.post_json_first(&url, &vec![draft], "Created item array was empty")
            .await
    }

    /// Renames and/or moves a collection identified by `collection_key`.
    ///
    /// # Arguments
    ///
    /// - `collection_key`: Key of the collection to update.
    /// - `name`: Optional new name for the collection.
    /// - `parent_key`: Optional parent key; pass `Some("")` to move to top level.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn update_collection(
        &self,
        collection_key: &str,
        name: Option<&str>,
        parent_key: Option<&str>,
    ) -> Result<ZoteroCollection, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}/users/0/collections/{}",
            self.state.zotero_api_url, collection_key
        );
        let resp = self
            .ensure_success(
                self.state.send_with_retry(self.state.client.get(&url)).await?,
            )
            .await?;
        let current: ZoteroCollection = resp.json().await?;

        let new_name = name.unwrap_or(&current.data.name);
        let new_parent = match parent_key {
            Some("") => serde_json::Value::Bool(false),
            Some(k) => serde_json::Value::String(k.to_owned()),
            None => current
                .data
                .parent_collection
                .clone()
                .unwrap_or(serde_json::Value::Bool(false)),
        };
        let payload = serde_json::json!({
            "key": collection_key,
            "version": current.version,
            "name": new_name,
            "parentCollection": new_parent,
        });

        let put_resp = self
            .state
            .send_with_retry(self.state.client.put(&url).json(&payload))
            .await?;
        let put_resp = self.ensure_success(put_resp).await?;
        if let Ok(collection) = put_resp.json::<ZoteroCollection>().await {
            Ok(collection)
        } else {
            let refetch =
                self.state.send_with_retry(self.state.client.get(&url)).await?;
            Ok(self.ensure_success(refetch).await?.json().await?)
        }
    }

    /// Renames tag `old_tag` to `new_tag` across every item in the library.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn rename_tag(
        &self,
        old_tag: &str,
        new_tag: &str,
    ) -> Result<usize, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let items = self.search_by_tag(old_tag, 100).await?;
        let mut count: usize = 0;
        for item in items {
            let new_tags = diff_tags(
                item.data.tags,
                std::slice::from_ref(&new_tag.to_owned()),
                std::slice::from_ref(&old_tag.to_owned()),
            );
            let patch =
                serde_json::json!({"tags": new_tags, "version": item.version});
            self.update_item(&item.key, patch).await?;
            count = count.saturating_add(1);
        }
        Ok(count)
    }

    /// Deletes up to 50 `tags` from the entire library in a single request.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::PermissionDenied`] if writes are disabled
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    pub(crate) async fn delete_tags(
        &self,
        tags: &[String],
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let version = self.get_library_version().await?;
        let joined = tags
            .iter()
            .map(|t| urlencoding::encode(t).into_owned())
            .collect::<Vec<_>>()
            .join(" || ");
        let url = format!(
            "{}/users/0/tags?tag={}",
            self.state.zotero_api_url, joined
        );
        self.delete(&url, version).await
    }
}

fn diff_tags(
    existing: Vec<ZoteroTag>,
    add: &[String],
    remove: &[String],
) -> Vec<serde_json::Value> {
    let mut tags_set: std::collections::BTreeSet<String> =
        existing.into_iter().map(|t| t.tag).collect();
    for a in add {
        tags_set.insert(a.clone());
    }
    for r in remove {
        tags_set.remove(r);
    }
    tags_set.into_iter().map(|t| serde_json::json!({ "tag": t })).collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::{
        super::client::tests::fixtures::{
            http_response, mock_server, test_state,
        },
        *,
    };

    #[tokio::test]
    async fn create_note_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err = ZoteroClient::new(&state)
            .create_note("PARENT1", "note body")
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn create_note_returns_created_item_on_success() {
        let created = json!([{
            "key": "NOTE1",
            "version": 1,
            "data": { "key": "NOTE1", "version": 1, "itemType": "note", "note": "note body" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let state = test_state(base, true);

        let item = ZoteroClient::new(&state)
            .create_note("PARENT1", "note body")
            .await
            .unwrap();
        assert_eq!(item.key, "NOTE1");
    }

    #[tokio::test]
    async fn create_collection_returns_created_collection_on_success() {
        let created = json!([{
            "key": "COL1",
            "version": 1,
            "data": { "key": "COL1", "name": "New Collection", "parentCollection": "false" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let state = test_state(base, true);

        let col = ZoteroClient::new(&state)
            .create_collection("New Collection", None)
            .await
            .unwrap();
        assert_eq!(col.key, "COL1");
    }

    #[tokio::test]
    async fn manage_collection_items_adds_and_removes() {
        let base = mock_server(vec![
            http_response("200 OK", ""),
            http_response("200 OK", ""),
        ]);
        let state = test_state(base, true);

        let res_add = ZoteroClient::new(&state)
            .manage_collection_items(
                "COL1",
                &["ITEM1".to_owned(), "ITEM2".to_owned()],
                false,
            )
            .await;
        assert!(res_add.is_ok());

        let res_rem = ZoteroClient::new(&state)
            .manage_collection_items("COL1", &["ITEM1".to_owned()], true)
            .await;
        assert!(res_rem.is_ok());
    }

    #[tokio::test]
    async fn update_item_updates_fields() {
        let item = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "title": "Updated Title" }
        });
        let base =
            mock_server(vec![http_response("200 OK", &item.to_string())]);
        let state = test_state(base, true);

        let res = ZoteroClient::new(&state)
            .update_item("ITEM1", json!({"title": "Updated Title"}))
            .await
            .unwrap();
        assert_eq!(res.key, "ITEM1");
    }

    #[tokio::test]
    async fn attach_file_link_creates_attachment() {
        let created = json!([{
            "key": "ATT1",
            "version": 1,
            "data": { "key": "ATT1", "version": 1, "itemType": "attachment", "title": "PDF Attachment" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let state = test_state(base, true);

        let item = ZoteroClient::new(&state)
            .attach_file_link("ITEM1", "PDF Attachment", "/path/file.pdf", None)
            .await
            .unwrap();
        assert_eq!(item.key, "ATT1");
    }

    #[tokio::test]
    async fn batch_update_tags_updates_tags_across_items() {
        let item = json!({
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Paper", "tags": [{ "tag": "old_tag" }] }
        });
        let updated = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "title": "Paper", "tags": [{ "tag": "new_tag" }] }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let state = test_state(base, true);

        let count = ZoteroClient::new(&state)
            .batch_update_tags(
                &["ITEM1".to_owned()],
                &["new_tag".to_owned()],
                &["old_tag".to_owned()],
            )
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn delete_item_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err =
            ZoteroClient::new(&state).delete_item("ITEM1").await.unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn delete_item_deletes_after_fetching_current_version() {
        let item = json!({
            "key": "ITEM1",
            "version": 7,
            "data": { "key": "ITEM1", "version": 7, "itemType": "journalArticle" }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("204 No Content", ""),
        ]);
        let state = test_state(base, true);

        let result = ZoteroClient::new(&state).delete_item("ITEM1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn set_item_deleted_true_marks_item_trashed() {
        let item = json!({
            "key": "ITEM1",
            "version": 7,
            "data": { "key": "ITEM1", "version": 7, "itemType": "journalArticle" }
        });
        let updated = json!({
            "key": "ITEM1",
            "version": 8,
            "data": { "key": "ITEM1", "version": 8, "itemType": "journalArticle", "deleted": true }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let state = test_state(base, true);

        let trashed = ZoteroClient::new(&state)
            .set_item_deleted("ITEM1", true)
            .await
            .unwrap();
        assert!(trashed.data.deleted);
    }

    #[tokio::test]
    async fn set_item_deleted_false_restores_item() {
        let item = json!({
            "key": "ITEM1",
            "version": 8,
            "data": { "key": "ITEM1", "version": 8, "itemType": "journalArticle", "deleted": true }
        });
        let updated = json!({
            "key": "ITEM1",
            "version": 9,
            "data": { "key": "ITEM1", "version": 9, "itemType": "journalArticle", "deleted": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let state = test_state(base, true);

        let restored = ZoteroClient::new(&state)
            .set_item_deleted("ITEM1", false)
            .await
            .unwrap();
        assert!(!restored.data.deleted);
    }

    #[tokio::test]
    async fn delete_collection_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err = ZoteroClient::new(&state)
            .delete_collection("COL1")
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn delete_collection_deletes_after_fetching_current_version() {
        let collection = json!({
            "key": "COL1",
            "version": 3,
            "data": { "key": "COL1", "name": "Old Collection", "parentCollection": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &collection.to_string()),
            http_response("204 No Content", ""),
        ]);
        let state = test_state(base, true);

        let result = ZoteroClient::new(&state).delete_collection("COL1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn create_annotation_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err = ZoteroClient::new(&state)
            .create_annotation(
                "ATT1",
                "highlight",
                Some("selected text"),
                None,
                None,
                None,
                r#"{"pageIndex":0,"rects":[[100,200,300,220]]}"#,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn create_annotation_returns_created_item_on_success() {
        let created = json!([{
            "key": "ANNOT1",
            "version": 1,
            "data": { "key": "ANNOT1", "version": 1, "itemType": "annotation", "annotationType": "highlight" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let state = test_state(base, true);

        let item = ZoteroClient::new(&state)
            .create_annotation(
                "ATT1",
                "highlight",
                Some("selected text"),
                None,
                None,
                None,
                r#"{"pageIndex":0,"rects":[[100,200,300,220]]}"#,
            )
            .await
            .unwrap();
        assert_eq!(item.key, "ANNOT1");
    }

    #[tokio::test]
    async fn create_annotation_rejects_invalid_position_json() {
        let state = test_state(mock_server(vec![]), true);

        let err = ZoteroClient::new(&state)
            .create_annotation(
                "ATT1",
                "highlight",
                Some("selected text"),
                None,
                None,
                None,
                "not json",
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::Json(_)));
    }

    #[tokio::test]
    async fn create_item_from_metadata_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err = ZoteroClient::new(&state)
            .create_item_from_metadata(
                json!({"itemType": "book", "title": "A Book"}),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn create_item_from_metadata_returns_created_item() {
        let created = json!([{
            "key": "NEWITEM1",
            "version": 1,
            "data": { "key": "NEWITEM1", "version": 1, "itemType": "book", "title": "A Book" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let state = test_state(base, true);

        let item = ZoteroClient::new(&state)
            .create_item_from_metadata(
                json!({"itemType": "book", "title": "A Book"}),
            )
            .await
            .unwrap();
        assert_eq!(item.key, "NEWITEM1");
    }

    #[tokio::test]
    async fn update_collection_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err = ZoteroClient::new(&state)
            .update_collection("COL1", Some("New Name"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn update_collection_renames_when_name_given() {
        let current = json!({
            "key": "COL1",
            "version": 3,
            "data": { "key": "COL1", "name": "Old Name", "parentCollection": false }
        });
        let updated = json!({
            "key": "COL1",
            "version": 4,
            "data": { "key": "COL1", "name": "New Name", "parentCollection": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &current.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let state = test_state(base, true);

        let collection = ZoteroClient::new(&state)
            .update_collection("COL1", Some("New Name"), None)
            .await
            .unwrap();
        assert_eq!(collection.data.name, "New Name");
    }

    #[tokio::test]
    async fn update_collection_reparents_when_parent_key_given() {
        let current = json!({
            "key": "COL1",
            "version": 3,
            "data": { "key": "COL1", "name": "Old Name", "parentCollection": false }
        });
        let updated = json!({
            "key": "COL1",
            "version": 4,
            "data": { "key": "COL1", "name": "Old Name", "parentCollection": "PARENT1" }
        });
        let base = mock_server(vec![
            http_response("200 OK", &current.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let state = test_state(base, true);

        let collection = ZoteroClient::new(&state)
            .update_collection("COL1", None, Some("PARENT1"))
            .await
            .unwrap();
        assert_eq!(collection.data.parent_collection, Some(json!("PARENT1")));
    }

    #[tokio::test]
    async fn update_collection_moves_to_top_level_when_parent_key_is_empty_string()
     {
        let current = json!({
            "key": "COL1",
            "version": 3,
            "data": { "key": "COL1", "name": "Old Name", "parentCollection": "PARENT1" }
        });
        let updated = json!({
            "key": "COL1",
            "version": 4,
            "data": { "key": "COL1", "name": "Old Name", "parentCollection": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &current.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let state = test_state(base, true);

        let collection = ZoteroClient::new(&state)
            .update_collection("COL1", None, Some(""))
            .await
            .unwrap();
        assert_eq!(collection.data.parent_collection, Some(json!(false)));
    }

    #[tokio::test]
    async fn rename_tag_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err = ZoteroClient::new(&state)
            .rename_tag("old_tag", "new_tag")
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn rename_tag_swaps_tag_across_matching_items() {
        let items = json!([
            {
                "key": "ITEM1",
                "version": 1,
                "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "tags": [{ "tag": "old_tag" }] }
            },
            {
                "key": "ITEM2",
                "version": 1,
                "data": { "key": "ITEM2", "version": 1, "itemType": "journalArticle", "tags": [{ "tag": "old_tag" }] }
            }
        ]);
        let patch1 = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "tags": [{ "tag": "new_tag" }] }
        });
        let patch2 = json!({
            "key": "ITEM2",
            "version": 2,
            "data": { "key": "ITEM2", "version": 2, "itemType": "journalArticle", "tags": [{ "tag": "new_tag" }] }
        });
        let base = mock_server(vec![
            http_response("200 OK", &items.to_string()),
            http_response("200 OK", &patch1.to_string()),
            http_response("200 OK", &patch2.to_string()),
        ]);
        let state = test_state(base, true);

        let count = ZoteroClient::new(&state)
            .rename_tag("old_tag", "new_tag")
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn delete_tags_rejects_when_write_is_disabled() {
        let state = test_state(String::new(), false);
        let err = ZoteroClient::new(&state)
            .delete_tags(&["old_tag".to_owned()])
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn delete_tags_sends_joined_tag_query_and_succeeds() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        std::thread::spawn(move || {
            let (mut stream, _) =
                listener.accept().expect("accept version request");
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);
            let version_resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\
                                 Content-Type: application/json\r\n\
                                 Last-Modified-Version: 9\r\n\
                                 Connection: close\r\n\r\n[]";
            let _ = stream.write_all(version_resp.as_bytes());

            let (mut stream2, _) =
                listener.accept().expect("accept delete request");
            let mut buf2 = [0_u8; 1024];
            let _ = stream2.read(&mut buf2);
            let _ = stream2.write_all(
                "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"
                    .as_bytes(),
            );
        });
        let state = test_state(format!("http://{addr}"), true);

        let result = ZoteroClient::new(&state)
            .delete_tags(&["old_tag".to_owned(), "other tag".to_owned()])
            .await;
        assert!(result.is_ok());
    }
}
