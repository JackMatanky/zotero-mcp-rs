use reqwest::Client;
use std::env;

#[derive(Clone, Debug)]
pub struct AppState {
    pub client: Client,
    pub zotero_api_url: String,
    pub better_bibtex_url: String,
    pub better_notes_url: String,
    // ponytail: write gate defaults to read-only; enabled via ZOTERO_WRITE_ENABLED
    pub write_enabled: bool,
}

impl AppState {
    pub fn from_env() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| Client::new());

        let zotero_api_url =
            env::var("ZOTERO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:23119/api".to_string());

        let better_bibtex_url = env::var("BETTER_BIBTEX_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:23119/better-bibtex/json-rpc".to_string());

        let better_notes_url = env::var("BETTER_NOTES_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:23119/better-notes".to_string());

        let write_enabled = env::var("ZOTERO_WRITE_ENABLED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Self {
            client,
            zotero_api_url,
            better_bibtex_url,
            better_notes_url,
            write_enabled,
        }
    }

    pub fn check_write_permission(&self) -> Result<(), crate::errors::ZoteroMcpError> {
        if !self.write_enabled {
            Err(crate::errors::ZoteroMcpError::PermissionDenied(
                "Write operation rejected: set ZOTERO_WRITE_ENABLED=1 to enable modifying Zotero library".to_string()
            ))
        } else {
            Ok(())
        }
    }
}
