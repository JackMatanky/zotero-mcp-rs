use crate::better_notes::models::{
    BetterNotesStatus, MarkdownResponse, NoteItemResponse, NoteTreeResponse, RelationsResponse,
    TemplateResponse,
};
use crate::errors::ZoteroMcpError;
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;

#[expect(dead_code, reason = "Client invoked by MCP tool handlers")]
pub struct BetterNotesClient<'a> {
    state: &'a AppState,
}

#[expect(dead_code, reason = "Client methods invoked by MCP tool handlers")]
impl<'a> BetterNotesClient<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub async fn check_status(&self) -> BetterNotesStatus {
        let url = format!("{}/status", self.state.better_notes_url);
        match self.state.client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let val: serde_json::Value = resp.json().await.unwrap_or_default();
                    BetterNotesStatus {
                        online: true,
                        url: self.state.better_notes_url.clone(),
                        version: val
                            .get("version")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        error: None,
                    }
                } else {
                    BetterNotesStatus {
                        online: false,
                        url: self.state.better_notes_url.clone(),
                        version: None,
                        error: Some(format!("HTTP {}", resp.status())),
                    }
                }
            }
            Err(e) => BetterNotesStatus {
                online: false,
                url: self.state.better_notes_url.clone(),
                version: None,
                error: Some(e.to_string()),
            },
        }
    }

    async fn post_json<P: Serialize, R: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        payload: P,
    ) -> Result<R, ZoteroMcpError> {
        let url = format!("{}{}", self.state.better_notes_url, endpoint);
        let resp = self.state.client.post(&url).json(&payload).send().await?;

        if !resp.status().is_success() {
            return Err(ZoteroMcpError::BetterNotes(format!(
                "HTTP {} calling {}",
                resp.status(),
                endpoint
            )));
        }

        let res: R = resp.json().await?;
        Ok(res)
    }

    pub async fn to_markdown(
        &self,
        item_key: Option<&str>,
        html: Option<&str>,
    ) -> Result<String, ZoteroMcpError> {
        let payload = serde_json::json!({
            "itemKey": item_key,
            "html": html,
        });
        let res: MarkdownResponse = self.post_json("/notes/to-markdown", payload).await?;
        Ok(res.markdown)
    }

    pub async fn convert_from_markdown(
        &self,
        parent_key: &str,
        markdown: &str,
    ) -> Result<String, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let payload = serde_json::json!({
            "parentKey": parent_key,
            "markdown": markdown,
        });
        let res: NoteItemResponse = self.post_json("/notes/from-markdown", payload).await?;
        Ok(res.item_key)
    }

    pub async fn run_template(&self, name: &str, item_key: &str) -> Result<Value, ZoteroMcpError> {
        let payload = serde_json::json!({
            "name": name,
            "itemKey": item_key,
        });
        let res: TemplateResponse = self.post_json("/templates/run", payload).await?;
        Ok(res.result)
    }

    pub async fn get_relations(&self, item_key: &str) -> Result<Value, ZoteroMcpError> {
        let payload = serde_json::json!({
            "itemKey": item_key,
        });
        let res: RelationsResponse = self.post_json("/relations/get", payload).await?;
        Ok(res.relations)
    }

    pub async fn get_tree(&self, item_key: &str) -> Result<Value, ZoteroMcpError> {
        let payload = serde_json::json!({
            "itemKey": item_key,
        });
        let res: NoteTreeResponse = self.post_json("/notes/tree", payload).await?;
        Ok(res.tree)
    }
}
