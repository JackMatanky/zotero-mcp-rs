//! PDF path resolution helpers for MCP Zotero tools.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    ZoteroMcpServer,
    errors::ZoteroMcpError,
    zotero::{ItemType, ZoteroItem},
};

const ZOTERO_ATTACHMENTS_PREFIX: &str = "attachments:";
const BRIDGE_FILE_ROOTS_PATH: &str = "/file-roots";

#[derive(Debug, Deserialize)]
struct BridgeFileRootsResponse {
    #[serde(default)]
    roots: Vec<BridgeFileRoot>,
}

#[derive(Debug, Deserialize)]
struct BridgeFileRoot {
    kind: String,
    path: String,
}

pub(super) struct ResolvedPdfPath {
    pub(super) path: PathBuf,
    pub(super) requires_root_check: bool,
}

impl ZoteroMcpServer {
    pub(super) async fn fetch_bridge_pdf_roots(
        &self,
    ) -> Vec<(String, PathBuf)> {
        let url = format!(
            "{}{}",
            self.state.better_notes_url.trim_end_matches('/'),
            BRIDGE_FILE_ROOTS_PATH
        );
        let resp = match self
            .state
            .client
            .post(url)
            .json(&serde_json::json!({}))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp,
            Ok(_) | Err(_) => return Vec::new(),
        };
        let Ok(body) = self
            .state
            .read_limited_text(
                resp,
                self.state.security.max_http_body_bytes,
                "file roots response",
            )
            .await
        else {
            return Vec::new();
        };
        let Ok(parsed) = serde_json::from_str::<BridgeFileRootsResponse>(&body)
        else {
            return Vec::new();
        };
        parsed
            .roots
            .into_iter()
            .filter(|root| {
                matches!(
                    root.kind.as_str(),
                    "zotero-storage" | "zotero-linked-base" | "attanger-dest"
                ) && !root.path.is_empty()
            })
            .map(|root| (root.kind, PathBuf::from(root.path)))
            .collect()
    }

    pub(super) fn validate_pdf_read_path(
        &self,
        path: &Path,
        bridge_roots: &[(String, PathBuf)],
        direct_input: bool,
    ) -> Result<PathBuf, ZoteroMcpError> {
        let roots = merge_pdf_roots(
            &self.state.security.allowed_read_dirs,
            bridge_roots,
        );
        match self.state.check_existing_read_path(path, &roots, "PDF read") {
            Ok(checked) => {
                self.state.check_pdf_file(&checked)?;
                Ok(checked)
            }
            Err(_) if direct_input => {
                self.state.check_direct_file_paths_enabled()?;
                let checked = self.state.check_existing_read_path(
                    path,
                    &self.state.security.allowed_read_dirs,
                    "PDF read",
                )?;
                self.state.check_pdf_file(&checked)?;
                Ok(checked)
            }
            Err(e) => Err(e),
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "canonicalization is the security boundary for imported Zotero \
              PDFs"
)]
pub(super) fn canonicalize_existing_path(
    path: &Path,
) -> Result<PathBuf, ZoteroMcpError> {
    Ok(std::fs::canonicalize(path)?)
}

fn merge_pdf_roots(
    configured: &[PathBuf],
    bridge_roots: &[(String, PathBuf)],
) -> Vec<PathBuf> {
    configured
        .iter()
        .cloned()
        .chain(bridge_roots.iter().map(|(_, path)| path.clone()))
        .collect()
}

fn linked_base_roots(
    bridge_roots: &[(String, PathBuf)],
) -> impl Iterator<Item = &PathBuf> {
    bridge_roots
        .iter()
        .filter(|(kind, _)| kind == "zotero-linked-base")
        .map(|(_, path)| path)
}

fn file_url_to_path(href: &str) -> Option<PathBuf> {
    let url = url::Url::parse(href).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path().ok()
}

fn enclosure_file_path(item: &ZoteroItem) -> Option<PathBuf> {
    let href = item.links.get("enclosure")?.get("href")?.as_str()?;
    file_url_to_path(href)
}

fn resolve_linked_attachment_path(
    raw_path: &str,
    bridge_roots: &[(String, PathBuf)],
) -> Option<PathBuf> {
    let Some(relative) = raw_path.strip_prefix(ZOTERO_ATTACHMENTS_PREFIX)
    else {
        return Some(PathBuf::from(raw_path));
    };
    let relative = relative.trim_start_matches(['/', '\\']);
    linked_base_roots(bridge_roots).next().map(|root| root.join(relative))
}

pub(super) fn resolve_attachment_pdf_path(
    item: &ZoteroItem,
    bridge_roots: &[(String, PathBuf)],
) -> Option<ResolvedPdfPath> {
    if item.data.item_type != ItemType::Attachment {
        return None;
    }

    if item.data.link_mode.as_deref() == Some("imported_file") {
        if let Some(path) = enclosure_file_path(item) {
            return Some(ResolvedPdfPath {
                path,
                requires_root_check: false,
            });
        }
    }

    if item.data.link_mode.as_deref() == Some("linked_file") {
        if let Some(path) =
            item.data.path.as_deref().and_then(|path| {
                resolve_linked_attachment_path(path, bridge_roots)
            })
        {
            return Some(ResolvedPdfPath {
                path,
                requires_root_check: true,
            });
        }
    }

    if item.data.content_type.as_deref() == Some("application/pdf") {
        if let Some(path) =
            item.data.path.as_deref().and_then(|path| {
                resolve_linked_attachment_path(path, bridge_roots)
            })
        {
            return Some(ResolvedPdfPath {
                path,
                requires_root_check: true,
            });
        }
    }

    None
}

pub(super) fn find_pdf_path(
    children: &[ZoteroItem],
    bridge_roots: &[(String, PathBuf)],
) -> Option<ResolvedPdfPath> {
    children
        .iter()
        .find_map(|child| resolve_attachment_pdf_path(child, bridge_roots))
}
