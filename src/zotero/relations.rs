//! Helpers for reading and mutating Zotero item relation values.
//!
//! Zotero stores item links in each item's `relations` map under the
//! `dc:relation` predicate as URI values that are either a single string or an
//! array of strings (the API switches forms by value count). These pure
//! helpers parse both forms and compute set-based, idempotent add/remove
//! patches that always write `dc:relation` as an array.

use std::collections::BTreeSet;

use crate::zotero::models::RelationUri;

/// Reads the `dc:relation` URI values from an item's `relations` map,
/// accepting either a single URI string or an array of URI strings (Zotero
/// switches forms by value count). Missing, empty, and non-string entries are
/// ignored.
#[allow(
    dead_code,
    reason = "consumed by client methods in the relations module"
)]
pub(crate) fn parse_relation_keys(
    relations: &serde_json::Value,
) -> Vec<RelationUri> {
    let Some(dc_relation) = relations.get("dc:relation") else {
        return Vec::new();
    };
    match dc_relation {
        serde_json::Value::String(uri) => vec![RelationUri::from(uri.as_str())],
        serde_json::Value::Array(uris) => uris
            .iter()
            .filter_map(|v| v.as_str().map(RelationUri::from))
            .collect(),
        _ => Vec::new(),
    }
}

/// Computes the item's `relations` map after adding and removing `dc:relation`
/// URIs, preserving all other predicates verbatim.
///
/// `dc:relation` is always written as an array (the canonical multi-value form
/// per zotero/dataserver#74), even when it holds a single or zero URIs.
#[allow(
    dead_code,
    reason = "consumed by client methods in the relations module"
)]
pub(crate) fn apply_relations(
    current: &serde_json::Value,
    add: &[RelationUri],
    remove: &[RelationUri],
) -> serde_json::Value {
    let mut uris: BTreeSet<String> =
        parse_relation_keys(current).into_iter().map(|u| u.0).collect();
    for uri in add {
        uris.insert(uri.as_str().to_owned());
    }
    for uri in remove {
        uris.remove(uri.as_str());
    }
    let mut result: serde_json::Map<String, serde_json::Value> =
        current.as_object().cloned().unwrap_or_default();
    result.insert(
        "dc:relation".to_owned(),
        serde_json::Value::Array(
            uris.into_iter().map(serde_json::Value::String).collect(),
        ),
    );
    serde_json::Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const URI_A: &str = "http://zotero.org/users/0/items/AAAAAAAA";
    const URI_B: &str = "http://zotero.org/users/0/items/BBBBBBBB";

    mod parse_relation_keys {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn extracts_uris_from_array_form() {
            let relations = serde_json::json!({
                "dc:relation": [URI_A, URI_B]
            });

            let uris = super::parse_relation_keys(&relations);

            assert_eq!(uris, vec![
                RelationUri::from(URI_A),
                RelationUri::from(URI_B)
            ]);
        }

        #[test]
        fn extracts_uri_from_single_string_form() {
            let relations = serde_json::json!({ "dc:relation": URI_A });

            let uris = super::parse_relation_keys(&relations);

            assert_eq!(uris, vec![RelationUri::from(URI_A)]);
        }

        #[test]
        fn returns_empty_when_relations_missing() {
            let empty = super::parse_relation_keys(&serde_json::json!({}));
            assert!(empty.is_empty());

            let other_predicate_only = super::parse_relation_keys(
                &serde_json::json!({
                    "owl:sameAs": "http://zotero.org/groups/36222/items/AAAAAAAA"
                }),
            );
            assert!(other_predicate_only.is_empty());
        }

        #[test]
        fn returns_empty_for_empty_array() {
            let relations = serde_json::json!({ "dc:relation": [] });

            let uris = super::parse_relation_keys(&relations);

            assert!(uris.is_empty());
        }

        #[test]
        fn ignores_non_string_entries() {
            let relations =
                serde_json::json!({ "dc:relation": [123, { "x": 1 }] });

            let uris = super::parse_relation_keys(&relations);

            assert!(uris.is_empty());
        }
    }

    mod apply_relations {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn adds_new_uri() {
            let result = super::apply_relations(
                &serde_json::json!({}),
                &[RelationUri::from(URI_B)],
                &[],
            );

            assert_eq!(result, serde_json::json!({ "dc:relation": [URI_B] }));
        }

        #[test]
        fn is_idempotent_on_readd() {
            let current = serde_json::json!({ "dc:relation": [URI_A] });

            let result = super::apply_relations(
                &current,
                &[RelationUri::from(URI_A)],
                &[],
            );

            assert_eq!(result, serde_json::json!({ "dc:relation": [URI_A] }));
        }

        #[test]
        fn removes_uri_from_string_form() {
            let current = serde_json::json!({ "dc:relation": URI_A });

            let result =
                super::apply_relations(&current, &[], &[RelationUri::from(
                    URI_A,
                )]);

            assert_eq!(result, serde_json::json!({ "dc:relation": [] }));
        }

        #[test]
        fn removes_uri_from_array_form() {
            let current = serde_json::json!({ "dc:relation": [URI_A, URI_B] });

            let result =
                super::apply_relations(&current, &[], &[RelationUri::from(
                    URI_A,
                )]);

            assert_eq!(result, serde_json::json!({ "dc:relation": [URI_B] }));
        }

        #[test]
        fn writes_array_form_for_single_remaining_uri() {
            let current = serde_json::json!({ "dc:relation": URI_A });

            let result = super::apply_relations(&current, &[], &[]);

            assert_eq!(result, serde_json::json!({ "dc:relation": [URI_A] }));
        }

        #[test]
        fn preserves_other_predicates_verbatim() {
            let current = serde_json::json!({
                "owl:sameAs": "http://zotero.org/groups/36222/items/AAAAAAAA"
            });

            let result = super::apply_relations(
                &current,
                &[RelationUri::from(URI_B)],
                &[],
            );

            assert_eq!(
                result,
                serde_json::json!({
                    "owl:sameAs": "http://zotero.org/groups/36222/items/AAAAAAAA",
                    "dc:relation": [URI_B],
                })
            );
        }

        #[test]
        fn handles_empty_current_value() {
            let result =
                super::apply_relations(&serde_json::json!({}), &[], &[]);

            assert_eq!(result, serde_json::json!({ "dc:relation": [] }));
        }
    }
}
