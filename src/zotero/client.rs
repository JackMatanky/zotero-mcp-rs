use crate::errors::ZoteroMcpError;
use crate::state::AppState;
use crate::zotero::models::{LocalApiStatus, ZoteroCollection, ZoteroItem};
use reqwest::StatusCode;

#[expect(dead_code, reason = "Client invoked by MCP tool handlers")]
pub struct ZoteroClient<'a> {
    state: &'a AppState,
}

#[expect(dead_code, reason = "Client methods invoked by MCP tool handlers")]
impl<'a> ZoteroClient<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub async fn check_status(&self) -> LocalApiStatus {
        let url = format!("{}/users/0/items?limit=1", self.state.zotero_api_url);
        match self.state.client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    LocalApiStatus {
                        online: true,
                        url: self.state.zotero_api_url.clone(),
                        version: resp
                            .headers()
                            .get("zotero-api-version")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string()),
                        error: None,
                    }
                } else {
                    LocalApiStatus {
                        online: false,
                        url: self.state.zotero_api_url.clone(),
                        version: None,
                        error: Some(format!("HTTP status {}", status)),
                    }
                }
            }
            Err(e) => LocalApiStatus {
                online: false,
                url: self.state.zotero_api_url.clone(),
                version: None,
                error: Some(e.to_string()),
            },
        }
    }

    pub async fn get_recent_items(&self, limit: usize) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items?limit={}&sort=dateModified&direction=desc&itemType=-note",
            self.state.zotero_api_url, limit
        );
        let resp = self.state.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    pub async fn search_items(
        &self,
        query: &str,
        collection_key: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let base = match collection_key {
            Some(col) => format!(
                "{}/users/0/collections/{}/items",
                self.state.zotero_api_url, col
            ),
            None => format!("{}/users/0/items", self.state.zotero_api_url),
        };
        let encoded_q = urlencoding::encode(query);
        let url = format!("{base}?q={encoded_q}&limit={limit}&itemType=-note");

        let resp = self.state.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    pub async fn get_item(&self, item_key: &str) -> Result<ZoteroItem, ZoteroMcpError> {
        let url = format!("{}/users/0/items/{}", self.state.zotero_api_url, item_key);
        let resp = self.state.client.get(&url).send().await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ZoteroMcpError::NotFound(format!("Item {}", item_key)));
        }
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let item: ZoteroItem = resp.json().await?;
        Ok(item)
    }

    pub async fn get_collections(&self) -> Result<Vec<ZoteroCollection>, ZoteroMcpError> {
        let url = format!("{}/users/0/collections", self.state.zotero_api_url);
        let resp = self.state.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let collections: Vec<ZoteroCollection> = resp.json().await?;
        Ok(collections)
    }

    pub async fn get_collection_items(
        &self,
        collection_key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/collections/{}/items",
            self.state.zotero_api_url, collection_key
        );
        let resp = self.state.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    pub async fn get_item_children(
        &self,
        item_key: &str,
    ) -> Result<Vec<ZoteroItem>, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/children",
            self.state.zotero_api_url, item_key
        );
        let resp = self.state.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let items: Vec<ZoteroItem> = resp.json().await?;
        Ok(items)
    }

    pub async fn get_item_fulltext(&self, item_key: &str) -> Result<String, ZoteroMcpError> {
        let url = format!(
            "{}/users/0/items/{}/fulltext",
            self.state.zotero_api_url, item_key
        );
        let resp = self.state.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let val: serde_json::Value = resp.json().await?;
        let content = val
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(content)
    }

    pub async fn create_note(
        &self,
        parent_item_key: &str,
        note_content: &str,
    ) -> Result<ZoteroItem, ZoteroMcpError> {
        self.state.check_write_permission()?;
        let url = format!("{}/users/0/items", self.state.zotero_api_url);
        let payload = serde_json::json!([{
            "itemType": "note",
            "parentItem": parent_item_key,
            "note": note_content,
        }]);

        let resp = self.state.client.post(&url).json(&payload).send().await?;
        if !resp.status().is_success() {
            return Err(ZoteroMcpError::LocalApi {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let created: Vec<ZoteroItem> = resp.json().await?;
        created
            .into_iter()
            .next()
            .ok_or_else(|| ZoteroMcpError::LocalApi {
                status: 500,
                message: "Created note array was empty".to_string(),
            })
    }
}
