//! Metadata resolution for adding items by DOI, arXiv ID, or ISBN.

use serde::Deserialize;

use crate::{errors::ZoteroMcpError, state::AppState};

/// Public-identifier type for [`resolve_metadata`].
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum IdentifierKind {
    Doi,
    Arxiv,
    Isbn,
}

/// Resolves a public identifier against its metadata API and returns a Zotero
/// item draft.
///
/// Returns a JSON object structured for Zotero item creation (`itemType`,
/// `title`, `creators`, `date`, `url`, and `DOI`/`ISBN` as applicable).
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
) -> Result<serde_json::Value, ZoteroMcpError> {
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
) -> Result<serde_json::Value, ZoteroMcpError> {
    let url =
        format!("{}/works/{}", state.crossref_url, urlencoding::encode(doi));
    let body = fetch_json(state, &url).await?;
    let msg = body.get("message").cloned().unwrap_or_default();
    let title = str_at(&msg, &["title", "0"]).unwrap_or_default();
    let creators: Vec<serde_json::Value> = msg
        .get("author")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| {
            serde_json::json!({
                "creatorType": "author",
                "firstName": str_at(a, &["given"]).unwrap_or_default(),
                "lastName": str_at(a, &["family"]).unwrap_or_default(),
            })
        })
        .collect();
    let year = i64_at(&msg, &["published", "date-parts", "0", "0"])
        .or_else(|| i64_at(&msg, &["issued", "date-parts", "0", "0"]));
    Ok(serde_json::json!({
        "itemType": "journalArticle",
        "title": title,
        "creators": creators,
        "date": year.map(|y| y.to_string()).unwrap_or_default(),
        "DOI": str_at(&msg, &["DOI"]).unwrap_or(doi),
        "url": str_at(&msg, &["URL"]).unwrap_or_default(),
        "publicationTitle": str_at(&msg, &["container-title", "0"]).unwrap_or_default(),
    }))
}

async fn resolve_arxiv(
    state: &AppState,
    arxiv_id: &str,
) -> Result<serde_json::Value, ZoteroMcpError> {
    let url = format!(
        "{}/graph/v1/paper/arXiv:{}?fields=title,authors,year,abstract,\
         externalIds,venue",
        state.semantic_scholar_url, arxiv_id
    );
    let body = fetch_json(state, &url).await?;
    let title = str_at(&body, &["title"]).unwrap_or_default();
    let creators: Vec<serde_json::Value> = body
        .get("authors")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| {
            serde_json::json!({"creatorType": "author", "name": str_at(a, &["name"]).unwrap_or_default()})
        })
        .collect();
    let doi = str_at(&body, &["externalIds", "DOI"]);
    Ok(serde_json::json!({
        "itemType": if doi.is_some() { "journalArticle" } else { "preprint" },
        "title": title,
        "creators": creators,
        "date": i64_at(&body, &["year"]).map(|y| y.to_string()).unwrap_or_default(),
        "DOI": doi.unwrap_or_default(),
        "url": format!("https://arxiv.org/abs/{arxiv_id}"),
        "abstractNote": str_at(&body, &["abstract"]).unwrap_or_default(),
        "publicationTitle": str_at(&body, &["venue"]).unwrap_or_default(),
    }))
}

async fn resolve_isbn(
    state: &AppState,
    isbn: &str,
) -> Result<serde_json::Value, ZoteroMcpError> {
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
    let title = str_at(record, &["title"]).unwrap_or_default();
    let creators: Vec<serde_json::Value> = record
        .get("authors")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| serde_json::json!({"creatorType": "author", "name": str_at(a, &["name"]).unwrap_or_default()}))
        .collect();
    let publisher =
        str_at(record, &["publishers", "0", "name"]).unwrap_or_default();
    Ok(serde_json::json!({
        "itemType": "book",
        "title": title,
        "creators": creators,
        "date": str_at(record, &["publish_date"]).unwrap_or_default(),
        "ISBN": isbn,
        "publisher": publisher,
        "url": str_at(record, &["url"]).unwrap_or_default(),
    }))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::{
        super::client::tests::fixtures::{http_response, mock_server},
        *,
    };

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

    #[tokio::test]
    async fn resolve_doi_maps_crossref_fields() {
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

        let draft = resolve_metadata(&state, IdentifierKind::Doi, "10.1/xyz")
            .await
            .unwrap();
        assert_eq!(str_at(&draft, &["title"]), Some("A Great Paper"));
        assert_eq!(str_at(&draft, &["itemType"]), Some("journalArticle"));
        assert_eq!(
            str_at(&draft, &["creators", "0", "lastName"]),
            Some("McAuthor")
        );
        assert_eq!(str_at(&draft, &["date"]), Some("2021"));
    }

    #[tokio::test]
    async fn resolve_doi_returns_not_found_on_404() {
        let base = mock_server(vec![http_response("404 Not Found", "{}")]);
        let state = state_with(base, String::new(), String::new());

        let err = resolve_metadata(&state, IdentifierKind::Doi, "10.1/missing")
            .await
            .unwrap_err();
        assert!(matches!(err, ZoteroMcpError::NotFound(_)));
    }

    #[tokio::test]
    async fn resolve_arxiv_maps_semantic_scholar_fields() {
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
        assert_eq!(
            str_at(&draft, &["title"]),
            Some("Attention Is All You Need")
        );
        assert_eq!(str_at(&draft, &["itemType"]), Some("preprint"));
        assert_eq!(
            str_at(&draft, &["url"]),
            Some("https://arxiv.org/abs/1706.03762")
        );
    }

    #[tokio::test]
    async fn resolve_isbn_maps_open_library_fields() {
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
        assert_eq!(str_at(&draft, &["title"]), Some("Effective Java"));
        assert_eq!(str_at(&draft, &["itemType"]), Some("book"));
        assert_eq!(str_at(&draft, &["publisher"]), Some("Addison-Wesley"));
    }
}
