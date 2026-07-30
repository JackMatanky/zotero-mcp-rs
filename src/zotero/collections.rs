//! Collection operations for the Zotero Local HTTP API.

use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    zotero::{
        client::ZoteroClient,
        models::{CollectionKey, ItemKey, ZoteroCollection, ZoteroItem},
    },
};

/// Action for adding or removing items to or from a collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CollectionItemAction {
    Add,
    Remove,
}

impl ZoteroClient<'_> {
    /// Fetches every collection in the library.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_collections(
        &self,
    ) -> Result<Vec<ZoteroCollection>, ZoteroMcpError> {
        let url = format!("{}/users/0/collections", self.state.zotero_api_url);
        self.get_json(&url).await
    }

    /// Searches collections by `query`, matching collection names
    /// case-insensitively.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn search_collections(
        &self,
        query: &str,
    ) -> Result<Vec<ZoteroCollection>, ZoteroMcpError> {
        let collections = self.get_collections().await?;
        let query_lc = query.to_lowercase();
        let filtered = collections
            .into_iter()
            .filter(|c| c.data.name.to_lowercase().contains(&query_lc))
            .collect();
        Ok(filtered)
    }

    /// Fetches every item inside the collection identified by
    /// `collection_key`.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn get_collection_items(
        &self,
        collection_key: &CollectionKey,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/collections/{}/items",
            self.state.zotero_api_url, collection_key
        );
        self.get_json(&url).await
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
        parent_key: Option<&CollectionKey>,
    ) -> Result<ZoteroCollection, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/collections", self.state.zotero_api_url);
        let parent_val = match parent_key {
            Some(k) => serde_json::Value::String(k.to_string()),
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
    /// - `action`: Action to perform (`CollectionItemAction::Add` or
    ///   `CollectionItemAction::Remove`).
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
        collection_key: &CollectionKey,
        item_keys: &[ItemKey],
        action: CollectionItemAction,
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}/users/0/collections/{}/items",
            self.state.zotero_api_url, collection_key
        );
        let body_str =
            item_keys.iter().map(ItemKey::as_str).collect::<Vec<_>>().join(" ");

        let req = match action {
            CollectionItemAction::Remove => {
                self.state.client.delete(&url).body(body_str)
            }
            CollectionItemAction::Add => {
                self.state.client.post(&url).body(body_str)
            }
        };

        self.ensure_success(self.state.send_with_retry(req).await?).await?;
        Ok(())
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
        collection_key: &CollectionKey,
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
        self.delete(&url, collection.version.into()).await
    }

    /// Renames and/or moves a collection identified by `collection_key`.
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
        collection_key: &CollectionKey,
        name: Option<&str>,
        parent_key: Option<&CollectionKey>,
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
            Some(k) if k.as_str().is_empty() => serde_json::Value::Bool(false),
            Some(k) => serde_json::Value::String(k.to_string()),
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
}
