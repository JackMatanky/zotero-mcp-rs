//! Metadata resolution for adding items by DOI, arXiv ID, or ISBN.

use serde::{Deserialize, Serialize};

use crate::{
    errors::ZoteroMcpError,
    state::AppState,
    zotero::models::{CollectionKey, CreatorType, ItemType, ZoteroCreator},
};

/// A Zotero item ready for creation, resolved from a public identifier
/// lookup (DOI, arXiv ID, or ISBN).
///
/// Typed in place of a raw `serde_json::Value` so a typo in a field name
/// fails to compile instead of silently producing a malformed Zotero item.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ItemDraft {
    #[serde(rename = "itemType")]
    pub(crate) item_type: ItemType,
    pub(crate) title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) creators: Vec<ZoteroCreator>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) date: String,
    #[serde(rename = "DOI", default, skip_serializing_if = "String::is_empty")]
    pub(crate) doi: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) publication_title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) abstract_note: String,
    #[serde(
        rename = "ISBN",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub(crate) isbn: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) publisher: String,
    /// Collections to file the created item into.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) collections: Vec<CollectionKey>,
}

/// Public-identifier type for [`resolve_metadata`].
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum IdentifierKind {
    Doi,
    Arxiv,
    Isbn,
}

/// Resolves a public identifier against its metadata API and returns a
/// Zotero item draft ready for creation.
///
/// # Arguments
///
/// * `state` - Shared application state containing metadata API endpoints
/// * `kind` - Public identifier type ([`IdentifierKind::Doi`],
///   [`IdentifierKind::Arxiv`], or [`IdentifierKind::Isbn`])
/// * `id` - Identifier string to resolve
///
/// # Errors
///
/// - [`NotFound`] if the identifier cannot be resolved (404 status from the
///   source API)
/// - [`LocalApi`] if the source API responds with a non-2xx status other than
///   404
/// - [`Network`] if the request fails at the transport level
/// - [`Json`] if the metadata response cannot be decoded
///
/// [`NotFound`]: ZoteroMcpError::NotFound
/// [`LocalApi`]: ZoteroMcpError::LocalApi
/// [`Network`]: ZoteroMcpError::Network
/// [`Json`]: ZoteroMcpError::Json
pub(crate) async fn resolve_metadata(
    state: &AppState,
    kind: IdentifierKind,
    id: &str,
) -> Result<ItemDraft, ZoteroMcpError> {
    match kind {
        IdentifierKind::Doi => resolve_doi(state, id).await,
        IdentifierKind::Arxiv => resolve_arxiv(state, id).await,
        IdentifierKind::Isbn => resolve_isbn(state, id).await,
    }
}

async fn fetch_json(
    state: &AppState,
    url: &str,
) -> Result<serde_json::Value, ZoteroMcpError> {
    let resp = state.send_with_retry(state.client.get(url)).await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ZoteroMcpError::NotFound(format!(
            "No metadata found for {url}"
        )));
    }
    if !resp.status().is_success() {
        return Err(ZoteroMcpError::LocalApi {
            status: resp.status().as_u16(),
            message: resp.status().to_string(),
        });
    }
    Ok(resp.json().await?)
}

/// Reads a nested string field via a `.`-separated path of object keys and
/// array indices.
///
/// Returns `Some(&str)` if `path` resolves to a string value in `value`, or
/// `None` otherwise. Avoids indexing a [`serde_json::Value`] directly to
/// prevent panics on unexpected JSON shapes.
fn str_at<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for segment in path {
        current = match segment.parse::<usize>() {
            Ok(index) => current.get(index)?,
            Err(_) => current.get(segment)?,
        };
    }
    current.as_str()
}

fn i64_at(value: &serde_json::Value, path: &[&str]) -> Option<i64> {
    let mut current = value;
    for segment in path {
        current = match segment.parse::<usize>() {
            Ok(index) => current.get(index)?,
            Err(_) => current.get(segment)?,
        };
    }
    current.as_i64()
}

async fn resolve_doi(
    state: &AppState,
    doi: &str,
) -> Result<ItemDraft, ZoteroMcpError> {
    let url =
        format!("{}/works/{}", state.crossref_url, urlencoding::encode(doi));
    let body = fetch_json(state, &url).await?;
    let msg = body.get("message").cloned().unwrap_or_default();
    let title = str_at(&msg, &["title", "0"]).unwrap_or_default().to_owned();
    let creators = msg
        .get("author")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|a| ZoteroCreator {
            creator_type: Some(CreatorType::Author),
            first_name: Some(
                str_at(a, &["given"]).unwrap_or_default().to_owned(),
            ),
            last_name: Some(
                str_at(a, &["family"]).unwrap_or_default().to_owned(),
            ),
            name: None,
        })
        .collect();
    let year = i64_at(&msg, &["published", "date-parts", "0", "0"])
        .or_else(|| i64_at(&msg, &["issued", "date-parts", "0", "0"]));
    Ok(ItemDraft {
        item_type: ItemType::JournalArticle,
        title,
        creators,
        date: year.map(|y| y.to_string()).unwrap_or_default(),
        doi: str_at(&msg, &["DOI"]).unwrap_or(doi).to_owned(),
        url: str_at(&msg, &["URL"]).unwrap_or_default().to_owned(),
        publication_title: str_at(&msg, &["container-title", "0"])
            .unwrap_or_default()
            .to_owned(),
        ..ItemDraft::default()
    })
}

async fn resolve_arxiv(
    state: &AppState,
    arxiv_id: &str,
) -> Result<ItemDraft, ZoteroMcpError> {
    let url = format!(
        "{}/graph/v1/paper/arXiv:{}?fields=title,authors,year,abstract,\
         externalIds,venue",
        state.semantic_scholar_url, arxiv_id
    );
    let body = fetch_json(state, &url).await?;
    let title = str_at(&body, &["title"]).unwrap_or_default().to_owned();
    let creators = body
        .get("authors")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|a| ZoteroCreator {
            creator_type: Some(CreatorType::Author),
            first_name: None,
            last_name: None,
            name: Some(str_at(a, &["name"]).unwrap_or_default().to_owned()),
        })
        .collect();
    let doi = str_at(&body, &["externalIds", "DOI"]);
    Ok(ItemDraft {
        item_type: if doi.is_some() {
            ItemType::JournalArticle
        } else {
            ItemType::Preprint
        },
        title,
        creators,
        date: i64_at(&body, &["year"])
            .map(|y| y.to_string())
            .unwrap_or_default(),
        doi: doi.unwrap_or_default().to_owned(),
        url: format!("https://arxiv.org/abs/{arxiv_id}"),
        abstract_note: str_at(&body, &["abstract"])
            .unwrap_or_default()
            .to_owned(),
        publication_title: str_at(&body, &["venue"])
            .unwrap_or_default()
            .to_owned(),
        ..ItemDraft::default()
    })
}

async fn resolve_isbn(
    state: &AppState,
    isbn: &str,
) -> Result<ItemDraft, ZoteroMcpError> {
    let url = format!(
        "{}/api/books?bibkeys=ISBN:{}&jscmd=data&format=json",
        state.open_library_url, isbn
    );
    let body = fetch_json(state, &url).await?;
    let key = format!("ISBN:{isbn}");
    let Some(record) = body.get(&key) else {
        return Err(ZoteroMcpError::NotFound(format!(
            "No book found for ISBN {isbn}"
        )));
    };
    let title = str_at(record, &["title"]).unwrap_or_default().to_owned();
    let creators = record
        .get("authors")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|a| ZoteroCreator {
            creator_type: Some(CreatorType::Author),
            first_name: None,
            last_name: None,
            name: Some(str_at(a, &["name"]).unwrap_or_default().to_owned()),
        })
        .collect();
    let publisher = str_at(record, &["publishers", "0", "name"])
        .unwrap_or_default()
        .to_owned();
    Ok(ItemDraft {
        item_type: ItemType::Book,
        title,
        creators,
        date: str_at(record, &["publish_date"]).unwrap_or_default().to_owned(),
        isbn: isbn.to_owned(),
        publisher,
        url: str_at(record, &["url"]).unwrap_or_default().to_owned(),
        ..ItemDraft::default()
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    mod fixtures {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        pub(super) fn http_response(status: &str, body: &str) -> String {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
                 application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }

        pub(super) fn mock_server(responses: Vec<String>) -> String {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let addr = listener.local_addr().expect("local addr");
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) =
                        listener.accept().expect("accept connection");
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            format!("http://{addr}")
        }
    }

    use fixtures::*;

    fn state_with(
        crossref: String,
        semantic_scholar: String,
        open_library: String,
    ) -> AppState {
        AppState {
            crossref_url: crossref,
            semantic_scholar_url: semantic_scholar,
            open_library_url: open_library,
            ..AppState::from_env()
        }
    }

    mod resolve_doi {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn parses_crossref_response_into_item_draft() {
            let body = json!({"message": {
                "title": ["A Great Paper"],
                "author": [{"given": "Sam", "family": "McAuthor"}],
                "published": {"date-parts": [[2021]]},
                "DOI": "10.1/xyz",
                "URL": "https://doi.org/10.1/xyz",
                "container-title": ["Journal of Things"]
            }});
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let state = state_with(base, String::new(), String::new());

            let draft =
                resolve_metadata(&state, IdentifierKind::Doi, "10.1/xyz")
                    .await
                    .unwrap();
            assert_eq!(draft.title, "A Great Paper");
            assert_eq!(draft.item_type, ItemType::JournalArticle);
            assert_eq!(
                draft.creators.first().and_then(|c| c.last_name.as_deref()),
                Some("McAuthor")
            );
            assert_eq!(draft.date, "2021");
        }

        #[tokio::test]
        async fn returns_not_found_on_404() {
            let base = mock_server(vec![http_response("404 Not Found", "{}")]);
            let state = state_with(base, String::new(), String::new());

            let err =
                resolve_metadata(&state, IdentifierKind::Doi, "10.1/missing")
                    .await
                    .unwrap_err();
            assert!(matches!(err, ZoteroMcpError::NotFound(_)));
        }
    }

    mod resolve_arxiv {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn parses_semantic_scholar_response_into_item_draft() {
            let body = json!({
                "title": "Attention Is All You Need",
                "authors": [{"name": "A. Vaswani"}],
                "year": 2017,
                "abstract": "We propose...",
                "externalIds": {"DOI": null},
                "venue": "NeurIPS"
            });
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let state = state_with(String::new(), base, String::new());

            let draft =
                resolve_metadata(&state, IdentifierKind::Arxiv, "1706.03762")
                    .await
                    .unwrap();
            assert_eq!(draft.title, "Attention Is All You Need");
            assert_eq!(draft.item_type, ItemType::Preprint);
            assert_eq!(draft.url, "https://arxiv.org/abs/1706.03762");
        }

        #[tokio::test]
        async fn returns_not_found_on_404() {
            let base = mock_server(vec![http_response("404 Not Found", "{}")]);
            let state = state_with(String::new(), base, String::new());

            let err =
                resolve_metadata(&state, IdentifierKind::Arxiv, "0000.00000")
                    .await
                    .unwrap_err();
            assert!(matches!(err, ZoteroMcpError::NotFound(_)));
        }
    }

    mod resolve_isbn {
        use pretty_assertions::assert_eq;

        use super::*;

        #[tokio::test]
        async fn parses_open_library_response_into_item_draft() {
            let body = json!({"ISBN:9780134685991": {
                "title": "Effective Java",
                "authors": [{"name": "Joshua Bloch"}],
                "publish_date": "2018",
                "publishers": [{"name": "Addison-Wesley"}]
            }});
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let state = state_with(String::new(), String::new(), base);

            let draft =
                resolve_metadata(&state, IdentifierKind::Isbn, "9780134685991")
                    .await
                    .unwrap();
            assert_eq!(draft.title, "Effective Java");
            assert_eq!(draft.item_type, ItemType::Book);
            assert_eq!(draft.publisher, "Addison-Wesley");
        }

        #[tokio::test]
        async fn returns_not_found_when_isbn_is_missing_from_response() {
            let body = json!({});
            let base =
                mock_server(vec![http_response("200 OK", &body.to_string())]);
            let state = state_with(String::new(), String::new(), base);

            let err =
                resolve_metadata(&state, IdentifierKind::Isbn, "9780000000000")
                    .await
                    .unwrap_err();
            assert!(matches!(err, ZoteroMcpError::NotFound(_)));
        }
    }

    mod helpers {
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn str_at_resolves_nested_path() {
            let json = json!({
                "a": {
                    "b": [
                        {"c": "target_value"}
                    ]
                }
            });
            assert_eq!(
                str_at(&json, &["a", "b", "0", "c"]),
                Some("target_value")
            );
        }

        #[test]
        fn str_at_returns_none_on_missing_or_wrong_type() {
            let json = json!({"a": 123});
            assert_eq!(str_at(&json, &["a"]), None);
            assert_eq!(str_at(&json, &["b"]), None);
        }

        #[test]
        fn i64_at_resolves_number() {
            let json = json!({"year": 2024});
            assert_eq!(i64_at(&json, &["year"]), Some(2024));
        }

        #[test]
        fn i64_at_returns_none_on_missing_or_wrong_type() {
            let json = json!({"year": "2024"});
            assert_eq!(i64_at(&json, &["year"]), None);
            assert_eq!(i64_at(&json, &["missing"]), None);
        }
    }
}
