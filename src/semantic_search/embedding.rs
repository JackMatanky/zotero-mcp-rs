//! The [`Embedding`] newtype (L2 normalization, dot-product scoring, and the
//! BLOB codec used by `store.rs`/`search.rs`) plus local ONNX embedding
//! generation via `fastembed`.

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

impl std::fmt::Debug for FastEmbedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastEmbedProvider").finish_non_exhaustive()
    }
}

impl EmbeddingProvider for FastEmbedProvider {
    fn embed(
        &self,
        texts: &[String],
    ) -> Result<Vec<Embedding>, ZoteroMcpError> {
        let mut model = self.model.lock().map_err(|_| {
            ZoteroMcpError::Embedding(
                "embedding model mutex poisoned".to_owned(),
            )
        })?;
        let vectors: Vec<Vec<f32>> = model
            .embed(texts, Some(EMBED_BATCH_SIZE))
            .map_err(|e| ZoteroMcpError::Embedding(e.to_string()))?;
        Ok(vectors.into_iter().map(Embedding::from).collect())
    }
}

/// A dense embedding vector produced by the model and stored in the index.
///
/// Newtype over `Vec<f32>` so dimensionality and normalization are handled
/// at typed boundaries (BLOB decode, dot products) rather than as free
/// `Vec<f32>` bookkeeping.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Embedding(Vec<f32>);

impl Embedding {
    /// L2-normalizes in place. A zero vector is left unchanged.
    pub(crate) fn normalize(&mut self) {
        let norm_sq: f32 = self.0.iter().map(|x| x * x).sum();
        if norm_sq <= 0.0 {
            return;
        }
        let norm = norm_sq.sqrt();
        for x in &mut self.0 {
            *x /= norm;
        }
    }

    /// Dot product of two equal-length, pre-normalized vectors — equal to
    /// their cosine similarity. Returns `0.0` if lengths differ (defensive:
    /// should never happen since only one model/dimensionality is stored).
    pub(crate) fn dot(&self, other: &Embedding) -> f32 {
        if self.0.len() != other.0.len() {
            return 0.0;
        }
        self.0.iter().zip(&other.0).map(|(x, y)| x * y).sum()
    }

    /// Encodes the vector as little-endian `f32` bytes for `BLOB` storage.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.0.len().saturating_mul(4));
        for value in &self.0 {
            buf.extend_from_slice(&value.to_le_bytes());
        }
        buf
    }
}

impl From<Vec<f32>> for Embedding {
    fn from(values: Vec<f32>) -> Self {
        Self(values)
    }
}

impl TryFrom<&[u8]> for Embedding {
    type Error = ZoteroMcpError;

    /// Decodes little-endian `f32` bytes back into an embedding.
    ///
    /// # Errors
    ///
    /// - [`ZoteroMcpError::Embedding`] if `bytes.len()` is not a multiple of 4
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let (chunks, remainder) = bytes.as_chunks::<4>();
        if !remainder.is_empty() {
            return Err(ZoteroMcpError::Embedding(
                "corrupt embedding blob: length is not a multiple of 4"
                    .to_owned(),
            ));
        }
        let values = chunks.iter().map(|c| f32::from_le_bytes(*c)).collect();
        Ok(Self(values))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn encode_decode_round_trips_including_negative_and_zero() {
        let original = Embedding::from(vec![0.0, -1.5, 3.25, -0.000_1, 42.0]);
        let encoded = original.encode();
        let decoded = Embedding::try_from(encoded.as_slice()).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn decode_rejects_non_multiple_of_four_length() {
        let bytes = vec![0_u8, 1, 2];
        assert!(Embedding::try_from(bytes.as_slice()).is_err());
    }

    #[test]
    fn normalize_leaves_zero_vector_unchanged() {
        let mut vector = Embedding::from(vec![0.0_f32, 0.0, 0.0]);
        vector.normalize();
        assert_eq!(vector, Embedding::from(vec![0.0, 0.0, 0.0]));
    }

    #[test]
    fn normalized_self_similarity_is_approximately_one() {
        let mut vector = Embedding::from(vec![1.0_f32, 2.0, 3.0, -4.0]);
        vector.normalize();
        let similarity = vector.dot(&vector);
        assert!(
            (similarity - 1.0).abs() < 1e-6,
            "expected ~1.0, got {similarity}"
        );
    }

    #[test]
    fn dot_mismatched_lengths_returns_zero() {
        let a = Embedding::from(vec![1.0_f32, 2.0]);
        let b = Embedding::from(vec![1.0_f32, 2.0, 3.0]);
        assert!(a.dot(&b).abs() < f32::EPSILON);
    }
}
