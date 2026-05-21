use crate::protocol::layers::{
    MissionCloseInput, MissionEventInput, MissionHandoffInput, MissionNextInput, MissionStartInput,
    MissionVerifyInput,
};
use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const STATE_SCHEMA: &str = "m1nd-mission-control-state-v1";
const START_SCHEMA: &str = "m1nd-mission-start-v0";
const EVENT_SCHEMA: &str = "m1nd-mission-event-v1";
const NEXT_SCHEMA: &str = "m1nd-mission-next-v0";
const VERIFY_SCHEMA: &str = "m1nd-mission-verify-v0";
const HANDOFF_SCHEMA: &str = "m1nd-mission-handoff-v1";
const CLOSE_SCHEMA: &str = "m1nd-mission-proof-packet-v1";

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
    let mut mission = load_mission(state, &mission_id)?;
    ensure_agent(&mission, &agent_id)?;

    let event_value = normalize_mission_event_value(event, payload, outcome, agent_confidence);
    let event_id = append_event(&mut mission, event_value, "mission_event");
    mission.updated_at_ms = now_ms();
    save_mission(state, &mission)?;

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
    }))
}

pub fn handle_mission_next(state: &mut SessionState, input: MissionNextInput) -> M1ndResult<Value> {
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
    }))
}

pub fn handle_mission_verify(
    state: &mut SessionState,
    input: MissionVerifyInput,
) -> M1ndResult<Value> {
    let mut mission = load_mission(state, &input.mission_id)?;
    ensure_agent(&mission, &input.agent_id)?;

    let grade = classify_evidence(&input.evidence_refs, &mission.events);
    let mut missing = Vec::new();
    let verdict = if grade == "direct" {
        "verified_for_mission"
    } else {
        missing.push("direct_source_read_or_runtime_probe".to_string());
        "insufficient_evidence"
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
        confidence: input.confidence,
        created_at_ms: now_ms(),
    };
    mission.claims.push(claim);
    mission.phase = "verify".into();
    mission.updated_at_ms = now_ms();
    save_mission(state, &mission)?;

    Ok(json!({
        "schema": VERIFY_SCHEMA,
        "mission_id": mission.mission_id,
        "claim_id": claim_id,
        "verdict": verdict,
        "evidence_grade": grade,
        "missing": missing,
        "next_required_move": next_required_move,
        "non_claims": mission.non_claims,
    }))
}

pub fn handle_mission_handoff(
    state: &mut SessionState,
    input: MissionHandoffInput,
) -> M1ndResult<Value> {
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

    Ok(handoff)
}

pub fn handle_mission_close(
    state: &mut SessionState,
    input: MissionCloseInput,
) -> M1ndResult<Value> {
    let mut mission = load_mission(state, &input.mission_id)?;
    ensure_agent(&mission, &input.agent_id)?;

    mission.status = "closed".into();
    mission.phase = "closed".into();
    mission.updated_at_ms = now_ms();
    save_mission(state, &mission)?;

    let verified_claims = verified_claims_json(&mission);
    let rejected_claims = rejected_claims_json(&mission);
    let mut non_claims = mission.non_claims.clone();
    non_claims.extend(input.non_claims);
    non_claims.sort();
    non_claims.dedup();

    Ok(json!({
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
    }))
}

fn mission_dir(state: &SessionState) -> PathBuf {
    state.runtime_root.join("mission-control")
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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

fn classify_evidence(evidence_refs: &[String], events: &[Value]) -> String {
    let refs_lower = evidence_refs
        .iter()
        .map(|reference| reference.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if evidence_refs
        .iter()
        .any(|reference| is_direct_kind(&reference.to_ascii_lowercase()))
    {
        return "direct".into();
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
        let grade = classify_evidence(&["seek:auth flow".to_string()], &[]);
        assert_eq!(grade, "graph_only");
    }

    #[test]
    fn direct_evidence_wins() {
        let grade = classify_evidence(&["file_read:src/auth.rs:42".to_string()], &[]);
        assert_eq!(grade, "direct");
    }

    #[test]
    fn unrelated_direct_event_does_not_prove_claim() {
        let events = vec![json!({
            "event_id": "evt_1",
            "event": "file_read",
            "path": "src/auth.rs",
            "evidence_class": "direct"
        })];

        let grade = classify_evidence(&["seek:auth flow".to_string()], &events);

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

        let grade = classify_evidence(&["event:evt_1".to_string()], &events);

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
}
