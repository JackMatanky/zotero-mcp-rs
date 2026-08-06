//! Controlled vocabulary value types for Zotero entities.
//!
//! Provides enumerations for item types, annotation kinds, attachment storage
//! modes, and collection parent relationships. Unknown API values are preserved
//! in `Other` variants to ensure lossless round-tripping.
//!
//! # Main Types
//!
//! - [`ItemType`]: Item classification (`journalArticle`, `book`, etc.).
//! - [`AnnotationType`]: PDF annotation kind (`highlight`, `underline`, etc.).
//! - [`LinkMode`]: Attachment storage mode (`imported_file`, `linked_file`,
//!   etc.).
//! - [`CollectionParent`]: Parent collection state
//!   ([`CollectionParent::TopLevel`] or [`CollectionParent::Parent`]).
//!
//! # Examples
//!
//! ```
//! use zotero_api::ItemType;
//!
//! let item_type = ItemType::from("journalArticle");
//! assert_eq!(item_type, ItemType::JournalArticle);
//! assert_eq!(item_type.as_str(), "journalArticle");
//! ```

use serde::{Deserialize, Serialize};

use crate::keys::CollectionKey;

/// Zotero item kind carried in the `itemType` field.
///
/// Only variants this crate branches on are named explicitly. Every other
/// Zotero item type, such as `webpage`, `bookSection`, or `thesis`, round-trips
/// through [`ItemType::Other`] with its original API string preserved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ItemType {
    /// Annotation item (`annotation`).
    Annotation,
    /// Artwork item (`artwork`).
    Artwork,
    /// Attachment item (`attachment`).
    Attachment,
    /// Audio recording item (`audioRecording`).
    AudioRecording,
    /// Bill item (`bill`).
    Bill,
    /// Blog post item (`blogPost`).
    BlogPost,
    /// Book item (`book`).
    Book,
    /// Book section item (`bookSection`).
    BookSection,
    /// Case item (`case`).
    Case,
    /// Computer program item (`computerProgram`).
    ComputerProgram,
    /// Conference paper item (`conferencePaper`).
    ConferencePaper,
    /// Dictionary entry item (`dictionaryEntry`).
    DictionaryEntry,
    /// Document item (`document`).
    Document,
    /// Email item (`email`).
    Email,
    /// Encyclopedia article item (`encyclopediaArticle`).
    EncyclopediaArticle,
    /// Film item (`film`).
    Film,
    /// Forum post item (`forumPost`).
    ForumPost,
    /// Hearing item (`hearing`).
    Hearing,
    /// Instant message item (`instantMessage`).
    InstantMessage,
    /// Interview item (`interview`).
    Interview,
    /// Journal article item (`journalArticle`).
    JournalArticle,
    /// Letter item (`letter`).
    Letter,
    /// Magazine article item (`magazineArticle`).
    MagazineArticle,
    /// Manuscript item (`manuscript`).
    Manuscript,
    /// Map item (`map`).
    Map,
    /// Newspaper article item (`newspaperArticle`).
    NewspaperArticle,
    /// Note item (`note`).
    Note,
    /// Patent item (`patent`).
    Patent,
    /// Podcast item (`podcast`).
    Podcast,
    /// Preprint item (`preprint`).
    Preprint,
    /// Presentation item (`presentation`).
    Presentation,
    /// Radio broadcast item (`radioBroadcast`).
    RadioBroadcast,
    /// Report item (`report`).
    Report,
    /// Statute item (`statute`).
    Statute,
    /// Thesis item (`thesis`).
    Thesis,
    /// TV broadcast item (`tvBroadcast`).
    TvBroadcast,
    /// Video recording item (`videoRecording`).
    VideoRecording,
    /// Webpage item (`webpage`).
    Webpage,
    /// Any Zotero item type not modeled above; carries the original API value.
    Other(String),
}

impl ItemType {
    /// Borrows the API string representation of this [`ItemType`].
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Annotation => "annotation",
            Self::Artwork => "artwork",
            Self::Attachment => "attachment",
            Self::AudioRecording => "audioRecording",
            Self::Bill => "bill",
            Self::BlogPost => "blogPost",
            Self::Book => "book",
            Self::BookSection => "bookSection",
            Self::Case => "case",
            Self::ComputerProgram => "computerProgram",
            Self::ConferencePaper => "conferencePaper",
            Self::DictionaryEntry => "dictionaryEntry",
            Self::Document => "document",
            Self::Email => "email",
            Self::EncyclopediaArticle => "encyclopediaArticle",
            Self::Film => "film",
            Self::ForumPost => "forumPost",
            Self::Hearing => "hearing",
            Self::InstantMessage => "instantMessage",
            Self::Interview => "interview",
            Self::JournalArticle => "journalArticle",
            Self::Letter => "letter",
            Self::MagazineArticle => "magazineArticle",
            Self::Manuscript => "manuscript",
            Self::Map => "map",
            Self::NewspaperArticle => "newspaperArticle",
            Self::Note => "note",
            Self::Patent => "patent",
            Self::Podcast => "podcast",
            Self::Preprint => "preprint",
            Self::Presentation => "presentation",
            Self::RadioBroadcast => "radioBroadcast",
            Self::Report => "report",
            Self::Statute => "statute",
            Self::Thesis => "thesis",
            Self::TvBroadcast => "tvBroadcast",
            Self::VideoRecording => "videoRecording",
            Self::Webpage => "webpage",
            Self::Other(value) => value,
        }
    }

    /// Returns `true` if this item type is eligible for search and embedding
    /// indexing: everything except attachments, notes, and annotations.
    #[inline]
    pub(crate) fn is_indexable(&self) -> bool {
        !matches!(self, Self::Attachment | Self::Note | Self::Annotation)
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
            "annotation" => Self::Annotation,
            "artwork" => Self::Artwork,
            "attachment" => Self::Attachment,
            "audioRecording" => Self::AudioRecording,
            "bill" => Self::Bill,
            "blogPost" => Self::BlogPost,
            "book" => Self::Book,
            "bookSection" => Self::BookSection,
            "case" => Self::Case,
            "computerProgram" => Self::ComputerProgram,
            "conferencePaper" => Self::ConferencePaper,
            "dictionaryEntry" => Self::DictionaryEntry,
            "document" => Self::Document,
            "email" => Self::Email,
            "encyclopediaArticle" => Self::EncyclopediaArticle,
            "film" => Self::Film,
            "forumPost" => Self::ForumPost,
            "hearing" => Self::Hearing,
            "instantMessage" => Self::InstantMessage,
            "interview" => Self::Interview,
            "journalArticle" => Self::JournalArticle,
            "letter" => Self::Letter,
            "magazineArticle" => Self::MagazineArticle,
            "manuscript" => Self::Manuscript,
            "map" => Self::Map,
            "newspaperArticle" => Self::NewspaperArticle,
            "note" => Self::Note,
            "patent" => Self::Patent,
            "podcast" => Self::Podcast,
            "preprint" => Self::Preprint,
            "presentation" => Self::Presentation,
            "radioBroadcast" => Self::RadioBroadcast,
            "report" => Self::Report,
            "statute" => Self::Statute,
            "thesis" => Self::Thesis,
            "tvBroadcast" => Self::TvBroadcast,
            "videoRecording" => Self::VideoRecording,
            "webpage" => Self::Webpage,
            _ => Self::Other(value),
        }
    }
}

impl From<&str> for ItemType {
    #[inline]
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
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
pub enum AnnotationType {
    /// Text highlight annotation (`highlight`).
    Highlight,
    /// Text underline annotation (`underline`).
    Underline,
    /// Standalone PDF note annotation (`note`).
    Note,
    /// Any annotation kind not modeled above; carries the original API value.
    Other(String),
}

impl AnnotationType {
    /// Borrows the API string representation of this [`AnnotationType`].
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Highlight => "highlight",
            Self::Underline => "underline",
            Self::Note => "note",
            Self::Other(value) => value,
        }
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
    /// Primary author or creator (`author`).
    Author,
    /// Editor (`editor`).
    Editor,
    /// Translator (`translator`).
    Translator,
    /// Any creator role not modeled above; carries the original API value.
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
pub enum LinkMode {
    /// File stored directly inside Zotero's storage directory
    /// (`imported_file`).
    ImportedFile,
    /// File linked from an external filesystem path (`linked_file`).
    LinkedFile,
    /// Web page or remote URL link (`linked_url`).
    LinkedUrl,
    /// Saved HTML snapshot or imported URL content (`imported_url`).
    ImportedUrl,
    /// Any link mode not modeled above; carries the original API value.
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
pub enum CollectionParent {
    /// Top-level collection with no parent collection.
    #[default]
    TopLevel,
    /// Child collection belonging to a parent collection identified by
    /// [`CollectionKey`].
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

/// Tag source carried in Zotero's numeric `type` field.
///
/// Zotero uses `0` for user-created tags and `1` for tags assigned
/// automatically on import.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(from = "u8", into = "u8")]
pub(crate) enum TagOrigin {
    /// Tag explicitly created by a user (`0`).
    #[default]
    User,
    /// Tag assigned automatically on import or export (`1`).
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
        fn round_trips_all_35_zotero_item_types() {
            let types = vec![
                (ItemType::Annotation, "annotation"),
                (ItemType::Artwork, "artwork"),
                (ItemType::Attachment, "attachment"),
                (ItemType::AudioRecording, "audioRecording"),
                (ItemType::Bill, "bill"),
                (ItemType::BlogPost, "blogPost"),
                (ItemType::Book, "book"),
                (ItemType::BookSection, "bookSection"),
                (ItemType::Case, "case"),
                (ItemType::ComputerProgram, "computerProgram"),
                (ItemType::ConferencePaper, "conferencePaper"),
                (ItemType::DictionaryEntry, "dictionaryEntry"),
                (ItemType::Document, "document"),
                (ItemType::Email, "email"),
                (ItemType::EncyclopediaArticle, "encyclopediaArticle"),
                (ItemType::Film, "film"),
                (ItemType::ForumPost, "forumPost"),
                (ItemType::Hearing, "hearing"),
                (ItemType::InstantMessage, "instantMessage"),
                (ItemType::Interview, "interview"),
                (ItemType::JournalArticle, "journalArticle"),
                (ItemType::Letter, "letter"),
                (ItemType::MagazineArticle, "magazineArticle"),
                (ItemType::Manuscript, "manuscript"),
                (ItemType::Map, "map"),
                (ItemType::NewspaperArticle, "newspaperArticle"),
                (ItemType::Note, "note"),
                (ItemType::Patent, "patent"),
                (ItemType::Podcast, "podcast"),
                (ItemType::Preprint, "preprint"),
                (ItemType::Presentation, "presentation"),
                (ItemType::RadioBroadcast, "radioBroadcast"),
                (ItemType::Report, "report"),
                (ItemType::Statute, "statute"),
                (ItemType::Thesis, "thesis"),
                (ItemType::TvBroadcast, "tvBroadcast"),
                (ItemType::VideoRecording, "videoRecording"),
                (ItemType::Webpage, "webpage"),
            ];

            for (variant, expected_str) in types {
                assert_eq!(variant.as_str(), expected_str);
                let stringified: String = variant.clone().into();
                assert_eq!(stringified, expected_str);
                assert_eq!(ItemType::from(expected_str), variant);
            }
        }

        #[test]
        fn defaults_to_other_variant() {
            assert_eq!(ItemType::default(), ItemType::Other(String::new()));
        }

        #[test]
        fn is_indexable_excludes_attachments_notes_and_annotations() {
            for item_type in
                [ItemType::Attachment, ItemType::Note, ItemType::Annotation]
            {
                assert!(
                    !item_type.is_indexable(),
                    "{item_type:?} must not be indexable"
                );
            }
            assert!(ItemType::JournalArticle.is_indexable());
            assert!(ItemType::Other("webpage".to_owned()).is_indexable());
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

    mod creator_type {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn creator_type_round_trips_author_editor_and_other() {
            for (value, expected) in [
                ("author", CreatorType::Author),
                ("editor", CreatorType::Editor),
                ("reviewer", CreatorType::Other("reviewer".to_owned())),
            ] {
                let creator_type = CreatorType::from(value.to_owned());
                let serialized: String = creator_type.clone().into();

                assert_eq!(creator_type, expected, "case {value}");
                assert_eq!(serialized, value, "case {value}");
            }
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
