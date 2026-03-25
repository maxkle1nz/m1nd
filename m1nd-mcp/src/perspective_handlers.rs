// === m1nd-mcp/src/perspective_handlers.rs ===
// Handlers for the 12 perspective MCP tools.
// Split from server.rs dispatch (Theme 8).

use crate::perspective::keys::route_content_id;
use crate::perspective::state::*;
use crate::perspective::validation::*;
use crate::protocol::perspective::*;
use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::types::EdgeIdx;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn perspective_not_found_error(tool: &str, agent_id: &str, perspective_id: &str) -> M1ndError {
    M1ndError::InvalidParams {
        tool: tool.into(),
        detail: format!(
            "perspective `{}` was not found for agent `{}`. It may have been closed, expired, or belongs to a different agent. Call `perspective_list` to inspect active perspectives, or `perspective_start` to create a new one.",
            perspective_id, agent_id
        ),
    }
}

fn route_set_stale_error(tool: &str, requested_version: u64, current_version: u64) -> M1ndError {
    M1ndError::InvalidParams {
        tool: tool.into(),
        detail: format!(
            "stale `route_set_version` {}. Current version is {}. Call `perspective_routes` to refresh the route set, then retry this operation with the new `route_set_version`.",
            requested_version, current_version
        ),
    }
}

fn route_not_found_error(tool: &str, perspective_id: &str, route_ref: &str) -> M1ndError {
    M1ndError::InvalidParams {
        tool: tool.into(),
        detail: format!(
            "route reference `{}` was not found in perspective `{}`. Call `perspective_routes` to list the current page of routes, then retry with a fresh `route_id` or 1-based `route_index`.",
            route_ref, perspective_id
        ),
    }
}

fn perspective_route_contract(
    routes: &[Route],
    focus_node: Option<&str>,
    perspective_id: &str,
) -> (String, Option<String>, Option<String>, Option<String>) {
    if let Some(route) = routes.first() {
        return (
            "triaging".into(),
            Some("perspective_inspect".into()),
            Some(route.route_id.clone()),
            Some(format!(
                "Inspect route {} to validate the next hop from `{}`.",
                route.route_id, route.target_label
            )),
        );
    }

    if focus_node.is_some() {
        return (
            "blocked".into(),
            Some("perspective_suggest".into()),
            Some(perspective_id.into()),
            Some(
                "This perspective has no live routes. Ask `perspective_suggest` whether to backtrack or close it."
                    .into(),
            ),
        );
    }

    (
        "blocked".into(),
        Some("seek".into()),
        None,
        Some(
            "No focus node was resolved for this perspective. Use `seek` or restart with a stronger anchor."
                .into(),
        ),
    )
}

fn perspective_inspect_contract(
    route: &Route,
) -> (String, Option<String>, Option<String>, Option<String>) {
    if route.peek_available {
        (
            "proving".into(),
            Some("perspective_peek".into()),
            Some(route.route_id.clone()),
            Some(format!(
                "Peek route {} to inspect source evidence before moving the focus.",
                route.route_id
            )),
        )
    } else {
        (
            "triaging".into(),
            Some("perspective_follow".into()),
            Some(route.route_id.clone()),
            Some(format!(
                "Route {} has no peekable source. Follow it to keep the investigation moving.",
                route.route_id
            )),
        )
    }
}

fn perspective_suggestion_contract(
    perspective_id: &str,
    suggestion: &SuggestResult,
) -> (String, Option<String>, Option<String>, Option<String>) {
    if let Some(route_id) = suggestion.recommended_action.strip_prefix("follow ") {
        return (
            "triaging".into(),
            Some("perspective_follow".into()),
            Some(route_id.to_string()),
            Some(suggestion.why.clone()),
        );
    }

    match suggestion.recommended_action.as_str() {
        "perspective.back" => (
            "blocked".into(),
            Some("perspective_back".into()),
            Some(perspective_id.into()),
            Some(suggestion.why.clone()),
        ),
        "perspective.close" => (
            "blocked".into(),
            Some("perspective_close".into()),
            Some(perspective_id.into()),
            Some(suggestion.why.clone()),
        ),
        _ => ("triaging".into(), None, None, Some(suggestion.why.clone())),
    }
}

fn perspective_affinity_contract(
    perspective_id: &str,
    route: &Route,
    candidates: &[AffinityCandidate],
) -> (String, Option<String>, Option<String>, Option<String>) {
    if candidates.is_empty() {
        (
            "blocked".into(),
            Some("perspective_inspect".into()),
            Some(route.route_id.clone()),
            Some(format!(
                "Affinity is under-indexed here. Inspect route {} directly or peek its source before branching wider.",
                route.route_id
            )),
        )
    } else {
        (
            "proving".into(),
            Some("perspective_follow".into()),
            Some(route.route_id.clone()),
            Some(format!(
                "Affinity found probable continuations from `{}`. Follow the route or inspect it more deeply first.",
                perspective_id
            )),
        )
    }
}

fn perspective_list_contract(
    perspectives: &[PerspectiveSummary],
) -> (String, Option<String>, Option<String>, Option<String>) {
    if let Some(first) = perspectives.first() {
        (
            "triaging".into(),
            Some("perspective_routes".into()),
            Some(first.perspective_id.clone()),
            Some(format!(
                "Resume navigation in `{}` before opening another perspective.",
                first.perspective_id
            )),
        )
    } else {
        (
            "blocked".into(),
            Some("perspective_start".into()),
            None,
            Some(
                "No active perspectives are open. Start one with a seed query or anchor node."
                    .into(),
            ),
        )
    }
}

/// Check perspective ownership and return reference, or error.
fn require_perspective<'a>(
    state: &'a SessionState,
    agent_id: &str,
    perspective_id: &str,
    tool: &str,
) -> M1ndResult<&'a PerspectiveState> {
    state
        .get_perspective(agent_id, perspective_id)
        .ok_or_else(|| perspective_not_found_error(tool, agent_id, perspective_id))
}

/// Check perspective ownership and return mutable reference, or error.
fn require_perspective_mut<'a>(
    state: &'a mut SessionState,
    agent_id: &str,
    perspective_id: &str,
    tool: &str,
) -> M1ndResult<&'a mut PerspectiveState> {
    state
        .get_perspective_mut(agent_id, perspective_id)
        .ok_or_else(|| perspective_not_found_error(tool, agent_id, perspective_id))
}

/// Synthesize routes from graph for a focus node.
/// Uses graph's existing activation data to build route candidates.
/// This is a simplified V1 implementation that builds routes from direct graph neighbors.
fn synthesize_routes(
    state: &SessionState,
    focus_node: &str,
    lens: &PerspectiveLens,
    visited: &HashSet<String>,
    mode_ctx: &ModeContext,
) -> (Vec<Route>, u64) {
    let graph = state.graph.read();
    let version = now_ms();

    // Find the focus node in graph: try external_id match first, then label match
    let focus_nid = graph
        .id_to_node
        .iter()
        .find_map(|(interned, &nid)| {
            let ext_id = graph.strings.resolve(*interned);
            if ext_id == focus_node {
                Some(nid)
            } else {
                None
            }
        })
        .or_else(|| {
            // Fallback: match by node label (handles anchor_node = short label)
            for idx in 0..graph.num_nodes() as usize {
                if idx < graph.nodes.label.len() {
                    let lbl = graph.strings.resolve(graph.nodes.label[idx]);
                    if lbl == focus_node {
                        return Some(m1nd_core::types::NodeId::new(idx as u32));
                    }
                }
            }
            None
        })
        .or_else(|| {
            // Final fallback: substring match on external_id (contains)
            graph.id_to_node.iter().find_map(|(interned, &nid)| {
                let ext_id = graph.strings.resolve(*interned);
                if ext_id.contains(focus_node) {
                    Some(nid)
                } else {
                    None
                }
            })
        });

    let focus_nid = match focus_nid {
        Some(nid) => nid,
        None => return (vec![], version),
    };

    // Collect neighbor nodes as route candidates
    let mut routes = Vec::new();
    let mut route_index: u32 = 0;

    // Get edges from CSR if finalized
    if graph.finalized {
        let idx = focus_nid.as_usize();
        if idx < graph.num_nodes() as usize {
            let start = if idx == 0 {
                0
            } else {
                graph.csr.offsets[idx] as usize
            };
            let end = graph.csr.offsets[idx + 1] as usize;

            for edge_pos in start..end.min(start + lens.top_k as usize) {
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
                let _target_type = format!("{:?}", graph.nodes.node_type[target_idx]);

                // Determine route family from edge relation
                let family = RouteFamily::Structural; // V1: default to structural

                let route_id = route_content_id(&target_label, &family);

                // Compute basic score
                let weight: f32 = graph.csr.read_weight(EdgeIdx::new(edge_pos as u32)).get();

                let novelty = if visited.contains(&target_label) {
                    0.0
                } else {
                    1.0
                };

                let score = (weight * 0.6 + novelty * 0.4).min(1.0);

                // Check provenance availability
                let provenance_info = graph.resolve_node_provenance(target_nid);
                let peek_available =
                    !provenance_info.is_empty() && provenance_info.source_path.is_some();

                let provenance = if provenance_info.is_empty() {
                    None
                } else {
                    Some(RouteProvenance {
                        source_path: provenance_info.source_path,
                        line_start: provenance_info.line_start,
                        line_end: provenance_info.line_end,
                    })
                };

                route_index += 1;
                routes.push(Route {
                    route_id,
                    route_index,
                    family,
                    target_node: target_label.clone(),
                    target_label,
                    reason: format!("connected to {}", focus_node),
                    score,
                    peek_available,
                    provenance,
                });
            }
        }
    }

    // V1.1: Also collect reverse edges (nodes that point TO focus_node)
    // This prevents dead ends at hub/sink nodes like Files
    if graph.finalized && routes.len() < lens.top_k as usize {
        let remaining = lens.top_k as usize - routes.len();
        let mut seen_targets: HashSet<String> =
            routes.iter().map(|r| r.target_label.clone()).collect();

        for src_idx in 0..graph.num_nodes() as usize {
            if seen_targets.len() >= remaining + routes.len() {
                break;
            }
            let src_start = if src_idx == 0 {
                0
            } else {
                graph.csr.offsets[src_idx] as usize
            };
            let src_end = graph.csr.offsets[src_idx + 1] as usize;

            for edge_pos in src_start..src_end {
                if edge_pos >= graph.csr.targets.len() {
                    break;
                }
                let tgt = graph.csr.targets[edge_pos];
                if tgt == focus_nid && src_idx != focus_nid.as_usize() {
                    let src_label = graph
                        .strings
                        .resolve(graph.nodes.label[src_idx])
                        .to_string();
                    if seen_targets.contains(&src_label) {
                        continue;
                    }
                    seen_targets.insert(src_label.clone());

                    let family = RouteFamily::Structural;
                    let route_id = route_content_id(&src_label, &family);
                    let weight: f32 = graph.csr.read_weight(EdgeIdx::new(edge_pos as u32)).get();
                    let novelty = if visited.contains(&src_label) {
                        0.0
                    } else {
                        1.0
                    };
                    let score = (weight * 0.5 + novelty * 0.3).min(1.0); // slightly lower than forward

                    let src_nid = m1nd_core::types::NodeId::new(src_idx as u32);
                    let prov_info = graph.resolve_node_provenance(src_nid);
                    let peek_available = !prov_info.is_empty() && prov_info.source_path.is_some();
                    let provenance = if prov_info.is_empty() {
                        None
                    } else {
                        Some(RouteProvenance {
                            source_path: prov_info.source_path,
                            line_start: prov_info.line_start,
                            line_end: prov_info.line_end,
                        })
                    };

                    route_index += 1;
                    routes.push(Route {
                        route_id,
                        route_index,
                        family,
                        target_node: src_label.clone(),
                        target_label: src_label,
                        reason: format!("references {}", focus_node),
                        score,
                        peek_available,
                        provenance,
                    });
                }
            }
        }
    }

    // Sort by score descending, then deterministic tie-breaking (Theme 4)
    routes.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.route_id.cmp(&b.route_id))
            .then_with(|| a.target_node.cmp(&b.target_node))
            .then_with(|| a.family.ordinal().cmp(&b.family.ordinal()))
    });

    // Re-number after sort
    for (i, route) in routes.iter_mut().enumerate() {
        route.route_index = (i + 1) as u32;
    }

    (routes, version)
}

/// Build a diagnostic for empty results.
fn empty_diagnostic(state: &SessionState, reason: &str, suggestion: &str) -> Diagnostic {
    let graph = state.graph.read();
    Diagnostic {
        sources_checked: vec!["graph_neighbors".into()],
        sources_with_results: vec![],
        sources_failed: vec![],
        reason: reason.into(),
        suggestion: suggestion.into(),
        graph_stats: DiagnosticGraphStats {
            node_count: graph.num_nodes(),
            edge_count: graph.num_edges() as u64,
        },
    }
}

// ---------------------------------------------------------------------------
// perspective.start
// ---------------------------------------------------------------------------

pub fn handle_perspective_start(
    state: &mut SessionState,
    input: PerspectiveStartInput,
) -> M1ndResult<serde_json::Value> {
    // Check limits
    let count = state.agent_perspective_count(&input.agent_id);
    if count >= state.perspective_limits.max_perspectives_per_agent {
        return Err(M1ndError::PerspectiveLimitExceeded {
            agent_id: input.agent_id.clone(),
            current: count,
            limit: state.perspective_limits.max_perspectives_per_agent,
        });
    }

    // Memory budget check
    let mem = state.perspective_and_lock_memory_bytes();
    if mem >= state.perspective_limits.max_total_memory_bytes {
        return Err(M1ndError::PerspectiveLimitExceeded {
            agent_id: input.agent_id.clone(),
            current: count,
            limit: state.perspective_limits.max_perspectives_per_agent,
        });
    }

    let perspective_id = state.next_perspective_id(&input.agent_id);
    let lens = input.lens.unwrap_or_default();
    let ts = now_ms();

    // Determine mode
    let mode = if input.anchor_node.is_some() {
        PerspectiveMode::Anchored
    } else {
        PerspectiveMode::Local
    };

    // Find focus node: anchor_node if provided, otherwise first activated node from query
    let focus_node = input.anchor_node.clone().or_else(|| {
        // Try to find a node matching the query
        let graph = state.graph.read();
        graph.id_to_node.iter().find_map(|(interned, _)| {
            let label = graph.strings.resolve(*interned);
            if label.contains(&input.query) {
                Some(label.to_string())
            } else {
                None
            }
        })
    });

    let mode_ctx = ModeContext {
        mode: mode.clone(),
        anchor_node: input.anchor_node.clone(),
        anchor_query: Some(input.query.clone()),
    };

    // Synthesize initial routes
    let mut visited = HashSet::new();
    if let Some(ref f) = focus_node {
        visited.insert(f.clone());
    }

    let (routes, version) = if let Some(ref f) = focus_node {
        synthesize_routes(state, f, &lens, &visited, &mode_ctx)
    } else {
        (vec![], now_ms())
    };

    let total_routes = routes.len();
    let page_size = 6u32;
    let total_pages = if total_routes == 0 {
        1
    } else {
        (total_routes as u32).div_ceil(page_size)
    };
    let page_routes: Vec<Route> = routes.iter().take(page_size as usize).cloned().collect();

    let suggested = page_routes
        .first()
        .map(|r| format!("inspect {}", r.route_id));
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        perspective_route_contract(&page_routes, focus_node.as_deref(), &perspective_id);

    // Create perspective state
    let persp_state = PerspectiveState {
        perspective_id: perspective_id.clone(),
        agent_id: input.agent_id.clone(),
        mode: mode.clone(),
        anchor_node: input.anchor_node.clone(),
        anchor_query: Some(input.query.clone()),
        focus_node: focus_node.clone(),
        lens: lens.clone(),
        entry_path: focus_node.iter().cloned().collect(),
        navigation_history: vec![NavigationEvent {
            action: "start".into(),
            target: focus_node.clone(),
            timestamp_ms: ts,
            route_set_version: version,
        }],
        checkpoints: vec![],
        visited_nodes: visited,
        route_cache: Some(CachedRouteSet {
            routes,
            total_routes,
            page_size,
            version,
            synthesis_elapsed_ms: 0.0,
            captured_cache_generation: state.cache_generation,
        }),
        route_set_version: version,
        captured_cache_generation: state.cache_generation,
        stale: false,
        created_at_ms: ts,
        last_accessed_ms: ts,
        branches: vec![],
    };

    state.perspectives.insert(
        (input.agent_id.clone(), perspective_id.clone()),
        persp_state,
    );

    let output = PerspectiveStartOutput {
        perspective_id,
        mode,
        anchor_node: input.anchor_node,
        focus_node,
        routes: page_routes,
        total_routes,
        page: 1,
        total_pages,
        route_set_version: version,
        cache_generation: state.cache_generation,
        suggested,
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.routes
// ---------------------------------------------------------------------------

pub fn handle_perspective_routes(
    state: &mut SessionState,
    input: PerspectiveRoutesInput,
) -> M1ndResult<serde_json::Value> {
    // v0.4.0 FIX (ADVERSARY B1+B3): Re-synthesize routes when cache is None
    // instead of returning empty. This fixes the bug where perspective.start
    // creates with focus_node=None and routes never computed, or cache was
    // invalidated between start and routes calls.

    let persp = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.routes",
    )?;

    // Staleness check: instead of erroring, mark as stale and continue
    let mut _stale = false;
    if let Some(client_version) = input.route_set_version {
        if client_version != persp.route_set_version {
            _stale = true;
            // Continue with current version instead of erroring (ADVERSARY B3 fix)
        }
    }

    // FIX: If route_cache is None (invalidated or never computed), re-synthesize
    let needs_resynth = persp.route_cache.is_none();
    let focus_node = persp.focus_node.clone();
    let lens = persp.lens.clone();
    let visited = persp.visited_nodes.clone();
    let mode_ctx = ModeContext {
        mode: persp.mode.clone(),
        anchor_node: persp.anchor_node.clone(),
        anchor_query: persp.anchor_query.clone(),
    };

    if needs_resynth {
        if let Some(ref focus) = focus_node {
            // Re-synthesize routes from current graph state
            let (routes, version) = synthesize_routes(state, focus, &lens, &visited, &mode_ctx);
            let total_routes = routes.len();
            let page_size = 6u32;
            let cache_gen = state.cache_generation;

            if let Some(p) = state.get_perspective_mut(&input.agent_id, &input.perspective_id) {
                p.route_cache = Some(CachedRouteSet {
                    routes,
                    total_routes,
                    page_size,
                    version,
                    synthesis_elapsed_ms: 0.0,
                    captured_cache_generation: cache_gen,
                });
                p.route_set_version = version;
                p.stale = false;
            }
        }
    }

    // Re-read perspective after potential re-synthesis
    let persp = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.routes",
    )?;

    // Validate pagination
    let cached = persp.route_cache.as_ref();
    let total_routes = cached.map_or(0, |c| c.total_routes);
    let pagination = validate_pagination(input.page, input.page_size, total_routes)?;

    // Get routes page
    let routes: Vec<Route> = cached
        .map(|c| {
            c.routes
                .iter()
                .skip(pagination.offset)
                .take(pagination.page_size as usize)
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let mode_effective = if persp.mode == PerspectiveMode::Anchored {
        let hops = persp.entry_path.len();
        if hops > 8 {
            "local".into()
        } else {
            "anchored".into()
        }
    } else {
        "local".into()
    };

    let diagnostic = if routes.is_empty() && focus_node.is_none() {
        Some(empty_diagnostic(
            state,
            "no_focus_node",
            "Perspective has no focus node. Use anchor_node or a more specific query.",
        ))
    } else if routes.is_empty() {
        Some(empty_diagnostic(
            state,
            "graph_empty",
            "Try a different query or ingest more data",
        ))
    } else {
        None
    };

    let suggested = routes.first().map(|r| format!("inspect {}", r.route_id));

    let lens_summary = format!(
        "dims={} top_k={} xlr={}",
        persp.lens.dimensions.len(),
        persp.lens.top_k,
        persp.lens.xlr,
    );
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        perspective_route_contract(&routes, persp.focus_node.as_deref(), &input.perspective_id);

    let output = PerspectiveRoutesOutput {
        perspective_id: input.perspective_id.clone(),
        mode: persp.mode.clone(),
        mode_effective,
        anchor: persp.anchor_node.clone(),
        focus: persp.focus_node.clone(),
        lens_summary,
        page: pagination.page,
        total_pages: pagination.total_pages,
        total_routes,
        route_set_version: persp.route_set_version,
        cache_generation: persp.captured_cache_generation,
        routes,
        suggested,
        diagnostic,
        family_diversity_warning: None,
        dominant_family: None,
        page_size_clamped: pagination.page_size_clamped,
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };

    // Update last_accessed
    if let Some(p) = state.get_perspective_mut(&input.agent_id, &input.perspective_id) {
        p.last_accessed_ms = now_ms();
    }

    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.inspect
// ---------------------------------------------------------------------------

pub fn handle_perspective_inspect(
    state: &mut SessionState,
    input: PerspectiveInspectInput,
) -> M1ndResult<serde_json::Value> {
    let route_ref = validate_route_ref(&input.route_id, &input.route_index, "perspective.inspect")?;
    let persp = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.inspect",
    )?;

    // Staleness check
    if input.route_set_version != persp.route_set_version {
        return Err(route_set_stale_error(
            "perspective.inspect",
            input.route_set_version,
            persp.route_set_version,
        ));
    }

    // Find the route
    let cached = persp.route_cache.as_ref().ok_or_else(|| {
        route_not_found_error(
            "perspective.inspect",
            &input.perspective_id,
            "no cached routes",
        )
    })?;

    let route = match route_ref {
        ValidatedRouteRef::ById(ref id) => cached.routes.iter().find(|r| &r.route_id == id),
        ValidatedRouteRef::ByIndex(idx) => cached.routes.iter().find(|r| r.route_index == idx),
    }
    .ok_or_else(|| {
        route_not_found_error(
            "perspective.inspect",
            &input.perspective_id,
            &match &route_ref {
                ValidatedRouteRef::ById(id) => id.clone(),
                ValidatedRouteRef::ByIndex(idx) => format!("route_index={}", idx),
            },
        )
    })?;

    let provenance = route.provenance.as_ref().map(|p| InspectProvenance {
        source_path: p.source_path.clone(),
        line_start: p.line_start,
        line_end: p.line_end,
        namespace: None,
        provenance_stale: false,
    });
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        perspective_inspect_contract(route);

    let output = PerspectiveInspectOutput {
        route_id: route.route_id.clone(),
        route_index: route.route_index,
        family: route.family.clone(),
        target_node: route.target_node.clone(),
        target_label: route.target_label.clone(),
        target_type: "module".into(),
        path_preview: persp
            .entry_path
            .iter()
            .chain(std::iter::once(&route.target_node))
            .cloned()
            .collect(),
        family_explanation: format!("{:?} connection", route.family),
        score: route.score,
        score_breakdown: InspectScoreBreakdown {
            local_activation: route.score * 0.6,
            path_coherence: route.score * 0.25,
            novelty: if persp.visited_nodes.contains(&route.target_node) {
                0.0
            } else {
                1.0
            },
            anchor_relevance: if persp.mode == PerspectiveMode::Anchored {
                Some(0.15)
            } else {
                None
            },
            continuity: if persp.mode == PerspectiveMode::Anchored {
                Some(0.10)
            } else {
                None
            },
        },
        provenance,
        peek_available: route.peek_available,
        affinity_candidates: vec![],
        response_chars: 0, // filled after serialization
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };

    if let Some(p) = state.get_perspective_mut(&input.agent_id, &input.perspective_id) {
        p.last_accessed_ms = now_ms();
    }

    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.peek
// ---------------------------------------------------------------------------

pub fn handle_perspective_peek(
    state: &mut SessionState,
    input: PerspectivePeekInput,
) -> M1ndResult<serde_json::Value> {
    let route_ref = validate_route_ref(&input.route_id, &input.route_index, "perspective.peek")?;
    let persp = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.peek",
    )?;

    if input.route_set_version != persp.route_set_version {
        return Err(route_set_stale_error(
            "perspective.peek",
            input.route_set_version,
            persp.route_set_version,
        ));
    }

    let cached = persp.route_cache.as_ref().ok_or_else(|| {
        route_not_found_error(
            "perspective.peek",
            &input.perspective_id,
            "no cached routes",
        )
    })?;

    let route = match route_ref {
        ValidatedRouteRef::ById(ref id) => cached.routes.iter().find(|r| &r.route_id == id),
        ValidatedRouteRef::ByIndex(idx) => cached.routes.iter().find(|r| r.route_index == idx),
    }
    .ok_or_else(|| {
        route_not_found_error(
            "perspective.peek",
            &input.perspective_id,
            &match &route_ref {
                ValidatedRouteRef::ById(id) => id.clone(),
                ValidatedRouteRef::ByIndex(idx) => format!("route_index={}", idx),
            },
        )
    })?;

    if !route.peek_available {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: format!("peek not available for route {}", route.route_id),
        });
    }

    // Get source path from provenance
    let source_path = route
        .provenance
        .as_ref()
        .and_then(|p| p.source_path.as_ref())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: "no source path in provenance".into(),
        })?;

    let line_hint = route.provenance.as_ref().and_then(|p| p.line_start);

    // Run security pipeline (Theme 6)
    let content = crate::perspective::peek_security::secure_peek(
        source_path,
        &state.peek_security,
        line_hint,
        None,
    )?;
    let next_suggested_tool = Some("perspective_follow".into());
    let next_suggested_target = Some(route.route_id.clone());
    let next_step_hint = Some(format!(
        "If this snippet confirms the route, follow {} to move focus to `{}`.",
        route.route_id, route.target_label
    ));

    let output = PerspectivePeekOutput {
        route_id: route.route_id.clone(),
        route_index: route.route_index,
        target_node: route.target_node.clone(),
        content,
        proof_state: "proving".into(),
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };

    if let Some(p) = state.get_perspective_mut(&input.agent_id, &input.perspective_id) {
        p.last_accessed_ms = now_ms();
    }

    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.follow
// ---------------------------------------------------------------------------

pub fn handle_perspective_follow(
    state: &mut SessionState,
    input: PerspectiveFollowInput,
) -> M1ndResult<serde_json::Value> {
    let route_ref = validate_route_ref(&input.route_id, &input.route_index, "perspective.follow")?;

    // First borrow: read-only to validate and extract needed data
    let (target_node, previous_focus, mode, lens, visited, mode_ctx, version_check) = {
        let persp = require_perspective(
            state,
            &input.agent_id,
            &input.perspective_id,
            "perspective.follow",
        )?;

        if input.route_set_version != persp.route_set_version {
            return Err(route_set_stale_error(
                "perspective.follow",
                input.route_set_version,
                persp.route_set_version,
            ));
        }

        let cached = persp.route_cache.as_ref().ok_or_else(|| {
            route_not_found_error(
                "perspective.follow",
                &input.perspective_id,
                "no cached routes",
            )
        })?;

        let route = match &route_ref {
            ValidatedRouteRef::ById(id) => cached.routes.iter().find(|r| &r.route_id == id),
            ValidatedRouteRef::ByIndex(idx) => cached.routes.iter().find(|r| r.route_index == *idx),
        }
        .ok_or_else(|| {
            route_not_found_error(
                "perspective.follow",
                &input.perspective_id,
                &match &route_ref {
                    ValidatedRouteRef::ById(id) => id.clone(),
                    ValidatedRouteRef::ByIndex(idx) => format!("route_index={}", idx),
                },
            )
        })?;

        (
            route.target_node.clone(),
            persp.focus_node.clone(),
            persp.mode.clone(),
            persp.lens.clone(),
            persp.visited_nodes.clone(),
            ModeContext {
                mode: persp.mode.clone(),
                anchor_node: persp.anchor_node.clone(),
                anchor_query: persp.anchor_query.clone(),
            },
            persp.route_set_version,
        )
    };

    // Synthesize new routes at the target
    let mut new_visited = visited;
    new_visited.insert(target_node.clone());
    let (routes, new_version) =
        synthesize_routes(state, &target_node, &lens, &new_visited, &mode_ctx);

    let total_routes = routes.len();
    let page_size = 6u32;
    let total_pages = if total_routes == 0 {
        1
    } else {
        (total_routes as u32).div_ceil(page_size)
    };
    let page_routes: Vec<Route> = routes.iter().take(page_size as usize).cloned().collect();

    let diagnostic = if routes.is_empty() {
        Some(empty_diagnostic(
            state,
            "dead_end",
            "Try perspective.back or start a new perspective",
        ))
    } else {
        None
    };

    let suggested = page_routes
        .first()
        .map(|r| format!("inspect {}", r.route_id));
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        if let Some(route) = page_routes.first() {
            (
                "triaging".into(),
                Some("perspective_inspect".into()),
                Some(route.route_id.clone()),
                Some(format!(
                    "Inspect route {} to decide the next move from `{}`.",
                    route.route_id, target_node
                )),
            )
        } else {
            (
                "blocked".into(),
                Some("perspective_back".into()),
                Some(input.perspective_id.clone()),
                Some("This follow reached a dead end. Go back or start a sibling branch.".into()),
            )
        };

    let mode_effective = if mode == PerspectiveMode::Anchored {
        "anchored".into()
    } else {
        "local".into()
    };

    // Now mutate the perspective state
    let max_checkpoints = state.perspective_limits.max_checkpoints_per_perspective;
    let cache_gen = state.cache_generation;
    let persp = require_perspective_mut(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.follow",
    )?;
    let ts = now_ms();

    // Save checkpoint before moving
    persp.checkpoints.push(PerspectiveCheckpoint {
        focus_node: persp.focus_node.clone(),
        lens: persp.lens.clone(),
        mode: persp.mode.clone(),
        route_set_version: version_check,
        timestamp_ms: ts,
    });
    // Enforce checkpoint limit (Theme 5)
    while persp.checkpoints.len() > max_checkpoints {
        persp.checkpoints.remove(0);
    }

    persp.focus_node = Some(target_node.clone());
    persp.entry_path.push(target_node.clone());
    persp.visited_nodes = new_visited;
    persp.navigation_history.push(NavigationEvent {
        action: "follow".into(),
        target: Some(target_node.clone()),
        timestamp_ms: ts,
        route_set_version: new_version,
    });
    persp.route_cache = Some(CachedRouteSet {
        routes,
        total_routes,
        page_size,
        version: new_version,
        synthesis_elapsed_ms: 0.0,
        captured_cache_generation: cache_gen,
    });
    persp.route_set_version = new_version;
    persp.captured_cache_generation = cache_gen;
    persp.last_accessed_ms = ts;

    let output = PerspectiveFollowOutput {
        perspective_id: input.perspective_id,
        previous_focus,
        new_focus: target_node,
        mode,
        mode_effective,
        routes: page_routes,
        total_routes,
        page: 1,
        total_pages,
        route_set_version: new_version,
        cache_generation: state.cache_generation,
        suggested,
        diagnostic,
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.suggest
// ---------------------------------------------------------------------------

pub fn handle_perspective_suggest(
    state: &mut SessionState,
    input: PerspectiveSuggestInput,
) -> M1ndResult<serde_json::Value> {
    let persp = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.suggest",
    )?;

    if input.route_set_version != persp.route_set_version {
        return Err(route_set_stale_error(
            "perspective.suggest",
            input.route_set_version,
            persp.route_set_version,
        ));
    }

    let cached = persp.route_cache.as_ref();
    let top_route = cached.and_then(|c| c.routes.first());

    let suggestion = if let Some(route) = top_route {
        // Has routes: suggest following the highest-scored unvisited route
        let unvisited = cached.and_then(|c| {
            c.routes
                .iter()
                .find(|r| !persp.visited_nodes.contains(&r.target_node))
        });
        let best = unvisited.unwrap_or(route);

        SuggestResult {
            recommended_action: format!("follow {}", best.route_id),
            confidence: best.score.min(0.85),
            why: format!(
                "Highest-scored {} route to {}",
                format!("{:?}", best.family).to_lowercase(),
                best.target_label
            ),
            based_on: if persp.navigation_history.len() > 1 {
                "navigation_history".into()
            } else {
                "initial_ranking".into()
            },
            alternatives: cached
                .map(|c| {
                    c.routes
                        .iter()
                        .filter(|r| r.route_id != best.route_id)
                        .take(3)
                        .map(|r| SuggestAlternative {
                            action: format!("follow {}", r.route_id),
                            confidence: r.score.min(0.85),
                            why: format!("{:?} route to {}", r.family, r.target_label),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    } else {
        // Cold start or dead end
        SuggestResult {
            recommended_action: "perspective.back".into(),
            confidence: 0.50,
            why: "No routes available at current focus".into(),
            based_on: "exhaustion_recovery".into(),
            alternatives: vec![SuggestAlternative {
                action: "perspective.close".into(),
                confidence: 0.30,
                why: "Start fresh with a new perspective".into(),
            }],
        }
    };

    let diagnostic = if top_route.is_none() {
        Some(empty_diagnostic(
            state,
            "dead_end",
            "Navigate back or start a new perspective",
        ))
    } else {
        None
    };
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        perspective_suggestion_contract(&input.perspective_id, &suggestion);

    let output = PerspectiveSuggestOutput {
        perspective_id: input.perspective_id.clone(),
        suggestion,
        diagnostic,
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };

    if let Some(p) = state.get_perspective_mut(&input.agent_id, &input.perspective_id) {
        p.last_accessed_ms = now_ms();
    }

    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.affinity
// ---------------------------------------------------------------------------

pub fn handle_perspective_affinity(
    state: &mut SessionState,
    input: PerspectiveAffinityInput,
) -> M1ndResult<serde_json::Value> {
    let route_ref =
        validate_route_ref(&input.route_id, &input.route_index, "perspective.affinity")?;
    let persp = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.affinity",
    )?;

    if input.route_set_version != persp.route_set_version {
        return Err(route_set_stale_error(
            "perspective.affinity",
            input.route_set_version,
            persp.route_set_version,
        ));
    }

    let cached = persp.route_cache.as_ref().ok_or_else(|| {
        route_not_found_error(
            "perspective.affinity",
            &input.perspective_id,
            "no cached routes",
        )
    })?;

    let route = match route_ref {
        ValidatedRouteRef::ById(ref id) => cached.routes.iter().find(|r| &r.route_id == id),
        ValidatedRouteRef::ByIndex(idx) => cached.routes.iter().find(|r| r.route_index == idx),
    }
    .ok_or_else(|| {
        route_not_found_error(
            "perspective.affinity",
            &input.perspective_id,
            &match &route_ref {
                ValidatedRouteRef::ById(id) => id.clone(),
                ValidatedRouteRef::ByIndex(idx) => format!("route_index={}", idx),
            },
        )
    })?;

    // V1: affinity uses simplified computation
    // TODO: Full implementation uses confidence.rs normalization + geometric mean
    let candidates: Vec<AffinityCandidate> = vec![]; // V1: empty until engine_ops ready
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        perspective_affinity_contract(&input.perspective_id, route, &candidates);

    let output = PerspectiveAffinityOutput {
        route_id: route.route_id.clone(),
        target_node: route.target_node.clone(),
        notice: "Probable connections, not verified edges.".into(),
        candidates,
        diagnostic: Some(empty_diagnostic(
            state,
            "under_indexed",
            "Affinity requires more graph data for meaningful results",
        )),
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };

    if let Some(p) = state.get_perspective_mut(&input.agent_id, &input.perspective_id) {
        p.last_accessed_ms = now_ms();
    }

    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.branch
// ---------------------------------------------------------------------------

pub fn handle_perspective_branch(
    state: &mut SessionState,
    input: PerspectiveBranchInput,
) -> M1ndResult<serde_json::Value> {
    // Check branch limit
    let persp = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.branch",
    )?;
    if persp.branches.len() >= state.perspective_limits.max_branches_per_agent {
        return Err(M1ndError::BranchDepthExceeded {
            perspective_id: input.perspective_id.clone(),
            depth: persp.branches.len(),
            limit: state.perspective_limits.max_branches_per_agent,
        });
    }

    // Must have at least 1 navigation event
    if persp.navigation_history.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.branch".into(),
            detail: "cannot branch at root — navigate first".into(),
        });
    }

    let focus = persp.focus_node.clone();
    let branch_count = persp.branches.len();
    let branch_name = input
        .branch_name
        .unwrap_or_else(|| format!("branch_{}", branch_count + 1));
    let cloned_persp = persp.clone();

    // Clone current perspective into a new one
    let new_id = state.next_perspective_id(&input.agent_id);
    let mut new_persp = cloned_persp;
    new_persp.perspective_id = new_id.clone();
    new_persp.created_at_ms = now_ms();
    new_persp.last_accessed_ms = now_ms();

    // Record branch in parent
    let parent = require_perspective_mut(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.branch",
    )?;
    parent.branches.push(new_id.clone());

    // Insert new perspective
    state
        .perspectives
        .insert((input.agent_id.clone(), new_id.clone()), new_persp);

    let output = PerspectiveBranchOutput {
        perspective_id: input.perspective_id,
        branch_perspective_id: new_id.clone(),
        branch_name,
        branched_from_focus: focus,
        proof_state: "triaging".into(),
        next_suggested_tool: Some("perspective_routes".into()),
        next_suggested_target: Some(new_id.clone()),
        next_step_hint: Some(
            "Open the new branch's routes to continue from the forked state.".into(),
        ),
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.back
// ---------------------------------------------------------------------------

pub fn handle_perspective_back(
    state: &mut SessionState,
    input: PerspectiveBackInput,
) -> M1ndResult<serde_json::Value> {
    // Validate and extract data in a scoped borrow
    let (checkpoint, had_checkpoints) = {
        let persp = require_perspective(
            state,
            &input.agent_id,
            &input.perspective_id,
            "perspective.back",
        )?;
        if persp.checkpoints.is_empty() {
            return Err(M1ndError::NavigationAtRoot {
                perspective_id: input.perspective_id.clone(),
            });
        }
        (persp.checkpoints.last().cloned(), true)
    };

    let checkpoint = checkpoint.unwrap();

    // Now mutate
    let persp = require_perspective_mut(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.back",
    )?;
    persp.checkpoints.pop();
    persp.focus_node = checkpoint.focus_node.clone();
    persp.lens = checkpoint.lens.clone();
    persp.mode = checkpoint.mode.clone();
    if let Some(ref f) = persp.focus_node {
        if let Some(pos) = persp.entry_path.iter().rposition(|n| n == f) {
            persp.entry_path.truncate(pos + 1);
        }
    }

    let ts = now_ms();
    persp.navigation_history.push(NavigationEvent {
        action: "back".into(),
        target: checkpoint.focus_node.clone(),
        timestamp_ms: ts,
        route_set_version: persp.route_set_version,
    });
    persp.last_accessed_ms = ts;

    // Re-synthesize routes at restored focus
    let lens = persp.lens.clone();
    let visited = persp.visited_nodes.clone();
    let mode_ctx = ModeContext {
        mode: persp.mode.clone(),
        anchor_node: persp.anchor_node.clone(),
        anchor_query: persp.anchor_query.clone(),
    };
    let restored_focus = persp.focus_node.clone();
    let restored_mode = persp.mode.clone();

    let (routes, version) = if let Some(ref f) = restored_focus {
        synthesize_routes(state, f, &lens, &visited, &mode_ctx)
    } else {
        (vec![], now_ms())
    };

    let total_routes = routes.len();
    let page_size = 6u32;
    let total_pages = if total_routes == 0 {
        1
    } else {
        (total_routes as u32).div_ceil(page_size)
    };
    let page_routes: Vec<Route> = routes.iter().take(page_size as usize).cloned().collect();

    // Update cache
    let cache_gen = state.cache_generation;
    if let Some(p) = state.get_perspective_mut(&input.agent_id, &input.perspective_id) {
        p.route_cache = Some(CachedRouteSet {
            routes,
            total_routes,
            page_size,
            version,
            synthesis_elapsed_ms: 0.0,
            captured_cache_generation: cache_gen,
        });
        p.route_set_version = version;
    }

    let perspective_id = input.perspective_id;
    let back_next_tool = if let Some(route) = page_routes.first() {
        Some("perspective_inspect".into())
    } else {
        Some("perspective_suggest".into())
    };
    let back_next_target = if let Some(route) = page_routes.first() {
        Some(route.route_id.clone())
    } else {
        Some(perspective_id.clone())
    };
    let back_next_hint = if let Some(route) = page_routes.first() {
        Some(format!(
            "Inspect route {} after backtracking to re-enter the route set cleanly.",
            route.route_id
        ))
    } else {
        Some("This checkpoint also has no routes. Ask `perspective_suggest` how to recover.".into())
    };

    let output = PerspectiveBackOutput {
        perspective_id,
        restored_focus,
        restored_mode,
        routes: page_routes,
        total_routes,
        page: 1,
        total_pages,
        route_set_version: version,
        cache_generation: state.cache_generation,
        proof_state: if total_routes == 0 {
            "blocked".into()
        } else {
            "triaging".into()
        },
        next_suggested_tool: back_next_tool,
        next_suggested_target: back_next_target,
        next_step_hint: back_next_hint,
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.compare
// ---------------------------------------------------------------------------

pub fn handle_perspective_compare(
    state: &mut SessionState,
    input: PerspectiveCompareInput,
) -> M1ndResult<serde_json::Value> {
    let persp_a = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id_a,
        "perspective.compare",
    )?;
    let persp_b = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id_b,
        "perspective.compare",
    )?;

    let visited_a: HashSet<&String> = persp_a.visited_nodes.iter().collect();
    let visited_b: HashSet<&String> = persp_b.visited_nodes.iter().collect();

    let shared: Vec<String> = visited_a
        .intersection(&visited_b)
        .map(|s| (*s).clone())
        .collect();
    let unique_a: Vec<String> = visited_a
        .difference(&visited_b)
        .map(|s| (*s).clone())
        .collect();
    let unique_b: Vec<String> = visited_b
        .difference(&visited_a)
        .map(|s| (*s).clone())
        .collect();

    let gen_warning = if persp_a.captured_cache_generation != persp_b.captured_cache_generation {
        Some(format!(
            "Generation mismatch: {} vs {}. Results may not be directly comparable.",
            persp_a.captured_cache_generation, persp_b.captured_cache_generation
        ))
    } else {
        None
    };

    let output = PerspectiveCompareOutput {
        perspective_id_a: input.perspective_id_a,
        perspective_id_b: input.perspective_id_b,
        shared_nodes: shared,
        unique_to_a: unique_a,
        unique_to_b: unique_b,
        dimension_deltas: vec![], // V1: requires engine_ops for dimension scoring
        response_chars: 0,
        generation_mismatch_warning: gen_warning,
        proof_state: "triaging".into(),
        next_suggested_tool: Some("perspective_routes".into()),
        next_suggested_target: Some(persp_a.perspective_id.clone()),
        next_step_hint: Some(
            "Re-open one of the compared perspectives and inspect the route set where the delta looks most promising."
                .into(),
        ),
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.list
// ---------------------------------------------------------------------------

pub fn handle_perspective_list(
    state: &SessionState,
    input: PerspectiveListInput,
) -> M1ndResult<serde_json::Value> {
    let perspectives: Vec<PerspectiveSummary> = state
        .perspectives
        .iter()
        .filter(|((a, _), _)| a == &input.agent_id)
        .map(|((_, _), p)| PerspectiveSummary {
            perspective_id: p.perspective_id.clone(),
            mode: p.mode.clone(),
            focus_node: p.focus_node.clone(),
            route_count: p.route_cache.as_ref().map_or(0, |c| c.total_routes),
            nav_event_count: p.navigation_history.len(),
            stale: p.stale,
            created_at_ms: p.created_at_ms,
            last_accessed_ms: p.last_accessed_ms,
        })
        .collect();

    let total_memory = state.perspective_and_lock_memory_bytes();
    let (proof_state, next_suggested_tool, next_suggested_target, next_step_hint) =
        perspective_list_contract(&perspectives);

    let output = PerspectiveListOutput {
        agent_id: input.agent_id,
        perspectives,
        total_memory_bytes: total_memory,
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// perspective.close
// ---------------------------------------------------------------------------

pub fn handle_perspective_close(
    state: &mut SessionState,
    input: PerspectiveCloseInput,
) -> M1ndResult<serde_json::Value> {
    // Check it exists
    let _ = require_perspective(
        state,
        &input.agent_id,
        &input.perspective_id,
        "perspective.close",
    )?;

    // Find and release associated locks
    let agent_locks: Vec<String> = state
        .locks
        .values()
        .filter(|l| l.agent_id == input.agent_id)
        .map(|l| l.lock_id.clone())
        .collect();

    // Remove the perspective
    state
        .perspectives
        .remove(&(input.agent_id.clone(), input.perspective_id.clone()));

    // Remove associated locks (cascade cleanup, Theme 5)
    let mut released = Vec::new();
    for lock_id in &agent_locks {
        // Only release if no other perspectives from this agent reference it
        // V1: release all agent locks on close (simplified)
        if state.agent_perspective_count(&input.agent_id) == 0 {
            state.locks.remove(lock_id);
            released.push(lock_id.clone());
        }
    }

    let output = PerspectiveCloseOutput {
        perspective_id: input.perspective_id,
        closed: true,
        locks_released: released,
        proof_state: "ready_to_edit".into(),
        next_suggested_tool: Some("perspective_list".into()),
        next_suggested_target: None,
        next_step_hint: Some(
            "List active perspectives to continue an existing trail, or start a new one if this investigation is finished."
                .into(),
        ),
    };
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perspective_not_found_error_teaches_recovery() {
        let err = perspective_not_found_error("perspective.peek", "agent-1", "persp-9").to_string();
        assert!(err.contains("perspective `persp-9` was not found"));
        assert!(err.contains("perspective_list"));
        assert!(err.contains("perspective_start"));
    }

    #[test]
    fn route_set_stale_error_teaches_refresh_flow() {
        let err = route_set_stale_error("perspective.follow", 7, 11).to_string();
        assert!(err.contains("stale `route_set_version` 7"));
        assert!(err.contains("Current version is 11"));
        assert!(err.contains("perspective_routes"));
        assert!(err.contains("retry this operation"));
    }

    #[test]
    fn route_not_found_error_teaches_route_discovery() {
        let err =
            route_not_found_error("perspective.inspect", "persp-2", "route_index=4").to_string();
        assert!(err.contains("route reference `route_index=4` was not found"));
        assert!(err.contains("perspective_routes"));
        assert!(err.contains("fresh `route_id` or 1-based `route_index`"));
    }

    #[test]
    fn perspective_route_contract_prefers_inspect_when_routes_exist() {
        let route = Route {
            route_id: "R01".into(),
            route_index: 1,
            family: RouteFamily::Structural,
            target_node: "file::src/lib.rs".into(),
            target_label: "src/lib.rs".into(),
            reason: "hot edge".into(),
            score: 0.82,
            peek_available: true,
            provenance: None,
        };

        let (proof_state, tool, target, hint) =
            perspective_route_contract(&[route], Some("file::src/main.rs"), "persp-1");
        assert_eq!(proof_state, "triaging");
        assert_eq!(tool.as_deref(), Some("perspective_inspect"));
        assert_eq!(target.as_deref(), Some("R01"));
        assert!(hint.unwrap().contains("Inspect route R01"));
    }

    #[test]
    fn perspective_suggestion_contract_maps_back_recovery() {
        let suggestion = SuggestResult {
            recommended_action: "perspective.back".into(),
            confidence: 0.5,
            why: "No routes available".into(),
            based_on: "exhaustion_recovery".into(),
            alternatives: vec![],
        };

        let (proof_state, tool, target, hint) =
            perspective_suggestion_contract("persp-7", &suggestion);
        assert_eq!(proof_state, "blocked");
        assert_eq!(tool.as_deref(), Some("perspective_back"));
        assert_eq!(target.as_deref(), Some("persp-7"));
        assert_eq!(hint.as_deref(), Some("No routes available"));
    }
}
