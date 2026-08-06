//! Saved search management and execution.
//!
//! Provides types and client methods for retrieving, executing, creating, and
//! deleting saved searches in a Zotero library.
//!
//! # Key Types
//!
//! - [`SavedSearch`]: Saved search query representation.
//!
//! # Examples
//!
//! ```no_run
//! use zotero_api::{AppState, ZoteroClient};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let state = AppState::from_env();
//! let client = ZoteroClient::new(&state);
//! let searches = client.list_searches().await?;
//! for s in searches {
//!     println!("Saved search: {} ({})", s.name, s.key);
//! }
//! # Ok(())
//! # }
//! ```

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
    /// Lists all saved search filters configured in the target library.
    ///
    /// Queries `GET <prefix>/searches`. Returns a list of [`SavedSearch`]
    /// objects containing search keys, display names, and condition arrays.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero responds with a non-2xx status.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if saved search payload decoding fails.
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

    /// Fetches a single saved search definition by its unique search key.
    ///
    /// Queries `GET <prefix>/searches/<key>`.
    ///
    /// # Arguments
    ///
    /// * `key` - Eight-character saved search key identifier.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if the saved search object cannot be decoded.
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

    /// Executes a saved search on the Zotero server and returns matching items.
    ///
    /// Queries `GET <prefix>/searches/<key>/items`, leveraging Zotero desktop's
    /// server-side search engine.
    ///
    /// # Arguments
    ///
    /// * `key` - Eight-character key of the saved search to execute.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if response item payload decoding fails.
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

    /// Batch-creates new saved search definitions in the library.
    ///
    /// Verifies write permissions and issues a `POST` request to
    /// `<prefix>/searches` with an array of search definition payloads.
    ///
    /// # Arguments
    ///
    /// * `searches` - Slice of raw JSON search objects (name and conditions).
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled
    ///   in [`AppState`](crate::state::AppState).
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects the creation request.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if response decoding fails.
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

    /// Batch-deletes saved searches by key in a single request.
    ///
    /// Verifies write permissions and issues `DELETE
    /// <prefix>/searches?searchKey=K1,K2,...` with optimistic version
    /// concurrency validation.
    ///
    /// # Arguments
    ///
    /// * `keys` - Slice of saved search key strings to delete.
    /// * `version` - Current library version required for concurrency checks.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx status code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
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
