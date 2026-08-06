//! Fast, zero-dependency native `BibTeX` and `BibLaTeX` serializer for Zotero
//! items.
//!
//! Provides formatting functions to convert [`ZoteroItem`] structures directly
//! into `BibTeX` or `BibLaTeX` syntax strings without requiring external tools.
//!
//! # Examples
//!
//! ```
//! use zotero_api::{ZoteroItem, item_to_bibtex};
//!
//! let json_data = serde_json::json!({
//!     "key": "ITEM01",
//!     "version": 1,
//!     "library": { "type": "user", "id": 0 },
//!     "data": {
//!         "key": "ITEM01",
//!         "version": 1,
//!         "itemType": "journalArticle",
//!         "title": "Quantum Computation",
//!         "creators": [],
//!         "tags": [],
//!         "collections": [],
//!         "relations": {}
//!     }
//! });
//! let item: ZoteroItem = serde_json::from_value(json_data).unwrap();
//! let bib = item_to_bibtex(&item, "Smith2024", "bibtex");
//! assert!(bib.starts_with("@article{Smith2024,"));
//! ```

use std::fmt::Write;

use crate::{
    objects::{ZoteroCreator, ZoteroItem},
    types::ItemType,
};

/// Formats an [`ItemType`] into its standard `BibTeX`/`BibLaTeX` entry type
/// identifier.
fn bibtex_entry_type(item_type: &ItemType, is_biblatex: bool) -> &'static str {
    match item_type {
        ItemType::JournalArticle => "article",
        ItemType::Book => "book",
        ItemType::BookSection => "incollection",
        ItemType::ConferencePaper => "inproceedings",
        ItemType::Thesis => "phdthesis",
        ItemType::Report => "techreport",
        ItemType::Patent => "patent",
        ItemType::Webpage
        | ItemType::BlogPost
        | ItemType::ForumPost
        | ItemType::Preprint
            if is_biblatex =>
        {
            "online"
        }
        _ => "misc",
    }
}

/// Formats creator lists into standard `BibTeX` author strings (`Last, First
/// and Last2, First2`).
fn format_creators(creators: &[ZoteroCreator]) -> String {
    let mut result = String::new();
    for (i, creator) in creators.iter().enumerate() {
        if i > 0 {
            result.push_str(" and ");
        }
        match (&creator.last_name, &creator.first_name, &creator.name) {
            (Some(last), Some(first), _) if !last.is_empty() => {
                let _ = write!(result, "{last}, {first}");
            }
            (Some(last), None, _) if !last.is_empty() => {
                result.push_str(last);
            }
            (_, _, Some(single_name)) if !single_name.is_empty() => {
                result.push_str(single_name);
            }
            (None, Some(first), _) if !first.is_empty() => {
                result.push_str(first);
            }
            _ => {}
        }
    }
    result
}

/// Escapes special `BibTeX` markup characters (`&`, `%`, `$`, `#`, `_`, `{`,
/// `}`).
fn escape_bibtex(val: &str) -> String {
    let mut out = String::with_capacity(val.len());
    for c in val.chars() {
        match c {
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Serializes a single [`ZoteroItem`] into a formatted `BibTeX` or `BibLaTeX`
/// string entry.
///
/// # Arguments
///
/// * `item` - Zotero item structure to serialize
/// * `citekey` - Custom citation key; if empty, falls back to item citation key
///   or key ID
/// * `style` - Serialization format: `"bibtex"` or `"biblatex"`
#[must_use]
#[inline]
#[expect(
    clippy::too_many_lines,
    reason = "formats full BibTeX entry fields across all types"
)]
pub fn item_to_bibtex(item: &ZoteroItem, citekey: &str, style: &str) -> String {
    let is_biblatex = style.eq_ignore_ascii_case("biblatex");
    let entry_type = bibtex_entry_type(&item.data.item_type, is_biblatex);

    let key = if citekey.trim().is_empty() {
        if let Some(citation_key) = &item.data.citation_key {
            citation_key.as_str()
        } else {
            item.data.key.as_str()
        }
    } else {
        citekey
    };

    let mut entry = format!("@{entry_type}{{{key},\n");

    if let Some(title) = &item.data.title {
        let _ = writeln!(entry, "  title = {{{}}},", escape_bibtex(title));
    }

    let authors = format_creators(&item.data.creators);
    if !authors.is_empty() {
        let _ = writeln!(entry, "  author = {{{}}},", escape_bibtex(&authors));
    }

    match item.data.item_type {
        ItemType::JournalArticle => {
            if let Some(journal) = &item.data.publication_title {
                let _ = writeln!(
                    entry,
                    "  journal = {{{}}},",
                    escape_bibtex(journal)
                );
            }
        }
        ItemType::BookSection | ItemType::ConferencePaper => {
            if let Some(booktitle) = &item.data.publication_title {
                let _ = writeln!(
                    entry,
                    "  booktitle = {{{}}},",
                    escape_bibtex(booktitle)
                );
            }
            if let Some(publisher) = &item.data.publisher {
                let _ = writeln!(
                    entry,
                    "  publisher = {{{}}},",
                    escape_bibtex(publisher)
                );
            }
        }
        ItemType::Book => {
            if let Some(publisher) = &item.data.publisher {
                let _ = writeln!(
                    entry,
                    "  publisher = {{{}}},",
                    escape_bibtex(publisher)
                );
            }
        }
        ItemType::Report => {
            if let Some(inst) = &item.data.institution {
                let _ = writeln!(
                    entry,
                    "  institution = {{{}}},",
                    escape_bibtex(inst)
                );
            }
        }
        ItemType::Webpage
        | ItemType::BlogPost
        | ItemType::ForumPost
        | ItemType::Preprint => {
            if let Some(site) = &item.data.publication_title {
                if is_biblatex {
                    let _ = writeln!(
                        entry,
                        "  organization = {{{}}},",
                        escape_bibtex(site)
                    );
                } else {
                    let _ = writeln!(
                        entry,
                        "  howpublished = {{{}}},",
                        escape_bibtex(site)
                    );
                }
            }
        }
        _ => {
            if let Some(pub_title) = &item.data.publication_title {
                let _ = writeln!(
                    entry,
                    "  journal = {{{}}},",
                    escape_bibtex(pub_title)
                );
            }
        }
    }
    if let Some(date) = &item.data.date {
        let year = date.chars().take(4).collect::<String>();
        if year.chars().all(|c| c.is_ascii_digit()) && year.len() == 4 {
            let _ = writeln!(entry, "  year = {{{year}}},");
        } else {
            let _ = writeln!(entry, "  year = {{{}}},", escape_bibtex(date));
        }
    }

    if let Some(vol) = &item.data.volume {
        let _ = writeln!(entry, "  volume = {{{}}},", escape_bibtex(vol));
    }

    if let Some(issue) = &item.data.issue {
        let _ = writeln!(entry, "  number = {{{}}},", escape_bibtex(issue));
    }

    if let Some(pages) = &item.data.pages {
        let _ = writeln!(entry, "  pages = {{{}}},", escape_bibtex(pages));
    }

    if let Some(doi) = &item.data.doi {
        let _ = writeln!(entry, "  doi = {{{}}},", escape_bibtex(doi));
    }

    if let Some(url) = &item.data.url {
        let _ = writeln!(entry, "  url = {{{}}},", escape_bibtex(url));
    }

    if let Some(isbn) = &item.data.isbn {
        let _ = writeln!(entry, "  isbn = {{{}}},", escape_bibtex(isbn));
    }
    if let Some(issn) = &item.data.issn {
        let _ = writeln!(entry, "  issn = {{{}}},", escape_bibtex(issn));
    }

    entry.push('}');
    entry
}
/// Serializes multiple items with custom citation keys into a single `BibTeX`
/// or `BibLaTeX` string block.
///
/// # Arguments
///
/// * `items` - Slice of tuples containing each item and its assigned citation
///   key
/// * `style` - Serialization format: `"bibtex"` or `"biblatex"`
#[must_use]
#[inline]
pub fn items_to_bibtex(items: &[(ZoteroItem, String)], style: &str) -> String {
    let mut out = String::new();
    for (i, (item, citekey)) in items.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&item_to_bibtex(item, citekey, style));
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        keys::{ItemKey, LibraryVersion},
        objects::ZoteroItemData,
    };

    #[test]
    fn serializes_journal_article_to_bibtex() {
        let item = ZoteroItem {
            key: ItemKey::from("ITEM0001"),
            version: LibraryVersion(1),
            library: None,
            links: None,
            meta: None,
            data: ZoteroItemData {
                key: ItemKey::from("ITEM0001"),
                version: LibraryVersion(1),
                item_type: ItemType::JournalArticle,
                title: Some("Quantum & Classical Mechanics".to_owned()),
                creators: vec![
                    ZoteroCreator {
                        first_name: Some("Alice".to_owned()),
                        last_name: Some("Smith".to_owned()),
                        name: None,
                        creator_type: Option::default(),
                    },
                    ZoteroCreator {
                        first_name: Some("Bob".to_owned()),
                        last_name: Some("Jones".to_owned()),
                        name: None,
                        creator_type: Option::default(),
                    },
                ],
                publication_title: Some("Journal of Physics".to_owned()),
                date: Some("2024-05-15".to_owned()),
                doi: Some("10.1000/1234".to_owned()),
                ..Default::default()
            },
        };

        let bibtex = item_to_bibtex(&item, "Smith2024", "bibtex");
        assert_eq!(
            bibtex,
            "@article{Smith2024,\n  title = {Quantum \\& Classical \
             Mechanics},\n  author = {Smith, Alice and Jones, Bob},\n  \
             journal = {Journal of Physics},\n  year = {2024},\n  doi = \
             {10.1000/1234},\n}"
        );
    }

    #[test]
    fn serializes_webpage_to_biblatex_online_entry() {
        let item = ZoteroItem {
            key: ItemKey::from("WEB00001"),
            version: LibraryVersion(1),
            library: None,
            links: None,
            meta: None,
            data: ZoteroItemData {
                key: ItemKey::from("WEB00001"),
                version: LibraryVersion(1),
                item_type: ItemType::Webpage,
                title: Some("Zotero Docs".to_owned()),
                url: Some("https://zotero.org/doc".to_owned()),
                publication_title: Some("Zotero Org".to_owned()),
                ..Default::default()
            },
        };

        let biblatex = item_to_bibtex(&item, "zotero_docs", "biblatex");
        assert_eq!(
            biblatex,
            "@online{zotero_docs,\n  title = {Zotero Docs},\n  organization = {Zotero Org},\n  url = {https://zotero.org/doc},\n}"
        );
    }
}
