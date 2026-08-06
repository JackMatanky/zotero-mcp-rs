//! Tag operations for the Zotero Local HTTP API.
//!
//! Provides methods on [`ZoteroClient`] for listing library tags, batch
//! updating tags across items, renaming tags library-wide, and deleting tags.
//!
//! # Key Operations
//!
//! - [`ZoteroClient::list_tags`]: List library-wide tag names.
//! - [`ZoteroClient::batch_update_tags`]: Add or remove tags across items.
//! - [`ZoteroClient::rename_tag`]: Rename a tag across all items in the
//!   library.
//! - [`ZoteroClient::delete_tags`]: Delete tags library-wide.
//!
//! ```no_run
//! # use zotero_api::errors::ZoteroApiError;
//! # use zotero_api::AppState;
//! # use zotero_api::ZoteroClient;
//! # async fn example() -> Result<(), ZoteroApiError> {
//! let state = AppState::from_env();
//! let client = ZoteroClient::new(&state);
//! let tags = client.list_tags(50).await?;
//! println!("Found {} tags", tags.len());
//! # Ok(())
//! # }
//! ```

use std::collections::BTreeSet;

use crate::{
    client::ZoteroClient,
    errors::ZoteroApiError,
    keys::{ItemKey, TagName},
    objects::ZoteroTag,
};

impl ZoteroClient<'_> {
    /// Lists all tag names present in the library, returning up to `limit` tag
    /// strings.
    ///
    /// Queries `GET <prefix>/tags?limit=<limit>`. Extracts and deduplicates the
    /// `tag` property string array returned by Zotero.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of tag names to fetch.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::LocalApi`] if Zotero responds with a non-2xx status
    ///   code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    /// - [`ZoteroApiError::Json`] if tag array decoding fails.
    #[inline]
    pub async fn list_tags(
        &self,
        limit: usize,
    ) -> Result<Vec<TagName>, ZoteroApiError> {
        let url = format!(
            "{}{}/tags?limit={}",
            self.state.zotero_api_url(),
            self.target_prefix(),
            limit
        );
        let raw: Vec<serde_json::Value> = self.get_json(&url).await?;
        Ok(raw
            .into_iter()
            .filter_map(|v| {
                v.get("tag").and_then(|t| t.as_str()).map(TagName::from)
            })
            .collect())
    }

    /// Batch-updates tags across multiple items by adding and removing tag
    /// lists.
    ///
    /// Iterates over `item_keys`, fetches each item's current tag list,
    /// computes set differences (`current + add_tags - remove_tags`), and
    /// patches each item in Zotero.
    ///
    /// # Arguments
    ///
    /// * `item_keys` - Slice of item keys to modify.
    /// * `add_tags` - Tag names to attach to each item.
    /// * `remove_tags` - Tag names to strip from each item.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::NotFound`] if any item key does not exist.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects any item tag update.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn batch_update_tags(
        &self,
        item_keys: &[ItemKey],
        add_tags: &[TagName],
        remove_tags: &[TagName],
    ) -> Result<usize, ZoteroApiError> {
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

    /// Renames a tag from `old_tag` to `new_tag` across all matching items in
    /// the library target.
    ///
    /// Queries items matching `old_tag` via
    /// [`search_by_tag`](Self::search_by_tag) and patches each
    /// matching item to add `new_tag` and remove `old_tag`. Returns the number
    /// of updated items.
    ///
    /// # Arguments
    ///
    /// * `old_tag` - Existing tag name to replace.
    /// * `new_tag` - Replacement tag name to assign.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero rejects tag update requests.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn rename_tag(
        &self,
        old_tag: &TagName,
        new_tag: &TagName,
    ) -> Result<usize, ZoteroApiError> {
        self.state.check_write_permission()?;
        let items = self.search_by_tag(old_tag, 100).await?;
        let mut count: usize = 0;
        for item in items {
            let new_tags = diff_tags(
                item.data.tags,
                std::slice::from_ref(new_tag),
                std::slice::from_ref(old_tag),
            );
            let patch =
                serde_json::json!({"tags": new_tags, "version": item.version});
            self.update_item(&item.key, patch).await?;
            count = count.saturating_add(1);
        }
        Ok(count)
    }

    /// Deletes up to 50 tag names from the entire library in a single request.
    ///
    /// Issues `DELETE <prefix>/tags?tag=<joined_tags>` with URL-encoded tag
    /// names separated by ` || `, passing the current library version
    /// header for optimistic concurrency check.
    ///
    /// # Arguments
    ///
    /// * `tags` - Slice of tag names to delete from the library.
    ///
    /// # Errors
    ///
    /// - [`ZoteroApiError::PermissionDenied`] if write permission is disabled.
    /// - [`ZoteroApiError::LocalApi`] if Zotero returns a non-2xx HTTP status
    ///   code.
    /// - [`ZoteroApiError::Network`] if transport failures occur.
    #[inline]
    pub async fn delete_tags(
        &self,
        tags: &[TagName],
    ) -> Result<(), ZoteroApiError> {
        self.state.check_write_permission()?;
        let version = self.get_library_version().await?;
        let joined = tags
            .iter()
            .map(|t| urlencoding::encode(t.as_str()).into_owned())
            .collect::<Vec<_>>()
            .join(" || ");
        let url = format!(
            "{}{}/tags?tag={}",
            self.state.zotero_api_url(),
            self.target_prefix(),
            joined
        );
        self.delete(&url, version).await
    }
}

/// Computes the updated tag array for an item after applying additions and
/// removals.
///
/// # Arguments
///
/// * `existing` - Current tag objects attached to the item
/// * `add` - Tag names to attach to the item
/// * `remove` - Tag names to strip from the item
pub(crate) fn diff_tags(
    existing: Vec<ZoteroTag>,
    add: &[TagName],
    remove: &[TagName],
) -> Vec<serde_json::Value> {
    let mut tags_set: BTreeSet<TagName> =
        existing.into_iter().map(|t| t.tag).collect();
    tags_set.extend(add.iter().cloned());
    for r in remove {
        tags_set.remove(r);
    }
    tags_set
        .into_iter()
        .map(|t| serde_json::json!({ "tag": t.as_str() }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TagOrigin;

    mod diff_tags {
        use pretty_assertions::assert_eq;

        use super::*;
        #[test]
        fn adds_new_tags_and_removes_specified_existing_tags() {
            let existing = vec![ZoteroTag {
                tag: TagName::from("old"),
                origin: TagOrigin::default(),
            }];
            let add = vec![TagName::from("new")];
            let remove = vec![TagName::from("old")];

            let result = super::diff_tags(existing, &add, &remove);

            assert_eq!(result.len(), 1);
            assert_eq!(
                result.first().and_then(|v| v.get("tag")),
                Some(&serde_json::Value::String("new".to_owned()))
            );
        }

        #[test]
        fn handles_empty_add_and_remove_tag_lists() {
            let existing = vec![ZoteroTag {
                tag: TagName::from("keep_me"),
                origin: TagOrigin::default(),
            }];

            let result = super::diff_tags(existing, &[], &[]);

            assert_eq!(result.len(), 1);
            assert_eq!(
                result.first().and_then(|v| v.get("tag")),
                Some(&serde_json::Value::String("keep_me".to_owned()))
            );
        }

        #[test]
        fn sorts_resulting_tags_deterministically() {
            let existing = vec![ZoteroTag {
                tag: TagName::from("zeta"),
                origin: TagOrigin::default(),
            }];
            let add = vec![TagName::from("alpha"), TagName::from("middle")];

            let result = super::diff_tags(existing, &add, &[]);
            let tags: Vec<_> = result
                .iter()
                .filter_map(|value| {
                    value.get("tag").and_then(|tag| tag.as_str())
                })
                .collect();

            assert_eq!(tags, vec!["alpha", "middle", "zeta"]);
        }

        #[test]
        fn deduplicates_added_tags_when_already_present() {
            let existing = vec![ZoteroTag {
                tag: TagName::from("rust"),
                origin: TagOrigin::default(),
            }];
            let add = vec![TagName::from("rust")];

            let result = super::diff_tags(existing, &add, &[]);

            assert_eq!(result.len(), 1);
            assert_eq!(
                result.first().and_then(|v| v.get("tag")),
                Some(&serde_json::Value::String("rust".to_owned()))
            );
        }
    }
}
