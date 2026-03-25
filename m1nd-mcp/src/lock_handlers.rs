// === m1nd-mcp/src/lock_handlers.rs ===
// Handlers for the 5 lock MCP tools.
// Split from server.rs dispatch (Theme 8).

use crate::perspective::keys::{edge_content_key, normalize_bidi_endpoints};
use crate::perspective::state::*;
use crate::protocol::lock::*;
use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::types::EdgeIdx;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

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

/// Capture a baseline snapshot of a subgraph region.
/// Returns (nodes, edges) for the lock scope.
fn capture_baseline(
    state: &SessionState,
    scope: &LockScopeConfig,
) -> (HashSet<String>, HashMap<String, EdgeSnapshotEntry>) {
    let graph = state.graph.read();
    let mut nodes = HashSet::new();
    let mut edges = HashMap::new();

    // Collect root nodes: (usize_index, label) with 3-tier lookup
    let root_nids: Vec<(usize, String)> = scope
        .root_nodes
        .iter()
        .filter_map(|root| {
            // Tier 1: exact external_id match
            graph
                .id_to_node
                .iter()
                .find_map(|(interned, &nid)| {
                    let ext_id = graph.strings.resolve(*interned);
                    if ext_id == root.as_str() {
                        Some((nid.as_usize(), ext_id.to_string()))
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    // Tier 2: match by node label
                    for idx in 0..graph.num_nodes() as usize {
                        if idx < graph.nodes.label.len() {
                            let lbl = graph.strings.resolve(graph.nodes.label[idx]);
                            if lbl == root.as_str() {
                                return Some((idx, lbl.to_string()));
                            }
                        }
                    }
                    None
                })
                .or_else(|| {
                    // Tier 3: substring match on external_id
                    graph.id_to_node.iter().find_map(|(interned, &nid)| {
                        let ext_id = graph.strings.resolve(*interned);
                        if ext_id.contains(root.as_str()) {
                            Some((nid.as_usize(), ext_id.to_string()))
                        } else {
                            None
                        }
                    })
                })
        })
        .collect();

    match scope.scope_type {
        LockScope::Node => {
            for (_, label) in &root_nids {
                nodes.insert(label.clone());
            }
        }
        LockScope::Subgraph => {
            let radius = scope.radius.unwrap_or(2);
            let mut frontier: Vec<(usize, u32)> =
                root_nids.iter().map(|(idx, _)| (*idx, 0u32)).collect();
            let mut visited: HashSet<usize> = root_nids.iter().map(|(idx, _)| *idx).collect();

            for (_, label) in &root_nids {
                nodes.insert(label.clone());
            }

            while let Some((idx, depth)) = frontier.pop() {
                if depth >= radius || !graph.finalized {
                    continue;
                }
                if idx >= graph.num_nodes() as usize {
                    continue;
                }
                let start = if idx == 0 {
                    0
                } else {
                    graph.csr.offsets[idx] as usize
                };
                let end = graph.csr.offsets[idx + 1] as usize;

                for edge_pos in start..end {
                    if edge_pos >= graph.csr.targets.len() {
                        break;
                    }
                    let target_nid = graph.csr.targets[edge_pos];
                    let target_idx = target_nid.as_usize();
                    if target_idx >= graph.num_nodes() as usize {
                        continue;
                    }

                    let target_label = graph
                        .strings
                        .resolve(graph.nodes.label[target_idx])
                        .to_string();
                    nodes.insert(target_label.clone());

                    if !visited.contains(&target_idx) {
                        visited.insert(target_idx);
                        frontier.push((target_idx, depth + 1));
                    }
                }
            }
        }
        LockScope::QueryNeighborhood => {
            for (_, label) in &root_nids {
                nodes.insert(label.clone());
            }
        }
        LockScope::Path => {
            if let Some(ref path) = scope.path_nodes {
                for node in path {
                    nodes.insert(node.clone());
                }
            }
            for (_, label) in &root_nids {
                nodes.insert(label.clone());
            }
        }
    }

    // Capture edges between nodes in scope
    if graph.finalized {
        for node_label in &nodes {
            let node_nid = graph.id_to_node.iter().find_map(|(interned, &nid)| {
                let label = graph.strings.resolve(*interned);
                if label == node_label.as_str() {
                    Some(nid)
                } else {
                    None
                }
            });

            if let Some(nid) = node_nid {
                let idx = nid.as_usize();
                if idx >= graph.num_nodes() as usize {
                    continue;
                }
                let start = if idx == 0 {
                    0
                } else {
                    graph.csr.offsets[idx] as usize
                };
                let end = graph.csr.offsets[idx + 1] as usize;

                for edge_pos in start..end {
                    if edge_pos >= graph.csr.targets.len() {
                        break;
                    }
                    let target_nid = graph.csr.targets[edge_pos];
                    let target_idx = target_nid.as_usize();
                    if target_idx >= graph.num_nodes() as usize {
                        continue;
                    }

                    let target_label = graph
                        .strings
                        .resolve(graph.nodes.label[target_idx])
                        .to_string();

                    // Only include edges where both endpoints are in scope
                    if nodes.contains(&target_label) {
                        let relation = if edge_pos < graph.csr.relations.len() {
                            graph
                                .strings
                                .resolve(graph.csr.relations[edge_pos])
                                .to_string()
                        } else {
                            "unknown".to_string()
                        };

                        let weight = graph.csr.read_weight(EdgeIdx::new(edge_pos as u32)).get();

                        let (lo, hi) = normalize_bidi_endpoints(node_label, &target_label);
                        let key = edge_content_key(lo, hi, &relation);

                        edges.entry(key).or_insert(EdgeSnapshotEntry {
                            source: node_label.clone(),
                            target: target_label,
                            relation,
                            weight,
                        });
                    }
                }
            }
        }
    }

    (nodes, edges)
}

fn lock_create_contract(lock_id: &str) -> (String, Option<String>, Option<String>, Option<String>) {
    (
        "triaging".into(),
        Some("lock_diff".into()),
        Some(lock_id.into()),
        Some("Capture a diff against this lock after graph activity to see whether the protected region changed.".into()),
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
            Some("The baseline is stale. Rebase the lock before trusting this diff.".into()),
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

    let scope = LockScopeConfig {
        scope_type: input.scope.clone(),
        root_nodes: input.root_nodes.clone(),
        radius: input.radius,
        query: input.query,
        path_nodes: input.path_nodes,
    };

    // Capture baseline
    let (nodes, edges) = capture_baseline(state, &scope);

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
            key_format: "v1_content_addr".into(),
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
            "Watcher armed. Run `lock_diff` after ingest or learn events to inspect the protected scope."
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

    // If graph hasn't changed since baseline, fast-path: no changes
    if baseline.graph_generation == state.graph_generation && !baseline_stale {
        let diff = LockDiffResult {
            lock_id: input.lock_id.clone(),
            no_changes: true,
            new_nodes: vec![],
            removed_nodes: vec![],
            new_edges: vec![],
            removed_edges: vec![],
            boundary_edges_added: vec![],
            boundary_edges_removed: vec![],
            weight_changes: vec![],
            baseline_stale: false,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        };

        // Drain watcher events for this lock
        let drained = drain_watcher_events(state, &input.lock_id);

        // Update last_diff_ms
        if let Some(l) = state.locks.get_mut(&input.lock_id) {
            l.last_diff_ms = now_ms();
        }

        let output = LockDiffOutput {
            diff,
            watcher_events_drained: drained,
            rebase_suggested: None,
            proof_state: "ready_to_edit".into(),
            next_suggested_tool: None,
            next_suggested_target: None,
            next_step_hint: Some("The lock scope is unchanged relative to its baseline.".into()),
        };
        return serde_json::to_value(output).map_err(M1ndError::Serde);
    }

    // Capture current state for the same scope
    let scope = lock.scope.clone();
    let (current_nodes, current_edges) = capture_baseline(state, &scope);

    // Compute diffs
    let new_nodes: Vec<String> = current_nodes
        .difference(&baseline.nodes)
        .take(state.perspective_limits.max_lock_diff_new_nodes)
        .cloned()
        .collect();

    let removed_nodes: Vec<String> = baseline.nodes.difference(&current_nodes).cloned().collect();

    let current_edge_keys: HashSet<&String> = current_edges.keys().collect();
    let baseline_edge_keys: HashSet<&String> = baseline.edges.keys().collect();

    let new_edges: Vec<String> = current_edge_keys
        .difference(&baseline_edge_keys)
        .take(state.perspective_limits.max_lock_diff_new_edges)
        .map(|k| (*k).clone())
        .collect();

    let removed_edges: Vec<String> = baseline_edge_keys
        .difference(&current_edge_keys)
        .map(|k| (*k).clone())
        .collect();

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

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    let diff = LockDiffResult {
        lock_id: input.lock_id.clone(),
        no_changes: new_nodes.is_empty()
            && removed_nodes.is_empty()
            && new_edges.is_empty()
            && removed_edges.is_empty()
            && weight_changes.is_empty(),
        new_nodes,
        removed_nodes,
        new_edges,
        removed_edges,
        boundary_edges_added: vec![], // V1: simplified
        boundary_edges_removed: vec![],
        weight_changes,
        baseline_stale,
        elapsed_ms,
    };

    // Drain watcher events
    let drained = drain_watcher_events(state, &input.lock_id);

    // Update last_diff_ms
    if let Some(l) = state.locks.get_mut(&input.lock_id) {
        l.last_diff_ms = now_ms();
    }

    let rebase_suggested = if baseline_stale {
        Some("Baseline is stale. Call lock.rebase to re-capture.".into())
    } else {
        None
    };
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        lock_diff_contract(&diff);

    let output = LockDiffOutput {
        diff,
        watcher_events_drained: drained,
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
    let (nodes, edges) = capture_baseline(state, &scope);
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
        key_format: "v1_content_addr".into(),
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
            "Create a new lock before the next coordinated edit if you still need a guarded region."
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
