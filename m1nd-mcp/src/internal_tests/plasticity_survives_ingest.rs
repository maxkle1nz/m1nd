//! The Hebbian layer must survive an ingest.
//!
//! Measured on a production owner before this battery was written: a
//! `plasticity_state.json` holding 73,332 synaptic rows, of which **zero** had
//! `strengthen_count > 0`, **zero** had `weaken_count > 0`, **zero** had
//! `last_used_query > 0`, and zero carried an LTP/LTD flag. The signature
//! feature had never accumulated anything in the field.
//!
//! It is not dead code. `activate` reaches `QueryOrchestrator::query` step 8 and
//! `PlasticityEngine::update` writes the counters with no early return. What
//! erased them is the ingest: `finalize_ingest_with_inventory` replaces the live
//! graph wholesale, the replacement's `edge_plasticity` arrays are born zeroed
//! (`Graph::add_edge`), nothing on that path re-imported the learned state, and
//! the `state.persist()` at the end of the same function wrote the zeros over
//! the sidecar. A served owner re-ingests by traffic, so on any repo that moves,
//! the counters were erased faster than they could grow.
//!
//! The mechanism to survive already existed and was already documented:
//! `import_state` matches edges by `(source_label, target_label, relation)` —
//! labels, never numeric indices — precisely so learned weights outlive a
//! re-ingest that renumbers every node. The ingest simply never called it.
//!
//! Every case below drives the REAL door: `refresh` is the one ingest mode a
//! client may run (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §1), it takes the
//! same `finalize_ingest` commit path as `replace`, and it is reached here
//! through the same policy gate both transports run — so nothing here proves a
//! handler while leaving the door untested.

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::plasticity::SynapticState;
use m1nd_mcp::server::{dispatch_tool, enforce_generic_action_policy, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

const AGENT: &str = "plasticity-carry-probe";

/// A brain whose runtime lives under `runtime`, with no roots declared yet.
fn build_state(runtime: &Path) -> SessionState {
    std::fs::create_dir_all(runtime).expect("runtime dir");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

/// A small, deterministic Rust crate with enough modules that dropping one is
/// still well clear of the refresh shrink floor.
fn write_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"plastfixture\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod helper;\npub mod extra;\npub mod spare;\n\
         pub fn top() -> i64 { helper::help() + extra::extra() + spare::spare() }\n\
         pub struct Top { pub v: i64 }\npub fn second() -> i64 { helper::help() }\n",
    )
    .expect("lib.rs");
    std::fs::write(
        root.join("src/helper.rs"),
        "pub fn help() -> i64 { 41 }\npub struct Helper { pub v: i64 }\n\
         pub fn helper_two() -> i64 { help() + 7 }\npub fn helper_three() -> i64 { help() + 8 }\n",
    )
    .expect("helper.rs");
    std::fs::write(
        root.join("src/extra.rs"),
        "pub fn extra() -> i64 { 5 }\npub struct Extra { pub v: i64 }\n\
         pub fn extra_two() -> i64 { extra() + 1 }\n",
    )
    .expect("extra.rs");
    std::fs::write(
        root.join("src/spare.rs"),
        "pub fn spare() -> i64 { 3 }\npub struct Spare { pub v: i64 }\n\
         pub fn spare_two() -> i64 { spare() + 1 }\n",
    )
    .expect("spare.rs");
}

/// Seed the brain the way the field seeds one: the trusted library ingest that
/// production `ingest` itself uses, then declare the root.
fn seed_declared_root(state: &mut SessionState, repo: &Path) {
    let (graph, _) = m1nd_ingest::Ingestor::new(m1nd_ingest::IngestConfig {
        root: repo.to_path_buf(),
        parallelism: 1,
        ..m1nd_ingest::IngestConfig::default()
    })
    .ingest()
    .expect("trusted fixture ingest");
    {
        let mut live = state.graph.write();
        *live = graph;
        if !live.finalized {
            live.finalize().expect("finalize seeded graph");
        }
    }
    state.rebuild_engines().expect("rebuild engines");
    declare_root(state, repo);
}

/// Declare `repo` as this brain's one root, without touching the graph.
fn declare_root(state: &mut SessionState, repo: &Path) {
    let declared = canonical(repo);
    state.ingest_roots = vec![declared.clone()];
    state.workspace_root = Some(declared);
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// The exact sequence both transports run: the pure policy gate first, then
/// dispatch. Returns the gate's refusal as `Err`, the handler's payload as `Ok`.
fn admit_then_dispatch(
    state: &mut SessionState,
    tool: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    enforce_generic_action_policy(tool, params).map_err(|error| error.to_string())?;
    dispatch_tool(state, tool, params).map_err(|error| error.to_string())
}

/// Re-ingest the declared root through the one door a client may open.
fn refresh(state: &mut SessionState, repo: &Path) -> serde_json::Value {
    let root = canonical(repo);
    state.caller_root = Some(root.clone());
    admit_then_dispatch(
        state,
        "ingest",
        &json!({ "path": root, "agent_id": AGENT, "mode": "refresh" }),
    )
    .expect("an exact-root refresh must be admitted and must not fail")
}

/// Run the ONE retrieval verb that reaches plasticity step 8.
fn activate(state: &mut SessionState, query: &str) {
    admit_then_dispatch(
        state,
        "activate",
        &json!({ "query": query, "agent_id": AGENT, "top_k": 25 }),
    )
    .expect("activate must succeed");
}

/// The live graph's learned state, keyed by the documented edge identity.
fn learned_by_identity(state: &SessionState) -> HashMap<(String, String, String), SynapticState> {
    let graph = state.graph.read();
    state
        .plasticity
        .export_state(&graph)
        .expect("export the live synaptic state")
        .into_iter()
        .map(|row| {
            (
                (
                    row.source_label.clone(),
                    row.target_label.clone(),
                    row.relation.clone(),
                ),
                row,
            )
        })
        .collect()
}

/// The identities the fixture actually strengthened. A case that finds this
/// empty is not measuring anything, so every case asserts on it first.
fn strengthened(state: &SessionState) -> Vec<((String, String, String), u16)> {
    learned_by_identity(state)
        .into_iter()
        .filter(|(_, row)| row.strengthen_count > 0)
        .map(|(identity, row)| (identity, row.strengthen_count))
        .collect()
}

/// Warm the graph the way traffic warms it, and return what it learned.
fn warm(state: &mut SessionState) -> Vec<((String, String, String), u16)> {
    for query in ["helper help", "top second", "extra spare", "helper two"] {
        activate(state, query);
    }
    let learned = strengthened(state);
    assert!(
        !learned.is_empty(),
        "precondition: the fixture must strengthen at least one edge, or this \
         battery proves nothing"
    );
    learned
}

// ---------------------------------------------------------------------------
// The defect itself
// ---------------------------------------------------------------------------

/// The core case. Strengthen, persist, re-ingest — and read the counter back
/// off the edge that owns the same identity triple. RED before the fix: the
/// replacement graph is born zeroed and nothing re-applies the sidecar.
#[test]
fn a_strengthened_edge_survives_a_re_ingest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo);
    seed_declared_root(&mut state, &repo);

    let learned = warm(&mut state);
    state.persist().expect("persist the warmed brain");

    let payload = refresh(&mut state, &repo);
    assert_eq!(payload["ok"], json!(true), "payload was {payload}");

    let after = learned_by_identity(&state);
    for (identity, count) in &learned {
        let row = after.get(identity).unwrap_or_else(|| {
            panic!("the re-ingest dropped the edge {identity:?} the fixture strengthened")
        });
        assert_eq!(
            row.strengthen_count, *count,
            "the re-ingest erased the learning on {identity:?}"
        );
    }
}

/// The freshness half. The sidecar on disk is one persist BEHIND the running
/// session, and an ingest that restored only the file would trade one silent
/// erasure for another. Nothing is persisted after the warm-up here.
#[test]
fn strengthening_newer_than_the_sidecar_survives_a_re_ingest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo);
    seed_declared_root(&mut state, &repo);
    // The stale file: written before a single query ran.
    state.persist().expect("persist the cold brain");

    let learned = warm(&mut state);

    let payload = refresh(&mut state, &repo);
    assert_eq!(payload["ok"], json!(true), "payload was {payload}");

    let after = learned_by_identity(&state);
    for (identity, count) in &learned {
        assert_eq!(
            after.get(identity).map(|row| row.strengthen_count),
            Some(*count),
            "the in-flight session's strengthening on {identity:?} was lost to the stale sidecar"
        );
    }
}

/// The other half of "freshest truth": a session whose live graph knows nothing
/// (a boot that found no graph snapshot, so the friendly boot import never ran)
/// must still take the learning back off the sidecar when the ingest lands.
#[test]
fn the_sidecar_restores_when_the_live_graph_carries_nothing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let repo = temp.path().join("repo");
    write_repo(&repo);

    let learned = {
        let mut warm_state = build_state(&runtime);
        seed_declared_root(&mut warm_state, &repo);
        let learned = warm(&mut warm_state);
        warm_state.persist().expect("persist the warmed brain");
        learned
    };

    // A second session over the same runtime, with an EMPTY graph in memory.
    let mut cold = build_state(&runtime);
    assert_eq!(
        cold.graph.read().num_nodes(),
        0,
        "precondition: this session must carry nothing of its own"
    );
    declare_root(&mut cold, &repo);

    let payload = refresh(&mut cold, &repo);
    assert_eq!(payload["ok"], json!(true), "payload was {payload}");

    let after = learned_by_identity(&cold);
    for (identity, count) in &learned {
        assert_eq!(
            after.get(identity).map(|row| row.strengthen_count),
            Some(*count),
            "the persisted learning on {identity:?} was not restored onto the ingested graph"
        );
    }
}

// ---------------------------------------------------------------------------
// What the carry-forward must NOT do
// ---------------------------------------------------------------------------

/// Restoring learned state must never resurrect topology. An edge whose source
/// file is gone after the re-ingest stays gone — the restore lands on edges the
/// new graph already owns, and creates none.
#[test]
fn an_edge_that_no_longer_exists_does_not_resurrect() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo);
    seed_declared_root(&mut state, &repo);
    warm(&mut state);
    state.persist().expect("persist the warmed brain");

    let doomed: Vec<(String, String, String)> = learned_by_identity(&state)
        .into_keys()
        .filter(|(source, target, _)| source.contains("spare.rs") || target.contains("spare.rs"))
        .collect();
    assert!(
        !doomed.is_empty(),
        "precondition: the fixture must own edges on the module about to be deleted"
    );

    std::fs::remove_file(repo.join("src/spare.rs")).expect("delete a module");
    std::fs::write(
        repo.join("src/lib.rs"),
        "pub mod helper;\npub mod extra;\n\
         pub fn top() -> i64 { helper::help() + extra::extra() }\n\
         pub struct Top { pub v: i64 }\npub fn second() -> i64 { helper::help() }\n",
    )
    .expect("lib.rs without the deleted module");

    let payload = refresh(&mut state, &repo);
    assert_eq!(payload["ok"], json!(true), "payload was {payload}");

    let after = learned_by_identity(&state);
    for identity in &doomed {
        assert!(
            !after.contains_key(identity),
            "the restore resurrected {identity:?}, an edge the re-ingest deleted"
        );
    }
}

/// Fail-open, the repo's standing posture for a sidecar: a corrupt
/// `plasticity_state.json` degrades to "counters start over", never to a failed
/// ingest. The session's own live learning still crosses the replacement.
#[test]
fn a_corrupt_sidecar_degrades_without_failing_the_ingest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let mut state = build_state(&runtime);
    let repo = temp.path().join("repo");
    write_repo(&repo);
    seed_declared_root(&mut state, &repo);
    let learned = warm(&mut state);
    state.persist().expect("persist the warmed brain");

    std::fs::write(
        runtime.join("plasticity_state.json"),
        b"{ this is not a synaptic sidecar",
    )
    .expect("corrupt the sidecar");

    let payload = refresh(&mut state, &repo);
    assert_eq!(
        payload["ok"],
        json!(true),
        "a corrupt sidecar must not fail the ingest; payload was {payload}"
    );
    assert!(
        state.graph.read().num_nodes() > 0,
        "the graph must still be replaced by the re-ingest"
    );

    let after = learned_by_identity(&state);
    for (identity, count) in &learned {
        assert_eq!(
            after.get(identity).map(|row| row.strengthen_count),
            Some(*count),
            "the live session's learning on {identity:?} must survive a corrupt file"
        );
    }
}

/// A brain that has never persisted has no sidecar at all. That is a cold
/// start, not a fault.
#[test]
fn a_missing_sidecar_does_not_fail_the_ingest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let mut state = build_state(&runtime);
    let repo = temp.path().join("repo");
    write_repo(&repo);
    seed_declared_root(&mut state, &repo);
    assert!(
        !runtime.join("plasticity_state.json").exists(),
        "precondition: no sidecar has been written yet"
    );

    let payload = refresh(&mut state, &repo);
    assert_eq!(payload["ok"], json!(true), "payload was {payload}");
    assert!(state.graph.read().num_nodes() > 0);
}

/// The import's weight firewall is not softened by the carry-forward. A
/// non-finite weight in the file is refused at the load boundary (FM-PL-001 /
/// FM-PL-007), the ingest still lands, and no non-finite weight reaches the
/// graph.
#[test]
fn a_non_finite_weight_in_the_sidecar_is_refused_not_carried() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let mut state = build_state(&runtime);
    let repo = temp.path().join("repo");
    write_repo(&repo);
    seed_declared_root(&mut state, &repo);
    warm(&mut state);
    state.persist().expect("persist the warmed brain");

    let path = runtime.join("plasticity_state.json");
    let poisoned = std::fs::read_to_string(&path)
        .expect("read the sidecar")
        .replacen("\"current_weight\": 1.0", "\"current_weight\": 1e999", 1);
    std::fs::write(&path, poisoned).expect("poison the sidecar");

    let payload = refresh(&mut state, &repo);
    assert_eq!(
        payload["ok"],
        json!(true),
        "a poisoned sidecar must not fail the ingest; payload was {payload}"
    );
    for row in learned_by_identity(&state).values() {
        assert!(
            row.current_weight.is_finite() && row.original_weight.is_finite(),
            "a non-finite weight reached the graph on {}->{}",
            row.source_label,
            row.target_label
        );
    }
}
