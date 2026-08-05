//! Related-item relations for the Zotero Local HTTP API.
//!
//! Zotero stores item links in each item's `relations` map under the
//! `dc:relation` predicate. The value may be a single URI string or an array of
//! URI strings, depending on count.
//!
//! The helpers keep that wire format manageable:
//!
//! - [`parse_relation_keys`]: reads both single-string and array forms.
//! - [`apply_relations`]: computes idempotent add/remove patches while
//!   preserving unrelated relation predicates.
//!
//! [`ZoteroClient::get_related_items`] resolves relation URIs to items.
//! [`ZoteroClient::add_item_relation`] and
//! [`ZoteroClient::remove_item_relation`] patch both endpoints of a link.
//!
//! # Examples
//!
//! ```no_run
//! # use zotero_mcp_rs::state::AppState;
//! # use zotero_mcp_rs::zotero::client::ZoteroClient;
//! # use zotero_mcp_rs::zotero::ItemKey;
//! # async fn run(
//! #     state: &AppState,
//! #     key_a: &ItemKey,
//! #     key_b: &ItemKey,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let client = ZoteroClient::new(state);
//! client.add_item_relation(key_a, key_b).await?;
//! let related = client.get_related_items(key_a).await?;
//! println!("Item has {} relations", related.len());
//! # Ok(())
//! # }
//! ```

use std::collections::BTreeSet;

use serde::Serialize;

use crate::{
    errors::ZoteroMcpError,
    zotero::{ItemKey, ItemType, client::ZoteroClient, keys::RelationUri},
};

/// A single item linked to another via a `dc:relation` URI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RelatedItem {
    /// Item key of the related item.
    pub(crate) key: ItemKey,
    /// Optional item title.
    pub(crate) title: Option<String>,
    /// Item type of the related item.
    pub(crate) item_type: ItemType,
}

impl ZoteroClient<'_> {
    /// Fetches the items linked to `item_key` via `dc:relation`.
    ///
    /// Returns a vector of [`RelatedItem`] structures for all resolved relation
    /// URIs. Relation values that do not carry a Zotero item key are
    /// skipped, as are keys the Local API 404s on (e.g. group library items
    /// invisible to the `/users/0` client).
    ///
    /// # Errors
    ///
    /// - [`NotFound`] if `item_key` does not exist
    /// - [`LocalApi`] if Zotero responds with a non-2xx status code
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if the response cannot be decoded
    ///
    /// [`NotFound`]: ZoteroMcpError::NotFound
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(crate) async fn get_related_items(
        &self,
        item_key: &ItemKey,
    ) -> Result<Vec<RelatedItem>, ZoteroMcpError> {
        let item = self.get_item(item_key).await?;
        let mut related = Vec::new();
        for uri in parse_relation_keys(&item.data.relations) {
            let Ok(key) = ItemKey::try_from(&uri) else {
                continue;
            };
            match self.get_item(&key).await {
                Ok(related_item) => related.push(RelatedItem {
                    key: related_item.key,
                    title: related_item.data.title,
                    item_type: related_item.data.item_type,
                }),
                Err(ZoteroMcpError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(related)
    }

    /// Links `a` and `b` bidirectionally by adding each other's URI to their
    /// `dc:relation` maps.
    ///
    /// The two `PATCH` requests are not transactional: if the second fails,
    /// the first has already landed. Both operations are set-based and
    /// idempotent, so a retry is safe.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if write permission is disabled in configuration
    /// - [`InputRejected`] if `a` and `b` are the same item key
    /// - [`NotFound`] if either item does not exist
    /// - [`LocalApi`] if Zotero responds with a non-2xx status code
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if the response cannot be decoded
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`InputRejected`]: ZoteroMcpError::InputRejected
    /// [`NotFound`]: ZoteroMcpError::NotFound
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(crate) async fn add_item_relation(
        &self,
        a: &ItemKey,
        b: &ItemKey,
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        if a == b {
            return Err(ZoteroMcpError::InputRejected(
                "cannot relate an item to itself".to_owned(),
            ));
        }
        let a_item = self.get_item(a).await?;
        let b_item = self.get_item(b).await?;
        let a_relations = apply_relations(
            &a_item.data.relations,
            &[RelationUri::from(b)],
            &[],
        );
        let b_relations = apply_relations(
            &b_item.data.relations,
            &[RelationUri::from(a)],
            &[],
        );
        self.update_item(
            a,
            serde_json::json!({
                "relations": a_relations,
                "version": a_item.version,
            }),
        )
        .await?;
        self.update_item(
            b,
            serde_json::json!({
                "relations": b_relations,
                "version": b_item.version,
            }),
        )
        .await?;
        Ok(())
    }

    /// Unlinks `a` and `b` bidirectionally by removing each other's URI from
    /// their `dc:relation` maps.
    ///
    /// The two `PATCH` requests are not transactional: if the second fails,
    /// the first has already landed. Both operations are set-based and
    /// idempotent, so a retry is safe.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`] if write permission is disabled in configuration
    /// - [`NotFound`] if either item does not exist
    /// - [`LocalApi`] if Zotero responds with a non-2xx status code
    /// - [`Network`] if the request fails at the transport level
    /// - [`Json`] if the response cannot be decoded
    ///
    /// [`PermissionDenied`]: ZoteroMcpError::PermissionDenied
    /// [`NotFound`]: ZoteroMcpError::NotFound
    /// [`LocalApi`]: ZoteroMcpError::LocalApi
    /// [`Network`]: ZoteroMcpError::Network
    /// [`Json`]: ZoteroMcpError::Json
    pub(crate) async fn remove_item_relation(
        &self,
        a: &ItemKey,
        b: &ItemKey,
    ) -> Result<(), ZoteroMcpError> {
        self.state.check_write_permission()?;
        let a_item = self.get_item(a).await?;
        let b_item = self.get_item(b).await?;
        let a_relations =
            apply_relations(&a_item.data.relations, &[], &[RelationUri::from(
                b,
            )]);
        let b_relations =
            apply_relations(&b_item.data.relations, &[], &[RelationUri::from(
                a,
            )]);
        self.update_item(
            a,
            serde_json::json!({
                "relations": a_relations,
                "version": a_item.version,
            }),
        )
        .await?;
        self.update_item(
            b,
            serde_json::json!({
                "relations": b_relations,
                "version": b_item.version,
            }),
        )
        .await?;
        Ok(())
    }
}

/// Reads the `dc:relation` URI values from an item's `relations` map,
/// accepting either a single URI string or an array of URI strings (Zotero
/// switches forms by value count). Missing, empty, and non-string entries are
/// ignored.
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
///
/// # Arguments
///
/// * `current` - Current `relations` JSON map from the item payload
/// * `add` - Slice of relation URIs to add
/// * `remove` - Slice of relation URIs to remove
pub(crate) fn apply_relations(
    current: &serde_json::Value,
    add: &[RelationUri],
    remove: &[RelationUri],
) -> serde_json::Value {
    let mut uris: BTreeSet<String> = parse_relation_keys(current)
        .into_iter()
        .map(|u| u.as_str().to_owned())
        .collect();
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

    mod fixtures {
        use crate::state::AppState;
        pub(super) use crate::zotero::test_http::{
            MockServer, http_response, request_body,
        };

        /// Builds an [`AppState`] fixture for testing with `zotero_api_url` and
        /// `write_enabled`.
        pub(super) fn test_state(
            zotero_api_url: impl AsRef<str>,
            write_enabled: bool,
        ) -> AppState {
            AppState::test_default()
                .with_zotero_api_url(zotero_api_url.as_ref())
                .with_write_enabled(write_enabled)
        }

        /// Serializes a minimal [`ZoteroItem`]-shaped JSON response body for
        /// `key` with the given `relations` map.
        pub(super) fn item_json(
            key: &str,
            relations: &serde_json::Value,
        ) -> String {
            serde_json::json!({
                "key": key,
                "version": 1,
                "data": {
                    "key": key,
                    "version": 1,
                    "itemType": "journalArticle",
                    "relations": relations.clone(),
                },
            })
            .to_string()
        }
    }

    mod get_related_items {
        use pretty_assertions::assert_eq;

        use super::{
            fixtures::{MockServer, http_response, test_state},
            *,
        };
        use crate::zotero::{ItemKey, ItemType};

        #[tokio::test]
        async fn resolves_related_items_and_skips_unresolvable_keys() {
            let source = serde_json::json!({
                "key": "ITEM0001",
                "version": 1,
                "data": {
                    "key": "ITEM0001",
                    "version": 1,
                    "itemType": "journalArticle",
                    "relations": {
                        "dc:relation": [
                            "http://zotero.org/users/0/items/ITEM0002",
                            "http://zotero.org/groups/1/items/ITEM0003",
                            "https://example.com/not-a-zotero-uri",
                        ],
                    },
                },
            });
            let related_book = serde_json::json!({
                "key": "ITEM0002",
                "version": 1,
                "data": {
                    "key": "ITEM0002",
                    "version": 1,
                    "itemType": "book",
                    "title": "Related Book",
                },
            });
            let server = MockServer::new(vec![
                http_response("200 OK", &source.to_string()),
                http_response("200 OK", &related_book.to_string()),
                http_response("404 Not Found", ""),
            ]);
            let base = server.url();
            let state = test_state(base, false);

            let related = ZoteroClient::new(&state)
                .get_related_items(&ItemKey::from("ITEM0001"))
                .await
                .unwrap();

            assert_eq!(related, vec![RelatedItem {
                key: ItemKey::from("ITEM0002"),
                title: Some("Related Book".to_owned()),
                item_type: ItemType::Book,
            }]);
        }

        #[tokio::test]
        async fn propagates_non_404_errors_from_related_item_fetch() {
            let source = serde_json::json!({
                "key": "ITEM0001",
                "version": 1,
                "data": {
                    "key": "ITEM0001",
                    "version": 1,
                    "itemType": "journalArticle",
                    "relations": {
                        "dc:relation": "http://zotero.org/users/0/items/ITEM0002",
                    },
                },
            });
            // 500 is transient, so the retry policy sends the related-item GET
            // up to `RETRY_MAX_ATTEMPTS` times before surfacing the error.
            let server = MockServer::new(vec![
                http_response("200 OK", &source.to_string()),
                http_response("500 Internal Server Error", ""),
                http_response("500 Internal Server Error", ""),
                http_response("500 Internal Server Error", ""),
            ]);
            let base = server.url();
            let state = test_state(base, false);

            let err = ZoteroClient::new(&state)
                .get_related_items(&ItemKey::from("ITEM0001"))
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroMcpError::LocalApi {
                status: 500,
                ..
            }));
        }
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "test assertions index recorded requests by fixed position"
    )]
    mod add_item_relation {
        use super::{
            fixtures::{
                MockServer, http_response, item_json, request_body, test_state,
            },
            *,
        };
        use crate::zotero::ItemKey;

        #[tokio::test]
        async fn patches_both_items_with_each_others_uri() {
            let (server, recorded) = MockServer::recording(vec![
                http_response(
                    "200 OK",
                    &item_json("ITEM0001", &serde_json::json!({})),
                ),
                http_response(
                    "200 OK",
                    &item_json("ITEM0002", &serde_json::json!({})),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0001",
                        &serde_json::json!({
                            "dc:relation": [
                                "http://zotero.org/users/0/items/ITEM0002",
                            ],
                        }),
                    ),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0002",
                        &serde_json::json!({
                            "dc:relation": [
                                "http://zotero.org/users/0/items/ITEM0001",
                            ],
                        }),
                    ),
                ),
            ]);
            let base = server.url();
            let state = test_state(base, true);

            let result = ZoteroClient::new(&state)
                .add_item_relation(
                    &ItemKey::from("ITEM0001"),
                    &ItemKey::from("ITEM0002"),
                )
                .await;

            assert!(result.is_ok());
            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 4);
            assert!(requests[0].starts_with("GET /users/0/items/ITEM0001"));
            assert!(requests[1].starts_with("GET /users/0/items/ITEM0002"));
            assert!(requests[2].starts_with("PATCH /users/0/items/ITEM0001"));
            assert!(requests[3].starts_with("PATCH /users/0/items/ITEM0002"));

            let body_a = request_body(&requests[2]);
            assert!(body_a.is_ok(), "request body should be JSON: {body_a:?}");
            let body_a = body_a.unwrap_or_default();
            assert_eq!(
                body_a["relations"]["dc:relation"],
                serde_json::json!(["http://zotero.org/users/0/items/ITEM0002"])
            );
            assert_eq!(body_a["version"], 1);

            let body_b = request_body(&requests[3]);
            assert!(body_b.is_ok(), "request body should be JSON: {body_b:?}");
            let body_b = body_b.unwrap_or_default();
            assert_eq!(
                body_b["relations"]["dc:relation"],
                serde_json::json!(["http://zotero.org/users/0/items/ITEM0001"])
            );
            assert_eq!(body_b["version"], 1);
        }

        #[tokio::test]
        async fn rejects_self_relation() {
            let state = test_state(String::new(), true);

            let err = ZoteroClient::new(&state)
                .add_item_relation(
                    &ItemKey::from("ITEM0001"),
                    &ItemKey::from("ITEM0001"),
                )
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroMcpError::InputRejected(_)));
        }

        #[tokio::test]
        async fn denies_writes_when_write_permission_is_disabled() {
            let state = test_state(String::new(), false);

            let err = ZoteroClient::new(&state)
                .add_item_relation(
                    &ItemKey::from("ITEM0001"),
                    &ItemKey::from("ITEM0002"),
                )
                .await
                .unwrap_err();

            assert!(matches!(err, ZoteroMcpError::PermissionDenied(_)));
        }
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "test assertions index recorded requests by fixed position"
    )]
    mod remove_item_relation {
        use super::{
            fixtures::{
                MockServer, http_response, item_json, request_body, test_state,
            },
            *,
        };
        use crate::zotero::ItemKey;

        #[tokio::test]
        async fn patches_both_items_removing_each_others_uri() {
            let (server, recorded) = MockServer::recording(vec![
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0001",
                        &serde_json::json!({
                            "dc:relation": [
                                "http://zotero.org/users/0/items/ITEM0002",
                            ],
                        }),
                    ),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0002",
                        &serde_json::json!({
                            "dc:relation": [
                                "http://zotero.org/users/0/items/ITEM0001",
                            ],
                        }),
                    ),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0001",
                        &serde_json::json!({
                            "dc:relation": [],
                        }),
                    ),
                ),
                http_response(
                    "200 OK",
                    &item_json(
                        "ITEM0002",
                        &serde_json::json!({
                            "dc:relation": [],
                        }),
                    ),
                ),
            ]);
            let base = server.url();
            let state = test_state(base, true);

            let result = ZoteroClient::new(&state)
                .remove_item_relation(
                    &ItemKey::from("ITEM0001"),
                    &ItemKey::from("ITEM0002"),
                )
                .await;

            assert!(result.is_ok());
            let requests = recorded.lock().expect("request log lock");
            assert_eq!(requests.len(), 4);
            assert!(requests[2].starts_with("PATCH /users/0/items/ITEM0001"));
            assert!(requests[3].starts_with("PATCH /users/0/items/ITEM0002"));

            let body_a = request_body(&requests[2]);
            assert!(body_a.is_ok(), "request body should be JSON: {body_a:?}");
            let body_a = body_a.unwrap_or_default();
            assert_eq!(
                body_a["relations"]["dc:relation"],
                serde_json::json!([])
            );
            assert_eq!(body_a["version"], 1);

            let body_b = request_body(&requests[3]);
            assert!(body_b.is_ok(), "request body should be JSON: {body_b:?}");
            let body_b = body_b.unwrap_or_default();
            assert_eq!(
                body_b["relations"]["dc:relation"],
                serde_json::json!([])
            );
            assert_eq!(body_b["version"], 1);
        }
    }
}
