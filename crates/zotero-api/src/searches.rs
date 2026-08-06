//! Saved searches management and local execution endpoint wrapper
//! (`<prefix>/searches`).

use serde::{Deserialize, Serialize};

use crate::{
    client::ZoteroClient,
    errors::ZoteroApiError,
    keys::LibraryVersion,
    objects::{BatchWriteResponse, ZoteroItem},
};

/// Saved search object returned by `GET <prefix>/searches`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SavedSearch {
    /// 8-character search key identifier.
    pub key: String,
    /// Library version counter.
    pub version: LibraryVersion,
    /// Human-readable search name.
    pub name: String,
    /// Query condition definitions.
    #[serde(default)]
    pub conditions: Vec<serde_json::Value>,
}

impl ZoteroClient<'_> {
    /// Lists all saved searches in the target library.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status.
    /// - [`Network`]: Transport errors.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    #[inline]
    pub async fn list_searches(
        &self,
    ) -> Result<Vec<SavedSearch>, ZoteroApiError> {
        let url = format!(
            "{}{}/searches",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.get_json(&url).await
    }

    /// Fetches a single saved search definition by key.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status.
    /// - [`Network`]: Transport errors.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    #[inline]
    pub async fn get_search(
        &self,
        key: &str,
    ) -> Result<SavedSearch, ZoteroApiError> {
        let url = format!(
            "{}{}/searches/{key}",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.get_json(&url).await
    }

    /// Executes a saved search and returns matching items.
    ///
    /// Special Local API capability hitting `GET
    /// <prefix>/searches/<key>/items`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status.
    /// - [`Network`]: Transport errors.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    #[inline]
    pub async fn execute_saved_search(
        &self,
        key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroApiError> {
        let url = format!(
            "{}{}/searches/{key}/items",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.get_json(&url).await
    }

    /// Creates saved searches in a batch via `POST <prefix>/searches`.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`]: If write operations are disabled.
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status.
    /// - [`Network`]: Transport errors.
    ///
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    #[inline]
    pub async fn create_searches(
        &self,
        searches: &[serde_json::Value],
    ) -> Result<BatchWriteResponse, ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/searches",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let req = self
            .apply_auth_headers(self.state.client().post(&url).json(searches));
        let resp = self.state.send_with_retry(req).await?;
        Ok(self.ensure_success(resp).await?.json().await?)
    }

    /// Deletes saved searches by key via `DELETE
    /// <prefix>/searches?searchKey=K1,K2,...`.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`]: If write operations are disabled.
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status.
    /// - [`Network`]: Transport errors.
    ///
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    #[inline]
    pub async fn delete_searches(
        &self,
        keys: &[String],
        version: LibraryVersion,
    ) -> Result<(), ZoteroApiError> {
        self.state.check_write_permission()?;
        let keys_str = keys.join(",");
        let url = format!(
            "{}{}/searches?searchKey={keys_str}",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.delete(&url, version).await
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        client::test_http::{MockServer, http_response},
        state::AppState,
    };

    #[tokio::test]
    async fn parses_list_searches_response() {
        let json_resp = serde_json::json!([
            {
                "key": "SEARCH01",
                "version": 1,
                "name": "Recent Quantum Papers",
                "conditions": [{"field": "title", "operator": "contains", "value": "quantum"}]
            }
        ])
        .to_string();

        let server = MockServer::new(vec![http_response("200 OK", &json_resp)]);
        let state = AppState::test_default().with_zotero_api_url(server.url());
        let client = ZoteroClient::new(&state);

        let searches = client.list_searches().await.unwrap();
        assert_eq!(searches.len(), 1);
        assert_eq!(searches.first().unwrap().name, "Recent Quantum Papers");
    }
}
