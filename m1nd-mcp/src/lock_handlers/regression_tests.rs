use super::*;
use crate::server::McpConfig;
use m1nd_core::{
    domain::DomainConfig,
    graph::Graph,
    types::{EdgeDirection, FiniteF32, NodeType},
};
use serde_json::{json, Value};

fn graph(extra: bool) -> Graph {
    let mut g = Graph::new();
    let a = g
        .add_node("id::a", "alpha", NodeType::Function, &[], 0.0, 0.0)
        .unwrap();
    let b = g
        .add_node("id::b", "beta", NodeType::Function, &[], 0.0, 0.0)
        .unwrap();
    let c = g
        .add_node("id::c", "gamma", NodeType::Function, &[], 0.0, 0.0)
        .unwrap();
    g.add_edge(
        a,
        b,
        "calls",
        FiniteF32::new(0.5),
        EdgeDirection::Forward,
        false,
        FiniteF32::ZERO,
    )
    .unwrap();
    if extra {
        g.add_edge(
            c,
            a,
            "calls",
            FiniteF32::new(0.7),
            EdgeDirection::Forward,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    }
    g.finalize().unwrap();
    g
}
fn fixture() -> (tempfile::TempDir, SessionState) {
    let dir = tempfile::tempdir().unwrap();
    let config = McpConfig {
        graph_source: dir.path().join("graph.json"),
        plasticity_state: dir.path().join("plasticity.json"),
        runtime_dir: Some(dir.path().to_owned()),
        ..Default::default()
    };
    let state = SessionState::initialize(graph(false), &config, DomainConfig::code()).unwrap();
    (dir, state)
}
fn create(state: &mut SessionState, agent: &str, scope: &str, roots: &[&str]) -> M1ndResult<Value> {
    handle_lock_create(state, serde_json::from_value(json!({"agent_id":agent,"scope":scope,"root_nodes":roots,"radius":if scope=="subgraph" {Some(1)} else {None}})).unwrap())
}
fn diff(state: &mut SessionState, id: &str) -> Value {
    handle_lock_diff(
        state,
        LockDiffInput {
            agent_id: "audit".into(),
            lock_id: id.into(),
        },
    )
    .unwrap()
}
#[test]
fn lock_snapshot_uses_external_ids_for_labels_and_neighbors() {
    let (_dir, mut state) = fixture();
    let out = create(&mut state, "audit", "subgraph", &["alpha"]).unwrap();
    assert_eq!(out["baseline_edges"], 1);
    let lock = &state.locks[out["lock_id"].as_str().unwrap()];
    assert_eq!(
        lock.baseline.nodes,
        HashSet::from(["id::a".into(), "id::b".into()])
    );
    assert_eq!(lock.scope.root_nodes, vec!["id::a"]);
}
#[test]
fn lock_node_scope_detects_incoming_and_outgoing_boundary_changes() {
    let (_dir, mut state) = fixture();
    let out = create(&mut state, "audit", "node", &["id::a"]).unwrap();
    let id = out["lock_id"].as_str().unwrap();
    *state.graph.write() = graph(true);
    state.graph_generation += 1;
    let d = diff(&mut state, id);
    assert_eq!(d["diff"]["no_changes"], false);
    assert_eq!(
        d["diff"]["boundary_edges_added"].as_array().unwrap().len(),
        1
    );
    assert!(d["diff"]["new_edges"].as_array().unwrap().is_empty());
    *state.graph.write() = graph(false);
    state.graph_generation += 1;
    assert_eq!(diff(&mut state, id)["diff"]["no_changes"], true);
}
#[test]
fn lock_diff_sees_weight_changes_without_graph_generation_bump() {
    let (_dir, mut state) = fixture();
    let out = create(&mut state, "audit", "subgraph", &["id::a"]).unwrap();
    let id = out["lock_id"].as_str().unwrap();
    state
        .graph
        .write()
        .csr
        .atomic_write_weight(EdgeIdx::new(0), FiniteF32::new(0.9), 10)
        .unwrap();
    let d = diff(&mut state, id);
    assert_eq!(d["diff"]["no_changes"], false);
    assert_eq!(d["diff"]["weight_changes"].as_array().unwrap().len(), 1);
}
#[test]
fn lock_invalid_scope_refuses_before_state_mutation() {
    let (_dir, mut state) = fixture();
    for value in [
        json!({"scope":"node","root_nodes":[]}),
        json!({"scope":"node","root_nodes":["missing"]}),
        json!({"scope":"query_neighborhood","root_nodes":["id::a"],"query":"alpha"}),
        json!({"scope":"subgraph","root_nodes":["id::a"],"radius":0}),
        json!({"scope":"subgraph","root_nodes":["id::a"],"radius":5}),
    ] {
        let mut value = value;
        value["agent_id"] = json!("audit");
        assert!(
            handle_lock_create(&mut state, serde_json::from_value(value.clone()).unwrap()).is_err(),
            "accepted {value}"
        );
        assert!(state.locks.is_empty());
    }
}
#[test]
fn lock_ids_cannot_clobber_another_agent_with_same_prefix() {
    let (_dir, mut state) = fixture();
    let a = create(&mut state, "samepref-A", "node", &["id::a"]).unwrap();
    let b = create(&mut state, "samepref-B", "node", &["id::b"]).unwrap();
    assert_ne!(a["lock_id"], b["lock_id"]);
    assert_eq!(state.locks.len(), 2);
}
#[test]
fn lock_ids_accept_unicode_agent_names() {
    let (_dir, mut state) = fixture();
    assert!(create(&mut state, "ééé€agent", "node", &["id::a"]).is_ok());
}

#[test]
fn lock_canonical_root_does_not_retarget_after_removal_or_duplicate_label() {
    let (_dir, mut state) = fixture();
    let out = create(&mut state, "audit", "node", &["alpha"]).unwrap();
    let id = out["lock_id"].as_str().unwrap();
    let mut replacement = Graph::new();
    replacement
        .add_node(
            "id::replacement",
            "alpha",
            NodeType::Function,
            &[],
            0.0,
            0.0,
        )
        .unwrap();
    replacement.finalize().unwrap();
    *state.graph.write() = replacement;
    state.graph_generation += 1;
    let d = diff(&mut state, id);
    assert_eq!(d["diff"]["removed_nodes"], json!(["id::a"]));
    assert_eq!(d["diff"]["new_nodes"], json!([]));
    assert!(handle_lock_rebase(
        &mut state,
        LockRebaseInput {
            agent_id: "audit".into(),
            lock_id: id.into()
        }
    )
    .is_err());
    assert!(state.locks[id].baseline.nodes.contains("id::a"));
}
#[test]
fn lock_ambiguous_labels_and_partial_ids_are_rejected() {
    let (_dir, mut state) = fixture();
    let mut g = graph(false);
    g.add_node("other::a", "alpha", NodeType::Function, &[], 0.0, 0.0)
        .unwrap();
    g.finalize().unwrap();
    *state.graph.write() = g;
    assert!(create(&mut state, "audit", "node", &["alpha"]).is_err());
    assert!(create(&mut state, "audit", "node", &["id::"]).is_err());
    assert!(create(&mut state, "audit", "node", &["id::a"]).is_ok());
}
fn arc_graph(arcs: &[(&str, &str, f32, EdgeDirection)]) -> Graph {
    let mut g = Graph::new();
    for name in ["a", "b", "c", "d"] {
        g.add_node(
            &format!("id::{name}"),
            name,
            NodeType::Function,
            &[],
            0.0,
            0.0,
        )
        .unwrap();
    }
    for &(source, target, weight, direction) in arcs {
        let a = g.resolve_id(&format!("id::{source}")).unwrap();
        let b = g.resolve_id(&format!("id::{target}")).unwrap();
        g.add_edge(
            a,
            b,
            "calls",
            FiniteF32::new(weight),
            direction,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    }
    g.finalize().unwrap();
    g
}
#[test]
fn lock_bfs_uses_shortest_distance_and_tracks_directed_parallel_arcs() {
    let (_dir, mut state) = fixture();
    let f = EdgeDirection::Forward;
    *state.graph.write() = arc_graph(&[
        ("a", "b", 0.5, f),
        ("a", "c", 0.5, f),
        ("c", "b", 0.5, f),
        ("b", "d", 0.5, f),
    ]);
    let out = handle_lock_create(
        &mut state,
        serde_json::from_value(
            json!({"agent_id":"audit","scope":"subgraph","root_nodes":["id::a"],"radius":2}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(out["baseline_nodes"], 4);
    let id = out["lock_id"].as_str().unwrap();
    *state.graph.write() = arc_graph(&[("a", "b", 0.5, f), ("a", "b", 0.8, f), ("b", "a", 0.5, f)]);
    let d = diff(&mut state, id);
    assert_eq!(d["diff"]["new_edges"].as_array().unwrap().len(), 2);
    assert_eq!(d["diff"]["no_changes"], false);
    handle_lock_rebase(
        &mut state,
        LockRebaseInput {
            agent_id: "audit".into(),
            lock_id: id.into(),
        },
    )
    .unwrap();
    *state.graph.write() = arc_graph(&[("a", "b", 0.8, f), ("b", "a", 0.5, f)]);
    let d = diff(&mut state, id);
    assert_eq!(d["diff"]["removed_edges"].as_array().unwrap().len(), 1);
    assert_eq!(d["diff"]["no_changes"], false);
    handle_lock_rebase(
        &mut state,
        LockRebaseInput {
            agent_id: "audit".into(),
            lock_id: id.into(),
        },
    )
    .unwrap();
    *state.graph.write() = arc_graph(&[("a", "b", 0.8, EdgeDirection::Bidirectional)]);
    let d = diff(&mut state, id);
    assert_eq!(d["diff"]["new_edges"].as_array().unwrap().len(), 2);
    assert_eq!(d["diff"]["removed_edges"].as_array().unwrap().len(), 2);
    let out = handle_lock_rebase(
        &mut state,
        LockRebaseInput {
            agent_id: "audit".into(),
            lock_id: id.into(),
        },
    )
    .unwrap();
    assert_eq!(out["baseline_edges"], 2);
}
#[test]
fn lock_rebase_edge_budget_refuses_without_replacing_baseline() {
    let (_dir, mut state) = fixture();
    let out = create(&mut state, "audit", "node", &["id::a"]).unwrap();
    let id = out["lock_id"].as_str().unwrap();
    state.perspective_limits.max_lock_baseline_edges = 1;
    *state.graph.write() = graph(true);
    assert!(handle_lock_rebase(
        &mut state,
        LockRebaseInput {
            agent_id: "audit".into(),
            lock_id: id.into()
        }
    )
    .is_err());
    assert_eq!(state.locks[id].baseline.edges.len(), 1);
}
#[test]
fn lock_diff_truncation_never_hides_change_and_watches_coalesce_per_lock() {
    let (_dir, mut state) = fixture();
    let a = create(&mut state, "audit", "subgraph", &["id::a"]).unwrap();
    let a = a["lock_id"].as_str().unwrap();
    let b = create(&mut state, "audit", "node", &["id::b"]).unwrap();
    let b = b["lock_id"].as_str().unwrap();
    for id in [a, b] {
        handle_lock_watch(
            &mut state,
            LockWatchInput {
                agent_id: "audit".into(),
                lock_id: id.into(),
                strategy: WatchStrategy::OnIngest,
            },
        )
        .unwrap();
    }
    for _ in 0..100 {
        state.notify_watchers(WatchTrigger::Ingest);
    }
    assert_eq!(state.pending_watcher_events.len(), 2);
    state.perspective_limits.max_lock_diff_new_nodes = 0;
    state.perspective_limits.max_lock_diff_new_edges = 0;
    *state.graph.write() = graph(true);
    let out = diff(&mut state, a);
    assert_eq!(out["diff_truncated"], true);
    assert_eq!(out["diff"]["no_changes"], false);
    assert_eq!(out["watcher_events_drained"], 1);
    assert_eq!(state.pending_watcher_events.len(), 1);
    assert_eq!(state.pending_watcher_events[0].lock_id, b);
    handle_lock_release(
        &mut state,
        LockReleaseInput {
            agent_id: "audit".into(),
            lock_id: b.into(),
        },
    )
    .unwrap();
    assert!(state.pending_watcher_events.is_empty());
}
#[test]
fn lock_path_canonicalizes_every_member_and_rejects_missing_members() {
    let (_dir, mut state) = fixture();
    let out = handle_lock_create(
        &mut state,
        serde_json::from_value(
            json!({"agent_id":"audit","scope":"path","root_nodes":["alpha"],"path_nodes":["beta"]}),
        )
        .unwrap(),
    )
    .unwrap();
    let id = out["lock_id"].as_str().unwrap();
    assert_eq!(
        state.locks[id].baseline.nodes,
        HashSet::from(["id::a".into(), "id::b".into()])
    );
    assert!(handle_lock_create(&mut state,serde_json::from_value(json!({"agent_id":"audit","scope":"path","root_nodes":["alpha"],"path_nodes":["missing"]})).unwrap()).is_err());
    assert_eq!(state.locks.len(), 1);
}

#[test]
fn lock_arc_keys_do_not_collide_when_ids_contain_delimiters() {
    let (_dir, mut state) = fixture();
    let mut g = Graph::new();
    let ids: Vec<_> = ["a->b", "c", "a", "b->c"]
        .into_iter()
        .map(|id| {
            g.add_node(id, id, NodeType::Function, &[], 0.0, 0.0)
                .unwrap()
        })
        .collect();
    for (a, b) in [(0, 1), (2, 3)] {
        g.add_edge(
            ids[a],
            ids[b],
            "x",
            FiniteF32::new(0.5),
            EdgeDirection::Forward,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    }
    g.finalize().unwrap();
    *state.graph.write() = g;
    let out = create(&mut state, "audit", "path", &["a->b", "c", "a", "b->c"]).unwrap();
    assert_eq!(out["baseline_edges"], 2);
}

#[test]
fn lock_parallel_arc_order_is_irrelevant_but_weight_changes_are_visible() {
    let (_dir, mut state) = fixture();
    let f = EdgeDirection::Forward;
    *state.graph.write() = arc_graph(&[("a", "b", 0.5, f), ("a", "b", 0.8, f)]);
    let out = create(&mut state, "audit", "subgraph", &["id::a"]).unwrap();
    let id = out["lock_id"].as_str().unwrap();
    *state.graph.write() = arc_graph(&[("a", "b", 0.8, f), ("a", "b", 0.5, f)]);
    assert_eq!(diff(&mut state, id)["diff"]["no_changes"], true);
    *state.graph.write() = arc_graph(&[("a", "b", 0.9, f), ("a", "b", 0.5, f)]);
    let out = diff(&mut state, id);
    assert_eq!(out["diff"]["no_changes"], false);
    assert_eq!(out["diff"]["weight_changes"].as_array().unwrap().len(), 1);
}

#[test]
fn lock_watch_manual_and_learn_observe_only_the_selected_trigger() {
    let (_dir, mut state) = fixture();
    let out = create(&mut state, "audit", "node", &["id::a"]).unwrap();
    let id = out["lock_id"].as_str().unwrap();
    handle_lock_watch(
        &mut state,
        LockWatchInput {
            agent_id: "audit".into(),
            lock_id: id.into(),
            strategy: WatchStrategy::Manual,
        },
    )
    .unwrap();
    state.notify_watchers(WatchTrigger::Ingest);
    state.notify_watchers(WatchTrigger::Learn);
    assert_eq!(diff(&mut state, id)["watcher_events_drained"], 0);
    handle_lock_watch(
        &mut state,
        LockWatchInput {
            agent_id: "audit".into(),
            lock_id: id.into(),
            strategy: WatchStrategy::OnLearn,
        },
    )
    .unwrap();
    state.notify_watchers(WatchTrigger::Ingest);
    assert!(state.pending_watcher_events.is_empty());
    state.notify_watchers(WatchTrigger::Learn);
    state.notify_watchers(WatchTrigger::Learn);
    assert_eq!(diff(&mut state, id)["watcher_events_drained"], 1);
    assert_eq!(diff(&mut state, id)["watcher_events_drained"], 0);
}
