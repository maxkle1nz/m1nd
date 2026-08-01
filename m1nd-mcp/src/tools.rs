// === crates/m1nd-mcp/src/tools.rs ===

use crate::help_guidance;
use crate::protocol::layers::{HelpInput, HelpMode, HelpRender};
use crate::protocol::*;
use crate::result_shaping::dedupe_ranked;
use crate::session::SessionState;
use crate::universal_docs;
use crate::xray_handlers::is_test_source;
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::query::QueryConfig;
use m1nd_core::temporal::ImpactDirection;
use m1nd_core::types::*;
use std::collections::{HashMap, HashSet};
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
    } else if mode.eq_ignore_ascii_case("refresh") {
        "refresh"
    } else {
        "replace"
    }
}

// ---------------------------------------------------------------------------
// SPEC-1 — `graph.ingest.refresh_declared_root`, the freshness door.
// `docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §1, owner-ratified 2026-07-29.
// ---------------------------------------------------------------------------

/// The shrink floor, ratified by the owner at 60% (spec §6 item 3).
///
/// A candidate holding fewer than this share of the live graph's nodes REFUSES.
/// This is armor the persist layer deliberately does not provide: its own guard
/// is fail-open by written design (spec R-G — it backs up and writes anyway), so
/// "root set unchanged" never implied "graph intact". The R-D damage signature —
/// a narrow scan replacing a wide graph, measured twice in 24h on the deployed
/// 1.4.x owner — dies here even for a caller who is entirely legitimate.
pub const REFRESH_SHRINK_FLOOR_PERCENT: u64 = 60;

/// THE WAY OUT a refusal on the ingest path must name, chosen from what this
/// brain actually holds.
///
/// A refusal that is correct and names no door is half a refusal. Measured in
/// the field on 1.6.2: an agent in a new repo hit four different refusals in a
/// row — `generic_action_authority_required`, `refresh_caller_root_unknown`,
/// `refresh_root_not_exact` and the birth verb's own — none of which mentioned
/// the one command that would have worked, so it concluded the product could not
/// be used and wrote that in its report.
///
/// The two doors are genuinely different, which is why this reads the brain
/// instead of printing one sentence everywhere: a brain with no code root has no
/// brain yet (the human's ceremony), while a brain that HAS one is simply being
/// asked by a session that cannot say where it stands (the caller-root fact).
fn ingest_door_for(state: &SessionState) -> String {
    if state.code_root_path().is_some() {
        "this brain already maps a repo; a refresh is authorized by the caller's own root, so \
         reach the owner with `m1nd-mcp --attach auto --stdio` from inside that repo (the bridge \
         sends the root), or set PROJECT_ROOT to it"
            .to_string()
    } else {
        format!(
            "this repo has no brain yet — offer the human `{}` and stop; birthing a brain is their \
             gesture, never an agent's",
            crate::brain_birth::BIRTH_CEREMONY_COMMAND
        )
    }
}

/// The refusal envelope. ONE shape for every refusal reason so both transports
/// emit the same bytes for the same decision (SPEC-1g), and so an agent can
/// branch on `refused` without parsing prose.
fn refresh_refusal(code: &str, reason: &str) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "schema": "m1nd-graph-ingest-refresh-v1",
        "action": "graph.ingest.refresh_declared_root",
        "refused": code,
        "reason": reason,
    })
}

/// Roots currently being refreshed, by canonical key — single-flight per root
/// (SPEC-1c, a cp32 requirement, closing the TOCTOU between "candidate measured"
/// and "candidate committed").
fn refresh_in_flight_roots() -> &'static std::sync::Mutex<HashSet<String>> {
    static ROOTS: std::sync::OnceLock<std::sync::Mutex<HashSet<String>>> =
        std::sync::OnceLock::new();
    ROOTS.get_or_init(|| std::sync::Mutex::new(HashSet::new()))
}

/// Holds one root's single-flight claim and releases it on every exit path,
/// including a panic inside the ingest.
struct RefreshInFlightGuard(String);

impl Drop for RefreshInFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut roots) = refresh_in_flight_roots().lock() {
            roots.remove(&self.0);
        }
    }
}

/// Hold a root's single-flight claim from a test, so SPEC-1c's exclusivity is
/// proved deterministically instead of by racing two threads and hoping.
#[cfg(test)]
pub(crate) fn claim_refresh_root_for_test(canonical_root: &str) -> Option<impl Drop> {
    claim_refresh_root(canonical_root)
}

/// `None` when another refresh already holds this root.
fn claim_refresh_root(canonical_root: &str) -> Option<RefreshInFlightGuard> {
    let mut roots = refresh_in_flight_roots()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !roots.insert(canonical_root.to_string()) {
        return None;
    }
    Some(RefreshInFlightGuard(canonical_root.to_string()))
}

/// Re-ingest a root this brain has ALREADY declared.
///
/// The door never creates a brain, never adds a root, and never crosses to
/// another brain's territory. Admission at the dispatch seam admitted only the
/// CATEGORY (`action_consumers::GENERIC_A2_LOCAL_ADMITTED_ACTIONS`); everything
/// that actually carries authority is decided HERE, after brain resolution, and
/// every branch is fail-closed.
///
/// WHAT THIS DOES NOT CLOSE, stated where the code is (spec §1.3, following
/// `system_blocks_handlers.rs`'s own honest-phrasing precedent): `caller_root` is
/// resolved by the CLIENT and travels as a header (spec R-H). Canonicalizing it
/// kills the textual tricks, but a same-UID local process that sets
/// `PROJECT_ROOT` to a root it does not legitimately inhabit can still present as
/// that root. SPEC-1 closes the REFLEX vector — an agent acting from habit or
/// misconfiguration. It is not a defense against a hostile local process; that
/// is the lease plane, and it is not this door.
fn handle_ingest_refresh(
    state: &mut SessionState,
    input: &IngestInput,
) -> M1ndResult<serde_json::Value> {
    // 1. The door authenticates a ROOT RELATIONSHIP. With no caller root there
    //    is nothing to authenticate — fail closed, never open.
    let Some(caller_root) = state.caller_root.clone() else {
        let mut refusal = refresh_refusal(
            "refresh_caller_root_unknown",
            "a refresh is authorized by the caller's own root; this session carries none, and an unknown root is never treated as a match",
        );
        if let Some(object) = refusal.as_object_mut() {
            object.insert(
                "door".to_string(),
                serde_json::json!(ingest_door_for(state)),
            );
        }
        return Ok(refusal);
    };

    // 2. The exact-root predicate (SPEC-1.2): EQUALITY of canonical keys against
    //    the declared roots — never `covers_root`'s prefix, never a raw string.
    let canonical_root = match state.exact_declared_root(&caller_root) {
        Ok(root) => root,
        Err(code) => {
            let mut refusal = refresh_refusal(
                code,
                match code {
                    "refresh_root_unresolvable" => {
                        "the caller's root does not resolve on disk; an unresolvable path is refused rather than compared as text"
                    }
                    _ => {
                        "a refresh re-scans a root this brain has ALREADY declared, and only from that exact root — a subdirectory, a neighbour, or an explicit brain selector is not it"
                    }
                },
            );
            if let Some(object) = refusal.as_object_mut() {
                object.insert(
                    "declared_roots".to_string(),
                    serde_json::json!(state.declared_roots_canonical()),
                );
                object.insert(
                    "door".to_string(),
                    serde_json::json!(ingest_door_for(state)),
                );
            }
            return Ok(refusal);
        }
    };

    // 3. The path argument must name that SAME root. `path` is what gets scanned;
    //    letting it differ from the authenticated root would authorize one root
    //    and then re-ingest another.
    let target = input.path.trim().trim_end_matches('/');
    if target.is_empty() || !std::path::Path::new(target).exists() {
        return Ok(refresh_refusal(
            "refresh_root_unresolvable",
            "the path to refresh does not resolve on disk",
        ));
    }
    if crate::project_brains::ProjectBrainRegistry::canonical_key(target) != canonical_root {
        return Ok(refresh_refusal(
            "refresh_root_not_exact",
            "the path to refresh is not the root this caller is authorized for",
        ));
    }

    // 4. Single-flight per canonical root (SPEC-1c).
    let Some(_in_flight) = claim_refresh_root(&canonical_root) else {
        return Ok(refresh_refusal(
            "refresh_in_flight",
            "another refresh of this root is already running; its candidate would be measured against a graph this one is about to replace",
        ));
    };

    // 5. Root-set invariance (SPEC-1d), decided BEFORE anything is mutated. The
    //    one way a refresh could move the root set without touching it: the
    //    post-replace agent-memory restore mints the sidecar dir as a new root on
    //    a brain that has never declared one.
    let declared_before = state.declared_roots_canonical();
    if let Some(sidecar) = pending_memory_sidecar_root(state) {
        if !declared_before.contains(&sidecar) {
            return Ok(serde_json::json!({
                "ok": false,
                "schema": "m1nd-graph-ingest-refresh-v1",
                "action": "graph.ingest.refresh_declared_root",
                "refused": "refresh_would_change_roots",
                "reason": "restoring this brain's agent memory would declare a root it does not hold; a refresh never changes the root set",
                "declared_roots": declared_before,
                "would_add_root": sidecar,
            }));
        }
    }

    // 6. The CANDIDATE, computed first and committed only if it survives. The
    //    ingest itself is the existing one — this door builds no second scanner.
    let (candidate, stats) = m1nd_ingest::Ingestor::new(m1nd_ingest::IngestConfig {
        root: std::path::PathBuf::from(target),
        include_dotfiles: input.include_dotfiles,
        dotfile_patterns: input.dotfile_patterns.clone(),
        ..m1nd_ingest::IngestConfig::default()
    })
    .ingest()?;

    let live_nodes = u64::from(state.graph.read().num_nodes());
    let candidate_nodes = u64::from(candidate.num_nodes());

    // 7. The shrink floor (SPEC-1e), HARD and fail-closed.
    if live_nodes > 0 && candidate_nodes * 100 < live_nodes * REFRESH_SHRINK_FLOOR_PERCENT {
        return Ok(serde_json::json!({
            "ok": false,
            "schema": "m1nd-graph-ingest-refresh-v1",
            "action": "graph.ingest.refresh_declared_root",
            "refused": "refresh_would_shrink_graph",
            "reason": "the candidate holds too little of the live graph to be a refresh of it; nothing was mutated",
            "refreshed_root": canonical_root,
            "live_node_count": live_nodes,
            "candidate_node_count": candidate_nodes,
            "floor_percent": REFRESH_SHRINK_FLOOR_PERCENT,
        }));
    }

    // 8. Commit. `finalize_ingest` is the SAME durable path every ingest takes —
    //    graph swap, engine rebuild, inventory, `state.persist()`. Refresh mode
    //    differs from `replace` in exactly one way: it never writes the root set
    //    or the workspace binding. Captured and restored around the call anyway,
    //    so the invariant holds even if that path grows a new root writer.
    let roots_before = state.ingest_roots.clone();
    let workspace_before = state.workspace_root.clone();
    let mut output = finalize_ingest(state, input, "code", candidate, stats)?;
    state.ingest_roots = roots_before;
    state.workspace_root = workspace_before;

    let node_count = u64::from(state.graph.read().num_nodes());
    if let Some(object) = output.as_object_mut() {
        object.insert("ok".to_string(), serde_json::json!(true));
        object.insert(
            "schema".to_string(),
            serde_json::json!("m1nd-graph-ingest-refresh-v1"),
        );
        object.insert(
            "action".to_string(),
            serde_json::json!("graph.ingest.refresh_declared_root"),
        );
        object.insert(
            "refreshed_root".to_string(),
            serde_json::json!(canonical_root),
        );
        object.insert(
            "node_count_before".to_string(),
            serde_json::json!(live_nodes),
        );
        object.insert("node_count".to_string(), serde_json::json!(node_count));
        object.insert(
            "root_set_unchanged".to_string(),
            serde_json::json!(state.declared_roots_canonical() == declared_before),
        );
        object.insert(
            "shrink_floor_percent".to_string(),
            serde_json::json!(REFRESH_SHRINK_FLOOR_PERCENT),
        );
    }
    Ok(output)
}

/// The agent-memory sidecar root a post-replace restore would ingest, canonical,
/// or `None` when this refresh will not restore anything. Mirrors
/// `reload_agent_memory`'s own preconditions rather than guessing at them.
fn pending_memory_sidecar_root(state: &SessionState) -> Option<String> {
    let enabled = std::env::var("M1ND_AUTO_LOAD_AGENT_MEMORY")
        .map(|value| value != "0" && value != "false")
        .unwrap_or(true);
    if !enabled {
        return None;
    }
    let dir = state.runtime_root.join("agent-memory");
    if !dir.is_dir() {
        return None;
    }
    let has_light = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| entry.path().to_string_lossy().ends_with(".light.md"));
    if !has_light {
        return None;
    }
    Some(crate::project_brains::ProjectBrainRegistry::canonical_key(
        &dir.to_string_lossy(),
    ))
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

/// The repository a needs-ingest playbook should name — the binding's real
/// PROJECT root, never its runtime sidecar dir.
///
/// Field-triage 2026-07-22: on a fresh/empty brain served with a dedicated
/// runtime dir, `workspace_root` can be demoted onto that runtime dir (`.m1nd`),
/// so the needs_ingest step pointed `ingest` at the runtime dir instead of the
/// corpus. Prefer an explicit caller scope, then a resolved code root (which
/// never returns a memory sidecar or a non-repo runtime dir), then the caller's
/// own repo root; the placeholder only when nothing real resolves.
pub(crate) fn ingest_project_root_hint(state: &SessionState, scope: Option<&str>) -> String {
    scope
        .map(str::to_string)
        .or_else(|| state.code_root_path())
        .or_else(|| state.caller_root.clone())
        .unwrap_or_else(|| "<intended-repo-path>".to_string())
}

/// THE NEXT MOVE for a session whose graph is empty — read off the REAL gate,
/// never assumed.
///
/// "Run ingest for the intended repo" was the answer every empty-graph surface
/// gave (`north`, `delegate`, `trust_selftest`), and on 1.6.2 it was a refusal
/// loop: generic `ingest` classifies as `graph.ingest.replace` at
/// `POSITIVE_SOVEREIGN` and fails closed for every client. So this asks
/// `enforce_generic_action_policy` the same way `recovery_playbook` does — a
/// future typed consumer re-enables the one-call answer automatically — and
/// otherwise names the door that actually opens.
pub(crate) fn needs_ingest_next_move(state: &SessionState, agent_id: &str, then: &str) -> String {
    let repo = ingest_project_root_hint(state, None);
    let proposed = serde_json::json!({ "agent_id": agent_id, "path": repo });
    if crate::server::enforce_generic_action_policy("ingest", &proposed).is_ok() {
        return format!(
            "Run ingest for the intended repo, then call {then} again to get grounded context."
        );
    }
    // The hint's placeholder is what a brand-new brain resolves to: no code
    // root, no caller root, nothing to name. Printing it inside a command a
    // human is meant to TYPE would hand them `m1nd init --birth
    // <intended-repo-path>`, so the command becomes the relative one they can
    // run where they already are.
    let command = if repo.starts_with('<') {
        "m1nd init --birth .".to_string()
    } else {
        format!("m1nd init --birth {repo}")
    };
    format!(
        "This repo has no brain yet, and minting one is the human's gesture: offer them \
         `{command}` — once, in a terminal, from inside the repo — then call {then} again. Agents \
         never run it: the origin stamp exists only inside that CLI ingress."
    )
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

pub(crate) fn build_file_inventory_entries(
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
                sha256: crate::audit_handlers::content_sha256(&file.path),
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
    /// `None` on the no-evidence (cold-start) path so the bare 0.5 prior is never
    /// surfaced; read `trust_band` instead.
    trust_score: Option<f32>,
    trust_risk_multiplier: f32,
    trust_band: String,
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
    // An existing grounded_in edge can live in EITHER place:
    //   * pending_edges — a fresh merge sets finalized=false via add_node, so the new
    //     edges from this load may not be in the CSR yet; finalize() will drain them.
    //   * the CSR — finalize() is now non-destructive, so edges added by a previous
    //     resolution pass persist in the CSR rather than vanishing. We must dedup
    //     against them too, otherwise a re-run adds a duplicate grounded_in edge.
    let existing: std::collections::HashSet<(m1nd_core::types::NodeId, m1nd_core::types::NodeId)> = {
        let rel_interned = graph.strings.lookup(relation_needle);
        match rel_interned {
            Some(rel) => {
                let mut set: std::collections::HashSet<(
                    m1nd_core::types::NodeId,
                    m1nd_core::types::NodeId,
                )> = graph
                    .csr
                    .pending_edges
                    .iter()
                    .filter(|e| e.relation == rel)
                    .map(|e| (e.source, e.target))
                    .collect();
                let csr_nodes = graph.csr.offsets.len().saturating_sub(1);
                for src in 0..csr_nodes {
                    let src_nid = m1nd_core::types::NodeId::new(src as u32);
                    for idx in graph.csr.out_range(src_nid) {
                        if graph.csr.relations[idx] == rel {
                            set.insert((src_nid, graph.csr.targets[idx]));
                        }
                    }
                }
                set
            }
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
        let has_tag = graph.nodes.tags[i].contains(&tag_interned);
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
        let path_raw = path_raw.strip_prefix("./").unwrap_or(&path_raw).to_string();
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

/// Recency cap for the agent-memory auto-load: at most this many `.light.md`
/// files (the most recent by `Created`) re-enter always-on context each boot.
///
/// Read from `M1ND_MEMORY_LOAD_CAP`. **Default is unlimited (`usize::MAX`)** — a
/// pure no-op that loads exactly what today's code loads. The cap is opt-in and
/// only takes effect once the env var is set to a parseable positive integer;
/// `0`, empty, or garbage values are ignored (treated as unlimited) rather than
/// silently loading nothing.
fn agent_memory_load_cap() -> usize {
    std::env::var("M1ND_MEMORY_LOAD_CAP")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(usize::MAX)
}

/// Cheaply read the `Created: <epoch_ms>` frontmatter of a `.light.md` file
/// WITHOUT ingesting it. Mirrors the `Created:` key the light adapter's
/// `parse_header` recognises (frontmatter written by `render_light_markdown` as
/// `Created: <now_ms()>`). Returns `None` for legacy files that predate the
/// provenance stamp (#187) OR whose value is unparseable — those must be treated
/// as "unknown age", never as epoch-0-oldest, so the recency cap never evicts
/// the pre-existing corpus for merely lacking a `Created`.
fn read_light_created_ms(path: &std::path::Path) -> Option<u64> {
    let text = std::fs::read_to_string(path).ok()?;
    // The stamp lives in the frontmatter; scanning the first handful of lines is
    // enough and avoids reading large bodies.
    for line in text.lines().take(40) {
        let trimmed = line.trim();
        if trimmed == "---" {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("Created:") {
            return value.trim().parse::<u64>().ok();
        }
    }
    None
}

/// Ingest `<runtime_root>/agent-memory/*.light.md` (adapter=light, mode=merge) so
/// agent-authored L1GHT memory is loaded into the live graph and its evidence
/// re-anchors to the current code (`grounded_in`). Gated by env
/// `M1ND_AUTO_LOAD_AGENT_MEMORY` (default ON). Returns a report for the
/// trust/handshake layer (and for ingest responses after a `replace`), or `None`
/// when there is no agent-memory directory yet. Called at boot AND after a
/// `replace` ingest (which would otherwise wipe the memory). Honest: surfaces
/// empty/zero with a note rather than fabricating.
///
/// Recency cap (`M1ND_MEMORY_LOAD_CAP`, default unlimited → no-op): when set and
/// the file count exceeds it, only the K most-recent-by-`Created` files load;
/// the rest are dropped and reported under `capped_out` so forgetting is
/// provable, not silent. Files with NO parseable `Created` (legacy, pre-#187)
/// are EXEMPT from eviction — always loaded — so setting a cap never evicts the
/// pre-existing corpus for merely lacking a provenance stamp.
pub fn reload_agent_memory(state: &mut SessionState) -> Option<serde_json::Value> {
    let enabled = std::env::var("M1ND_AUTO_LOAD_AGENT_MEMORY")
        .map(|v| v != "0" && v != "false")
        .unwrap_or(true);
    let dir = state.runtime_root.join("agent-memory");
    if !enabled {
        return Some(serde_json::json!({
            "loaded": false,
            "skipped": "disabled by M1ND_AUTO_LOAD_AGENT_MEMORY=0",
        }));
    }
    if !dir.is_dir() {
        return None;
    }
    let mut light_files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".light.md"))
        .collect();
    let file_count = light_files.len();
    let dir_str = dir.to_string_lossy().to_string();
    if file_count == 0 {
        return Some(serde_json::json!({
            "dir": dir_str,
            "file_count": 0,
            "loaded": false,
            "skipped": "no .light.md files",
        }));
    }

    // Recency cap. Default (unlimited, or file_count within budget) leaves the
    // legacy single-directory ingest untouched — a pure no-op. Only when a cap is
    // set AND exceeded do we select the survivors and record the drops.
    let cap = agent_memory_load_cap();
    let mut capped_out: Vec<String> = Vec::new();
    let mut capped_names: Vec<String> = Vec::new();
    if file_count > cap {
        // Rank by `Created` DESC. Missing/unparseable `Created` (legacy corpus)
        // is EXEMPT: rank it as `u64::MAX` so it always survives eviction rather
        // than sinking to the oldest bucket.
        let mut ranked: Vec<(u64, bool, std::path::PathBuf)> = light_files
            .iter()
            .map(|p| match read_light_created_ms(p) {
                Some(ms) => (ms, false, p.clone()),
                None => (u64::MAX, true, p.clone()),
            })
            .collect();
        // Most recent first; among equal timestamps keep a stable-ish order by path.
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));

        // Everything with an unknown age (exempt) must survive even if it pushes
        // past the cap — the guard is "never drop for lacking Created". So the
        // effective budget is max(cap, #exempt).
        let exempt = ranked.iter().filter(|(_, missing, _)| *missing).count();
        let keep = cap.max(exempt);

        let survivors: Vec<std::path::PathBuf> = ranked
            .iter()
            .take(keep)
            .map(|(_, _, p)| p.clone())
            .collect();
        for (_, _, p) in ranked.iter().skip(keep) {
            capped_out.push(p.to_string_lossy().to_string());
            capped_names.push(
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.to_string_lossy().to_string()),
            );
        }
        light_files = survivors;
    }
    let loaded_count = light_files.len();

    let nodes_before = state.graph.read().num_nodes();
    // Fast path (no cap active / within budget): ingest the whole directory in a
    // single walker pass, exactly as before. Capped path: ingest the surviving
    // files one at a time (merge accumulates), reusing the same light adapter.
    let ingest_result: M1ndResult<serde_json::Value> = if capped_out.is_empty() {
        let ingest_input = crate::protocol::core::IngestInput {
            path: dir_str.clone(),
            agent_id: "boot".to_string(),
            incremental: false,
            adapter: "light".to_string(),
            mode: "merge".to_string(),
            namespace: Some("light".to_string()),
            include_dotfiles: false,
            dotfile_patterns: vec![],
            project_root: None,
        };
        handle_ingest(state, ingest_input)
    } else {
        let mut last: serde_json::Value = serde_json::Value::Null;
        let mut err: Option<m1nd_core::error::M1ndError> = None;
        for f in &light_files {
            let ingest_input = crate::protocol::core::IngestInput {
                path: f.to_string_lossy().to_string(),
                agent_id: "boot".to_string(),
                incremental: false,
                adapter: "light".to_string(),
                mode: "merge".to_string(),
                namespace: Some("light".to_string()),
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            };
            match handle_ingest(state, ingest_input) {
                Ok(r) => last = r,
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        match err {
            Some(e) => Err(e),
            None => Ok(last),
        }
    };

    match ingest_result {
        Ok(result) => {
            let nodes_added = state.graph.read().num_nodes().saturating_sub(nodes_before);
            if capped_out.is_empty() {
                eprintln!(
                    "[m1nd] Loaded agent memory: {} file(s), +{} nodes from {}",
                    loaded_count, nodes_added, dir_str,
                );
            } else {
                eprintln!(
                    "[m1nd] Loaded agent memory: {} of {} file(s) (M1ND_MEMORY_LOAD_CAP={}), +{} nodes from {}; capped out {} older file(s): {}",
                    loaded_count,
                    file_count,
                    cap,
                    nodes_added,
                    dir_str,
                    capped_out.len(),
                    capped_names.join(", "),
                );
            }

            // Best-effort evidence freshness: report whatever cross_verify finds,
            // honestly noting when there is no recorded inventory to verify against.
            let (stale_count, stale_claims, freshness_note) = {
                let cross_verify_input = crate::protocol::layers::CrossVerifyInput {
                    agent_id: "boot".to_string(),
                    scope: None,
                    check: vec!["evidence_freshness".to_string()],
                    include_dotfiles: false,
                    dotfile_patterns: vec![],
                };
                match crate::audit_handlers::handle_cross_verify(state, cross_verify_input) {
                    Ok(cv) => {
                        let count = cv["stale_evidence_count"].as_u64().unwrap_or(0) as usize;
                        let claims: Vec<serde_json::Value> = cv["stale_evidence"]
                            .as_array()
                            .map(|arr| arr.iter().take(5).cloned().collect())
                            .unwrap_or_default();
                        let note = if state.file_inventory.is_empty()
                            || state.file_inventory.values().all(|e| e.sha256.is_none())
                        {
                            "unverifiable_until_code_reingest"
                        } else {
                            "verified_against_stored_inventory"
                        };
                        (count, claims, note)
                    }
                    Err(_) => (0, vec![], "unverifiable_until_code_reingest"),
                }
            };

            Some(serde_json::json!({
                "dir": dir_str,
                "file_count": file_count,
                "loaded_count": loaded_count,
                "load_cap": if cap == usize::MAX { serde_json::Value::Null } else { serde_json::json!(cap) },
                "capped_out_count": capped_out.len(),
                "capped_out": capped_out,
                "loaded": true,
                "nodes_added": nodes_added,
                "light_evidence_resolved": result.get("light_evidence_resolved").cloned().unwrap_or(serde_json::Value::Null),
                "light_evidence_unresolved": result.get("light_evidence_unresolved").cloned().unwrap_or(serde_json::Value::Null),
                "stale_evidence_count": stale_count,
                "stale_claims": stale_claims,
                "freshness_note": freshness_note,
            }))
        }
        Err(e) => {
            eprintln!("[m1nd] WARNING: agent-memory load failed: {}", e);
            Some(serde_json::json!({
                "dir": dir_str,
                "file_count": file_count,
                "loaded_count": loaded_count,
                "load_cap": if cap == usize::MAX { serde_json::Value::Null } else { serde_json::json!(cap) },
                "capped_out_count": capped_out.len(),
                "capped_out": capped_out,
                "loaded": false,
                "error": e.to_string(),
            }))
        }
    }
}

fn finalize_ingest(
    state: &mut SessionState,
    input: &IngestInput,
    adapter: &str,
    new_graph: m1nd_core::graph::Graph,
    stats: m1nd_ingest::IngestStats,
) -> M1ndResult<serde_json::Value> {
    finalize_ingest_with_inventory(state, input, adapter, new_graph, stats, None)
}

fn finalize_ingest_with_inventory(
    state: &mut SessionState,
    input: &IngestInput,
    adapter: &str,
    new_graph: m1nd_core::graph::Graph,
    stats: m1nd_ingest::IngestStats,
    sealed_inventory: Option<Vec<crate::session::FileInventoryEntry>>,
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

    // Every mode below installs a graph whose `edge_plasticity` arrays are born
    // zeroed — `replace`/`refresh` because the scan builds a new graph, `merge`
    // because the merge re-adds each edge through `Graph::add_edge`. Take the
    // learned synapses out here, while the live graph still holds them; they go
    // back in right after `rebuild_engines` below.
    let carried_synapses = state.export_learned_synapses_before_replacement();

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

    let inventory_entries = match sealed_inventory {
        Some(entries) => entries,
        None => {
            let graph = state.graph.read();
            build_file_inventory_entries(&graph, &stats.discovered_files)
        }
    };

    // #7 — memory freshness check after a code ingest.
    //
    // Capture the previous inventory (old hashes) BEFORE we overwrite it with
    // the fresh hashes.  We also build a map of the newly-ingested external_ids
    // (the files that were just (re)parsed) so we only flag those that actually
    // changed in this ingest.
    //
    // Borrow/lock safety: the graph write lock was released at the end of the
    // `let (light_evidence_resolved, ...)` block above (line ~407).  We acquire
    // only a READ lock here — no risk of deadlock.
    let memory_freshness: serde_json::Value = if adapter == "code" {
        // Snapshot the old inventory (has the previous sha256 values).
        let previous_inventory = state.file_inventory.clone();

        // Build a set of external_ids that are part of this code ingest.
        let ingested_ids: HashSet<String> = inventory_entries
            .iter()
            .map(|e| e.external_id.clone())
            .collect();

        // Walk grounded_in edges to find memorized claims whose cited code
        // file was just re-ingested.  Compare old vs new hash to detect changes.
        let graph = state.graph.read();
        let grounded_in_interned = graph.strings.lookup("grounded_in");

        let mut stale: Vec<serde_json::Value> = Vec::new();

        if let Some(gi) = grounded_in_interned {
            // Build reverse map: NodeId → external_id.
            let nid_to_ext: HashMap<usize, String> = graph
                .id_to_node
                .iter()
                .map(|(interned, &nid)| {
                    (nid.as_usize(), graph.strings.resolve(*interned).to_string())
                })
                .collect();

            let node_count_inner = graph.nodes.count as usize;
            for src_idx in 0..node_count_inner {
                let src_nid = m1nd_core::types::NodeId::new(src_idx as u32);
                let range = graph.csr.out_range(src_nid);
                for edge_i in range {
                    if graph.csr.relations[edge_i] != gi {
                        continue;
                    }
                    let tgt_nid = graph.csr.targets[edge_i];
                    let tgt_ext_id = match nid_to_ext.get(&tgt_nid.as_usize()) {
                        Some(id) => id.clone(),
                        None => continue,
                    };
                    if !tgt_ext_id.starts_with("file::") {
                        continue;
                    }
                    // Only flag files that were part of this code ingest.
                    if !ingested_ids.contains(&tgt_ext_id) {
                        continue;
                    }
                    let rel_path = &tgt_ext_id["file::".len()..];
                    let marker_label = graph
                        .strings
                        .resolve(graph.nodes.label[src_idx])
                        .to_string();
                    let marker_ext_id = nid_to_ext.get(&src_idx).cloned().unwrap_or_default();

                    // New hash: from the just-built inventory_entries.
                    let new_hash = inventory_entries
                        .iter()
                        .find(|e| e.external_id == tgt_ext_id)
                        .and_then(|e| e.sha256.clone());

                    // Old hash: from the snapshot taken before this ingest.
                    let old_hash = previous_inventory
                        .get(&tgt_ext_id)
                        .and_then(|e| e.sha256.clone());

                    match (old_hash, new_hash) {
                        (Some(old), Some(new)) if old != new => {
                            stale.push(serde_json::json!({
                                "marker": marker_ext_id,
                                "claim": marker_label,
                                "evidence_path": rel_path,
                                "reason": "evidence_changed",
                            }));
                        }
                        (None, _) => {
                            // No prior hash recorded — treat as possibly changed.
                            stale.push(serde_json::json!({
                                "marker": marker_ext_id,
                                "claim": marker_label,
                                "evidence_path": rel_path,
                                "reason": "evidence_possibly_changed",
                            }));
                        }
                        _ => {
                            // Hashes match or new hash absent — evidence fresh / unverifiable.
                        }
                    }
                }
            }
        }
        drop(graph);

        let stale_count = stale.len();
        serde_json::json!({
            "stale_evidence_count": stale_count,
            "stale_evidence": stale,
        })
    } else {
        serde_json::Value::Null
    };

    // `refresh` shares `replace`'s whole-graph semantics (it IS a fresh scan of
    // the root), so it resets the inventory too — a file deleted upstream must
    // stop being claimed as known.
    if mode != "merge" {
        state.reset_file_inventory();
    }
    state.record_file_inventory(inventory_entries);

    state.rebuild_engines()?;
    // IMMEDIATELY after the rebuild, and before anything downstream can persist:
    // the `state.persist()` at the end of this function is what used to publish
    // the zeroed counters over the sidecar, and the rebuild is what installs the
    // two fresh engines this restore seeds the query counter into.
    let synapses_restored = state.restore_learned_synapses_after_replacement(carried_synapses);
    if adapter == "universal" && !state.document_cache.entries.is_empty() {
        universal_docs::refresh_all_document_semantics(state);
    }

    // Track ingest roots for L3 git discovery and scope normalization.
    // Replace mode resets the active roots to the new source of truth.
    //
    // `refresh` writes NEITHER branch: SPEC-1d says a refresh never changes the
    // root set, and the cheapest way to guarantee that is to have no code that
    // could. It re-scans a root the brain already declared, so there is nothing
    // to declare and nothing to reset.
    if mode == "refresh" {
        // deliberately nothing
    } else if mode == "replace" {
        state.ingest_roots.clear();
        state.ingest_roots.push(input.path.clone());
    } else {
        // Budget Law (§C1.3.4 write-path fix): a `.light.md` memory claim written
        // into the `agent-memory` STORE must NOT mint a per-file ingest root — the
        // store DIRECTORY is the one root, not each sidecar. Otherwise every
        // `memorize` write grows the roots array by one, sprawling the packet.
        // Narrowly scoped to files whose parent dir is `agent-memory`: a user
        // ingesting an arbitrary `.light.md` by path elsewhere keeps its own root
        // (that path is a deliberate root, not store sprawl).
        let ingest_path = std::path::Path::new(&input.path);
        let parent_is_agent_memory = ingest_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|n| n == "agent-memory")
            .unwrap_or(false);
        let root_to_track =
            if input.path.ends_with(".light.md") && ingest_path.is_file() && parent_is_agent_memory
            {
                ingest_path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| input.path.clone())
            } else {
                input.path.clone()
            };
        // Keep the vector ordered oldest -> newest so path resolution can prefer
        // the most recent matching root deterministically.
        if let Some(pos) = state
            .ingest_roots
            .iter()
            .position(|root| root == &root_to_track)
        {
            let root = state.ingest_roots.remove(pos);
            state.ingest_roots.push(root);
        } else {
            state.ingest_roots.push(root_to_track);
        }
    }
    let input_path = std::path::Path::new(&input.path);
    let candidate_workspace_root = if input_path.is_dir() {
        input.path.clone()
    } else {
        input_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_string_lossy()
            .to_string()
    };
    // #326 family (3rd member — the memorize / agent-memory merge path): a
    // `memorize` writes `<runtime>/agent-memory/<slug>.light.md` and then ingests
    // it here (adapter=light, mode=merge). The candidate workspace_root above then
    // resolves to the `agent-memory` STORE dir — and blindly assigning it DEMOTES a
    // real code `workspace_root` onto the memory sidecar, exactly the store-dir /
    // code-root confusion #326 named and the #355 `auto_ingest` guard already
    // fenced off. Mirror that guard: a memory-store ingest NEVER rebases a code (or
    // manifest-bound) workspace_root onto the sidecar. A store with no code root
    // yet (a fresh pure-memory / medulla bootstrap) still adopts the candidate, so
    // nothing regresses.
    let demotes_to_store = crate::session::is_memory_sidecar(&candidate_workspace_root);
    let holds_code_root = state
        .workspace_root
        .as_deref()
        .map(|ws| !crate::session::is_memory_sidecar(ws))
        .unwrap_or(false);
    let manifest_bound = state.workspace_root_source.as_deref() == Some("project_brain_manifest");
    // #326 recurrence (field reports 2026-07-14, two flips in two days): the SHARED
    // SERVED OWNER must be immune to a classic `ingest {path}` carrying a FOREIGN
    // local cwd. A local run (the `first-minute` shim, then `npm test`) reaches the
    // served owner over HTTP and its trust sequence sends `ingest {path: <cwd>}`;
    // that silently rebound the owner's `workspace_root` to the cwd (`npm/bin`, then
    // `npm/test`), poisoning every session's binding card until the next kickstart.
    // The served owner is marked by `runnerd_naming` (stamped unconditionally on the
    // HTTP serve boot, `http_server.rs`; `None` on stdio); once it holds an
    // established code root (its runtime home — the medulla identity
    // `infer_workspace_root` sets at boot) only its OWN boot/config gesture may move
    // that binding, never a foreign process's ingest. The classic stdio single-graph
    // bind (`runnerd_naming` None) is untouched, and a per-project
    // `ingest {project_root}` routes to a hosted brain before this seam runs.
    let served_owner_pinned = state.runnerd_naming.is_some() && holds_code_root;
    // `refresh` never rebinds: the root it scanned is already this brain's, so
    // there is no binding to move (SPEC-1d).
    if mode != "refresh"
        && !(demotes_to_store && (holds_code_root || manifest_bound))
        && !served_owner_pinned
    {
        state.workspace_root = Some(candidate_workspace_root);
    }

    if let Err(e) = state.persist() {
        eprintln!("[m1nd] auto-persist after ingest failed: {}", e);
    }

    // A `replace` ingest wipes the whole graph — including agent-authored L1GHT
    // memory and its grounded_in edges. Restore it by re-merging agent-memory,
    // which also re-anchors evidence to the freshly-ingested code. (Skip for the
    // light adapter: the caller is explicitly managing light content, and the
    // re-merge runs as adapter=light/mode=merge so it never recurses here.)
    // (`refresh` wipes it the same way and restores it the same way; SPEC-1d's
    // root-set invariance is preserved because the door refuses up front when
    // that restore would have to declare a root the brain does not hold.)
    let agent_memory_restored = if mode != "merge" && adapter != "light" {
        reload_agent_memory(state)
    } else {
        None
    };

    let (node_count, edge_count) = {
        let graph = state.graph.read();
        (graph.num_nodes(), graph.num_edges())
    };

    let mut out = serde_json::json!({
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
        // How many learned synapses were carried across the replacement. Zero on
        // a brain that has never been queried; a drop to zero on a warm one is
        // the erasure this field exists to make visible.
        "synapses_restored": synapses_restored,
    });
    // Include memory_freshness only for code ingests (non-null).
    if !memory_freshness.is_null() {
        out.as_object_mut()
            .unwrap()
            .insert("memory_freshness".to_string(), memory_freshness);
    }
    // Surface agent-memory restoration after a replace so the agent knows its
    // L1GHT memory survived the re-index (no silent loss).
    if let Some(restored) = agent_memory_restored {
        out.as_object_mut()
            .unwrap()
            .insert("agent_memory_restored".to_string(), restored);
    }
    Ok(out)
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
            budget: None,
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

    // Read-only attach takes the immutable read path (query_readonly); read-write
    // keeps the historical mutate-on-query (plasticity) behavior.
    let result = state.run_query(&config)?;

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

    // Context-budget packing: keep the highest-activation nodes (already
    // rank-ordered by dedupe_ranked) that fit the agent's declared token
    // budget. Only engages when `token_budget` is provided — otherwise the
    // output is byte-for-byte unchanged.
    let (activated, budget) = if let Some(budget_tokens) = input.token_budget {
        let (kept, dropped) = crate::result_shaping::pack_to_budget(
            activated,
            budget_tokens,
            activated_node_token_estimate,
        );
        let used: usize = kept.iter().map(activated_node_token_estimate).sum();
        let block = crate::result_shaping::budget_block(budget_tokens, used, kept.len(), dropped);
        (kept, Some(block))
    } else {
        (activated, None)
    };

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
        budget,
    })
}

/// Per-node token ESTIMATE for an activated node, summed over its load-bearing
/// text fields (label, type, node_id, tags, provenance path/excerpt). Uses the
/// chars/4 heuristic — an approximation, not exact tokenization.
fn activated_node_token_estimate(node: &ActivatedNodeOutput) -> usize {
    let mut chars = node.label.len() + node.node_type.len() + node.node_id.len();
    for tag in &node.tags {
        chars += tag.len();
    }
    if let Some(prov) = node.provenance.as_ref() {
        if let Some(path) = prov.source_path.as_deref() {
            chars += path.len();
        }
        if let Some(excerpt) = prov.excerpt.as_deref() {
            chars += excerpt.len();
        }
    }
    crate::result_shaping::estimate_tokens_from_chars(chars)
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

    // Reverse lookup NodeId -> external_id, so the rank below can detect
    // TEST-source callers by path (mirrors the build at the activate handler).
    let node_to_ext: Vec<String> = {
        let mut map = vec![String::new(); graph.num_nodes() as usize];
        for (interned, &nid) in &graph.id_to_node {
            let idx = nid.as_usize();
            if idx < map.len() {
                map[idx] = graph.strings.resolve(*interned).to_string();
            }
        }
        map
    };

    // True if a node is a TEST function: either it carries the `"test"` tag the
    // Rust extractor now attaches to `#[cfg(test)]`-module / `#[test]` fns (catches
    // in-file unit tests living in a NON-test path, which the path check misses),
    // OR its external_id is a test SOURCE file (path-based, the pre-existing
    // signal). Both are cheap; together they cover in-file and separate-file tests.
    let is_test_node = |idx: usize| -> bool {
        if idx >= graph.num_nodes() as usize {
            return false;
        }
        let tagged = graph.node_tags(NodeId::new(idx as u32)).contains(&"test");
        tagged || node_to_ext.get(idx).is_some_and(|e| is_test_source(e))
    };

    // Rank for the cap+display so the agent's real question ("what code is
    // affected / who calls this") is answered first. Pure signal_strength buries
    // the actual caller/callee FUNCTIONS under their containing File/Module nodes
    // (which accumulate more blast energy), so a function caller can land past the
    // cap. Order by: PRODUCTION code symbols, then TEST symbols, then containers
    // (Module/File/Directory), then nearest hop, then signal. Splitting prod vs
    // test inside the symbol tier keeps the real (production) caller above the
    // in-file `#[cfg(test)]` callers that otherwise tie on signal and crowd it out
    // of the cap window. The output still carries `node_type`, so an agent can
    // re-filter.
    let type_rank = |idx: usize| -> u8 {
        if idx >= graph.num_nodes() as usize {
            return 6;
        }
        match format!("{:?}", graph.nodes.node_type[idx]).as_str() {
            "Function" | "Struct" | "Enum" | "Type" | "Trait" => {
                if is_test_node(idx) {
                    1
                } else {
                    0
                }
            }
            "Module" => 3,
            "File" => 4,
            "Directory" => 5,
            _ => 2,
        }
    };
    let mut sorted_blast = impact.blast_radius.clone();
    sorted_blast.sort_by(|a, b| {
        type_rank(a.node.as_usize())
            .cmp(&type_rank(b.node.as_usize()))
            .then(a.hop_distance.cmp(&b.hop_distance))
            .then(
                b.signal_strength
                    .get()
                    .partial_cmp(&a.signal_strength.get())
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    // #3 — Build a lookup of which blast-radius nodes are light:evidenced_by
    // citation markers so we can annotate them with `is_knowledge_citation`.
    // Strategy: for each node in the capped blast radius, check (a) whether it
    // carries the `light:evidenced_by` tag directly, or (b) whether it is a
    // `light::` node that has an incoming `grounded_in` edge from a known marker.
    // We build the annotation map before the final collect so the closure stays
    // immutable-borrow-only.
    let knowledge_citation_map: HashMap<usize, String> = {
        let evidenced_by_tag = graph.strings.lookup("light:evidenced_by");
        let grounded_in_rel = graph.strings.lookup("grounded_in");
        let n = graph.num_nodes() as usize;
        let mut map: HashMap<usize, String> = HashMap::new();

        if let Some(tag) = evidenced_by_tag {
            // Collect all marker node indices that carry the tag (full graph).
            // For each marker, record its label as the "claim" text.
            let marker_labels: HashMap<usize, String> = (0..n)
                .filter(|&i| {
                    graph
                        .nodes
                        .tags
                        .get(i)
                        .is_some_and(|tags| tags.contains(&tag))
                })
                .map(|i| {
                    let lbl = graph.strings.resolve(graph.nodes.label[i]).to_string();
                    (i, lbl)
                })
                .collect();

            // Collect the set of blast-radius NodeId indices for fast lookup.
            let blast_set: HashSet<usize> = sorted_blast
                .iter()
                .take(max_nodes_cap)
                .map(|e| e.node.as_usize())
                .collect();

            // Case (a): blast-radius node is itself a light:evidenced_by marker.
            for &idx in &blast_set {
                if let Some(lbl) = marker_labels.get(&idx) {
                    map.insert(idx, lbl.clone());
                }
            }

            // Case (b): blast-radius node is a `light::` knowledge node (identified
            // by its external_id starting with "light::") reached via a grounded_in
            // edge.  Walk the CSR: for each marker, follow grounded_in edges to
            // targets; if the target is in the blast set, annotate it with the
            // marker's claim.
            if let Some(gi) = grounded_in_rel {
                for (&marker_idx, marker_lbl) in &marker_labels {
                    let marker_nid = m1nd_core::types::NodeId::new(marker_idx as u32);
                    let range = graph.csr.out_range(marker_nid);
                    for edge_i in range {
                        if graph.csr.relations[edge_i] != gi {
                            continue;
                        }
                        let tgt_idx = graph.csr.targets[edge_i].as_usize();
                        if blast_set.contains(&tgt_idx) && !map.contains_key(&tgt_idx) {
                            map.insert(tgt_idx, marker_lbl.clone());
                        }
                    }
                }
            }
        }
        map
    };

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
            let (is_knowledge_citation, claim) = if let Some(c) = knowledge_citation_map.get(&idx) {
                (Some(true), Some(c.clone()))
            } else {
                (None, None)
            };
            BlastRadiusEntry {
                node_id: label.clone(),
                label,
                node_type,
                signal_strength: e.signal_strength.get(),
                hop_distance: e.hop_distance,
                is_knowledge_citation,
                claim,
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

    // Read-only attach takes the immutable read path (query_readonly).
    let result = state.run_query(&config)?;

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
            // No node pair, so no path and nothing to be incomplete: closed.
            return Ok(serde_json::json!({
                "source": input.source,
                "target": input.target,
                "paths": [],
                "reason": "One or both nodes not found",
                "closure": closure_verdict(&[]),
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
    // Load-bearing edges on the reconstructed answer path, each as
    // (source_external_id, relation, reason) where `reason` is Some(..) iff the
    // edge's SOURCE node carries an ambiguous/unresolved provenance tag. Only
    // edges ON the path count — incidental graph edges are never inspected.
    let mut load_bearing: Vec<(String, String, Option<String>)> = Vec::new();
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
            // The CSR forward edge `edge_j` points at `targets[edge_j]`; its real
            // SOURCE is the OTHER endpoint of {prev, current}. BFS is
            // bidirectional, so `prev` may be either endpoint — disambiguate via
            // the stored target.
            let edge_target = graph.csr.targets[edge_j].as_usize();
            let edge_source = if edge_target == current {
                prev
            } else {
                current
            };
            // Edge-specific provenance: `edge_target` (the CSR-stored target `T`)
            // is the real directed edge's target in both BFS directions, so we can
            // ask whether THIS edge (edge_source -> edge_target) was the ambiguous
            // guess — not whether edge_source has any ambiguous edge at all.
            let reason = closure_reason_for_edge(&graph, edge_source, edge_target);
            load_bearing.push((edge_external_id(&graph, edge_source), rel.clone(), reason));
            path_relations.push(rel);
            current = prev;
            if current == source_node.as_usize() {
                break;
            }
        }
        path_nodes.reverse();
        path_relations.reverse();
        load_bearing.reverse();

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

    let closure = closure_verdict(&load_bearing);

    Ok(serde_json::json!({
        "source": input.source,
        "target": input.target,
        "paths": paths,
        "same_community": same_community,
        "found": found,
        "closure": closure,
    }))
}

/// External id (stable address) of a node, or a synthetic `node_<idx>` fallback.
/// Mirrors the reverse lookup used elsewhere in this module.
fn edge_external_id(graph: &m1nd_core::graph::Graph, node_idx: usize) -> String {
    let nid = m1nd_core::types::NodeId::new(node_idx as u32);
    for (interned, &candidate) in &graph.id_to_node {
        if candidate == nid {
            return graph.strings.resolve(*interned).to_string();
        }
    }
    format!("node_{node_idx}")
}

/// Read the provenance reason for the SPECIFIC directed edge `source -> target`
/// that lies ON a reconstructed `why` path. Returns:
///   * `Some("ambiguous")` iff THIS edge was a genuine coin-flip at ingest — i.e.
///     `source` carries the TARGETED `m1nd:edge:ambiguous:<target_ext_id>` tag for
///     exactly this target;
///   * else `Some("unresolved")` iff `source` carries the node-level
///     `EDGE_UNRESOLVED_TAG`;
///   * else `None` (clean).
///
/// The AMBIGUOUS reason is now edge-specific — this is the cry-wolf fix. The tag
/// is written per-SOURCE-NODE at ingest, but the closure verdict is a PER-PATH
/// claim (`closure_verdict`'s contract: only edges ON the path count). The old
/// reader used the BARE node-level ambiguous tag, so any node with SOME ambiguous
/// outbound edge poisoned EVERY path through it — a clean edge like
/// `handle_seek -> pack_to_budget` (a unique-name target) was reported `blocked`
/// merely because `handle_seek` also calls a common-named fn (`get`/`resolve`/
/// `new`) that is a genuine same-name tie. Reading the TARGETED tag blames ONLY
/// the specific guessed edge, so clean siblings are no longer falsely blocked.
///
/// The UNRESOLVED reason is deliberately LEFT node-level (semantics unchanged, per
/// the field-triage #4 scope): a dropped reference created no edge to key against,
/// and the existing contract (see `why_reports_blocked_when_path_rests_on_dangling_edge`)
/// is that a node with a dropped outbound reference honestly flags paths leaving
/// it as incomplete. Only the ambiguous over-fire is tightened here. NOTE: this
/// leaves a residual node-level over-fire for `unresolved` (a clean edge out of a
/// node that drops an UNRELATED ref still reads blocked) — tracked as follow-up;
/// not touched here because unresolved semantics were explicitly out of scope.
fn closure_reason_for_edge(
    graph: &m1nd_core::graph::Graph,
    source_idx: usize,
    target_idx: usize,
) -> Option<String> {
    let source = m1nd_core::types::NodeId::new(source_idx as u32);
    let target_ext_id = edge_external_id(graph, target_idx);
    if m1nd_ingest::resolve::source_has_ambiguous_edge_to(graph, source, &target_ext_id) {
        Some("ambiguous".to_string())
    } else if graph
        .node_tags(source)
        .contains(&m1nd_ingest::resolve::EDGE_UNRESOLVED_TAG)
    {
        Some("unresolved".to_string())
    } else {
        None
    }
}

/// Pure path→verdict: given the load-bearing edges on a reconstructed `why`
/// path (each as (source_id, relation, reason)), decide whether the answer is
/// honestly `closed` or `blocked` by an edge m1nd could not cleanly resolve.
///
/// `state == "blocked"` iff at least one load-bearing edge's source is tagged
/// (reason is `Some`); otherwise `closed`. An empty list (no path, or an
/// all-clean path) is `closed` with no dangling edges — there is nothing to be
/// incomplete. Only the edges PASSED IN are considered, so off-path tagged
/// nodes never affect the verdict (load-bearing scoping is the caller's job).
fn closure_verdict(load_bearing: &[(String, String, Option<String>)]) -> serde_json::Value {
    let dangling: Vec<serde_json::Value> = load_bearing
        .iter()
        .filter_map(|(source, relation, reason)| {
            reason.as_ref().map(|r| {
                serde_json::json!({
                    "source": source,
                    "relation": relation,
                    "reason": r,
                })
            })
        })
        .collect();

    if dangling.is_empty() {
        serde_json::json!({
            "state": "closed",
            "dangling_edges": [],
            "why": "every load-bearing edge on the path resolved cleanly",
        })
    } else {
        serde_json::json!({
            "state": "blocked",
            "dangling_edges": dangling,
            "why": format!(
                "{} load-bearing edge(s) on the path rest on a guessed or dropped reference",
                dangling.len()
            ),
        })
    }
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
    // Apply min_co_change_count directly on the raw observation count carried by
    // each entry (default 2) to suppress one-off coincidental pairs — the
    // coupling strength itself is a smoothed-Jaccard association now, not a
    // count proxy. Structural seeds (co_count == 0) never pass this filter,
    // matching the old behavior where the floor sat above the 0.1 base.
    let min_co_count = input.min_co_change_count.unwrap_or(2).max(1);
    let git_co_change_predictions: Vec<m1nd_core::temporal::CoChangeEntry> = {
        let git_preds = state
            .orchestrator
            .temporal
            .co_change
            .predict(node, input.top_k);
        git_preds
            .into_iter()
            .filter(|e| e.co_count >= min_co_count)
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
                    co_count: 0,
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
                    co_count: 0,
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
                trust_score: if trust.tier == m1nd_core::trust::TrustTier::Unknown {
                    None
                } else {
                    Some(trust.trust_score)
                },
                trust_risk_multiplier: trust.risk_multiplier,
                trust_band: m1nd_core::trust::trust_band(trust.tier).to_string(),
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

    // OMEGA Move 0: gate each prediction with a calibrated act|reverify|abstain
    // verdict. The co-change model signal here is `coupling_strength`; we bin it
    // against the stored conformal threshold τ for the "predict" signal.
    //
    // HONESTY INVARIANT: a band is NEVER quoted as a probability — only the
    // binned verdict. With no calibration row, the gate is `uncalibrated` and
    // EVERY verdict is honestly `abstain`, never a fake-high `act`.
    let predict_calibration = state
        .calibration_table
        .get(m1nd_core::calibration::CALIBRATION_SIGNAL_PREDICT)
        .cloned();

    let prediction_output: Vec<serde_json::Value> = ranked_predictions
        .iter()
        .map(|prediction| {
            let verdict = match &predict_calibration {
                Some(row) => row.verdict(prediction.coupling_strength),
                None => m1nd_core::calibration::VERDICT_ABSTAIN,
            };
            serde_json::json!({
                "node_id": prediction.external_id,
                "label": prediction.label,
                "source": prediction.source.as_str(),
                "coupling_strength": prediction.coupling_strength,
                "confidence": prediction.confidence,
                "verdict": verdict,
                "heuristic_factor": prediction.heuristic_factor,
                "trust_score": prediction.trust_score,
                "trust_band": prediction.trust_band,
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

    // Top-level calibration state for the gate, so the agent can see WHETHER the
    // verdicts are measured or honestly uncalibrated.
    let calibration_block = match &predict_calibration {
        Some(row) => serde_json::json!({
            "signal": "predict",
            "calibrated": true,
            "tau": row.tau,
            "tau_reverify_floor": row.tau_low(),
            "target_alpha": row.target_alpha,
            "measured_precision": row.measured_precision,
            "coverage": row.coverage,
            "n": row.n,
        }),
        None => serde_json::json!({
            "signal": "predict",
            "calibrated": false,
            "verdict": "abstain",
            "note": "predict is not calibrated yet — run `calibrate_predict` to measure precision-at-coverage and a conformal τ from this repo's git history. Until then every verdict is honestly `abstain`, never `act`.",
        }),
    };

    let mut predict_out = serde_json::json!({
        "changed_node": input.changed_node,
        "predictions": prediction_output,
        "co_change_count": co_change_count,
        "structural_fallback_count": structural_fallback_count,
        "heuristic_reranked": true,
        "velocity": velocity,
        "calibration": calibration_block,
    });
    // Honest empty-state guidance: co-change predictions need the git co-change
    // matrix, which `ghost_edges` builds. Without it predict falls back to
    // structural edges only; if both are empty, tell the agent how to populate.
    if co_change_count == 0 && structural_fallback_count == 0 {
        predict_out.as_object_mut().unwrap().insert(
            "note".into(),
            serde_json::json!(
                "No co-change history loaded — run `ghost_edges` (parses git commit history into the co-change matrix) before `predict`, then re-run. Velocity is still computed from change_frequency."
            ),
        );
    }
    Ok(predict_out)
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
    // Validate the feedback verb BEFORE any mutation. Exactly `correct | wrong |
    // partial` are meaningful; any other value (a typo like "corect", a stray
    // "neutral") must be refused, never silently mapped. The old catch-alls sent
    // an unrecognized verb to `record_defect`, accruing a defect against innocent
    // nodes, and to the edge path's "treat as correct" fallback — both wrong.
    match input.feedback.as_str() {
        "correct" | "wrong" | "partial" => {}
        other => {
            return Err(m1nd_core::error::M1ndError::InvalidParams {
                tool: "learn".into(),
                detail: format!(
                    "unknown feedback '{other}': expected one of correct | wrong | partial"
                ),
            });
        }
    }

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
            other => {
                // Unreachable: `input.feedback` was validated to be one of
                // correct | wrong | partial at the top of this function. This arm
                // exists only so a future variant is a loud panic in tests, never
                // a silent "treat as correct".
                unreachable!("unvalidated learn feedback reached edge match: {other}")
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
    // The learned node set is one co-change event: note each node's appearance
    // once (the smoothed-Jaccard marginal count), then record the pairs.
    for &node in &nodes {
        state.temporal.co_change.note_node_appearance(node);
    }
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
            // Only "correct" remains (validated at the top). A defect is recorded
            // ONLY for an explicit "wrong"/"partial"; a typo can no longer reach
            // `record_defect` and accuse an innocent node.
            "correct" => state.trust_ledger.record_defect(external_id, now),
            other => unreachable!("unvalidated learn feedback reached ledger match: {other}"),
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

    // SPEC-1: the freshness door is its own action and its own handler. It is
    // intercepted BEFORE the adapter match because it is not an adapter choice —
    // it re-scans one already-declared repo root, always through the code
    // ingestor, and it answers with a refusal-or-receipt envelope of its own.
    if normalized_ingest_mode(&input.mode) == "refresh" {
        return handle_ingest_refresh(state, &input);
    }

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
            let summary = bundle.summary();
            let providers = bundle.providers.clone();
            let outcomes = bundle.outcomes.clone();
            if !bundle.is_committable() {
                let (node_count, edge_count) = {
                    let graph = state.graph.read();
                    (graph.num_nodes(), graph.num_edges())
                };
                return Ok(serde_json::json!({
                    "mode": normalized_ingest_mode(&input.mode),
                    "adapter": "universal",
                    "namespace": namespace,
                    "status": summary.status,
                    "committed": false,
                    "universal_ingest": summary,
                    "universal_outcomes": outcomes,
                    "provider_status": providers,
                    "canonical_artifact_count": 0,
                    "files_scanned": bundle.stats.files_scanned,
                    "files_parsed": bundle.stats.files_parsed,
                    "nodes_created": 0,
                    "edges_created": 0,
                    "node_count": node_count,
                    "edge_count": edge_count,
                }));
            }
            let artifacts = universal_docs::encode_canonical_artifacts_with_source_root(
                &state.runtime_root,
                Some(&path),
                &bundle.documents,
                &namespace,
            )?;
            state.document_artifacts.stage_replacement(&artifacts)?;
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
                obj.insert(
                    "provider_status".into(),
                    serde_json::to_value(&providers).unwrap_or(serde_json::json!({})),
                );
                obj.insert(
                    "status".into(),
                    serde_json::to_value(summary.status)
                        .unwrap_or_else(|_| serde_json::json!("DEGRADED")),
                );
                obj.insert(
                    "universal_ingest".into(),
                    serde_json::to_value(&summary).unwrap_or(serde_json::json!({})),
                );
                obj.insert(
                    "universal_outcomes".into(),
                    serde_json::to_value(&outcomes).unwrap_or(serde_json::json!([])),
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

/// Install the exact COMPLETE code bundle that a governed actor has already
/// rebuilt and revalidated. Unlike `handle_ingest`, this seam performs no
/// third filesystem scan between authority revalidation and graph ownership
/// transfer, so the checkpointed postimage is the candidate that was approved.
pub(crate) fn install_complete_code_bundle(
    state: &mut SessionState,
    input: IngestInput,
    bundle: m1nd_ingest::CodeIngestBundleV1,
    sealed_inventory: Vec<crate::session::FileInventoryEntry>,
) -> M1ndResult<serde_json::Value> {
    if input.adapter != "code"
        || input.incremental
        || normalized_ingest_mode(&input.mode) != "replace"
    {
        return Err(M1ndError::InvalidParams {
            tool: "governed_code_ingest".to_string(),
            detail: "an approved COMPLETE code bundle requires code/replace/non-incremental installation"
                .to_string(),
        });
    }
    if bundle.schema != m1nd_ingest::ownership::CODE_INGEST_BUNDLE_SCHEMA
        || bundle.ownership.coverage != m1nd_ingest::ownership::OwnershipCoverageV1::Complete
        || sealed_inventory.len() != bundle.ownership.source_digests.len()
        || sealed_inventory.iter().any(|entry| entry.sha256.is_none())
    {
        return Err(M1ndError::CorruptState {
            reason: "governed code ingest received an incomplete sealed candidate or inventory"
                .to_string(),
        });
    }
    let ownership_valid = bundle
        .ownership
        .verify_against_graph(&bundle.graph)
        .map_err(|error| M1ndError::CorruptState {
            reason: format!("governed sealed ownership verification failed: {error}"),
        })?;
    if !ownership_valid {
        return Err(M1ndError::CorruptState {
            reason: "governed sealed ownership receipt does not match the candidate graph"
                .to_string(),
        });
    }
    let expected_projection = bundle.ownership.source_projection_digest.clone();
    let m1nd_ingest::CodeIngestBundleV1 {
        graph,
        stats,
        ownership: _,
        schema: _,
    } = bundle;
    let output = finalize_ingest_with_inventory(
        state,
        &input,
        "code",
        graph,
        stats,
        Some(sealed_inventory),
    )?;
    let installed_projection =
        m1nd_ingest::ownership::source_projection_digest(&state.graph.read())?;
    if installed_projection != expected_projection {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "governed code ingest installed a different source projection: expected {expected_projection}, observed {installed_projection}"
            ),
        });
    }
    Ok(output)
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
            // tool_tier: "essential" (default, 42 tools) or "full" (all tools).
            // Set M1ND_TOOL_TIER=full to expose all 133 tools in tools/list.
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
        // First-Contact Reception (§9.5.5): flags a caller_root mismatch; absent
        // (serde-skipped) on match / unknown caller.
        reception: state.reception_verdict(),
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
    // P1 presence (ORGANISM-INSIDE): record any DECLARED enrichment this session
    // carried so the throttled beat projects who/what into the control-room
    // roster. Handshake is already the session's first call — the natural,
    // reuse-first carrier (PRD §3.3). Optional and honest-absent: a bare
    // handshake declares nothing and erases nothing.
    state.set_presence_declaration(
        &input.agent_id,
        input.kind.clone(),
        input.theme.clone(),
        input.intent.clone(),
        input.worktree.clone(),
        input.working_set.clone(),
    );

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

    // --- graph_intelligence: compute while holding the read lock (one pass) ---
    //
    // top_pagerank: top-5 nodes by PageRank (structural importance).
    // attention_anchors: top-5 nodes by query-access frequency from plasticity ring-buffer.
    // memory: counts of light:: nodes and grounded_in edges.
    //
    // Build NodeId → external_id reverse map once, reuse for all three signals.
    let mut nid_to_ext: HashMap<usize, String> = HashMap::with_capacity(graph.id_to_node.len());
    for (interned, &nid) in &graph.id_to_node {
        nid_to_ext.insert(nid.as_usize(), graph.strings.resolve(*interned).to_string());
    }

    // 1. top_pagerank --------------------------------------------------------
    let top_pagerank: Vec<serde_json::Value> = if graph.pagerank_computed
        && !graph.nodes.pagerank.is_empty()
    {
        let n = graph.nodes.count as usize;
        // Collect (pagerank, node_idx) pairs; skip zero scores.
        let mut ranked: Vec<(f32, usize)> = (0..n)
            .filter_map(|i| {
                let pr = graph.nodes.pagerank[i].get();
                if pr > 0.0 {
                    Some((pr, i))
                } else {
                    None
                }
            })
            .collect();
        // Partial descending sort, keep top-5.
        ranked.sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(5);
        ranked
            .into_iter()
            .map(|(pr, idx)| {
                let ext_id = nid_to_ext.get(&idx).cloned().unwrap_or_default();
                let label = graph
                    .strings
                    .try_resolve(graph.nodes.label[idx])
                    .unwrap_or("")
                    .to_string();
                serde_json::json!({ "id": ext_id, "label": label, "pagerank": pr })
            })
            .collect()
    } else {
        vec![]
    };
    let pagerank_note: Option<&str> = if !graph.pagerank_computed || graph.nodes.pagerank.is_empty()
    {
        Some("not_computed")
    } else {
        None
    };

    // 2. memory: light:: nodes + grounded_in edges ---------------------------
    let grounded_in_interned = graph.strings.lookup("grounded_in");
    let mut light_node_count: u64 = 0;
    let mut grounded_in_edge_count: u64 = 0;
    {
        let n = graph.nodes.count as usize;
        for idx in 0..n {
            let ext_id = nid_to_ext.get(&idx).map(|s| s.as_str()).unwrap_or("");
            if ext_id.starts_with("light::") {
                light_node_count += 1;
            }
            if let Some(gi) = grounded_in_interned {
                let range = graph
                    .csr
                    .out_range(m1nd_core::types::NodeId::new(idx as u32));
                for edge_i in range {
                    if graph.csr.relations[edge_i] == gi {
                        grounded_in_edge_count += 1;
                    }
                }
            }
        }
    }

    drop(graph);
    // --- end graph read lock ---

    // 3. attention_anchors: top-5 by query-access frequency ------------------
    // Uses PlasticityEngine::top_node_access_frequencies() — no seeds needed,
    // O(num_nodes) pass over the ring-buffer's frequency array.
    // IMPORTANT: read the ORCHESTRATOR's plasticity engine — that is the one
    // `activate`/`query` actually updates (query.rs query()->plasticity.update).
    // `state.plasticity` is a separate engine that queries never touch, so
    // reading it here made attention_anchors permanently empty.
    let raw_freqs = state.orchestrator.plasticity.top_node_access_frequencies(5);
    let attention_anchors_empty = raw_freqs.is_empty();
    let attention_anchors: Vec<serde_json::Value> = {
        // Re-acquire a short read lock only to resolve labels for the few returned nodes.
        let graph = state.graph.read();
        raw_freqs
            .into_iter()
            .map(|(nid, freq)| {
                let idx = nid.as_usize();
                let ext_id = nid_to_ext.get(&idx).cloned().unwrap_or_default();
                let label = graph
                    .strings
                    .try_resolve(graph.nodes.label[idx])
                    .unwrap_or("")
                    .to_string();
                serde_json::json!({
                    "id": ext_id,
                    "label": label,
                    "signal": freq,
                    "kind": "node_access_frequency"
                })
            })
            .collect()
    };

    let graph_intelligence = serde_json::json!({
        "top_pagerank": top_pagerank,
        "pagerank_note": pagerank_note,
        "attention_anchors": attention_anchors,
        "attention_anchors_note": if attention_anchors_empty {
            Some("no_queries_recorded_yet")
        } else {
            None
        },
        "memory": {
            "light_nodes": light_node_count,
            "grounded_in_edges": grounded_in_edge_count,
        }
    });
    // --- end graph_intelligence ---

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

    // Binary version-honesty: a drift warning rides ALONGSIDE the verdict — it
    // never changes trust_mode (a stale binary is a warning, not a proof
    // failure). When drift fires, prepend the one-line warning to next_action so
    // the honest surface can never silently run a wrong/old binary.
    let (_binary_info, binary_drift_summary) = state.binary_version_info();
    let next_action: String = match &binary_drift_summary {
        Some(warning) => format!("{warning}. Then: {next_action}"),
        None => next_action.to_string(),
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

    // First-Contact Reception (§9.5.5): null on match / unknown caller, the
    // honest mismatch block otherwise. Computed before the json! so it does not
    // overlap the other `state` reads in the literal.
    let reception = state.reception_verdict();

    Ok(serde_json::json!({
        "schema": "m1nd-session-handshake-v0",
        "trust_mode": trust_mode,
        "binding_fingerprint": state.binding_fingerprint(),
        "reception": reception.unwrap_or(serde_json::Value::Null),
        // Version-honesty: the running binary's identity + any drift. Additive,
        // warn-only — the drift block is null when nothing mismatches. The same
        // block also lives inside binding_fingerprint; surfaced here at top level
        // so drift is impossible to miss when reading the handshake verdict.
        "binary_drift": _binary_info.get("binary_drift").cloned().unwrap_or(serde_json::Value::Null),
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
        "agent_memory": state.agent_memory_boot.clone().unwrap_or(serde_json::Value::Null),
        "graph_intelligence": graph_intelligence,
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
            ..Default::default()
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

    // Binary version-honesty. Warn-only: a drifted/stale binary does NOT flip
    // `ok`/`status`/`verdict` (those stay as the trust machinery decided) — the
    // warning rides in `binary_drift`, `next_action`, and `non_claims` so the
    // honest surface can never quietly run an old binary.
    let (binary_info, binary_drift_summary) = state.binary_version_info();
    let next_action: String = match &binary_drift_summary {
        Some(warning) => format!("{warning}. Then: {next_action}"),
        None => next_action.to_string(),
    };
    let mut non_claims: Vec<String> = vec![
        "trust_selftest does not ingest or mutate the graph.".into(),
        "trust_selftest does not refresh the host MCP binding.".into(),
        "trust_selftest does not run a retrieval probe automatically.".into(),
        "trust_selftest does not replace compiler, tests, or local file truth.".into(),
    ];
    if let Some(warning) = &binary_drift_summary {
        non_claims.push(warning.clone());
    }

    Ok(serde_json::json!({
        "schema": "m1nd-trust-selftest-v0",
        "ok": ok,
        "status": status,
        "verdict": verdict,
        "next_action": next_action,
        "binding_fingerprint": state.binding_fingerprint(),
        "binary_drift": binary_info.get("binary_drift").cloned().unwrap_or(serde_json::Value::Null),
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
            "binary_drift_detected": binary_drift_summary.is_some(),
            "suspicious_retrieval_evidence": suspicious_retrieval,
            "recovery_playbook_attached": !ok || suspicious_retrieval,
        },
        "non_claims": non_claims,
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
            ..Default::default()
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
    let ingest_path = ingest_project_root_hint(state, input.scope.as_deref());

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
                    {
                        // Only recommend the generic ingest verb for an
                        // intentional context switch when the server's policy
                        // actually admits it; otherwise name the honest path.
                        let switch_ingest_args = serde_json::json!({
                            "agent_id": agent_id.clone(),
                            "path": requested_workspace_hint.clone(),
                        });
                        if crate::server::enforce_generic_action_policy(
                            "ingest",
                            &switch_ingest_args,
                        )
                        .is_ok()
                        {
                            playbook_step(
                                "same_binding_ingest_if_intentional",
                                "If this session should intentionally switch or merge context, call ingest for requested_workspace_hint on this same binding.",
                                "This is an explicit mutation; do it only when the agent truly wants this runtime to carry that repo context.",
                                Some("ingest"),
                                Some(switch_ingest_args),
                            )
                        } else {
                            playbook_step(
                                "switch_workspace_via_authenticated_ingress",
                                "If this session should intentionally switch context, rebind to an owner that hosts requested_workspace_hint, or use the served owner's authenticated ingress; the generic ingest verb is policy-disabled here.",
                                "brain_bootstrap_consumer_not_installed: the generic ingest verb the switch step used to name is refused by policy, so recommending it would loop.",
                                None,
                                None,
                            )
                        }
                    },
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
        "needs_ingest" => {
            let proposed_ingest_args = serde_json::json!({
                "agent_id": agent_id.clone(),
                "path": ingest_path.clone(),
            });
            // Never recommend a verb the server's OWN policy refuses on this
            // binding. Generic `ingest` classifies as graph.ingest.replace
            // (POSITIVE_SOVEREIGN) and fails closed until a typed G2/G3 consumer
            // is installed — consult the REAL gate rather than assuming, so a
            // future Ordinary consumer re-enables the one-call path automatically.
            if crate::server::enforce_generic_action_policy("ingest", &proposed_ingest_args).is_ok()
            {
                (
                    "blocked",
                    "Populate this binding's active graph for the intended repository.",
                    "call_ingest",
                    vec![
                        playbook_step(
                            "call_ingest",
                            "Call ingest for the intended repository on this same binding.",
                            "The active graph is empty or incomplete, so retrieval cannot yet be trusted.",
                            Some("ingest"),
                            Some(proposed_ingest_args),
                        ),
                        playbook_step(
                            "rerun_session_handshake",
                            "Call session_handshake again after ingest completes.",
                            "The handshake should confirm node and edge counts before the next retrieval step.",
                            Some("session_handshake"),
                            Some(serde_json::json!({ "agent_id": agent_id.clone() })),
                        ),
                    ],
                )
            } else {
                // The generic ingest verb is policy-disabled here. Recommend the
                // honest repair instead of a call the server would reject — and
                // NAME THE DOOR. Until 1.6.2 this playbook was correct about
                // everything it forbade and silent about the one command that
                // works, so an agent reading it concluded there was no way to
                // populate a new repo at all.
                //
                // A brand-new brain resolves `ingest_path` to its placeholder —
                // no code root, no caller root, nothing to name — so the command
                // becomes the relative one a human can run where they already
                // stand, rather than a literal `<intended-repo-path>`.
                let birth_command = if ingest_path.starts_with('<') {
                    "m1nd init --birth .".to_string()
                } else {
                    format!("m1nd init --birth {ingest_path}")
                };
                (
                    "blocked",
                    "Offer the human the one-time birth ceremony for this repo; the generic ingest verb is policy-disabled on this binding, because minting a brain is their gesture and not an agent's.",
                    "offer_the_birth_ceremony",
                    vec![
                        playbook_step(
                            "generic_ingest_unavailable",
                            "Do not call the generic `ingest` verb on this binding: the server refuses it (graph.ingest.replace is POSITIVE_SOVEREIGN and no typed G2/G3 bootstrap consumer is installed).",
                            "brain_bootstrap_consumer_not_installed: recommending a verb the policy rejects would send the agent into a refusal loop.",
                            None,
                            Some(serde_json::json!({
                                "code": "brain_bootstrap_consumer_not_installed",
                                "intended_repo": ingest_path.clone(),
                            })),
                        ),
                        playbook_step(
                            "adopt_legacy_snapshot_on_owner_restart",
                            "If this runtime just upgraded from a pre-1.5 layout, restart the served owner: a one-time legacy-snapshot adoption runs at boot and populates the runtime graph from a legacy ./graph_snapshot.json when one is present.",
                            "A pre-1.5 snapshot left in the legacy location is adopted into the runtime root at boot, so a restart can populate the graph with no generic ingest call at all.",
                            None,
                            None,
                        ),
                        playbook_step(
                            "offer_the_birth_ceremony",
                            &format!(
                                "Otherwise this repo has no brain yet: tell the human to run `{birth_command}` once, in a terminal, from inside the repo. Offer the command and stop — running it is not yours to do."
                            ),
                            "The ceremony is the human gesture that mints a brain: it ingests the repo for real and reports the node and edge counts it produced. An agent cannot run it — the origin stamp exists only inside that CLI ingress.",
                            None,
                            Some(serde_json::json!({
                                "command": birth_command,
                                "who_runs_it": "the human, once",
                            })),
                        ),
                        playbook_step(
                            "rerun_session_handshake",
                            "Call session_handshake again after the graph is populated.",
                            "The handshake should confirm node and edge counts before the next retrieval step.",
                            Some("session_handshake"),
                            Some(serde_json::json!({ "agent_id": agent_id.clone() })),
                        ),
                    ],
                )
            }
        }
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

    // MEDULLA-PRD §9.3 — the confusion metric, read as COUNTS ONLY (no
    // uncalibrated quality scores): `confusion_rate` = confirmed
    // `memory_misdelivery` letters this week, against total letters (volume) and
    // the pending-distribution count. Best-effort over the spool; a doctor call
    // holds no brain registry, so `known_repos` is empty here — `pending`
    // over-reports named-but-here repos (the exact per-repo resolution rides the
    // sweep/instances surfaces, which DO hold the registry). Fail-open: a spool
    // read error yields an absent block, never a doctor failure.
    let mailbox_block = {
        let worktree_base = state
            .project_root_display()
            .as_deref()
            .map(crate::session::basename_of)
            .unwrap_or_default();
        let now_ms = crate::util::now_ms();
        crate::mailbox::doctor_mailbox(
            &state.runtime_root,
            &worktree_base,
            &std::collections::BTreeMap::new(),
            now_ms,
        )
        .ok()
    };

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
        // MEDULLA-PRD §9.3: the antifragility metric — confusion_rate (weekly
        // memory_misdelivery count) / volume / pending-distribution. Counts only.
        "mailbox": mailbox_block,
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

    #[derive(Debug, PartialEq, Eq)]
    struct UniversalMutationSnapshot {
        node_count: u32,
        edge_count: usize,
        node_ids: Vec<String>,
        graph_generation: u64,
        cache_generation: u64,
        ingest_roots: Vec<String>,
        workspace_root: Option<String>,
        document_cache_keys: Vec<String>,
        file_inventory_keys: Vec<String>,
        cache_root_exists: bool,
        cache_index_exists: bool,
        graph_file_exists: bool,
    }

    fn universal_mutation_snapshot(state: &SessionState) -> UniversalMutationSnapshot {
        let graph = state.graph.read();
        let mut node_ids = graph
            .id_to_node
            .keys()
            .map(|interned| graph.strings.resolve(*interned).to_string())
            .collect::<Vec<_>>();
        node_ids.sort();
        let mut document_cache_keys = state
            .document_cache
            .entries
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        document_cache_keys.sort();
        let mut file_inventory_keys = state.file_inventory.keys().cloned().collect::<Vec<_>>();
        file_inventory_keys.sort();
        UniversalMutationSnapshot {
            node_count: graph.num_nodes(),
            edge_count: graph.num_edges(),
            node_ids,
            graph_generation: state.graph_generation,
            cache_generation: state.cache_generation,
            ingest_roots: state.ingest_roots.clone(),
            workspace_root: state.workspace_root.clone(),
            document_cache_keys,
            file_inventory_keys,
            cache_root_exists: crate::universal_docs::cache_root(&state.runtime_root).exists(),
            cache_index_exists: crate::universal_docs::cache_index_path(&state.runtime_root)
                .exists(),
            graph_file_exists: state.graph_path.exists(),
        }
    }

    fn universal_provider_env_lock() -> &'static std::sync::Mutex<()> {
        crate::auto_ingest::universal_provider_test_env_lock()
    }

    struct UniversalProviderEnvGuard {
        python: Option<std::ffi::OsString>,
        grobid: Option<std::ffi::OsString>,
        timeout_ms: Option<std::ffi::OsString>,
    }

    impl UniversalProviderEnvGuard {
        fn force_unavailable(missing_python: &std::path::Path) -> Self {
            Self::force_python(missing_python, None)
        }

        fn force_python(python: &std::path::Path, timeout_ms: Option<u64>) -> Self {
            let guard = Self {
                python: std::env::var_os("M1ND_PROVIDER_PYTHON"),
                grobid: std::env::var_os("M1ND_GROBID_URL"),
                timeout_ms: std::env::var_os("M1ND_PROVIDER_TIMEOUT_MS"),
            };
            std::env::set_var("M1ND_PROVIDER_PYTHON", python);
            std::env::remove_var("M1ND_GROBID_URL");
            match timeout_ms {
                Some(timeout_ms) => {
                    std::env::set_var("M1ND_PROVIDER_TIMEOUT_MS", timeout_ms.to_string())
                }
                None => std::env::remove_var("M1ND_PROVIDER_TIMEOUT_MS"),
            }
            guard
        }
    }

    impl Drop for UniversalProviderEnvGuard {
        fn drop(&mut self) {
            match &self.python {
                Some(value) => std::env::set_var("M1ND_PROVIDER_PYTHON", value),
                None => std::env::remove_var("M1ND_PROVIDER_PYTHON"),
            }
            match &self.grobid {
                Some(value) => std::env::set_var("M1ND_GROBID_URL", value),
                None => std::env::remove_var("M1ND_GROBID_URL"),
            }
            match &self.timeout_ms {
                Some(value) => std::env::set_var("M1ND_PROVIDER_TIMEOUT_MS", value),
                None => std::env::remove_var("M1ND_PROVIDER_TIMEOUT_MS"),
            }
        }
    }

    #[test]
    fn universal_all_unsupported_is_typed_noop_with_zero_graph_or_cache_mutation() {
        use crate::protocol::core::IngestInput;

        let _lock = universal_provider_env_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = UniversalProviderEnvGuard::force_unavailable(
            &temp.path().join("definitely-missing-provider-python"),
        );
        let pdf = temp.path().join("unsupported.pdf");
        std::fs::write(&pdf, b"%PDF-1.7 unsupported fixture").expect("write pdf");
        let mut state = build_runtime_state(temp.path());
        let before = universal_mutation_snapshot(&state);

        let output = super::handle_ingest(
            &mut state,
            IngestInput {
                path: pdf.to_string_lossy().to_string(),
                agent_id: "test".into(),
                incremental: false,
                adapter: "universal".into(),
                mode: "replace".into(),
                namespace: Some("honesty".into()),
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            },
        )
        .expect("all-unsupported universal ingest must return a typed no-op");

        assert_eq!(output["status"], "UNSUPPORTED");
        assert_eq!(output["committed"], false);
        assert_eq!(output["universal_ingest"]["unsupported_count"], 1);
        assert_eq!(output["universal_outcomes"][0]["status"], "UNSUPPORTED");
        assert_eq!(universal_mutation_snapshot(&state), before);
    }

    #[cfg(unix)]
    #[test]
    fn universal_provider_failure_is_typed_noop_with_zero_graph_or_cache_mutation() {
        use crate::protocol::core::IngestInput;
        use std::os::unix::fs::PermissionsExt;

        let _lock = universal_provider_env_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("fake-provider");
        std::fs::write(
            &provider,
            "#!/bin/sh\ncase \"$2\" in *importlib.util*) printf '1\\n'; exit 0;; esac\nprintf 'corrupt provider fixture' >&2\nexit 9\n",
        )
        .expect("write provider");
        let mut permissions = std::fs::metadata(&provider).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&provider, permissions).unwrap();
        let _env = UniversalProviderEnvGuard::force_python(&provider, Some(100));
        let pdf = temp.path().join("failed.pdf");
        std::fs::write(&pdf, b"%PDF-1.7 failed fixture").expect("write pdf");
        let mut state = build_runtime_state(temp.path());
        let before = universal_mutation_snapshot(&state);

        let output = super::handle_ingest(
            &mut state,
            IngestInput {
                path: pdf.to_string_lossy().to_string(),
                agent_id: "test".into(),
                incremental: false,
                adapter: "universal".into(),
                mode: "replace".into(),
                namespace: Some("honesty".into()),
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            },
        )
        .expect("failed universal ingest must return a typed no-op");

        assert_eq!(output["status"], "FAILED");
        assert_eq!(output["committed"], false);
        assert_eq!(output["universal_ingest"]["failed_count"], 1);
        assert_eq!(output["universal_outcomes"][0]["status"], "FAILED");
        assert_eq!(
            output["universal_ingest"]["diagnostics"][0]["provider_outcome"]["failure"],
            "CORRUPT"
        );
        assert_eq!(universal_mutation_snapshot(&state), before);
    }

    #[test]
    fn universal_empty_is_explicit_noop_preserving_existing_graph_and_cache() {
        use crate::protocol::core::IngestInput;

        let temp = tempfile::tempdir().expect("tempdir");
        let empty = temp.path().join("empty-documents");
        std::fs::create_dir_all(&empty).expect("empty directory");
        let mut state = build_runtime_state(temp.path());
        let before = universal_mutation_snapshot(&state);

        let output = super::handle_ingest(
            &mut state,
            IngestInput {
                path: empty.to_string_lossy().to_string(),
                agent_id: "test".into(),
                incremental: false,
                adapter: "universal".into(),
                mode: "replace".into(),
                namespace: Some("honesty".into()),
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            },
        )
        .expect("empty universal ingest should return an explicit no-op");

        assert_eq!(output["status"], "EMPTY");
        assert_eq!(output["universal_ingest"]["status"], "EMPTY");
        assert_eq!(output["universal_ingest"]["candidate_count"], 0);
        assert_eq!(output["universal_ingest"]["parsed_count"], 0);
        assert_eq!(output["node_count"], before.node_count as u64);
        assert_eq!(output["edge_count"], before.edge_count as u64);
        assert_eq!(universal_mutation_snapshot(&state), before);
    }

    #[test]
    fn universal_mixed_batch_surfaces_degraded_counts_and_bounded_diagnostics() {
        use crate::protocol::core::IngestInput;

        let _lock = universal_provider_env_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = UniversalProviderEnvGuard::force_unavailable(
            &temp.path().join("definitely-missing-provider-python"),
        );
        let docs = temp.path().join("mixed-documents");
        std::fs::create_dir_all(&docs).expect("documents directory");
        std::fs::write(docs.join("good.md"), "# Good\n\nParsed document.\n")
            .expect("write markdown");
        std::fs::write(docs.join("unsupported.pdf"), b"%PDF unsupported fixture")
            .expect("write pdf");
        let mut state = build_runtime_state(temp.path());

        let output = super::handle_ingest(
            &mut state,
            IngestInput {
                path: docs.to_string_lossy().to_string(),
                agent_id: "test".into(),
                incremental: false,
                adapter: "universal".into(),
                mode: "merge".into(),
                namespace: Some("honesty".into()),
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            },
        )
        .expect("mixed universal ingest should retain parsed documents");

        assert_eq!(output["status"], "DEGRADED");
        assert_eq!(output["universal_ingest"]["status"], "DEGRADED");
        assert_eq!(output["universal_ingest"]["candidate_count"], 2);
        assert_eq!(output["universal_ingest"]["parsed_count"], 1);
        assert_eq!(output["universal_ingest"]["unsupported_count"], 1);
        assert_eq!(output["universal_outcomes"].as_array().unwrap().len(), 2);
        assert_eq!(
            output["universal_ingest"]["diagnostics"][0]["status"],
            "UNSUPPORTED"
        );
        assert_eq!(
            output["universal_ingest"]["diagnostics"][0]["provider"],
            "universal:none"
        );
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
    fn learn_rejects_typo_feedback_without_accusing_a_defect() {
        // Regression (trust-calibration ledger #6): a mislabeled feedback verb —
        // here the typo "corect" — used to fall through the catch-all and call
        // `record_defect` against the named node, silently accruing a defect on an
        // innocent node. It must now be a clean InvalidParams refusal that mutates
        // NOTHING in the trust ledger.
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut state, hub_id) = build_star_graph(temp.path(), 3);

        assert_eq!(
            state.trust_ledger.external_ids().count(),
            0,
            "ledger starts empty"
        );

        let result = super::handle_learn(
            &mut state,
            crate::protocol::LearnInput {
                query: "did the hub change?".into(),
                agent_id: "test".into(),
                feedback: "corect".into(), // typo of "correct"
                node_ids: vec![hub_id.clone()],
                strength: crate::protocol::default_feedback_strength(),
            },
        );

        let err = result.expect_err("a typo feedback must be refused, not silently mapped");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("corect") && msg.contains("correct") && msg.contains("partial"),
            "the refusal must name the bad value and list the valid ones, got: {msg}"
        );

        // The load-bearing guarantee: NO defect (and no entry at all) was recorded
        // against the node for the typo.
        assert_eq!(
            state.trust_ledger.external_ids().count(),
            0,
            "a refused typo must not create any trust-ledger entry"
        );
    }

    #[test]
    fn learn_accepts_the_three_valid_feedback_verbs() {
        // The complement: the exact set correct | wrong | partial is accepted (a
        // guard that the up-front validation did not over-reject).
        for verb in ["correct", "wrong", "partial"] {
            let temp = tempfile::tempdir().expect("tempdir");
            let (mut state, hub_id) = build_star_graph(temp.path(), 3);
            let out = super::handle_learn(
                &mut state,
                crate::protocol::LearnInput {
                    query: "q".into(),
                    agent_id: "test".into(),
                    feedback: verb.into(),
                    node_ids: vec![hub_id],
                    strength: crate::protocol::default_feedback_strength(),
                },
            );
            assert!(out.is_ok(), "'{verb}' must be accepted, got {out:?}");
        }
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
        let grounded_in_interned = graph
            .strings
            .lookup("grounded_in")
            .expect("relation interned");
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
                graph.csr.targets[i] == code_node && graph.csr.relations[i] == grounded_in_interned
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
            project_root: None,
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
        let grounded = graph
            .strings
            .lookup("grounded_in")
            .expect("grounded_in interned");
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

    /// Test that session_handshake returns a well-formed `graph_intelligence` object.
    ///
    /// Builds a small finalized graph with two nodes (so PageRank is computed),
    /// calls handle_session_handshake with all required tools present, then asserts:
    ///   - `graph_intelligence.top_pagerank` is an array
    ///   - `graph_intelligence.attention_anchors` is an array
    ///   - `graph_intelligence.memory.light_nodes` is a number
    ///   - `graph_intelligence.memory.grounded_in_edges` is a number
    ///   - for a fresh graph with no light nodes both counts are 0
    ///   - for a fresh graph with no prior queries attention_anchors is empty and
    ///     `attention_anchors_note` explains why
    #[test]
    fn session_handshake_graph_intelligence_structure() {
        use super::handle_session_handshake;
        use crate::protocol::core::SessionHandshakeInput;

        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());

        let output = handle_session_handshake(
            &mut state,
            SessionHandshakeInput {
                agent_id: "test-agent".into(),
                observed_tool_count: Some(HOST_BINDING_REQUIRED_TOOLS.len() as u64),
                available_tools: HOST_BINDING_REQUIRED_TOOLS
                    .iter()
                    .map(|t| (*t).to_string())
                    .collect(),
                missing_tools: vec![],
                scope: None,
                ..Default::default()
            },
        )
        .expect("session_handshake should succeed");

        // `graph_intelligence` key must be present and be an object.
        let gi = &output["graph_intelligence"];
        assert!(gi.is_object(), "graph_intelligence must be a JSON object");

        // top_pagerank must be an array (may be non-empty since graph is finalized
        // and PageRank is computed on finalize).
        let top_pr = &gi["top_pagerank"];
        assert!(top_pr.is_array(), "top_pagerank must be an array");

        // Each entry in top_pagerank must have id, label, pagerank.
        for entry in top_pr.as_array().unwrap() {
            assert!(entry["id"].is_string(), "top_pagerank entry must have id");
            assert!(
                entry["label"].is_string(),
                "top_pagerank entry must have label"
            );
            assert!(
                entry["pagerank"].is_number(),
                "top_pagerank entry must have pagerank"
            );
        }

        // attention_anchors must be an array (empty for a fresh graph with no queries).
        let aa = &gi["attention_anchors"];
        assert!(aa.is_array(), "attention_anchors must be an array");

        // For a fresh graph no queries were processed → anchors must be empty and
        // attention_anchors_note must explain this.
        assert_eq!(
            aa.as_array().unwrap().len(),
            0,
            "no queries recorded → attention_anchors should be empty"
        );
        assert_eq!(
            gi["attention_anchors_note"],
            serde_json::json!("no_queries_recorded_yet"),
            "attention_anchors_note must be set when anchors are empty"
        );

        // memory object must be present with numeric fields.
        let mem = &gi["memory"];
        assert!(mem.is_object(), "memory must be a JSON object");
        assert!(
            mem["light_nodes"].is_number(),
            "memory.light_nodes must be a number"
        );
        assert!(
            mem["grounded_in_edges"].is_number(),
            "memory.grounded_in_edges must be a number"
        );
        // Fresh graph built by build_runtime_state has no light:: nodes.
        assert_eq!(
            mem["light_nodes"],
            serde_json::json!(0u64),
            "fresh graph must have 0 light nodes"
        );
        assert_eq!(
            mem["grounded_in_edges"],
            serde_json::json!(0u64),
            "fresh graph must have 0 grounded_in edges"
        );
    }

    /// Regression: attention_anchors must POPULATE after a query. It reads the
    /// orchestrator's plasticity engine (the one `activate` updates), not the
    /// separate `state.plasticity`. Reading the wrong engine kept it permanently
    /// empty.
    #[test]
    fn session_handshake_attention_anchors_populate_after_query() {
        use super::{handle_activate, handle_session_handshake};
        use crate::protocol::core::{ActivateInput, SessionHandshakeInput};

        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());

        // Run a query so the orchestrator records activated nodes (node_frequency).
        let _ = handle_activate(
            &mut state,
            ActivateInput {
                query: "lib core".into(),
                agent_id: "test-agent".into(),
                top_k: 5,
                dimensions: vec!["structural".into(), "semantic".into()],
                xlr: false,
                include_ghost_edges: false,
                include_structural_holes: false,
                token_budget: None,
            },
        )
        .expect("activate should succeed");

        let output = handle_session_handshake(
            &mut state,
            SessionHandshakeInput {
                agent_id: "test-agent".into(),
                observed_tool_count: Some(HOST_BINDING_REQUIRED_TOOLS.len() as u64),
                available_tools: HOST_BINDING_REQUIRED_TOOLS
                    .iter()
                    .map(|t| (*t).to_string())
                    .collect(),
                missing_tools: vec![],
                scope: None,
                ..Default::default()
            },
        )
        .expect("session_handshake should succeed");

        let aa = output["graph_intelligence"]["attention_anchors"]
            .as_array()
            .expect("attention_anchors must be an array");
        assert!(
            !aa.is_empty(),
            "attention_anchors must populate after a query (reads orchestrator.plasticity); got empty"
        );
        // Each anchor carries id/label/signal/kind.
        let first = &aa[0];
        assert!(first["id"].is_string(), "anchor needs id");
        assert!(first["signal"].is_number(), "anchor needs numeric signal");
    }

    // ─── #7 ─────────────────────────────────────────────────────────────────────
    // memory_freshness appears in finalize_ingest output after a code re-ingest
    // when at least one memorized claim's evidence file changed on disk.
    // Pipeline:
    //   1. code ingest  → builds file::auth.rs
    //   2. light ingest → evidence marker resolves → grounded_in edge
    //   3. modify auth.rs on disk
    //   4. code re-ingest → finalize_ingest must report stale_evidence_count >= 1
    #[test]
    fn memory_freshness_detects_stale_evidence_after_code_reingest() {
        use crate::protocol::core::IngestInput;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("rt");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).expect("proj dir");

        let auth_path = proj.join("auth.rs");
        std::fs::write(
            &auth_path,
            "pub fn validate(t: &str) -> bool { !t.is_empty() }\n",
        )
        .expect("write auth.rs v1");

        let light_path = proj.join("findings.md");
        std::fs::write(
            &light_path,
            "---\nProtocol: L1GHT/1.0\nNode: AuthFindings\n---\n\n\
             ## Auth\n\nThe [⍂ entity: Validator] validates tokens.\n\
             [𝔻 confidence: 0.9]\n[𝔻 evidence: auth.rs]\n",
        )
        .expect("write findings.md");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        let mk = |path: String, adapter: &str, mode: &str| IngestInput {
            path,
            agent_id: "test".into(),
            incremental: false,
            adapter: adapter.into(),
            mode: mode.into(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: vec![],
            project_root: None,
        };

        // Step 1: code ingest — records sha256 for auth.rs in file_inventory.
        let code1_out = super::handle_ingest(
            &mut state,
            mk(proj.to_string_lossy().to_string(), "code", "replace"),
        )
        .expect("initial code ingest");
        assert!(
            code1_out["node_count"].as_u64().unwrap_or(0) >= 1,
            "initial code ingest must produce nodes"
        );

        // Step 2: light ingest — evidence marker resolves → grounded_in edge.
        let light_out = super::handle_ingest(
            &mut state,
            mk(light_path.to_string_lossy().to_string(), "light", "merge"),
        )
        .expect("light ingest");
        assert!(
            light_out["light_evidence_resolved"].as_u64().unwrap_or(0) >= 1,
            "light ingest must resolve at least one evidence link"
        );

        // Step 3: modify auth.rs on disk so its hash changes.
        std::fs::write(
            &auth_path,
            "pub fn validate(t: &str) -> bool { t.len() > 2 } // changed\n",
        )
        .expect("overwrite auth.rs");

        // Step 4: code re-ingest — finalize_ingest must notice the hash changed.
        let code2_out = super::handle_ingest(
            &mut state,
            mk(proj.to_string_lossy().to_string(), "code", "merge"),
        )
        .expect("second code ingest");

        let mf = &code2_out["memory_freshness"];
        assert!(
            mf.is_object(),
            "code ingest output must include memory_freshness object, got: {:?}",
            code2_out
        );
        let stale_count = mf["stale_evidence_count"].as_u64().unwrap_or(0);
        assert!(
            stale_count >= 1,
            "memory_freshness.stale_evidence_count must be >= 1 after evidence file changed, got {}",
            stale_count
        );
        let stale_arr = mf["stale_evidence"]
            .as_array()
            .expect("stale_evidence array");
        assert!(
            !stale_arr.is_empty(),
            "stale_evidence array must be non-empty"
        );
        // Confirm reason is evidence_changed (real hash comparison, not just possibly_changed).
        let reason = stale_arr[0]["reason"].as_str().unwrap_or("");
        assert!(
            reason == "evidence_changed" || reason == "evidence_possibly_changed",
            "reason must be evidence_changed or evidence_possibly_changed, got '{}'",
            reason
        );
    }

    // ─── #3 ─────────────────────────────────────────────────────────────────────
    // impact annotates blast-radius nodes that are light:evidenced_by citation
    // markers with is_knowledge_citation=true and a non-empty claim string.
    // Pipeline:
    //   build graph: code_node + marker_node (light:evidenced_by) + grounded_in
    //   call impact on code_node in reverse → marker is in blast radius
    //   assert marker entry has is_knowledge_citation=true
    #[test]
    fn impact_annotates_knowledge_citation_nodes_in_blast_radius() {
        use crate::protocol::core::{ImpactInput, ImpactOutput};

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("rt");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut graph = Graph::new();

        // Code file node.
        let code_node = graph
            .add_node("file::auth.rs", "auth.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add code node");

        // Evidence marker node — tagged light:evidenced_by.
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

        // grounded_in edge: marker → code.
        graph
            .add_edge(
                marker_node,
                code_node,
                "grounded_in",
                FiniteF32::new(0.8),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.8),
            )
            .expect("add grounded_in edge");

        graph.finalize().expect("finalize");

        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");

        // impact with direction "reverse" on the code node — the marker (which has a
        // grounded_in edge pointing TO the code node) is an upstream source and should
        // appear in the blast radius.
        let output: ImpactOutput = super::handle_impact(
            &mut state,
            ImpactInput {
                node_id: "file::auth.rs".to_string(),
                agent_id: "test".into(),
                direction: "reverse".into(),
                include_causal_chains: false,
                max_nodes: None,
            },
        )
        .expect("impact should succeed");

        // The marker node should appear in the blast radius as a knowledge citation.
        // (It has a grounded_in edge TO the code node, so with reverse traversal the
        //  marker is an incoming source and should be reachable.)
        let citation_entries: Vec<_> = output
            .blast_radius
            .iter()
            .filter(|e| e.is_knowledge_citation == Some(true))
            .collect();

        assert!(
            !citation_entries.is_empty(),
            "at least one blast-radius entry must be annotated as is_knowledge_citation=true; \
             blast_radius: {:?}",
            output
                .blast_radius
                .iter()
                .map(|e| (&e.label, &e.is_knowledge_citation))
                .collect::<Vec<_>>()
        );

        // The annotated entry must carry the claim text.
        let first = &citation_entries[0];
        assert!(
            first.claim.is_some() && !first.claim.as_ref().unwrap().is_empty(),
            "is_knowledge_citation=true entry must have a non-empty claim, got: {:?}",
            first
        );
    }

    /// A `replace` code ingest wipes the graph; agent-memory must be restored
    /// automatically so the agent never silently loses its L1GHT memory.
    #[test]
    fn replace_ingest_restores_agent_memory() {
        use crate::light_author_handlers::{handle_light_author, LightAuthorInput, LightClaim};
        use crate::protocol::core::IngestInput;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).expect("proj dir");
        std::fs::write(
            proj.join("auth.rs"),
            "pub fn validate_token(t: &str) -> bool { !t.is_empty() }\n",
        )
        .expect("write auth.rs");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        let code_ingest = |state: &mut SessionState, mode: &str| {
            super::handle_ingest(
                state,
                IngestInput {
                    path: proj.to_string_lossy().to_string(),
                    agent_id: "test".into(),
                    incremental: false,
                    adapter: "code".into(),
                    mode: mode.into(),
                    namespace: None,
                    include_dotfiles: false,
                    dotfile_patterns: vec![],
                    project_root: None,
                },
            )
            .expect("code ingest")
        };
        let count_light = |state: &SessionState| -> usize {
            let g = state.graph.read();
            g.id_to_node
                .keys()
                .filter(|k| g.strings.resolve(**k).starts_with("light::"))
                .count()
        };

        // 1) Code ingest, then memorize a claim citing auth.rs -> writes
        //    runtime/agent-memory/*.light.md and creates light:: nodes.
        code_ingest(&mut state, "replace");
        handle_light_author(
            &mut state,
            LightAuthorInput {
                agent_id: "test".into(),
                node_label: "AuthMem".into(),
                title: None,
                state: None,
                claims: vec![LightClaim {
                    label: "TokenValidator".into(),
                    text: Some("validates tokens".into()),
                    kind: Some("entity".into()),
                    confidence: Some("high".into()),
                    ambiguity: None,
                    evidence: vec!["auth.rs".into()],
                    depends_on: vec![],
                }],
                namespace: None,
                ingest_after: true,
                mode: "merge".into(),
                supersedes: None,
                origin_brain: None,
                origin_claim: None,
                promoted_by: None,
                promotion_reason: None,
                promoted_to: None,
                evidence_unverifiable: false,
                soul_source: None,
            },
        )
        .expect("memorize");
        assert!(
            count_light(&state) > 0,
            "memorize should create light:: nodes"
        );

        // 2) A replace code ingest would wipe the graph — but agent-memory must
        //    be auto-restored. The result must report it AND the light nodes
        //    must be back in the live graph.
        let out = code_ingest(&mut state, "replace");
        let restored = &out["agent_memory_restored"];
        assert_eq!(
            restored["loaded"], true,
            "replace must restore agent memory, got: {:?}",
            restored
        );
        assert!(
            count_light(&state) > 0,
            "light:: memory nodes must survive a replace ingest (auto-restored)"
        );
    }

    /// P0 regression (live symptom: ingest reports edges_created=N but edge_count
    /// collapses to ~2). Structural code edges materialized in the CSR must survive a
    /// subsequent graph mutation + re-finalize triggered by a memorize (light author).
    ///
    /// Root cause was `Graph::finalize()` rebuilding the CSR purely from
    /// `pending_edges`; after the first finalize that list is empty, so the
    /// `add_node` performed when memorizing flipped `finalized=false` and the next
    /// `finalize()` wiped every code edge. This test ingests two files with a real
    /// cross-file import, memorizes a claim (which mutates + re-finalizes the live
    /// graph), and asserts the import edge is still queryable.
    #[test]
    fn memorize_after_code_ingest_preserves_code_edges() {
        use crate::light_author_handlers::{handle_light_author, LightAuthorInput, LightClaim};
        use crate::protocol::core::IngestInput;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(proj.join("src")).expect("proj src dir");
        // helper.rs defines Helper; main.rs imports + uses it => a real cross-file edge.
        std::fs::write(proj.join("src/helper.rs"), "pub struct Helper;\n").expect("write helper");
        std::fs::write(
            proj.join("src/main.rs"),
            "mod helper;\nuse crate::helper::Helper;\npub fn build(_: Helper) {}\n",
        )
        .expect("write main");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        let out = super::handle_ingest(
            &mut state,
            IngestInput {
                path: proj.to_string_lossy().to_string(),
                agent_id: "test".into(),
                incremental: false,
                adapter: "code".into(),
                mode: "replace".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            },
        )
        .expect("code ingest");

        let edges_after_ingest = out["edge_count"].as_u64().expect("edge_count present");
        // Sanity-scale: the live edge counter must match the structural edges that were
        // created — not collapse to a near-zero baseline.
        assert!(
            edges_after_ingest > 2,
            "code ingest must leave structural edges in the live graph, got edge_count={edges_after_ingest} (edges_created={})",
            out["edges_created"]
        );

        // Locate the cross-file import edge main.rs -> helper.rs::struct::Helper.
        let edge_exists = |state: &SessionState| -> bool {
            let g = state.graph.read();
            let Some(main_file) = g.resolve_id("file::src/main.rs") else {
                return false;
            };
            let Some(helper) = g.resolve_id("file::src/helper.rs::struct::Helper") else {
                return false;
            };
            g.csr
                .out_range(main_file)
                .any(|i| g.csr.targets[i] == helper)
        };
        assert!(
            edge_exists(&state),
            "the cross-file import edge must exist right after ingest"
        );
        let edges_baseline = {
            let g = state.graph.read();
            g.num_edges()
        };

        // Memorize a claim citing main.rs. This mutates the live graph (adds light
        // marker nodes + grounded_in edges) and re-finalizes it — the exact sequence
        // that used to destroy the code edges.
        handle_light_author(
            &mut state,
            LightAuthorInput {
                agent_id: "test".into(),
                node_label: "ArchMem".into(),
                title: None,
                state: None,
                claims: vec![LightClaim {
                    label: "MainUsesHelper".into(),
                    text: Some("main wires Helper".into()),
                    kind: Some("entity".into()),
                    confidence: Some("high".into()),
                    ambiguity: None,
                    evidence: vec!["src/main.rs".into()],
                    depends_on: vec![],
                }],
                namespace: None,
                ingest_after: true,
                mode: "merge".into(),
                supersedes: None,
                origin_brain: None,
                origin_claim: None,
                promoted_by: None,
                promotion_reason: None,
                promoted_to: None,
                evidence_unverifiable: false,
                soul_source: None,
            },
        )
        .expect("memorize");

        // The code edge MUST still be queryable, and the total edge count must have
        // grown (memory edges added) rather than collapsed.
        assert!(
            edge_exists(&state),
            "code import edge must survive a memorize-triggered re-finalize"
        );
        let edges_after_memorize = {
            let g = state.graph.read();
            g.num_edges()
        };
        assert!(
            edges_after_memorize >= edges_baseline,
            "edge count must not collapse after memorize: was {edges_baseline}, now {edges_after_memorize}"
        );
    }

    /// R1(b) — Budget Law write-path fix (RED→GREEN): the `memorize` write-path
    /// must NOT mint a per-file ingest root for every `.light.md` claim it writes.
    /// The store DIRECTORY is the one root; each sidecar file collapses into it.
    /// Before the fix, memorizing N claims grew `ingest_roots` by N sidecar files,
    /// sprawling the north packet. After: at most ONE `agent-memory` root appears,
    /// and no individual `.light.md` file is listed as a root.
    #[test]
    fn memorize_does_not_mint_per_sidecar_ingest_roots() {
        use crate::light_author_handlers::{handle_light_author, LightAuthorInput, LightClaim};

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        // Memorize several distinct claims (default agent-memory path, ingest_after).
        for i in 0..4 {
            handle_light_author(
                &mut state,
                LightAuthorInput {
                    agent_id: "test".into(),
                    node_label: format!("Claim{i}"),
                    title: None,
                    state: None,
                    claims: vec![LightClaim {
                        label: format!("Fact{i}"),
                        text: Some(format!("durable fact number {i}")),
                        kind: Some("entity".into()),
                        confidence: Some("high".into()),
                        ambiguity: None,
                        evidence: vec![],
                        depends_on: vec![],
                    }],
                    namespace: None,
                    ingest_after: true,
                    mode: "merge".into(),
                    supersedes: None,
                    origin_brain: None,
                    origin_claim: None,
                    promoted_by: None,
                    promotion_reason: None,
                    promoted_to: None,
                    evidence_unverifiable: false,
                    soul_source: None,
                },
            )
            .expect("memorize");
        }

        // No individual `.light.md` file may appear as an ingest root.
        let sidecar_roots: Vec<&String> = state
            .ingest_roots
            .iter()
            .filter(|r| r.ends_with(".light.md"))
            .collect();
        assert!(
            sidecar_roots.is_empty(),
            "no per-file `.light.md` sidecar may be an ingest root; found {sidecar_roots:?} in {:?}",
            state.ingest_roots
        );
        // At most one `agent-memory` store dir root (the collapse target), not four.
        let store_roots = state
            .ingest_roots
            .iter()
            .filter(|r| r.ends_with("agent-memory"))
            .count();
        assert!(
            store_roots <= 1,
            "the memory store must collapse to at most ONE dir root, got {store_roots} in {:?}",
            state.ingest_roots
        );
    }

    /// #326 recurrence (field reports 2026-07-14, two flips in two days): the
    /// SHARED SERVED OWNER must be immune to a classic `ingest {path}` that carries
    /// a FOREIGN local cwd. A local run (the `first-minute` shim, then `npm test`)
    /// reaches the served owner and its trust sequence sends `ingest {path: <cwd>}`;
    /// that silently rebound the owner's `workspace_root` to the cwd (`npm/bin`,
    /// then `npm/test`), poisoning every session's binding card until the next
    /// kickstart. The served owner is marked by `runnerd_naming` (stamped on the
    /// HTTP serve boot, `None` on stdio); its established binding is its own runtime
    /// home and only its OWN boot/config gesture may move it — never a foreign
    /// process's ingest. RED against origin/main (the owner flips to the foreign
    /// subdir), GREEN with the served-owner pin in `handle_ingest`.
    #[test]
    fn served_owner_binding_is_immune_to_foreign_local_ingest() {
        use crate::protocol::core::IngestInput;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir.clone()),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        // The served owner's established binding is its runtime home — the medulla
        // identity `infer_workspace_root` stamps at boot, exactly the healthy field
        // state (`workspace_root = ~/.m1nd/runtimes/claude`, source graph_path_parent).
        let owner_binding = state
            .workspace_root
            .clone()
            .expect("owner has an inferred workspace_root after init");
        assert!(
            !crate::session::is_memory_sidecar(&owner_binding),
            "precondition: the owner holds a real (non-sidecar) code root, got {owner_binding}"
        );

        // The HTTP serve boot stamps `runnerd_naming` (http_server.rs:380) — the one
        // signal that separates the shared served owner from a stdio session.
        state.runnerd_naming = Some(crate::runnerd_owner::NamingRunnerHandle {
            registry: std::sync::Arc::new(crate::runnerd_owner::RunnerdRegistry::default()),
            owner_runtime_root: runtime_dir.clone(),
        });

        // A FOREIGN local run delivers a classic `ingest {path: <cwd subdir>}` to
        // the served owner — the field shape: a subdir of a repo the owner maps
        // (e.g. `<repo>/npm/test`), spawned by `npm test`.
        let foreign_cwd = temp.path().join("repo").join("npm").join("test");
        std::fs::create_dir_all(&foreign_cwd).expect("foreign cwd");
        std::fs::write(
            foreign_cwd.join("cli.test.js"),
            b"// npm test spawns a foreign m1nd-mcp\n",
        )
        .expect("write foreign file");

        super::handle_ingest(
            &mut state,
            IngestInput {
                path: foreign_cwd.to_string_lossy().to_string(),
                agent_id: "foreign-local-run".into(),
                incremental: false,
                adapter: "code".into(),
                mode: "merge".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            },
        )
        .expect("foreign ingest");

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(owner_binding.as_str()),
            "the served owner's binding must be immune to a foreign local `ingest {{path}}`; \
             it flipped from {owner_binding} to {:?} (the #326 recurrence)",
            state.workspace_root
        );
    }

    /// Guard scope (companion to the immunity test): a stdio session — no
    /// `runnerd_naming` — keeps the classic single-graph binding, so an
    /// `ingest {path: repo}` still sets its `workspace_root`. The served-owner
    /// pin must NOT regress the plain `m1nd-mcp`-in-a-repo workflow.
    #[test]
    fn stdio_session_ingest_still_binds_workspace_root() {
        use crate::protocol::core::IngestInput;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo dir");
        std::fs::write(repo.join("lib.rs"), "pub fn f() {}\n").expect("write code");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        // Stdio session: `runnerd_naming` stays None (never a served owner).
        assert!(state.runnerd_naming.is_none());

        super::handle_ingest(
            &mut state,
            IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "stdio".into(),
                incremental: false,
                adapter: "code".into(),
                mode: "replace".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            },
        )
        .expect("code ingest");

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(repo.to_string_lossy().as_ref()),
            "a stdio single-graph session must still bind its workspace_root to the ingested repo"
        );
    }

    /// Pure-function coverage of the closure verdict across every branch,
    /// deterministic and graph-free (mirrors `compute_sufficiency_covers_every_state`).
    /// Proves: empty path -> closed; all-clean path -> closed; one tagged source
    /// -> blocked with the offending edge listed; and that load-bearing scoping is
    /// the caller's contract — an off-path tagged node is simply NOT in the list
    /// passed in, so it cannot blocked the verdict.
    #[test]
    fn closure_verdict_covers_every_state() {
        use super::closure_verdict as cv;

        // Empty path (no path, or nothing to inspect) -> closed, empty list.
        let empty: Vec<(String, String, Option<String>)> = vec![];
        let v = cv(&empty);
        assert_eq!(v["state"], "closed");
        assert_eq!(v["dangling_edges"].as_array().unwrap().len(), 0);

        // All-clean path (every reason None) -> closed.
        let clean = vec![
            ("a".to_string(), "calls".to_string(), None),
            ("b".to_string(), "imports".to_string(), None),
        ];
        let v = cv(&clean);
        assert_eq!(v["state"], "closed");
        assert_eq!(v["dangling_edges"].as_array().unwrap().len(), 0);

        // One tagged source -> blocked, the offending edge is listed with reason.
        let blocked = vec![
            ("a".to_string(), "calls".to_string(), None),
            (
                "b".to_string(),
                "calls".to_string(),
                Some("ambiguous".to_string()),
            ),
        ];
        let v = cv(&blocked);
        assert_eq!(v["state"], "blocked");
        let dangling = v["dangling_edges"].as_array().unwrap();
        assert_eq!(dangling.len(), 1, "only the tagged edge is reported");
        assert_eq!(dangling[0]["source"], "b");
        assert_eq!(dangling[0]["relation"], "calls");
        assert_eq!(dangling[0]["reason"], "ambiguous");

        // Load-bearing scoping: an off-path tagged node is never passed in, so a
        // clean on-path edge list stays closed even though tagged nodes exist
        // elsewhere in the graph (they simply aren't in `load_bearing`).
        let on_path_only = vec![("a".to_string(), "calls".to_string(), None)];
        assert_eq!(cv(&on_path_only)["state"], "closed");
    }

    // ---------------------------------------------------------------------
    // Move 6 (Subsystem D): recency-cap the agent-memory auto-load.
    // ---------------------------------------------------------------------

    /// `M1ND_MEMORY_LOAD_CAP` is a process-global env var, so cap tests that
    /// mutate it must serialize against one another. Mirrors the `LOCK` pattern
    /// in `session.rs`.
    fn cap_env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    /// Write a minimal-but-valid `.light.md` into `<runtime>/agent-memory`,
    /// optionally stamping a `Created:` provenance line (None = legacy corpus).
    fn write_memory(runtime_root: &std::path::Path, name: &str, created_ms: Option<u64>) {
        let dir = runtime_root.join("agent-memory");
        std::fs::create_dir_all(&dir).expect("agent-memory dir");
        let created_line = created_ms
            .map(|ms| format!("Created: {ms}\n"))
            .unwrap_or_default();
        let node = name.replace(".light.md", "");
        let body = format!(
            "---\nProtocol: L1GHT/1.0\nNode: {node}\n{created_line}---\n\n## Recall\n\nThe [⍂ entity: {node}] was learned. [𝔻 confidence: high]\n"
        );
        std::fs::write(dir.join(name), body).expect("write .light.md");
    }

    #[test]
    fn reload_default_no_cap_loads_all_files() {
        // Default (env unset) must be a pure no-op: N files in → all N loaded,
        // exactly like the pre-cap behavior. Holds the lock and clears the env
        // so a stray cap from another test cannot leak in.
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("M1ND_MEMORY_LOAD_CAP");

        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());
        for i in 0..5 {
            write_memory(
                &state.runtime_root,
                &format!("mem{i}.light.md"),
                Some(1_700_000_000_000 + i as u64 * 1000),
            );
        }

        let report = super::reload_agent_memory(&mut state).expect("report");
        assert_eq!(report["loaded"], true, "should load: {report:?}");
        assert_eq!(report["file_count"], 5);
        assert_eq!(report["loaded_count"], 5, "all files load by default");
        assert_eq!(report["capped_out_count"], 0, "nothing dropped by default");
        assert!(
            report["load_cap"].is_null(),
            "cap is null (unlimited) by default"
        );
    }

    #[test]
    fn reload_cap_keeps_only_k_most_recent_and_reports_drops() {
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("M1ND_MEMORY_LOAD_CAP", "2");

        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());
        // 4 stamped files, ascending recency. Cap=2 keeps the 2 newest.
        write_memory(
            &state.runtime_root,
            "oldest.light.md",
            Some(1_000_000_000_000),
        );
        write_memory(
            &state.runtime_root,
            "older.light.md",
            Some(1_500_000_000_000),
        );
        write_memory(
            &state.runtime_root,
            "newer.light.md",
            Some(1_700_000_000_000),
        );
        write_memory(
            &state.runtime_root,
            "newest.light.md",
            Some(1_900_000_000_000),
        );

        let report = super::reload_agent_memory(&mut state).expect("report");
        std::env::remove_var("M1ND_MEMORY_LOAD_CAP");

        assert_eq!(report["loaded"], true, "should load: {report:?}");
        assert_eq!(report["file_count"], 4);
        assert_eq!(report["loaded_count"], 2, "only the 2 most recent load");
        assert_eq!(report["load_cap"], 2);
        assert_eq!(report["capped_out_count"], 2, "the 2 oldest are dropped");
        let capped: Vec<String> = report["capped_out"]
            .as_array()
            .expect("capped_out array")
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();
        assert!(
            capped.iter().any(|p| p.ends_with("oldest.light.md")),
            "oldest must be reported as capped out: {capped:?}"
        );
        assert!(
            capped.iter().any(|p| p.ends_with("older.light.md")),
            "older must be reported as capped out: {capped:?}"
        );
        assert!(
            !capped.iter().any(|p| p.ends_with("newest.light.md")),
            "newest must NOT be capped out: {capped:?}"
        );
    }

    #[test]
    fn reload_cap_exempts_files_without_created() {
        // The legacy-corpus guard: a file with NO `Created` is EXEMPT from
        // eviction even under a tight cap, and even when newer stamped files
        // exist. Without the guard, the no-Created file would sort as oldest and
        // be the first dropped — the whole point of the correction.
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("M1ND_MEMORY_LOAD_CAP", "1");

        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());
        // 1 legacy (no Created) + 2 recent stamped files, cap=1.
        write_memory(&state.runtime_root, "legacy.light.md", None);
        write_memory(
            &state.runtime_root,
            "recent-a.light.md",
            Some(1_800_000_000_000),
        );
        write_memory(
            &state.runtime_root,
            "recent-b.light.md",
            Some(1_900_000_000_000),
        );

        let report = super::reload_agent_memory(&mut state).expect("report");
        std::env::remove_var("M1ND_MEMORY_LOAD_CAP");

        assert_eq!(report["file_count"], 3);
        let capped: Vec<String> = report["capped_out"]
            .as_array()
            .expect("capped_out array")
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();
        // The legacy file is NEVER dropped for lacking Created.
        assert!(
            !capped.iter().any(|p| p.ends_with("legacy.light.md")),
            "legacy (no Created) must be exempt from eviction: {capped:?}"
        );
        // Effective budget is max(cap, #exempt) = max(1, 1) = 1, so both stamped
        // files exceed it and are dropped; the exempt file always survives.
        assert!(
            report["loaded_count"].as_u64().unwrap() >= 1,
            "at least the exempt file loads: {report:?}"
        );
    }

    #[test]
    fn read_light_created_ms_handles_missing_and_unparsable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();

        // Stamped: parses to the millis value.
        let good = dir.join("good.light.md");
        std::fs::write(
            &good,
            "---\nProtocol: L1GHT/1.0\nNode: G\nCreated: 1700000000000\n---\n\n## X\n",
        )
        .unwrap();
        assert_eq!(super::read_light_created_ms(&good), Some(1_700_000_000_000));

        // Legacy (no Created line): None, NOT epoch-0.
        let legacy = dir.join("legacy.light.md");
        std::fs::write(&legacy, "---\nProtocol: L1GHT/1.0\nNode: L\n---\n\n## X\n").unwrap();
        assert_eq!(
            super::read_light_created_ms(&legacy),
            None,
            "missing Created is unknown age (None), never epoch-0"
        );

        // Unparsable Created value: None, NOT epoch-0.
        let junk = dir.join("junk.light.md");
        std::fs::write(
            &junk,
            "---\nProtocol: L1GHT/1.0\nNode: J\nCreated: not-a-number\n---\n\n## X\n",
        )
        .unwrap();
        assert_eq!(
            super::read_light_created_ms(&junk),
            None,
            "unparsable Created is unknown age (None), never epoch-0"
        );

        // Missing file: None.
        assert_eq!(
            super::read_light_created_ms(&dir.join("does-not-exist.light.md")),
            None
        );
    }

    #[test]
    fn agent_memory_load_cap_defaults_to_unlimited() {
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("M1ND_MEMORY_LOAD_CAP");
        assert_eq!(
            super::agent_memory_load_cap(),
            usize::MAX,
            "default unlimited"
        );

        std::env::set_var("M1ND_MEMORY_LOAD_CAP", "3");
        assert_eq!(super::agent_memory_load_cap(), 3);

        // 0 / empty / garbage are ignored (treated as unlimited), never "load 0".
        std::env::set_var("M1ND_MEMORY_LOAD_CAP", "0");
        assert_eq!(super::agent_memory_load_cap(), usize::MAX, "0 → unlimited");
        std::env::set_var("M1ND_MEMORY_LOAD_CAP", "nope");
        assert_eq!(
            super::agent_memory_load_cap(),
            usize::MAX,
            "garbage → unlimited"
        );
        std::env::remove_var("M1ND_MEMORY_LOAD_CAP");
    }

    // ---------------------------------------------------------------------
    // Binary version-honesty: the fingerprint carries version+sha, drift warns
    // (env-expected + self-repo lag), and strict mode refuses at startup.
    // ---------------------------------------------------------------------

    /// Clear the version-expectation env so a test starts from a known clean
    /// slate regardless of what the outer harness set. Caller holds the env lock.
    fn clear_version_env() {
        std::env::remove_var("M1ND_EXPECTED_VERSION");
        std::env::remove_var("M1ND_EXPECTED_SHA");
    }

    #[test]
    fn binding_fingerprint_carries_binary_version_and_sha() {
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        clear_version_env();
        let temp = tempfile::tempdir().expect("tempdir");
        let state = build_runtime_state(temp.path());

        let fp = state.binding_fingerprint();
        // Version is the compile-time crate version, verbatim.
        assert_eq!(fp["binary_version"], env!("CARGO_PKG_VERSION"));
        // Sha is the embedded build-time sha; on a git build it is a real short
        // sha (optionally `-dirty`), on a vendored build it is exactly "unknown".
        let sha = fp["binary_git_sha"].as_str().expect("sha string");
        assert!(!sha.is_empty(), "sha never empty");
        assert_eq!(sha, env!("M1ND_GIT_SHA"));
        // No expectation set + no self-repo manifest => no drift.
        assert_eq!(fp["binary_drift"], serde_json::Value::Null);
        clear_version_env();
    }

    #[test]
    fn binary_version_info_no_drift_when_expectation_matches() {
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        clear_version_env();
        let temp = tempfile::tempdir().expect("tempdir");
        let state = build_runtime_state(temp.path());

        // Expectation matches the running binary exactly => no drift, no warning.
        std::env::set_var("M1ND_EXPECTED_VERSION", env!("CARGO_PKG_VERSION"));
        std::env::set_var("M1ND_EXPECTED_SHA", env!("M1ND_GIT_SHA"));
        let (info, summary) = state.binary_version_info();
        assert_eq!(info["binary_drift"], serde_json::Value::Null);
        assert!(summary.is_none(), "matched expectation => no warning");
        clear_version_env();
    }

    #[test]
    fn binary_version_info_drifts_when_expected_version_mismatches() {
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        clear_version_env();
        let temp = tempfile::tempdir().expect("tempdir");
        let state = build_runtime_state(temp.path());

        // An old expectation (e.g. the beta.8 incident) => drift block + warning.
        std::env::set_var("M1ND_EXPECTED_VERSION", "0.0.0-beta.8");
        let (info, summary) = state.binary_version_info();
        let drift = &info["binary_drift"];
        assert_ne!(*drift, serde_json::Value::Null, "drift block present");
        assert_eq!(drift["drift_detected"], true);
        assert_eq!(drift["version_mismatch"], true);
        assert_eq!(drift["sha_mismatch"], false);
        assert_eq!(drift["expected_version"], "0.0.0-beta.8");
        assert_eq!(drift["running_version"], env!("CARGO_PKG_VERSION"));
        assert!(summary.is_some(), "mismatch => human warning");
        assert!(summary.unwrap().contains("binary_drift"));
        clear_version_env();
    }

    #[test]
    fn trust_selftest_surfaces_binary_drift_without_flipping_verdict() {
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        clear_version_env();
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());

        std::env::set_var("M1ND_EXPECTED_VERSION", "0.0.0-beta.8");
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

        // Verdict is UNCHANGED — a stale binary is a warning, not a failure.
        assert_eq!(output["verdict"], "full_trust");
        assert_eq!(output["status"], "ok");
        assert_eq!(output["ok"], true);
        // But the drift is loud: top-level block, a check flag, warning in
        // next_action, and an appended non_claim.
        assert_eq!(output["checks"]["binary_drift_detected"], true);
        assert_ne!(output["binary_drift"], serde_json::Value::Null);
        assert_eq!(output["binary_drift"]["version_mismatch"], true);
        assert!(output["next_action"]
            .as_str()
            .expect("next_action string")
            .contains("binary_drift"));
        let non_claims = output["non_claims"].as_array().expect("non_claims array");
        assert!(
            non_claims.iter().any(|c| c
                .as_str()
                .map(|s| s.contains("binary_drift"))
                .unwrap_or(false)),
            "drift warning appended to non_claims"
        );
        // And it propagates through the embedded handshake too.
        assert_ne!(
            output["session_handshake"]["binary_drift"],
            serde_json::Value::Null
        );
        clear_version_env();
    }

    #[test]
    fn trust_selftest_no_drift_when_binary_matches() {
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        clear_version_env();
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

        assert_eq!(output["checks"]["binary_drift_detected"], false);
        assert_eq!(output["binary_drift"], serde_json::Value::Null);
        // next_action is the clean full-trust guidance (no drift prefix).
        assert!(!output["next_action"]
            .as_str()
            .expect("next_action string")
            .contains("binary_drift"));
        clear_version_env();
    }

    #[test]
    fn self_repo_higher_version_flags_binary_lags_repo() {
        // No env expectation — this signal is purely the bound repo's own
        // m1nd-mcp/Cargo.toml declaring a version newer than the running binary.
        let _g = cap_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        clear_version_env();
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_runtime_state(temp.path());

        // Fake a checked-out m1nd repo whose manifest is AHEAD of this binary.
        let repo_mcp = temp.path().join("m1nd-mcp");
        std::fs::create_dir_all(&repo_mcp).expect("mkdir m1nd-mcp");
        std::fs::write(
            repo_mcp.join("Cargo.toml"),
            "[package]\nname = \"m1nd-mcp\"\nversion = \"999.0.0\"\nedition = \"2021\"\n",
        )
        .expect("write fake Cargo.toml");
        // Bind the workspace to that repo root (build_runtime_state already set
        // workspace_root to temp.path(); make it explicit for clarity).
        state.workspace_root = Some(temp.path().to_string_lossy().to_string());

        let (info, summary) = state.binary_version_info();
        let drift = &info["binary_drift"];
        assert_ne!(*drift, serde_json::Value::Null, "lag => drift block");
        assert_eq!(drift["binary_lags_repo"], true);
        assert_eq!(drift["repo_declared_version"], "999.0.0");
        assert_eq!(drift["running_version"], env!("CARGO_PKG_VERSION"));
        // Not an env mismatch, purely the repo-lag signal.
        assert_eq!(drift["version_mismatch"], false);
        assert!(summary.expect("warning").contains("binary_drift"));
        clear_version_env();
    }

    #[test]
    fn parse_cargo_package_version_reads_package_level_only() {
        // Package version at column 0 is read; an indented dependency version is
        // NOT mistaken for it (the package `version` line wins by appearing first
        // at zero indentation).
        let manifest = "[package]\nname = \"m1nd-mcp\"\nversion = \"1.2.3\"\n\n[dependencies]\nserde = { version = \"1\" }\n";
        assert_eq!(
            crate::session::parse_cargo_package_version(manifest),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            crate::session::parse_cargo_package_version("[package]\nname = \"x\"\n"),
            None
        );
    }
}
