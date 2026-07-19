use crate::protocol::layers::{
    MissionCloseInput, MissionEventInput, MissionHandoffInput, MissionNextInput, MissionStartInput,
    MissionVerifyInput,
};
use crate::session::SessionState;
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const STATE_SCHEMA: &str = "m1nd-mission-control-state-v1";
const START_SCHEMA: &str = "m1nd-mission-start-v0";
const EVENT_SCHEMA: &str = "m1nd-mission-event-v1";
const NEXT_SCHEMA: &str = "m1nd-mission-next-v0";
const VERIFY_SCHEMA: &str = "m1nd-mission-verify-v0";
const HANDOFF_SCHEMA: &str = "m1nd-mission-handoff-v1";
const CLOSE_SCHEMA: &str = "m1nd-mission-proof-packet-v1";

/// A `direct_unverified` claim (a direct label with no corroborating path or
/// recorded event) may not report confidence above this ceiling — the number
/// cannot outrun the evidence backing it.
const DIRECT_UNVERIFIED_CONFIDENCE_CAP: f32 = 0.5;

const DEFAULT_NON_CLAIMS: &[&str] = &[
    "mission control does not prove graph contents are correct",
    "mission control does not refresh an already-open host MCP tool cache",
    "mission control does not replace direct source reads, tests, compiler output, or runtime probes",
    "mission control does not claim autonomous multi-agent orchestration",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MissionState {
    schema: String,
    mission_id: String,
    agent_id: String,
    repo: String,
    task: String,
    mode: String,
    budget: String,
    risk: String,
    #[serde(default)]
    parent_mission_id: Option<String>,
    #[serde(default)]
    evidence_link: Option<crate::evidence_spine::EvidenceCorrelationLinkV1>,
    #[serde(default)]
    evidence_projection_gaps: Vec<String>,
    route: String,
    phase: String,
    status: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    next_step_id: u64,
    budget_envelope: Value,
    graph_state_at_start: Value,
    context_guard_at_start: Value,
    #[serde(default)]
    events: Vec<Value>,
    #[serde(default)]
    claims: Vec<MissionClaimState>,
    #[serde(default)]
    handoffs: Vec<Value>,
    non_claims: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MissionClaimState {
    claim_id: String,
    claim: String,
    evidence_refs: Vec<String>,
    evidence_grade: String,
    verdict: String,
    missing: Vec<String>,
    confidence: Option<f32>,
    created_at_ms: u64,
}

pub fn handle_mission_start(
    state: &mut SessionState,
    input: MissionStartInput,
) -> M1ndResult<Value> {
    validate_mission_start_input(&input)?;
    if let Some(link) = &input.evidence_link {
        crate::evidence_spine_owner::validate_record_workspace(
            state,
            "mission_start",
            &input.repo,
        )?;
        crate::evidence_spine_owner::validate_link(state, "mission_start", link)?;
    }
    let now = now_ms();
    let mission_id = build_mission_id(now, &input.agent_id, &input.task);
    let route = route_for(&input.mode, &input.budget, &input.risk);
    let budget_envelope = budget_envelope(&input.budget);
    let graph_state = graph_state(state);
    let context_guard = context_guard_projection(&graph_state, &input.repo);
    let non_claims = DEFAULT_NON_CLAIMS
        .iter()
        .map(|claim| (*claim).to_string())
        .collect::<Vec<_>>();

    let mission = MissionState {
        schema: STATE_SCHEMA.into(),
        mission_id: mission_id.clone(),
        agent_id: input.agent_id.clone(),
        repo: input.repo.clone(),
        task: input.task.clone(),
        mode: input.mode.clone(),
        budget: input.budget.clone(),
        risk: input.risk.clone(),
        parent_mission_id: input.parent_mission_id.clone(),
        evidence_link: input.evidence_link.clone(),
        evidence_projection_gaps: if input.evidence_link.is_none() {
            vec!["canonical_evidence_link_absent: Mission Control remains an independent reasoning trail; G3 correlation is NOT_PROVEN".to_string()]
        } else {
            Vec::new()
        },
        route: route.clone(),
        phase: "locate".into(),
        status: "active".into(),
        created_at_ms: now,
        updated_at_ms: now,
        next_step_id: 1,
        budget_envelope: budget_envelope.clone(),
        graph_state_at_start: graph_state.clone(),
        context_guard_at_start: context_guard.clone(),
        events: Vec::new(),
        claims: Vec::new(),
        handoffs: Vec::new(),
        non_claims: non_claims.clone(),
    };

    save_mission(state, &mission)?;
    let evidence_projection = project_mission_control(state, &mission);

    Ok(json!({
        "schema": START_SCHEMA,
        "mission_id": mission_id,
        "agent_id": input.agent_id,
        "repo": input.repo,
        "task": input.task,
        "mode": input.mode,
        "route": route,
        "trust": trust_projection(&graph_state, &input.repo),
        "context_guard": context_guard,
        "expected_phases": expected_phases(&mission.mode),
        "budget_envelope": budget_envelope,
        "starter_moves": starter_moves(&mission, &graph_state),
        "non_goals": non_goals_for(&mission.mode),
        "non_claims": non_claims,
        "evidence_projection": evidence_projection,
    }))
}

pub fn handle_mission_event(
    state: &mut SessionState,
    input: MissionEventInput,
) -> M1ndResult<Value> {
    let MissionEventInput {
        agent_id,
        mission_id,
        event,
        payload,
        outcome,
        agent_confidence,
    } = input;
    // Serialize the load→mutate→save against concurrent writers to this mission.
    let _lock = mission_lock(state, &mission_id)?;
    let mut mission = load_mission(state, &mission_id)?;
    ensure_agent(&mission, &agent_id)?;

    let event_value = normalize_mission_event_value(event, payload, outcome, agent_confidence);
    let event_id = append_event(&mut mission, event_value, "mission_event");
    mission.updated_at_ms = now_ms();
    save_mission(state, &mission)?;
    let evidence_projection = project_mission_control(state, &mission);

    let event = mission.events.last().cloned().unwrap_or_else(|| json!({}));
    Ok(json!({
        "schema": EVENT_SCHEMA,
        "mission_id": mission.mission_id,
        "event_id": event_id,
        "event": event,
        "event_count": mission.events.len(),
        "budget_consumed": budget_consumed(&mission),
        "event_digest": event_digest(&mission.events),
        "non_claims": mission.non_claims,
        "evidence_projection": evidence_projection,
    }))
}

pub fn handle_mission_next(state: &mut SessionState, input: MissionNextInput) -> M1ndResult<Value> {
    let _lock = mission_lock(state, &input.mission_id)?;
    let mut mission = load_mission(state, &input.mission_id)?;
    ensure_agent(&mission, &input.agent_id)?;
    if let Some(event) = input.last_event {
        append_event(&mut mission, event, "mission_next");
    }

    let step_id = mission.next_step_id;
    mission.next_step_id = mission.next_step_id.saturating_add(1);
    let analysis = analyze_events(&mission);
    let (phase, move_value, do_not, soft_warning) = next_move(&mission, &analysis);
    mission.phase = phase.clone();
    mission.updated_at_ms = now_ms();
    save_mission(state, &mission)?;
    let evidence_projection = project_mission_control(state, &mission);

    Ok(json!({
        "schema": NEXT_SCHEMA,
        "mission_id": mission.mission_id,
        "step_id": step_id,
        "phase": phase,
        "route": mission.route,
        "move": move_value,
        "do_not": do_not,
        "soft_warning": soft_warning,
        "dissent_allowed": true,
        "dissent_protocol": {
            "event": "dissent",
            "required_fields": ["why", "chosen_tool", "evidence_required"]
        },
        "budget_consumed": budget_consumed(&mission),
        "event_count": mission.events.len(),
        "non_claims": mission.non_claims,
        "evidence_projection": evidence_projection,
    }))
}

pub fn handle_mission_verify(
    state: &mut SessionState,
    input: MissionVerifyInput,
) -> M1ndResult<Value> {
    let _lock = mission_lock(state, &input.mission_id)?;
    let mut mission = load_mission(state, &input.mission_id)?;
    ensure_agent(&mission, &input.agent_id)?;

    let verify_roots = mission_verify_roots(state);
    let grade = classify_evidence(&input.evidence_refs, &mission.events, &verify_roots);
    let mut missing = Vec::new();
    // Only a corroborated `direct` grade closes a claim. `direct_unverified` — a
    // direct LABEL with neither a real cited path nor a backing recorded event —
    // is treated as insufficient and its self-reported confidence is capped, so a
    // forged label can no longer forge a `verified_for_mission`.
    let verdict = if grade == "direct" {
        "verified_for_mission"
    } else if grade == "direct_unverified" {
        missing.push("unverifiable_direct_label_needs_existing_path_or_recorded_event".to_string());
        "insufficient_evidence"
    } else {
        missing.push("direct_source_read_or_runtime_probe".to_string());
        "insufficient_evidence"
    };
    // Cap confidence for a label-only "direct" claim: the number cannot ride
    // higher than the evidence backing it.
    let effective_confidence = if grade == "direct_unverified" {
        input
            .confidence
            .map(|c| c.min(DIRECT_UNVERIFIED_CONFIDENCE_CAP))
    } else {
        input.confidence
    };
    let next_required_move = if verdict == "verified_for_mission" && mission.mode == "bug_hunt" {
        json!({
            "type": "mission_next",
            "why": "bug_hunt claims are verified individually; ask mission_next whether another direct sweep is required before close",
            "evidence_required": "coverage_sweep_or_next_claim"
        })
    } else if verdict == "verified_for_mission" {
        json!({
            "type": "claim_or_close",
            "why": "claim has at least one direct evidence reference; either verify the next claim or close with explicit gaps"
        })
    } else if grade == "direct_unverified" {
        json!({
            "type": "read_file",
            "why": "a direct label with no existing cited path and no recorded mission event is unverifiable — cite a real path or record the read as a mission_event before closing",
            "evidence_required": "existing_path_or_recorded_event"
        })
    } else {
        json!({
            "type": "read_file",
            "why": "graph-only or inferred evidence cannot close a mission claim",
            "evidence_required": "direct_read_or_runtime_probe"
        })
    };

    let claim_id = format!("clm_{}_{}", mission.claims.len() + 1, now_ms());
    let claim = MissionClaimState {
        claim_id: claim_id.clone(),
        claim: input.claim.clone(),
        evidence_refs: input.evidence_refs.clone(),
        evidence_grade: grade.clone(),
        verdict: verdict.into(),
        missing: missing.clone(),
        confidence: effective_confidence,
        created_at_ms: now_ms(),
    };
    mission.claims.push(claim);
    mission.phase = "verify".into();
    mission.updated_at_ms = now_ms();
    save_mission(state, &mission)?;
    let evidence_projection = project_mission_control(state, &mission);

    Ok(json!({
        "schema": VERIFY_SCHEMA,
        "mission_id": mission.mission_id,
        "claim_id": claim_id,
        "verdict": verdict,
        "evidence_grade": grade,
        "missing": missing,
        "next_required_move": next_required_move,
        "non_claims": mission.non_claims,
        "evidence_projection": evidence_projection,
    }))
}

pub fn handle_mission_handoff(
    state: &mut SessionState,
    input: MissionHandoffInput,
) -> M1ndResult<Value> {
    let _lock = mission_lock(state, &input.mission_id)?;
    let mut mission = load_mission(state, &input.mission_id)?;
    ensure_agent(&mission, &input.agent_id)?;

    let handoff_id = format!("hnd_{}_{}", mission.handoffs.len() + 1, now_ms());
    let analysis = analyze_events(&mission);
    let (phase, next_move, do_not, soft_warning) = next_move(&mission, &analysis);
    let handoff = json!({
        "schema": HANDOFF_SCHEMA,
        "handoff_id": handoff_id,
        "mission_id": mission.mission_id,
        "parent_mission_id": mission.parent_mission_id,
        "agent_id": mission.agent_id,
        "recipient_agent_id": input.recipient_agent_id,
        "repo": mission.repo,
        "task": mission.task,
        "mode": mission.mode,
        "route": mission.route,
        "phase": phase,
        "summary": input.summary,
        "verified_claims": verified_claims_json(&mission),
        "rejected_claims": rejected_claims_json(&mission),
        "open_hypotheses": open_hypotheses(&mission),
        "dead_paths": dead_paths(&mission),
        "files_read": files_read(&mission),
        "tools_observed": tools_observed(&mission),
        "graph_anchors": graph_anchors(&mission),
        "next_required_move": {
            "move": next_move,
            "do_not": do_not,
            "soft_warning": soft_warning,
        },
        "resume_hint": format!("phase={phase}; call mission_next or mission_verify before final output"),
        "event_digest": event_digest(&mission.events),
        "events": if input.include_events { Value::Array(mission.events.clone()) } else { Value::Null },
        "non_claims": mission.non_claims,
    });
    mission.handoffs.push(handoff.clone());
    mission.updated_at_ms = now_ms();
    save_mission(state, &mission)?;
    let evidence_projection = project_mission_control(state, &mission);
    let mut handoff = handoff;
    if let Some(object) = handoff.as_object_mut() {
        object.insert("evidence_projection".to_string(), evidence_projection);
    }
    Ok(handoff)
}

pub fn handle_mission_close(
    state: &mut SessionState,
    input: MissionCloseInput,
) -> M1ndResult<Value> {
    let _lock = mission_lock(state, &input.mission_id)?;
    let mut mission = load_mission(state, &input.mission_id)?;
    ensure_agent(&mission, &input.agent_id)?;

    mission.status = "closed".into();
    mission.phase = "closed".into();
    mission.updated_at_ms = now_ms();
    save_mission(state, &mission)?;
    let evidence_projection = project_mission_control(state, &mission);

    let verified_claims = verified_claims_json(&mission);
    let rejected_claims = rejected_claims_json(&mission);
    let mut non_claims = mission.non_claims.clone();
    non_claims.extend(input.non_claims.clone());
    non_claims.sort();
    non_claims.dedup();

    let mut packet = json!({
        "schema": CLOSE_SCHEMA,
        "mission_id": mission.mission_id,
        "agent_id": mission.agent_id,
        "repo": mission.repo,
        "task": mission.task,
        "route": mission.route,
        "summary": input.summary,
        "verified_claims": verified_claims,
        "rejected_claims": rejected_claims,
        "event_count": mission.events.len(),
        "tools_observed": tools_observed(&mission),
        "budget_consumed": budget_consumed(&mission),
        "graph_state_at_start": mission.graph_state_at_start,
        "context_guard_at_start": mission.context_guard_at_start,
        "event_digest": event_digest(&mission.events),
        "handoff_count": mission.handoffs.len(),
        "gaps": input.gaps,
        "non_claims": non_claims,
        "evidence_projection": evidence_projection,
    });

    // Optional: write verified claims as a .light.md and ingest.
    // A failure here must NOT fail the close — attach error key instead.
    let mut light_memory_written = false;
    if input.write_light_memory {
        let light_result = try_write_light_memory(state, &mission);
        if let Some(obj) = packet.as_object_mut() {
            match light_result {
                Ok(path) => {
                    light_memory_written = true;
                    obj.insert("light_memory".into(), Value::String(path));
                }
                Err(e) => {
                    obj.insert("light_memory_error".into(), Value::String(e.to_string()));
                }
            }
        }
    }

    // Host-agnostic habit nudge: every MCP host shows the model `next_action`.
    // Steer the agent to persist verified knowledge so it compounds across sessions.
    let has_verified = !verified_claims.is_empty();
    let next_action = if light_memory_written {
        "Verified knowledge persisted as L1GHT memory — it auto-loads next session and self-flags as stale via cross_verify(check:[\"evidence_freshness\"]) if the cited code changes.".to_string()
    } else if has_verified {
        "Persist the verified claims so they compound across sessions: re-run mission_close with write_light_memory:true, or call memorize(...) with evidence paths to the backing code.".to_string()
    } else {
        "No verified claims to persist. If you concluded anything durable, call memorize(...) with evidence paths so it anchors to code and auto-loads next session.".to_string()
    };
    if let Some(obj) = packet.as_object_mut() {
        obj.insert("next_action".into(), Value::String(next_action));
    }

    Ok(packet)
}

/// Build a `LightAuthorInput` from mission verified claims and call the author handler.
/// Returns the path of the written file on success.
fn try_write_light_memory(
    state: &mut SessionState,
    mission: &MissionState,
) -> Result<String, Box<dyn std::error::Error>> {
    use crate::light_author_handlers::{handle_light_author, LightAuthorInput, LightClaim};

    let claims: Vec<LightClaim> = mission
        .claims
        .iter()
        .filter(|c| c.verdict == "verified_for_mission")
        .map(|c| {
            // Derive a short label from the first few words of the claim text.
            let label = c
                .claim
                .split_whitespace()
                .take(6)
                .collect::<Vec<_>>()
                .join("-");
            LightClaim {
                label,
                text: Some(c.claim.clone()),
                kind: Some("entity".into()),
                confidence: c.confidence.map(|f| format!("{:.2}", f)),
                ambiguity: None,
                evidence: c.evidence_refs.clone(),
                depends_on: vec![],
            }
        })
        .collect();

    let light_input = LightAuthorInput {
        agent_id: mission.agent_id.clone(),
        node_label: mission.mission_id.clone(),
        title: Some(mission.task.clone()),
        state: Some("closed".into()),
        claims,
        namespace: Some("light".into()),
        ingest_after: true,
        mode: "merge".into(),
        supersedes: None,
        // Default path: the handler stamps Origin-Brain from the session (§6).
        origin_brain: None,
        origin_claim: None,
        promoted_by: None,
        promotion_reason: None,
        promoted_to: None,
        evidence_unverifiable: false,
        soul_source: None,
    };

    let result = handle_light_author(state, light_input)
        .map_err(|e| format!("light_author error: {}", e))?;

    result["path"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "memorize returned no path".into())
}

fn mission_dir(state: &SessionState) -> PathBuf {
    state.runtime_root.join("mission-control")
}

/// P1 presence enrichment: the MEASURED `task_ref` for a presence sidecar — the
/// `msn_…` id of the agent's newest still-open (`status == "active"`)
/// mission-control charter under `runtime_root`, or `None`. Measured from the
/// charter, never a free declaration (askGOD verdict 2026-07-13). Best-effort and
/// total: any unreadable/unparseable card is skipped, an absent dir yields `None`.
pub fn latest_open_mission_for(runtime_root: &Path, agent_id: &str) -> Option<String> {
    let dir = runtime_root.join("mission-control");
    let entries = fs::read_dir(&dir).ok()?;
    let mut best: Option<(u64, String)> = None;
    for item in entries.flatten() {
        let path = item.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mission) = serde_json::from_str::<MissionState>(&raw) else {
            continue;
        };
        if mission.agent_id != agent_id || mission.status != "active" {
            continue;
        }
        let supersedes = match best.as_ref() {
            Some((ts, _)) => mission.updated_at_ms >= *ts,
            None => true,
        };
        if supersedes {
            best = Some((mission.updated_at_ms, mission.mission_id));
        }
    }
    best.map(|(_, id)| id)
}

/// Acquire the per-mission exclusive lock held across a load→mutate→save
/// read-modify-write, so two concurrent writers to the same mission cannot
/// clobber each other's update. Reuses the memorize lock primitive
/// ([`crate::light_author_handlers::LockGuard`]) — an in-process mutex on every
/// platform, plus a cross-process `flock` on unix — on
/// `<runtime_root>/mission-control/.locks/<mission_id>.lock`.
fn mission_lock(
    state: &SessionState,
    mission_id: &str,
) -> M1ndResult<crate::light_author_handlers::LockGuard> {
    let locks_dir = mission_dir(state).join(".locks");
    crate::light_author_handlers::LockGuard::acquire_in(&locks_dir, mission_id)
}

fn mission_path(state: &SessionState, mission_id: &str) -> M1ndResult<PathBuf> {
    validate_mission_id(mission_id)?;
    Ok(mission_dir(state).join(format!("{mission_id}.json")))
}

fn save_mission(state: &SessionState, mission: &MissionState) -> M1ndResult<()> {
    let dir = mission_dir(state);
    fs::create_dir_all(&dir).map_err(M1ndError::Io)?;
    let path = mission_path(state, &mission.mission_id)?;
    let body = serde_json::to_string_pretty(mission).map_err(M1ndError::Serde)?;
    fs::write(path, body).map_err(M1ndError::Io)
}

fn project_mission_control(state: &SessionState, mission: &MissionState) -> Value {
    match serde_json::to_value(mission) {
        Ok(record) => crate::evidence_spine_owner::record_mission_control(
            state,
            mission.evidence_link.as_ref(),
            &record,
            mission.updated_at_ms,
        ),
        Err(error) => crate::evidence_spine_owner::gap_status(
            "mission_control_projection_encoding_failed",
            error.to_string(),
        ),
    }
}

/// The candidate repo roots a cited evidence path is resolved against when
/// grading a `direct` label. The workspace root (if known) plus every ingest
/// root — deduped — so a relative `src/auth.rs` in an evidence ref can be checked
/// for real existence. Empty when nothing is bound (then a relative path cannot
/// be verified, and a bare direct label honestly grades `direct_unverified`).
fn mission_verify_roots(state: &SessionState) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut push_unique = |root: PathBuf| {
        if !root.as_os_str().is_empty() && !roots.contains(&root) {
            roots.push(root);
        }
    };
    if let Some(workspace_root) = &state.workspace_root {
        push_unique(PathBuf::from(workspace_root));
    }
    for ingest_root in &state.ingest_roots {
        push_unique(PathBuf::from(ingest_root));
    }
    roots
}

fn load_mission(state: &SessionState, mission_id: &str) -> M1ndResult<MissionState> {
    let path = mission_path(state, mission_id)?;
    let body = fs::read_to_string(&path).map_err(|error| M1ndError::InvalidParams {
        tool: "mission".into(),
        detail: format!("mission_id {mission_id} could not be loaded: {error}"),
    })?;
    serde_json::from_str(&body).map_err(M1ndError::Serde)
}

fn validate_mission_id(mission_id: &str) -> M1ndResult<()> {
    let valid = mission_id.starts_with("msn_")
        && mission_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
    if valid {
        Ok(())
    } else {
        Err(M1ndError::InvalidParams {
            tool: "mission".into(),
            detail: "mission_id must be a generated msn_* id with no path separators".into(),
        })
    }
}

fn ensure_agent(mission: &MissionState, agent_id: &str) -> M1ndResult<()> {
    if mission.agent_id == agent_id {
        Ok(())
    } else {
        Err(M1ndError::InvalidParams {
            tool: "mission".into(),
            detail: format!(
                "mission {} belongs to agent_id {}; got {}",
                mission.mission_id, mission.agent_id, agent_id
            ),
        })
    }
}

fn validate_mission_start_input(input: &MissionStartInput) -> M1ndResult<()> {
    if input.repo.trim().is_empty() {
        return invalid_start("repo must not be empty");
    }
    if input.task.trim().is_empty() {
        return invalid_start("task must not be empty");
    }
    validate_allowed(
        "mode",
        &input.mode,
        &[
            "bug_hunt",
            "review",
            "refactor",
            "docs_drift",
            "architecture",
            "release",
        ],
    )?;
    validate_allowed("budget", &input.budget, &["short", "normal", "deep"])?;
    validate_allowed("risk", &input.risk, &["low", "medium", "high"])?;
    Ok(())
}

fn validate_allowed(field: &str, value: &str, allowed: &[&str]) -> M1ndResult<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        invalid_start(&format!(
            "{field} must be one of {}; got {value}",
            allowed.join(", ")
        ))
    }
}

fn invalid_start<T>(detail: &str) -> M1ndResult<T> {
    Err(M1ndError::InvalidParams {
        tool: "mission_start".into(),
        detail: detail.to_string(),
    })
}

fn build_mission_id(now: u64, agent_id: &str, task: &str) -> String {
    let slug = format!("{agent_id}-{task}")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(18)
        .collect::<String>()
        .to_ascii_lowercase();
    if slug.is_empty() {
        format!("msn_{now}")
    } else {
        format!("msn_{now}_{slug}")
    }
}

fn graph_state(state: &SessionState) -> Value {
    let graph = state.graph.read();
    json!({
        "node_count": graph.num_nodes(),
        "edge_count": graph.num_edges(),
        "finalized": graph.finalized,
        "graph_generation": state.graph_generation,
        "workspace_root": state.workspace_root.clone(),
        "workspace_root_source": state.workspace_root_source.clone(),
        "runtime_root": state.runtime_root.to_string_lossy().to_string(),
    })
}

fn trust_projection(graph_state: &Value, requested_repo: &str) -> Value {
    let node_count = graph_state
        .get("node_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let finalized = graph_state
        .get("finalized")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let workspace_root = graph_state
        .get("workspace_root")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let scope_ok = !workspace_root.is_empty()
        && (requested_repo.starts_with(workspace_root)
            || workspace_root.starts_with(requested_repo));
    let graph_freshness = if node_count == 0 {
        "needs_ingest"
    } else if !scope_ok {
        "workspace_binding_unverified"
    } else if finalized {
        "graph_present"
    } else {
        "not_finalized"
    };
    json!({
        "scope_ok": scope_ok,
        "graph_freshness": graph_freshness,
        "binding": if scope_ok { "repo_matches_workspace_binding" } else { "mission_repo_may_differ_from_workspace_binding" },
    })
}

fn context_guard_projection(graph_state: &Value, requested_repo: &str) -> Value {
    let workspace_root = graph_state
        .get("workspace_root")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let workspace_match = !workspace_root.is_empty()
        && (requested_repo.starts_with(workspace_root)
            || workspace_root.starts_with(requested_repo));
    json!({
        "schema": "m1nd-mission-context-guard-v1",
        "workspace_match": workspace_match,
        "requested_repo": requested_repo,
        "workspace_root": workspace_root,
        "workspace_root_source": graph_state
            .get("workspace_root_source")
            .cloned()
            .unwrap_or(Value::Null),
        "runtime_root": graph_state
            .get("runtime_root")
            .cloned()
            .unwrap_or(Value::Null),
        "graph_generation": graph_state
            .get("graph_generation")
            .cloned()
            .unwrap_or(Value::Null),
        "binary": {
            "name": "m1nd-mcp",
            "version": env!("CARGO_PKG_VERSION")
        },
        "non_claims": [
            "mission context guard does not rebind the MCP host",
            "mission context guard does not ingest or mutate graph contents",
            "mission context guard does not prove the requested repo is the correct task target"
        ]
    })
}

fn route_for(mode: &str, budget: &str, risk: &str) -> String {
    match (mode, budget, risk) {
        ("refactor", _, "high") | ("refactor", "deep", _) => "risk_first".into(),
        ("docs_drift", _, _) => "docs_binding".into(),
        ("architecture", "deep", _) => "architecture_survey".into(),
        ("release", _, _) => "release_preflight".into(),
        ("bug_hunt", "short", _) | ("review", "short", _) => "short_audit".into(),
        ("bug_hunt", _, _) | ("review", _, _) => "trained_audit".into(),
        _ => "balanced".into(),
    }
}

fn budget_envelope(budget: &str) -> Value {
    match budget {
        "short" => json!({
            "max_tool_calls": 8,
            "max_files_read": 5,
            "soft_deadline_ms": 90_000
        }),
        "deep" => json!({
            "max_tool_calls": 32,
            "max_files_read": 20,
            "soft_deadline_ms": 600_000
        }),
        _ => json!({
            "max_tool_calls": 16,
            "max_files_read": 10,
            "soft_deadline_ms": 240_000
        }),
    }
}

fn expected_phases(mode: &str) -> Vec<&'static str> {
    match mode {
        "release" => vec!["orient", "verify", "gate", "close"],
        "docs_drift" => vec!["orient", "bind", "verify", "close"],
        _ => vec!["locate", "verify", "report"],
    }
}

fn starter_moves(mission: &MissionState, graph_state: &Value) -> Vec<Value> {
    let node_count = graph_state
        .get("node_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if node_count == 0 {
        return vec![json!({
            "tool": "ingest",
            "target": mission.repo,
            "why": "mission graph has zero nodes; retrieval cannot be trusted before ingest"
        })];
    }
    match mission.route.as_str() {
        "short_audit" | "trained_audit" => vec![
            json!({
                "tool": "search",
                "target": mission.task,
                "why": "cheap exact/semantic orientation before direct proof"
            }),
            json!({
                "tool": "mission_next",
                "target": mission.mission_id,
                "why": "force a phase decision after one orientation event"
            }),
        ],
        "risk_first" => vec![json!({
            "tool": "validate_plan",
            "target": mission.task,
            "why": "risky changes should surface gaps and hotspots before edits"
        })],
        _ => vec![json!({
            "tool": "audit",
            "target": mission.repo,
            "why": "repo-level orientation is the cheapest reliable first move for this mode"
        })],
    }
}

fn non_goals_for(mode: &str) -> Vec<&'static str> {
    match mode {
        "bug_hunt" => vec!["public performance claim", "complete security audit"],
        "release" => vec!["host cache refresh proof", "graph repair proof"],
        "architecture" => vec!["runtime correctness proof", "full refactor plan"],
        _ => vec!["unbounded exploration", "production readiness claim"],
    }
}

fn append_event(mission: &mut MissionState, event: Value, source: &str) -> String {
    let mut object = match event {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("payload".into(), other);
            map
        }
    };
    let generated_id = format!("evt_{}", mission.events.len() + 1);
    let event_id = object
        .get("event_id")
        .and_then(Value::as_str)
        .filter(|value| is_safe_record_id(value))
        .map(str::to_string)
        .unwrap_or(generated_id);
    object.insert("event_id".into(), Value::String(event_id.clone()));
    object
        .entry("schema")
        .or_insert_with(|| Value::String(EVENT_SCHEMA.to_string()));
    object
        .entry("source")
        .or_insert_with(|| Value::String(source.to_string()));
    object
        .entry("observed_at_ms")
        .or_insert_with(|| Value::Number(now_ms().into()));
    let kind = object
        .get("event")
        .and_then(Value::as_str)
        .or_else(|| object.get("type").and_then(Value::as_str))
        .or_else(|| object.get("tool").and_then(Value::as_str))
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    object
        .entry("evidence_class")
        .or_insert_with(|| Value::String(evidence_class_for_kind(&kind).to_string()));
    mission.events.push(Value::Object(object));
    event_id
}

fn normalize_mission_event_value(
    event: Value,
    payload: Option<Value>,
    outcome: Option<String>,
    agent_confidence: Option<f32>,
) -> Value {
    let mut object = match event {
        Value::Object(map) => map,
        Value::String(kind) => {
            let mut map = Map::new();
            map.insert("event".into(), Value::String(kind));
            map
        }
        other => {
            let mut map = Map::new();
            map.insert("payload".into(), other);
            map
        }
    };
    if let Some(payload) = payload {
        object.entry("payload").or_insert(payload);
    }
    if let Some(outcome) = outcome {
        object.entry("outcome").or_insert(Value::String(outcome));
    }
    if let Some(confidence) = agent_confidence {
        object
            .entry("agent_confidence")
            .or_insert_with(|| json!(confidence));
    }
    Value::Object(object)
}

#[derive(Default)]
struct EventAnalysis {
    graph_events: usize,
    direct_events: usize,
    verify_events: usize,
    coverage_sweep_events: usize,
    repeated_graph_queries: bool,
}

fn analyze_events(mission: &MissionState) -> EventAnalysis {
    let mut analysis = EventAnalysis::default();
    let mut graph_query_streak = 0usize;
    for event in &mission.events {
        let kind = event_kind(event).to_ascii_lowercase();
        if is_graph_kind(&kind) {
            analysis.graph_events += 1;
            graph_query_streak += 1;
        } else {
            graph_query_streak = 0;
        }
        if is_direct_kind(&kind) {
            analysis.direct_events += 1;
        }
        if kind.contains("verify") || kind.contains("test") {
            analysis.verify_events += 1;
        }
        if is_coverage_sweep_kind(&kind) {
            analysis.coverage_sweep_events += 1;
        }
        if graph_query_streak >= 2 {
            analysis.repeated_graph_queries = true;
        }
    }
    analysis
}

fn event_kind(event: &Value) -> String {
    for key in ["event", "type", "tool", "move_type"] {
        if let Some(value) = event.get(key).and_then(Value::as_str) {
            return value.to_string();
        }
    }
    "unknown".into()
}

fn next_move(
    mission: &MissionState,
    analysis: &EventAnalysis,
) -> (String, Value, Vec<&'static str>, Option<String>) {
    let budget = budget_consumed(mission);
    if analysis.repeated_graph_queries || (analysis.graph_events > 0 && analysis.direct_events == 0)
    {
        return (
            "verify".into(),
            json!({
                "type": "read_file",
                "target": mission.repo,
                "why": "graph orientation has already happened; switch to direct proof before claiming",
                "evidence_required": "direct_read"
            }),
            vec!["activate", "seek"],
            Some("graph budget is spent until a direct source read, test, or runtime probe is observed".into()),
        );
    }
    if analysis.direct_events > 0 && mission.claims.is_empty() {
        return (
            "verify".into(),
            json!({
                "type": "claim",
                "tool": "mission_verify",
                "why": "direct evidence exists; turn it into a candidate claim before reporting",
                "evidence_required": "evidence_refs"
            }),
            vec![],
            None,
        );
    }
    if mission
        .claims
        .iter()
        .any(|claim| claim.verdict == "verified_for_mission")
    {
        if mission.mode == "bug_hunt" && analysis.coverage_sweep_events == 0 {
            return (
                "verify".into(),
                json!({
                    "type": "direct_sweep",
                    "target": mission.repo,
                    "why": "bug_hunt missions must do one negative-space sweep after verified findings before closing",
                    "evidence_required": "boundary_or_contract_sweep",
                    "suggested_focus": [
                        "public contracts and docs",
                        "boundary values",
                        "error paths",
                        "async or concurrency semantics",
                        "helper/exported APIs not covered by current claims"
                    ]
                }),
                vec!["activate", "seek"],
                Some("verified findings exist, but bug_hunt mode needs a final direct coverage sweep before close".into()),
            );
        }
        return (
            "report".into(),
            json!({
                "type": "close",
                "tool": "mission_close",
                "why": "at least one claim is verified; close with explicit gaps/non-claims or continue only if scope requires it"
            }),
            vec!["activate", "seek"],
            None,
        );
    }
    if budget > 0.6 {
        return (
            "verify".into(),
            json!({
                "type": "read_file",
                "target": mission.repo,
                "why": "mission has consumed most of its budget without direct evidence",
                "evidence_required": "direct_read"
            }),
            vec!["activate", "seek"],
            Some("over 60% of mission budget is consumed".into()),
        );
    }
    (
        "locate".into(),
        json!({
            "type": "graph_query",
            "tool": if mission.route == "short_audit" { "search" } else { "audit" },
            "target": if mission.route == "short_audit" { &mission.task } else { &mission.repo },
            "why": "one cheap orientation move is still allowed before direct proof",
            "evidence_required": "orientation_only"
        }),
        vec![],
        None,
    )
}

/// Grade a claim's evidence. `verify_roots` are the candidate repo roots a cited
/// path is resolved against so a bare `direct` LABEL only earns the full `direct`
/// grade when it carries a verifiable signal — a path that actually exists on
/// disk (or a referenced recorded mission event). A label with no backing event
/// and no resolvable path grades `direct_unverified`: the agent asserted a read
/// that cannot be corroborated, so the claim must not close on the label alone.
fn classify_evidence(
    evidence_refs: &[String],
    events: &[Value],
    verify_roots: &[PathBuf],
) -> String {
    let refs_lower = evidence_refs
        .iter()
        .map(|reference| reference.to_ascii_lowercase())
        .collect::<Vec<_>>();
    // A bare label claiming a direct read: trust it as `direct` ONLY when a cited
    // path is real (verifiable beyond the label). Otherwise it is label-only.
    let direct_labeled: Vec<&String> = evidence_refs
        .iter()
        .filter(|reference| is_direct_kind(&reference.to_ascii_lowercase()))
        .collect();
    if !direct_labeled.is_empty() {
        if direct_labeled
            .iter()
            .any(|reference| direct_ref_has_verifiable_path(reference, verify_roots))
        {
            return "direct".into();
        }
        // Fall through: a referenced recorded event below can still corroborate
        // the read; only if none does is this graded `direct_unverified`.
        if !events
            .iter()
            .any(|event| event_is_referenced(event, &refs_lower))
        {
            return "direct_unverified".into();
        }
    }
    for event in events {
        if !event_is_referenced(event, &refs_lower) {
            continue;
        }
        let kind = event_kind(event).to_ascii_lowercase();
        let evidence_class = event
            .get("evidence_class")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        if is_direct_kind(&kind)
            || evidence_class == "direct"
            || evidence_class == "direct_coverage_sweep"
        {
            return "direct".into();
        }
        if is_graph_kind(&kind) || evidence_class == "graph_orientation" {
            return "graph_only".into();
        }
    }
    if evidence_refs
        .iter()
        .any(|reference| is_graph_kind(&reference.to_ascii_lowercase()))
    {
        return "graph_only".into();
    }
    "inferred".into()
}

fn event_is_referenced(event: &Value, refs_lower: &[String]) -> bool {
    let Some(event_id) = event.get("event_id").and_then(Value::as_str) else {
        return false;
    };
    let event_id = event_id.to_ascii_lowercase();
    refs_lower
        .iter()
        .any(|reference| reference == &event_id || reference.contains(&format!("event:{event_id}")))
}

fn is_graph_kind(kind: &str) -> bool {
    kind.contains("activate")
        || kind.contains("seek")
        || kind.contains("audit")
        || kind.contains("graph_query")
}

fn is_direct_kind(kind: &str) -> bool {
    kind.contains("file_read")
        || kind.contains("read_file")
        || kind.contains("view")
        || kind.contains("test_run")
        || kind.contains("run_test")
        || kind.contains("compiler")
        || kind.contains("runtime_probe")
        || kind.contains("rg")
        || kind.contains("grep")
}

/// Does a direct-labeled evidence ref cite a path that actually EXISTS under one
/// of the verify roots? This is the verifiable signal that separates a real read
/// from a forged label: `file_read:src/auth.rs:42` grades `direct` only when
/// `src/auth.rs` exists. Candidate path tokens are pulled from the ref by
/// stripping a leading `kind:` prefix and any trailing `:line[:col]` locator, and
/// by splitting on whitespace (`read_file src/auth.rs`). An absolute cited path
/// is checked as-is; a relative one is joined onto each root.
fn direct_ref_has_verifiable_path(reference: &str, verify_roots: &[PathBuf]) -> bool {
    for candidate in candidate_paths_from_ref(reference) {
        let candidate_path = Path::new(&candidate);
        if candidate_path.is_absolute() {
            if candidate_path.exists() {
                return true;
            }
            continue;
        }
        for root in verify_roots {
            if root.join(&candidate).exists() {
                return true;
            }
        }
    }
    false
}

/// Extract candidate filesystem paths from a direct evidence ref. Handles the two
/// shipped shapes — `kind:path:line[:col]` and `kind path` — without inventing a
/// grammar: it yields the substring after the first `:` (locator trimmed) and the
/// last whitespace-delimited token, so a real path in either form is found.
fn candidate_paths_from_ref(reference: &str) -> Vec<String> {
    let mut out = Vec::new();
    let trimmed = reference.trim();

    // Shape `kind:path:line:col` — take everything after the first colon, then
    // strip a trailing `:<digits>` locator (line, then optional col).
    if let Some((_, rest)) = trimmed.split_once(':') {
        let mut path = rest.trim();
        for _ in 0..2 {
            if let Some((head, tail)) = path.rsplit_once(':') {
                if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
                    path = head;
                    continue;
                }
            }
            break;
        }
        let path = path.trim();
        if !path.is_empty() {
            out.push(path.to_string());
        }
    }

    // Shape `kind path` — the last whitespace-delimited token, locator stripped.
    if let Some(token) = trimmed.split_whitespace().next_back() {
        let mut token = token;
        for _ in 0..2 {
            if let Some((head, tail)) = token.rsplit_once(':') {
                if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
                    token = head;
                    continue;
                }
            }
            break;
        }
        if !token.is_empty() && !out.iter().any(|p| p == token) {
            out.push(token.to_string());
        }
    }

    out
}

fn is_coverage_sweep_kind(kind: &str) -> bool {
    kind.contains("coverage_sweep")
        || kind.contains("boundary_sweep")
        || kind.contains("edge_case_sweep")
        || kind.contains("negative_space_sweep")
        || kind.contains("public_contract_sweep")
        || kind.contains("followup_sweep")
}

fn evidence_class_for_kind(kind: &str) -> &'static str {
    if is_direct_kind(kind) {
        "direct"
    } else if is_coverage_sweep_kind(kind) {
        "direct_coverage_sweep"
    } else if is_graph_kind(kind) {
        "graph_orientation"
    } else {
        "inferred_or_unclassified"
    }
}

fn budget_consumed(mission: &MissionState) -> f64 {
    let max_tool_calls = mission
        .budget_envelope
        .get("max_tool_calls")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as f64;
    ((mission.events.len() as f64) / max_tool_calls).min(1.0)
}

fn tools_observed(mission: &MissionState) -> Vec<String> {
    let mut tools = mission
        .events
        .iter()
        .filter_map(|event| {
            event
                .get("tool")
                .and_then(Value::as_str)
                .or_else(|| event.get("event").and_then(Value::as_str))
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    tools.sort();
    tools.dedup();
    tools
}

fn verified_claims_json(mission: &MissionState) -> Vec<Value> {
    mission
        .claims
        .iter()
        .filter(|claim| claim.verdict == "verified_for_mission")
        .map(|claim| {
            json!({
                "claim_id": claim.claim_id.clone(),
                "claim": claim.claim.clone(),
                "evidence_refs": claim.evidence_refs.clone(),
                "evidence_grade": claim.evidence_grade.clone(),
            })
        })
        .collect()
}

fn rejected_claims_json(mission: &MissionState) -> Vec<Value> {
    mission
        .claims
        .iter()
        .filter(|claim| claim.verdict != "verified_for_mission")
        .map(|claim| {
            json!({
                "claim_id": claim.claim_id.clone(),
                "claim": claim.claim.clone(),
                "verdict": claim.verdict.clone(),
                "missing": claim.missing.clone(),
            })
        })
        .collect()
}

fn open_hypotheses(mission: &MissionState) -> Vec<Value> {
    mission
        .claims
        .iter()
        .filter(|claim| claim.verdict != "verified_for_mission")
        .map(|claim| {
            json!({
                "claim_id": claim.claim_id.clone(),
                "claim": claim.claim.clone(),
                "missing": claim.missing.clone(),
            })
        })
        .collect()
}

fn dead_paths(mission: &MissionState) -> Vec<Value> {
    mission
        .events
        .iter()
        .filter(|event| {
            let text = event.to_string().to_ascii_lowercase();
            text.contains("dead_path") || text.contains("dead path") || text.contains("ruled_out")
        })
        .cloned()
        .collect()
}

fn files_read(mission: &MissionState) -> Vec<String> {
    unique_event_strings(mission, &["path", "file", "target"], |kind| {
        is_direct_kind(kind)
    })
}

fn graph_anchors(mission: &MissionState) -> Vec<String> {
    unique_event_strings(mission, &["target", "query", "node_id", "tool"], |kind| {
        is_graph_kind(kind)
    })
}

fn unique_event_strings<F>(mission: &MissionState, keys: &[&str], predicate: F) -> Vec<String>
where
    F: Fn(&str) -> bool,
{
    let mut values = mission
        .events
        .iter()
        .filter(|event| predicate(&event_kind(event).to_ascii_lowercase()))
        .filter_map(|event| {
            keys.iter()
                .find_map(|key| event_string_value(event, key))
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn event_string_value<'a>(event: &'a Value, key: &str) -> Option<&'a str> {
    event
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| event.get("payload")?.get(key)?.as_str())
}

fn event_digest(events: &[Value]) -> String {
    let body = serde_json::to_string(events).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut hasher);
    format!("hash64:{:016x}", hasher.finish())
}

fn is_safe_record_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_id_rejects_path_escape() {
        assert!(validate_mission_id("msn_123_ok").is_ok());
        assert!(validate_mission_id("../msn_bad").is_err());
        assert!(validate_mission_id("msn_bad/path").is_err());
    }

    #[test]
    fn graph_only_evidence_is_not_enough() {
        let grade = classify_evidence(&["seek:auth flow".to_string()], &[], &[]);
        assert_eq!(grade, "graph_only");
    }

    #[test]
    fn direct_label_with_real_path_wins() {
        // A direct label whose cited path actually EXISTS earns the full grade.
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("auth.rs");
        fs::write(&file, "// code").unwrap();
        let grade = classify_evidence(
            &["file_read:auth.rs:42".to_string()],
            &[],
            &[temp.path().to_path_buf()],
        );
        assert_eq!(grade, "direct");
    }

    /// FIX 4a — the forge: a direct LABEL citing a path that does NOT exist, with
    /// no backing recorded event, used to grade full `direct` and close the claim.
    /// It must now grade `direct_unverified` — the label is not enough.
    #[test]
    fn forged_direct_label_with_no_real_path_is_downgraded() {
        let temp = tempfile::tempdir().unwrap();
        // The path does not exist under the (real) root — a lying label.
        let grade = classify_evidence(
            &["file_read:src/totally-made-up.rs:42".to_string()],
            &[],
            &[temp.path().to_path_buf()],
        );
        assert_eq!(
            grade, "direct_unverified",
            "a direct label with no existing path and no recorded event is unverifiable"
        );
    }

    #[test]
    fn unrelated_direct_event_does_not_prove_claim() {
        let events = vec![json!({
            "event_id": "evt_1",
            "event": "file_read",
            "path": "src/auth.rs",
            "evidence_class": "direct"
        })];

        let grade = classify_evidence(&["seek:auth flow".to_string()], &events, &[]);

        assert_eq!(grade, "graph_only");
    }

    #[test]
    fn referenced_direct_event_proves_claim() {
        let events = vec![json!({
            "event_id": "evt_1",
            "event": "file_read",
            "path": "src/auth.rs",
            "evidence_class": "direct"
        })];

        // No path resolution needed: a direct label backed by a REFERENCED recorded
        // event is corroborated (the read really happened this session).
        let grade = classify_evidence(&["event:evt_1".to_string()], &events, &[]);

        assert_eq!(grade, "direct");
    }

    /// A bare direct label is still trusted when a REFERENCED recorded event
    /// corroborates it — even if the path can't be resolved (no roots). The forge
    /// only downgrades when NEITHER a real path NOR a backing event exists.
    #[test]
    fn direct_label_backed_by_referenced_event_stays_direct() {
        let events = vec![json!({
            "event_id": "evt_9",
            "event": "file_read",
            "path": "src/auth.rs",
            "evidence_class": "direct"
        })];
        let grade = classify_evidence(
            &[
                "file_read:src/auth.rs".to_string(),
                "event:evt_9".to_string(),
            ],
            &events,
            &[],
        );
        assert_eq!(grade, "direct");
    }

    #[test]
    fn append_event_adds_schema_and_evidence_class() {
        let mut mission = test_mission("review");
        let event_id = append_event(
            &mut mission,
            json!({"event": "file_read", "path": "src/auth.rs"}),
            "mission_event",
        );

        assert_eq!(event_id, "evt_1");
        assert_eq!(mission.events[0]["schema"], EVENT_SCHEMA);
        assert_eq!(mission.events[0]["evidence_class"], "direct");
    }

    #[test]
    fn mission_event_normalizes_split_payload_fields() {
        let event = normalize_mission_event_value(
            json!("file_read"),
            Some(json!({"path": "src/auth.rs", "lines": [42, 55]})),
            Some("hypothesis_supported".into()),
            Some(0.72),
        );
        let mut mission = test_mission("review");
        append_event(&mut mission, event, "mission_event");

        assert_eq!(mission.events[0]["event"], "file_read");
        assert_eq!(mission.events[0]["payload"]["path"], "src/auth.rs");
        assert_eq!(mission.events[0]["outcome"], "hypothesis_supported");
        assert_eq!(mission.events[0]["agent_confidence"], json!(0.72f32));
        assert_eq!(mission.events[0]["evidence_class"], "direct");
    }

    #[test]
    fn mission_event_split_fields_do_not_override_object_event_fields() {
        let event = normalize_mission_event_value(
            json!({
                "event": "test_run",
                "payload": {"command": "cargo test"},
                "outcome": "failed"
            }),
            Some(json!({"path": "ignored.rs"})),
            Some("hypothesis_supported".into()),
            Some(0.9),
        );

        assert_eq!(event["event"], "test_run");
        assert_eq!(event["payload"]["command"], "cargo test");
        assert_eq!(event["outcome"], "failed");
        assert_eq!(event["agent_confidence"], json!(0.9f32));
    }

    #[test]
    fn unique_event_strings_reads_nested_payload_fields() {
        let mut mission = test_mission("review");
        append_event(
            &mut mission,
            normalize_mission_event_value(
                json!("file_read"),
                Some(json!({"path": "src/auth.rs"})),
                Some("read direct source".into()),
                None,
            ),
            "mission_event",
        );

        let files = files_read(&mission);

        assert_eq!(files, vec!["src/auth.rs".to_string()]);
    }

    #[test]
    fn event_digest_changes_with_events() {
        let empty = event_digest(&[]);
        let one = event_digest(&[json!({"event": "file_read"})]);

        assert_ne!(empty, one);
        assert!(one.starts_with("hash64:"));
    }

    #[test]
    fn mission_start_rejects_unknown_mode() {
        let input = MissionStartInput {
            agent_id: "jimi".into(),
            repo: "/tmp/project".into(),
            task: "audit".into(),
            mode: "wander".into(),
            budget: "normal".into(),
            risk: "medium".into(),
            parent_mission_id: None,
            evidence_link: None,
        };
        assert!(validate_mission_start_input(&input).is_err());
    }

    #[test]
    fn bug_hunt_requires_coverage_sweep_after_verified_claim() {
        let mut mission = test_mission("bug_hunt");
        mission
            .events
            .push(json!({"event": "file_read", "path": "index.js"}));
        mission.claims.push(verified_claim());

        let analysis = analyze_events(&mission);
        let (phase, move_value, do_not, warning) = next_move(&mission, &analysis);

        assert_eq!(phase, "verify");
        assert_eq!(move_value["type"], "direct_sweep");
        assert!(move_value["suggested_focus"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "boundary values"));
        assert!(do_not.contains(&"seek"));
        assert!(warning
            .unwrap()
            .contains("final direct coverage sweep before close"));
    }

    #[test]
    fn bug_hunt_can_close_after_coverage_sweep() {
        let mut mission = test_mission("bug_hunt");
        mission
            .events
            .push(json!({"event": "file_read", "path": "index.js"}));
        mission.events.push(json!({
            "event": "boundary_sweep",
            "outcome": "checked public docs, boundary values, and helper APIs"
        }));
        mission.claims.push(verified_claim());

        let analysis = analyze_events(&mission);
        let (phase, move_value, do_not, warning) = next_move(&mission, &analysis);

        assert_eq!(phase, "report");
        assert_eq!(move_value["type"], "close");
        assert!(do_not.contains(&"activate"));
        assert!(warning.is_none());
    }

    #[test]
    fn review_can_close_after_verified_claim_without_extra_sweep() {
        let mut mission = test_mission("review");
        mission
            .events
            .push(json!({"event": "file_read", "path": "src/auth.rs"}));
        mission.claims.push(verified_claim());

        let analysis = analyze_events(&mission);
        let (phase, move_value, _, _) = next_move(&mission, &analysis);

        assert_eq!(phase, "report");
        assert_eq!(move_value["type"], "close");
    }

    fn test_mission(mode: &str) -> MissionState {
        MissionState {
            schema: STATE_SCHEMA.into(),
            mission_id: "msn_123_jimi".into(),
            agent_id: "jimi".into(),
            repo: "/tmp/project".into(),
            task: "audit behavioral defects".into(),
            mode: mode.into(),
            budget: "normal".into(),
            risk: "medium".into(),
            parent_mission_id: None,
            evidence_link: None,
            evidence_projection_gaps: Vec::new(),
            route: route_for(mode, "normal", "medium"),
            phase: "verify".into(),
            status: "active".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
            next_step_id: 1,
            budget_envelope: budget_envelope("normal"),
            graph_state_at_start: json!({}),
            context_guard_at_start: json!({}),
            events: Vec::new(),
            claims: Vec::new(),
            handoffs: Vec::new(),
            non_claims: Vec::new(),
        }
    }

    fn verified_claim() -> MissionClaimState {
        MissionClaimState {
            claim_id: "clm_1".into(),
            claim: "verified finding".into(),
            evidence_refs: vec!["file_read:index.js:1".into()],
            evidence_grade: "direct".into(),
            verdict: "verified_for_mission".into(),
            missing: Vec::new(),
            confidence: Some(0.9),
            created_at_ms: 1,
        }
    }

    fn build_session(root: &std::path::Path) -> SessionState {
        use crate::server::McpConfig;
        use m1nd_core::domain::DomainConfig;
        use m1nd_core::graph::Graph;

        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
    }

    fn start_mission(state: &mut SessionState, agent_id: &str) -> String {
        let repo = state
            .workspace_root
            .clone()
            .unwrap_or_else(|| state.runtime_root.to_string_lossy().to_string());
        let out = handle_mission_start(
            state,
            MissionStartInput {
                agent_id: agent_id.into(),
                repo,
                task: "audit behavioral defects".into(),
                mode: "review".into(),
                budget: "normal".into(),
                risk: "medium".into(),
                parent_mission_id: None,
                evidence_link: None,
            },
        )
        .expect("mission_start");
        out["mission_id"].as_str().expect("mission_id").to_string()
    }

    /// FIX 4a (end-to-end) — a forged direct label must NOT close a claim. Before
    /// the fix, `evidence_refs:["file_read:src/nope.rs:1"]` graded `direct` and the
    /// verdict was `verified_for_mission` with the self-reported confidence intact.
    /// Now the label is unverifiable (no real path, no recorded event), so the
    /// verdict is `insufficient_evidence` and confidence is capped.
    #[test]
    fn forged_direct_label_does_not_verify_a_mission_claim() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_session(temp.path());
        let mission_id = start_mission(&mut state, "jimi");

        let out = handle_mission_verify(
            &mut state,
            MissionVerifyInput {
                agent_id: "jimi".into(),
                mission_id: mission_id.clone(),
                claim: "auth is safe".into(),
                evidence_refs: vec!["file_read:src/nope.rs:1".into()],
                confidence: Some(0.99),
            },
        )
        .expect("mission_verify");

        assert_eq!(
            out["verdict"], "insufficient_evidence",
            "a forged direct label must not verify a claim"
        );
        assert_eq!(out["evidence_grade"], "direct_unverified");

        // The persisted claim's confidence was capped below the self-reported 0.99.
        let mission = load_mission(&state, &mission_id).expect("reload");
        let claim = mission.claims.last().expect("claim recorded");
        assert!(
            claim.confidence.unwrap() <= DIRECT_UNVERIFIED_CONFIDENCE_CAP,
            "label-only confidence must be capped, got {:?}",
            claim.confidence
        );
    }

    /// A direct label citing a path that ACTUALLY exists still verifies — the fix
    /// gates on a verifiable signal, it does not break honest evidence.
    #[test]
    fn direct_label_with_existing_path_still_verifies() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_session(temp.path());
        // The evidence path is resolved against the runtime_root (an ingest root).
        let real = state.runtime_root.join("real_evidence.rs");
        fs::write(&real, "// real code read by the agent").expect("write evidence");
        let mission_id = start_mission(&mut state, "jimi");

        let out = handle_mission_verify(
            &mut state,
            MissionVerifyInput {
                agent_id: "jimi".into(),
                mission_id,
                claim: "the read really happened".into(),
                evidence_refs: vec!["file_read:real_evidence.rs:1".into()],
                confidence: Some(0.9),
            },
        )
        .expect("mission_verify");

        assert_eq!(out["verdict"], "verified_for_mission");
        assert_eq!(out["evidence_grade"], "direct");
    }

    /// Concurrent callers append to one mission through the runtime's unique
    /// writable brain actor. Each callback performs multiple load→mutate→save
    /// cycles; the final count proves actor admission serialized the complete
    /// turns without manufacturing forbidden sibling SessionState writers.
    #[test]
    fn concurrent_mission_writes_serialize_and_lose_no_events() {
        use std::sync::Arc;
        use std::thread;

        const THREADS: usize = 8;
        const PER_THREAD: usize = 60;

        let temp = tempfile::tempdir().expect("tempdir");
        let root = Arc::new(temp.path().to_path_buf());

        // Create the mission once, then reload it from sibling sessions.
        let mission_id = {
            let mut state = build_session(root.as_path());
            start_mission(&mut state, "jimi")
        };

        let state = build_session(root.as_path());
        let runtime_root = state.runtime_root.clone();
        let session = Arc::new(crate::brain_runtime::BrainSessionCell::new(state));
        let actor = crate::brain_runtime::BrainActorHandle::start(
            "mission-event-race".to_string(),
            session,
            runtime_root.join(crate::brain_runtime::BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(crate::brain_runtime::UnboundBrainCheckpointAuthority),
            THREADS,
            None,
        )
        .expect("start brain actor");

        let mut handles = Vec::new();
        for i in 0..THREADS {
            let mission_id = mission_id.clone();
            let actor = Arc::clone(&actor);
            handles.push(thread::spawn(move || {
                actor
                    .try_execute(true, move |state| {
                        for j in 0..PER_THREAD {
                            handle_mission_event(
                                state,
                                MissionEventInput {
                                    agent_id: "jimi".into(),
                                    mission_id: mission_id.clone(),
                                    event: json!("file_read"),
                                    payload: Some(json!({"path": format!("t{i}_e{j}.rs")})),
                                    outcome: None,
                                    agent_confidence: None,
                                },
                            )
                            .map_err(|error| {
                                crate::runtime_jobs::RuntimeJobFailure::new(
                                    "mission_event_failed",
                                    error.to_string(),
                                )
                            })?;
                        }
                        Ok(())
                    })
                    .expect("serialized mission events");
            }));
        }
        for h in handles {
            h.join().expect("thread join");
        }

        // Reload through the same owner actor: every appended event survived.
        let mission = actor
            .try_execute(false, move |state| {
                load_mission(state, &mission_id).map_err(|error| {
                    crate::runtime_jobs::RuntimeJobFailure::new(
                        "mission_reload_failed",
                        error.to_string(),
                    )
                })
            })
            .expect("reload mission");
        assert_eq!(
            mission.events.len(),
            THREADS * PER_THREAD,
            "the lock must serialize concurrent RMW so no appended event is lost"
        );
        actor.stop().expect("stop brain actor");
    }
}
