//! Standalone smoke test: proves the `ort-load-dynamic` + mise-provisioned
//! `ORT_DYLIB_PATH` build actually loads a real ONNX embedding model and
//! runs real inference (not just compiles). Downloads the BGE-small model on
//! first run (~130MB from Hugging Face); not part of the default test run.
//!
//! Run explicitly: `cargo test --test ort_smoke -- --ignored --nocapture`

#[test]
#[ignore = "downloads a real ONNX model from Hugging Face; run explicitly"]
fn real_fastembed_provider_embeds_text() {
    let cache_dir = std::env::temp_dir().join("zotero-mcp-rs-ort-smoke-models");
    let options = fastembed::TextInitOptions::new(
        fastembed::EmbeddingModel::BGESmallENV15,
    )
    .with_cache_dir(cache_dir)
    .with_show_download_progress(true);
    let mut model = fastembed::TextEmbedding::try_new(options)
        .expect("real ONNX model should load");
    let embeddings = model
        .embed(vec!["borrow checker ensures memory safety".to_owned()], None)
        .expect("real ONNX inference should succeed");
    assert_eq!(embeddings.len(), 1);
    let vector = embeddings.first().expect("one embedding");
    assert_eq!(vector.len(), 384);
    assert!(vector.iter().any(|v| *v != 0.0));
}
