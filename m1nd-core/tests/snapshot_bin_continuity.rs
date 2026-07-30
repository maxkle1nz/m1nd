//! Binary-snapshot continuity: the on-disk bytes that must survive a `bincode`
//! major bump.
//!
//! `snapshot_bin` is the compact binary persistence path `m1nd-mcp` writes for
//! every `persist {format:"bin"}` — a real graph snapshot on a real disk that
//! outlives the binary that wrote it. `bincode` is NOT self-describing, so a
//! change of integer width, endianness or length prefix does not fail loudly:
//! it decodes the same bytes into DIFFERENT values. Measured on this exact
//! layout, a non-legacy bincode configuration reads a real 88528-byte snapshot
//! as `version=4, nodes=0, edges=0` after consuming 3 bytes — the leading `u32`
//! version probe passes by accident and the graph comes back EMPTY, with no
//! error anywhere. A round-trip test cannot see that, because a stack that
//! encodes and decodes with the same (wrong) rules always agrees with itself.
//!
//! So the fixtures below are frozen BYTES, not a round trip. They pin what the
//! reader must recover from an already-written file, plus — where the format
//! promises it — the exact bytes the writer must still produce.
//!
//! ## Fixture provenance
//!
//! Every hex payload below was produced on `origin/main` @ 874702f6 by the
//! PRE-BUMP stack — `bincode 1.3.3` (fixed-width little-endian integers, `u64`
//! length prefixes) — via a throwaway generator driving this crate's own public
//! `snapshot_bin` / `EmbeddingCache` API, and is never re-derived from the code
//! under test. The generator is not committed: re-deriving these bytes from the
//! current stack would delete the only evidence they contain. The inputs are
//! synthetic and fully deterministic (neutral node ids, exact binary fractions
//! such as 0.75 / 0.125 / 1.375 so no value depends on float formatting); no
//! live snapshot and no live cache was read to build them.
//!
//! The embedding-cache half is gated on the optional `embed` feature, because
//! `m1nd_core::embed_cache` is. The CI gate covers it: `--workspace` unifies
//! features and `m1nd-mcp` enables `embed` by default. A crate-local run needs
//! the flag — `cargo test -p m1nd-core --features embed --test
//! snapshot_bin_continuity`.

use m1nd_core::graph::Graph;
use m1nd_core::snapshot_bin;
use m1nd_core::types::{EdgeDirection, EdgeIdx, NodeId, NodeType};

/// A V4 snapshot (`SNAPSHOT_VERSION == 4`) of a three-node graph that exercises
/// every wire shape the format uses: `Option` set and unset (provenance strings,
/// line numbers, the reverse-slot weights of a bidirectional edge), an empty and
/// a populated `Vec<String>` (tags), `f64` (`last_modified`), `f32` (weights,
/// causal strength), `bool` (`canonical`, `inhibitory`), `u8` (node type,
/// direction, including a `Custom` type above the built-in range) and `String`.
const GRAPH_V4_HEX: &str = "04000000030000000000000020000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e69740400000000000000696e69740203000000000000000400000000000000727573741300000000000000727573743a7669736962696c6974793a7075621200000000000000787261793a73746174653a626564726f636b00002060a699d9410000803e010a000000000000007372632f6c69622e727301030000000109000000011500000000000000666e20696e69742829202d3e207538207b2037207d0110000000000000007265706f5f616c7068613a3a636f72650124000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a74656172646f776e080000000000000074656172646f776e6b000000000000000000000000000000000000000000000000000016000000000000007265706f2d616c7068613a3a7372632f6d6f642e727306000000000000006d6f642e727300010000000000000004000000000000007275737400005060a699d9410000c03f010a000000000000007372632f6d6f642e727301010000000101000000000000020000000000000020000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e697424000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a74656172646f776e050000000000000063616c6c730000403f0000b03f000000000000003e20000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e697416000000000000007265706f2d616c7068613a3a7372632f6d6f642e72730b000000000000006465636c617265645f696e0000003f0000a03f010000603f010000c03e01010000803d";

/// A legacy V3 snapshot — the historical layout whose edges carry ONE weight and
/// no reverse-slot state. It exists on disk from before `SNAPSHOT_VERSION` 4 and
/// the loader must keep upgrading it. Written directly in the V3 shape (the
/// current writer can no longer emit it).
const GRAPH_V3_HEX: &str = "03000000020000000000000020000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e69740400000000000000696e697402020000000000000004000000000000007275737406000000000000006c656761637900002060a699d9410000803e010a000000000000007372632f6c69622e72730103000000010900000000000116000000000000007265706f2d616c7068613a3a7372632f6d6f642e727306000000000000006d6f642e7273000000000000000000000000000000000000000000000000000000020000000000000020000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e697416000000000000007265706f2d616c7068613a3a7372632f6d6f642e7273050000000000000063616c6c730000203f00000000003e16000000000000007265706f2d616c7068613a3a7372632f6d6f642e727320000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e6974070000000000000072656c617465640000003f01010000803d";

/// [`GRAPH_V3_HEX`] read back and re-written by the V4 writer. Pins the upgrade
/// itself: a legacy file loaded and persisted again must land on exactly these
/// bytes, so the V3 -> V4 promotion cannot drift silently either.
const GRAPH_V3_UPGRADED_HEX: &str = "04000000020000000000000020000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e69740400000000000000696e697402020000000000000004000000000000007275737406000000000000006c656761637900002060a699d9410000803e010a000000000000007372632f6c69622e72730103000000010900000000000116000000000000007265706f2d616c7068613a3a7372632f6d6f642e727306000000000000006d6f642e7273000000000000000000000000000000000000000000000000000000020000000000000020000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e697416000000000000007265706f2d616c7068613a3a7372632f6d6f642e7273050000000000000063616c6c730000203f0000203f000000000000003e20000000000000007265706f2d616c7068613a3a7372632f6c69622e72733a3a666e3a3a696e697416000000000000007265706f2d616c7068613a3a7372632f6d6f642e7273070000000000000072656c617465640000003f0000003f010000003f010000003f01010000803d";

const NODE_INIT: &str = "repo-alpha::src/lib.rs::fn::init";
const NODE_TEARDOWN: &str = "repo-alpha::src/lib.rs::fn::teardown";
const NODE_MOD: &str = "repo-alpha::src/mod.rs";

fn decode_hex(value: &str) -> Vec<u8> {
    assert!(value.len().is_multiple_of(2), "hex fixture must be even");
    (0..value.len() / 2)
        .map(|index| {
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("hex fixture digit")
        })
        .collect()
}

/// Materialise a frozen fixture on disk and load it through the real reader.
fn load_frozen(dir: &std::path::Path, name: &str, hex: &str) -> Graph {
    let path = dir.join(name);
    std::fs::write(&path, decode_hex(hex)).expect("write frozen fixture");
    snapshot_bin::load_graph(&path).expect("frozen fixture must load")
}

/// Bytes the current writer produces for `graph`.
fn re_encode(dir: &std::path::Path, name: &str, graph: &Graph) -> Vec<u8> {
    let path = dir.join(name);
    snapshot_bin::save_graph(graph, &path).expect("re-encode frozen fixture");
    std::fs::read(&path).expect("read re-encoded fixture")
}

/// The CSR slot for one directed (source, target, relation) triple. Relation is
/// part of the key because a bidirectional edge's reverse mirror can share a
/// node pair with an unrelated forward edge.
fn edge_slot(graph: &Graph, source: NodeId, target: NodeId, relation: &str) -> usize {
    graph
        .csr
        .out_range(source)
        .find(|&slot| {
            graph.csr.targets[slot] == target
                && graph.strings.resolve(graph.csr.relations[slot]) == relation
        })
        .unwrap_or_else(|| panic!("no CSR slot for {relation}"))
}

fn node(graph: &Graph, external_id: &str) -> NodeId {
    graph
        .resolve_id(external_id)
        .unwrap_or_else(|| panic!("frozen node {external_id} must survive"))
}

#[test]
fn frozen_v4_snapshot_decodes_to_exact_field_values() {
    let dir = tempfile::tempdir().expect("tempdir");
    let graph = load_frozen(dir.path(), "v4.bin", GRAPH_V4_HEX);

    assert_eq!(graph.num_nodes(), 3, "frozen V4 node count");
    assert_eq!(
        graph.num_edges(),
        3,
        "frozen V4 CSR slot count (one forward edge plus a bidirectional pair)"
    );

    let init = node(&graph, NODE_INIT);
    let teardown = node(&graph, NODE_TEARDOWN);
    let module = node(&graph, NODE_MOD);

    // --- nodes -------------------------------------------------------------
    assert_eq!(
        graph.strings.resolve(graph.nodes.label[init.as_usize()]),
        "init"
    );
    assert_eq!(
        graph.nodes.node_type[init.as_usize()],
        NodeType::Function,
        "built-in node type byte"
    );
    assert_eq!(
        graph.node_tags(init),
        vec!["rust", "rust:visibility:pub", "xray:state:bedrock"],
        "tag vector must survive in order"
    );
    assert_eq!(
        graph.nodes.last_modified[init.as_usize()],
        1_718_000_000.5,
        "f64 field"
    );
    assert_eq!(
        graph.nodes.change_frequency[init.as_usize()].get(),
        0.25,
        "f32 field"
    );

    assert_eq!(
        graph.nodes.node_type[teardown.as_usize()],
        NodeType::Custom(7),
        "custom node type byte (100 + v)"
    );
    assert!(
        graph.node_tags(teardown).is_empty(),
        "empty tag vector must stay empty"
    );
    assert_eq!(graph.nodes.last_modified[teardown.as_usize()], 0.0);
    assert_eq!(graph.nodes.change_frequency[teardown.as_usize()].get(), 0.0);

    assert_eq!(graph.nodes.node_type[module.as_usize()], NodeType::File);
    assert_eq!(graph.node_tags(module), vec!["rust"]);
    assert_eq!(
        graph.nodes.last_modified[module.as_usize()],
        1_718_000_001.25
    );
    assert_eq!(graph.nodes.change_frequency[module.as_usize()].get(), 1.5);

    // --- provenance: every Option set, every Option unset, and a mix --------
    let init_provenance = graph.resolve_node_provenance(init);
    assert_eq!(init_provenance.source_path.as_deref(), Some("src/lib.rs"));
    assert_eq!(init_provenance.line_start, Some(3));
    assert_eq!(init_provenance.line_end, Some(9));
    assert_eq!(
        init_provenance.excerpt.as_deref(),
        Some("fn init() -> u8 { 7 }")
    );
    assert_eq!(
        init_provenance.namespace.as_deref(),
        Some("repo_alpha::core")
    );
    assert!(init_provenance.canonical);

    let teardown_provenance = graph.resolve_node_provenance(teardown);
    assert_eq!(teardown_provenance.source_path, None);
    assert_eq!(teardown_provenance.line_start, None);
    assert_eq!(teardown_provenance.line_end, None);
    assert_eq!(teardown_provenance.excerpt, None);
    assert_eq!(teardown_provenance.namespace, None);
    assert!(!teardown_provenance.canonical);

    let module_provenance = graph.resolve_node_provenance(module);
    assert_eq!(module_provenance.source_path.as_deref(), Some("src/mod.rs"));
    assert_eq!(module_provenance.line_start, Some(1));
    assert_eq!(
        module_provenance.line_end,
        Some(1),
        "the graph normalises a missing line_end onto line_start BEFORE the snapshot \
         is written, so that is what the frozen bytes carry"
    );
    assert_eq!(
        module_provenance.excerpt, None,
        "an unset Option beside set ones"
    );
    assert_eq!(module_provenance.namespace, None);
    assert!(!module_provenance.canonical);

    // --- edges: forward slot, no reverse state -----------------------------
    let calls = edge_slot(&graph, init, teardown, "calls");
    assert_eq!(graph.csr.directions[calls], EdgeDirection::Forward);
    assert!(!graph.csr.inhibitory[calls]);
    assert_eq!(graph.csr.causal_strengths[calls].get(), 0.125);
    assert_eq!(
        graph.edge_plasticity.original_weight[calls].get(),
        0.75,
        "original weight must not be overwritten by the learned one"
    );
    assert_eq!(graph.edge_plasticity.current_weight[calls].get(), 1.375);
    assert_eq!(
        graph.csr.read_weight(EdgeIdx::new(calls as u32)).get(),
        1.375
    );

    // --- edges: bidirectional pair, both slots learned independently -------
    let forward = edge_slot(&graph, init, module, "declared_in");
    let reverse = edge_slot(&graph, module, init, "declared_in");
    assert_eq!(graph.csr.directions[forward], EdgeDirection::Bidirectional);
    assert!(
        graph.csr.inhibitory[forward],
        "inhibitory bool must survive"
    );
    assert_eq!(graph.csr.causal_strengths[forward].get(), 0.0625);
    assert_eq!(graph.edge_plasticity.original_weight[forward].get(), 0.5);
    assert_eq!(graph.edge_plasticity.current_weight[forward].get(), 1.25);
    assert_eq!(graph.edge_plasticity.original_weight[reverse].get(), 0.875);
    assert_eq!(graph.edge_plasticity.current_weight[reverse].get(), 0.375);
    assert_eq!(
        graph.csr.read_weight(EdgeIdx::new(reverse as u32)).get(),
        0.375
    );
}

#[test]
fn frozen_v4_snapshot_re_encodes_byte_identically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let graph = load_frozen(dir.path(), "v4.bin", GRAPH_V4_HEX);
    assert_eq!(
        re_encode(dir.path(), "v4-again.bin", &graph),
        decode_hex(GRAPH_V4_HEX),
        "the V4 writer must still emit the exact bytes it wrote before"
    );
}

#[test]
fn frozen_legacy_v3_snapshot_upgrades_to_exact_field_values() {
    let dir = tempfile::tempdir().expect("tempdir");
    let graph = load_frozen(dir.path(), "v3.bin", GRAPH_V3_HEX);

    assert_eq!(graph.num_nodes(), 2, "frozen V3 node count");
    assert_eq!(graph.num_edges(), 3, "frozen V3 CSR slot count");

    let init = node(&graph, NODE_INIT);
    let module = node(&graph, NODE_MOD);

    assert_eq!(graph.node_tags(init), vec!["rust", "legacy"]);
    assert_eq!(graph.nodes.last_modified[init.as_usize()], 1_718_000_000.5);
    assert_eq!(graph.nodes.change_frequency[init.as_usize()].get(), 0.25);
    let init_provenance = graph.resolve_node_provenance(init);
    assert_eq!(init_provenance.source_path.as_deref(), Some("src/lib.rs"));
    assert_eq!(init_provenance.line_start, Some(3));
    assert_eq!(init_provenance.line_end, Some(9));
    assert!(init_provenance.canonical);
    assert!(graph.node_tags(module).is_empty());

    // A V3 forward edge carries ONE weight: it becomes both original and current.
    let calls = edge_slot(&graph, init, module, "calls");
    assert_eq!(graph.csr.directions[calls], EdgeDirection::Forward);
    assert_eq!(graph.edge_plasticity.original_weight[calls].get(), 0.625);
    assert_eq!(graph.edge_plasticity.current_weight[calls].get(), 0.625);
    assert_eq!(graph.csr.causal_strengths[calls].get(), 0.125);

    // A V3 bidirectional edge mirrors its single weight onto the reverse slot.
    let related_forward = edge_slot(&graph, module, init, "related");
    let related_reverse = edge_slot(&graph, init, module, "related");
    assert_eq!(
        graph.csr.directions[related_forward],
        EdgeDirection::Bidirectional
    );
    assert!(graph.csr.inhibitory[related_forward]);
    assert_eq!(graph.csr.causal_strengths[related_forward].get(), 0.0625);
    for slot in [related_forward, related_reverse] {
        assert_eq!(graph.edge_plasticity.original_weight[slot].get(), 0.5);
        assert_eq!(graph.edge_plasticity.current_weight[slot].get(), 0.5);
    }
}

#[test]
fn frozen_legacy_v3_snapshot_re_encodes_to_the_frozen_v4_form() {
    let dir = tempfile::tempdir().expect("tempdir");
    let graph = load_frozen(dir.path(), "v3.bin", GRAPH_V3_HEX);
    assert_eq!(
        re_encode(dir.path(), "v3-upgraded.bin", &graph),
        decode_hex(GRAPH_V3_UPGRADED_HEX),
        "loading a legacy snapshot and persisting it must land on the frozen V4 bytes"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Warm embedding cache — same discipline, optional `embed` feature.
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "embed")]
mod embed_cache_continuity {
    use super::decode_hex;
    use m1nd_core::embed_cache::{EmbeddingCache, EMBED_CACHE_VERSION};

    const MODEL_ID: &str = "local/potion-base-8M";

    /// A single-entry cache: `HashMap` iteration order is not a promise, so this
    /// is the only cache fixture whose re-encoding can be byte-compared.
    const CACHE_ONE_HEX: &str = "0100000014000000000000006c6f63616c2f706f74696f6e2d626173652d384d04000000010000000000000011e2900e4088b97e04000000000000000000003f000080be0000003e00000000";
    const CACHE_ONE_KEY: u64 = 9_131_479_528_174_051_857;
    const CACHE_ONE_DIM: u32 = 4;

    /// A three-entry cache — decode only, asserted key by key.
    const CACHE_MANY_HEX: &str = "0100000014000000000000006c6f63616c2f706f74696f6e2d626173652d384d0300000003000000000000003aeb3f0855a7b9c303000000000000000000803e0000403f000000be9b080ad15236e61503000000000000000000803f0000000000000000f7fd126bc0570837030000000000000000000000000080bf0000003f";
    const CACHE_MANY_DIM: u32 = 3;
    const CACHE_MANY_ENTRIES: [(u64, [f32; 3]); 3] = [
        (1_578_008_448_762_251_419, [1.0, 0.0, 0.0]),
        (3_965_515_955_841_465_847, [0.0, -1.0, 0.5]),
        (14_103_487_691_739_884_346, [0.25, 0.75, -0.125]),
    ];

    fn frozen(dir: &std::path::Path, name: &str, hex: &str, dim: u32) -> EmbeddingCache {
        let path = dir.join(name);
        std::fs::write(&path, decode_hex(hex)).expect("write frozen cache");
        EmbeddingCache::load_compatible(&path, MODEL_ID, dim)
            .expect("frozen cache must load as compatible")
    }

    #[test]
    fn frozen_single_entry_cache_decodes_and_re_encodes_byte_identically() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = frozen(dir.path(), "one.bin", CACHE_ONE_HEX, CACHE_ONE_DIM);

        assert_eq!(cache.version, EMBED_CACHE_VERSION);
        assert_eq!(cache.model_id, MODEL_ID);
        assert_eq!(cache.dim, CACHE_ONE_DIM);
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(
            cache.entries.get(&CACHE_ONE_KEY).map(|v| &v[..]),
            Some(&[0.5f32, -0.25, 0.125, 0.0][..]),
            "persisted vector must come back element for element"
        );

        let out = dir.path().join("one-again.bin");
        cache.save(&out).expect("re-encode frozen cache");
        assert_eq!(
            std::fs::read(&out).expect("read re-encoded cache"),
            decode_hex(CACHE_ONE_HEX),
            "the cache writer must still emit the exact bytes it wrote before"
        );
    }

    #[test]
    fn frozen_multi_entry_cache_decodes_to_exact_vectors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = frozen(dir.path(), "many.bin", CACHE_MANY_HEX, CACHE_MANY_DIM);

        assert_eq!(cache.entries.len(), CACHE_MANY_ENTRIES.len());
        for (key, expected) in CACHE_MANY_ENTRIES {
            assert_eq!(
                cache.entries.get(&key).map(|v| &v[..]),
                Some(&expected[..]),
                "frozen cache entry {key} must decode unchanged"
            );
        }
    }

    #[test]
    fn frozen_cache_identity_guards_still_refuse_a_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("one.bin");
        std::fs::write(&path, decode_hex(CACHE_ONE_HEX)).expect("write frozen cache");

        assert!(
            EmbeddingCache::load_compatible(&path, "other/model", CACHE_ONE_DIM).is_none(),
            "a model mismatch must invalidate persisted vectors"
        );
        assert!(
            EmbeddingCache::load_compatible(&path, MODEL_ID, CACHE_ONE_DIM + 1).is_none(),
            "a dimension mismatch must invalidate persisted vectors"
        );
    }
}
