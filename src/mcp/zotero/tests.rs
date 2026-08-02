//! Tests for the `mcp::zotero` grouped-router tool handlers.
//!
//! Organized by testing concern (read passthroughs, write-permission
//! gating, PDF path security, identifier resolution, relations, local
//! SQLite search) rather than by domain module, since most cases exercise
//! the grouped-router permission/security boundary shared across domains.

use serde_json::json;

use super::{
    collections::*, items::*, notes::*, pdf::*, relations::*, search::*,
    sqlite::*, tags::*,
};
use crate::{
    ZoteroMcpServer,
    security::SecurityConfig,
    state::AppState,
    zotero::{AnnotationPosition, AnnotationType},
};

mod fixtures {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    use rmcp::model::CallToolResult;
    use serde_json::json;

    use crate::{security::SecurityConfig, state::AppState};
    pub(super) fn zotero_state(zotero_api_url: String) -> AppState {
        AppState {
            zotero_api_url,
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: String::new(),
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: true,
            ..AppState::from_env()
        }
    }

    pub(super) fn http_response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: \
             application/json\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    pub(super) fn http_response_with_headers(
        status: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> String {
        let hdrs = headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}\r\n"))
            .collect::<Vec<_>>()
            .join("");
        format!(
            "HTTP/1.1 {status}\r\n{hdrs}Content-Length: {}\r\nContent-Type: \
             application/json\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    pub(super) fn mock_server(responses: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
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

    pub(super) fn security_with_pdf_limit(
        max_pdf_bytes: u64,
    ) -> SecurityConfig {
        SecurityConfig {
            max_pdf_bytes,
            ..SecurityConfig::default()
        }
    }

    pub(super) fn parent_journal_item() -> serde_json::Value {
        json!({
            "key": "ITEM0001",
            "version": 1,
            "data": {
                "key": "ITEM0001",
                "version": 1,
                "itemType": "journalArticle",
            },
        })
    }

    pub(super) fn zotero_pdf_server(children: serde_json::Value) -> String {
        mock_server(vec![
            http_response("200 OK", &parent_journal_item().to_string()),
            http_response("200 OK", &children.to_string()),
        ])
    }

    pub(super) fn bridge_pdf_root(
        kind: &str,
        path: &std::path::Path,
    ) -> String {
        let body = json!({
            "roots": [{
                "kind": kind,
                "path": path.canonicalize().unwrap(),
            }],
        });
        mock_server(vec![http_response("200 OK", &body.to_string())])
    }

    pub(super) fn tool_text(res: &CallToolResult) -> String {
        res.content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.to_string())
            .unwrap_or_default()
    }
}

use fixtures::*;

mod read_operations {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn get_recent_returns_items() {
        // Arrange
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Test Title" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_get_recent_impl(GetRecentArgs {
                limit: Some(10),
            })
            .await
            .expect("get recent ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn get_unfiled_items_returns_items() {
        // Arrange
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "title": "Unfiled Item", "collections": [] }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &items.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_get_unfiled_items_impl(GetUnfiledItemsArgs {
                limit: Some(50),
            })
            .await
            .expect("get unfiled ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn list_tags_returns_tags() {
        // Arrange
        let tags = json!([{"tag": "quantum", "meta": {"numItems": 3}}]);
        let base =
            mock_server(vec![http_response("200 OK", &tags.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_list_tags_impl(ListTagsArgs {
                limit: Some(50),
            })
            .await
            .expect("list tags ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }
}

mod write_operations {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn delete_item_deletes_item() {
        // Arrange
        let item = json!({
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle" }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("204 No Content", ""),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_delete_item_impl(DeleteItemArgs {
                item_key: "ITEM1".into(),
            })
            .await
            .expect("delete item ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn trash_item_moves_item_to_trash() {
        // Arrange
        let item = json!({
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle" }
        });
        let updated = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "deleted": true }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_trash_item_impl(TrashItemArgs {
                item_key: "ITEM1".into(),
            })
            .await
            .expect("trash item ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn restore_item_restores_item_from_trash() {
        // Arrange
        let item = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "deleted": true }
        });
        let updated = json!({
            "key": "ITEM1",
            "version": 3,
            "data": { "key": "ITEM1", "version": 3, "itemType": "journalArticle", "deleted": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &item.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_restore_item_impl(TrashItemArgs {
                item_key: "ITEM1".into(),
            })
            .await
            .expect("restore item ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn delete_collection_removes_collection() {
        // Arrange
        let collection = json!({
            "key": "COL1",
            "version": 1,
            "data": { "key": "COL1", "name": "Old Collection", "parentCollection": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &collection.to_string()),
            http_response("204 No Content", ""),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_delete_collection_impl(DeleteCollectionArgs {
                collection_key: "COL1".into(),
            })
            .await
            .expect("delete collection ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn update_collection_renames_collection() {
        // Arrange
        let current = json!({
            "key": "COL1",
            "version": 3,
            "data": { "key": "COL1", "name": "Old Name", "parentCollection": false }
        });
        let updated = json!({
            "key": "COL1",
            "version": 4,
            "data": { "key": "COL1", "name": "New Name", "parentCollection": false }
        });
        let base = mock_server(vec![
            http_response("200 OK", &current.to_string()),
            http_response("200 OK", &updated.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_update_collection_impl(UpdateCollectionArgs {
                collection_key: "COL1".into(),
                name: Some("New Name".to_owned()),
                parent_key: None,
            })
            .await
            .expect("update collection ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn rename_tag_patches_item_tags() {
        // Arrange
        let items = json!([{
            "key": "ITEM1",
            "version": 1,
            "data": { "key": "ITEM1", "version": 1, "itemType": "journalArticle", "tags": [{ "tag": "old_tag" }] }
        }]);
        let patched = json!({
            "key": "ITEM1",
            "version": 2,
            "data": { "key": "ITEM1", "version": 2, "itemType": "journalArticle", "tags": [{ "tag": "new_tag" }] }
        });
        let base = mock_server(vec![
            http_response("200 OK", &items.to_string()),
            http_response("200 OK", &patched.to_string()),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_rename_tag_impl(RenameTagArgs {
                old_tag: "old_tag".into(),
                new_tag: "new_tag".into(),
            })
            .await
            .expect("rename tag ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn delete_tags_removes_tags() {
        // Arrange
        let base = mock_server(vec![
            http_response_with_headers(
                "200 OK",
                &[("Last-Modified-Version", "9")],
                "[]",
            ),
            http_response("204 No Content", ""),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_delete_tags_impl(DeleteTagsArgs {
                tags: vec!["old_tag".into()],
            })
            .await
            .expect("delete tags ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn create_annotation_creates_pdf_annotation() {
        // Arrange
        let created = json!([{
            "key": "ANNOT1",
            "version": 1,
            "data": { "key": "ANNOT1", "version": 1, "itemType": "annotation", "annotationType": "highlight" }
        }]);
        let base =
            mock_server(vec![http_response("200 OK", &created.to_string())]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_create_annotation_impl(CreateAnnotationArgs {
                parent_attachment_key: "ATT1".into(),
                annotation_type: AnnotationType::Highlight,
                text: Some("selected text".to_owned()),
                comment: None,
                color: None,
                page_label: None,
                position: AnnotationPosition::from(
                    json!({"pageIndex": 0, "rects": [[100, 200, 300, 220]]}),
                ),
            })
            .await
            .expect("create annotation ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn delete_item_returns_error_when_write_disabled() {
        // Arrange
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: String::new(),
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            write_enabled: false,
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_delete_item_impl(DeleteItemArgs {
                item_key: "ITEM1".into(),
            })
            .await
            .expect("write disabled result");

        // Assert
        assert_eq!(res.is_error, Some(true));
    }
}

mod identifiers {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn add_by_identifier_creates_new_item() {
        // Arrange
        let crossref_body = json!({"message": {
            "title": ["A Great Paper"],
            "author": [{"given": "Sam", "family": "McAuthor"}],
            "published": {"date-parts": [[2021]]},
            "DOI": "10.1/xyz",
            "URL": "https://doi.org/10.1/xyz",
            "container-title": ["Journal of Things"]
        }});
        let crossref_base = mock_server(vec![http_response(
            "200 OK",
            &crossref_body.to_string(),
        )]);
        let created = json!([{
            "key": "NEWITEM1",
            "version": 1,
            "data": { "key": "NEWITEM1", "version": 1, "itemType": "journalArticle", "title": "A Great Paper" }
        }]);
        let zotero_base = mock_server(vec![
            http_response("200 OK", "[]"),
            http_response("200 OK", &created.to_string()),
        ]);
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: crossref_base,
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: true,
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_add_by_identifier_impl(AddByIdentifierArgs {
                kind: crate::zotero::IdentifierKind::Doi,
                identifier: "10.1/xyz".to_owned(),
                collection_key: None,
            })
            .await
            .expect("add by identifier ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
    }

    #[tokio::test]
    async fn add_by_identifier_returns_existing_item_when_duplicate_found() {
        // Arrange
        let crossref_body = json!({"message": {
            "title": ["A Great Paper"],
            "author": [{"given": "Sam", "family": "McAuthor"}],
            "published": {"date-parts": [[2021]]},
            "DOI": "10.1/xyz",
            "URL": "https://doi.org/10.1/xyz",
            "container-title": ["Journal of Things"]
        }});
        let crossref_base = mock_server(vec![http_response(
            "200 OK",
            &crossref_body.to_string(),
        )]);
        let existing = json!([{
            "key": "EXISTING1",
            "version": 1,
            "data": { "key": "EXISTING1", "version": 1, "itemType": "journalArticle", "title": "A Great Paper" }
        }]);
        let zotero_base =
            mock_server(vec![http_response("200 OK", &existing.to_string())]);
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            crossref_url: crossref_base,
            semantic_scholar_url: String::new(),
            open_library_url: String::new(),
            write_enabled: true,
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_add_by_identifier_impl(AddByIdentifierArgs {
                kind: crate::zotero::IdentifierKind::Doi,
                identifier: "10.1/xyz".to_owned(),
                collection_key: None,
            })
            .await
            .expect("add by identifier duplicate ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
        let text = tool_text(&res);
        assert!(text.contains("EXISTING1"));
    }
}

mod pdf_pages {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn rejects_direct_path_by_default() {
        // Arrange
        let temp = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let server = ZoteroMcpServer::new(AppState {
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: temp.path().display().to_string(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("Direct file paths are disabled"));
    }

    #[tokio::test]
    async fn allows_direct_path_inside_bridge_pdf_root_without_direct_flag() {
        // Arrange
        let root = tempfile::TempDir::new().unwrap();
        let pdf = root.path().join("bad.pdf");
        std::fs::write(&pdf, b"not a pdf").unwrap();
        let body = json!({
            "roots": [{
                "kind": "attanger-dest",
                "path": root.path().canonicalize().unwrap(),
            }],
        });
        let bridge_base =
            mock_server(vec![http_response("200 OK", &body.to_string())]);
        let server = ZoteroMcpServer::new(AppState {
            better_notes_url: bridge_base,
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: pdf.display().to_string(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("PDF extraction error"));
    }

    #[tokio::test]
    async fn rejects_direct_path_outside_bridge_pdf_roots() {
        // Arrange
        let root = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let pdf = outside.path().join("bad.pdf");
        std::fs::write(&pdf, b"not a pdf").unwrap();
        let body = json!({
            "roots": [{
                "kind": "attanger-dest",
                "path": root.path().canonicalize().unwrap(),
            }],
        });
        let bridge_base =
            mock_server(vec![http_response("200 OK", &body.to_string())]);
        let server = ZoteroMcpServer::new(AppState {
            better_notes_url: bridge_base,
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: pdf.display().to_string(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("Direct file paths are disabled"));
    }

    #[tokio::test]
    async fn allows_direct_path_inside_configured_root_when_bridge_unavailable()
    {
        // Arrange
        let root = tempfile::TempDir::new().unwrap();
        let pdf = root.path().join("bad.pdf");
        std::fs::write(&pdf, b"not a pdf").unwrap();
        let mut security = SecurityConfig::default();
        security.direct_file_paths = true;
        security.allowed_read_dirs = vec![root.path().canonicalize().unwrap()];
        let server = ZoteroMcpServer::new(AppState {
            better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
            security,
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: pdf.display().to_string(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("PDF extraction error"));
    }

    #[tokio::test]
    async fn rejects_direct_path_outside_allowed_root() {
        // Arrange
        let allowed = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let pdf = outside.path().join("bad.pdf");
        std::fs::write(&pdf, b"not a pdf").unwrap();
        let mut security = SecurityConfig::default();
        security.direct_file_paths = true;
        security.allowed_read_dirs =
            vec![allowed.path().canonicalize().unwrap()];
        let server = ZoteroMcpServer::new(AppState {
            security,
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: pdf.display().to_string(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("outside allowed"));
    }

    #[tokio::test]
    async fn reads_imported_attachment_enclosure_without_allowed_dirs() {
        // Arrange
        let pdf = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        std::fs::write(pdf.path(), b"not a pdf").unwrap();
        let file_url =
            url::Url::from_file_path(pdf.path()).unwrap().to_string();
        let children = json!([{
            "key": "PDF00001",
            "version": 1,
            "links": {
                "enclosure": {
                    "href": file_url,
                    "type": "application/pdf",
                    "title": "bad.pdf",
                },
            },
            "data": {
                "key": "PDF00001",
                "version": 1,
                "itemType": "attachment",
                "linkMode": "imported_file",
                "contentType": "application/pdf",
                "filename": "bad.pdf",
            },
        }]);
        let zotero_base = zotero_pdf_server(children);
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: "ITEM0001".to_owned(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("PDF extraction error"));
    }

    #[tokio::test]
    async fn reads_linked_attanger_attachment_inside_bridge_root() {
        // Arrange
        let root = tempfile::TempDir::new().unwrap();
        let pdf = root.path().join("bad.pdf");
        std::fs::write(&pdf, b"not a pdf").unwrap();
        let children = json!([{
            "key": "PDF00001",
            "version": 1,
            "data": {
                "key": "PDF00001",
                "version": 1,
                "itemType": "attachment",
                "linkMode": "linked_file",
                "contentType": "application/pdf",
                "path": pdf.display().to_string(),
            },
        }]);
        let zotero_base = zotero_pdf_server(children);
        let bridge_base = bridge_pdf_root("attanger-dest", root.path());
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_notes_url: bridge_base,
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: "ITEM0001".to_owned(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("PDF extraction error"));
    }

    #[tokio::test]
    async fn rejects_linked_attachment_outside_pdf_roots() {
        // Arrange
        let root = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let pdf = outside.path().join("bad.pdf");
        std::fs::write(&pdf, b"not a pdf").unwrap();
        let children = json!([{
            "key": "PDF00001",
            "version": 1,
            "data": {
                "key": "PDF00001",
                "version": 1,
                "itemType": "attachment",
                "linkMode": "linked_file",
                "contentType": "application/pdf",
                "path": pdf.display().to_string(),
            },
        }]);
        let zotero_base = zotero_pdf_server(children);
        let bridge_base = bridge_pdf_root("attanger-dest", root.path());
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_notes_url: bridge_base,
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: "ITEM0001".to_owned(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("outside allowed"));
    }

    #[tokio::test]
    async fn resolves_relative_linked_attachment_from_zotero_base_root() {
        // Arrange
        let base = tempfile::TempDir::new().unwrap();
        let subdir = base.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        let pdf = subdir.join("bad.pdf");
        std::fs::write(&pdf, b"not a pdf").unwrap();
        let children = json!([{
            "key": "PDF00001",
            "version": 1,
            "data": {
                "key": "PDF00001",
                "version": 1,
                "itemType": "attachment",
                "linkMode": "linked_file",
                "contentType": "application/pdf",
                "path": "attachments:subdir/bad.pdf",
            },
        }]);
        let zotero_base = zotero_pdf_server(children);
        let bridge_base = bridge_pdf_root("zotero-linked-base", base.path());
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_notes_url: bridge_base,
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_read_pdf_pages_impl(ReadPdfPagesArgs {
                item_key_or_path: "ITEM0001".to_owned(),
                pages: None,
            })
            .await
            .expect("read pdf pages result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("PDF extraction error"));
    }
}

mod pdf_outline {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn rejects_direct_path_by_default() {
        // Arrange
        let temp = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let server = ZoteroMcpServer::new(AppState {
            security: security_with_pdf_limit(1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                item_key_or_path: temp.path().display().to_string(),
            })
            .await
            .expect("get pdf outline result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("Direct file paths are disabled"));
    }

    #[tokio::test]
    async fn returns_outline_for_direct_path_inside_configured_root() {
        // Arrange
        let root = tempfile::TempDir::new().unwrap();
        let pdf = root.path().join("outline.pdf");
        crate::pdf::write_pdf_with_outline(&pdf);
        let mut security = SecurityConfig::default();
        security.direct_file_paths = true;
        security.allowed_read_dirs = vec![root.path().canonicalize().unwrap()];
        let server = ZoteroMcpServer::new(AppState {
            security,
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                item_key_or_path: pdf.display().to_string(),
            })
            .await
            .expect("get pdf outline result");

        // Assert
        assert_eq!(res.is_error, Some(false));
        let text = tool_text(&res);
        assert!(text.contains("Chapter 1"));
        assert!(text.contains("Section 2.1"));
    }

    #[tokio::test]
    async fn returns_empty_outline_for_pdf_without_bookmarks() {
        // Arrange
        let root = tempfile::TempDir::new().unwrap();
        let pdf = root.path().join("plain.pdf");
        crate::pdf::write_pdf_without_outline(&pdf);
        let mut security = SecurityConfig::default();
        security.direct_file_paths = true;
        security.allowed_read_dirs = vec![root.path().canonicalize().unwrap()];
        let server = ZoteroMcpServer::new(AppState {
            security,
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                item_key_or_path: pdf.display().to_string(),
            })
            .await
            .expect("get pdf outline result");

        // Assert
        assert_eq!(res.is_error, Some(false));
        assert!(tool_text(&res).contains("[]"));
    }

    #[tokio::test]
    async fn reads_imported_attachment_enclosure_outline() {
        // Arrange
        let pdf = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        crate::pdf::write_pdf_with_outline(pdf.path());
        let file_url =
            url::Url::from_file_path(pdf.path()).unwrap().to_string();
        let children = json!([{
            "key": "PDF00001",
            "version": 1,
            "links": {
                "enclosure": {
                    "href": file_url,
                    "type": "application/pdf",
                    "title": "outline.pdf",
                },
            },
            "data": {
                "key": "PDF00001",
                "version": 1,
                "itemType": "attachment",
                "linkMode": "imported_file",
                "contentType": "application/pdf",
                "filename": "outline.pdf",
            },
        }]);
        let zotero_base = zotero_pdf_server(children);
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: zotero_base,
            better_notes_url: "http://127.0.0.1:9/better-notes".to_owned(),
            security: security_with_pdf_limit(1024 * 1024),
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_get_pdf_outline_impl(GetPdfOutlineArgs {
                item_key_or_path: "ITEM0001".to_owned(),
            })
            .await
            .expect("get pdf outline result");

        // Assert
        assert_eq!(res.is_error, Some(false));
        assert!(tool_text(&res).contains("Chapter 1"));
    }
}

mod related_items {
    use pretty_assertions::assert_eq;

    use super::*;

    fn item_json(key: &str, relations: &serde_json::Value) -> String {
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

    fn related_item_json(key: &str, title: &str) -> String {
        serde_json::json!({
            "key": key,
            "version": 1,
            "data": {
                "key": key,
                "version": 1,
                "itemType": "journalArticle",
                "title": title,
            },
        })
        .to_string()
    }

    const URI_A_TO_B: &str = "http://zotero.org/users/0/items/ITEM0002";
    const URI_B_TO_A: &str = "http://zotero.org/users/0/items/ITEM0001";

    #[tokio::test]
    async fn get_related_items_returns_related_items() {
        // Arrange
        let source = item_json(
            "ITEM0001",
            &serde_json::json!({
                "dc:relation": [URI_A_TO_B],
            }),
        );
        let base = mock_server(vec![
            http_response("200 OK", &source),
            http_response(
                "200 OK",
                &related_item_json("ITEM0002", "Related Article"),
            ),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_get_related_items_impl(GetRelatedItemsArgs {
                item_key: "ITEM0001".into(),
            })
            .await
            .expect("get related items ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
        let text = tool_text(&res);
        assert!(text.contains("ITEM0002"));
        assert!(text.contains("Related Article"));
    }

    #[tokio::test]
    async fn add_item_relation_links_items_and_returns_success() {
        // Arrange
        let base = mock_server(vec![
            http_response("200 OK", &item_json("ITEM0001", &json!({}))),
            http_response("200 OK", &item_json("ITEM0002", &json!({}))),
            http_response(
                "200 OK",
                &item_json(
                    "ITEM0001",
                    &serde_json::json!({
                        "dc:relation": [URI_A_TO_B],
                    }),
                ),
            ),
            http_response(
                "200 OK",
                &item_json(
                    "ITEM0002",
                    &serde_json::json!({
                        "dc:relation": [URI_B_TO_A],
                    }),
                ),
            ),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_add_item_relation_impl(AddItemRelationArgs {
                item_key: "ITEM0001".into(),
                related_item_key: "ITEM0002".into(),
            })
            .await
            .expect("add item relation ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
        assert!(tool_text(&res).contains("Item relation added"));
    }

    #[tokio::test]
    async fn add_item_relation_returns_error_when_write_disabled() {
        // Arrange
        let server = ZoteroMcpServer::new(AppState {
            zotero_api_url: String::new(),
            better_bibtex_url: String::new(),
            better_notes_url: String::new(),
            write_enabled: false,
            ..AppState::from_env()
        });

        // Act
        let res = server
            .zotero_add_item_relation_impl(AddItemRelationArgs {
                item_key: "ITEM0001".into(),
                related_item_key: "ITEM0002".into(),
            })
            .await
            .expect("write disabled result");

        // Assert
        assert_eq!(res.is_error, Some(true));
        assert!(tool_text(&res).contains("Permission denied"));
    }

    #[tokio::test]
    async fn remove_item_relation_unlinks_items_and_returns_success() {
        // Arrange
        let base = mock_server(vec![
            http_response(
                "200 OK",
                &item_json(
                    "ITEM0001",
                    &serde_json::json!({
                        "dc:relation": [URI_A_TO_B],
                    }),
                ),
            ),
            http_response(
                "200 OK",
                &item_json(
                    "ITEM0002",
                    &serde_json::json!({
                        "dc:relation": [URI_B_TO_A],
                    }),
                ),
            ),
            http_response("200 OK", &item_json("ITEM0001", &json!({}))),
            http_response("200 OK", &item_json("ITEM0002", &json!({}))),
        ]);
        let server = ZoteroMcpServer::new(zotero_state(base));

        // Act
        let res = server
            .zotero_remove_item_relation_impl(RemoveItemRelationArgs {
                item_key: "ITEM0001".into(),
                related_item_key: "ITEM0002".into(),
            })
            .await
            .expect("remove item relation ok");

        // Assert
        assert_eq!(res.is_error, Some(false));
        assert!(tool_text(&res).contains("Item relation removed"));
    }
}

mod sqlite_tools {
    use std::{path::Path, str::FromStr};

    use pretty_assertions::assert_eq;
    use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

    use super::*;

    #[expect(
        clippy::too_many_lines,
        reason = "seeds a realistic Zotero schema across many tables"
    )]
    async fn seed_db(path: &Path) {
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            path.display()
        ))
        .unwrap()
        .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();
        sqlx::query(
            "CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName \
             TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE items (itemID INTEGER PRIMARY KEY, key TEXT, \
             itemTypeID INTEGER, dateAdded TEXT, dateModified TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE fields (fieldID INTEGER PRIMARY KEY, fieldName TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemData (itemID INTEGER, fieldID INTEGER, valueID \
             INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, value \
             TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, firstName \
             TEXT, lastName TEXT, fieldMode INT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemCreators (itemID INTEGER, creatorID INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE deletedItems (itemID INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE fulltextWords (wordID INTEGER PRIMARY KEY, word \
             TEXT UNIQUE)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE fulltextItemWords (wordID INT, itemID INT, PRIMARY \
             KEY (wordID, itemID))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemNotes (itemID INTEGER, parentItemID INTEGER, \
             note TEXT, title TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemAnnotations (itemID INTEGER, parentItemID \
             INTEGER, text TEXT, comment TEXT, type INTEGER, color TEXT, \
             pageLabel TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE itemAttachments (itemID INTEGER, parentItemID \
             INTEGER, path TEXT, contentType TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO fields (fieldID, fieldName) VALUES (1, 'title'), \
             (16, 'extra'), (7, 'DOI')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemTypes (itemTypeID, typeName) VALUES (1, \
             'journalArticle'), (2, 'note'), (3, 'attachment')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
             dateModified) VALUES (1, 'K00001', 1, '2024-01-01', '2024-02-01')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemData (itemID, fieldID, valueID) VALUES (1, 1, \
             100), (1, 7, 101)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemDataValues (valueID, value) VALUES (100, 'Rust \
             in Action'), (101, '10.1000/rust')",
        )
        .execute(&pool)
        .await
        .unwrap();
        // attachment child (item 3) carries the indexed fulltext words
        sqlx::query(
            "INSERT INTO items (itemID, key, itemTypeID, dateAdded, \
             dateModified) VALUES (3, 'A00001', 3, '2024-01-02', '2024-02-02')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO itemAttachments (itemID, parentItemID, path, \
             contentType) VALUES (3, 1, 'storage:K00001.pdf', \
             'application/pdf')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO fulltextWords (wordID, word) VALUES (1, 'the'), (2, \
             'borrow'), (3, 'checker'), (4, 'ensures'), (5, 'memory'), (6, \
             'safety')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO fulltextItemWords (wordID, itemID) VALUES (1, 3), \
             (2, 3), (3, 3), (4, 3), (5, 3), (6, 3)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn fulltext_tool_returns_gate_error_when_disabled() {
        let mut state = zotero_state(String::new());
        state.sqlite_access = false;
        let server = ZoteroMcpServer::new(state.clone());
        let res = server
            .zotero_fulltext_search_impl(FulltextSearchArgs {
                query: "borrow".to_owned(),
                limit: Some(10),
            })
            .await
            .unwrap();
        let text = tool_text(&res);
        assert!(text.contains("ZOTERO_SQLITE_ACCESS"));
    }

    #[tokio::test]
    async fn fulltext_tool_returns_hits_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        seed_db(&db_path).await;

        let mut state = zotero_state(String::new());
        state.sqlite_access = true;
        state.zotero_db_path = Some(db_path);
        let server = ZoteroMcpServer::new(state);
        let res = server
            .zotero_fulltext_search_impl(FulltextSearchArgs {
                query: "borrow checker".to_owned(),
                limit: Some(10),
            })
            .await
            .unwrap();
        let text = tool_text(&res);
        assert!(text.contains("Rust in Action"));
    }

    #[tokio::test]
    async fn fulltext_tool_uses_state_db_path_without_env_var() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zotero.sqlite");
        seed_db(&db_path).await;
        let previous = std::env::var_os("ZOTERO_DB_PATH");
        std::env::remove_var("ZOTERO_DB_PATH");

        let mut state = zotero_state(String::new());
        state.sqlite_access = true;
        state.zotero_db_path = Some(db_path);
        let server = ZoteroMcpServer::new(state);
        let res = server
            .zotero_fulltext_search_impl(FulltextSearchArgs {
                query: "borrow checker".to_owned(),
                limit: Some(10),
            })
            .await
            .unwrap();

        if let Some(value) = previous {
            std::env::set_var("ZOTERO_DB_PATH", value);
        }
        let text = tool_text(&res);
        assert_eq!(res.is_error, Some(false));
        assert!(text.contains("Rust in Action"));
    }
}
