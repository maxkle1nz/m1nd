//! Exact MCP-tool to semantic-action routing for the G2 policy boundary.
//!
//! Tool names are transport syntax; `m1nd-control` policy is expressed in
//! semantic actions. This module makes that translation total and fail-closed.
//! Argument-sensitive tools never fall back to a weaker action when a selector
//! is absent or unknown. Facts which depend on current owner state (for example
//! whether ingest roots would change) are supplied separately as trusted facts;
//! user JSON cannot assert them.

use std::collections::BTreeSet;

use m1nd_control::{
    m1nd10_action_catalog, ActionCatalogEntryV1, ActionId, AuthorityFloor, Effect, Ingress,
    RiskClass,
};
use serde_json::Value;
use std::error::Error;
use std::fmt;

/// The complete MCP registry expected by this source revision. A parity test
/// compares these names to `server::all_tool_schemas`; adding a callable tool
/// without policy routing fails CI.
pub const MCP_TOOL_ROUTE_NAMES: &[&str] = &[
    "orient",
    "north",
    "cockpit",
    "delegate",
    "debrief",
    "evidence_query",
    "am_i_stale",
    "activate",
    "impact",
    "missing",
    "why",
    "warmup",
    "counterfactual",
    "predict",
    "fingerprint",
    "drift",
    "learn",
    "ingest",
    "document_resolve",
    "document_provider_health",
    "document_bindings",
    "document_drift",
    "auto_ingest_start",
    "auto_ingest_stop",
    "auto_ingest_status",
    "auto_ingest_tick",
    "health",
    "session_handshake",
    "trust_selftest",
    "recovery_playbook",
    "doctor",
    "perspective_start",
    "perspective_routes",
    "perspective_inspect",
    "perspective_peek",
    "perspective_follow",
    "perspective_suggest",
    "perspective_affinity",
    "perspective_branch",
    "perspective_back",
    "perspective_compare",
    "perspective_list",
    "perspective_close",
    "seek",
    "focus",
    "scan",
    "timeline",
    "diverge",
    "trail_save",
    "trail_resume",
    "trail_merge",
    "trail_list",
    "hypothesize",
    "differential",
    "trace",
    "validate_plan",
    "federate",
    "antibody_scan",
    "antibody_list",
    "antibody_create",
    "flow_simulate",
    "epidemic",
    "tremor",
    "trust",
    "layers",
    "layer_inspect",
    "ghost_edges",
    "calibrate_predict",
    "calibrate_envelope",
    "taint_trace",
    "twins",
    "refactor_plan",
    "runtime_overlay",
    "heuristics_surface",
    "surgical_context",
    "apply",
    "view",
    "batch_view",
    "surgical_context_v2",
    "apply_batch",
    "edit_preview",
    "edit_commit",
    "search",
    "glob",
    "scan_all",
    "cross_verify",
    "soul_check",
    "soul_read",
    "coverage_session",
    "external_references",
    "federate_auto",
    "help",
    "mission_start",
    "mission_next",
    "mission_event",
    "mission_verify",
    "mission_handoff",
    "mission_close",
    "report",
    "audit",
    "daemon_start",
    "daemon_stop",
    "daemon_status",
    "daemon_tick",
    "alerts_list",
    "alerts_ack",
    "panoramic",
    "persist",
    "boot_memory",
    "metrics",
    "type_trace",
    "diagram",
    "memorize",
    "promote",
    "xray_retag",
    "xray_orient",
    "xray_gate",
    "xray_apply",
    "xray_paint",
    "xray_ledger",
    "system_blocks_snapshot",
    "skeleton_candidate",
    "system_blocks_seed_import",
    "system_blocks_ratify",
    "system_blocks_reconcile",
    "receipt_recompute",
    "system_blocks_archive",
    "system_blocks_delete",
    "candidate_edit",
    "candidate_lease",
    "candidate_naming",
    "mission_service",
    "external_mutation_service",
    "graph_ingest_preview",
    "authority_session_challenge",
    "authority_session_authenticate",
    "authority_authorize",
    "mission_spawn",
];

/// Current-state facts computed by the owner before classification. `None`
/// means the owner did not prove the fact, so the classifier refuses instead of
/// choosing the less privileged branch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrustedMcpRouteFacts {
    pub ingest_changes_roots: Option<bool>,
    pub auto_ingest_changes_roots: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedMcpActionV1 {
    pub tool_name: String,
    pub action: ActionId,
    pub effects: BTreeSet<Effect>,
    pub risk_class: RiskClass,
    pub authority_floor: AuthorityFloor,
}

#[derive(Debug)]
pub enum McpActionRouteError {
    UnknownTool {
        tool: String,
    },
    MissingTrustedFact {
        tool: String,
        fact: &'static str,
    },
    UnsupportedSelector {
        tool: String,
        field: &'static str,
        observed: String,
    },
    CatalogActionMissing {
        action: String,
    },
    CatalogIngressMismatch {
        action: String,
    },
    Catalog(String),
}

impl fmt::Display for McpActionRouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool { tool } => {
                write!(formatter, "unknown or unrouted MCP tool '{tool}'")
            }
            Self::MissingTrustedFact { tool, fact } => write!(
                formatter,
                "tool '{tool}' requires trusted owner fact '{fact}' before policy evaluation"
            ),
            Self::UnsupportedSelector {
                tool,
                field,
                observed,
            } => write!(
                formatter,
                "tool '{tool}' has unsupported selector {field}={observed}"
            ),
            Self::CatalogActionMissing { action } => write!(
                formatter,
                "semantic action '{action}' is absent from the canonical action catalog"
            ),
            Self::CatalogIngressMismatch { action } => write!(
                formatter,
                "semantic action '{action}' is not reachable from MCP ingress"
            ),
            Self::Catalog(detail) => {
                write!(
                    formatter,
                    "canonical action catalog failed validation: {detail}"
                )
            }
        }
    }
}

impl Error for McpActionRouteError {}

/// Return every semantic action the tool can select. This is used by parity
/// proof and policy generation; execution still calls [`classify_mcp_action`]
/// to select exactly one branch.
pub fn possible_mcp_actions(tool: &str) -> Option<Vec<&'static str>> {
    let bare = bare_tool_name(tool);
    let actions = match bare {
        "ingest" => vec![
            "brain.bootstrap",
            "graph.ingest.change_roots",
            "graph.ingest.merge_existing",
            "graph.ingest.replace",
        ],
        "auto_ingest_start" => vec![
            "auto_ingest.change_roots",
            "auto_ingest.start_existing_roots",
        ],
        "federate_auto" => vec!["graph.federate_auto.preview", "graph.federate_auto.execute"],
        "mission_close" => vec!["mission.close", "mission.close_with_memory"],
        "persist" => vec![
            "store.persist.status",
            "store.persist.save_runtime",
            "store.persist.checkpoint",
            "store.persist.load_replace",
        ],
        "boot_memory" => vec!["boot_memory.read", "boot_memory.set", "boot_memory.delete"],
        "memorize" => vec!["memory.memorize.default"],
        "xray_retag" => vec!["xray.retag.dry_run", "xray.retag.commit"],
        "xray_apply" => vec!["xray.apply.dry_run", "xray.apply.commit"],
        "xray_paint" => vec!["xray.paint.dry_run", "xray.paint.commit"],
        "system_blocks_archive" => vec!["system_blocks.archive", "system_blocks.restore"],
        "candidate_lease" => vec![
            "system_blocks.lease.acquire",
            "system_blocks.lease.refresh",
            "system_blocks.lease.release",
        ],
        "mission_post" => vec![
            "mission.post.ordinary",
            "mission.post.landed",
            "mission.post.archive",
        ],
        "mission_service" => vec![
            "mission.service.land_intent",
            "mission.service.mission_transition",
            "mission.service.execution_dispatch",
            "mission.service.execution_started",
            "mission.service.execution_terminal",
            "mission.service.land",
        ],
        "external_mutation_service" => vec![
            "system_blocks.ratify",
            "brain.promote",
            "source.edit.commit",
            "graph.ingest.replace",
            "graph.ingest.merge_existing",
        ],
        "graph_ingest_preview" => vec!["graph.ingest.preview"],
        "authority_session_challenge" | "authority_session_authenticate" => {
            vec!["runtime.session.handshake"]
        }
        "authority_authorize" => vec!["authority.authorize"],
        _ => vec![fixed_mcp_action(bare)?],
    };
    Some(actions)
}

/// Conservative union of every effect an argument-sensitive MCP tool can
/// select. The proof middleware uses this only when an exact branch depends on
/// a trusted owner fact that has not been computed yet. A future action added to
/// a routed tool therefore expands the gate automatically instead of escaping a
/// name-based allow/deny list.
pub fn possible_mcp_effects(tool: &str) -> Result<BTreeSet<Effect>, McpActionRouteError> {
    let bare = bare_tool_name(tool);
    let actions = possible_mcp_actions(bare).ok_or_else(|| McpActionRouteError::UnknownTool {
        tool: bare.to_string(),
    })?;
    let catalog =
        m1nd10_action_catalog().map_err(|error| McpActionRouteError::Catalog(error.to_string()))?;
    let mut effects = BTreeSet::new();
    for action in actions {
        let entry = catalog
            .entries
            .iter()
            .find(|entry| entry.action.as_str() == action)
            .ok_or_else(|| McpActionRouteError::CatalogActionMissing {
                action: action.to_string(),
            })?;
        if !entry.ingresses.contains(&Ingress::Mcp) {
            return Err(McpActionRouteError::CatalogIngressMismatch {
                action: action.to_string(),
            });
        }
        effects.extend(entry.complete_effects.iter().copied());
    }
    Ok(effects)
}

/// Conservative union of every authority floor an argument-sensitive MCP tool
/// can select. Generic transports use this only when exact classification
/// depends on an owner-computed fact that is unavailable at the ingress seam.
/// In that case the caller is admitted only when every reachable branch is
/// [`AuthorityFloor::Ordinary`].
pub fn possible_mcp_authority_floors(
    tool: &str,
) -> Result<BTreeSet<AuthorityFloor>, McpActionRouteError> {
    let bare = bare_tool_name(tool);
    let actions = possible_mcp_actions(bare).ok_or_else(|| McpActionRouteError::UnknownTool {
        tool: bare.to_string(),
    })?;
    let catalog =
        m1nd10_action_catalog().map_err(|error| McpActionRouteError::Catalog(error.to_string()))?;
    let mut floors = BTreeSet::new();
    for action in actions {
        let entry = catalog
            .entries
            .iter()
            .find(|entry| entry.action.as_str() == action)
            .ok_or_else(|| McpActionRouteError::CatalogActionMissing {
                action: action.to_string(),
            })?;
        if !entry.ingresses.contains(&Ingress::Mcp) {
            return Err(McpActionRouteError::CatalogIngressMismatch {
                action: action.to_string(),
            });
        }
        floors.insert(entry.authority_floor);
    }
    Ok(floors)
}

/// Select one exact semantic action and resolve its canonical effects/authority
/// floor. Unknown selectors, missing trusted facts, catalog drift and ingress
/// mismatch are all refusals.
pub fn classify_mcp_action(
    tool: &str,
    arguments: &Value,
    trusted: TrustedMcpRouteFacts,
) -> Result<ClassifiedMcpActionV1, McpActionRouteError> {
    let bare = bare_tool_name(tool);
    let action = match bare {
        "ingest" => classify_ingest(arguments, trusted)?,
        "auto_ingest_start" => match trusted.auto_ingest_changes_roots {
            Some(true) => "auto_ingest.change_roots",
            Some(false) => "auto_ingest.start_existing_roots",
            None => {
                return Err(McpActionRouteError::MissingTrustedFact {
                    tool: bare.to_string(),
                    fact: "auto_ingest_changes_roots",
                })
            }
        },
        "federate_auto" => {
            if bool_field(arguments, "execute", false) {
                "graph.federate_auto.execute"
            } else {
                "graph.federate_auto.preview"
            }
        }
        "mission_close" => {
            if bool_field(arguments, "write_light_memory", false) {
                "mission.close_with_memory"
            } else {
                "mission.close"
            }
        }
        "persist" => classify_persist(arguments)?,
        "boot_memory" => match string_field(arguments, "action") {
            Some("set") => "boot_memory.set",
            Some("delete") => "boot_memory.delete",
            Some("get" | "list" | "status") => "boot_memory.read",
            other => return unsupported(bare, "action", other),
        },
        "memorize" => "memory.memorize.default",
        "xray_retag" => classify_dry_run_or_commit(bare, arguments, "xray.retag")?,
        "xray_apply" => classify_dry_run_or_commit(bare, arguments, "xray.apply")?,
        "xray_paint" => classify_dry_run_or_commit(bare, arguments, "xray.paint")?,
        "system_blocks_archive" => match string_field(arguments, "mode") {
            Some("archive") => "system_blocks.archive",
            Some("restore") => "system_blocks.restore",
            other => return unsupported(bare, "mode", other),
        },
        "candidate_lease" => match string_field(arguments, "action") {
            Some("acquire") => "system_blocks.lease.acquire",
            Some("refresh") => "system_blocks.lease.refresh",
            Some("release") => "system_blocks.lease.release",
            other => return unsupported(bare, "action", other),
        },
        "mission_post" => match arguments
            .get("letter")
            .and_then(|letter| letter.get("phase"))
            .and_then(Value::as_str)
        {
            Some("landed") => "mission.post.landed",
            Some("archived") => "mission.post.archive",
            Some(_) => "mission.post.ordinary",
            None => return unsupported(bare, "letter.phase", None),
        },
        "mission_service" => match string_field(arguments, "action") {
            Some("land_intent") => "mission.service.land_intent",
            Some("mission_transition") => "mission.service.mission_transition",
            Some("execution_dispatch") => "mission.service.execution_dispatch",
            Some("execution_started") => "mission.service.execution_started",
            Some("execution_terminal") => "mission.service.execution_terminal",
            Some("land") => "mission.service.land",
            other => return unsupported(bare, "action", other),
        },
        "external_mutation_service" => match string_field(arguments, "action") {
            Some("system_blocks_ratify") => "system_blocks.ratify",
            Some("brain_promote") => "brain.promote",
            Some("source_edit_commit") => "source.edit.commit",
            Some("graph_ingest_replace") => "graph.ingest.replace",
            Some("graph_ingest_merge_existing") => "graph.ingest.merge_existing",
            other => return unsupported(bare, "action", other),
        },
        _ => fixed_mcp_action(bare).ok_or_else(|| McpActionRouteError::UnknownTool {
            tool: bare.to_string(),
        })?,
    };

    resolve_catalog_action(bare, action)
}

fn classify_ingest(
    arguments: &Value,
    trusted: TrustedMcpRouteFacts,
) -> Result<&'static str, McpActionRouteError> {
    if nonempty_string(arguments, "project_root") {
        return Ok("brain.bootstrap");
    }
    match string_field(arguments, "mode").unwrap_or("replace") {
        "replace" => Ok("graph.ingest.replace"),
        "merge" => match trusted.ingest_changes_roots {
            Some(true) => Ok("graph.ingest.change_roots"),
            Some(false) => Ok("graph.ingest.merge_existing"),
            None => Err(McpActionRouteError::MissingTrustedFact {
                tool: "ingest".to_string(),
                fact: "ingest_changes_roots",
            }),
        },
        other => Err(McpActionRouteError::UnsupportedSelector {
            tool: "ingest".to_string(),
            field: "mode",
            observed: other.to_string(),
        }),
    }
}

fn classify_persist(arguments: &Value) -> Result<&'static str, McpActionRouteError> {
    match string_field(arguments, "action") {
        Some("status") => Ok("store.persist.status"),
        Some("save") => Ok("store.persist.save_runtime"),
        Some("checkpoint") => Ok("store.persist.checkpoint"),
        Some("load") => Ok("store.persist.load_replace"),
        other => unsupported("persist", "action", other),
    }
}

fn classify_dry_run_or_commit(
    tool: &str,
    arguments: &Value,
    semantic_prefix: &'static str,
) -> Result<&'static str, McpActionRouteError> {
    match string_field(arguments, "mode").unwrap_or("dry_run") {
        "dry_run" => Ok(match semantic_prefix {
            "xray.retag" => "xray.retag.dry_run",
            "xray.apply" => "xray.apply.dry_run",
            "xray.paint" => "xray.paint.dry_run",
            _ => unreachable!("closed semantic prefix"),
        }),
        "commit" => Ok(match semantic_prefix {
            "xray.retag" => "xray.retag.commit",
            "xray.apply" => "xray.apply.commit",
            "xray.paint" => "xray.paint.commit",
            _ => unreachable!("closed semantic prefix"),
        }),
        other => Err(McpActionRouteError::UnsupportedSelector {
            tool: tool.to_string(),
            field: "mode",
            observed: other.to_string(),
        }),
    }
}

fn resolve_catalog_action(
    tool_name: &str,
    action: &str,
) -> Result<ClassifiedMcpActionV1, McpActionRouteError> {
    let catalog =
        m1nd10_action_catalog().map_err(|error| McpActionRouteError::Catalog(error.to_string()))?;
    let entry: &ActionCatalogEntryV1 = catalog
        .entries
        .iter()
        .find(|entry| entry.action.as_str() == action)
        .ok_or_else(|| McpActionRouteError::CatalogActionMissing {
            action: action.to_string(),
        })?;
    if !entry.ingresses.contains(&Ingress::Mcp) {
        return Err(McpActionRouteError::CatalogIngressMismatch {
            action: action.to_string(),
        });
    }
    Ok(ClassifiedMcpActionV1 {
        tool_name: tool_name.to_string(),
        action: entry.action.clone(),
        effects: entry.complete_effects.clone(),
        risk_class: entry.risk_class,
        authority_floor: entry.authority_floor,
    })
}

fn fixed_mcp_action(tool: &str) -> Option<&'static str> {
    let action = match tool {
        "orient" => "query.orient",
        "north" => "query.north",
        "delegate" => "delegation.delegate",
        "debrief" => "delegation.debrief",
        "evidence_query" => "evidence.query",
        "activate" => "query.activate",
        "missing" => "query.missing",
        "learn" => "graph.learn",
        "document_resolve" => "documents.resolve.refresh_cache",
        "document_bindings" => "documents.bindings.refresh_cache",
        "document_drift" => "documents.drift.refresh_cache",
        "auto_ingest_stop" => "auto_ingest.stop",
        "auto_ingest_tick" => "auto_ingest.tick",
        "session_handshake" => "runtime.session.handshake",
        "perspective_start" => "perspective.start",
        "perspective_routes" => "perspective.routes",
        "perspective_inspect" => "perspective.inspect",
        "perspective_peek" => "perspective.peek",
        "perspective_follow" => "perspective.follow",
        "perspective_suggest" => "perspective.suggest",
        "perspective_affinity" => "perspective.affinity",
        "perspective_branch" => "perspective.branch",
        "perspective_back" => "perspective.back",
        "perspective_close" => "perspective.close",
        "lock_create" => "lock.create",
        "lock_watch" => "lock.watch",
        "lock_diff" => "lock.diff",
        "lock_rebase" => "lock.rebase",
        "lock_release" => "lock.release",
        "seek" => "query.seek",
        "scan" => "query.scan",
        "trail_save" => "trail.save",
        "trail_resume" => "trail.resume",
        "trail_merge" => "trail.merge",
        "federate" => "graph.federate.replace",
        "antibody_create" => "antibody.create",
        "ghost_edges" => "graph.ghost_edges",
        "calibrate_predict" => "calibration.predict",
        "calibrate_envelope" => "calibration.envelope",
        "taint_trace" => "query.taint_trace",
        "twins" => "query.twins",
        "refactor_plan" => "query.refactor_plan",
        "runtime_overlay" => "graph.runtime_overlay",
        "surgical_context" => "query.read",
        "apply" => "source.apply.single",
        "surgical_context_v2" => "source.surgical_context.mark_proof_ready",
        "apply_batch" => "source.apply.batch",
        "edit_preview" => "source.edit.preview",
        "edit_commit" => "source.edit.commit",
        "scan_all" => "query.scan_all",
        "mission_start" => "mission.start",
        "mission_next" => "mission.next",
        "mission_event" => "mission.event",
        "mission_verify" => "mission.verify",
        "mission_handoff" => "mission.handoff",
        "audit" => "graph.audit.replace",
        "daemon_start" => "daemon.start",
        "daemon_stop" => "daemon.stop",
        "daemon_tick" => "daemon.tick",
        "alerts_ack" => "daemon.alerts_ack",
        "promote" => "brain.promote",
        "skeleton_candidate" => "system_blocks.skeleton_candidate",
        "system_blocks_seed_import" => "system_blocks.seed_import.force",
        "system_blocks_ratify" => "system_blocks.ratify",
        "receipt_import" => "system_blocks.receipt_import",
        "system_blocks_reconcile" => "system_blocks.reconcile",
        "system_blocks_delete" => "system_blocks.delete.permanent",
        "candidate_edit" => "system_blocks.candidate_edit",
        "candidate_naming" => "system_blocks.candidate_naming",
        "mission_spawn" => "mission.spawn",
        "authority_session_challenge" | "authority_session_authenticate" => {
            "runtime.session.handshake"
        }
        "authority_authorize" => "authority.authorize",
        "graph_ingest_preview" => "graph.ingest.preview",
        tool if PURE_READ_TOOLS.contains(&tool) => "query.read",
        _ => return None,
    };
    Some(action)
}

const PURE_READ_TOOLS: &[&str] = &[
    "cockpit",
    "am_i_stale",
    "impact",
    "why",
    "warmup",
    "counterfactual",
    "predict",
    "fingerprint",
    "drift",
    "document_provider_health",
    "auto_ingest_status",
    "health",
    "trust_selftest",
    "recovery_playbook",
    "doctor",
    "perspective_compare",
    "perspective_list",
    "focus",
    "timeline",
    "diverge",
    "trail_list",
    "hypothesize",
    "differential",
    "trace",
    "validate_plan",
    "antibody_scan",
    "antibody_list",
    "flow_simulate",
    "epidemic",
    "tremor",
    "trust",
    "layers",
    "layer_inspect",
    "heuristics_surface",
    "view",
    "batch_view",
    "search",
    "glob",
    "cross_verify",
    "soul_check",
    "soul_read",
    "coverage_session",
    "external_references",
    "help",
    "report",
    "daemon_status",
    "alerts_list",
    "panoramic",
    "metrics",
    "type_trace",
    "diagram",
    "xray_orient",
    "xray_gate",
    "xray_ledger",
    "system_blocks_snapshot",
    "receipt_recompute",
];

fn bare_tool_name(tool: &str) -> &str {
    tool.strip_prefix("m1nd.")
        .or_else(|| tool.strip_prefix("m1nd_"))
        .unwrap_or(tool)
}

fn string_field<'a>(arguments: &'a Value, field: &str) -> Option<&'a str> {
    arguments.get(field).and_then(Value::as_str)
}

fn nonempty_string(arguments: &Value, field: &str) -> bool {
    string_field(arguments, field).is_some_and(|value| !value.trim().is_empty())
}

fn bool_field(arguments: &Value, field: &str, default: bool) -> bool {
    arguments
        .get(field)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn unsupported<T>(
    tool: &str,
    field: &'static str,
    observed: Option<&str>,
) -> Result<T, McpActionRouteError> {
    Err(McpActionRouteError::UnsupportedSelector {
        tool: tool.to_string(),
        field,
        observed: observed.unwrap_or("<missing-or-non-string>").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;

    #[test]
    fn route_name_inventory_has_no_duplicates_and_every_name_classifies() {
        let unique: BTreeSet<&str> = MCP_TOOL_ROUTE_NAMES.iter().copied().collect();
        assert_eq!(unique.len(), MCP_TOOL_ROUTE_NAMES.len());
        for tool in MCP_TOOL_ROUTE_NAMES {
            assert!(possible_mcp_actions(tool).is_some(), "unrouted {tool}");
        }
    }

    #[test]
    fn live_schema_registry_and_policy_route_inventory_are_exactly_equal() {
        let schemas = crate::server::all_tool_schemas();
        let actual: BTreeSet<&str> = schemas["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect();
        let routed: BTreeSet<&str> = MCP_TOOL_ROUTE_NAMES.iter().copied().collect();
        assert_eq!(actual, routed, "tool/policy parity drift");
    }

    #[test]
    fn every_possible_route_exists_in_catalog_and_is_mcp_reachable() {
        let catalog = m1nd10_action_catalog().expect("catalog");
        for tool in MCP_TOOL_ROUTE_NAMES {
            for action in possible_mcp_actions(tool).expect("route") {
                let entry = catalog
                    .entries
                    .iter()
                    .find(|entry| entry.action.as_str() == action)
                    .unwrap_or_else(|| panic!("{tool} routes to absent action {action}"));
                assert!(
                    entry.ingresses.contains(&Ingress::Mcp),
                    "{tool} routes to non-MCP action {action}"
                );
                assert!(!entry.complete_effects.is_empty());
            }
        }
    }

    #[test]
    fn callable_lock_tools_have_exact_catalog_routes() {
        let expected = [
            ("lock_create", "lock.create"),
            ("lock_watch", "lock.watch"),
            ("lock_diff", "lock.diff"),
            ("lock_rebase", "lock.rebase"),
            ("lock_release", "lock.release"),
        ];
        for (tool, action) in expected {
            let classified = classify_mcp_action(tool, &json!({}), TrustedMcpRouteFacts::default())
                .unwrap_or_else(|error| panic!("{tool} must classify: {error}"));
            assert_eq!(classified.action.as_str(), action);
        }
    }

    #[test]
    fn sensitive_variants_never_fall_back_to_weaker_actions() {
        let missing = classify_mcp_action(
            "ingest",
            &json!({"mode": "merge"}),
            TrustedMcpRouteFacts::default(),
        );
        assert!(matches!(
            missing,
            Err(McpActionRouteError::MissingTrustedFact { .. })
        ));

        let replace = classify_mcp_action(
            "ingest",
            &json!({"mode": "replace"}),
            TrustedMcpRouteFacts::default(),
        )
        .expect("replace");
        assert_eq!(replace.action.as_str(), "graph.ingest.replace");
        assert_eq!(replace.authority_floor, AuthorityFloor::PositiveSovereign);

        let managed = classify_mcp_action(
            "persist",
            &json!({"action": "save"}),
            TrustedMcpRouteFacts::default(),
        )
        .expect("managed save");
        assert_eq!(managed.action.as_str(), "store.persist.save_runtime");

        let committed = classify_mcp_action(
            "xray_apply",
            &json!({"mode": "commit"}),
            TrustedMcpRouteFacts::default(),
        )
        .expect("commit");
        assert!(committed.effects.contains(&Effect::SourceFilesystemWrite));
        let dry_run = classify_mcp_action(
            "xray_apply",
            &json!({"mode": "dry_run"}),
            TrustedMcpRouteFacts::default(),
        )
        .expect("dry run");
        assert_eq!(dry_run.effects, BTreeSet::from([Effect::Read]));
        let union = possible_mcp_effects("xray_apply").expect("effect union");
        assert!(union.contains(&Effect::Read));
        assert!(union.contains(&Effect::SourceFilesystemWrite));

        assert!(classify_mcp_action(
            "persist",
            &json!({"action": "mystery"}),
            TrustedMcpRouteFacts::default(),
        )
        .is_err());
    }

    #[test]
    fn external_a2_selectors_route_to_exact_non_generic_authority_floors() {
        let replace = classify_mcp_action(
            "external_mutation_service",
            &json!({"action": "graph_ingest_replace"}),
            TrustedMcpRouteFacts::default(),
        )
        .expect("A2 replace route");
        assert_eq!(replace.action.as_str(), "graph.ingest.replace");
        assert_eq!(replace.authority_floor, AuthorityFloor::PositiveSovereign);
        assert_eq!(replace.risk_class, RiskClass::Critical);

        let merge = classify_mcp_action(
            "external_mutation_service",
            &json!({"action": "graph_ingest_merge_existing"}),
            TrustedMcpRouteFacts::default(),
        )
        .expect("A2 merge route");
        assert_eq!(merge.action.as_str(), "graph.ingest.merge_existing");
        assert_eq!(merge.authority_floor, AuthorityFloor::ScopedGrantA2);
        assert_eq!(merge.risk_class, RiskClass::High);

        assert!(matches!(
            classify_mcp_action(
                "external_mutation_service",
                &json!({"action": "graph_ingest"}),
                TrustedMcpRouteFacts::default(),
            ),
            Err(McpActionRouteError::UnsupportedSelector { .. })
        ));
    }

    #[test]
    fn tool_prefixes_share_the_same_fail_closed_route() {
        for tool in ["north", "m1nd.north", "m1nd_north"] {
            let classified = classify_mcp_action(tool, &json!({}), TrustedMcpRouteFacts::default())
                .expect("north route");
            assert_eq!(classified.action.as_str(), "query.north");
        }
    }
}
