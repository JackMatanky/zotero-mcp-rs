//! Deleted library object synchronization.
//!
//! Provides response structures and client methods for querying deleted items,
//! collections, searches, and tags since a given library version.
//!
//! # Key Types
//!
//! - [`DeletedObjectsResponse`]: Container of deleted keys grouped by type.
//!
//! # Examples
//!
//! ```no_run
//! use zotero_api::{AppState, LibraryVersion, ZoteroClient};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let state = AppState::from_env();
//! let client = ZoteroClient::new(&state);
//! let deleted = client.get_deleted(LibraryVersion::from(100)).await?;
//! println!("Deleted items: {:?}", deleted.items);
//! # Ok(())
//! # }
//! ```
use serde::{Deserialize, Serialize};

use crate::{
    client::ZoteroClient, errors::ZoteroApiError, keys::LibraryVersion,
};

/// Response object from `GET <prefix>/deleted?since=<version>`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct DeletedObjectsResponse {
    /// Deleted collection keys.
    #[serde(default)]
    pub collections: Vec<String>,
    /// Deleted saved search keys.
    #[serde(default)]
    pub searches: Vec<String>,
    /// Deleted item keys.
    #[serde(default)]
    pub items: Vec<String>,
    /// Deleted tag strings.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ZoteroClient<'_> {
    /// Retrieves deleted library objects (items, collections, searches, tags)
    /// since `since`.
    ///
    /// # Errors
    ///
    /// Returns [`ZoteroApiError`] if fetching deleted objects fails.
    #[inline]
    pub async fn get_deleted(
        &self,
        since: LibraryVersion,
    ) -> Result<DeletedObjectsResponse, ZoteroApiError> {
        let url = format!(
            "{}{}/deleted?since={since}",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        self.get_json(&url).await
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
    async fn parses_deleted_objects_response() {
        let json_resp = serde_json::json!({
            "collections": ["C1"],
            "searches": [],
            "items": ["I1", "I2"],
            "tags": ["tag1"]
        })
        .to_string();

        let server = MockServer::new(vec![http_response("200 OK", &json_resp)]);
        let state = AppState::test_default().with_zotero_api_url(server.url());
        let client = ZoteroClient::new(&state);

        let deleted = client.get_deleted(LibraryVersion(10)).await.unwrap();
        assert_eq!(deleted.items, vec!["I1".to_owned(), "I2".to_owned()]);
        assert_eq!(deleted.collections, vec!["C1".to_owned()]);
        assert_eq!(deleted.tags, vec!["tag1".to_owned()]);
    }
}
