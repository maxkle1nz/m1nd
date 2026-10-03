use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use m1nd_control::{m1nd10_action_catalog, Ingress};
use m1nd_mcp::action_consumers::{m1nd10_action_consumer_registry, ActionConsumerDispositionV1};
use m1nd_mcp::action_routes::{possible_mcp_actions, MCP_TOOL_ROUTE_NAMES};
use serde::Serialize;
use serde_json::Value;

const INVENTORY_SCHEMA: &str = "m1nd-agent-capability-inventory-v0";
const EVIDENCE_KIND: &str = "static_registry_projection";

#[derive(Debug, Serialize)]
struct Inventory {
    schema: &'static str,
    evidence_kind: &'static str,
    transport_proven: bool,
    transport_non_claim: &'static str,
    summary: Summary,
    actions: Vec<ActionRow>,
    wave_1a_analysis: Wave1aAnalysis,
}

#[derive(Debug, Serialize)]
struct Summary {
    action_count: usize,
    action_denominator_count: usize,
    registry_tool_count: usize,
    core_menu_tool_count: usize,
    full_menu_tool_count: usize,
    routed_tool_count: usize,
    registered_variant_action_count: usize,
    mcp_declared_action_count: usize,
    mcp_policy_enabled_count: usize,
    mcp_policy_disabled_count: usize,
    mcp_unregistered_disposition_count: usize,
    rest_declared_action_count: usize,
    rest_policy_enabled_count: usize,
    rest_policy_disabled_count: usize,
}

#[derive(Debug, Serialize)]
struct ActionRow {
    action_id: String,
    registered_tool_variants: Vec<ToolVariant>,
    unregistered_mcp_disposition: Option<UnregisteredMcpDisposition>,
    catalog_ingresses: Value,
    authority_floor: Value,
    risk_class: Value,
    effects: Value,
    mcp: Surface,
    rest: Surface,
}

#[derive(Debug, Serialize)]
struct ToolVariant {
    tool: String,
    variant_action: String,
    argument_sensitive: bool,
    registered: bool,
    core_menu: bool,
    full_menu: bool,
}

#[derive(Debug, Serialize)]
struct Surface {
    catalog_declared: bool,
    registered_schema_present: Option<bool>,
    actual_route_present: Option<bool>,
    actual_route_provenance: &'static str,
    current_disposition: Value,
    policy_enabled: bool,
    generic_policy_enabled: bool,
    typed_consumer_declared: bool,
    typed_consumer_required: bool,
    refusal_or_repair: &'static str,
    evidence_kind: &'static str,
    transport_proven: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct UnregisteredMcpDisposition {
    kind: &'static str,
    justification: &'static str,
    sources: &'static [&'static str],
}

#[derive(Debug, Serialize)]
struct Wave1aAnalysis {
    minimal_cut_actions: Vec<&'static str>,
    current_initial_block_actions: Vec<&'static str>,
    denial_contracts_to_preserve: Vec<&'static str>,
    non_claim: &'static str,
}

fn tool_names(schemas: &Value) -> BTreeSet<String> {
    schemas["tools"]
        .as_array()
        .expect("tool registry must contain a tools array")
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .expect("every registered tool must have a name")
                .to_string()
        })
        .collect()
}

fn disposition_surface(
    disposition: &ActionConsumerDispositionV1,
    catalog_declared: bool,
    registered_schema_present: Option<bool>,
    actual_route_provenance: &'static str,
) -> Surface {
    let (
        policy_enabled,
        generic_policy_enabled,
        typed_consumer_declared,
        typed_consumer_required,
        refusal_or_repair,
    ) = match disposition {
        ActionConsumerDispositionV1::EnabledGenericOrdinary => {
            (true, true, false, false, "none_required")
        }
        ActionConsumerDispositionV1::EnabledGenericScopedA2Local => {
            (true, true, false, false, "action_local_checks_still_apply")
        }
        ActionConsumerDispositionV1::EnabledTypedConsumer { .. } => {
            (true, false, true, true, "typed_consumer_required")
        }
        ActionConsumerDispositionV1::PolicyDisabled { reason } => match reason {
            m1nd_mcp::action_consumers::ConsumerPolicyDisabledReasonV1::NotDeclared => {
                (false, false, false, false, "ingress_not_declared")
            }
            m1nd_mcp::action_consumers::ConsumerPolicyDisabledReasonV1::NoExactConsumer => {
                (false, false, false, false, "no_exact_consumer_installed")
            }
        },
    };
    Surface {
        catalog_declared,
        registered_schema_present,
        actual_route_present: None,
        actual_route_provenance,
        current_disposition: serde_json::to_value(disposition).expect("serialize disposition"),
        policy_enabled,
        generic_policy_enabled,
        typed_consumer_declared,
        typed_consumer_required,
        refusal_or_repair,
        evidence_kind: EVIDENCE_KIND,
        transport_proven: false,
    }
}

fn unregistered_mcp_disposition(action: &str) -> Option<UnregisteredMcpDisposition> {
    let disposition = match action {
        "lock.create" | "lock.watch" | "lock.diff" | "lock.rebase" | "lock.release" => {
            UnregisteredMcpDisposition {
                kind: "hidden_classified_route",
                justification: "The classifier accepts the corresponding lock_* name, but that name is intentionally outside the registered schema inventory.",
                sources: &["m1nd-mcp/src/action_routes.rs::fixed_mcp_action", "m1nd-mcp/src/action_routes.rs::callable_lock_tools_have_exact_catalog_routes"],
            }
        }
        "mission.post.ordinary" | "mission.post.landed" | "mission.post.archive"
        | "system_blocks.receipt_import" => UnregisteredMcpDisposition {
            kind: "desurfaced_legacy_tombstone",
            justification: "The raw legacy tool was removed from the registered surface and is tombstoned; typed mission/evidence consumers are the replacement boundary.",
            sources: &["m1nd-mcp/src/server.rs::all_tool_schemas", "m1nd-mcp/src/mission_service_wire_tests.rs"],
        },
        "runtime.presence.track_agent" => UnregisteredMcpDisposition {
            kind: "internal_transport_hook",
            justification: "Presence tracking rides authenticated tool traffic internally and is not a standalone registered tool schema.",
            sources: &["m1nd-mcp/src/session.rs::track_agent", "m1nd-mcp/src/mcp_http.rs"],
        },
        "memory.memorize.explicit_path" | "store.persist.save_explicit_path" => {
            UnregisteredMcpDisposition {
                kind: "withdrawn_sensitive_selector",
                justification: "The registered tool omits the sensitive explicit-path selector; schema guards reject attempts to supply it.",
                sources: &["m1nd-mcp/src/server.rs::all_tool_schemas", "m1nd-mcp/src/server.rs::public_memorize_and_persist_schemas_expose_no_filesystem_override"],
            }
        }
        "antibody.delete" | "antibody.disable" | "antibody.enable" => {
            UnregisteredMcpDisposition {
                kind: "catalog_projection_gap",
                justification: "The catalog declares MCP ingress, but the registered schema inventory enumerates no matching variant; dispatch reachability remains unclaimed.",
                sources: &["m1nd-control/src/action_catalog.rs", "m1nd-mcp/src/server.rs::all_tool_schemas"],
            }
        }
        _ => return None,
    };
    Some(disposition)
}

fn build_inventory() -> Inventory {
    let catalog = m1nd10_action_catalog().expect("canonical action catalog");
    let consumers = m1nd10_action_consumer_registry().expect("canonical consumer registry");
    let registry_tools = tool_names(&m1nd_mcp::server::all_tool_schemas());
    let core_tools = tool_names(&m1nd_mcp::server::tool_schemas_for_tier("core"));
    let full_tools = tool_names(&m1nd_mcp::server::tool_schemas_for_tier("full"));

    let mut action_tools: BTreeMap<String, Vec<ToolVariant>> = BTreeMap::new();
    for tool in MCP_TOOL_ROUTE_NAMES {
        let actions = possible_mcp_actions(tool)
            .unwrap_or_else(|| panic!("routed tool {tool} must enumerate semantic actions"));
        let argument_sensitive = actions.len() > 1;
        for action in actions {
            action_tools
                .entry(action.to_string())
                .or_default()
                .push(ToolVariant {
                    tool: (*tool).to_string(),
                    variant_action: action.to_string(),
                    argument_sensitive,
                    registered: registry_tools.contains(*tool),
                    core_menu: core_tools.contains(*tool),
                    full_menu: full_tools.contains(*tool),
                });
        }
    }
    for variants in action_tools.values_mut() {
        variants.sort_by(|left, right| left.tool.cmp(&right.tool));
        variants.dedup_by(|left, right| left.tool == right.tool);
    }

    let mut actions = Vec::with_capacity(catalog.entries.len());
    for entry in &catalog.entries {
        let registered_tool_variants = action_tools
            .remove(entry.action.as_str())
            .unwrap_or_default();
        let unregistered_mcp_disposition =
            if entry.ingresses.contains(&Ingress::Mcp) && registered_tool_variants.is_empty() {
                unregistered_mcp_disposition(entry.action.as_str())
            } else {
                None
            };
        let mcp_cell = consumers
            .cell(entry.action.as_str(), Ingress::Mcp)
            .expect("consumer matrix must contain every MCP cell");
        let rest_cell = consumers
            .cell(entry.action.as_str(), Ingress::Rest)
            .expect("consumer matrix must contain every REST cell");
        let mcp_declared = entry.ingresses.contains(&Ingress::Mcp);
        let rest_declared = entry.ingresses.contains(&Ingress::Rest);
        actions.push(ActionRow {
            action_id: entry.action.to_string(),
            mcp: disposition_surface(
                &mcp_cell.disposition,
                mcp_declared,
                Some(!registered_tool_variants.is_empty()),
                "not_audited_registered_schema_is_not_route_proof",
            ),
            rest: disposition_surface(
                &rest_cell.disposition,
                rest_declared,
                None,
                "not_audited_catalog_declaration_is_not_route_proof",
            ),
            registered_tool_variants,
            unregistered_mcp_disposition,
            catalog_ingresses: serde_json::to_value(&entry.ingresses).expect("serialize ingresses"),
            authority_floor: serde_json::to_value(entry.authority_floor).expect("serialize floor"),
            risk_class: serde_json::to_value(entry.risk_class).expect("serialize risk"),
            effects: serde_json::to_value(&entry.complete_effects).expect("serialize effects"),
        });
    }
    assert!(
        action_tools.is_empty(),
        "every routed semantic action must be present in the canonical catalog: {action_tools:?}"
    );

    let summary = Summary {
        action_count: actions.len(),
        action_denominator_count: catalog.entries.len(),
        registry_tool_count: registry_tools.len(),
        core_menu_tool_count: core_tools.len(),
        full_menu_tool_count: full_tools.len(),
        routed_tool_count: MCP_TOOL_ROUTE_NAMES.len(),
        registered_variant_action_count: actions
            .iter()
            .filter(|row| !row.registered_tool_variants.is_empty())
            .count(),
        mcp_declared_action_count: actions
            .iter()
            .filter(|row| row.mcp.catalog_declared)
            .count(),
        mcp_policy_enabled_count: actions
            .iter()
            .filter(|row| row.mcp.catalog_declared && row.mcp.policy_enabled)
            .count(),
        mcp_policy_disabled_count: actions
            .iter()
            .filter(|row| row.mcp.catalog_declared && !row.mcp.policy_enabled)
            .count(),
        mcp_unregistered_disposition_count: actions
            .iter()
            .filter(|row| row.unregistered_mcp_disposition.is_some())
            .count(),
        rest_declared_action_count: actions
            .iter()
            .filter(|row| row.rest.catalog_declared)
            .count(),
        rest_policy_enabled_count: actions
            .iter()
            .filter(|row| row.rest.catalog_declared && row.rest.policy_enabled)
            .count(),
        rest_policy_disabled_count: actions
            .iter()
            .filter(|row| row.rest.catalog_declared && !row.rest.policy_enabled)
            .count(),
    };

    Inventory {
        schema: INVENTORY_SCHEMA,
        evidence_kind: EVIDENCE_KIND,
        transport_proven: false,
        transport_non_claim: "This inventory proves static catalog/registry/classifier coverage. It does not claim a successful live MCP or REST call.",
        summary,
        actions,
        wave_1a_analysis: Wave1aAnalysis {
            minimal_cut_actions: vec![
                "brain.bootstrap",
                "brain.bootstrap.birth",
                "graph.ingest.replace",
                "query.north",
            ],
            current_initial_block_actions: vec![],
            denial_contracts_to_preserve: vec![
                "launcher-granted workspace root is the maximum local scope",
                "never write to a mismatched brain",
                "parent, child, symlink, and worktree identity conflicts fail closed",
                "bootstrap is idempotent and single-flight per exact workspace identity",
                "automatic local bootstrap never fabricates HumanOrigin",
                "external publication, privilege expansion, and cross-project sharing remain denied",
                "read-only source workspaces use isolated private cache without source mutation",
            ],
            non_claim: "Launcher-granted local bootstrap is implemented and transport-tested; this static inventory does not claim general graph-ingest authority, REST route proof, release, installation, or activation.",
        },
    }
}

fn render_markdown(inventory: &Inventory) -> String {
    let summary = &inventory.summary;
    let mut output = format!(
        "# Agent capability inventory\n\n\
         Evidence: `{}`. Live transport proven: **no**.\n\n\
         This report is generated from the canonical action catalog, consumer matrix, MCP route classifiers, and tool registry. It does not claim a successful live MCP or REST call.\n\n\
         ## Totals\n\n\
         - Actions: {} of {} canonical catalog entries projected.\n\
         - MCP: {} catalog-declared actions; {} policy-enabled dispositions; {} policy-disabled dispositions; {} explicit dispositions without a registered schema variant.\n\
         - REST: {} catalog-declared actions; {} policy-enabled dispositions; {} policy-disabled dispositions; actual routes were not audited.\n\
         - Tools: {} registered schemas; {} in the core menu; {} in the full menu; {} registered classifier names; {} actions represented by at least one registered schema variant.\n\n\
         ## Action and variant table\n\n\
         | Action | Registered tool variants | Floor | Effects | MCP | REST |\n\
         | --- | --- | --- | --- | --- | --- |\n",
        inventory.evidence_kind,
        summary.action_count,
        summary.action_denominator_count,
        summary.mcp_declared_action_count,
        summary.mcp_policy_enabled_count,
        summary.mcp_policy_disabled_count,
        summary.mcp_unregistered_disposition_count,
        summary.rest_declared_action_count,
        summary.rest_policy_enabled_count,
        summary.rest_policy_disabled_count,
        summary.registry_tool_count,
        summary.core_menu_tool_count,
        summary.full_menu_tool_count,
        summary.routed_tool_count,
        summary.registered_variant_action_count,
    );
    for row in &inventory.actions {
        let tools = if row.registered_tool_variants.is_empty() {
            row.unregistered_mcp_disposition
                .map(|disposition| format!("— ({})", disposition.kind))
                .unwrap_or_else(|| "—".to_string())
        } else {
            row.registered_tool_variants
                .iter()
                .map(|variant| {
                    format!(
                        "`{}`{}{}",
                        variant.tool,
                        if variant.argument_sensitive {
                            " (variant)"
                        } else {
                            ""
                        },
                        if variant.core_menu { " [core]" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let effects = row
            .effects
            .as_array()
            .expect("effect array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        let mcp = format!(
            "{} / policy {} / {}",
            if row.mcp.catalog_declared {
                "declared"
            } else {
                "not declared"
            },
            if row.mcp.policy_enabled {
                "enabled"
            } else {
                "disabled"
            },
            row.mcp.refusal_or_repair
        );
        let rest = format!(
            "{} / policy {} / route unknown / {}",
            if row.rest.catalog_declared {
                "declared"
            } else {
                "not declared"
            },
            if row.rest.policy_enabled {
                "enabled"
            } else {
                "disabled"
            },
            row.rest.refusal_or_repair
        );
        output.push_str(&format!(
            "| `{}` | {} | `{}` | {} | {} | {} |\n",
            row.action_id,
            tools,
            row.authority_floor.as_str().expect("floor string"),
            effects,
            mcp,
            rest,
        ));
    }
    output.push_str("\n## Implemented launcher boundary\n\n");
    output.push_str("The launcher-granted local bootstrap is implemented for an explicit canonical workspace and an isolated private runtime. The cold-entry transport tests cover preparation before public retrieval without fabricating `HumanOrigin`; generic graph-ingest authority remains unchanged.\n\n");
    output.push_str("Denial contracts that remain load-bearing:\n\n");
    for contract in &inventory.wave_1a_analysis.denial_contracts_to_preserve {
        output.push_str(&format!("- {contract}.\n"));
    }
    output.push_str("\n## Limits\n\n");
    output.push_str("This inventory is static evidence except for the separately maintained launcher-bootstrap transport regressions. Registered MCP schema variants are enumerated; hidden classifier names, withdrawn selectors, internal hooks, tombstones, and catalog projection gaps are explicitly dispositioned. REST route presence is unknown because the REST dispatcher was not audited. Policy-enabled and typed-consumer-declared states are not execution proof. Capabilities outside the launcher-bootstrap boundary still need authenticated transport tests with real arguments.\n");
    output
}

fn write_reports(output_dir: &Path, inventory: &Inventory) {
    fs::create_dir_all(output_dir).expect("create requested inventory output directory");
    let json = serde_json::to_string_pretty(inventory).expect("serialize inventory");
    fs::write(output_dir.join("00-inventory.json"), format!("{json}\n"))
        .expect("write JSON inventory");
    fs::write(
        output_dir.join("00-inventory.md"),
        render_markdown(inventory),
    )
    .expect("write Markdown inventory");
}

#[test]
fn generated_inventory_is_complete_unique_and_variant_explicit() {
    let inventory = build_inventory();
    let catalog = m1nd10_action_catalog().expect("catalog");
    assert_eq!(inventory.actions.len(), catalog.entries.len());

    let unique_actions: BTreeSet<&str> = inventory
        .actions
        .iter()
        .map(|row| row.action_id.as_str())
        .collect();
    assert_eq!(unique_actions.len(), inventory.actions.len());

    let find = |action: &str| {
        inventory
            .actions
            .iter()
            .find(|row| row.action_id == action)
            .unwrap_or_else(|| panic!("missing action variant {action}"))
    };
    for action in [
        "xray.apply.dry_run",
        "xray.apply.commit",
        "mission.close",
        "mission.close_with_memory",
    ] {
        assert!(
            find(action)
                .registered_tool_variants
                .iter()
                .any(|variant| variant.argument_sensitive),
            "{action} must remain an explicit argument-sensitive variant"
        );
    }
    assert_ne!(
        find("xray.apply.dry_run").authority_floor,
        find("xray.apply.commit").authority_floor
    );
    assert_ne!(
        find("mission.close").authority_floor,
        find("mission.close_with_memory").authority_floor
    );

    if let Some(output_dir) = std::env::var_os("M1ND_CAPABILITY_INVENTORY_DIR") {
        write_reports(Path::new(&output_dir), &inventory);
    }
}

#[test]
fn generated_inventory_describes_launcher_bootstrap_as_implemented() {
    let inventory = build_inventory();
    let markdown = render_markdown(&inventory);
    for stale_claim in [
        "next slice must replace the initial local-workspace block",
        "no policy change is implemented by this inventory",
        "Wave 0 classification and policy changes remain unfinished",
    ] {
        assert!(
            !markdown.contains(stale_claim),
            "stale capability claim remained: {stale_claim}"
        );
    }
    assert!(
        markdown.contains("launcher-granted local bootstrap is implemented"),
        "inventory must describe the implemented launcher boundary"
    );
}

#[test]
fn every_mcp_catalog_action_has_registered_variant_or_justified_disposition() {
    let inventory = build_inventory();
    let catalog = m1nd10_action_catalog().expect("catalog");
    for entry in catalog
        .entries
        .iter()
        .filter(|entry| entry.ingresses.contains(&Ingress::Mcp))
    {
        let row = inventory
            .actions
            .iter()
            .find(|row| row.action_id == entry.action.as_str())
            .unwrap_or_else(|| panic!("missing catalog action {}", entry.action));
        match (
            row.registered_tool_variants.is_empty(),
            row.unregistered_mcp_disposition,
        ) {
            (false, None) => {
                for variant in &row.registered_tool_variants {
                    assert!(
                        possible_mcp_actions(&variant.tool)
                            .expect("registered tool must enumerate actions")
                            .contains(&row.action_id.as_str()),
                        "{} must remain selected by registered tool {}",
                        row.action_id,
                        variant.tool
                    );
                }
            }
            (true, Some(disposition)) => {
                assert!(!disposition.justification.is_empty());
                assert!(!disposition.sources.is_empty());
            }
            (true, None) => panic!(
                "MCP action {} needs a registered schema variant or an explicit justified disposition",
                row.action_id
            ),
            (false, Some(_)) => panic!(
                "MCP action {} has a registered variant and must not use an exception",
                row.action_id
            ),
        }
    }
}

#[test]
fn policy_projection_matches_consumer_matrix_for_mcp_and_rest() {
    let inventory = build_inventory();
    let consumers = m1nd10_action_consumer_registry().expect("consumer registry");
    for row in &inventory.actions {
        for (ingress, surface) in [(Ingress::Mcp, &row.mcp), (Ingress::Rest, &row.rest)] {
            let disposition = &consumers
                .cell(&row.action_id, ingress)
                .expect("consumer matrix cell")
                .disposition;
            let expected_policy_enabled = !matches!(
                disposition,
                ActionConsumerDispositionV1::PolicyDisabled { .. }
            );
            let expected_generic = matches!(
                disposition,
                ActionConsumerDispositionV1::EnabledGenericOrdinary
                    | ActionConsumerDispositionV1::EnabledGenericScopedA2Local
            );
            let expected_typed = matches!(
                disposition,
                ActionConsumerDispositionV1::EnabledTypedConsumer { .. }
            );
            assert_eq!(
                surface.policy_enabled, expected_policy_enabled,
                "{} {ingress:?}",
                row.action_id
            );
            assert_eq!(
                surface.generic_policy_enabled, expected_generic,
                "{} {ingress:?}",
                row.action_id
            );
            assert_eq!(
                surface.typed_consumer_declared, expected_typed,
                "{} {ingress:?}",
                row.action_id
            );
            assert_eq!(
                surface.typed_consumer_required, expected_typed,
                "{} {ingress:?}",
                row.action_id
            );
        }
    }
}

#[test]
fn inventory_does_not_claim_execution_or_unobserved_rest_routes() {
    let inventory = serde_json::to_value(build_inventory()).expect("serialize inventory");
    let summary = inventory["summary"].as_object().expect("summary object");
    assert!(
        summary.keys().all(|key| !key.contains("executable_today")),
        "policy dispositions are not execution proof"
    );
    for row in inventory["actions"].as_array().expect("actions array") {
        let rest = row["rest"].as_object().expect("REST surface object");
        assert!(
            rest.get("actual_route_present")
                .is_some_and(serde_json::Value::is_null),
            "catalog declaration must not masquerade as an observed REST route for {}",
            row["action_id"]
        );
        assert_eq!(
            rest["actual_route_provenance"],
            "not_audited_catalog_declaration_is_not_route_proof"
        );
    }
}
