//! Regression: community density must not overflow for very large communities.
//!
//! `CommunityDetector::community_stats` computes `max_internal = nc * (nc - 1)`
//! where `nc: u32` is the community node count. When m1nd ingests a large repo
//! and Louvain collapses >= 65_537 nodes into one community, `nc * (nc - 1)`
//! exceeds `u32::MAX` and overflows (debug: panic; release: a wrong, wrapped
//! density). This builds exactly that community and asserts a finite, in-range
//! density instead.
//!
//! (Surfaced by the X-RAY adversarial bug hunt.)

use m1nd_core::graph::Graph;
use m1nd_core::topology::{CommunityDetector, CommunityResult};
use m1nd_core::types::{CommunityId, FiniteF32, NodeType};

#[test]
fn community_density_does_not_overflow_for_huge_community() {
    // 65_537 is the smallest count where nc * (nc - 1) exceeds u32::MAX:
    // 65_537 * 65_536 = 4_295_032_832 > 4_294_967_295.
    let n: u32 = 65_537;

    let mut g = Graph::new();
    for i in 0..n {
        g.add_node(&format!("n{i}"), "n", NodeType::Function, &[], 0.0, 0.0)
            .expect("add_node");
    }
    g.finalize().expect("finalize");

    // Every node in a single community — what Louvain yields when a dense graph
    // fully collapses. No edges are needed: the overflow is in the max-internal
    // term, which is computed before any density division.
    let result = CommunityResult {
        assignments: vec![CommunityId(0); n as usize],
        num_communities: 1,
        modularity: FiniteF32::new(0.0),
        passes: 1,
    };

    let stats = CommunityDetector::community_stats(&g, &result);
    assert_eq!(stats.len(), 1, "exactly one community expected");

    let density = stats[0].density.get();
    assert!(density.is_finite(), "density must be finite, got {density}");
    assert!(
        (0.0..=1.0).contains(&density),
        "density must stay in [0, 1], got {density}"
    );
    assert_eq!(
        stats[0].node_count, n,
        "node_count must match the community"
    );
}
