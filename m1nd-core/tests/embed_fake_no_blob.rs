#![cfg(feature = "embed")]
//! FIX 4 (semantic-embeddings sheet §Proof gaps): exercise the embedding path
//! WITHOUT the ~30 MB vendored model blob, using an injected DETERMINISTIC
//! [`FakeEmbedder`]. Every embed-gated invariant here previously self-skipped in
//! CI when the model was absent (`return` on missing blob), so CI exercised ZERO
//! embedding code. These run unconditionally.
//!
//! Covered: build over the injected embedder, cache warm-REUSE (hits, no
//! recompute of the sentinel), self-PRUNING (persisted keys == current graph),
//! single-writer persist (persist=false never writes), and file corruption
//! ignored (garbage cache → clean recompute, build still succeeds).

use m1nd_core::builder::GraphBuilder;
use m1nd_core::embed::{Embedder, FakeEmbedder};
use m1nd_core::embed_cache::{content_key, EmbeddingCache};
use m1nd_core::graph::Graph;
use m1nd_core::semantic::SemanticEngine;
use m1nd_core::types::{NodeType, SemanticWeights};

const DIM: usize = 32;
/// Must match `SemanticEngine::with_injected_embedder`'s recorded id.
fn fake_model_id() -> String {
    format!("injected-fake#{DIM}")
}

fn two_node_graph() -> (Graph, &'static str, &'static str) {
    let mut b = GraphBuilder::new();
    b.add_node(
        "n_sentinel",
        "sentinel_probe_label",
        NodeType::Function,
        &[],
    )
    .expect("sentinel");
    b.add_node(
        "n_fresh",
        "totally_different_label",
        NodeType::Function,
        &[],
    )
    .expect("fresh");
    let graph = b.finalize().expect("finalize");
    (graph, "sentinel_probe_label", "totally_different_label")
}

// ── Deterministic fake: same text → same vector, different text → different ──

#[test]
fn fake_embedder_is_deterministic_and_normalized() {
    let f = FakeEmbedder::new(DIM);
    let a1 = f.embed("hello world");
    let a2 = f.embed("hello world");
    let b = f.embed("something else entirely");
    assert_eq!(a1, a2, "same text must map to the same vector");
    assert_ne!(a1, b, "different text must map to a different vector");
    assert_eq!(f.dim(), DIM);
    let norm: f32 = a1.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 1e-4,
        "vectors are L2-normalized, got {norm}"
    );
    // Self-cosine is 1; distinct texts are far from parallel.
    assert!((m1nd_core::embed::cosine(&a1, &a2) - 1.0).abs() < 1e-4);
    assert!(m1nd_core::embed::cosine(&a1, &b) < 0.99);
}

// ── Build populates the side-map + retains the injected embedder ──

#[test]
fn injected_build_populates_embeddings_without_blob() {
    let (graph, _, _) = two_node_graph();
    let engine = SemanticEngine::with_injected_embedder(
        &graph,
        SemanticWeights::default(),
        std::sync::Arc::new(FakeEmbedder::new(DIM)),
        None,
        false,
    )
    .expect("build with injected embedder");

    assert_eq!(engine.embeddings.len(), 2, "every node embedded");
    assert!(
        engine.embedder.is_some(),
        "injected embedder retained for query encode"
    );
    for v in engine.embeddings.values() {
        assert_eq!(v.len(), DIM);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
    }
}

// ── Cache warm-REUSE: a preseeded sentinel vector is returned verbatim ──

#[test]
fn injected_build_reuses_warm_cache_vector() {
    let (graph, sentinel_text, fresh_text) = two_node_graph();
    let model_id = fake_model_id();

    let cache_path = std::env::temp_dir().join(format!(
        "m1nd_fake_warm_{}_{}.bin",
        std::process::id(),
        content_key(&model_id, sentinel_text)
    ));
    let _ = std::fs::remove_file(&cache_path);

    // Preseed a marker the fake would never emit (all 7.0, not normalized).
    let mut seed = EmbeddingCache::new(model_id.clone(), DIM as u32);
    let marker: Box<[f32]> = vec![7.0f32; DIM].into_boxed_slice();
    seed.entries
        .insert(content_key(&model_id, sentinel_text), marker.clone());
    seed.save(&cache_path).expect("seed cache");

    let engine = SemanticEngine::with_injected_embedder(
        &graph,
        SemanticWeights::default(),
        std::sync::Arc::new(FakeEmbedder::new(DIM)),
        Some(&cache_path),
        true,
    )
    .expect("build");

    // HIT: sentinel is the preseeded marker verbatim (reused, not recomputed).
    let ids: Vec<_> = engine.embeddings.keys().copied().collect();
    let sentinel_vec = ids
        .iter()
        .map(|id| &engine.embeddings[id])
        .find(|v| v.as_ref() == marker.as_ref())
        .expect("sentinel vector reused from cache (HIT)");
    assert_eq!(sentinel_vec.as_ref(), marker.as_ref());

    // Self-pruning persist: exactly the current graph's two texts survive.
    let reload = EmbeddingCache::load_compatible(&cache_path, &model_id, DIM as u32)
        .expect("cache still compatible");
    assert_eq!(
        reload.entries.len(),
        2,
        "persisted cache holds exactly 2 nodes"
    );
    assert!(reload
        .entries
        .contains_key(&content_key(&model_id, fresh_text)));
    let _ = std::fs::remove_file(&cache_path);
}

// ── Self-PRUNING: a stale absent-node entry is dropped on rebuild ──

#[test]
fn injected_build_self_prunes_stale_cache_entries() {
    let (graph, sentinel_text, fresh_text) = two_node_graph();
    let model_id = fake_model_id();
    let cache_path =
        std::env::temp_dir().join(format!("m1nd_fake_prune_{}.bin", std::process::id()));
    let _ = std::fs::remove_file(&cache_path);

    // Seed a cache containing an entry for a node NOT in the current graph.
    let mut seed = EmbeddingCache::new(model_id.clone(), DIM as u32);
    let ghost: Box<[f32]> = {
        let mut v = vec![0.1f32; DIM];
        m1nd_core::embed::l2_normalize(&mut v);
        v.into_boxed_slice()
    };
    seed.entries
        .insert(content_key(&model_id, "GHOST_absent_node_text"), ghost);
    seed.save(&cache_path).expect("seed");

    let _ = SemanticEngine::with_injected_embedder(
        &graph,
        SemanticWeights::default(),
        std::sync::Arc::new(FakeEmbedder::new(DIM)),
        Some(&cache_path),
        true,
    )
    .expect("build");

    let reload =
        EmbeddingCache::load_compatible(&cache_path, &model_id, DIM as u32).expect("compatible");
    assert_eq!(
        reload.entries.len(),
        2,
        "stale ghost entry pruned; only current nodes remain"
    );
    assert!(!reload
        .entries
        .contains_key(&content_key(&model_id, "GHOST_absent_node_text")));
    assert!(reload
        .entries
        .contains_key(&content_key(&model_id, sentinel_text)));
    assert!(reload
        .entries
        .contains_key(&content_key(&model_id, fresh_text)));
    let _ = std::fs::remove_file(&cache_path);
}

// ── Single-writer: persist=false must NEVER write the cache file ──

#[test]
fn injected_build_read_only_never_writes_cache() {
    let (graph, _, _) = two_node_graph();
    let cache_path = std::env::temp_dir().join(format!("m1nd_fake_ro_{}.bin", std::process::id()));
    let _ = std::fs::remove_file(&cache_path);
    assert!(!cache_path.exists());

    let _ = SemanticEngine::with_injected_embedder(
        &graph,
        SemanticWeights::default(),
        std::sync::Arc::new(FakeEmbedder::new(DIM)),
        Some(&cache_path),
        false, // read-only attacher: MUST NOT write
    )
    .expect("build");

    assert!(
        !cache_path.exists(),
        "a read-only (persist=false) build must never create the cache file"
    );
}

// ── Corruption ignored: a garbage cache file → clean recompute, build ok ──

#[test]
fn injected_build_ignores_corrupt_cache_file() {
    let (graph, _, _) = two_node_graph();
    let cache_path =
        std::env::temp_dir().join(format!("m1nd_fake_corrupt_{}.bin", std::process::id()));
    // Write garbage bytes that are not a valid serialized cache.
    std::fs::write(&cache_path, b"\x00\x01\x02not a real cache\xff\xfe").expect("write garbage");

    let engine = SemanticEngine::with_injected_embedder(
        &graph,
        SemanticWeights::default(),
        std::sync::Arc::new(FakeEmbedder::new(DIM)),
        Some(&cache_path),
        true,
    )
    .expect("build must succeed despite a corrupt cache");

    // The corrupt cache is ignored → all nodes freshly embedded (real vectors).
    assert_eq!(engine.embeddings.len(), 2);
    for v in engine.embeddings.values() {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-3,
            "recomputed vectors are normalized"
        );
    }
    let _ = std::fs::remove_file(&cache_path);
}
