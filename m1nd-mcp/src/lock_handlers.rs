// === m1nd-mcp/src/lock_handlers.rs ===
// Handlers for the 5 lock MCP tools.
// Split from server.rs dispatch (Theme 8).

use crate::perspective::state::*;
use crate::protocol::lock::*;
use crate::session::SessionState;
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::types::EdgeIdx;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Validate lock ownership and return reference, or error.
fn require_lock<'a>(
    state: &'a SessionState,
    agent_id: &str,
    lock_id: &str,
) -> M1ndResult<&'a LockState> {
    let lock = state
        .locks
        .get(lock_id)
        .ok_or_else(|| M1ndError::LockNotFound {
            lock_id: lock_id.into(),
        })?;
    if lock.agent_id != agent_id {
        return Err(M1ndError::LockOwnership {
            lock_id: lock_id.into(),
            owner: lock.agent_id.clone(),
            caller: agent_id.into(),
        });
    }
    Ok(lock)
}

/// Resolve caller names once. A lock keeps canonical external IDs so a later
/// rename or a new node with the same label cannot silently retarget its scope.
fn canonical_scope(state: &SessionState, input: &LockCreateInput) -> M1ndResult<LockScopeConfig> {
    let invalid = |detail: String| M1ndError::InvalidParams {
        tool: "lock_create".into(),
        detail,
    };
    if input.agent_id.trim().is_empty() {
        return Err(invalid("agent_id must not be empty".into()));
    }
    if input.scope == LockScope::QueryNeighborhood || input.query.is_some() {
        return Err(invalid("query_neighborhood is not supported; use seek to resolve external IDs, then node, subgraph, or path scope".into()));
    }
    if input.root_nodes.is_empty() {
        return Err(invalid(
            "root_nodes must contain at least one existing node".into(),
        ));
    }
    let radius = match input.scope {
        LockScope::Subgraph => {
            let radius = input.radius.unwrap_or(2);
            if !(1..=state.perspective_limits.max_lock_subgraph_radius).contains(&radius) {
                return Err(invalid(format!(
                    "subgraph radius must be between 1 and {}",
                    state.perspective_limits.max_lock_subgraph_radius
                )));
            }
            Some(radius)
        }
        _ => {
            if input.radius.is_some() {
                return Err(invalid("radius applies only to subgraph scope".into()));
            }
            None
        }
    };
    if input.scope != LockScope::Path && input.path_nodes.is_some() {
        return Err(invalid("path_nodes applies only to path scope".into()));
    }
    if input.path_nodes.as_ref().is_some_and(Vec::is_empty) {
        return Err(invalid("path_nodes must not be empty when supplied".into()));
    }
    let supplied = input.root_nodes.len() + input.path_nodes.as_ref().map_or(0, Vec::len);
    if supplied > state.perspective_limits.max_lock_baseline_nodes {
        return Err(M1ndError::LockScopeTooLarge {
            node_count: supplied,
            cap: state.perspective_limits.max_lock_baseline_nodes,
        });
    }
    let graph = state.graph.read();
    let resolve = |name: &str| -> M1ndResult<String> {
        if graph.resolve_id(name).is_some() {
            return Ok(name.to_owned());
        }
        let matches: Vec<String> = graph
            .id_to_node
            .iter()
            .filter(|(_, nid)| graph.strings.resolve(graph.nodes.label[nid.as_usize()]) == name)
            .map(|(ext, _)| graph.strings.resolve(*ext).to_owned())
            .collect();
        match matches.as_slice() {
            [id] => Ok(id.clone()),
            [] => Err(invalid(format!(
                "node `{name}` not found; use an exact external ID or unique label"
            ))),
            _ => Err(invalid(format!(
                "node label `{name}` is ambiguous; use an exact external ID"
            ))),
        }
    };
    let root_nodes = input
        .root_nodes
        .iter()
        .map(|name| resolve(name))
        .collect::<M1ndResult<Vec<_>>>()?;
    let path_nodes = input
        .path_nodes
        .as_ref()
        .map(|nodes| {
            nodes
                .iter()
                .map(|name| resolve(name))
                .collect::<M1ndResult<Vec<_>>>()
        })
        .transpose()?;
    Ok(LockScopeConfig {
        scope_type: input.scope.clone(),
        root_nodes,
        radius,
        query: None,
        path_nodes,
    })
}

/// Snapshot canonical nodes plus every incident edge. Boundary edges are
/// classified against this snapshot's node set when diffing. No label is an ID.
/// Missing canonical roots on a later scan are removals, never new label lookups.
fn capture_baseline(
    state: &SessionState,
    scope: &LockScopeConfig,
) -> M1ndResult<(HashSet<String>, HashMap<String, EdgeSnapshotEntry>)> {
    let graph = state.graph.read();
    let mut ids = vec![None; graph.num_nodes() as usize];
    for (ext, nid) in &graph.id_to_node {
        ids[nid.as_usize()] = Some(graph.strings.resolve(*ext));
    }
    let mut selected = HashSet::new();
    let mut frontier = std::collections::VecDeque::new();
    for name in scope
        .root_nodes
        .iter()
        .chain(scope.path_nodes.iter().flatten())
    {
        if let Some(nid) = graph.resolve_id(name) {
            if selected.insert(nid.as_usize()) {
                frontier.push_back((nid.as_usize(), 0));
            }
        }
    }
    if scope.scope_type == LockScope::Subgraph && graph.finalized {
        let radius = scope.radius.unwrap_or(2);
        while let Some((idx, depth)) = frontier.pop_front() {
            if depth >= radius {
                continue;
            }
            for edge_pos in graph.csr.offsets[idx] as usize..graph.csr.offsets[idx + 1] as usize {
                let target = graph.csr.targets[edge_pos].as_usize();
                if selected.insert(target) {
                    frontier.push_back((target, depth + 1));
                }
                if selected.len() > state.perspective_limits.max_lock_baseline_nodes {
                    return Err(M1ndError::LockScopeTooLarge {
                        node_count: selected.len(),
                        cap: state.perspective_limits.max_lock_baseline_nodes,
                    });
                }
            }
        }
    }
    if selected.len() > state.perspective_limits.max_lock_baseline_nodes {
        return Err(M1ndError::LockScopeTooLarge {
            node_count: selected.len(),
            cap: state.perspective_limits.max_lock_baseline_nodes,
        });
    }
    let nodes = selected
        .iter()
        .filter_map(|idx| ids[*idx].map(str::to_owned))
        .collect();
    let mut groups: HashMap<(String, String, String, u8, bool), Vec<f32>> = HashMap::new();
    let mut observed_edges = 0;
    if graph.finalized {
        for src in 0..graph.num_nodes() as usize {
            for edge_pos in graph.csr.offsets[src] as usize..graph.csr.offsets[src + 1] as usize {
                let dst = graph.csr.targets[edge_pos].as_usize();
                if !selected.contains(&src) && !selected.contains(&dst) {
                    continue;
                }
                let (Some(source), Some(target)) = (ids[src], ids[dst]) else {
                    continue;
                };
                let relation = graph.strings.resolve(graph.csr.relations[edge_pos]);
                let direction = graph.csr.directions[edge_pos] as u8;
                let inhibitory = graph.csr.inhibitory[edge_pos];
                // Preserve directed CSR arcs, including both halves of a
                // bidirectional relation and every genuine parallel edge.
                groups
                    .entry((
                        source.to_owned(),
                        target.to_owned(),
                        relation.to_owned(),
                        direction,
                        inhibitory,
                    ))
                    .or_default()
                    .push(graph.csr.read_weight(EdgeIdx::new(edge_pos as u32)).get());
                observed_edges += 1;
                if observed_edges > state.perspective_limits.max_lock_baseline_edges {
                    return Err(M1ndError::LockScopeTooLarge {
                        node_count: observed_edges,
                        cap: state.perspective_limits.max_lock_baseline_edges,
                    });
                }
            }
        }
    }
    let mut edges = HashMap::new();
    for ((source, target, relation, direction, inhibitory), mut weights) in groups {
        weights.sort_by(f32::total_cmp);
        for (occurrence, weight) in weights.into_iter().enumerate() {
            let key = serde_json::to_string(&(
                &source, &target, &relation, direction, inhibitory, occurrence,
            ))
            .map_err(M1ndError::Serde)?;
            edges.insert(
                key,
                EdgeSnapshotEntry {
                    source: source.clone(),
                    target: target.clone(),
                    relation: relation.clone(),
                    weight,
                },
            );
        }
    }
    Ok((nodes, edges))
}

fn edge_keys<'a>(
    nodes: &HashSet<String>,
    edges: &'a HashMap<String, EdgeSnapshotEntry>,
    boundary: bool,
) -> HashSet<&'a String> {
    edges
        .iter()
        .filter_map(|(key, edge)| {
            let is_boundary = !(nodes.contains(&edge.source) && nodes.contains(&edge.target));
            (is_boundary == boundary).then_some(key)
        })
        .collect()
}

fn sorted_difference<T: std::hash::Hash + Eq + AsRef<str>>(
    left: &HashSet<T>,
    right: &HashSet<T>,
) -> Vec<String> {
    let mut diff: Vec<String> = left
        .difference(right)
        .map(|key| key.as_ref().to_owned())
        .collect();
    diff.sort();
    diff
}

fn lock_create_contract(lock_id: &str) -> (String, Option<String>, Option<String>, Option<String>) {
    (
        "triaging".into(),
        Some("lock_diff".into()),
        Some(lock_id.into()),
        Some("Capture a diff against this lock after graph activity to see whether the observed region changed.".into()),
    )
}

fn lock_diff_contract(
    diff: &LockDiffResult,
) -> (String, Option<String>, Option<String>, Option<String>) {
    if diff.baseline_stale {
        return (
            "proving".into(),
            Some("lock_rebase".into()),
            Some(diff.lock_id.clone()),
            Some("The graph changed since the baseline. Review this diff first. Rebase only after accepting the new baseline.".into()),
        );
    }

    if diff.no_changes {
        return (
            "ready_to_edit".into(),
            None,
            None,
            Some("The lock scope is unchanged relative to its baseline.".into()),
        );
    }

    if let Some(node) = diff
        .new_nodes
        .first()
        .or_else(|| diff.removed_nodes.first())
        .cloned()
    {
        return (
            "triaging".into(),
            Some("view".into()),
            Some(node.clone()),
            Some(format!(
                "Inspect `{}` first to understand the most visible structural change inside the lock scope.",
                node
            )),
        );
    }

    (
        "proving".into(),
        Some("lock_rebase".into()),
        Some(diff.lock_id.clone()),
        Some("Edge-level changes were detected. Rebase the lock after reviewing whether the scope should be refreshed.".into()),
    )
}

// ---------------------------------------------------------------------------
// lock.create
// ---------------------------------------------------------------------------

pub fn handle_lock_create(
    state: &mut SessionState,
    input: LockCreateInput,
) -> M1ndResult<serde_json::Value> {
    // Check limits
    let count = state.agent_lock_count(&input.agent_id);
    if count >= state.perspective_limits.max_locks_per_agent {
        return Err(M1ndError::LockLimitExceeded {
            agent_id: input.agent_id.clone(),
            current: count,
            limit: state.perspective_limits.max_locks_per_agent,
        });
    }

    // Memory budget check
    let mem = state.perspective_and_lock_memory_bytes();
    if mem >= state.perspective_limits.max_total_memory_bytes {
        return Err(M1ndError::LockLimitExceeded {
            agent_id: input.agent_id.clone(),
            current: count,
            limit: state.perspective_limits.max_locks_per_agent,
        });
    }

    let scope = canonical_scope(state, &input)?;

    // Capture baseline
    let (nodes, edges) = capture_baseline(state, &scope)?;

    // Check scope size
    if nodes.len() > state.perspective_limits.max_lock_baseline_nodes {
        return Err(M1ndError::LockScopeTooLarge {
            node_count: nodes.len(),
            cap: state.perspective_limits.max_lock_baseline_nodes,
        });
    }
    if edges.len() > state.perspective_limits.max_lock_baseline_edges {
        return Err(M1ndError::LockScopeTooLarge {
            node_count: edges.len(),
            cap: state.perspective_limits.max_lock_baseline_edges,
        });
    }

    let ts = now_ms();
    let lock_id = state.next_lock_id(&input.agent_id);

    let baseline_nodes = nodes.len();
    let baseline_edges = edges.len();

    let lock_state = LockState {
        lock_id: lock_id.clone(),
        agent_id: input.agent_id.clone(),
        scope,
        baseline: LockSnapshot {
            nodes,
            edges,
            graph_generation: state.graph_generation,
            captured_at_ms: ts,
            key_format: "v2_directed_external".into(),
        },
        watcher: None,
        baseline_stale: false,
        created_at_ms: ts,
        last_diff_ms: ts,
    };

    state.locks.insert(lock_id.clone(), lock_state);

    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        lock_create_contract(&lock_id);

    let output = LockCreateOutput {
        lock_id,
        scope: input.scope,
        baseline_nodes,
        baseline_edges,
        graph_generation: state.graph_generation,
        created_at_ms: ts,
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// lock.watch
// ---------------------------------------------------------------------------

pub fn handle_lock_watch(
    state: &mut SessionState,
    input: LockWatchInput,
) -> M1ndResult<serde_json::Value> {
    // Reject Periodic in V1
    if input.strategy == WatchStrategy::Periodic {
        return Err(M1ndError::WatchStrategyNotSupported {
            strategy: "periodic".into(),
        });
    }

    let _ = require_lock(state, &input.agent_id, &input.lock_id)?;

    let lock = state
        .locks
        .get_mut(&input.lock_id)
        .ok_or_else(|| M1ndError::LockNotFound {
            lock_id: input.lock_id.clone(),
        })?;

    let previous_strategy = lock.watcher.as_ref().map(|w| w.strategy.clone());

    lock.watcher = Some(WatchConfig {
        strategy: input.strategy.clone(),
        last_scan_ms: now_ms(),
    });
    let lock_id = input.lock_id;

    let output = LockWatchOutput {
        lock_id: lock_id.clone(),
        strategy: input.strategy,
        previous_strategy,
        proof_state: "triaging".into(),
        next_suggested_tool: Some("lock_diff".into()),
        next_suggested_target: Some(lock_id),
        next_step_hint: Some(
            "Watcher armed. Run `lock_diff` after ingest or learn events to inspect the observed scope."
                .into(),
        ),
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// lock.diff
// ---------------------------------------------------------------------------

pub fn handle_lock_diff(
    state: &mut SessionState,
    input: LockDiffInput,
) -> M1ndResult<serde_json::Value> {
    let _ = require_lock(state, &input.agent_id, &input.lock_id)?;

    let lock = state
        .locks
        .get(&input.lock_id)
        .ok_or_else(|| M1ndError::LockNotFound {
            lock_id: input.lock_id.clone(),
        })?;

    let start = std::time::Instant::now();
    let baseline = &lock.baseline;
    let baseline_stale = lock.baseline_stale;

    // Capture current state for the same scope
    let scope = lock.scope.clone();
    let (current_nodes, current_edges) = capture_baseline(state, &scope)?;

    // Compute complete sorted differences before applying output preview caps.
    let mut new_nodes = sorted_difference(&current_nodes, &baseline.nodes);
    let removed_nodes = sorted_difference(&baseline.nodes, &current_nodes);
    let current_internal = edge_keys(&current_nodes, &current_edges, false);
    let baseline_internal = edge_keys(&baseline.nodes, &baseline.edges, false);
    let mut new_edges = sorted_difference(&current_internal, &baseline_internal);
    let removed_edges = sorted_difference(&baseline_internal, &current_internal);
    let current_boundary = edge_keys(&current_nodes, &current_edges, true);
    let baseline_boundary = edge_keys(&baseline.nodes, &baseline.edges, true);
    let mut boundary_edges_added = sorted_difference(&current_boundary, &baseline_boundary);
    let boundary_edges_removed = sorted_difference(&baseline_boundary, &current_boundary);
    let current_edge_keys: HashSet<&String> = current_edges.keys().collect();
    let baseline_edge_keys: HashSet<&String> = baseline.edges.keys().collect();

    // Weight changes for shared edges
    let mut weight_changes = Vec::new();
    for key in current_edge_keys.intersection(&baseline_edge_keys) {
        if let (Some(current), Some(old)) = (current_edges.get(*key), baseline.edges.get(*key)) {
            let delta = (current.weight - old.weight).abs();
            if delta > 0.001 {
                weight_changes.push(EdgeWeightChange {
                    edge_key: (*key).clone(),
                    old_weight: old.weight,
                    new_weight: current.weight,
                });
            }
        }
    }

    weight_changes.sort_by(|a, b| a.edge_key.cmp(&b.edge_key));
    let no_changes = new_nodes.is_empty()
        && removed_nodes.is_empty()
        && new_edges.is_empty()
        && removed_edges.is_empty()
        && boundary_edges_added.is_empty()
        && boundary_edges_removed.is_empty()
        && weight_changes.is_empty();
    let diff_truncated = new_nodes.len() > state.perspective_limits.max_lock_diff_new_nodes
        || new_edges.len() > state.perspective_limits.max_lock_diff_new_edges
        || boundary_edges_added.len() > state.perspective_limits.max_lock_diff_new_edges;
    new_nodes.truncate(state.perspective_limits.max_lock_diff_new_nodes);
    new_edges.truncate(state.perspective_limits.max_lock_diff_new_edges);
    boundary_edges_added.truncate(state.perspective_limits.max_lock_diff_new_edges);
    let diff = LockDiffResult {
        lock_id: input.lock_id.clone(),
        no_changes,
        new_nodes,
        removed_nodes,
        new_edges,
        removed_edges,
        boundary_edges_added,
        boundary_edges_removed,
        weight_changes,
        baseline_stale,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    };

    // Drain watcher events
    let drained = drain_watcher_events(state, &input.lock_id);

    // Update last_diff_ms
    if let Some(l) = state.locks.get_mut(&input.lock_id) {
        l.last_diff_ms = now_ms();
    }

    let rebase_suggested = if baseline_stale {
        Some(
            "Review the reported changes, then call lock_rebase only if accepting a new baseline."
                .into(),
        )
    } else {
        None
    };
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        lock_diff_contract(&diff);

    let output = LockDiffOutput {
        diff,
        watcher_events_drained: drained,
        diff_truncated,
        rebase_suggested,
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

/// Drain pending watcher events for a specific lock.
fn drain_watcher_events(state: &mut SessionState, lock_id: &str) -> usize {
    let before = state.pending_watcher_events.len();
    state
        .pending_watcher_events
        .retain(|e| e.lock_id != lock_id);
    before - state.pending_watcher_events.len()
}

// ---------------------------------------------------------------------------
// lock.rebase
// ---------------------------------------------------------------------------

pub fn handle_lock_rebase(
    state: &mut SessionState,
    input: LockRebaseInput,
) -> M1ndResult<serde_json::Value> {
    let _ = require_lock(state, &input.agent_id, &input.lock_id)?;

    let lock = state
        .locks
        .get(&input.lock_id)
        .ok_or_else(|| M1ndError::LockNotFound {
            lock_id: input.lock_id.clone(),
        })?;

    let previous_generation = lock.baseline.graph_generation;
    let scope = lock.scope.clone();
    let watcher = lock.watcher.clone();

    // Capture new baseline
    let (nodes, edges) = capture_baseline(state, &scope)?;
    if nodes.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "lock_rebase".into(),
            detail: "The observed scope no longer exists. Inspect the removal diff, then release this lock or create a new scope.".into(),
        });
    }
    let ts = now_ms();
    let baseline_nodes = nodes.len();
    let baseline_edges = edges.len();

    // Check scope size
    if nodes.len() > state.perspective_limits.max_lock_baseline_nodes {
        return Err(M1ndError::LockScopeTooLarge {
            node_count: nodes.len(),
            cap: state.perspective_limits.max_lock_baseline_nodes,
        });
    }

    let lock = state
        .locks
        .get_mut(&input.lock_id)
        .ok_or_else(|| M1ndError::LockNotFound {
            lock_id: input.lock_id.clone(),
        })?;

    lock.baseline = LockSnapshot {
        nodes,
        edges,
        graph_generation: state.graph_generation,
        captured_at_ms: ts,
        key_format: "v2_directed_external".into(),
    };
    lock.baseline_stale = false;
    lock.last_diff_ms = ts;
    // Preserve watcher across rebase
    lock.watcher = watcher.clone();

    let output = LockRebaseOutput {
        lock_id: input.lock_id.clone(),
        previous_generation,
        new_generation: state.graph_generation,
        baseline_nodes,
        baseline_edges,
        watcher_preserved: watcher.is_some(),
        proof_state: "triaging".into(),
        next_suggested_tool: Some("lock_diff".into()),
        next_suggested_target: Some(input.lock_id),
        next_step_hint: Some(
            "The lock baseline is fresh again. Diff it after the next structural change to detect drift."
                .into(),
        ),
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// lock.release
// ---------------------------------------------------------------------------

pub fn handle_lock_release(
    state: &mut SessionState,
    input: LockReleaseInput,
) -> M1ndResult<serde_json::Value> {
    let _ = require_lock(state, &input.agent_id, &input.lock_id)?;

    state.locks.remove(&input.lock_id);

    // Clean up pending watcher events
    state
        .pending_watcher_events
        .retain(|e| e.lock_id != input.lock_id);

    let output = LockReleaseOutput {
        lock_id: input.lock_id,
        released: true,
        proof_state: "ready_to_edit".into(),
        next_suggested_tool: Some("lock_create".into()),
        next_suggested_target: None,
        next_step_hint: Some(
            "Create a new lock before the next coordinated edit if you still need a observed region."
                .into(),
        ),
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_diff_contract_prefers_rebase_when_baseline_is_stale() {
        let diff = LockDiffResult {
            lock_id: "lock-7".into(),
            no_changes: false,
            new_nodes: vec![],
            removed_nodes: vec![],
            new_edges: vec![],
            removed_edges: vec![],
            boundary_edges_added: vec![],
            boundary_edges_removed: vec![],
            weight_changes: vec![],
            baseline_stale: true,
            elapsed_ms: 1.0,
        };

        let (proof_state, tool, target, hint) = lock_diff_contract(&diff);
        assert_eq!(proof_state, "proving");
        assert_eq!(tool.as_deref(), Some("lock_rebase"));
        assert_eq!(target.as_deref(), Some("lock-7"));
        assert!(hint.unwrap().contains("Rebase"));
    }

    #[test]
    fn lock_diff_contract_prefers_view_for_changed_nodes() {
        let diff = LockDiffResult {
            lock_id: "lock-9".into(),
            no_changes: false,
            new_nodes: vec!["file::src/lib.rs".into()],
            removed_nodes: vec![],
            new_edges: vec![],
            removed_edges: vec![],
            boundary_edges_added: vec![],
            boundary_edges_removed: vec![],
            weight_changes: vec![],
            baseline_stale: false,
            elapsed_ms: 1.0,
        };

        let (proof_state, tool, target, hint) = lock_diff_contract(&diff);
        assert_eq!(proof_state, "triaging");
        assert_eq!(tool.as_deref(), Some("view"));
        assert_eq!(target.as_deref(), Some("file::src/lib.rs"));
        assert!(hint.unwrap().contains("file::src/lib.rs"));
    }
}

#[cfg(test)]
mod regression_tests;
