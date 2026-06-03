// === crates/m1nd-mcp/src/tools.rs ===

use crate::help_guidance;
use crate::protocol::layers::{HelpInput, HelpMode, HelpRender};
use crate::protocol::*;
use crate::result_shaping::dedupe_ranked;
use crate::session::SessionState;
use crate::universal_docs;
use m1nd_core::error::M1ndResult;
use m1nd_core::query::QueryConfig;
use m1nd_core::temporal::ImpactDirection;
use m1nd_core::types::*;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Tool handlers — one per MCP tool (03-MCP Section 2)
// Each handler: parse input -> call engine -> format output.
// All handlers take &mut SessionState for graph + engine access.
// ---------------------------------------------------------------------------

pub const AGENT_TRUST_REQUIRED_TOOLS: [&str; 7] = [
    "health",
    "trust_selftest",
    "recovery_playbook",
    "doctor",
    "ingest",
    "seek",
    "help",
];

pub const HOST_BINDING_REQUIRED_TOOLS: [&str; 8] = [
    "health",
    "trust_selftest",
    "session_handshake",
    "recovery_playbook",
    "doctor",
    "ingest",
    "seek",
    "help",
];

fn normalized_ingest_mode(mode: &str) -> &str {
    if mode.eq_ignore_ascii_case("merge") {
        "merge"
    } else {
        "replace"
    }
}

fn playbook_step(
    id: &str,
    action: &str,
    reason: &str,
    tool: Option<&str>,
    arguments: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut step = serde_json::Map::new();
    step.insert("id".into(), serde_json::json!(id));
    step.insert("action".into(), serde_json::json!(action));
    step.insert("reason".into(), serde_json::json!(reason));
    if let Some(tool) = tool.filter(|value| !value.is_empty()) {
        step.insert("tool".into(), serde_json::json!(tool));
    }
    if let Some(arguments) = arguments {
        step.insert("arguments".into(), arguments);
    }
    serde_json::Value::Object(step)
}

fn note_learn_node_effect(
    weight_deltas: &mut HashMap<NodeId, f32>,
    edge_events: &mut HashMap<NodeId, u16>,
    node: NodeId,
    delta: f32,
    edge_count: u16,
) {
    *weight_deltas.entry(node).or_insert(0.0) += delta;
    let entry = edge_events.entry(node).or_insert(0);
    *entry = entry.saturating_add(edge_count);
}

fn maybe_store_auto_antibody(
    antibodies: &mut Vec<m1nd_core::antibody::Antibody>,
    candidate: m1nd_core::antibody::Antibody,
) -> bool {
    let is_duplicate = antibodies.iter().any(|existing| {
        m1nd_core::antibody::pattern_similarity(&existing.pattern, &candidate.pattern)
            >= m1nd_core::antibody::DUPLICATE_SIMILARITY_THRESHOLD
    });
    if is_duplicate {
        false
    } else {
        antibodies.push(candidate);
        true
    }
}

fn extension_language(ext: Option<&str>) -> String {
    match ext.unwrap_or_default() {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => "typescript",
        "go" => "go",
        "java" => "java",
        "md" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "" => "unknown",
        _ => "text",
    }
    .to_string()
}

fn simple_content_hash(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

fn build_file_inventory_entries(
    graph: &m1nd_core::graph::Graph,
    discovered_files: &[m1nd_ingest::walker::DiscoveredFile],
) -> Vec<crate::session::FileInventoryEntry> {
    let mut loc_by_external_id: HashMap<String, u32> = HashMap::new();
    for (interned, &nid) in &graph.id_to_node {
        let ext_id = graph.strings.resolve(*interned);
        if !ext_id.starts_with("file::") {
            continue;
        }
        let prov = graph.resolve_node_provenance(nid);
        let loc = prov
            .line_end
            .zip(prov.line_start)
            .map(|(end, start)| end.saturating_sub(start).saturating_add(1))
            .filter(|loc| *loc > 0);
        if let Some(loc) = loc {
            loc_by_external_id
                .entry(ext_id.to_string())
                .and_modify(|current: &mut u32| *current = (*current).max(loc))
                .or_insert(loc);
        }
    }

    discovered_files
        .iter()
        .map(|file| {
            let external_id = format!("file::{}", file.relative_path);
            crate::session::FileInventoryEntry {
                external_id: external_id.clone(),
                file_path: file.path.to_string_lossy().to_string(),
                size_bytes: file.size_bytes,
                last_modified_ms: (file.last_modified * 1000.0).round() as u64,
                language: extension_language(file.extension.as_deref()),
                commit_count: file.commit_count,
                loc: loc_by_external_id.get(&external_id).copied(),
                sha256: simple_content_hash(&file.path),
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PredictionSourceKind {
    CoChange,
    StructuralFallback,
}

impl PredictionSourceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::CoChange => "co_change",
            Self::StructuralFallback => "structural_fallback",
        }
    }

    fn score_bias(self) -> f32 {
        match self {
            Self::CoChange => 1.02,
            Self::StructuralFallback => 0.98,
        }
    }

    fn reason_fragment(self) -> &'static str {
        match self {
            Self::CoChange => "historical co-change",
            Self::StructuralFallback => "structural coupling",
        }
    }
}

struct RankedPrediction {
    target: NodeId,
    external_id: String,
    label: String,
    file_path: String,
    source: PredictionSourceKind,
    coupling_strength: f32,
    confidence: f32,
    final_score: f32,
    heuristic_factor: f32,
    trust_score: f32,
    trust_risk_multiplier: f32,
    trust_tier: String,
    tremor_magnitude: Option<f32>,
    tremor_observation_count: usize,
    tremor_risk_level: Option<String>,
    reason: String,
}

fn dampened_trust_factor(raw_factor: f32) -> f32 {
    1.0 + (raw_factor - 1.0) * 0.2
}

fn dampened_tremor_factor(alert: Option<&m1nd_core::tremor::TremorAlert>) -> f32 {
    1.0 + alert.map_or(0.0, |value| value.magnitude.min(1.0) * 0.1)
}

fn build_prediction_reason(
    source: PredictionSourceKind,
    trust_factor: f32,
    tremor_factor: f32,
    tremor_observation_count: usize,
) -> String {
    let mut parts = vec![source.reason_fragment().to_string()];
    if trust_factor > 1.01 {
        parts.push("low-trust risk prior".to_string());
    } else if trust_factor < 0.99 {
        parts.push("high-trust damping".to_string());
    }
    if tremor_factor > 1.01 && tremor_observation_count > 0 {
        parts.push("tremor acceleration".to_string());
    }
    parts.join(" + ")
}

/// Resolve L1GHT evidence marker nodes → code file nodes via `grounded_in` edges.
///
/// After a light ingest is merged with an existing code graph, evidence marker nodes
/// (tagged `"light:evidenced_by"`) reference code files by path in their label.
/// This pass walks all such markers, resolves the target `file::<path>` node, and
/// adds a `grounded_in` edge if one does not already exist.
///
/// Returns `(resolved, unresolved)`.
fn resolve_light_evidence(graph: &mut m1nd_core::graph::Graph) -> (usize, usize) {
    let tag_needle = "light:evidenced_by";
    let relation_needle = "grounded_in";

    // Phase 1 — collect already-present (marker, code) pairs to ensure idempotency.
    // We scan pending_edges because at this point the graph may not yet be finalized
    // (fresh merge sets finalized=false via add_node), so the CSR might not include
    // all edges.  Using pending_edges is correct because finalize() will drain them.
    let existing: std::collections::HashSet<(m1nd_core::types::NodeId, m1nd_core::types::NodeId)> = {
        let rel_interned = graph.strings.lookup(relation_needle);
        match rel_interned {
            Some(rel) => graph
                .csr
                .pending_edges
                .iter()
                .filter(|e| e.relation == rel)
                .map(|e| (e.source, e.target))
                .collect(),
            None => std::collections::HashSet::new(),
        }
    };

    // Phase 2 — iterate all nodes, find evidence markers, parse path from label.
    let node_count = graph.nodes.count as usize;
    let mut candidates: Vec<(m1nd_core::types::NodeId, String)> = Vec::new();

    let tag_interned = match graph.strings.lookup(tag_needle) {
        Some(t) => t,
        None => return (0, 0), // tag never interned → no evidence markers at all
    };

    for i in 0..node_count {
        let has_tag = graph.nodes.tags[i].iter().any(|&t| t == tag_interned);
        if !has_tag {
            continue;
        }
        let label = graph.strings.resolve(graph.nodes.label[i]).to_string();
        candidates.push((m1nd_core::types::NodeId::new(i as u32), label));
    }

    // Phase 3 — resolve each candidate and add bridge edges.
    let mut resolved = 0usize;
    let mut unresolved = 0usize;

    for (marker_id, label) in candidates {
        // Parse: substring after first "evidence:", trim, strip "./" prefix,
        // strip trailing ":<line>" (colon + digits at end), normalise backslashes.
        let path_raw = match label.find("evidence:") {
            Some(pos) => label[pos + "evidence:".len()..].trim().to_string(),
            None => {
                unresolved += 1;
                continue;
            }
        };
        let path_raw = path_raw.replace('\\', "/");
        let path_raw = path_raw
            .strip_prefix("./")
            .unwrap_or(&path_raw)
            .to_string();
        // Strip trailing ":<digits>" (line number)
        let path_clean = if let Some(colon_pos) = path_raw.rfind(':') {
            let suffix = &path_raw[colon_pos + 1..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                path_raw[..colon_pos].to_string()
            } else {
                path_raw.clone()
            }
        } else {
            path_raw.clone()
        };

        let candidate_id = format!("file::{}", path_clean);
        match graph.resolve_id(&candidate_id) {
            Some(code_node_id) => {
                if existing.contains(&(marker_id, code_node_id)) {
                    // Edge already present — idempotent skip
                    resolved += 1;
                    continue;
                }
                match graph.add_edge(
                    marker_id,
                    code_node_id,
                    relation_needle,
                    FiniteF32::new(0.8),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.8),
                ) {
                    Ok(_) => {
                        resolved += 1;
                    }
                    Err(_) => {
                        unresolved += 1;
                    }
                }
            }
            None => {
                unresolved += 1;
            }
        }
    }

    // CRITICAL: add_edge sets finalized=false, but only if at least one edge was added.
    // We set it explicitly when resolved > 0 so the caller's finalize() rebuilds the CSR.
    if resolved > 0 {
        graph.finalized = false;
    }

    (resolved, unresolved)
}

fn finalize_ingest(
    state: &mut SessionState,
    input: &IngestInput,
    adapter: &str,
    new_graph: m1nd_core::graph::Graph,
    stats: m1nd_ingest::IngestStats,
) -> M1ndResult<serde_json::Value> {
    let mode = normalized_ingest_mode(&input.mode).to_string();
    let namespace = input.namespace.clone().or_else(|| {
        if adapter == "memory" {
            Some("memory".to_string())
        } else if adapter == "light" {
            Some("light".to_string())
        } else {
            None
        }
    });

    let combined_graph = if mode == "merge" {
        let current = state.graph.read();
        if current.num_nodes() > 0 {
            m1nd_ingest::merge::merge_graphs(&current, &new_graph)?
        } else {
            new_graph
        }
    } else {
        new_graph
    };

    let (light_evidence_resolved, light_evidence_unresolved) = {
        let mut graph = state.graph.write();
        *graph = combined_graph;

        // Resolution pass: link L1GHT evidence markers → code file nodes.
        // Must run BEFORE finalize() so the new `grounded_in` edges are included in
        // the CSR rebuild.  Only needed for the light adapter where both node sets
        // coexist after a merge.
        let counts = if adapter == "light" {
            resolve_light_evidence(&mut graph)
        } else {
            (0, 0)
        };

        if !graph.finalized {
            graph.finalize()?;
        }

        counts
    };

    let inventory_entries = {
        let graph = state.graph.read();
        build_file_inventory_entries(&graph, &stats.discovered_files)
    };
    if mode == "replace" {
        state.reset_file_inventory();
    }
    state.record_file_inventory(inventory_entries);

    state.rebuild_engines()?;
    if adapter == "universal" && !state.document_cache.entries.is_empty() {
        universal_docs::refresh_all_document_semantics(state);
    }

    // Track ingest roots for L3 git discovery and scope normalization.
    // Replace mode resets the active roots to the new source of truth.
    if mode == "replace" {
        state.ingest_roots.clear();
        state.ingest_roots.push(input.path.clone());
    } else {
        // Keep the vector ordered oldest -> newest so path resolution can prefer
        // the most recent matching root deterministically.
        if let Some(pos) = state
            .ingest_roots
            .iter()
            .position(|root| root == &input.path)
        {
            let root = state.ingest_roots.remove(pos);
            state.ingest_roots.push(root);
        } else {
            state.ingest_roots.push(input.path.clone());
        }
    }
    let input_path = std::path::Path::new(&input.path);
    state.workspace_root = Some(if input_path.is_dir() {
        input.path.clone()
    } else {
        input_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_string_lossy()
            .to_string()
    });

    if let Err(e) = state.persist() {
        eprintln!("[m1nd] auto-persist after ingest failed: {}", e);
    }

    let (node_count, edge_count) = {
        let graph = state.graph.read();
        (graph.num_nodes(), graph.num_edges())
    };

    Ok(serde_json::json!({
        "mode": mode,
        "adapter": adapter,
        "namespace": namespace,
        "files_scanned": stats.files_scanned,
        "files_parsed": stats.files_parsed,
        "nodes_created": stats.nodes_created,
        "edges_created": stats.edges_created,
        "elapsed_ms": stats.elapsed_ms,
        "node_count": node_count,
        "edge_count": edge_count,
        "light_evidence_resolved": light_evidence_resolved,
        "light_evidence_unresolved": light_evidence_unresolved,
    }))
}

/// Handle activate (03-MCP Section 2.1).
/// Replaces: ConnectomeEngine.query() + AdaptiveXLREngine.query() + PlasticityEngine.query()
pub fn handle_activate(
    state: &mut SessionState,
    input: ActivateInput,
) -> M1ndResult<ActivateOutput> {
    let start = Instant::now();

    if input.query.trim().is_empty() {
        let (graph_state, recovery) = state.retrieval_failure_context(
            &input.agent_id,
            "activate",
            "blocked",
            Some(0),
            None,
            Some("activate query is empty"),
        );
        let agent_runtime_contract = Some(state.agent_runtime_contract(
            &input.agent_id,
            "activate",
            "blocked",
            Some(0),
            None,
            Some("activate query is empty"),
        ));
        return Ok(ActivateOutput {
            query: input.query,
            seeds: vec![],
            activated: vec![],
            ghost_edges: vec![],
            structural_holes: vec![],
            plasticity: PlasticityOutput {
                edges_strengthened: 0,
                edges_decayed: 0,
                ltp_events: 0,
                priming_nodes: 0,
            },
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            proof_state: "blocked".into(),
            next_suggested_tool: Some("recovery_playbook".into()),
            next_suggested_target: None,
            next_step_hint: Some("Call recovery_playbook with the provided recovery.arguments payload before falling back to shell search.".into()),
            confidence: Some(0.0),
            why_this_next_step: Some("Activate needs at least one query seed before graph propagation can produce evidence.".into()),
            what_is_missing: Some("A non-empty activation query is still missing.".into()),
            graph_state,
            recovery,
            agent_runtime_contract,
        });
    }

    let dimensions: Vec<Dimension> = input
        .dimensions
        .iter()
        .filter_map(|d| match d.as_str() {
            "structural" => Some(Dimension::Structural),
            "semantic" => Some(Dimension::Semantic),
            "temporal" => Some(Dimension::Temporal),
            "causal" => Some(Dimension::Causal),
            _ => None,
        })
        .collect();

    let config = QueryConfig {
        query: input.query.clone(),
        agent_id: input.agent_id.clone(),
        top_k: input.top_k,
        dimensions: if dimensions.is_empty() {
            vec![
                Dimension::Structural,
                Dimension::Semantic,
                Dimension::Temporal,
                Dimension::Causal,
            ]
        } else {
            dimensions
        },
        xlr_enabled: input.xlr,
        include_ghost_edges: input.include_ghost_edges,
        include_structural_holes: input.include_structural_holes,
        propagation: PropagationConfig::default(),
    };

    let result = {
        let mut graph = state.graph.write();
        state.orchestrator.query(&mut graph, &config)?
    };

    state.queries_processed += 1;
    if state.should_persist() {
        let _ = state.persist();
    }

    let graph = state.graph.read();

    // Map seeds
    let seeds: Vec<SeedOutput> = result
        .activation
        .seeds
        .iter()
        .map(|&(node, relevance)| {
            let idx = node.as_usize();
            let label = if idx < graph.num_nodes() as usize {
                graph.strings.resolve(graph.nodes.label[idx]).to_string()
            } else {
                format!("node_{}", idx)
            };
            SeedOutput {
                node_id: label.clone(),
                label,
                relevance: relevance.get(),
            }
        })
        .collect();
    let seed_count = seeds.len();
    let seeds = dedupe_ranked(seeds, seed_count);

    // Build reverse lookup: NodeId -> external ID string
    let mut node_to_ext: Vec<String> = vec![String::new(); graph.num_nodes() as usize];
    for (interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < node_to_ext.len() {
            node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
        }
    }

    // Map activated nodes
    let activated: Vec<ActivatedNodeOutput> = result
        .activation
        .activated
        .iter()
        .map(|a| {
            let idx = a.node.as_usize();
            let (ext_id, label, node_type, tags, provenance) = if idx < graph.num_nodes() as usize {
                let eid = &node_to_ext[idx];
                let l = graph.strings.resolve(graph.nodes.label[idx]).to_string();
                let t = format!("{:?}", graph.nodes.node_type[idx]);
                let tg: Vec<String> = graph.nodes.tags[idx]
                    .iter()
                    .map(|&ti| graph.strings.resolve(ti).to_string())
                    .collect();
                let provenance = graph.resolve_node_provenance(a.node);
                let provenance = if provenance.is_empty() {
                    None
                } else {
                    Some(ProvenanceOutput {
                        source_path: provenance.source_path,
                        line_start: provenance.line_start,
                        line_end: provenance.line_end,
                        excerpt: provenance.excerpt,
                        namespace: provenance.namespace,
                        canonical: provenance.canonical,
                    })
                };
                (eid.clone(), l, t, tg, provenance)
            } else {
                (
                    format!("node_{}", idx),
                    format!("node_{}", idx),
                    "Unknown".into(),
                    vec![],
                    None,
                )
            };
            ActivatedNodeOutput {
                node_id: ext_id,
                label,
                node_type,
                activation: a.activation.get(),
                dimensions: DimensionsOutput {
                    structural: a.dimensions[0].get(),
                    semantic: a.dimensions[1].get(),
                    temporal: a.dimensions[2].get(),
                    causal: a.dimensions[3].get(),
                },
                pagerank: if idx < graph.nodes.pagerank.len() {
                    graph.nodes.pagerank[idx].get()
                } else {
                    0.0
                },
                tags,
                provenance,
            }
        })
        .collect();
    let activated = dedupe_ranked(activated, input.top_k);

    // Map ghost edges
    let ghost_edges: Vec<GhostEdgeOutput> = result
        .ghost_edges
        .iter()
        .map(|ge| {
            let src_idx = ge.source.as_usize();
            let tgt_idx = ge.target.as_usize();
            let src = if src_idx < graph.num_nodes() as usize {
                graph
                    .strings
                    .resolve(graph.nodes.label[src_idx])
                    .to_string()
            } else {
                format!("node_{}", src_idx)
            };
            let tgt = if tgt_idx < graph.num_nodes() as usize {
                graph
                    .strings
                    .resolve(graph.nodes.label[tgt_idx])
                    .to_string()
            } else {
                format!("node_{}", tgt_idx)
            };
            GhostEdgeOutput {
                source: src,
                target: tgt,
                shared_dimensions: ge
                    .shared_dimensions
                    .iter()
                    .map(|d| format!("{:?}", d).to_lowercase())
                    .collect(),
                strength: ge.strength.get(),
            }
        })
        .collect();

    // Map structural holes
    let structural_holes: Vec<StructuralHoleOutput> = result
        .structural_holes
        .iter()
        .map(|sh| {
            let idx = sh.node.as_usize();
            let (label, node_type) = if idx < graph.num_nodes() as usize {
                (
                    graph.strings.resolve(graph.nodes.label[idx]).to_string(),
                    format!("{:?}", graph.nodes.node_type[idx]),
                )
            } else {
                (format!("node_{}", idx), "Unknown".into())
            };
            StructuralHoleOutput {
                node_id: label.clone(),
                label,
                node_type,
                reason: sh.reason.clone(),
            }
        })
        .collect();

    let plasticity = PlasticityOutput {
        edges_strengthened: result.plasticity.edges_strengthened,
        edges_decayed: result.plasticity.edges_decayed,
        ltp_events: result.plasticity.ltp_events,
        priming_nodes: result.plasticity.priming_nodes,
    };

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let visited_files: Vec<String> = activated
        .iter()
        .filter_map(|entry| entry.provenance.as_ref()?.source_path.clone())
        .collect();
    let visited_nodes: Vec<String> = activated
        .iter()
        .map(|entry| entry.node_id.clone())
        .collect();
    drop(graph);
    state.note_coverage(&input.agent_id, "activate", visited_files, visited_nodes);

    let activate_help_input = HelpInput {
        agent_id: input.agent_id.clone(),
        tool_name: Some("activate".into()),
        mode: None,
        intent: Some(input.query.clone()),
        stage: None,
        path: activated
            .first()
            .and_then(|entry| entry.provenance.as_ref())
            .and_then(|provenance| provenance.source_path.clone()),
        error_text: None,
        recent_tools: vec![],
        max_suggestions: None,
        render: Some(HelpRender::None),
    };
    let activated_count = activated.len();
    let default_activate_proof_state = if activated_count == 0 {
        "blocked"
    } else {
        "triaging"
    };
    let activate_projection = help_guidance::runtime_projection_for_tool(
        "activate",
        &activate_help_input,
        default_activate_proof_state,
    );
    let next_suggested_target = activated
        .first()
        .and_then(|entry| {
            entry
                .provenance
                .as_ref()
                .and_then(|provenance| provenance.source_path.clone())
                .or_else(|| Some(entry.node_id.clone()))
        })
        .or_else(|| {
            activate_projection
                .as_ref()
                .and_then(|projection| projection.next_suggested_target.clone())
        });
    let proof_state = activate_projection
        .as_ref()
        .map(|projection| projection.proof_state.clone())
        .unwrap_or_else(|| default_activate_proof_state.into());
    let failed_retrieval = proof_state == "blocked";
    let (graph_state, recovery) = state.retrieval_failure_context(
        &input.agent_id,
        "activate",
        &proof_state,
        Some(activated_count as u64),
        None,
        None,
    );
    let agent_runtime_contract = Some(state.agent_runtime_contract(
        &input.agent_id,
        "activate",
        &proof_state,
        Some(activated_count as u64),
        None,
        None,
    ));
    let next_suggested_tool = if failed_retrieval {
        Some("recovery_playbook".into())
    } else {
        activate_projection
            .as_ref()
            .and_then(|projection| projection.next_suggested_tool.clone())
    };
    let next_suggested_target = if failed_retrieval {
        None
    } else {
        next_suggested_target
    };
    let next_step_hint = if failed_retrieval {
        Some("Call recovery_playbook with the provided recovery.arguments payload before falling back to shell search.".into())
    } else {
        activate_projection
            .as_ref()
            .and_then(|projection| projection.next_step_hint.clone())
    };

    Ok(ActivateOutput {
        query: input.query,
        seeds,
        activated,
        ghost_edges,
        structural_holes,
        plasticity,
        elapsed_ms,
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
        confidence: activate_projection
            .as_ref()
            .and_then(|projection| projection.confidence),
        why_this_next_step: activate_projection
            .as_ref()
            .and_then(|projection| projection.why_this_next_step.clone()),
        what_is_missing: activate_projection
            .as_ref()
            .and_then(|projection| projection.what_is_missing.clone()),
        graph_state,
        recovery,
        agent_runtime_contract,
    })
}

/// Handle impact (03-MCP Section 2.2).
/// Replaces: ImpactRadiusCalculator.compute() + CausalChainDetector.detect()
pub fn handle_impact(state: &mut SessionState, input: ImpactInput) -> M1ndResult<ImpactOutput> {
    let graph = state.graph.read();

    let impact_help_input = HelpInput {
        agent_id: input.agent_id.clone(),
        tool_name: Some("impact".into()),
        mode: None,
        intent: Some(format!("impact for {}", input.node_id)),
        stage: None,
        path: Some(input.node_id.clone()),
        error_text: None,
        recent_tools: vec![],
        max_suggestions: None,
        render: Some(HelpRender::None),
    };
    let impact_projection =
        help_guidance::runtime_projection_for_tool("impact", &impact_help_input, "triaging");

    let node_id = graph.resolve_id(&input.node_id);
    let node = match node_id {
        Some(n) => n,
        None => {
            let recovery_input = HelpInput {
                agent_id: input.agent_id.clone(),
                tool_name: Some("impact".into()),
                mode: Some(HelpMode::Recovery),
                intent: None,
                stage: None,
                path: Some(input.node_id.clone()),
                error_text: Some(format!("Node not found: {}", input.node_id)),
                recent_tools: vec![],
                max_suggestions: Some(3),
                render: Some(HelpRender::None),
            };
            let recovery = help_guidance::build_recovery_resolution(&recovery_input);
            let projection =
                help_guidance::runtime_projection_from_resolution(&recovery, "blocked");
            return Ok(ImpactOutput {
                source: input.node_id.clone(),
                source_label: input.node_id,
                direction: input.direction.clone(),
                blast_radius: vec![],
                total_energy: 0.0,
                max_hops_reached: 0,
                causal_chains: vec![],
                proof_state: projection.proof_state,
                next_suggested_tool: projection.next_suggested_tool,
                next_suggested_target: projection.next_suggested_target,
                next_step_hint: projection.next_step_hint,
                total_blast_nodes: 0,
                truncated: false,
            });
        }
    };

    let direction = match input.direction.as_str() {
        "reverse" => ImpactDirection::Reverse,
        "both" => ImpactDirection::Both,
        _ => ImpactDirection::Forward,
    };

    let impact = state
        .temporal
        .impact_calculator
        .compute(&graph, node, direction)?;

    // Causal chains
    let chains = if input.include_causal_chains {
        state.temporal.chain_detector.detect(&graph, node)?
    } else {
        vec![]
    };

    let source_label = {
        let idx = node.as_usize();
        if idx < graph.num_nodes() as usize {
            graph.strings.resolve(graph.nodes.label[idx]).to_string()
        } else {
            input.node_id.clone()
        }
    };

    let max_nodes_cap = input.max_nodes.unwrap_or(150);
    let total_blast_nodes = impact.blast_radius.len();

    // Sort by signal_strength descending before capping
    let mut sorted_blast = impact.blast_radius.clone();
    sorted_blast.sort_by(|a, b| {
        b.signal_strength
            .get()
            .partial_cmp(&a.signal_strength.get())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let blast_radius: Vec<BlastRadiusEntry> = sorted_blast
        .iter()
        .take(max_nodes_cap)
        .map(|e| {
            let idx = e.node.as_usize();
            let (label, node_type) = if idx < graph.num_nodes() as usize {
                (
                    graph.strings.resolve(graph.nodes.label[idx]).to_string(),
                    format!("{:?}", graph.nodes.node_type[idx]),
                )
            } else {
                (format!("node_{}", idx), "Unknown".into())
            };
            BlastRadiusEntry {
                node_id: label.clone(),
                label,
                node_type,
                signal_strength: e.signal_strength.get(),
                hop_distance: e.hop_distance,
            }
        })
        .collect();
    let truncated = total_blast_nodes > max_nodes_cap;

    let causal_chains: Vec<CausalChainOutput> = chains
        .iter()
        .map(|c| {
            let path: Vec<String> = c
                .path
                .iter()
                .map(|&n| {
                    let idx = n.as_usize();
                    if idx < graph.num_nodes() as usize {
                        graph.strings.resolve(graph.nodes.label[idx]).to_string()
                    } else {
                        format!("node_{}", idx)
                    }
                })
                .collect();
            let relations: Vec<String> = c
                .relations
                .iter()
                .map(|&r| graph.strings.resolve(r).to_string())
                .collect();
            CausalChainOutput {
                path,
                relations,
                cumulative_strength: c.cumulative_strength.get(),
            }
        })
        .collect();

    Ok(ImpactOutput {
        source: input.node_id,
        source_label,
        direction: input.direction,
        blast_radius,
        total_energy: impact.total_energy.get(),
        max_hops_reached: impact.max_hops_reached,
        causal_chains,
        proof_state: impact_projection
            .as_ref()
            .map(|projection| projection.proof_state.clone())
            .unwrap_or_else(|| "triaging".into()),
        next_suggested_tool: impact_projection
            .as_ref()
            .and_then(|projection| projection.next_suggested_tool.clone()),
        next_suggested_target: sorted_blast
            .first()
            .map(|entry| {
                let idx = entry.node.as_usize();
                if idx < graph.num_nodes() as usize {
                    graph.strings.resolve(graph.nodes.label[idx]).to_string()
                } else {
                    format!("node_{}", idx)
                }
            })
            .or_else(|| {
                impact_projection
                    .as_ref()
                    .and_then(|projection| projection.next_suggested_target.clone())
            }),
        next_step_hint: impact_projection
            .as_ref()
            .and_then(|projection| projection.next_step_hint.clone()),
        total_blast_nodes,
        truncated,
    })
}

/// Handle m1nd.missing (03-MCP Section 2.3).
/// Replaces: ConnectomeEngine.query() + StructuralHoleDetector.detect()
pub fn handle_missing(
    state: &mut SessionState,
    input: MissingInput,
) -> M1ndResult<serde_json::Value> {
    let config = QueryConfig {
        query: input.query.clone(),
        agent_id: input.agent_id.clone(),
        top_k: 20,
        xlr_enabled: true,
        include_ghost_edges: false,
        include_structural_holes: true,
        ..QueryConfig::default()
    };

    let result = {
        let mut graph = state.graph.write();
        state.orchestrator.query(&mut graph, &config)?
    };

    let graph = state.graph.read();

    let holes: Vec<serde_json::Value> = result
        .structural_holes
        .iter()
        .map(|sh| {
            let idx = sh.node.as_usize();
            let label = if idx < graph.num_nodes() as usize {
                graph.strings.resolve(graph.nodes.label[idx]).to_string()
            } else {
                format!("node_{}", idx)
            };
            serde_json::json!({
                "node_id": label,
                "sibling_avg_activation": sh.sibling_avg_activation.get(),
                "reason": sh.reason,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "query": input.query,
        "structural_holes": holes,
        "ghost_edges": result.ghost_edges.len(),
    }))
}

/// Handle m1nd.why (03-MCP Section 2.4).
/// Replaces: bidirectional BFS + DimensionResult.paths + CommunityDetector
pub fn handle_why(state: &mut SessionState, input: WhyInput) -> M1ndResult<serde_json::Value> {
    let graph = state.graph.read();

    let source = graph.resolve_id(&input.source);
    let target = graph.resolve_id(&input.target);

    let (source_node, target_node) = match (source, target) {
        (Some(s), Some(t)) => (s, t),
        _ => {
            return Ok(serde_json::json!({
                "source": input.source,
                "target": input.target,
                "paths": [],
                "reason": "One or both nodes not found",
            }));
        }
    };

    // BFS from source to target (max_hops)
    let n = graph.num_nodes() as usize;
    let max_hops = input.max_hops as usize;
    let mut parent: Vec<Option<(usize, usize)>> = vec![None; n]; // (prev_node, edge_idx)
    let mut visited = vec![false; n];
    let mut queue = std::collections::VecDeque::new();

    visited[source_node.as_usize()] = true;
    queue.push_back((source_node, 0usize));

    let mut found = false;
    while let Some((node, depth)) = queue.pop_front() {
        if node == target_node {
            found = true;
            break;
        }
        if depth >= max_hops {
            continue;
        }
        // Forward edges
        let range = graph.csr.out_range(node);
        for j in range {
            let tgt = graph.csr.targets[j];
            let tgt_idx = tgt.as_usize();
            if tgt_idx < n && !visited[tgt_idx] {
                visited[tgt_idx] = true;
                parent[tgt_idx] = Some((node.as_usize(), j));
                queue.push_back((tgt, depth + 1));
            }
        }
        // Reverse edges (traverse incoming edges for full bidirectional BFS)
        let rev_range = graph.csr.in_range(node);
        for j in rev_range {
            let src = graph.csr.rev_sources[j];
            let src_idx = src.as_usize();
            let fwd_edge = graph.csr.rev_edge_idx[j].as_usize();
            if src_idx < n && !visited[src_idx] {
                visited[src_idx] = true;
                parent[src_idx] = Some((node.as_usize(), fwd_edge));
                queue.push_back((src, depth + 1));
            }
        }
    }

    let mut paths = Vec::new();
    if found {
        // Reconstruct path
        let mut path_nodes = vec![target_node.as_usize()];
        let mut path_relations = Vec::new();
        let mut current = target_node.as_usize();
        while let Some((prev, edge_j)) = parent[current] {
            path_nodes.push(prev);
            let rel = graph
                .strings
                .resolve(graph.csr.relations[edge_j])
                .to_string();
            path_relations.push(rel);
            current = prev;
            if current == source_node.as_usize() {
                break;
            }
        }
        path_nodes.reverse();
        path_relations.reverse();

        let path_labels: Vec<String> = path_nodes
            .iter()
            .map(|&i| {
                if i < graph.num_nodes() as usize {
                    graph.strings.resolve(graph.nodes.label[i]).to_string()
                } else {
                    format!("node_{}", i)
                }
            })
            .collect();

        paths.push(serde_json::json!({
            "nodes": path_labels,
            "relations": path_relations,
            "hops": path_labels.len() - 1,
        }));
    }

    // Check community membership
    let same_community = {
        let communities = state.topology.community_detector.detect(&graph);
        match communities {
            Ok(c) => {
                let s = source_node.as_usize();
                let t = target_node.as_usize();
                if s < c.assignments.len() && t < c.assignments.len() {
                    c.assignments[s] == c.assignments[t]
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    };

    Ok(serde_json::json!({
        "source": input.source,
        "target": input.target,
        "paths": paths,
        "same_community": same_community,
        "found": found,
    }))
}

/// Handle m1nd.warmup (03-MCP Section 2.5).
/// Replaces: SeedFinder.find_seeds() + QueryMemory.get_priming_signal()
pub fn handle_warmup(
    state: &mut SessionState,
    input: WarmupInput,
) -> M1ndResult<serde_json::Value> {
    let graph = state.graph.read();

    // Find seeds related to the task description
    let seeds = m1nd_core::seed::SeedFinder::find_seeds_semantic(
        &graph,
        &state.orchestrator.semantic,
        &input.task_description,
        50,
    )?;

    let seed_nodes: Vec<NodeId> = seeds.iter().map(|s| s.0).collect();

    // Get priming signal from plasticity memory
    let priming = state
        .plasticity
        .get_priming(&seed_nodes, FiniteF32::new(input.boost_strength));

    let seed_output: Vec<serde_json::Value> = seeds
        .iter()
        .take(20)
        .map(|&(node, relevance)| {
            let idx = node.as_usize();
            let label = if idx < graph.num_nodes() as usize {
                graph.strings.resolve(graph.nodes.label[idx]).to_string()
            } else {
                format!("node_{}", idx)
            };
            serde_json::json!({
                "node_id": label,
                "relevance": relevance.get(),
            })
        })
        .collect();
    let seed_count = seed_output.len();

    let priming_output: Vec<serde_json::Value> = priming
        .iter()
        .take(20)
        .map(|&(node, strength)| {
            let idx = node.as_usize();
            let label = if idx < graph.num_nodes() as usize {
                graph.strings.resolve(graph.nodes.label[idx]).to_string()
            } else {
                format!("node_{}", idx)
            };
            serde_json::json!({
                "node_id": label,
                "priming_strength": strength.get(),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "task_description": input.task_description,
        "seeds": seed_output,
        "priming_nodes": priming_output,
        "total_seeds": seed_count,
        "total_priming": priming.len(),
    }))
}

/// Handle m1nd.counterfactual (03-MCP Section 2.6).
/// Replaces: NodeRemovalSimulator + CascadeAnalyzer + WhatIfSimulator
pub fn handle_counterfactual(
    state: &mut SessionState,
    input: CounterfactualInput,
) -> M1ndResult<serde_json::Value> {
    let graph = state.graph.read();

    let remove_nodes: Vec<NodeId> = input
        .node_ids
        .iter()
        .filter_map(|id| graph.resolve_id(id))
        .collect();

    if remove_nodes.is_empty() {
        return Ok(serde_json::json!({
            "error": "No valid node IDs found",
            "node_ids": input.node_ids,
        }));
    }

    let config = PropagationConfig::default();

    // Combined removal (all nodes at once)
    let result = state.counterfactual.simulate_removal(
        &graph,
        &state.orchestrator.engine,
        &config,
        &remove_nodes,
    )?;

    // Cascade analysis for first node
    let cascade = if input.include_cascade && !remove_nodes.is_empty() {
        let c = state.counterfactual.cascade_analysis(
            &graph,
            &state.orchestrator.engine,
            &config,
            remove_nodes[0],
        )?;
        Some(serde_json::json!({
            "cascade_depth": c.cascade_depth,
            "total_affected": c.total_affected,
            "affected_by_depth": c.affected_by_depth.iter().map(|d| d.len()).collect::<Vec<_>>(),
        }))
    } else {
        None
    };

    // Synergy analysis: only when multiple nodes are removed.
    // Compares combined impact vs sum of individual impacts.
    //   synergy_factor > 1.0 → synergistic (together worse than sum of parts)
    //   synergy_factor < 1.0 → redundant (together less bad than sum of parts)
    //   synergy_factor ≈ 1.0 → independent
    let synergy = if remove_nodes.len() > 1 {
        let mut individual_impacts: Vec<serde_json::Value> = Vec::new();
        let mut sum_individual: f32 = 0.0;

        for &node in &remove_nodes {
            let individual = state.counterfactual.simulate_removal(
                &graph,
                &state.orchestrator.engine,
                &config,
                &[node],
            )?;
            let pct_lost = individual.pct_activation_lost.get();
            sum_individual += pct_lost;

            let idx = node.as_usize();
            let label = if idx < graph.num_nodes() as usize {
                graph.strings.resolve(graph.nodes.label[idx]).to_string()
            } else {
                format!("node_{}", idx)
            };
            individual_impacts.push(serde_json::json!({
                "node_id": label,
                "pct_activation_lost": pct_lost,
            }));
        }

        let combined_impact = result.pct_activation_lost.get();
        let synergy_factor = if sum_individual > 0.0 {
            combined_impact / sum_individual
        } else {
            1.0
        };

        Some(serde_json::json!({
            "individual_impacts": individual_impacts,
            "combined_impact": combined_impact,
            "synergy_factor": synergy_factor,
        }))
    } else {
        None
    };

    Ok(serde_json::json!({
        "removed_nodes": input.node_ids,
        "total_impact": result.total_impact.get(),
        "pct_activation_lost": result.pct_activation_lost.get(),
        "orphaned_count": result.orphaned_nodes.len(),
        "weakened_count": result.weakened_nodes.len(),
        "reachability_before": result.reachability_before,
        "reachability_after": result.reachability_after,
        "cascade": cascade,
        "synergy": synergy,
    }))
}

/// Handle m1nd.predict (03-MCP Section 2.7).
/// Replaces: CoChangeMatrix.predict() + VelocityScorer.score()
pub fn handle_predict(
    state: &mut SessionState,
    input: PredictInput,
) -> M1ndResult<serde_json::Value> {
    let graph = state.graph.read();

    let node = match graph.resolve_id(&input.changed_node) {
        Some(n) => n,
        None => {
            return Ok(serde_json::json!({
                "error": "Node not found",
                "changed_node": input.changed_node,
            }));
        }
    };

    let mut node_to_ext: Vec<String> = vec![String::new(); graph.num_nodes() as usize];
    for (interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < node_to_ext.len() {
            node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
        }
    }

    let co_change_predictions = state.temporal.co_change.predict(node, input.top_k);

    // --- Git-derived co-change fallback (Fix 3) ---
    // ghost_edges writes real git co-change into state.orchestrator.temporal.co_change.
    // If the bootstrap matrix has no entry for this node, merge git-derived entries.
    // Apply min_co_change_count filter (default 2, ~strength >= 0.2) to suppress
    // one-off coincidental pairs.
    let min_strength = {
        let count = input.min_co_change_count.unwrap_or(2).max(1);
        // Each co-change observation adds 0.1 to strength starting from 0.1,
        // so N observations ≈ strength 0.1 * N (capped at 1.0).
        (count as f32 * 0.1).min(1.0)
    };
    let git_co_change_predictions: Vec<m1nd_core::temporal::CoChangeEntry> = {
        let git_preds = state
            .orchestrator
            .temporal
            .co_change
            .predict(node, input.top_k);
        git_preds
            .into_iter()
            .filter(|e| e.strength.get() >= min_strength)
            .collect()
    };

    // Merge: bootstrap predictions take precedence (higher credibility).
    // Git-derived entries fill in missing slots.
    let co_change_predictions: Vec<m1nd_core::temporal::CoChangeEntry> = {
        let mut merged = co_change_predictions;
        let already_seen: HashSet<NodeId> = merged.iter().map(|e| e.target).collect();
        for entry in git_co_change_predictions {
            if !already_seen.contains(&entry.target) {
                merged.push(entry);
            }
        }
        merged.sort_by_key(|e| std::cmp::Reverse(e.strength));
        merged.truncate(input.top_k);
        merged
    };
    let co_change_count = co_change_predictions.len();

    // --- Structural fallback (Issue 3) ---
    // If co-change returns fewer than top_k results, supplement with
    // structural predictions: nodes connected via imports/calls/references
    // edges, scored by edge weight.  Co-change results rank higher.
    let mut seen: HashSet<NodeId> = co_change_predictions.iter().map(|p| p.target).collect();

    let mut structural_predictions: Vec<m1nd_core::temporal::CoChangeEntry> = Vec::new();

    if co_change_predictions.len() < input.top_k {
        let structural_relations: Vec<&str> = vec!["imports", "calls", "references"];
        let structural_interned: Vec<InternedStr> = structural_relations
            .iter()
            .filter_map(|r| {
                // Only match if the string is already interned (don't create it)
                graph.strings.lookup(r)
            })
            .collect();

        let range = graph.csr.out_range(node);
        for k in range {
            let target = graph.csr.targets[k];
            if target == node || seen.contains(&target) {
                continue;
            }
            let rel = graph.csr.relations[k];
            if structural_interned.contains(&rel) {
                let weight = graph.csr.read_weight(EdgeIdx::new(k as u32));
                structural_predictions.push(m1nd_core::temporal::CoChangeEntry {
                    target,
                    strength: weight,
                });
                seen.insert(target);
            }
        }

        // Also check incoming edges (reverse CSR) — if X imports this node,
        // X is likely impacted by changes here.
        let rev_range = graph.csr.in_range(node);
        for k in rev_range {
            let source = graph.csr.rev_sources[k];
            if source == node || seen.contains(&source) {
                continue;
            }
            let fwd_idx = graph.csr.rev_edge_idx[k];
            let rel = graph.csr.relations[fwd_idx.as_usize()];
            if structural_interned.contains(&rel) {
                let weight = graph.csr.read_weight(fwd_idx);
                structural_predictions.push(m1nd_core::temporal::CoChangeEntry {
                    target: source,
                    strength: weight,
                });
                seen.insert(source);
            }
        }

        // Sort structural by weight descending
        structural_predictions.sort_by_key(|entry| std::cmp::Reverse(entry.strength));
    }

    let structural_fallback_count = structural_predictions.len();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let mut ranked_predictions: Vec<RankedPrediction> = co_change_predictions
        .iter()
        .map(|entry| (PredictionSourceKind::CoChange, entry))
        .chain(
            structural_predictions
                .iter()
                .map(|entry| (PredictionSourceKind::StructuralFallback, entry)),
        )
        .map(|(source, entry)| {
            let idx = entry.target.as_usize();
            let label = if idx < graph.num_nodes() as usize {
                graph.strings.resolve(graph.nodes.label[idx]).to_string()
            } else {
                format!("node_{}", idx)
            };
            let stable_external_id = node_to_ext.get(idx).cloned().unwrap_or_default();
            let external_id = if stable_external_id.is_empty() {
                label.clone()
            } else {
                stable_external_id.clone()
            };
            let file_path = if idx < graph.num_nodes() as usize {
                graph
                    .resolve_node_provenance(entry.target)
                    .source_path
                    .or_else(|| {
                        external_id
                            .strip_prefix("file::")
                            .map(|value| value.to_string())
                    })
                    .unwrap_or_else(|| external_id.clone())
            } else {
                external_id.clone()
            };

            let trust = state.trust_ledger.compute_trust(&external_id, now);
            let raw_trust_factor = if stable_external_id.is_empty() {
                1.0
            } else {
                state.trust_ledger.adjust_prior(
                    1.0,
                    std::slice::from_ref(&stable_external_id),
                    false,
                    now,
                )
            };
            let trust_factor = dampened_trust_factor(raw_trust_factor);

            let tremor_observation_count = if stable_external_id.is_empty() {
                0
            } else {
                state.tremor_registry.observation_count(&stable_external_id)
            };
            let tremor_alert = if stable_external_id.is_empty() || tremor_observation_count < 3 {
                None
            } else {
                state
                    .tremor_registry
                    .analyze(
                        m1nd_core::tremor::TremorWindow::All,
                        0.0,
                        1,
                        Some(stable_external_id.as_str()),
                        now,
                        0,
                    )
                    .tremors
                    .into_iter()
                    .next()
            };
            let tremor_factor = dampened_tremor_factor(tremor_alert.as_ref());

            let heuristic_factor = source.score_bias() * trust_factor * tremor_factor;
            let coupling_strength = entry.strength.get();
            let final_score = (coupling_strength.max(0.0) * heuristic_factor).max(0.0);
            let reason = build_prediction_reason(
                source,
                trust_factor,
                tremor_factor,
                tremor_observation_count,
            );

            RankedPrediction {
                target: entry.target,
                external_id,
                label,
                file_path,
                source,
                coupling_strength,
                confidence: final_score.clamp(0.0, 1.0),
                final_score,
                heuristic_factor,
                trust_score: trust.trust_score,
                trust_risk_multiplier: trust.risk_multiplier,
                trust_tier: format!("{:?}", trust.tier),
                tremor_magnitude: tremor_alert.as_ref().map(|alert| alert.magnitude),
                tremor_observation_count,
                tremor_risk_level: tremor_alert
                    .as_ref()
                    .map(|alert| format!("{:?}", alert.risk_level)),
                reason,
            }
        })
        .collect();

    ranked_predictions.sort_by(|a, b| {
        b.final_score
            .partial_cmp(&a.final_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.coupling_strength.total_cmp(&a.coupling_strength))
            .then_with(|| a.external_id.cmp(&b.external_id))
    });
    ranked_predictions.truncate(input.top_k);

    let velocity = if input.include_velocity {
        let v = m1nd_core::temporal::VelocityScorer::score_one(&graph, node, now)?;
        Some(serde_json::json!({
            "velocity": v.velocity.get(),
            "trend": format!("{:?}", v.trend),
        }))
    } else {
        None
    };

    let prediction_output: Vec<serde_json::Value> = ranked_predictions
        .iter()
        .map(|prediction| {
            serde_json::json!({
                "node_id": prediction.external_id,
                "label": prediction.label,
                "source": prediction.source.as_str(),
                "coupling_strength": prediction.coupling_strength,
                "confidence": prediction.confidence,
                "heuristic_factor": prediction.heuristic_factor,
                "trust_score": prediction.trust_score,
                "trust_risk_multiplier": prediction.trust_risk_multiplier,
                "trust_tier": prediction.trust_tier,
                "tremor_magnitude": prediction.tremor_magnitude,
                "tremor_observation_count": prediction.tremor_observation_count,
                "tremor_risk_level": prediction.tremor_risk_level,
                "reason": prediction.reason,
                "heuristics_surface_ref": {
                    "node_id": prediction.external_id,
                    "file_path": prediction.file_path,
                },
            })
        })
        .collect();

    Ok(serde_json::json!({
        "changed_node": input.changed_node,
        "predictions": prediction_output,
        "co_change_count": co_change_count,
        "structural_fallback_count": structural_fallback_count,
        "heuristic_reranked": true,
        "velocity": velocity,
    }))
}

/// Handle m1nd.fingerprint (03-MCP Section 2.8).
/// Replaces: ActivationFingerprinter.compute_fingerprints() + find_equivalents()
pub fn handle_fingerprint(
    state: &mut SessionState,
    input: FingerprintInput,
) -> M1ndResult<serde_json::Value> {
    let graph = state.graph.read();

    // Generate probe queries from probe_queries or use defaults
    let probe_seeds: Vec<Vec<(NodeId, FiniteF32)>> = match &input.probe_queries {
        Some(queries) => queries
            .iter()
            .filter_map(|q| {
                let seeds = m1nd_core::seed::SeedFinder::find_seeds(&graph, q, 5).ok()?;
                if seeds.is_empty() {
                    None
                } else {
                    Some(seeds)
                }
            })
            .collect(),
        None => {
            // Default: use a few deterministic probes
            let n = graph.num_nodes();
            (0..5.min(n))
                .map(|i| vec![(NodeId::new(i), FiniteF32::ONE)])
                .collect()
        }
    };

    if probe_seeds.is_empty() {
        return Ok(serde_json::json!({
            "error": "No valid probe queries could be resolved",
        }));
    }

    let fingerprints = state.topology.fingerprinter.compute_fingerprints(
        &graph,
        &state.orchestrator.engine,
        &probe_seeds,
    )?;

    let result = if let Some(ref target_id) = input.target_node {
        // Find equivalents of a specific node
        match graph.resolve_id(target_id) {
            Some(target) => {
                let pairs = state.topology.fingerprinter.find_equivalents_of(
                    target,
                    &fingerprints,
                    &graph,
                )?;
                let equivalents: Vec<serde_json::Value> = pairs
                    .iter()
                    .map(|p| {
                        let idx_b = p.node_b.as_usize();
                        let label = if idx_b < graph.num_nodes() as usize {
                            graph.strings.resolve(graph.nodes.label[idx_b]).to_string()
                        } else {
                            format!("node_{}", idx_b)
                        };
                        serde_json::json!({
                            "node_id": label,
                            "cosine_similarity": p.cosine_similarity.get(),
                            "directly_connected": p.directly_connected,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "target_node": target_id,
                    "equivalents": equivalents,
                })
            }
            None => serde_json::json!({
                "error": "Target node not found",
                "target_node": target_id,
            }),
        }
    } else {
        // Find all equivalent pairs
        let pairs = state
            .topology
            .fingerprinter
            .find_equivalents(&fingerprints, &graph)?;
        let output: Vec<serde_json::Value> = pairs
            .iter()
            .take(20)
            .map(|p| {
                let idx_a = p.node_a.as_usize();
                let idx_b = p.node_b.as_usize();
                let label_a = if idx_a < graph.num_nodes() as usize {
                    graph.strings.resolve(graph.nodes.label[idx_a]).to_string()
                } else {
                    format!("node_{}", idx_a)
                };
                let label_b = if idx_b < graph.num_nodes() as usize {
                    graph.strings.resolve(graph.nodes.label[idx_b]).to_string()
                } else {
                    format!("node_{}", idx_b)
                };
                serde_json::json!({
                    "node_a": label_a,
                    "node_b": label_b,
                    "cosine_similarity": p.cosine_similarity.get(),
                    "directly_connected": p.directly_connected,
                })
            })
            .collect();
        serde_json::json!({
            "equivalent_pairs": output,
            "total_pairs": pairs.len(),
        })
    };

    Ok(result)
}

/// Handle m1nd.drift (03-MCP Section 2.9).
/// Replaces: PlasticityEngine state diff + CommunityDetector + VelocityScorer
pub fn handle_drift(state: &mut SessionState, input: DriftInput) -> M1ndResult<serde_json::Value> {
    let graph = state.graph.read();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    // Weight drift: find edges whose current weight differs most from baseline.
    // If since == "last_session" and a plasticity state file exists, use it as baseline.
    // Otherwise, fall back to original_weight comparison.
    let weight_drift = if input.include_weight_drift {
        // Try to load saved plasticity state as baseline
        let baseline_map: Option<std::collections::HashMap<(String, String, String), f32>> =
            if input.since == "last_session" {
                let state_path = std::path::Path::new("plasticity_state.json");
                match m1nd_core::snapshot::load_plasticity_state(state_path) {
                    Ok(states) => {
                        let mut map = std::collections::HashMap::new();
                        for s in &states {
                            map.insert(
                                (
                                    s.source_label.clone(),
                                    s.target_label.clone(),
                                    s.relation.clone(),
                                ),
                                s.current_weight,
                            );
                        }
                        Some(map)
                    }
                    Err(_) => None, // file missing or corrupt — fall back
                }
            } else {
                None
            };

        let num_edges = graph.edge_plasticity.original_weight.len();
        let num_nodes = graph.num_nodes() as usize;

        // Build edge_source map: edge_idx → source node index (from CSR offsets)
        let num_csr = graph.csr.num_edges();
        let mut edge_source = vec![0usize; num_csr];
        for i in 0..num_nodes {
            let lo = graph.csr.offsets[i] as usize;
            let hi = graph.csr.offsets[i + 1] as usize;
            for item in edge_source.iter_mut().take(hi).skip(lo) {
                *item = i;
            }
        }

        // Build node_ext_id: NodeId → external id string
        let mut node_ext_id = vec![String::new(); num_nodes];
        for (&interned, &node_id) in &graph.id_to_node {
            if node_id.as_usize() < num_nodes {
                node_ext_id[node_id.as_usize()] = graph.strings.resolve(interned).to_string();
            }
        }

        let cap = num_edges.min(num_csr);
        let mut drifts: Vec<(usize, f32, f32, f32)> = (0..cap)
            .filter_map(|j| {
                let curr = graph.edge_plasticity.current_weight[j].get();

                let baseline_weight = if let Some(ref bmap) = baseline_map {
                    let src_idx = edge_source[j];
                    let tgt_idx = graph.csr.targets[j].as_usize();
                    let src_label = if src_idx < num_nodes {
                        &node_ext_id[src_idx]
                    } else {
                        return None;
                    };
                    let tgt_label = if tgt_idx < num_nodes {
                        &node_ext_id[tgt_idx]
                    } else {
                        return None;
                    };
                    let rel = graph
                        .strings
                        .try_resolve(graph.csr.relations[j])
                        .unwrap_or("edge")
                        .to_string();
                    let key = (src_label.clone(), tgt_label.clone(), rel);
                    *bmap
                        .get(&key)
                        .unwrap_or(&graph.edge_plasticity.original_weight[j].get())
                } else {
                    graph.edge_plasticity.original_weight[j].get()
                };

                let delta = (curr - baseline_weight).abs();
                if delta > 0.001 {
                    Some((j, delta, baseline_weight, curr))
                } else {
                    None
                }
            })
            .collect();
        drifts.sort_by(|a, b| b.1.total_cmp(&a.1));
        drifts.truncate(20);

        let drift_output: Vec<serde_json::Value> = drifts
            .iter()
            .map(|&(j, delta, baseline, curr)| {
                serde_json::json!({
                    "edge_idx": j,
                    "baseline_weight": baseline,
                    "current_weight": curr,
                    "delta": delta,
                })
            })
            .collect();
        Some(drift_output)
    } else {
        None
    };

    // Velocity analysis
    let velocities = m1nd_core::temporal::VelocityScorer::score_all(&graph, now)?;
    let top_velocities: Vec<serde_json::Value> = velocities
        .iter()
        .take(10)
        .map(|v| {
            let idx = v.node.as_usize();
            let label = if idx < graph.num_nodes() as usize {
                graph.strings.resolve(graph.nodes.label[idx]).to_string()
            } else {
                format!("node_{}", idx)
            };
            serde_json::json!({
                "node_id": label,
                "velocity": v.velocity.get(),
                "trend": format!("{:?}", v.trend),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "since": input.since,
        "queries_processed": state.queries_processed,
        "weight_drift": weight_drift,
        "top_velocities": top_velocities,
        "uptime_seconds": state.uptime_seconds(),
    }))
}

/// Handle m1nd.learn (03-MCP Section 2.10).
/// Replaces: targeted edge strengthen/weaken bypass of Hebbian cycle
pub fn handle_learn(state: &mut SessionState, input: LearnInput) -> M1ndResult<serde_json::Value> {
    let mut graph = state.graph.write();

    let mut seen_nodes = HashSet::new();
    let resolved_nodes: Vec<(NodeId, String)> = input
        .node_ids
        .iter()
        .filter_map(|id| {
            let node = graph.resolve_id(id)?;
            if seen_nodes.insert(node) {
                Some((node, id.clone()))
            } else {
                None
            }
        })
        .collect();
    let nodes: Vec<NodeId> = resolved_nodes.iter().map(|(node, _)| *node).collect();

    if nodes.is_empty() {
        return Ok(serde_json::json!({
            "error": "No valid node IDs found",
            "node_ids": input.node_ids,
        }));
    }

    // Expand the node set to include direct children (outgoing "contains"
    // edges).  This ensures that learn(["file::a.rs", "file::b.rs"]) also
    // strengthens/weakens edges between functions/structs contained in those
    // files, which is where the actual cross-file relationships live.
    let mut expanded: Vec<NodeId> = nodes.clone();
    if let Some(contains_str) = graph.strings.lookup("contains") {
        for &node in &nodes {
            let range = graph.csr.out_range(node);
            for k in range {
                if graph.csr.relations[k] == contains_str {
                    let child = graph.csr.targets[k];
                    if !expanded.contains(&child) {
                        expanded.push(child);
                    }
                }
            }
        }
    }

    let strength = input.strength;
    let mut edges_modified = 0u32;
    let mut node_weight_deltas: HashMap<NodeId, f32> = HashMap::new();
    let mut node_edge_events: HashMap<NodeId, u16> = HashMap::new();

    // Determine which node pairs to strengthen/weaken based on feedback type.
    // "correct"  → strengthen edges between all given nodes (Hebbian: fire together, wire together)
    // "wrong"    → weaken edges between all given nodes
    // "partial"  → strengthen edges among first half, weaken edges between first half and rest
    //
    // Uses the expanded set (specified nodes + their children) so that
    // cross-file function/struct edges are included.
    #[allow(clippy::type_complexity)]
    let (strengthen_set, weaken_set): (Vec<(NodeId, NodeId)>, Vec<(NodeId, NodeId)>) =
        match input.feedback.as_str() {
            "correct" => {
                // Strengthen all pairs
                let mut pairs = Vec::new();
                for i in 0..expanded.len() {
                    for j in (i + 1)..expanded.len() {
                        pairs.push((expanded[i], expanded[j]));
                    }
                }
                (pairs, Vec::new())
            }
            "wrong" => {
                // Weaken all pairs
                let mut pairs = Vec::new();
                for i in 0..expanded.len() {
                    for j in (i + 1)..expanded.len() {
                        pairs.push((expanded[i], expanded[j]));
                    }
                }
                (Vec::new(), pairs)
            }
            "partial" => {
                let mid = expanded.len().div_ceil(2); // first half (rounded up)
                let first_half = &expanded[..mid];
                let rest = &expanded[mid..];
                // Strengthen edges among first half
                let mut s_pairs = Vec::new();
                for i in 0..first_half.len() {
                    for j in (i + 1)..first_half.len() {
                        s_pairs.push((first_half[i], first_half[j]));
                    }
                }
                // Weaken edges between first half and rest
                let mut w_pairs = Vec::new();
                for &a in first_half {
                    for &b in rest {
                        w_pairs.push((a, b));
                    }
                }
                (s_pairs, w_pairs)
            }
            _ => {
                // Unrecognized feedback — fall back to treating as "correct"
                let mut pairs = Vec::new();
                for i in 0..expanded.len() {
                    for j in (i + 1)..expanded.len() {
                        pairs.push((expanded[i], expanded[j]));
                    }
                }
                (pairs, Vec::new())
            }
        };

    // Helper closure: modify edge weight between src→tgt (if edge exists)
    let apply_delta =
        |graph: &mut m1nd_core::graph::Graph, src: NodeId, tgt: NodeId, delta: f32| -> u32 {
            let mut count = 0u32;
            let range = graph.csr.out_range(src);
            for k in range {
                if graph.csr.targets[k] == tgt {
                    let edge_idx = EdgeIdx::new(k as u32);
                    let current = graph.csr.read_weight(edge_idx).get();
                    let new_weight = (current + delta).clamp(0.05, 3.0);
                    let _ = graph
                        .csr
                        .atomic_write_weight(edge_idx, FiniteF32::new(new_weight), 64);
                    if k < graph.edge_plasticity.current_weight.len() {
                        graph.edge_plasticity.current_weight[k] = FiniteF32::new(new_weight);
                    }
                    count += 1;
                }
            }
            count
        };

    // Strengthen pairs
    for &(a, b) in &strengthen_set {
        let forward = apply_delta(&mut graph, a, b, strength);
        let reverse = apply_delta(&mut graph, b, a, strength);
        let edge_count = (forward + reverse).min(u16::MAX as u32) as u16;
        if edge_count > 0 {
            note_learn_node_effect(
                &mut node_weight_deltas,
                &mut node_edge_events,
                a,
                strength,
                edge_count,
            );
            note_learn_node_effect(
                &mut node_weight_deltas,
                &mut node_edge_events,
                b,
                strength,
                edge_count,
            );
        }
        edges_modified += forward + reverse;
    }

    // Weaken pairs
    for &(a, b) in &weaken_set {
        let forward = apply_delta(&mut graph, a, b, -strength);
        let reverse = apply_delta(&mut graph, b, a, -strength);
        let edge_count = (forward + reverse).min(u16::MAX as u32) as u16;
        if edge_count > 0 {
            note_learn_node_effect(
                &mut node_weight_deltas,
                &mut node_edge_events,
                a,
                -strength,
                edge_count,
            );
            note_learn_node_effect(
                &mut node_weight_deltas,
                &mut node_edge_events,
                b,
                -strength,
                edge_count,
            );
        }
        edges_modified += forward + reverse;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let auto_antibody = if input.feedback == "correct" && nodes.len() >= 2 {
        let antibody_name = format!("auto-learn-{}", now as u64);
        m1nd_core::antibody::extract_antibody_from_learn(
            &graph,
            &nodes,
            &antibody_name,
            &input.query,
            &input.agent_id,
        )
    } else {
        None
    };

    // Drop graph write-lock before accessing temporal (co_change needs &mut self)
    drop(graph);

    // Record co-change for all pairs of input nodes (feeds the predict tool).
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let _ = state
                .temporal
                .co_change
                .record_co_change(nodes[i], nodes[j], now);
            let _ = state
                .temporal
                .co_change
                .record_co_change(nodes[j], nodes[i], now);
        }
    }

    let mut tremor_observations_recorded = 0u32;
    for (node, external_id) in &resolved_nodes {
        match input.feedback.as_str() {
            "wrong" => state.trust_ledger.record_false_alarm(external_id, now),
            "partial" => state.trust_ledger.record_partial(external_id, now),
            _ => state.trust_ledger.record_defect(external_id, now),
        }

        let weight_delta = node_weight_deltas.get(node).copied().unwrap_or(0.0);
        let edge_events = node_edge_events.get(node).copied().unwrap_or(0);
        if edge_events > 0 || weight_delta.abs() > f32::EPSILON {
            state
                .tremor_registry
                .record_observation(external_id, weight_delta, edge_events, now);
            tremor_observations_recorded += 1;
        }
    }

    let antibody_added = auto_antibody
        .map(|candidate| maybe_store_auto_antibody(&mut state.antibodies, candidate))
        .unwrap_or(false);

    state.bump_plasticity_generation();
    state.invalidate_all_perspectives();
    state.mark_all_lock_baselines_stale();
    state.notify_watchers(crate::perspective::state::WatchTrigger::Learn);

    Ok(serde_json::json!({
        "query": input.query,
        "feedback": input.feedback,
        "nodes_found": nodes.len(),
        "nodes_expanded": expanded.len(),
        "edges_modified": edges_modified,
        "strength": strength,
        "trust_records_updated": resolved_nodes.len(),
        "tremor_observations_recorded": tremor_observations_recorded,
        "antibody_added": antibody_added,
    }))
}

/// Handle m1nd.ingest (03-MCP Section 2.11).
/// Replaces: CodebaseIngestor.ingest() / ingest_incremental()
pub fn handle_ingest(
    state: &mut SessionState,
    input: IngestInput,
) -> M1ndResult<serde_json::Value> {
    use m1nd_ingest::IngestAdapter;

    let path = std::path::PathBuf::from(&input.path);
    if input.incremental && input.adapter != "code" {
        return Ok(serde_json::json!({
            "error": "incremental ingest is only supported for adapter 'code'",
        }));
    }

    match input.adapter.as_str() {
        "code" => {
            // Existing code ingestion path (default)
            let config = m1nd_ingest::IngestConfig {
                root: path.clone(),
                include_dotfiles: input.include_dotfiles,
                dotfile_patterns: input.dotfile_patterns.clone(),
                ..m1nd_ingest::IngestConfig::default()
            };

            let ingestor = m1nd_ingest::Ingestor::new(config);

            let (new_graph, stats) = ingestor.ingest()?;
            finalize_ingest(state, &input, "code", new_graph, stats)
        }
        "json" => {
            // JSON descriptor adapter -- domain-agnostic ingestion
            let adapter = m1nd_ingest::json_adapter::JsonIngestAdapter;
            let (new_graph, stats) = adapter.ingest(&path)?;
            finalize_ingest(state, &input, "json", new_graph, stats)
        }
        "memory" => {
            let adapter =
                m1nd_ingest::memory_adapter::MemoryIngestAdapter::new(input.namespace.clone());
            let (new_graph, stats) = adapter.ingest(&path)?;
            finalize_ingest(state, &input, "memory", new_graph, stats)
        }
        "light" => {
            let adapter = m1nd_ingest::L1ghtIngestAdapter::new(input.namespace.clone());
            let (new_graph, stats) = adapter.ingest(&path)?;
            finalize_ingest(state, &input, "light", new_graph, stats)
        }
        "patent" => {
            let adapter = m1nd_ingest::PatentIngestAdapter::new(input.namespace.clone());
            let (new_graph, stats) = adapter.ingest(&path)?;
            finalize_ingest(state, &input, "patent", new_graph, stats)
        }
        "article" | "jats" => {
            let adapter = m1nd_ingest::JatsArticleAdapter::new(input.namespace.clone());
            let (new_graph, stats) = adapter.ingest(&path)?;
            finalize_ingest(state, &input, "article", new_graph, stats)
        }
        "bibtex" | "bib" => {
            let adapter = m1nd_ingest::BibTexAdapter::new(input.namespace.clone());
            let (new_graph, stats) = adapter.ingest(&path)?;
            finalize_ingest(state, &input, "bibtex", new_graph, stats)
        }
        "rfc" => {
            let adapter = m1nd_ingest::RfcAdapter::new(input.namespace.clone());
            let (new_graph, stats) = adapter.ingest(&path)?;
            finalize_ingest(state, &input, "rfc", new_graph, stats)
        }
        "crossref" | "doi" => {
            let adapter = m1nd_ingest::CrossRefAdapter::new(input.namespace.clone());
            let (new_graph, stats) = adapter.ingest(&path)?;
            finalize_ingest(state, &input, "crossref", new_graph, stats)
        }
        "universal" => {
            let namespace = input
                .namespace
                .clone()
                .unwrap_or_else(|| "universal".to_string());
            let adapter = m1nd_ingest::UniversalIngestAdapter::new(Some(namespace.clone()));
            let bundle = adapter.ingest_bundle(&path)?;
            let artifacts = universal_docs::write_canonical_artifacts_with_source_root(
                &state.runtime_root,
                Some(&path),
                &bundle.documents,
                &namespace,
            )?;
            universal_docs::ensure_cache_root_in_ingest_roots(state);
            let mut graph = bundle.graph;
            universal_docs::rewrite_graph_provenance_to_canonical(
                &mut graph,
                &artifacts.entries,
                &namespace,
            );
            for entry in artifacts.entries {
                state
                    .document_cache
                    .entries
                    .insert(entry.source_path.clone(), entry);
            }
            let mut output = finalize_ingest(state, &input, "universal", graph, bundle.stats)?;
            if let Some(obj) = output.as_object_mut() {
                obj.insert(
                    "canonical_artifact_count".into(),
                    serde_json::json!(state.document_cache.entries.len()),
                );
                let providers = universal_docs::provider_availability();
                obj.insert(
                    "provider_status".into(),
                    serde_json::to_value(providers).unwrap_or(serde_json::json!({})),
                );
            }
            Ok(output)
        }
        "auto" | "document" => {
            // Auto-detect format from file content
            let (format, adapter) =
                m1nd_ingest::document_router::DocumentRouter::detect_directory(&path);
            match adapter {
                Some(adapter) => {
                    let (new_graph, stats) = adapter.ingest(&path)?;
                    finalize_ingest(state, &input, &format.to_string(), new_graph, stats)
                }
                None => {
                    // Fallback to code adapter
                    let config = m1nd_ingest::IngestConfig {
                        root: path.clone(),
                        include_dotfiles: input.include_dotfiles,
                        dotfile_patterns: input.dotfile_patterns.clone(),
                        ..m1nd_ingest::IngestConfig::default()
                    };
                    let ingestor = m1nd_ingest::Ingestor::new(config);
                    let (new_graph, stats) = ingestor.ingest()?;
                    finalize_ingest(state, &input, "code", new_graph, stats)
                }
            }
        }
        other => Ok(serde_json::json!({
            "error": format!("Unknown adapter: '{}'. Supported: 'code', 'json', 'memory', 'light', 'patent', 'article', 'bibtex', 'rfc', 'crossref', 'universal', 'auto'", other),
        })),
    }
}

/// Handle m1nd.resonate — resonance analysis via ResonanceEngine.
/// Exposes harmonics, sympathetic pairs, and resonant frequencies.
pub fn handle_resonate(
    state: &mut SessionState,
    input: ResonateInput,
) -> M1ndResult<serde_json::Value> {
    let graph = state.graph.read();

    // Resolve seeds: either from query or from a specific node_id
    let seeds: Vec<(NodeId, FiniteF32)> = if let Some(ref query) = input.query {
        m1nd_core::seed::SeedFinder::find_seeds(&graph, query, 50)?
    } else if let Some(ref nid) = input.node_id {
        match graph.resolve_id(nid) {
            Some(node) => vec![(node, FiniteF32::ONE)],
            None => {
                return Ok(serde_json::json!({
                    "error": "Node not found",
                    "node_id": nid,
                }));
            }
        }
    } else {
        return Ok(serde_json::json!({
            "error": "Either 'query' or 'node_id' must be provided",
        }));
    };

    if seeds.is_empty() {
        return Ok(serde_json::json!({
            "error": "No seed nodes found for the given input",
        }));
    }

    let report = state.resonance.analyze(&graph, &seeds)?;

    let top_k = input.top_k;

    // Map harmonic results
    let harmonics: Vec<serde_json::Value> = report
        .harmonics
        .harmonics
        .iter()
        .map(|hr| {
            let antinodes: Vec<serde_json::Value> = hr
                .antinodes
                .iter()
                .take(top_k)
                .map(|&(node, amp)| {
                    let idx = node.as_usize();
                    let label = if idx < graph.num_nodes() as usize {
                        graph.strings.resolve(graph.nodes.label[idx]).to_string()
                    } else {
                        format!("node_{}", idx)
                    };
                    serde_json::json!({
                        "node_id": label,
                        "amplitude": amp.get(),
                    })
                })
                .collect();
            serde_json::json!({
                "harmonic": hr.harmonic,
                "frequency": hr.frequency.get(),
                "total_energy": hr.total_energy.get(),
                "antinodes": antinodes,
            })
        })
        .collect();

    // Map sympathetic resonance pairs
    let sympathetic_pairs: Vec<serde_json::Value> = report
        .sympathetic
        .sympathetic_nodes
        .iter()
        .take(top_k)
        .map(|&(node, amp)| {
            let idx = node.as_usize();
            let label = if idx < graph.num_nodes() as usize {
                graph.strings.resolve(graph.nodes.label[idx]).to_string()
            } else {
                format!("node_{}", idx)
            };
            serde_json::json!({
                "node_id": label,
                "resonance_amplitude": amp.get(),
            })
        })
        .collect();

    // Map resonant frequencies
    let resonant_frequencies: Vec<serde_json::Value> = report
        .resonant_frequencies
        .iter()
        .map(|rf| {
            serde_json::json!({
                "frequency": rf.frequency.get(),
                "total_energy": rf.total_energy.get(),
            })
        })
        .collect();

    // Standing wave summary
    let wave_pattern = serde_json::json!({
        "total_energy": report.standing_wave.total_energy.get(),
        "pulses_processed": report.standing_wave.pulses_processed,
        "antinode_count": report.standing_wave.antinodes.len(),
        "wave_node_count": report.standing_wave.wave_nodes.len(),
    });

    Ok(serde_json::json!({
        "harmonics": harmonics,
        "sympathetic_pairs": sympathetic_pairs,
        "resonant_frequencies": resonant_frequencies,
        "wave_pattern": wave_pattern,
        "harmonic_groups": report.harmonics.harmonic_groups.len(),
    }))
}

/// Handle m1nd.health (03-MCP Section 2.12).
pub fn handle_health(state: &mut SessionState, _input: HealthInput) -> M1ndResult<HealthOutput> {
    let graph = state.graph.read();
    let node_count = graph.num_nodes();
    let edge_count = graph.num_edges() as u64;
    let plasticity_edge_count = graph.edge_plasticity.original_weight.len();
    drop(graph);

    let last_persist = state
        .last_persist_time
        .map(|t| format!("{:.0}s ago", t.elapsed().as_secs_f64()));
    // Use the full registry count for the contract (all handlers exist regardless of tier).
    // The advertised count reflects only what tools/list currently exposes.
    let tool_schema_full = crate::server::all_tool_schemas();
    let full_registry_tool_count = tool_schema_full
        .get("tools")
        .and_then(|tools| tools.as_array())
        .map(|tools| tools.len() as u64)
        .unwrap_or(0);
    let tool_schema_advertised = crate::server::tool_schemas();
    let advertised_tool_count = tool_schema_advertised
        .get("tools")
        .and_then(|tools| tools.as_array())
        .map(|tools| tools.len() as u64)
        .unwrap_or(0);
    let tool_tier = crate::server::active_tool_tier();

    Ok(HealthOutput {
        status: "ok".into(),
        node_count,
        edge_count,
        queries_processed: state.queries_processed,
        uptime_seconds: state.uptime_seconds(),
        memory_usage_bytes: 0, // simplified -- would need jemalloc stats
        plasticity_state: format!("{} edges tracked", plasticity_edge_count),
        last_persist_time: last_persist,
        active_sessions: state.session_summary(),
        git: crate::audit_handlers::collect_git_state(state, 20),
        binding_fingerprint: state.binding_fingerprint(),
        tool_surface_contract: serde_json::json!({
            "schema": "m1nd-tool-surface-contract-v0",
            // full_registry_tool_count: all handlers that exist in the binary
            "full_registry_tool_count": full_registry_tool_count,
            // advertised_tool_count: what tools/list currently exposes (tier-dependent)
            "advertised_tool_count": advertised_tool_count,
            // tool_tier: "essential" (default, ~25 tools) or "full" (all tools).
            // Set M1ND_TOOL_TIER=full to expose all 102 tools in tools/list.
            // Hidden tools remain callable via tools/call dispatch at all times.
            "tool_tier": tool_tier,
            "required_agent_trust_tools": AGENT_TRUST_REQUIRED_TOOLS,
            "required_host_visible_tools": HOST_BINDING_REQUIRED_TOOLS,
            // minimum_safe_tool_count is now based on required tools, not total count,
            // so tiering does not falsely trigger "degraded" state.
            "minimum_safe_tool_count": HOST_BINDING_REQUIRED_TOOLS.len() as u64,
            "degraded_if_missing_any": HOST_BINDING_REQUIRED_TOOLS,
            "recovery_tool": "recovery_playbook",
            "diagnostic_tool": "doctor",
        }),
        host_binding_alignment: serde_json::json!({
            "schema": "m1nd-host-binding-alignment-v0",
            "status": "needs_client_surface_comparison",
            "rule": "Compare the host-visible m1nd tool names and count against tool_surface_contract. If trust_selftest, session_handshake, or recovery_playbook is missing, treat this host binding as degraded_host_tool_surface even when health responds.",
            "current_runtime_has_graph": node_count > 0 && edge_count > 0,
            "next_action": "Call trust_selftest with observed_tool_count and available_tools when visible; otherwise use session_handshake, local repo smoke, or refresh the MCP host binding.",
            "smoke_commands": [
                "python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --json",
                "python3 scripts/mcp_agent_smoke.py --repo . --transport http --handshake-only --json"
            ],
            "non_claims": [
                "health cannot see which subset of tools the client host injected",
                "health does not rebind the host or refresh tool schemas automatically"
            ],
        }),
    })
}

/// Handle m1nd.session_handshake.
///
/// The handshake is intentionally cheap: it inspects the host tool surface and
/// active graph state, then returns an operational trust verdict. It does not
/// ingest, mutate the graph, or run retrieval probes.
pub fn handle_session_handshake(
    state: &mut SessionState,
    input: SessionHandshakeInput,
) -> M1ndResult<serde_json::Value> {
    let mut available_tools = input.available_tools.clone();
    available_tools.sort();
    available_tools.dedup();
    let host_surface_names_observed =
        !available_tools.is_empty() || !input.missing_tools.is_empty();

    if !host_surface_names_observed {
        available_tools = AGENT_TRUST_REQUIRED_TOOLS
            .iter()
            .map(|tool| (*tool).to_string())
            .collect();
        available_tools.push("session_handshake".into());
        available_tools.push("recovery_playbook".into());
        available_tools.sort();
        available_tools.dedup();
    }

    let available_tool_set: HashSet<_> = available_tools.iter().cloned().collect();
    let mut missing_tools = input.missing_tools.clone();
    for tool in AGENT_TRUST_REQUIRED_TOOLS {
        if !available_tool_set.contains(tool) {
            missing_tools.push(tool.to_string());
        }
    }
    missing_tools.sort();
    missing_tools.dedup();

    let degraded_host_tool_surface = !missing_tools.is_empty();
    let can_ingest = available_tool_set.contains("ingest");
    let can_retrieve = available_tool_set.contains("seek");
    let can_recover = available_tool_set.contains("recovery_playbook");
    let can_diagnose = available_tool_set.contains("doctor");
    let workspace_binding_mismatch = state.workspace_binding_mismatch(input.scope.as_deref());
    let wrong_workspace_binding = workspace_binding_mismatch.is_some();

    let graph = state.graph.read();
    let node_count = graph.num_nodes();
    let edge_count = graph.num_edges() as u64;
    let graph_finalized = graph.finalized;
    drop(graph);

    let (trust_mode, next_action) = if degraded_host_tool_surface {
        (
            "degraded_host_tool_surface",
            "treat m1nd as orientation only, refresh the MCP binding, and verify final truth with local files",
        )
    } else if wrong_workspace_binding {
        (
            "wrong_workspace_binding",
            "select, bind, ingest, or federate the requested workspace before trusting scoped retrieval",
        )
    } else if node_count == 0 || edge_count == 0 {
        if can_ingest {
            (
                "needs_ingest",
                "run ingest for the intended repo before trusting graph retrieval",
            )
        } else {
            (
                "orientation_only",
                "use m1nd only as orientation and verify final truth with local files until ingest is available",
            )
        }
    } else {
        (
            "full_trust",
            "continue with m1nd-first retrieval; use compiler/tests for runtime truth",
        )
    };

    let doctor_recovery = if degraded_host_tool_surface {
        Some(serde_json::json!({
            "suggested_tool": if can_recover { "recovery_playbook" } else if can_diagnose { "doctor" } else { "" },
            "arguments": {
                "agent_id": input.agent_id,
                "observed_tool": "tools/list",
                "observed_proof_state": "blocked",
                "observed_tool_count": input.observed_tool_count.unwrap_or(available_tools.len() as u64),
                "available_tools": available_tools.clone(),
                "missing_tools": missing_tools.clone(),
            },
            "fallback": "if doctor is unavailable, restart or rebind the MCP host surface and use direct repo reads for final truth",
        }))
    } else if let Some(mismatch) = workspace_binding_mismatch.clone() {
        Some(serde_json::json!({
            "suggested_tool": if can_recover { "recovery_playbook" } else if can_diagnose { "doctor" } else { "" },
            "arguments": {
                "agent_id": input.agent_id,
                "observed_tool": "scope_router",
                "observed_proof_state": "blocked",
                "observed_candidates": 0,
                "scope": input.scope,
                "workspace_binding_mismatch": mismatch,
            },
            "fallback": "if recovery tools are unavailable, rebind the MCP host with M1ND_WORKSPACE_ROOT set to the requested workspace",
        }))
    } else if node_count == 0 || edge_count == 0 {
        Some(serde_json::json!({
            "suggested_tool": if can_recover { "recovery_playbook" } else if can_diagnose { "doctor" } else { "" },
            "arguments": {
                "agent_id": input.agent_id,
                "observed_tool": "health",
                "observed_proof_state": "blocked",
                "observed_candidates": 0,
            },
        }))
    } else {
        None
    };

    Ok(serde_json::json!({
        "schema": "m1nd-session-handshake-v0",
        "trust_mode": trust_mode,
        "binding_fingerprint": state.binding_fingerprint(),
        "can_ingest": can_ingest,
        "can_retrieve": can_retrieve,
        "can_recover": can_recover,
        "next_action": next_action,
        "tool_surface": {
            "status": if degraded_host_tool_surface { "degraded_host_tool_surface" } else { "ok" },
            "tool_count": input.observed_tool_count.unwrap_or(available_tools.len() as u64),
            "required_tools": AGENT_TRUST_REQUIRED_TOOLS,
            "required_tools_present": {
                "health": available_tool_set.contains("health"),
                "trust_selftest": available_tool_set.contains("trust_selftest"),
                "recovery_playbook": can_recover,
                "doctor": can_diagnose,
                "ingest": can_ingest,
                "seek": can_retrieve,
                "help": available_tool_set.contains("help"),
            },
            "missing_required_tools": missing_tools,
            "available_tools_sample": available_tools.iter().take(24).cloned().collect::<Vec<_>>(),
            "degraded_host_tool_surface": degraded_host_tool_surface,
        },
        "health": {
            "status": "ok",
            "node_count": node_count,
            "edge_count": edge_count,
            "queries_processed": state.queries_processed,
            "active_session_count": state.sessions.len(),
            "graph_finalized": graph_finalized,
        },
        "graph_state": state.mini_graph_state(),
        "context_guard": {
            "schema": "m1nd-context-guard-v0",
            "wrong_workspace_binding": wrong_workspace_binding,
            "workspace_binding_mismatch": workspace_binding_mismatch,
        },
        "doctor_recovery": doctor_recovery,
        "used_probe": false,
        "probe": serde_json::Value::Null,
    }))
}

/// Handle m1nd.trust_selftest.
///
/// The selftest is a one-call diagnostic verdict for agents. It composes the
/// current binding fingerprint, host-visible tool evidence, graph state,
/// session handshake, and recovery playbook when needed. It does not ingest,
/// mutate, repair, or probe retrieval on its own.
pub fn handle_trust_selftest(
    state: &mut SessionState,
    input: TrustSelftestInput,
) -> M1ndResult<serde_json::Value> {
    let agent_id = input.agent_id.clone();
    let observed_blocked = input.observed_proof_state.as_deref() == Some("blocked");
    let suspicious_retrieval = observed_blocked;

    let handshake = handle_session_handshake(
        state,
        SessionHandshakeInput {
            agent_id: agent_id.clone(),
            observed_tool_count: input.observed_tool_count,
            available_tools: input.available_tools.clone(),
            missing_tools: input.missing_tools.clone(),
            scope: input.scope.clone(),
        },
    )?;

    let handshake_trust_mode = handshake
        .get("trust_mode")
        .and_then(|value| value.as_str())
        .unwrap_or("orientation_only");
    let graph_state = state.graph_runtime_summary();
    let graph_has_nodes = graph_state
        .get("node_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
        > 0
        && graph_state
            .get("edge_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0;

    let verdict = match handshake_trust_mode {
        "degraded_host_tool_surface" => "degraded_host_tool_surface",
        "wrong_workspace_binding" => "wrong_workspace_binding",
        "needs_ingest" => "needs_ingest",
        "orientation_only" => "orientation_only",
        "full_trust" if graph_has_nodes && suspicious_retrieval => "stale_binding_suspected",
        "full_trust" => "full_trust",
        other if graph_has_nodes && suspicious_retrieval => {
            if other == "full_trust" {
                "stale_binding_suspected"
            } else {
                other
            }
        }
        other => other,
    }
    .to_string();

    let status = match verdict.as_str() {
        "full_trust" => "ok",
        "needs_ingest" | "wrong_workspace_binding" => "blocked",
        _ => "warn",
    };
    let ok = verdict == "full_trust";

    let recovery_playbook = if !ok || suspicious_retrieval {
        Some(handle_recovery_playbook(
            state,
            RecoveryPlaybookInput {
                agent_id: agent_id.clone(),
                trust_mode: Some(verdict.clone()),
                observed_tool: input.observed_tool.clone(),
                observed_proof_state: input.observed_proof_state.clone(),
                observed_candidates: input.observed_candidates,
                observed_tool_count: input.observed_tool_count,
                available_tools: input.available_tools.clone(),
                missing_tools: input.missing_tools.clone(),
                scope: input.scope.clone(),
                error_text: input.error_text.clone(),
            },
        )?)
    } else {
        None
    };

    let default_next_action = if ok {
        "proceed_with_m1nd_first"
    } else {
        "inspect_trust_selftest_verdict"
    };
    let next_action = recovery_playbook
        .as_ref()
        .and_then(|playbook| playbook.get("next_action"))
        .and_then(|value| value.as_str())
        .unwrap_or(default_next_action);

    Ok(serde_json::json!({
        "schema": "m1nd-trust-selftest-v0",
        "ok": ok,
        "status": status,
        "verdict": verdict,
        "next_action": next_action,
        "binding_fingerprint": state.binding_fingerprint(),
        "graph_state": graph_state,
        "session_handshake": handshake,
        "recovery_playbook": recovery_playbook.unwrap_or(serde_json::Value::Null),
        "checks": {
            "binding_fingerprint_present": true,
            "graph_populated": graph_has_nodes,
            "host_surface_complete": verdict != "degraded_host_tool_surface",
            "needs_ingest": verdict == "needs_ingest",
            "wrong_workspace_binding": verdict == "wrong_workspace_binding",
            "stale_binding_suspected": verdict == "stale_binding_suspected",
            "suspicious_retrieval_evidence": suspicious_retrieval,
            "recovery_playbook_attached": !ok || suspicious_retrieval,
        },
        "non_claims": [
            "trust_selftest does not ingest or mutate the graph.",
            "trust_selftest does not refresh the host MCP binding.",
            "trust_selftest does not run a retrieval probe automatically.",
            "trust_selftest does not replace compiler, tests, or local file truth."
        ],
    }))
}

/// Handle m1nd.recovery_playbook.
///
/// The recovery playbook is diagnostic-only. It inspects current runtime state
/// plus caller-provided host evidence, then returns a deterministic sequence of
/// next actions without mutating the graph or probing the filesystem.
pub fn handle_recovery_playbook(
    state: &mut SessionState,
    input: RecoveryPlaybookInput,
) -> M1ndResult<serde_json::Value> {
    let agent_id = input.agent_id.clone();
    let input_trust_mode = input.trust_mode.clone();
    let handshake = handle_session_handshake(
        state,
        SessionHandshakeInput {
            agent_id: agent_id.clone(),
            observed_tool_count: input.observed_tool_count,
            available_tools: input.available_tools.clone(),
            missing_tools: input.missing_tools.clone(),
            scope: input.scope.clone(),
        },
    )?;

    let graph = state.graph.read();
    let graph_has_nodes = graph.num_nodes() > 0;
    drop(graph);

    let observed_blocked = input.observed_proof_state.as_deref() == Some("blocked");
    let stale_binding_suspected = graph_has_nodes && observed_blocked;
    let workspace_binding_mismatch = state.workspace_binding_mismatch(input.scope.as_deref());
    let wrong_workspace_binding = workspace_binding_mismatch.is_some();

    let handshake_trust_mode = handshake
        .get("trust_mode")
        .and_then(|value| value.as_str())
        .unwrap_or("orientation_only");
    let trust_mode = match handshake_trust_mode {
        "degraded_host_tool_surface" => "degraded_host_tool_surface",
        _ if wrong_workspace_binding => "wrong_workspace_binding",
        "needs_ingest" => "needs_ingest",
        "orientation_only" => "orientation_only",
        "full_trust" if stale_binding_suspected => "stale_binding_suspected",
        "full_trust" => "full_trust",
        _ if stale_binding_suspected => "stale_binding_suspected",
        _ => handshake_trust_mode,
    };

    let can_diagnose = handshake
        .pointer("/tool_surface/required_tools_present/doctor")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let handshake_doctor_arguments = handshake
        .get("doctor_recovery")
        .and_then(|value| value.get("arguments"))
        .cloned();
    let observed_tool = input
        .observed_tool
        .clone()
        .unwrap_or_else(|| "seek".to_string());
    let observed_proof_state = input
        .observed_proof_state
        .clone()
        .unwrap_or_else(|| "blocked".to_string());
    let stale_doctor_arguments = state
        .doctor_recovery_payload(
            &agent_id,
            &observed_tool,
            &observed_proof_state,
            input.observed_candidates,
            input.scope.as_deref(),
            input.error_text.as_deref(),
        )
        .get("arguments")
        .cloned();
    let ingest_path = input
        .scope
        .clone()
        .or_else(|| state.workspace_root.clone())
        .unwrap_or_else(|| "<intended-repo-path>".to_string());

    let (status, recovery_goal, next_action, steps) = match trust_mode {
        "wrong_workspace_binding" => {
            let mismatch = workspace_binding_mismatch.clone().unwrap_or_else(|| {
                serde_json::json!({
                    "schema": "m1nd-workspace-binding-mismatch-v0",
                    "code": "wrong_workspace_binding"
                })
            });
            let requested_workspace_hint = mismatch
                .get("requested_workspace_hint")
                .and_then(|value| value.as_str())
                .unwrap_or("<requested-workspace-path>")
                .to_string();
            (
                "blocked",
                "Select or bind the requested workspace before trusting scoped retrieval.",
                "select_or_bind_workspace",
                vec![
                    playbook_step(
                        "inspect_context_guard",
                        "Read workspace_binding_mismatch and confirm requested_scope, active_workspace_root, and requested_workspace_hint.",
                        "The active graph can be healthy while the requested absolute scope belongs to another repository.",
                        None,
                        Some(mismatch.clone()),
                    ),
                    playbook_step(
                        "rebind_with_workspace_root",
                        "Restart or rebind the MCP host with M1ND_WORKSPACE_ROOT set to requested_workspace_hint.",
                        "An explicit workspace root is the safest host-neutral way to make the next binding load the intended project.",
                        None,
                        Some(serde_json::json!({
                            "env": {
                                "M1ND_WORKSPACE_ROOT": requested_workspace_hint.clone(),
                            }
                        })),
                    ),
                    playbook_step(
                        "same_binding_ingest_if_intentional",
                        "If this session should intentionally switch or merge context, call ingest for requested_workspace_hint on this same binding.",
                        "This is an explicit mutation; do it only when the agent truly wants this runtime to carry that repo context.",
                        Some("ingest"),
                        Some(serde_json::json!({
                            "agent_id": agent_id.clone(),
                            "path": requested_workspace_hint,
                        })),
                    ),
                    playbook_step(
                        "cross_repo_mode_if_needed",
                        "Use federate_auto or federate when the task genuinely needs multiple repositories at once.",
                        "Federation is for cross-repo reasoning; it is not a substitute for selecting the correct active workspace.",
                        Some("federate_auto"),
                        Some(serde_json::json!({
                            "agent_id": agent_id.clone(),
                            "execute": false,
                        })),
                    ),
                    playbook_step(
                        "fallback_local_file_truth",
                        "Use direct file reads and focused tests while the workspace binding remains unresolved.",
                        "This playbook never changes workspace, ingests, federates, or mutates files by itself.",
                        None,
                        None,
                    ),
                ],
            )
        }
        "degraded_host_tool_surface" => {
            let mut steps = vec![
                playbook_step(
                    "refresh_host_binding",
                    "Refresh or rebind the MCP host surface so the full m1nd recovery namespace is exposed.",
                    "The current host surface is missing required tools, so this binding cannot complete its own recovery loop.",
                    None,
                    None,
                ),
                playbook_step(
                    "rerun_tools_list",
                    "Rerun tools/list and capture available_tools plus missing_tools from the host surface.",
                    "A raw tool count is not enough to prove which recovery capabilities are actually available.",
                    None,
                    None,
                ),
            ];
            if can_diagnose {
                steps.push(playbook_step(
                    "call_doctor",
                    "Call doctor with the degraded host evidence.",
                    "Doctor will confirm whether the missing surface is host-only or reflects a wider runtime mismatch.",
                    Some("doctor"),
                    handshake_doctor_arguments.clone(),
                ));
            }
            steps.push(playbook_step(
                "rerun_session_handshake",
                "Call session_handshake again after the host surface is rebound.",
                "The handshake should move from degraded_host_tool_surface to either needs_ingest or full_trust before m1nd retrieval is trusted again.",
                Some("session_handshake"),
                Some(serde_json::json!({ "agent_id": agent_id.clone() })),
            ));
            steps.push(playbook_step(
                "use_local_file_truth",
                "Use local file reads, compiler output, and tests for final truth until the host surface is repaired.",
                "This playbook does not auto-repair, auto-ingest, or mutate the filesystem.",
                None,
                None,
            ));
            (
                "warn",
                "Restore a complete host-bound m1nd tool surface before trusting graph recovery.",
                "refresh_host_binding",
                steps,
            )
        }
        "needs_ingest" => (
            "blocked",
            "Populate this binding's active graph for the intended repository.",
            "call_ingest",
            vec![
                playbook_step(
                    "call_ingest",
                    "Call ingest for the intended repository on this same binding.",
                    "The active graph is empty or incomplete, so retrieval cannot yet be trusted.",
                    Some("ingest"),
                    Some(serde_json::json!({
                        "agent_id": agent_id.clone(),
                        "path": ingest_path,
                    })),
                ),
                playbook_step(
                    "rerun_session_handshake",
                    "Call session_handshake again after ingest completes.",
                    "The handshake should confirm node and edge counts before the next retrieval step.",
                    Some("session_handshake"),
                    Some(serde_json::json!({ "agent_id": agent_id.clone() })),
                ),
            ],
        ),
        "orientation_only" => (
            "warn",
            "Recover an ingest-capable binding or fall back to local file truth.",
            "refresh_binding_for_ingest",
            vec![
                playbook_step(
                    "refresh_binding_for_ingest",
                    "Refresh or rebind the host surface until ingest is available on this session.",
                    "The current binding can orient but cannot populate or refresh the graph state.",
                    None,
                    None,
                ),
                playbook_step(
                    "use_local_file_truth",
                    "Use local file reads and runtime truth while the host surface remains orientation-only.",
                    "Without ingest on this binding, m1nd cannot repair the trust gap from inside the current host session.",
                    None,
                    None,
                ),
                playbook_step(
                    "rerun_session_handshake",
                    "Call session_handshake after the binding exposes ingest.",
                    "The handshake will tell you whether the recovered binding still needs ingest or is ready for full trust.",
                    Some("session_handshake"),
                    Some(serde_json::json!({ "agent_id": agent_id.clone() })),
                ),
            ],
        ),
        "stale_binding_suspected" => (
            "warn",
            "Prove whether host, binary, runtime, or graph identity drift is causing split-brain retrieval.",
            "call_doctor",
            vec![
                playbook_step(
                    "call_doctor",
                    "Call doctor with the blocked retrieval observation.",
                    "Doctor will correlate the suspicious retrieval result with graph state, session continuity, and transport clues.",
                    Some("doctor"),
                    stale_doctor_arguments,
                ),
                playbook_step(
                    "compare_binding_fingerprint",
                    "Compare this binding_fingerprint with the host, repo-local stdio, and repo-local HTTP handshake outputs.",
                    "Matching process_id, current_exe, runtime_root, graph_path, and generation counters is the fastest way to prove or disprove split-brain binding drift.",
                    None,
                    None,
                ),
                playbook_step(
                    "run_stdio_smoke",
                    "Run `python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --json` and compare its trust_mode plus binding_fingerprint.",
                    "A repo-local stdio smoke checks the binary directly without the host MCP surface in the middle.",
                    None,
                    None,
                ),
                playbook_step(
                    "run_http_smoke",
                    "Run `python3 scripts/mcp_agent_smoke.py --repo . --transport http --handshake-only --json` and compare the same fingerprint fields.",
                    "A repo-local HTTP smoke helps separate transport-specific host issues from shared runtime identity.",
                    None,
                    None,
                ),
                playbook_step(
                    "fallback_local_file_truth",
                    "Use direct repo files and focused tests while the binding mismatch remains unresolved.",
                    "This playbook never performs an automatic repair, ingest, or retrieval probe on your behalf.",
                    None,
                    None,
                ),
            ],
        ),
        _ => (
            "ok",
            "Continue with m1nd-first retrieval on the current binding.",
            "proceed_with_m1nd_first",
            vec![playbook_step(
                "proceed_with_m1nd_first",
                "Proceed with m1nd-first retrieval such as seek, activate, or search on this binding.",
                "The current graph state and host surface do not show a recovery blocker.",
                None,
                None,
            )],
        ),
    };

    Ok(serde_json::json!({
        "schema": "m1nd-recovery-playbook-v0",
        "status": status,
        "trust_mode": trust_mode,
        "input_trust_mode": input_trust_mode,
        "binding_fingerprint": handshake.get("binding_fingerprint").cloned().unwrap_or_else(|| state.binding_fingerprint()),
        "graph_state": state.graph_runtime_summary(),
        "tool_surface": handshake.get("tool_surface").cloned().unwrap_or_else(|| serde_json::json!({})),
        "context_guard": {
            "schema": "m1nd-context-guard-v0",
            "wrong_workspace_binding": wrong_workspace_binding,
            "workspace_binding_mismatch": workspace_binding_mismatch,
        },
        "recovery_goal": recovery_goal,
        "steps": steps,
        "next_action": next_action,
        "non_claims": [
            "No automatic repair was performed.",
            "No ingest or graph mutation was performed.",
            "No retrieval probe or filesystem mutation was performed.",
            "This playbook is derived only from current session state and caller-supplied host evidence."
        ],
    }))
}

/// Handle m1nd.doctor.
///
/// Doctor is intentionally diagnostic only: it reports the active graph,
/// runtime, session, and likely recovery path without mutating the graph.
pub fn handle_doctor(
    state: &mut SessionState,
    input: DoctorInput,
) -> M1ndResult<serde_json::Value> {
    let graph = state.graph.read();
    let node_count = graph.num_nodes();
    let edge_count = graph.num_edges() as u64;
    let graph_finalized = graph.finalized;
    drop(graph);

    let observed_tool = input
        .observed_tool
        .clone()
        .unwrap_or_else(|| "unknown".into());
    let observed_proof_state = input.observed_proof_state.clone();
    let observed_candidates = input.observed_candidates;
    let observed_tool_count = input.observed_tool_count;
    let workspace_binding_mismatch = state.workspace_binding_mismatch(input.scope.as_deref());
    let wrong_workspace_binding = workspace_binding_mismatch.is_some();
    let mut available_tools = input.available_tools.clone();
    available_tools.sort();
    available_tools.dedup();
    let available_tool_set: std::collections::HashSet<_> =
        available_tools.iter().cloned().collect();
    let required_recovery_tools = ["ingest", "seek", "help", "doctor"];
    let mut missing_tools = input.missing_tools.clone();
    if !available_tools.is_empty() {
        for tool in required_recovery_tools {
            if !available_tool_set.contains(tool) {
                missing_tools.push(tool.to_string());
            }
        }
    }
    missing_tools.sort();
    missing_tools.dedup();
    let degraded_host_tool_surface = !missing_tools.is_empty();
    let observed_blocked = observed_proof_state.as_deref() == Some("blocked");
    let graph_has_nodes = node_count > 0;
    let has_ingest_roots = !state.ingest_roots.is_empty();
    let workspace_root_known = state.workspace_root.is_some();
    let agent_session = state.sessions.get(&input.agent_id);

    let mut warnings = Vec::new();
    let mut next_actions = Vec::new();
    let mut probable_causes = Vec::new();

    if !graph_has_nodes {
        warnings.push("active graph has zero nodes".to_string());
        probable_causes.push("ingest did not populate this active MCP session".to_string());
        probable_causes
            .push("the agent is attached to a different m1nd instance than expected".to_string());
        next_actions.push(
            "run ingest against the intended repository on this same tool binding".to_string(),
        );
        next_actions
            .push("call doctor again and confirm node_count is greater than zero".to_string());
    }

    if graph_has_nodes && observed_blocked {
        warnings.push(format!(
            "{} reported blocked retrieval while the active graph is populated",
            observed_tool
        ));
        probable_causes.push(
            "host MCP binding, transport, or agent session is pointed at stale state".to_string(),
        );
        probable_causes
            .push("scope/path normalization filtered out the intended graph region".to_string());
        next_actions.push(
            "verify the same binding with stdio and HTTP smokes before declaring the graph stale"
                .to_string(),
        );
        next_actions.push(
            "retry retrieval without scope, then with both absolute and repo-relative scope"
                .to_string(),
        );
    }

    if let Some(mismatch) = workspace_binding_mismatch.as_ref() {
        let requested_workspace_hint = mismatch
            .get("requested_workspace_hint")
            .and_then(|value| value.as_str())
            .unwrap_or("requested workspace");
        warnings.push(format!(
            "requested scope is outside the active workspace binding; requested workspace hint: {}",
            requested_workspace_hint
        ));
        probable_causes.push(
            "the agent is asking one repository's m1nd binding about another repository"
                .to_string(),
        );
        probable_causes.push(
            "a weak shell hint such as OLDPWD or a stale host environment selected the wrong workspace root".to_string(),
        );
        next_actions.push(
            "rebind the MCP host with M1ND_WORKSPACE_ROOT set to the requested workspace"
                .to_string(),
        );
        next_actions.push(
            "use federate_auto/federate only if the task truly requires cross-repo reasoning"
                .to_string(),
        );
    }

    if degraded_host_tool_surface {
        warnings.push(format!(
            "host tool surface is missing required m1nd tools: {}",
            missing_tools.join(", ")
        ));
        probable_causes
            .push("the MCP client injected a partial tool namespace or stale binding".to_string());
        probable_causes.push(
            "this agent may be seeing a different public tool surface than the local m1nd runtime"
                .to_string(),
        );
        next_actions.push(
            "treat m1nd as an orientation signal only until the tool surface is rebound"
                .to_string(),
        );
        next_actions.push(
            "use direct repo reads for final truth when ingest is unavailable on this host surface"
                .to_string(),
        );
        next_actions.push(
            "restart or refresh the MCP binding, then rerun tools/list and the repo-local smoke harness"
                .to_string(),
        );
    }

    if graph_has_nodes && !has_ingest_roots {
        warnings.push("graph is populated but ingest_roots are empty".to_string());
        probable_causes.push(
            "the graph was loaded from an older snapshot without ingest root sidecar state"
                .to_string(),
        );
        next_actions.push(
            "rerun ingest in replace or merge mode so workspace_root and ingest_roots are refreshed"
                .to_string(),
        );
    }

    if !workspace_root_known {
        warnings.push("workspace_root is unknown".to_string());
        next_actions.push(
            "ingest a repository path rather than only a standalone graph snapshot".to_string(),
        );
    }

    if agent_session.is_none() {
        warnings.push(format!(
            "agent session '{}' is not yet present in this runtime state",
            input.agent_id
        ));
        probable_causes.push(
            "this transport may not be tracking agent sessions before dispatch, or the agent_id changed"
                .to_string(),
        );
        next_actions.push("keep agent_id stable across the investigation".to_string());
    }

    if next_actions.is_empty() {
        next_actions.push(
            "continue with m1nd-first retrieval; use compiler/tests for runtime truth".to_string(),
        );
    }

    warnings.sort();
    warnings.dedup();
    probable_causes.sort();
    probable_causes.dedup();
    next_actions.sort();
    next_actions.dedup();

    let stale_binding_suspected = graph_has_nodes && observed_blocked;
    let status = if !graph_has_nodes || wrong_workspace_binding {
        "blocked"
    } else if degraded_host_tool_surface || !warnings.is_empty() {
        "warn"
    } else {
        "ok"
    };

    let recent_agent_queries: Vec<_> = state
        .query_log
        .iter()
        .rev()
        .filter(|entry| entry.agent_id == input.agent_id)
        .take(5)
        .cloned()
        .collect();

    let last_persist_secs_ago = state
        .last_persist_time
        .map(|last| last.elapsed().as_secs_f64());

    Ok(serde_json::json!({
        "schema": "m1nd-doctor-v0",
        "status": status,
        "agent_id": input.agent_id,
        "diagnostics": {
            "graph_has_nodes": graph_has_nodes,
            "graph_finalized": graph_finalized,
            "has_ingest_roots": has_ingest_roots,
            "workspace_root_known": workspace_root_known,
            "agent_session_known": agent_session.is_some(),
            "stale_binding_suspected": stale_binding_suspected,
            "degraded_host_tool_surface": degraded_host_tool_surface,
            "wrong_workspace_binding": wrong_workspace_binding,
        },
        "observed": {
            "tool": observed_tool,
            "proof_state": observed_proof_state,
            "candidates": observed_candidates,
            "tool_count": observed_tool_count,
            "scope": input.scope,
            "error_text": input.error_text,
        },
        "tool_surface": {
            "observed_tool_count": observed_tool_count,
            "available_tools_sample": available_tools.iter().take(24).cloned().collect::<Vec<_>>(),
            "missing_tools": missing_tools,
            "required_recovery_tools": ["ingest", "seek", "help", "doctor"],
            "degraded_host_tool_surface": degraded_host_tool_surface,
            "operator_rule": "if ingest is unavailable, m1nd cannot repair or refresh the active graph from inside this host session",
        },
        "graph_state": state.graph_runtime_summary(),
        "context_guard": {
            "schema": "m1nd-context-guard-v0",
            "wrong_workspace_binding": wrong_workspace_binding,
            "workspace_binding_mismatch": workspace_binding_mismatch,
        },
        "runtime_state": {
            "runtime_root": state.runtime_root.to_string_lossy(),
            "graph_path": state.graph_path.to_string_lossy(),
            "graph_path_exists": state.graph_path.exists(),
            "plasticity_path": state.plasticity_path.to_string_lossy(),
            "plasticity_path_exists": state.plasticity_path.exists(),
            "workspace_root": state.workspace_root,
            "ingest_roots": state.ingest_roots,
            "last_persist_secs_ago": last_persist_secs_ago,
            "instance": state.instance.summary(),
        },
        "session_state": {
            "active_agent_sessions": state.sessions.len(),
            "agent_session": agent_session.map(|session| serde_json::json!({
                "agent_id": session.agent_id,
                "first_seen_secs_ago": session.first_seen.elapsed().as_secs_f64(),
                "last_seen_secs_ago": session.last_seen.elapsed().as_secs_f64(),
                "query_count": session.query_count,
            })),
            "queries_processed": state.queries_processed,
            "recent_agent_queries": recent_agent_queries,
        },
        "transport_clues": {
            "doctor_is_transport_neutral": true,
            "split_brain_rule": "if repo-local stdio/http smokes pass but host MCP retrieval is blocked, suspect host binding or session split-brain before blaming the graph",
            "repo_local_smokes": [
                "python3 scripts/mcp_agent_smoke.py --repo . --json",
                "python3 scripts/mcp_agent_smoke.py --repo . --transport http --json"
            ],
        },
        "warnings": warnings,
        "probable_causes": probable_causes,
        "next_actions": next_actions,
        "non_claims": [
            "doctor does not repair or refresh the host MCP binding.",
            "doctor does not ingest, mutate, or repair graph contents.",
            "doctor does not prove semantic retrieval correctness.",
            "doctor does not replace compiler, test, log, or direct file truth."
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        handle_doctor, handle_recovery_playbook, handle_trust_selftest, AGENT_TRUST_REQUIRED_TOOLS,
        HOST_BINDING_REQUIRED_TOOLS,
    };
    use crate::protocol::{DoctorInput, RecoveryPlaybookInput, TrustSelftestInput};
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};

    fn build_runtime_state(root: &std::path::Path) -> SessionState {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };

        let mut graph = Graph::new();
        let a = graph
            .add_node("file::src/lib.rs", "lib.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add lib node");
        let b = graph
            .add_node(
                "file::src/core.rs",
                "core.rs",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add core node");
        graph
            .add_edge(
                a,
                b,
                "imports",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.8),
            )
            .expect("add edge");
        graph.finalize().expect("finalize graph");

        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());
        state
    }

    #[test]
    fn trust_selftest_keeps_zero_candidates_without_blocked_proof_in_full_trust() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());

        let output = handle_trust_selftest(
            &mut state,
            TrustSelftestInput {
                agent_id: "jimi".into(),
                observed_tool_count: Some(HOST_BINDING_REQUIRED_TOOLS.len() as u64),
                available_tools: HOST_BINDING_REQUIRED_TOOLS
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect(),
                missing_tools: vec![],
                observed_tool: Some("seek".into()),
                observed_proof_state: Some("triaging".into()),
                observed_candidates: Some(0),
                scope: None,
                error_text: None,
            },
        )
        .expect("trust selftest output");

        assert_eq!(output["verdict"], "full_trust");
        assert_eq!(output["status"], "ok");
        assert_eq!(output["checks"]["suspicious_retrieval_evidence"], false);
        assert_eq!(output["checks"]["recovery_playbook_attached"], false);
        assert_eq!(output["recovery_playbook"], serde_json::Value::Null);
    }

    #[test]
    fn recovery_playbook_keeps_zero_candidates_without_blocked_proof_in_full_trust() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());

        let output = handle_recovery_playbook(
            &mut state,
            RecoveryPlaybookInput {
                agent_id: "jimi".into(),
                trust_mode: Some("full_trust".into()),
                observed_tool: Some("seek".into()),
                observed_proof_state: Some("triaging".into()),
                observed_candidates: Some(0),
                observed_tool_count: Some(AGENT_TRUST_REQUIRED_TOOLS.len() as u64),
                available_tools: HOST_BINDING_REQUIRED_TOOLS
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect(),
                missing_tools: vec![],
                scope: None,
                error_text: None,
            },
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "full_trust");
        assert_eq!(output["status"], "ok");
        assert_eq!(output["next_action"], "proceed_with_m1nd_first");
        assert_eq!(output["steps"][0]["id"], "proceed_with_m1nd_first");
    }

    #[test]
    fn doctor_keeps_zero_candidates_without_blocked_proof_out_of_stale_bucket() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());
        state.track_agent("jimi");

        let output = handle_doctor(
            &mut state,
            DoctorInput {
                agent_id: "jimi".into(),
                observed_tool: Some("seek".into()),
                observed_proof_state: Some("triaging".into()),
                observed_candidates: Some(0),
                observed_tool_count: Some(HOST_BINDING_REQUIRED_TOOLS.len() as u64),
                available_tools: HOST_BINDING_REQUIRED_TOOLS
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect(),
                missing_tools: vec![],
                scope: None,
                error_text: None,
            },
        )
        .expect("doctor output");

        assert_eq!(output["status"], "ok");
        assert_eq!(
            output["diagnostics"]["stale_binding_suspected"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(output["warnings"], serde_json::json!([]));
        assert!(output["next_actions"][0]
            .as_str()
            .expect("next action")
            .contains("continue with m1nd-first retrieval"));
    }

    /// Build a star graph where `hub` has N outgoing edges to leaf nodes.
    /// Returns (state, hub_id_string).
    fn build_star_graph(root: &std::path::Path, leaf_count: usize) -> (SessionState, String) {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };

        let mut graph = Graph::new();
        let hub = graph
            .add_node("file::hub.rs", "hub.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add hub");

        for i in 0..leaf_count {
            let leaf_id = format!("file::leaf_{}.rs", i);
            let leaf_label = format!("leaf_{}.rs", i);
            let leaf = graph
                .add_node(&leaf_id, &leaf_label, NodeType::File, &[], 0.0, 0.0)
                .expect("add leaf");
            graph
                .add_edge(
                    hub,
                    leaf,
                    "imports",
                    FiniteF32::new(1.0),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.5),
                )
                .expect("add edge");
        }
        graph.finalize().expect("finalize");

        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());
        (state, "file::hub.rs".to_string())
    }

    #[test]
    fn impact_max_nodes_cap_is_honored() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut state, hub_id) = build_star_graph(temp.path(), 20);

        let output = super::handle_impact(
            &mut state,
            crate::protocol::core::ImpactInput {
                node_id: hub_id,
                agent_id: "test".into(),
                direction: "forward".into(),
                include_causal_chains: false,
                max_nodes: Some(5),
            },
        )
        .expect("impact should succeed");

        // blast_radius must be capped
        assert!(
            output.blast_radius.len() <= 5,
            "blast_radius capped to max_nodes=5, got {}",
            output.blast_radius.len()
        );
        // total_blast_nodes reflects pre-cap count
        assert!(
            output.total_blast_nodes >= output.blast_radius.len(),
            "total_blast_nodes ({}) >= blast_radius.len() ({})",
            output.total_blast_nodes,
            output.blast_radius.len()
        );
        // truncated flag must be set when cap was applied
        if output.total_blast_nodes > 5 {
            assert!(
                output.truncated,
                "truncated must be true when total_blast_nodes ({}) > max_nodes (5)",
                output.total_blast_nodes
            );
        }
    }

    /// Unit test for `resolve_light_evidence`.
    ///
    /// Builds a small graph with:
    ///   - a code file node  `file::auth.rs`
    ///   - an evidence marker node tagged `light:evidenced_by`, labelled
    ///     `"𝔻 evidence: auth.rs"`
    ///
    /// Asserts that after calling `resolve_light_evidence`:
    ///   - resolved == 1, unresolved == 0
    ///   - a `grounded_in` edge exists from the marker node to `file::auth.rs`
    ///   - calling again is idempotent (no duplicate edge, resolved still 1)
    #[test]
    fn resolve_light_evidence_adds_grounded_in_edge() {
        let mut graph = Graph::new();

        // Code file node
        let code_node = graph
            .add_node("file::auth.rs", "auth.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add code node");

        // Evidence marker node — tagged "light:evidenced_by", label is the raw marker text
        let marker_node = graph
            .add_node(
                "light::default::tag::auth_rs::1::evidence",
                "\u{1d53b} evidence: auth.rs",
                NodeType::Reference,
                &["light:evidenced_by"],
                0.0,
                0.0,
            )
            .expect("add marker node");

        // Graph is unfinalized after add_node; finalize so CSR is coherent before pass.
        // (resolve_light_evidence works on pending_edges for idempotency, but the
        //  node count/id_to_node must be consistent.)
        graph.finalize().expect("initial finalize");

        // Run the resolution pass
        let (resolved, unresolved) = super::resolve_light_evidence(&mut graph);

        assert_eq!(resolved, 1, "expected 1 resolved evidence link");
        assert_eq!(unresolved, 0, "expected 0 unresolved");

        // After the pass, finalized must be false so CSR gets rebuilt
        assert!(
            !graph.finalized,
            "graph.finalized must be false after edges were added"
        );

        // Finalize to build CSR and confirm the edge is reachable
        graph.finalize().expect("post-pass finalize");

        // Confirm `grounded_in` edge from marker → code in the CSR
        let grounded_in_interned = graph.strings.lookup("grounded_in").expect("relation interned");
        let marker_idx = marker_node.as_usize();
        let lo = graph.csr.offsets[marker_idx] as usize;
        let hi = graph.csr.offsets[marker_idx + 1] as usize;
        let found = (lo..hi).any(|i| {
            graph.csr.targets[i] == code_node && graph.csr.relations[i] == grounded_in_interned
        });
        assert!(
            found,
            "expected a grounded_in edge from marker node to file::auth.rs in the CSR"
        );

        // Idempotency: running the pass again must not add a duplicate edge
        let (resolved2, unresolved2) = super::resolve_light_evidence(&mut graph);
        // The existing edge is detected; resolved count still 1 (or 0 if we treat
        // already-existing as "not new"), but the critical invariant is no duplicate.
        // Our impl counts pre-existing pairs as resolved (idempotent skip path).
        let _ = (resolved2, unresolved2); // counts are informational; key check is edge count
        graph.finalize().expect("idempotency finalize");
        let lo2 = graph.csr.offsets[marker_idx] as usize;
        let hi2 = graph.csr.offsets[marker_idx + 1] as usize;
        let count = (lo2..hi2)
            .filter(|&i| {
                graph.csr.targets[i] == code_node
                    && graph.csr.relations[i] == grounded_in_interned
            })
            .count();
        assert_eq!(
            count, 1,
            "grounded_in edge must appear exactly once after idempotent second pass"
        );
    }

    /// Full-pipeline test: ingest real code, then merge a real L1GHT doc whose
    /// `[𝔻 evidence: auth.rs]` marker must resolve to the `file::auth.rs` code node
    /// through `handle_ingest` -> adapter -> merge -> finalize_ingest -> resolve_light_evidence.
    #[test]
    fn light_evidence_resolves_to_code_node_through_full_ingest() {
        use crate::protocol::core::IngestInput;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).expect("proj dir");

        // Real code file -> code node id becomes `file::auth.rs` (root-relative to proj).
        std::fs::write(
            proj.join("auth.rs"),
            "pub fn validate_token(t: &str) -> bool { !t.is_empty() }\n",
        )
        .expect("write auth.rs");

        // Real L1GHT doc citing auth.rs as evidence for the TokenValidator claim.
        std::fs::write(
            proj.join("notes.md"),
            "---\nProtocol: L1GHT/1.0\nNode: AuthNotes\n---\n\n## Auth\n\nThe [⍂ entity: TokenValidator] validates tokens.\n[𝔻 confidence: 0.7]\n[𝔻 evidence: auth.rs]\n",
        )
        .expect("write notes.md");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        let ingest_input = |path: String, adapter: &str, mode: &str| IngestInput {
            path,
            agent_id: "test".into(),
            incremental: false,
            adapter: adapter.into(),
            mode: mode.into(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: vec![],
        };

        // 1) Code ingest (replace) -> builds `file::auth.rs`.
        let code_out = super::handle_ingest(
            &mut state,
            ingest_input(proj.to_string_lossy().to_string(), "code", "replace"),
        )
        .expect("code ingest");
        assert!(
            code_out["node_count"].as_u64().unwrap_or(0) >= 1,
            "code ingest produced nodes"
        );

        // 2) Light ingest (merge) -> evidence marker must resolve to the code node.
        let light_out = super::handle_ingest(
            &mut state,
            ingest_input(
                proj.join("notes.md").to_string_lossy().to_string(),
                "light",
                "merge",
            ),
        )
        .expect("light ingest");

        assert!(
            light_out["light_evidence_resolved"].as_u64().unwrap_or(0) >= 1,
            "expected >=1 resolved evidence link through the full pipeline, got {:?}",
            light_out["light_evidence_resolved"]
        );

        // Confirm a grounded_in edge to file::auth.rs exists in the live graph.
        let graph = state.graph.read();
        let code_node = graph
            .resolve_id("file::auth.rs")
            .expect("file::auth.rs code node present after merge");
        let grounded = graph.strings.lookup("grounded_in").expect("grounded_in interned");
        let ci = code_node.as_usize();
        let lo = graph.csr.offsets[ci] as usize;
        let hi = graph.csr.offsets[ci + 1] as usize;
        // grounded_in is forward marker->code; check reverse adjacency if present,
        // otherwise scan all edges for one targeting the code node with that relation.
        let found = graph
            .csr
            .targets
            .iter()
            .zip(graph.csr.relations.iter())
            .any(|(&tgt, &rel)| tgt == code_node && rel == grounded);
        let _ = (lo, hi);
        assert!(
            found,
            "expected a grounded_in edge targeting file::auth.rs after full ingest"
        );
    }
}
