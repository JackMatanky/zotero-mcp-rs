//! Tag operations for the Zotero Local HTTP API.
//!
//! Adds [`ZoteroClient`] methods for listing library tags, updating tags across
//! items, renaming tags library-wide, and deleting tags.
//!
//! # Key operations
//!
//! - [`ZoteroClient::list_tags`]: list library-wide tag names.
//! - [`ZoteroClient::batch_update_tags`]: add or remove tags across items.
//! - [`ZoteroClient::rename_tag`] and [`ZoteroClient::delete_tags`]: bulk tag
//!   mutation and cleanup.

use std::collections::BTreeSet;

use crate::{
    errors::ZoteroMcpError,
    zotero::{ItemKey, TagName, client::ZoteroClient, objects::ZoteroTag},
};

impl ZoteroClient<'_> {
    /// Lists all tag names in the library, returning up to `limit` tags.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::LocalApi`] if Zotero responds with a non-2xx status
    /// - [`ZoteroMcpError::Network`] if the request fails at the transport
    ///   level
    /// - [`ZoteroMcpError::Json`] if the response cannot be decoded
    pub(crate) async fn list_tags(
        &self,
        limit: usize,
    ) -> Result<Vec<TagName>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/tags?limit={}",
            self.state.zotero_api_url, limit
        );
        let raw: Vec<serde_json::Value> = self.get_json(&url).await?;
        Ok(raw
            .into_iter()
            .filter_map(|v| {
                v.get("tag").and_then(|t| t.as_str()).map(TagName::from)
            })
            .collect())
    }

    /// Batch updates tags across multiple items by adding and removing tags.
    ///
    /// # Arguments
    ///
    /// * `item_keys` - Target item keys to update
    /// * `add_tags` - Tag names to attach to each item
    /// * `remove_tags` - Tag names to strip from each item
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
        item_keys: &[ItemKey],
        add_tags: &[TagName],
        remove_tags: &[TagName],
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
        old_tag: &TagName,
        new_tag: &TagName,
    ) -> Result<usize, ZoteroMcpError> {
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
        tags: &[TagName],
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let version = self.get_library_version().await?;
        let joined = tags
            .iter()
            .map(|t| urlencoding::encode(t.as_str()).into_owned())
            .collect::<Vec<_>>()
            .join(" || ");
        let url = format!(
            "{}/users/0/tags?tag={}",
            self.state.zotero_api_url, joined
        );
        self.delete(&url, version).await
    }
}

/// Computes the tag list after applying additions and removals.
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
    use crate::zotero::types::TagOrigin;

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
