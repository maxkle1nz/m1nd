// === Phase C measurement harness — real item extents in ingest ===
//
// The graph's biggest write-verb gap is declaration-line-only provenance
// (line_start == line_end for functions). The Phase C EXPERIMENT teaches the
// Rust extractor to record the item's REAL end line; this #[ignore]d test is the
// before/after INSTRUMENT, not a gate: it ingests the real m1nd-core crate,
// reports ingest time, persisted snapshot size, and how many Function nodes
// carry a real extent (line_end > line_start). Run manually:
//
//   cargo test -p m1nd-mcp --test transplant_extent_experiment -- --ignored --nocapture

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::types::NodeType;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use std::path::Path;
use std::time::Instant;

#[test]
#[ignore = "Phase C measurement instrument — run manually, reports numbers only"]
fn measure_ingest_extents_on_m1nd_core() {
    let wt = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let core = wt.join("m1nd-core");

    let tmp = tempfile::tempdir().unwrap();
    let snapshot_path = tmp.path().join("graph_snapshot.json");
    let config = McpConfig {
        graph_source: snapshot_path.clone(),
        plasticity_state: tmp.path().join("plasticity_state.json"),
        ..McpConfig::default()
    };
    let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
        .expect("init session");
    state.ingest_roots = vec![core.to_string_lossy().to_string()];

    let t0 = Instant::now();
    m1nd_mcp::tools::handle_ingest(
        &mut state,
        m1nd_mcp::protocol::IngestInput {
            path: core.to_string_lossy().to_string(),
            agent_id: "extent-experiment".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            adapter: "code".to_string(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            project_root: None,
        },
    )
    .expect("ingest m1nd-core");
    let ingest_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // Extent census over Function nodes.
    let (n_fns, n_with_extent, max_span) = {
        let graph = state.graph.read();
        let n = graph.num_nodes() as usize;
        let mut fns = 0usize;
        let mut with_extent = 0usize;
        let mut max_span = 0u32;
        for i in 0..n {
            if graph.nodes.node_type[i] == NodeType::Function {
                fns += 1;
                let p = &graph.nodes.provenance[i];
                if p.line_end > p.line_start {
                    with_extent += 1;
                    max_span = max_span.max(p.line_end - p.line_start);
                }
            }
        }
        (fns, with_extent, max_span)
    };

    // Persist and weigh the snapshot.
    dispatch_tool(
        &mut state,
        "persist",
        &serde_json::json!({"agent_id": "extent-experiment", "action": "save"}),
    )
    .expect("persist save");
    let snapshot_bytes = std::fs::metadata(&snapshot_path)
        .map(|m| m.len())
        .unwrap_or(0);

    eprintln!("EXTENT-EXPERIMENT ingest_ms      = {ingest_ms:.0}");
    eprintln!("EXTENT-EXPERIMENT function_nodes = {n_fns}");
    eprintln!("EXTENT-EXPERIMENT with_extent    = {n_with_extent}");
    eprintln!("EXTENT-EXPERIMENT max_span_lines = {max_span}");
    eprintln!("EXTENT-EXPERIMENT snapshot_bytes = {snapshot_bytes}");
}
