//! Local ONNX embedding generation via `fastembed`, plus the BLOB codec and
//! cosine similarity used by `store.rs` and `search.rs`.

use std::{path::Path, sync::Mutex};

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

use crate::{errors::ZoteroMcpError, semantic_search::EmbeddingProvider};

/// The single fixed embedding model this server uses. `BGESmallENV15` is
/// fastembed's own documented default (BAAI/bge-small-en-v1.5, 384
/// dimensions): no query/document instruction-prefix handling required,
/// small download (~130MB), strong retrieval quality for the model size.
/// Changing this requires deleting the existing index db (dimensions and
/// vector space are incompatible across models) — there is no per-model
/// partitioning in this schema (see `store.rs`).
const MODEL: EmbeddingModel = EmbeddingModel::BGESmallENV15;

/// Number of texts embedded per `fastembed` batch call.
const EMBED_BATCH_SIZE: usize = 32;

/// Production [`EmbeddingProvider`] backed by a local ONNX model via
/// `fastembed`. Model inference requires `&mut self` internally, so the
/// model is held behind a [`Mutex`]; callers already run `embed` inside
/// `tokio::task::spawn_blocking` (see `index.rs`/`search.rs`), so the lock is
/// never held across an `.await`.
pub(crate) struct FastEmbedProvider {
    model: Mutex<TextEmbedding>,
}

impl std::fmt::Debug for FastEmbedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastEmbedProvider").finish_non_exhaustive()
    }
}

impl FastEmbedProvider {
    /// Loads (downloading on first use into `cache_dir` if not already
    /// present) the fixed embedding model.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Embedding`] if model load/download fails
    pub(crate) fn load(cache_dir: &Path) -> Result<Self, ZoteroMcpError> {
        let options = TextInitOptions::new(MODEL)
            .with_cache_dir(cache_dir.to_path_buf())
            .with_show_download_progress(false);
        let model = TextEmbedding::try_new(options)
            .map_err(|e| ZoteroMcpError::Embedding(e.to_string()))?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }
}

impl EmbeddingProvider for FastEmbedProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ZoteroMcpError> {
        let mut model = self.model.lock().map_err(|_| {
            ZoteroMcpError::Embedding(
                "embedding model mutex poisoned".to_owned(),
            )
        })?;
        model
            .embed(texts, Some(EMBED_BATCH_SIZE))
            .map_err(|e| ZoteroMcpError::Embedding(e.to_string()))
    }
}

/// L2-normalizes `vector` in place. A zero vector is left unchanged.
pub(crate) fn normalize(vector: &mut [f32]) {
    let norm_sq: f32 = vector.iter().map(|x| x * x).sum();
    if norm_sq <= 0.0 {
        return;
    }
    let norm = norm_sq.sqrt();
    for x in vector.iter_mut() {
        *x /= norm;
    }
}

/// Dot product of two equal-length, pre-normalized vectors — equal to their
/// cosine similarity. Returns `0.0` if lengths differ (defensive: should
/// never happen since only one model/dimensionality is ever stored).
pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Encodes a vector as little-endian `f32` bytes for `BLOB` storage.
pub(crate) fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(vector.len().saturating_mul(4));
    for value in vector {
        buf.extend_from_slice(&value.to_le_bytes());
    }
    buf
}

/// Decodes little-endian `f32` bytes back into a vector.
///
/// # Errors
///
/// - [`ZoteroMcpError::Embedding`] if `bytes.len()` is not a multiple of 4
pub(crate) fn decode_embedding(
    bytes: &[u8],
) -> Result<Vec<f32>, ZoteroMcpError> {
    let mut chunks = bytes.chunks_exact(4);
    let values: Result<Vec<f32>, ZoteroMcpError> = (&mut chunks)
        .map(|chunk| {
            let array: [u8; 4] = chunk.try_into().map_err(|_| {
                ZoteroMcpError::Embedding(
                    "corrupt embedding blob: chunk is not 4 bytes".to_owned(),
                )
            })?;
            Ok(f32::from_le_bytes(array))
        })
        .collect();
    let values = values?;
    if !chunks.remainder().is_empty() {
        return Err(ZoteroMcpError::Embedding(
            "corrupt embedding blob: length is not a multiple of 4".to_owned(),
        ));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn encode_decode_round_trips_including_negative_and_zero() {
        let original: Vec<f32> = vec![0.0, -1.5, 3.25, -0.000_1, 42.0];
        let encoded = encode_embedding(&original);
        let decoded = decode_embedding(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn decode_rejects_non_multiple_of_four_length() {
        let bytes = vec![0_u8, 1, 2];
        assert!(decode_embedding(&bytes).is_err());
    }

    #[test]
    fn normalize_leaves_zero_vector_unchanged() {
        let mut vector = vec![0.0_f32, 0.0, 0.0];
        normalize(&mut vector);
        assert_eq!(vector, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn normalized_self_similarity_is_approximately_one() {
        let mut vector = vec![1.0_f32, 2.0, 3.0, -4.0];
        normalize(&mut vector);
        let similarity = cosine_similarity(&vector, &vector);
        assert!(
            (similarity - 1.0).abs() < 1e-6,
            "expected ~1.0, got {similarity}"
        );
    }

    #[test]
    fn cosine_similarity_mismatched_lengths_returns_zero() {
        let a = vec![1.0_f32, 2.0];
        let b = vec![1.0_f32, 2.0, 3.0];
        assert!(cosine_similarity(&a, &b).abs() < f32::EPSILON);
    }
}
