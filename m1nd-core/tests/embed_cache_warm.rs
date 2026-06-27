#![cfg(feature = "embed")]
//! Integration proof for the on-disk embedding cache (OPTIONAL `embed` feature).
//!
//! A warm `SemanticEngine::build_with_cache` must CONSULT the cache: a preseeded
//! sentinel vector (one the model would never emit) is returned verbatim for the
//! matching node, proving a real cache HIT — while a node absent from the cache
//! is freshly embedded (a MISS) and the build re-persists exactly the current
//! graph's entries (self-pruning).
//!
//! Skipped when the model blob is not vendored locally (e.g. CI without Git-LFS),
//! matching the `embed.rs` unit-test convention — never failed for a missing model.

use m1nd_core::builder::GraphBuilder;
use m1nd_core::embed::{Embedder, Model2VecEmbedder};
use m1nd_core::embed_cache::{content_key, EmbeddingCache};
use m1nd_core::semantic::SemanticEngine;
use m1nd_core::types::{NodeType, SemanticWeights};

fn model_available() -> bool {
    Model2VecEmbedder::default_model_dir()
        .join("model.safetensors")
        .exists()
}

#[test]
fn warm_build_reuses_preseeded_cache_vector() {
    if !model_available() {
        eprintln!("SKIP warm_build_reuses_preseeded_cache_vector: model not vendored");
        return;
    }

    // Identity of the live model — exactly what build_embeddings records.
    let emb = Model2VecEmbedder::from_default().expect("load model");
    let model_id = emb.model_id().to_string();
    let dim = emb.dim();

    // Tiny graph: a node we will preseed, and a node we will not.
    let mut b = GraphBuilder::new();
    let sentinel = b
        .add_node(
            "n_sentinel",
            "sentinel_probe_label",
            NodeType::Function,
            &[],
        )
        .expect("add sentinel node");
    let fresh = b
        .add_node(
            "n_fresh",
            "totally_different_label",
            NodeType::Function,
            &[],
        )
        .expect("add fresh node");
    let graph = b.finalize().expect("finalize graph");

    // Preseed the cache with a NON-normalized marker (all 7.0) the model could
    // never produce, keyed by the sentinel node's embed text (label, no excerpt).
    let cache_path = std::env::temp_dir().join(format!(
        "m1nd_embed_warm_{}.bin",
        content_key(&model_id, "sentinel_probe_label")
    ));
    let _ = std::fs::remove_file(&cache_path);
    let mut seed = EmbeddingCache::new(model_id.clone(), dim as u32);
    let marker: Box<[f32]> = vec![7.0f32; dim].into_boxed_slice();
    seed.entries.insert(
        content_key(&model_id, "sentinel_probe_label"),
        marker.clone(),
    );
    seed.save(&cache_path).expect("seed cache");

    let engine = SemanticEngine::build_with_cache(
        &graph,
        SemanticWeights::default(),
        Some(&cache_path),
        true, // persist = true (writable owner)
    )
    .expect("build with cache");

    // Cache HIT: the sentinel node is the preseeded marker verbatim.
    let got_sentinel = engine
        .embeddings
        .get(&sentinel)
        .expect("sentinel node embedded");
    assert_eq!(
        &got_sentinel[..],
        &marker[..],
        "warm build must return the preseeded cache vector (cache HIT)"
    );

    // Cache MISS: the fresh node is a real, L2-normalized embedding.
    let got_fresh = engine.embeddings.get(&fresh).expect("fresh node embedded");
    let norm: f32 = got_fresh.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 1e-3,
        "cache-missed node must be a real L2-normalized embedding, got norm {norm}"
    );

    // Self-pruning persist: the saved cache holds EXACTLY the current graph's two
    // node texts (both the reused sentinel and the newly embedded fresh node).
    let reload = EmbeddingCache::load_compatible(&cache_path, &model_id, dim as u32)
        .expect("saved cache still compatible");
    assert_eq!(
        reload.entries.len(),
        2,
        "cache must hold exactly the 2 nodes"
    );
    assert!(
        reload
            .entries
            .contains_key(&content_key(&model_id, "totally_different_label")),
        "freshly embedded node must be persisted into the cache"
    );

    let _ = std::fs::remove_file(&cache_path);
}

/// Read-only sessions (persist=false) must REUSE a warm cache but NEVER write
/// one — the major adversarial-review finding. A non-persisting build over a
/// fresh (nonexistent) cache path must leave no file behind.
#[test]
fn non_persisting_build_never_writes_cache() {
    if !model_available() {
        eprintln!("SKIP non_persisting_build_never_writes_cache: model not vendored");
        return;
    }

    let mut b = GraphBuilder::new();
    b.add_node("n_ro", "read_only_probe_label", NodeType::Function, &[])
        .expect("add node");
    let graph = b.finalize().expect("finalize graph");

    let cache_path = std::env::temp_dir().join("m1nd_embed_nopersist_probe.bin");
    let _ = std::fs::remove_file(&cache_path);

    let _engine = SemanticEngine::build_with_cache(
        &graph,
        SemanticWeights::default(),
        Some(&cache_path),
        false, // persist = false (read-only attacher)
    )
    .expect("build without persist");

    assert!(
        !cache_path.exists(),
        "persist=false (read-only) must not write the cache file"
    );
}
