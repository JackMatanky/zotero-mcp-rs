//! Zotero object keys, relation URIs, and library versions.
//!
//! Provides strongly-typed identifier newtypes ([`ItemKey`], [`CollectionKey`],
//! [`TagName`], [`CitationKey`], [`RelationUri`]) and library version counters
//! ([`LibraryVersion`]) used across the Zotero domain layer to prevent string
//! transposition errors.
//!
//! # Main Types
//!
//! - [`ItemKey`] - 8-character alphanumeric item identifier
//! - [`CollectionKey`] - 8-character alphanumeric collection identifier
//! - [`TagName`] - Tag name wrapper
//! - [`CitationKey`] - Citation key wrapper
//! - [`RelationUri`] - Item relation URI (`http://zotero.org/...`)
//! - [`LibraryVersion`] - Library version counter
//!
//! # Examples
//!
//! ```ignore
//! use zotero_api::zotero::{ItemKey, RelationUri};
//!
//! let item_key = ItemKey::from("ABC12345");
//! let relation_uri = RelationUri::from(&item_key);
//! assert_eq!(
//!     relation_uri.as_str(),
//!     "http://zotero.org/users/0/items/ABC12345"
//! );
//!
//! let parsed_key = ItemKey::try_from(&relation_uri).unwrap();
//! assert_eq!(parsed_key, item_key);
//! ```

use serde::{Deserialize, Serialize};

string_newtype!(
    pub ItemKey,
    "Zotero item key: an 8-character alphanumeric identifier unique within a \
     library. Distinct from [`CollectionKey`] to prevent the two from being \
     transposed at call sites.",
);
string_newtype!(
    pub CollectionKey,
    "Zotero collection key: an 8-character alphanumeric identifier unique \
     within a library. Distinct from [`ItemKey`] to prevent the two from \
     being transposed at call sites.",
);
string_newtype!(
    pub TagName,
    "Zotero tag name: wrapper for tag name strings to prevent transposition \
     with free-text query strings or keys.",
);
string_newtype!(
    pub CitationKey,
    "Zotero citation key: wrapper for citation keys to enforce type safety \
     and key semantics across search and item metadata.",
);
string_newtype!(
    pub(crate) RelationUri,
    "Zotero relation URI: an item URI stored as a value in an item's \
     `relations` map, of the form `http://zotero.org/users/0/items/{KEY}` or \
     `http://zotero.org/groups/{ID}/items/{KEY}`. Bridges [`ItemKey`] and the \
     URI strings Zotero writes for relations: [`From<&ItemKey>`](ItemKey) \
     builds a `/users/0` URI on write, while \
     [`ItemKey::try_from`](ItemKey) recovers the trailing key on read, \
     regardless of the URI prefix.",
);

/// Prefix used when constructing item relation URIs to write back to Zotero,
/// matching the Local API's own `/users/0` namespace.
const ITEM_RELATION_URI_BASE: &str = "http://zotero.org/users/0/items/";

/// Error returned when a [`RelationUri`] does not carry a valid Zotero item
/// key as its trailing URI segment.
#[derive(Debug)]
pub(crate) struct RelationUriError;

impl From<&ItemKey> for RelationUri {
    #[inline]
    fn from(key: &ItemKey) -> Self {
        Self(format!("{ITEM_RELATION_URI_BASE}{}", key.as_str()))
    }
}

impl TryFrom<&RelationUri> for ItemKey {
    type Error = RelationUriError;

    #[inline]
    fn try_from(uri: &RelationUri) -> Result<Self, Self::Error> {
        let value = uri.as_str();
        if !value.contains("/items/") {
            return Err(RelationUriError);
        }
        let Some(key) = value.rsplit('/').next() else {
            return Err(RelationUriError);
        };
        if key.len() == 8 && key.chars().all(|c| c.is_ascii_alphanumeric()) {
            Ok(ItemKey::from(key))
        } else {
            Err(RelationUriError)
        }
    }
}

impl std::fmt::Display for RelationUriError {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("not a Zotero item URI")
    }
}

/// Zotero library version counter.
#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Deserialize,
    Serialize,
)]
#[serde(transparent)]
pub struct LibraryVersion(pub(crate) u64);

impl std::fmt::Display for LibraryVersion {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for LibraryVersion {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<LibraryVersion> for u64 {
    #[inline]
    fn from(value: LibraryVersion) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod string_key {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn implements_display_from_and_equality_comparisons() {
            let item_key = ItemKey::from("ITEM123");
            assert_eq!(item_key.to_string(), "ITEM123");
            assert_eq!(item_key.as_ref(), "ITEM123");
            assert_eq!(item_key, "ITEM123");
            assert_eq!(item_key.to_string(), "ITEM123".to_owned());

            let col_key = CollectionKey::from("COL123".to_owned());
            assert_eq!(col_key.to_string(), "COL123");
            assert_eq!(col_key, "COL123");
        }
    }

    mod relation_uri {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn from_item_key_round_trips() {
            let key = ItemKey::from("ABC12345");
            let uri = RelationUri::from(&key);
            assert_eq!(
                uri.to_string(),
                "http://zotero.org/users/0/items/ABC12345"
            );

            let result = ItemKey::try_from(&uri);

            assert_eq!(result.as_ref().ok(), Some(&key));
        }

        #[test]
        fn try_from_extracts_key_from_user_library_uri() {
            let uri =
                RelationUri::from("http://zotero.org/users/0/items/ABC12345");
            let result = ItemKey::try_from(&uri);

            assert_eq!(
                result.ok().as_ref().map(ItemKey::as_str),
                Some("ABC12345")
            );
        }

        #[test]
        fn try_from_extracts_key_from_group_library_uri() {
            let uri = RelationUri::from(
                "http://zotero.org/groups/36222/items/E6IGUT5Z",
            );
            let result = ItemKey::try_from(&uri);

            assert_eq!(
                result.ok().as_ref().map(ItemKey::as_str),
                Some("E6IGUT5Z")
            );
        }

        #[test]
        fn try_from_rejects_bare_item_key_string() {
            let uri = RelationUri::from("ITEM123");
            assert!(
                ItemKey::try_from(&uri).is_err(),
                "bare non-URI key must be rejected"
            );

            let full_length_key = RelationUri::from("ABCDEFGH");
            assert!(
                ItemKey::try_from(&full_length_key).is_err(),
                "bare eight-character key must be rejected"
            );
        }

        #[test]
        fn try_from_rejects_malformed_uris() {
            let empty = RelationUri::from("");
            assert!(
                ItemKey::try_from(&empty).is_err(),
                "empty URI must be rejected"
            );

            let no_items_segment =
                RelationUri::from("http://zotero.org/users/0/ABC12345");
            assert!(
                ItemKey::try_from(&no_items_segment).is_err(),
                "URI without /items/ segment must be rejected"
            );

            let bad_key_shape =
                RelationUri::from("http://zotero.org/users/0/items/ABC");
            assert!(
                ItemKey::try_from(&bad_key_shape).is_err(),
                "short trailing key must be rejected"
            );

            let no_trailing_key =
                RelationUri::from("http://zotero.org/users/0/items/");
            assert!(
                ItemKey::try_from(&no_trailing_key).is_err(),
                "URI without trailing key must be rejected"
            );
        }
    }
}
