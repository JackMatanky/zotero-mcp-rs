//! Settings management API wrapper (`<prefix>/settings`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{client::ZoteroClient, errors::ZoteroApiError};

/// Setting entry payload for client configuration settings.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SettingEntry {
    /// Setting key name.
    pub key: String,
    /// Setting value payload.
    pub value: serde_json::Value,
}

impl ZoteroClient<'_> {
    /// Fetches all settings for the target library.
    ///
    /// Issues `GET <prefix>/settings`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status.
    /// - [`Network`]: Transport errors.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    #[inline]
    #[expect(clippy::else_if_without_else, reason = "fallback list handling")]
    pub async fn get_settings(
        &self,
    ) -> Result<HashMap<String, SettingEntry>, ZoteroApiError> {
        let url = format!(
            "{}{}/settings",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let raw: serde_json::Value = self.get_json(&url).await?;
        let mut result = HashMap::new();
        if let Some(obj) = raw.as_object() {
            for (k, v) in obj {
                result.insert(k.clone(), SettingEntry {
                    key: k.clone(),
                    value: v.clone(),
                });
            }
        } else if let Ok(list) =
            serde_json::from_value::<Vec<SettingEntry>>(raw)
        {
            for item in list {
                result.insert(item.key.clone(), item);
            }
        }
        Ok(result)
    }

    /// Fetches a single setting entry by key.
    ///
    /// Issues `GET <prefix>/settings/<key>`.
    ///
    /// # Errors
    ///
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status.
    /// - [`Network`]: Transport errors.
    ///
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    #[inline]
    pub async fn get_setting(
        &self,
        key: &str,
    ) -> Result<SettingEntry, ZoteroApiError> {
        let url = format!(
            "{}{}/settings/{key}",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let val: serde_json::Value = self.get_json(&url).await?;
        if let Ok(entry) = serde_json::from_value::<SettingEntry>(val.clone()) {
            Ok(entry)
        } else {
            Ok(SettingEntry {
                key: key.to_owned(),
                value: val,
            })
        }
    }

    /// Updates a setting value by key via `PUT <prefix>/settings/<key>`.
    ///
    /// # Errors
    ///
    /// - [`PermissionDenied`]: If write operations are disabled.
    /// - [`LocalApi`]: If Zotero responds with a non-2xx status.
    /// - [`Network`]: Transport errors.
    ///
    /// [`PermissionDenied`]: ZoteroApiError::PermissionDenied
    /// [`LocalApi`]: ZoteroApiError::LocalApi
    /// [`Network`]: ZoteroApiError::Network
    #[inline]
    pub async fn update_setting(
        &self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), ZoteroApiError> {
        self.state.check_write_permission()?;
        let url = format!(
            "{}{}/settings/{key}",
            self.state.zotero_api_url(),
            self.target_prefix()
        );
        let req =
            self.apply_auth_headers(self.state.client().put(&url).json(&value));
        let resp = self.state.send_with_retry(req).await?;
        self.ensure_success(resp).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        client::test_http::{MockServer, http_response},
        state::AppState,
    };

    #[tokio::test]
    async fn parses_get_settings_map_response() {
        let json_resp = serde_json::json!({
            "export.quickCopy.setting": "as-bibtex",
            "sync.auto": true
        })
        .to_string();

        let server = MockServer::new(vec![http_response("200 OK", &json_resp)]);
        let state = AppState::test_default().with_zotero_api_url(server.url());
        let client = ZoteroClient::new(&state);

        let settings = client.get_settings().await.unwrap();
        assert_eq!(
            settings.get("export.quickCopy.setting").unwrap().value,
            serde_json::json!("as-bibtex")
        );
    }
}
