//! Zotero controlled-vocabulary value types.
//!
//! These enums model the Zotero strings this crate branches on while preserving
//! unknown API values for round-tripping.

use serde::{Deserialize, Serialize};

use crate::zotero::keys::CollectionKey;

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
}
