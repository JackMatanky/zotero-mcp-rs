//! Zotero object keys, relation URIs, and library versions.
//!
//! Keeps identity-like values in one place: item keys, collection keys, tag
//! names, citation keys, relation URIs, and library-version counters.

use serde::{Deserialize, Serialize};

/// Generates a [`String`]-backed identifier newtype.
///
/// The generated type supports the conversions and comparisons needed for
/// domain keys: [`std::fmt::Display`], [`From<String>`], [`From<&str>`],
/// [`AsRef<str>`], and equality against plain strings.
macro_rules! string_key {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
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
        pub(crate) struct $name(pub(crate) String);

        impl $name {
            #[inline]
            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            #[inline]
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            #[inline]
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            #[inline]
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<$name> for str {
            #[inline]
            fn eq(&self, other: &$name) -> bool {
                self == other.0.as_str()
            }
        }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(
                generator: &mut schemars::SchemaGenerator,
            ) -> schemars::Schema {
                String::json_schema(generator)
            }
        }
    };
}

string_key!(
    ItemKey,
    "Zotero item key: an 8-character alphanumeric identifier unique within a \
     library. Distinct from [`CollectionKey`] to prevent the two from being \
     transposed at call sites."
);
string_key!(
    CollectionKey,
    "Zotero collection key: an 8-character alphanumeric identifier unique \
     within a library. Distinct from [`ItemKey`] to prevent the two from \
     being transposed at call sites."
);
string_key!(
    TagName,
    "Zotero tag name: wrapper for tag name strings to prevent transposition \
     with free-text query strings or keys."
);
string_key!(
    CitationKey,
    "Zotero citation key: wrapper for citation keys to enforce type safety \
     and key semantics across search and item metadata."
);
string_key!(
    RelationUri,
    "Zotero relation URI: an item URI stored as a value in an item's \
     `relations` map, of the form `http://zotero.org/users/0/items/{KEY}` or \
     `http://zotero.org/groups/{ID}/items/{KEY}`. Bridges [`ItemKey`] and the \
     URI strings Zotero writes for relations: [`From<&ItemKey>`](ItemKey) \
     builds a `/users/0` URI on write, while \
     [`ItemKey::try_from`](ItemKey) recovers the trailing key on read, \
     regardless of the URI prefix."
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
pub(crate) struct LibraryVersion(pub(crate) u64);

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

impl schemars::JsonSchema for LibraryVersion {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "LibraryVersion".into()
    }

    fn json_schema(
        generator: &mut schemars::SchemaGenerator,
    ) -> schemars::Schema {
        u64::json_schema(generator)
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

            let recovered = ItemKey::try_from(&uri).unwrap();
            assert_eq!(recovered, key);
        }

        #[test]
        fn try_from_extracts_key_from_user_library_uri() {
            let uri =
                RelationUri::from("http://zotero.org/users/0/items/ABC12345");
            let key = ItemKey::try_from(&uri).unwrap();
            assert_eq!(key, "ABC12345");
        }

        #[test]
        fn try_from_extracts_key_from_group_library_uri() {
            let uri = RelationUri::from(
                "http://zotero.org/groups/36222/items/E6IGUT5Z",
            );
            let key = ItemKey::try_from(&uri).unwrap();
            assert_eq!(key, "E6IGUT5Z");
        }

        #[test]
        fn try_from_rejects_bare_item_key_string() {
            let uri = RelationUri::from("ITEM123");
            assert!(ItemKey::try_from(&uri).is_err());

            let full_length_key = RelationUri::from("ABCDEFGH");
            assert!(ItemKey::try_from(&full_length_key).is_err());
        }

        #[test]
        fn try_from_rejects_malformed_uris() {
            let empty = RelationUri::from("");
            assert!(ItemKey::try_from(&empty).is_err());

            let no_items_segment =
                RelationUri::from("http://zotero.org/users/0/ABC12345");
            assert!(ItemKey::try_from(&no_items_segment).is_err());

            let bad_key_shape =
                RelationUri::from("http://zotero.org/users/0/items/ABC");
            assert!(ItemKey::try_from(&bad_key_shape).is_err());
        }
    }
}
