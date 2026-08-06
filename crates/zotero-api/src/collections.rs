//! Collection operations for the Zotero Local HTTP API.
//!
//! Adds [`ZoteroClient`] methods for collection browsing, search, creation,
//! updates, deletion, and item membership changes.
//!
//! # Key operations and types
//!
//! - [`ZoteroClient::get_collections`] and
//!   [`ZoteroClient::search_collections`]: browse or search collection trees.
//! - [`ZoteroClient::create_collection`] and
//!   [`ZoteroClient::update_collection`]: create, rename, or move collections.
//! - [`ZoteroClient::manage_collection_items`]: add or remove items using
//!   [`CollectionItemAction`].
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_api::AppState;
//! # use zotero_api::ZoteroClient;
//! # async fn run(
//! #     state: &AppState,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let client = ZoteroClient::new(state);
//! let collections = client.get_collections().await?;
//! println!("Found {} collections", collections.len(),);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};

use crate::{
    client::ZoteroClient,
    errors::ZoteroApiError,
    keys::{CollectionKey, ItemKey, LibraryVersion},
    objects::{ZoteroCollection, ZoteroItem},
    types::CollectionParent,
};

/// Action for adding or removing items to or from a collection.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum CollectionItemAction {
    /// Add items to the target collection.
    Add,
    /// Remove items from the target collection.
    Remove,
}

impl ZoteroClient<'_> {
    /// Fetches all collections defined in the library scope, returning the full
    /// collection tree.
    ///
    /// Queries `GET <prefix>/collections`. Returns a list of
    /// [`ZoteroCollection`] records containing collection metadata (keys,
    /// names, and parent collection relationships).
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if a transport-level error occurs.
    /// - [`ZoteroApiError::Json`] if the collection list fails JSON decoding.
    #[inline]
    pub async fn get_collections(
        &self,
    ) -> Result<Vec<ZoteroCollection>, ZoteroApiError> {
        let url = format!(
            "{}{}/collections",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.get_json(&url).await
    }

    /// Searches collections by matching `query` against collection names
    /// case-insensitively.
    ///
    /// Fetches all collections via [`get_collections`](Self::get_collections)
    /// and filters the list in memory, returning collections whose name
    /// contains `query`.
    ///
    /// # Arguments
    ///
    /// * `query` - Substring to match against collection names.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if fetching collections fails.
    /// - [`ZoteroApiError::Network`] if transport errors occur.
    /// - [`ZoteroApiError::Json`] if JSON decoding fails.
    #[inline]
    pub async fn search_collections(
        &self,
        query: &str,
    ) -> Result<Vec<ZoteroCollection>, ZoteroApiError> {
        let collections = self.get_collections().await?;
        let query_lc = query.to_lowercase();
        let filtered = collections
            .into_iter()
            .filter(|c| c.data.name.to_lowercase().contains(&query_lc))
            .collect();
        Ok(filtered)
    }

    /// Fetches every item contained within the collection identified by
    /// `collection_key`.
    ///
    /// Queries `GET <prefix>/collections/<collection_key>/items`. Returns a
    /// list of [`ZoteroItem`] structures stored inside the target
    /// collection.
    ///
    /// # Arguments
    ///
    /// * `collection_key` - Eight-character key of the target collection.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx HTTP status.
    /// - [`ZoteroApiError::Network`] if a network transport failure occurs.
    /// - [`ZoteroApiError::Json`] if the response body cannot be decoded into
    ///   items.
    #[inline]
    pub async fn get_collection_items(
        &self,
        collection_key: &CollectionKey,
    ) -> Result<Vec<ZoteroItem>, ZoteroApiError> {
        let url = format!(
            "{}{}/collections/{}/items",
            self.state.zotero_api_url(),
            self.target_prefix(),
            collection_key
        );
        self.get_json(&url).await
    }

    /// Creates a new collection with the given `name` and optional
    /// `parent_key`.
    ///
    /// Checks write permissions via
    /// [`AppState::check_write_permission`](crate::state::AppState::check_write_permission)
    /// and posts a single-element creation array to `POST
    /// <prefix>/collections`.
    ///
    /// # Arguments
    ///
    /// * `name` - Display name for the new collection.
    /// * `parent_key` - Optional parent collection key; [`None`] creates a
    ///   top-level collection.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled
    ///   in [`AppState`](crate::state::AppState).
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the collection creation
    ///   request.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if the created collection payload fails
    ///   deserialization.
    #[inline]
    pub async fn create_collection(
        &self,
        name: &str,
        parent_key: Option<&CollectionKey>,
    ) -> Result<ZoteroCollection, ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/collections",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let parent_val = parent_key.map_or(CollectionParent::TopLevel, |key| {
            CollectionParent::Parent(key.clone())
        });
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

    /// Adds items to or removes items from a collection without modifying item
    /// metadata.
    ///
    /// Verifies write permissions and formats `item_keys` as a space-separated
    /// string body. Sends `POST
    /// <prefix>/collections/<collection_key>/items` to add items, or
    /// `DELETE <prefix>/collections/<collection_key>/items` to remove items.
    ///
    /// # Arguments
    ///
    /// * `collection_key` - Target collection key.
    /// * `item_keys` - Slice of item keys to add or remove.
    /// * `action` - [`CollectionItemAction::Add`] to add items, or
    ///   [`CollectionItemAction::Remove`] to remove items.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn manage_collection_items(
        &self,
        collection_key: &CollectionKey,
        item_keys: &[ItemKey],
        action: CollectionItemAction,
    ) -> Result<(), ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/collections/{}/items",
            self.state.zotero_api_url(),
            self.target_prefix(),
            collection_key
        );
        let body_str =
            item_keys.iter().map(ItemKey::as_str).collect::<Vec<_>>().join(" ");

        let req = match action {
            CollectionItemAction::Remove => {
                self.state.client().delete(&url).body(body_str)
            }
            CollectionItemAction::Add => {
                self.state.client().post(&url).body(body_str)
            }
        };

        self.ensure_success(self.state.send_with_retry(req).await?).await?;
        Ok(())
    }

    /// Permanently deletes the collection identified by `collection_key`.
    ///
    /// Fetches the collection to determine its current version, then issues a
    /// `DELETE` request with an `If-Unmodified-Since-Version` header. Items
    /// inside the deleted collection are retained in the library (unfiled).
    ///
    /// # Arguments
    ///
    /// * `collection_key` - Key of the collection to delete.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::NotFound`] if the collection does not exist.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the deletion request
    ///   (e.g. 412 version conflict).
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn delete_collection(
        &self,
        collection_key: &CollectionKey,
    ) -> Result<(), ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/collections/{}",
            self.state.zotero_api_url(),
            self.target_prefix(),
            collection_key
        );
        let resp = self
            .ensure_success(
                self.state
                    .send_with_retry(self.state.client().get(&url))
                    .await?,
            )
            .await?;
        let collection: ZoteroCollection = resp.json().await?;
        self.delete(&url, collection.version).await
    }

    /// Renames a collection and/or moves it to a new parent collection
    /// location.
    ///
    /// Fetches current collection data, applies updated `name` and/or `parent`
    /// fields, and submits a `PUT` request to
    /// `<prefix>/collections/<collection_key>`. If Zotero returns an empty
    /// body, refetches the updated collection.
    ///
    /// # Arguments
    ///
    /// * `collection_key` - Key of the collection to update.
    /// * `name` - Optional new display name for the collection.
    /// * `parent` - Optional new parent collection location
    ///   ([`CollectionParent::TopLevel`] or [`CollectionParent::Parent`]).
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::NotFound`] if the collection does not exist.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the update.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn update_collection(
        &self,
        collection_key: &CollectionKey,
        name: Option<&str>,
        parent: Option<&CollectionParent>,
    ) -> Result<ZoteroCollection, ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/collections/{}",
            self.state.zotero_api_url(),
            self.target_prefix(),
            collection_key
        );
        let resp = self
            .ensure_success(
                self.state
                    .send_with_retry(self.state.client().get(&url))
                    .await?,
            )
            .await?;
        let current: ZoteroCollection = resp.json().await?;

        let new_name = name.unwrap_or(&current.data.name);
        let new_parent = parent
            .cloned()
            .unwrap_or_else(|| current.data.parent_collection.clone());
        let payload = serde_json::json!({
            "key": collection_key,
            "version": current.version,
            "name": new_name,
            "parentCollection": new_parent,
        });

        let put_resp = self
            .state
            .send_with_retry(self.state.client().put(&url).json(&payload))
            .await?;
        let put_resp = self.ensure_success(put_resp).await?;
        if let Ok(collection) = put_resp.json::<ZoteroCollection>().await {
            Ok(collection)
        } else {
            let refetch = self
                .state
                .send_with_retry(self.state.client().get(&url))
                .await?;
            Ok(self.ensure_success(refetch).await?.json().await?)
        }
    }

    /// Batch-deletes multiple collections by key in a single request via
    /// `DELETE <prefix>/collections?collectionKey=K1,K2,...`.
    ///
    /// Verifies write permissions and issues a comma-separated key deletion
    /// query with optimistic version header validation
    /// (`If-Unmodified-Since-Version`).
    ///
    /// # Arguments
    ///
    /// * `keys` - Slice of collection keys to delete.
    /// * `version` - Current library version required for concurrency
    ///   validation.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code
    ///   (e.g. 412 version conflict).
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn delete_collections(
        &self,
        keys: &[CollectionKey],
        version: LibraryVersion,
    ) -> Result<(), ZoteroApiError> {
        self.state.check_write_permission()?;
        let keys_str = keys
            .iter()
            .map(CollectionKey::as_str)
            .collect::<Vec<_>>()
            .join(",");
        let url = format!(
            "{}{}/collections?collectionKey={keys_str}",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.delete(&url, version).await
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::{
        client::{
            ZoteroClient,
            test_http::{MockServer, http_response, request_body},
        },
        keys::LibraryVersion,
        state::AppState,
    };

    fn state(zotero_api_url: impl AsRef<str>, write_enabled: bool) -> AppState {
        AppState::test_default()
            .with_zotero_api_url(zotero_api_url.as_ref())
            .with_write_enabled(write_enabled)
    }

    fn collection_json(
        key: &str,
        name: &str,
        version: u64,
        parent: &serde_json::Value,
    ) -> String {
        json!({"key":key,"version":version,"data":{"key":key,"name":name,"parentCollection":parent.clone()}}).to_string()
    }

    #[tokio::test]
    async fn search_collections_matches_names_case_insensitively() {
        let body = format!(
            "[{},{}]",
            collection_json("COLL0001", "Machine Learning", 1, &json!(false)),
            collection_json("COLL0002", "Other", 1, &json!(false))
        );
        let server = MockServer::new(vec![http_response("200 OK", &body)]);
        let app = state(server.url(), false);

        let result =
            ZoteroClient::new(&app).search_collections("machine").await;

        assert!(result.is_ok(), "collections should decode: {result:?}");
        let collections = result.unwrap_or_default();
        assert_eq!(
            collections
                .iter()
                .map(|collection| collection.key.as_str())
                .collect::<Vec<_>>(),
            vec!["COLL0001"]
        );
    }

    #[tokio::test]
    async fn create_collection_serializes_top_level_and_parent_collection() {
        let (server, recorded) = MockServer::recording(vec![
            http_response(
                "200 OK",
                &format!(
                    "[{}]",
                    collection_json("TOP00001", "Top", 1, &json!(false))
                ),
            ),
            http_response(
                "200 OK",
                &format!(
                    "[{}]",
                    collection_json("CHILD001", "Child", 1, &json!("PARENT01"))
                ),
            ),
        ]);
        let app = state(server.url(), true);
        let client = ZoteroClient::new(&app);

        let top = client.create_collection("Top", None).await;
        let child = client
            .create_collection("Child", Some(&CollectionKey::from("PARENT01")))
            .await;

        assert!(top.is_ok(), "top-level collection should be created: {top:?}");
        assert!(child.is_ok(), "child collection should be created: {child:?}");
        let requests = recorded.lock().expect("request log lock");
        let top_body = requests
            .first()
            .and_then(|request| request_body(request).ok())
            .unwrap_or_default();
        let child_body = requests
            .get(1)
            .and_then(|request| request_body(request).ok())
            .unwrap_or_default();
        assert_eq!(
            top_body.get(0).and_then(|item| item.get("parentCollection")),
            Some(&json!(false))
        );
        assert_eq!(
            child_body.get(0).and_then(|item| item.get("parentCollection")),
            Some(&json!("PARENT01"))
        );
    }

    #[tokio::test]
    async fn manage_collection_items_uses_post_for_add_and_delete_for_remove() {
        let (server, recorded) = MockServer::recording(vec![
            http_response("204 No Content", ""),
            http_response("204 No Content", ""),
        ]);
        let app = state(server.url(), true);
        let client = ZoteroClient::new(&app);
        let keys = [ItemKey::from("ITEM0001"), ItemKey::from("ITEM0002")];

        let added = client
            .manage_collection_items(
                &CollectionKey::from("COLL0001"),
                &keys,
                CollectionItemAction::Add,
            )
            .await;
        let removed = client
            .manage_collection_items(
                &CollectionKey::from("COLL0001"),
                &keys,
                CollectionItemAction::Remove,
            )
            .await;

        assert!(added.is_ok(), "add items should succeed: {added:?}");
        assert!(removed.is_ok(), "remove items should succeed: {removed:?}");
        let requests = recorded.lock().expect("request log lock");
        assert!(
            requests.first().is_some_and(|request| request
                .starts_with("POST /users/0/collections/COLL0001/items")),
            "add should POST: {requests:?}"
        );
        assert!(
            requests.get(1).is_some_and(|request| request
                .starts_with("DELETE /users/0/collections/COLL0001/items")),
            "remove should DELETE: {requests:?}"
        );
        assert!(
            requests
                .first()
                .is_some_and(|request| request.contains("ITEM0001 ITEM0002")),
            "body should contain space-separated keys: {requests:?}"
        );
    }

    #[tokio::test]
    async fn delete_collection_reads_version_then_deletes() {
        let (server, recorded) = MockServer::recording(vec![
            http_response(
                "200 OK",
                &collection_json("COLL0001", "Delete", 9, &json!(false)),
            ),
            http_response("204 No Content", ""),
        ]);
        let app = state(server.url(), true);

        let result = ZoteroClient::new(&app)
            .delete_collection(&CollectionKey::from("COLL0001"))
            .await;

        assert!(
            result.is_ok(),
            "delete should succeed after reading version: {result:?}"
        );
        let requests = recorded.lock().expect("request log lock");
        assert!(
            requests.first().is_some_and(|request| request
                .starts_with("GET /users/0/collections/COLL0001")),
            "delete should read collection first: {requests:?}"
        );
        assert!(
            requests.get(1).is_some_and(|request| request
                .starts_with("DELETE /users/0/collections/COLL0001")),
            "delete should send DELETE second: {requests:?}"
        );
        assert!(
            requests
                .get(1)
                .is_some_and(|request| request
                    .contains("if-unmodified-since-version: 9")),
            "DELETE should carry collection version: {requests:?}"
        );
    }

    #[tokio::test]
    async fn update_collection_refetches_when_put_response_is_empty() {
        let (server, recorded) = MockServer::recording(vec![
            http_response(
                "200 OK",
                &collection_json("COLL0001", "Old", 3, &json!(false)),
            ),
            http_response("200 OK", ""),
            http_response(
                "200 OK",
                &collection_json("COLL0001", "New", 4, &json!("PARENT01")),
            ),
        ]);
        let app = state(server.url(), true);

        let result = ZoteroClient::new(&app)
            .update_collection(
                &CollectionKey::from("COLL0001"),
                Some("New"),
                Some(&CollectionParent::Parent(CollectionKey::from(
                    "PARENT01",
                ))),
            )
            .await;

        assert!(
            result.is_ok(),
            "empty PUT response should refetch collection: {result:?}"
        );
        let collection = result.expect("asserted Ok above");
        assert_eq!(collection.data.name, "New");
        assert_eq!(collection.version, LibraryVersion(4));
        let requests = recorded.lock().expect("request log lock");
        assert!(
            requests.first().is_some_and(|request| request
                .starts_with("GET /users/0/collections/COLL0001")),
            "first request should read current collection: {requests:?}"
        );
        assert!(
            requests.get(1).is_some_and(|request| request
                .starts_with("PUT /users/0/collections/COLL0001")),
            "second request should PUT update: {requests:?}"
        );
        assert!(
            requests.get(2).is_some_and(|request| request
                .starts_with("GET /users/0/collections/COLL0001")),
            "third request should refetch after empty PUT: {requests:?}"
        );
    }
}
