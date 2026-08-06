//! Local semantic search: embedding generation, chunk storage, and
//! brute-force cosine similarity search over a side-car `SQLite` database this
//! server owns.
//!
//! This module operates independently of Zotero's primary `zotero.sqlite`
//! database, consuming [`ZoteroClient`](crate::zotero::ZoteroClient) to scan
//! the library for items and storing generated text chunks and vector
//! embeddings in a dedicated `SQLite` database.
//!
//! # Main Types
//!
//! - [`Embedding`] - L2-normalized `f32` vector representation.
//! - [`FastEmbedProvider`] - Local ONNX embedding provider via `fastembed`.
//! - [`SemanticIndex`] - Read/write semantic index database handle.
//! - [`EmbeddingProvider`] - Trait for embedding generator backends.
//!
//! # Submodules
//!
//! - [`chunking`] - Paragraph-aware text splitting algorithm.
//! - [`embedding`] - Vector representation, dot products, BLOB encoding, and
//!   ONNX models.
//! - [`index`] - Whole-library indexing and synchronization routines.
//! - [`search`] - Query embedding and cosine similarity ranking.
//! - [`store`] - `SQLite` persistence for chunks and embeddings.
//!
//! # Examples
//!
//! Resolving the semantic index database path and model cache directory:
//!
//! ```ignore
//! use std::path::Path;
//!
//! use zotero_api::semantic_search::{
//!     resolve_db_path, resolve_model_cache_dir,
//! };
//!
//! let db_path = resolve_db_path(None).unwrap();
//! let cache_dir = resolve_model_cache_dir(&db_path);
//! ```

mod chunking;
mod embedding;
mod index;
mod search;
mod store;

use std::{env, path::PathBuf};

pub use embedding::{Embedding, FastEmbedProvider};
pub use index::index_library;
pub use search::search_library;
#[cfg(any(test, feature = "test-util"))]
pub use store::NewChunk;
pub use store::SemanticIndex;

use crate::errors::ZoteroApiError;

/// Maximum characters of assembled text (title + abstract + fulltext) indexed
/// per item.
///
/// Longer text is truncated before chunking. A limit of 400,000 characters is
/// well beyond any realistic paper and exists as a safety valve against
/// pathological inputs.
pub(crate) const MAX_INDEXABLE_CHARS: usize = 400_000;

/// Ceiling on characters per chunk.
///
/// Roughly corresponds to 1,500 tokens at ~4 characters per token for English
/// academic text. See [`chunking`] for the paragraph and sentence splitting
/// logic.
pub(crate) const MAX_CHUNK_CHARS: usize = 6000;

/// Minimum cosine similarity threshold required for a search hit to be
/// returned.
pub const DEFAULT_MIN_SIMILARITY: f32 = 0.3;

/// Trait boundary around embedding generation.
///
/// Allows [`index_library`] and [`search_library`] to be tested without ONNX
/// inference or network access. The production implementor is
/// [`FastEmbedProvider`].
pub trait EmbeddingProvider: Send + Sync + std::fmt::Debug {
    /// Embeds a batch of texts, returning one vector per input in matching
    /// order.
    ///
    /// Vectors are not required to be normalized by the provider; callers
    /// normalize.
    ///
    /// # Errors
    ///
    /// Returns [`ZoteroApiError::Embedding`] if model inference fails.
    fn embed(&self, texts: &[String])
    -> Result<Vec<Embedding>, ZoteroApiError>;
}

/// Resolves the `SQLite` index file path.
///
/// Uses `override_path` if provided; otherwise defaults to
/// `embeddings.sqlite` inside the system default app data directory.
///
/// # Errors
///
/// Returns [`ZoteroApiError::LocalDb`] if `override_path` is [`None`] and no
/// platform data directory can be determined (for example, if `HOME` or
/// `APPDATA` is missing).
pub(crate) fn resolve_db_path(
    override_path: Option<&std::path::Path>,
) -> Result<PathBuf, ZoteroApiError> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }
    default_semantic_data_dir()
        .map(|dir| dir.join("embeddings.sqlite"))
        .ok_or_else(|| {
            ZoteroApiError::LocalDb(
                "Could not determine a data directory for the semantic search \
                 index; set ZOTERO_SEMANTIC_DB_PATH"
                    .to_owned(),
            )
        })
}

/// Resolves the directory where ONNX model files are cached.
///
/// Derives the model cache directory as a `models` sibling directory to
/// `db_path`.
pub(crate) fn resolve_model_cache_dir(db_path: &std::path::Path) -> PathBuf {
    db_path
        .parent()
        .map_or_else(|| PathBuf::from("models"), |parent| parent.join("models"))
}

/// Returns the per-user default app data directory for server files.
///
/// Distinct from Zotero's own data directory. Returns [`None`] if the path
/// cannot be determined for the current environment.
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
