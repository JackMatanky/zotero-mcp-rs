//! Local semantic search: embedding generation, chunk storage, and
//! brute-force cosine similarity search over a side-car `SQLite` database this
//! server owns (distinct from Zotero's own `zotero.sqlite`, which
//! `zotero::sqlite` reads read-only). Consumes `zotero::ZoteroClient` to scan
//! the library; owns everything else independently.

mod chunking;
mod embedding;
mod index;
mod search;
mod store;

use std::{env, path::PathBuf};

pub(crate) use embedding::{Embedding, FastEmbedProvider};
pub(crate) use index::index_library;
pub(crate) use search::search_library;
#[cfg(test)]
pub(crate) use store::NewChunk;
pub(crate) use store::SemanticIndex;

use crate::errors::ZoteroMcpError;

/// Maximum characters of assembled text (title + abstract + fulltext) indexed
/// per item; longer text is truncated before chunking. ~400k chars is well
/// beyond any realistic paper; this exists only as a safety valve against
/// pathological inputs, not a tuned limit.
pub(crate) const MAX_INDEXABLE_CHARS: usize = 400_000;

/// Ceiling on characters per chunk (~1500 tokens at ~4 chars/token for
/// English academic text). See `chunking.rs` for the splitting algorithm.
pub(crate) const MAX_CHUNK_CHARS: usize = 6000;

/// Minimum cosine similarity for a search hit to be returned.
pub(crate) const DEFAULT_MIN_SIMILARITY: f32 = 0.3;

/// Trait boundary around embedding generation so `index_library`/
/// `search_library` are testable without ONNX inference or network access.
/// The only production implementor is [`FastEmbedProvider`] (`embedding.rs`);
/// tests use a deterministic fake.
pub(crate) trait EmbeddingProvider:
    Send + Sync + std::fmt::Debug
{
    /// Embeds a batch of texts, returning one vector per input in the same
    /// order. Vectors are NOT required to be normalized; callers normalize.
    ///
    /// # Errors
    ///
    /// Returns [`ZoteroMcpError::Embedding`] if inference fails.
    fn embed(&self, texts: &[String])
    -> Result<Vec<Embedding>, ZoteroMcpError>;
}

/// Resolves the `SQLite` index file path: `override_path` if given, else
/// `{default_semantic_data_dir()}/embeddings.sqlite`.
///
/// # Errors
///
/// - [`ZoteroMcpError::LocalDb`] if `override_path` is [`None`] and no platform
///   data directory can be determined (missing `HOME`/`APPDATA`, or an
///   unsupported OS)
pub(crate) fn resolve_db_path(
    override_path: Option<&std::path::Path>,
) -> Result<PathBuf, ZoteroMcpError> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }
    default_semantic_data_dir()
        .map(|dir| dir.join("embeddings.sqlite"))
        .ok_or_else(|| {
            ZoteroMcpError::LocalDb(
                "Could not determine a data directory for the semantic search \
                 index; set ZOTERO_SEMANTIC_DB_PATH"
                    .to_owned(),
            )
        })
}

/// Resolves the directory fastembed caches downloaded model files in: the
/// parent directory of the resolved index db path, joined with `models`.
pub(crate) fn resolve_model_cache_dir(db_path: &std::path::Path) -> PathBuf {
    db_path
        .parent()
        .map_or_else(|| PathBuf::from("models"), |parent| parent.join("models"))
}

/// Returns the per-user default app data directory for this server's own
/// files (distinct from Zotero's own data directory), or [`None`] if it
/// cannot be determined for the current OS/environment.
///
/// Mirrors [`crate::zotero::sqlite::profiles_dirs`]'s per-OS branching style.
fn default_semantic_data_dir() -> Option<PathBuf> {
    if let Some(appdata) = env::var_os("APPDATA") {
        return Some(PathBuf::from(appdata).join("zotero-mcp-rs"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        return Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("zotero-mcp-rs"),
        );
    }
    #[cfg(target_os = "linux")]
    if let Some(home) = env::var_os("HOME") {
        return Some(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("zotero-mcp-rs"),
        );
    }
    None
}
