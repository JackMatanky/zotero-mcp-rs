//! MCP tool handlers for Zotero PDF annotations and synthesis.
//!
//! Main types:
//! - [`SynthesizeAnnotationsArgs`] - Arguments for the `synthesize` action
//! - [`CreateAnnotationArgs`] - Arguments for the `annotation` action

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    mcp::{json_result, text_result},
    zotero::{
        AnnotationDraft, AnnotationPosition, AnnotationType, ItemKey,
        ZoteroClient,
    },
};

/// Arguments for the `synthesize` action of `zotero_notes`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct SynthesizeAnnotationsArgs {
    /// Zotero item key ([`ItemKey`]).
    item_key: ItemKey,
}
/// Arguments for the `annotation` action of `zotero_notes_write`.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CreateAnnotationArgs {
    /// Key of the parent PDF attachment ([`ItemKey`]).
    parent_attachment_key: ItemKey,
    /// Type of annotation ([`AnnotationType`]).
    annotation_type: AnnotationType,
    /// Selected text (required for highlight/underline, omit for note).
    text: Option<String>,
    /// Optional user comment attached to the annotation.
    comment: Option<String>,
    /// CSS-style hex color, e.g. `"#ffd400"`.
    color: Option<String>,
    /// Optional PDF page label where the annotation appears.
    page_label: Option<String>,
    /// Zotero `annotationPosition` JSON object.
    position: AnnotationPosition,
}

impl ZoteroMcpServer {
    /// Handles Zotero annotation synthesis tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_synthesize_annotations_impl(
        &self,
        args: SynthesizeAnnotationsArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        Ok(text_result(client.synthesize_annotations(&args.item_key).await))
    }

    /// Handles Zotero PDF annotation creation tool calls.
    ///
    /// # Errors
    ///
    /// Returns [`rmcp::ErrorData`] for protocol-level failures. Backend
    /// failures are returned as MCP error content.
    pub(in crate::mcp::zotero) async fn zotero_create_annotation_impl(
        &self,
        args: CreateAnnotationArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = ZoteroClient::new(&self.state);
        let draft = AnnotationDraft {
            parent_attachment_key: args.parent_attachment_key,
            annotation_type: args.annotation_type,
            text: args.text,
            comment: args.comment,
            color: args.color,
            page_label: args.page_label,
            position: args.position,
        };
        Ok(json_result(client.create_annotation(draft).await))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        ZoteroMcpServer,
        mcp::zotero::fixtures::*,
        zotero::{AnnotationPosition, AnnotationType},
    };

    mod write_operations {

        use super::*;

        #[tokio::test]
        async fn create_annotation_creates_pdf_annotation() {
            // Arrange
            let created = json!([{
                "key": "ANNOT1",
                "version": 1,
                "data": { "key": "ANNOT1", "version": 1, "itemType": "annotation", "annotationType": "highlight" }
            }]);
            let base = mock_server(vec![http_response(
                "200 OK",
                &created.to_string(),
            )]);
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
    }
}
