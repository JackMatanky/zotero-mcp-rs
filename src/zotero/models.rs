//! Serde models for Zotero Local API JSON payloads.
//!
//! Defines typed item, collection, tag, creator, relation, and version models
//! used by the Zotero client. The key newtypes prevent accidentally mixing item
//! keys, collection keys, tag names, citation keys, and relation URIs.
//!
//! # Key types
//!
//! - [`ItemKey`] and [`CollectionKey`]: type-safe Zotero object identifiers.
//! - [`LibraryVersion`]: wrapper around Zotero library version counters.
//! - [`ZoteroItem`] and [`ZoteroItemData`]: bibliographic items, notes,
//!   attachments, and annotations.
//! - [`ZoteroCollection`]: collection tree nodes and parent metadata.
//! - [`ItemType`], [`AnnotationType`], and [`CreatorType`]: extensible enums
//!   that preserve unmodeled Zotero strings.

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

/// Zotero item kind carried in the `itemType` field.
///
/// Only variants this crate branches on are named explicitly. Every other
/// Zotero item type, such as `webpage`, `bookSection`, or `thesis`, round-trips
/// through [`ItemType::Other`] with its original API string preserved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub(crate) enum ItemType {
    JournalArticle,
    Book,
    Preprint,
    Note,
    Attachment,
    Annotation,
    /// Any Zotero item type not modeled above; carries the API's original
    /// value.
    Other(String),
}

impl ItemType {
    /// Borrows the API string this variant serializes to.
    #[inline]
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::JournalArticle => "journalArticle",
            Self::Book => "book",
            Self::Preprint => "preprint",
            Self::Note => "note",
            Self::Attachment => "attachment",
            Self::Annotation => "annotation",
            Self::Other(value) => value,
        }
    }
}

impl Default for ItemType {
    #[inline]
    fn default() -> Self {
        Self::Other(String::new())
    }
}

impl From<String> for ItemType {
    #[inline]
    fn from(value: String) -> Self {
        match value.as_str() {
            "journalArticle" => Self::JournalArticle,
            "book" => Self::Book,
            "preprint" => Self::Preprint,
            "note" => Self::Note,
            "attachment" => Self::Attachment,
            "annotation" => Self::Annotation,
            _ => Self::Other(value),
        }
    }
}

impl From<ItemType> for String {
    #[inline]
    fn from(value: ItemType) -> Self {
        match value {
            ItemType::Other(value) => value,
            known => known.as_str().to_owned(),
        }
    }
}

/// PDF annotation kind carried in the `annotationType` field.
///
/// Falls back to [`AnnotationType::Other`] for annotation kinds this crate does
/// not create, such as `image` or `ink`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub(crate) enum AnnotationType {
    Highlight,
    Underline,
    Note,
    /// Any annotation kind not modeled above; carries the API's original
    /// value.
    Other(String),
}

impl AnnotationType {
    /// Borrows the API string this variant serializes to.
    #[inline]
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::Highlight => "highlight",
            Self::Underline => "underline",
            Self::Note => "note",
            Self::Other(value) => value,
        }
    }
}

impl schemars::JsonSchema for AnnotationType {
    #[inline]
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AnnotationType".into()
    }

    #[inline]
    fn json_schema(
        generator: &mut schemars::SchemaGenerator,
    ) -> schemars::Schema {
        String::json_schema(generator)
    }
}

impl From<String> for AnnotationType {
    #[inline]
    fn from(value: String) -> Self {
        match value.as_str() {
            "highlight" => Self::Highlight,
            "underline" => Self::Underline,
            "note" => Self::Note,
            _ => Self::Other(value),
        }
    }
}

impl From<AnnotationType> for String {
    #[inline]
    fn from(value: AnnotationType) -> Self {
        match value {
            AnnotationType::Other(value) => value,
            known => known.as_str().to_owned(),
        }
    }
}

/// Creator role carried in the `creatorType` field.
///
/// Zotero defines many item-type-specific creator roles. The common roles are
/// named explicitly, while [`CreatorType::Other`] preserves anything else for
/// round-tripping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub(crate) enum CreatorType {
    Author,
    Editor,
    Translator,
    Other(String),
}

impl From<String> for CreatorType {
    #[inline]
    fn from(value: String) -> Self {
        match value.as_str() {
            "author" => Self::Author,
            "editor" => Self::Editor,
            "translator" => Self::Translator,
            _ => Self::Other(value),
        }
    }
}

impl From<CreatorType> for String {
    #[inline]
    fn from(value: CreatorType) -> Self {
        match value {
            CreatorType::Author => "author".to_owned(),
            CreatorType::Editor => "editor".to_owned(),
            CreatorType::Translator => "translator".to_owned(),
            CreatorType::Other(s) => s,
        }
    }
}

/// Attachment storage mode carried in the `linkMode` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub(crate) enum LinkMode {
    ImportedFile,
    LinkedFile,
    LinkedUrl,
    ImportedUrl,
    Other(String),
}

impl From<String> for LinkMode {
    #[inline]
    fn from(value: String) -> Self {
        match value.as_str() {
            "imported_file" => Self::ImportedFile,
            "linked_file" => Self::LinkedFile,
            "linked_url" => Self::LinkedUrl,
            "imported_url" => Self::ImportedUrl,
            _ => Self::Other(value),
        }
    }
}

impl From<LinkMode> for String {
    #[inline]
    fn from(value: LinkMode) -> Self {
        match value {
            LinkMode::ImportedFile => "imported_file".to_owned(),
            LinkMode::LinkedFile => "linked_file".to_owned(),
            LinkMode::LinkedUrl => "linked_url".to_owned(),
            LinkMode::ImportedUrl => "imported_url".to_owned(),
            LinkMode::Other(s) => s,
        }
    }
}

/// Parent relationship for a Zotero collection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "serde_json::Value", into = "serde_json::Value")]
pub(crate) enum CollectionParent {
    #[default]
    TopLevel,
    Parent(CollectionKey),
}

impl From<serde_json::Value> for CollectionParent {
    #[inline]
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::String(s) if !s.is_empty() && s != "false" => {
                Self::Parent(CollectionKey::from(s))
            }
            _ => Self::TopLevel,
        }
    }
}

impl From<CollectionParent> for serde_json::Value {
    #[inline]
    fn from(value: CollectionParent) -> Self {
        match value {
            CollectionParent::TopLevel => Self::Bool(false),
            CollectionParent::Parent(key) => {
                Self::String(key.as_str().to_owned())
            }
        }
    }
}

impl schemars::JsonSchema for CollectionParent {
    #[inline]
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CollectionParent".into()
    }

    #[inline]
    fn json_schema(
        generator: &mut schemars::SchemaGenerator,
    ) -> schemars::Schema {
        String::json_schema(generator)
    }
}

/// Tag source carried in Zotero's numeric `type` field.
///
/// Zotero uses `0` for user-created tags and `1` for tags assigned
/// automatically on import.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(from = "u8", into = "u8")]
pub(crate) enum TagOrigin {
    #[default]
    User,
    Automatic,
    /// Any origin value outside Zotero's documented `0`/`1`; carries the
    /// original integer.
    Other(u8),
}

impl From<u8> for TagOrigin {
    #[inline]
    fn from(value: u8) -> Self {
        match value {
            0 => Self::User,
            1 => Self::Automatic,
            other => Self::Other(other),
        }
    }
}

impl From<TagOrigin> for u8 {
    #[inline]
    fn from(value: TagOrigin) -> Self {
        match value {
            TagOrigin::User => 0,
            TagOrigin::Automatic => 1,
            TagOrigin::Other(other) => other,
        }
    }
}

/// A single Zotero library item as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroItem {
    pub(crate) key: ItemKey,
    pub(crate) version: LibraryVersion,
    /// Owning library metadata object.
    #[serde(default)]
    pub(crate) library: serde_json::Value,
    /// HATEOAS API link objects.
    #[serde(default)]
    pub(crate) links: serde_json::Value,
    /// Item metadata containing creator summary and child counts.
    #[serde(default)]
    pub(crate) meta: serde_json::Value,
    pub(crate) data: ZoteroItemData,
}

/// Bibliographic, attachment, note, and annotation fields for a Zotero item.
///
/// Maps Zotero's `camelCase` JSON field names across item types.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZoteroItemData {
    pub(crate) key: ItemKey,
    #[serde(default)]
    pub(crate) version: LibraryVersion,
    #[serde(rename = "itemType", default)]
    pub(crate) item_type: ItemType,
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) creators: Vec<ZoteroCreator>,
    /// Abstract or summary text string.
    pub(crate) abstract_note: Option<String>,
    pub(crate) publication_title: Option<String>,
    pub(crate) volume: Option<String>,
    pub(crate) issue: Option<String>,
    pub(crate) pages: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) series: Option<String>,
    pub(crate) series_title: Option<String>,
    pub(crate) series_text: Option<String>,
    pub(crate) journal_abbreviation: Option<String>,
    pub(crate) doi: Option<String>,
    /// Zotero's native citation key field.
    ///
    /// Zotero 9 exposes this as `itemFields.citationKey`, and quicksearch can
    /// search it server-side. This field takes precedence over any
    /// `Citation Key: ...` line Better `BibTeX` may write to
    /// [`ZoteroItemData::extra`].
    #[serde(rename = "citationKey")]
    pub(crate) citation_key: Option<String>,
    pub(crate) isbn: Option<String>,
    pub(crate) issn: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) access_date: Option<String>,
    pub(crate) archive: Option<String>,
    pub(crate) archive_location: Option<String>,
    pub(crate) library_catalog: Option<String>,
    pub(crate) call_number: Option<String>,
    pub(crate) rights: Option<String>,
    /// Free-form extra metadata field (e.g. citation keys or custom fields).
    pub(crate) extra: Option<String>,
    #[serde(default)]
    pub(crate) tags: Vec<ZoteroTag>,
    #[serde(default)]
    pub(crate) collections: Vec<CollectionKey>,
    /// Map of Zotero relation predicate names to URI values.
    #[serde(default)]
    pub(crate) relations: serde_json::Value,
    pub(crate) date_added: Option<String>,
    pub(crate) date_modified: Option<String>,
    /// Parent item key for attachment and child note items.
    pub(crate) parent_item: Option<ItemKey>,
    /// Attachment storage mode (e.g. `"imported_file"` or `"linked_url"`).
    pub(crate) link_mode: Option<LinkMode>,
    /// Attachment MIME content type.
    #[serde(rename = "contentType")]
    pub(crate) content_type: Option<String>,
    pub(crate) charset: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) path: Option<String>,
    /// HTML content body for note items.
    pub(crate) note: Option<String>,
    /// PDF annotation kind (e.g. `"highlight"`, `"underline"`, or `"note"`).
    #[serde(rename = "annotationType")]
    pub(crate) annotation_type: Option<AnnotationType>,
    /// Selected text for PDF highlight/underline annotations.
    #[serde(rename = "annotationText")]
    pub(crate) annotation_text: Option<String>,
    /// User comment attached to a PDF annotation.
    #[serde(rename = "annotationComment")]
    pub(crate) annotation_comment: Option<String>,
    /// CSS hex color string for PDF annotations.
    #[serde(rename = "annotationColor")]
    pub(crate) annotation_color: Option<String>,
    /// PDF page label where the annotation appears.
    #[serde(rename = "annotationPageLabel")]
    pub(crate) annotation_page_label: Option<String>,
    /// Whether the item is in the trash.
    #[serde(default)]
    pub(crate) deleted: bool,
}

/// An author, editor, or other creator credited on an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCreator {
    /// Creator role (e.g. `"author"`, `"editor"`).
    #[serde(rename = "creatorType")]
    pub(crate) creator_type: Option<CreatorType>,
    #[serde(rename = "firstName")]
    pub(crate) first_name: Option<String>,
    #[serde(rename = "lastName")]
    pub(crate) last_name: Option<String>,
    /// Single-field name for institutional or single-field creators.
    pub(crate) name: Option<String>,
}

/// A tag attached to an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroTag {
    pub(crate) tag: TagName,
    /// Tag origin: user-created vs. automatically assigned on import.
    #[serde(rename = "type", default)]
    pub(crate) origin: TagOrigin,
}

/// A Zotero collection as returned by the Local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollection {
    pub(crate) key: CollectionKey,
    pub(crate) version: LibraryVersion,
    pub(crate) data: ZoteroCollectionData,
}

/// Metadata payload for a [`ZoteroCollection`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZoteroCollectionData {
    pub(crate) key: CollectionKey,
    pub(crate) name: String,
    /// Parent collection state.
    #[serde(rename = "parentCollection", default)]
    pub(crate) parent_collection: CollectionParent,
}

/// Result of probing the Zotero Local API for availability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalApiStatus {
    pub(crate) online: bool,
    pub(crate) url: String,
    pub(crate) version: Option<String>,
    pub(crate) error: Option<String>,
}
#[cfg(test)]
mod tests {
    use super::*;

    mod item_type {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn round_trips_known_and_unknown_item_types() {
            let article = ItemType::JournalArticle;
            let article_str: String = article.clone().into();
            assert_eq!(article_str, "journalArticle");
            assert_eq!(ItemType::from(article_str), article);

            let custom = ItemType::from("customWebpage".to_owned());
            let custom_str: String = custom.clone().into();
            assert_eq!(custom_str, "customWebpage");
            assert_eq!(custom, ItemType::Other("customWebpage".to_owned()));
        }

        #[test]
        fn defaults_to_other_variant() {
            assert_eq!(ItemType::default(), ItemType::Other(String::new()));
        }
    }

    mod annotation_type {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn round_trips_known_and_unknown_annotation_types() {
            let highlight = AnnotationType::Highlight;
            let highlight_str: String = highlight.clone().into();
            assert_eq!(highlight_str, "highlight");
            assert_eq!(AnnotationType::from(highlight_str), highlight);

            let ink = AnnotationType::from("ink".to_owned());
            let ink_str: String = ink.clone().into();
            assert_eq!(ink_str, "ink");
            assert_eq!(ink, AnnotationType::Other("ink".to_owned()));
        }
    }

    mod tag_origin {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn converts_user_automatic_and_other_variants() {
            assert_eq!(TagOrigin::from(0), TagOrigin::User);
            assert_eq!(TagOrigin::from(1), TagOrigin::Automatic);
            assert_eq!(TagOrigin::from(42), TagOrigin::Other(42));

            let user_num: u8 = TagOrigin::User.into();
            let auto_num: u8 = TagOrigin::Automatic.into();
            let other_num: u8 = TagOrigin::Other(42).into();
            assert_eq!(user_num, 0);
            assert_eq!(auto_num, 1);
            assert_eq!(other_num, 42);
        }
    }

    mod link_mode {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn round_trips_known_and_unknown_link_modes() {
            let imported = LinkMode::ImportedFile;
            let imported_str: String = imported.clone().into();
            assert_eq!(imported_str, "imported_file");
            assert_eq!(LinkMode::from(imported_str), imported);

            let custom = LinkMode::from("custom_mode".to_owned());
            let custom_str: String = custom.clone().into();
            assert_eq!(custom_str, "custom_mode");
            assert_eq!(custom, LinkMode::Other("custom_mode".to_owned()));
        }
    }

    mod collection_parent {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn serializes_top_level_as_false_and_parent_as_key() {
            let top_level: serde_json::Value =
                CollectionParent::TopLevel.into();
            assert_eq!(top_level, serde_json::json!(false));

            let parent: serde_json::Value =
                CollectionParent::Parent(CollectionKey::from("PARENT01"))
                    .into();
            assert_eq!(parent, serde_json::json!("PARENT01"));
        }

        #[test]
        fn treats_false_null_and_string_false_as_top_level() {
            assert_eq!(
                CollectionParent::from(serde_json::json!(false)),
                CollectionParent::TopLevel
            );
            assert_eq!(
                CollectionParent::from(serde_json::Value::Null),
                CollectionParent::TopLevel
            );
            assert_eq!(
                CollectionParent::from(serde_json::json!("false")),
                CollectionParent::TopLevel
            );
        }
    }

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

    mod deserialization {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn leaves_omitted_optional_fields_as_none() {
            let raw_json = serde_json::json!({
                "key": "ABC12345",
                "version": 42,
                "data": {
                    "key": "ABC12345",
                    "version": 42,
                    "itemType": "journalArticle",
                    "title": "Quantum Computing Advances"
                }
            });

            let item: ZoteroItem = serde_json::from_value(raw_json).unwrap();

            assert_eq!(item.key, "ABC12345");
            assert_eq!(
                item.data.title.as_deref(),
                Some("Quantum Computing Advances")
            );
            assert!(item.data.doi.is_none());
        }

        #[test]
        fn parses_creator_camel_case_names() {
            let raw_json = serde_json::json!({
                "creatorType": "author",
                "firstName": "Ada",
                "lastName": "Lovelace"
            });

            let creator: ZoteroCreator =
                serde_json::from_value(raw_json).unwrap();

            assert_eq!(creator.creator_type, Some(CreatorType::Author));
            assert_eq!(creator.first_name.as_deref(), Some("Ada"));
            assert_eq!(creator.last_name.as_deref(), Some("Lovelace"));
        }

        #[test]
        fn defaults_deleted_to_false_when_absent() {
            let raw_json = serde_json::json!({
                "key": "ABC12345",
                "version": 42,
                "data": {
                    "key": "ABC12345",
                    "version": 42,
                    "itemType": "journalArticle"
                }
            });

            let item: ZoteroItem = serde_json::from_value(raw_json).unwrap();

            assert!(!item.data.deleted);
        }

        #[test]
        fn round_trips_deleted_flag() {
            let raw_json = serde_json::json!({
                "key": "ABC12345",
                "version": 42,
                "itemType": "journalArticle",
                "deleted": true
            });

            let data: ZoteroItemData =
                serde_json::from_value(raw_json).unwrap();
            assert!(data.deleted);

            let serialized = serde_json::to_string(&data).unwrap();
            assert!(serialized.contains("\"deleted\":true"));
        }

        #[test]
        fn parses_native_citation_key_field() {
            let raw_json = serde_json::json!({
                "key": "ABC12345",
                "version": 42,
                "itemType": "journalArticle",
                "citationKey": "smith2020deep"
            });

            let data: ZoteroItemData =
                serde_json::from_value(raw_json).unwrap();
            assert_eq!(data.citation_key.as_deref(), Some("smith2020deep"));
        }
        #[test]
        fn deserializes_collection_with_parent_collection_key() {
            let raw_json = serde_json::json!({
                "key": "COL12345",
                "version": 10,
                "data": {
                    "key": "COL12345",
                    "version": 10,
                    "name": "Machine Learning",
                    "parentCollection": "PARENT01"
                }
            });

            let col: ZoteroCollection =
                serde_json::from_value(raw_json).unwrap();
            assert_eq!(col.key, "COL12345");
            assert_eq!(col.data.name, "Machine Learning");
            assert_eq!(
                col.data.parent_collection,
                CollectionParent::Parent(CollectionKey::from("PARENT01"))
            );
        }
    }
}
