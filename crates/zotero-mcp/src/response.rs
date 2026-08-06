//! Formats [`ZoteroApiError`] and successful values into MCP
//! [`CallToolResult`] responses.
//!
//! Kept separate from `zotero_api::errors::ZoteroApiError` per the
//! domain/protocol error boundary: client-facing sanitization is an MCP
//! concern, not a domain concern.

use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use zotero_api::ZoteroApiError;

/// Returns a sanitized error message suitable for external MCP clients,
/// suppressing sensitive internal paths, system details, and database
/// queries.
pub(crate) fn client_message(err: &ZoteroApiError) -> String {
    match err {
        ZoteroApiError::Sqlite(_) => "Local database query failed".to_owned(),
        ZoteroApiError::Io(err) => format!("I/O error: {}", err.kind()),
        ZoteroApiError::Network(_) => {
            "Upstream network request failed".to_owned()
        }
        ZoteroApiError::LocalApi {
            status,
            message,
        } => {
            format!("Local API HTTP {status}: {message}")
        }
        _ => err.to_string(),
    }
}

/// Wraps `text` in a successful [`CallToolResult`].
pub(crate) fn text_success(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text.into())])
}

/// Wraps `error` in an error [`CallToolResult`], sanitizing internal details.
pub(crate) fn text_error(error: &ZoteroApiError) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(client_message(error))])
}

/// Wraps `result` or `error` in a [`CallToolResult`], matching on [`Result`].
pub(crate) fn text_result(
    result: Result<String, ZoteroApiError>,
) -> CallToolResult {
    match result {
        Ok(text) => text_success(text),
        Err(e) => text_error(&e),
    }
}

/// Wraps `value` as pretty-printed JSON in a successful [`CallToolResult`],
/// or an error result if serialization fails.
pub(crate) fn json_success<T: Serialize>(value: &T) -> CallToolResult {
    serde_json::to_string_pretty(value)
        .map_or_else(|e| text_error(&ZoteroApiError::Json(e)), text_success)
}

/// Wraps `result` or `error` in a JSON [`CallToolResult`], matching on
/// [`Result`].
pub(crate) fn json_result<T: Serialize>(
    result: Result<T, ZoteroApiError>,
) -> CallToolResult {
    match result {
        Ok(value) => json_success(&value),
        Err(e) => text_error(&e),
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::*;

    mod formatting {
        use pretty_assertions::assert_eq;

        use super::*;

        #[derive(Serialize)]
        struct SampleData {
            id: u32,
            name: String,
        }

        #[test]
        fn text_success_wraps_text_in_successful_result() {
            // Act
            let res = text_success("Operation completed");

            assert_eq!(res.is_error, Some(false));
            assert_eq!(res.content.len(), 1);
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str());
            assert_eq!(text, Some("Operation completed"));
        }

        #[test]
        fn text_error_wraps_error_in_error_result() {
            let err =
                ZoteroApiError::BetterNotes("Something went wrong".to_owned());
            let res = text_error(&err);

            // Assert
            assert_eq!(res.is_error, Some(true));
            assert_eq!(res.content.len(), 1);
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str());
            assert_eq!(text, Some("Better Notes error: Something went wrong"));
        }

        #[test]
        fn text_result_converts_ok_to_success() {
            // Arrange
            let res_ok: Result<String, ZoteroApiError> =
                Ok("Success payload".to_owned());

            // Act
            let tool_res = text_result(res_ok);

            assert_eq!(tool_res.is_error, Some(false));
        }

        #[test]
        fn text_result_converts_err_to_error() {
            // Arrange
            let res_err: Result<String, ZoteroApiError> =
                Err(ZoteroApiError::BetterNotes("Failure payload".to_owned()));

            // Act
            let tool_res = text_result(res_err);

            // Assert
            assert_eq!(tool_res.is_error, Some(true));
        }

        #[test]
        fn json_success_formats_value_as_pretty_json() {
            // Arrange
            let data = SampleData {
                id: 42,
                name: "Test Item".to_owned(),
            };

            // Act
            let res = json_success(&data);

            assert_eq!(res.is_error, Some(false));
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str())
                .unwrap_or_default();
            assert!(text.contains("\"id\": 42"));
            assert!(text.contains("\"name\": \"Test Item\""));
        }

        #[test]
        fn json_success_returns_error_result_when_serialization_fails() {
            // Arrange: a type whose `Serialize` impl always fails, since
            // ordinary Rust values (even non-finite floats) don't fail
            // `serde_json` serialization.
            struct Unrepresentable;
            impl Serialize for Unrepresentable {
                fn serialize<S: serde::Serializer>(
                    &self,
                    _serializer: S,
                ) -> Result<S::Ok, S::Error> {
                    Err(serde::ser::Error::custom("cannot represent value"))
                }
            }

            // Act
            let res = json_success(&Unrepresentable);

            // Assert
            assert_eq!(res.is_error, Some(true));
            let text = res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str())
                .unwrap_or_default();
            assert!(text.contains("JSON serialization error"));
        }

        #[test]
        fn json_result_converts_ok_to_json_success() {
            // Arrange
            let data = SampleData {
                id: 1,
                name: "Ok Item".to_owned(),
            };
            let res_ok: Result<SampleData, ZoteroApiError> = Ok(data);

            // Act
            let tool_res = json_result(res_ok);

            assert_eq!(tool_res.is_error, Some(false));
        }

        #[test]
        fn json_result_converts_err_to_text_error() {
            // Arrange
            let res_err: Result<SampleData, ZoteroApiError> =
                Err(ZoteroApiError::BetterNotes("JSON error".to_owned()));

            // Act
            let tool_res = json_result(res_err);

            // Assert
            assert_eq!(tool_res.is_error, Some(true));
        }

        #[test]
        fn sanitizes_sqlite_and_io_errors_for_clients() {
            let io_err = ZoteroApiError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "secret_path.txt not found",
            ));
            assert_eq!(client_message(&io_err), "I/O error: entity not found");
            assert!(!client_message(&io_err).contains("secret_path.txt"));

            let sqlite_err = ZoteroApiError::Sqlite(sqlx::Error::RowNotFound);
            assert_eq!(
                client_message(&sqlite_err),
                "Local database query failed"
            );

            let tool_res = text_error(&sqlite_err);
            let text = tool_res
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str())
                .unwrap_or_default();
            assert_eq!(text, "Local database query failed");
        }
    }
}
