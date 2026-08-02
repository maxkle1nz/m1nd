// === m1nd-mcp/src/report_handlers.rs ===
//
// v0.4.0: Handlers for m1nd.report and m1nd.panoramic.
//
// Brand gate G1.5 (founder decision 2026-07-03): the opt-in `savings` tool and
// every tokens-saved / CO2 field it emitted were removed as unmeasured claims.
// `report` keeps its honest content (query counts, elapsed, heuristic hotspots,
// graph size) — the savings/tokens sections were stripped.

use crate::protocol::layers::{
    PanoramicAlert, PanoramicInput, PanoramicModule, PanoramicOutput, ReportHeuristicHotspot,
    ReportInput, ReportOutput, ReportQueryEntry, ReportVerbUsage,
};
use crate::scope::normalize_scope_path;
use crate::session::SessionState;
use crate::surgical_handlers::build_surgical_heuristic_summary;
use m1nd_core::error::{M1ndError, M1ndResult};
use std::time::Instant;

// ---------------------------------------------------------------------------
// m1nd.report
// ---------------------------------------------------------------------------

pub fn handle_report(state: &mut SessionState, input: ReportInput) -> M1ndResult<ReportOutput> {
    let start = Instant::now();

    // Filter query log by agent_id (ADVERSARY R3: cross-agent privacy)
    let agent_queries: Vec<_> = state
        .query_log
        .iter()
        .filter(|q| q.agent_id == input.agent_id)
        .collect();

    let session_queries = agent_queries.len() as u32;
    let session_elapsed_ms: f64 = agent_queries.iter().map(|q| q.elapsed_ms).sum();
    let queries_answered = session_queries; // All m1nd queries are "answered"

    // Recent queries (last 10)
    let recent_queries: Vec<ReportQueryEntry> = agent_queries
        .iter()
        .rev()
        .take(10)
        .map(|q| ReportQueryEntry {
            tool: q.tool.clone(),
            query: q.query_preview.clone(),
            elapsed_ms: q.elapsed_ms,
            m1nd_answered: true,
        })
        .collect();

    // The DURABLE half of the report: what this brain has recorded about its
    // own use, across restarts. `recent_queries` above is this session's ring
    // buffer and dies with the process; these counters do not. They are NOT
    // filtered by `agent_id` — the ledger has no agent dimension, which is the
    // privacy decision, not an oversight (`crate::verb_usage`).
    let mut verb_usage: Vec<ReportVerbUsage> = state
        .verb_usage
        .entries()
        .map(|(verb, counters)| ReportVerbUsage {
            verb: verb.to_string(),
            answered: counters.answered,
            refused_at_authority_floor: counters.refused_at_authority_floor,
            refused_at_dispatch: counters.refused_at_dispatch,
            first_seen_ms: counters.first_seen_ms,
            last_seen_ms: counters.last_seen_ms,
        })
        .collect();
    verb_usage.sort_by(|a, b| {
        let a_total = a.answered + a.refused_at_authority_floor + a.refused_at_dispatch;
        let b_total = b.answered + b.refused_at_authority_floor + b.refused_at_dispatch;
        b_total.cmp(&a_total).then_with(|| a.verb.cmp(&b.verb))
    });

    let heuristic_hotspots: Vec<ReportHeuristicHotspot> = {
        let graph = state.graph.read();
        let mut candidates: Vec<(String, String)> = graph
            .id_to_node
            .keys()
            .map(|interned| graph.strings.resolve(*interned))
            .filter(|ext_id| ext_id.starts_with("file::"))
            .map(|ext_id| {
                let file_path = ext_id.trim_start_matches("file::").to_string();
                (ext_id.to_string(), file_path)
            })
            .collect();
        drop(graph);

        candidates.sort();
        candidates.dedup();

        let mut hotspots: Vec<ReportHeuristicHotspot> = candidates
            .into_iter()
            .map(|(node_id, file_path)| {
                let summary = build_surgical_heuristic_summary(state, &node_id, &file_path);
                ReportHeuristicHotspot {
                    node_id,
                    file_path,
                    risk_level: summary.risk_level,
                    risk_score: summary.risk_score,
                    heuristic_signals: summary.heuristic_signals,
                }
            })
            .filter(|entry| {
                entry.risk_score > 0.0
                    || entry.heuristic_signals.tremor_observation_count > 0
                    || entry.heuristic_signals.trust_risk_multiplier > 1.0
            })
            .collect();

        hotspots.sort_by(|a, b| {
            b.risk_score
                .partial_cmp(&a.risk_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hotspots.truncate(5);
        hotspots
    };

    // Build markdown summary
    let graph = state.graph.read();
    let node_count = graph.num_nodes();
    let edge_count = graph.num_edges() as u64;
    drop(graph);

    let uptime = state.uptime_seconds();
    let markdown_summary = format!(
        "## m1nd Session Report\n\n\
         | Metric | Value |\n|---|---|\n\
         | Uptime | {:.0}s |\n\
         | Queries (this agent) | {} |\n\
         | Total elapsed | {:.0}ms |\n\
         | Graph nodes | {} |\n\
         | Graph edges | {} |\n\
         | Verbs ever called (all sessions) | {} |\n\n\
         ### Recent Queries\n{}\n\
         ### Verb Usage (durable, all agents, all sessions)\n{}\n\
         ### Heuristic Hotspots\n{}",
        uptime,
        session_queries,
        session_elapsed_ms,
        node_count,
        edge_count,
        verb_usage.len(),
        recent_queries
            .iter()
            .map(|q| format!("- **{}** `{}` ({:.0}ms)\n", q.tool, q.query, q.elapsed_ms))
            .collect::<String>(),
        verb_usage
            .iter()
            .take(20)
            .map(|entry| format!(
                "- **{}** answered={} refused_floor={} refused_dispatch={}\n",
                entry.verb,
                entry.answered,
                entry.refused_at_authority_floor,
                entry.refused_at_dispatch
            ))
            .collect::<String>(),
        heuristic_hotspots
            .iter()
            .map(|entry| {
                format!(
                    "- **{}** `{}` score={:.2} reason={}\n",
                    entry.risk_level,
                    entry.file_path,
                    entry.risk_score,
                    entry.heuristic_signals.reason
                )
            })
            .collect::<String>(),
    );
    let (markdown_summary, truncated, inline_summary) = if let Some(limit) = input.max_output_chars
    {
        if markdown_summary.chars().count() > limit {
            (
                markdown_summary.chars().take(limit).collect::<String>(),
                true,
                Some(format!(
                    "report markdown exceeded {} chars and was truncated inline. Raise max_output_chars for the full narrative.",
                    limit
                )),
            )
        } else {
            (markdown_summary, false, None)
        }
    } else {
        (markdown_summary, false, None)
    };

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    Ok(ReportOutput {
        agent_id: input.agent_id,
        session_queries,
        session_elapsed_ms,
        queries_answered,
        recent_queries,
        heuristic_hotspots,
        verb_usage,
        markdown_summary,
        truncated,
        inline_summary,
    })
}

// ---------------------------------------------------------------------------
// m1nd.panoramic
// ---------------------------------------------------------------------------

pub fn handle_panoramic(
    state: &mut SessionState,
    input: PanoramicInput,
) -> M1ndResult<PanoramicOutput> {
    let start = Instant::now();
    let top_n = (input.top_n as usize).clamp(1, 1000);
    let normalized_scope = normalize_panoramic_scope(input.scope.as_deref(), &state.ingest_roots);
    let scope = normalized_scope.as_deref();
    let scope_applied = scope.is_some();

    // Collect all file-level nodes
    let graph = state.graph.read();
    let num_nodes = graph.num_nodes() as usize;

    if num_nodes == 0 {
        drop(graph);
        let (graph_state, recovery) = state.retrieval_failure_context(
            &input.agent_id,
            "panoramic",
            "blocked",
            Some(0),
            input.scope.as_deref(),
            None,
        );
        let agent_runtime_contract = Some(state.agent_runtime_contract(
            &input.agent_id,
            "panoramic",
            "blocked",
            Some(0),
            input.scope.as_deref(),
            None,
        ));
        return Ok(PanoramicOutput {
            modules: vec![],
            total_modules: 0,
            critical_alerts: vec![],
            scope_applied,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            proof_state: Some("blocked".into()),
            next_suggested_tool: Some("recovery_playbook".into()),
            next_step_hint: Some(
                "Call recovery_playbook with the provided recovery.arguments payload before treating an empty panorama as true repo state.".into(),
            ),
            graph_state,
            recovery,
            agent_runtime_contract,
        });
    }

    let mut modules: Vec<PanoramicModule> = Vec::new();

    for (interned, &nid) in graph.id_to_node.iter() {
        let ext_id = graph.strings.resolve(*interned);

        // Scope filter
        if let Some(prefix) = scope {
            if !ext_id.starts_with(prefix) {
                continue;
            }
        }

        // Only file-level nodes for panoramic. The `file::` prefix alone does NOT
        // say that: a symbol's id is built from its file's, so `file::app.ts::fn::s`
        // shares it. Measured on a 103k-node brain, that let minifier-renamed
        // helpers — `…::fn::s`, `…::fn::t`, `…::fn::h` — take the top of the risk
        // ranking, which is a ranking of MODULES an agent might have to change
        // (askGOD F5 verdict, 2026-07-24). Ask the node what it is.
        if !matches!(
            graph.nodes.node_type[nid.as_usize()],
            m1nd_core::types::NodeType::File
        ) {
            continue;
        }

        // Calculate blast radius using CSR (forward: out-edges, backward: in-edges)
        let out_range = graph.csr.out_range(nid);
        let in_range = graph.csr.in_range(nid);
        let blast_forward = out_range.len() as u32;
        let blast_backward = in_range.len() as u32;

        // Calculate centrality (normalized degree)
        let total_edges = (blast_forward + blast_backward) as f32;
        let max_possible = if num_nodes > 1 {
            (num_nodes - 1) as f32 * 2.0
        } else {
            1.0
        };
        let centrality = (total_edges / max_possible).min(1.0);

        // Estimate churn from tremor data
        let churn = 0.0f32; // Default; tremor gives volatility, not churn directly

        // Combined risk: blast*0.5 + centrality*0.3 + churn*0.2
        let blast_normalized =
            ((blast_forward + blast_backward) as f32 / (num_nodes as f32).max(1.0)).min(1.0);
        let combined_risk = blast_normalized * 0.5 + centrality * 0.3 + churn * 0.2;
        let is_critical = combined_risk >= 0.7;

        let label = ext_id.strip_prefix("file::").unwrap_or(ext_id).to_string();

        modules.push(PanoramicModule {
            node_id: ext_id.to_string(),
            label: label.clone(),
            file_path: label,
            blast_forward,
            blast_backward,
            centrality,
            combined_risk,
            is_critical,
        });
    }

    drop(graph);

    // Sort by combined_risk descending
    modules.sort_by(|a, b| {
        b.combined_risk
            .partial_cmp(&a.combined_risk)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_modules = modules.len();

    // Build critical alerts
    let critical_alerts: Vec<PanoramicAlert> = modules
        .iter()
        .filter(|m| m.is_critical)
        .map(|m| PanoramicAlert {
            node_id: m.node_id.clone(),
            label: m.label.clone(),
            combined_risk: m.combined_risk,
            reason: format!(
                "high combined risk ({:.2}): blast_fwd={}, blast_bwd={}, centrality={:.2}",
                m.combined_risk, m.blast_forward, m.blast_backward, m.centrality
            ),
        })
        .collect();

    // Truncate to top_n
    modules.truncate(top_n);

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let failed_retrieval = total_modules == 0;
    let (graph_state, recovery) = if failed_retrieval {
        state.retrieval_failure_context(
            &input.agent_id,
            "panoramic",
            "blocked",
            Some(0),
            input.scope.as_deref(),
            None,
        )
    } else {
        (None, None)
    };
    let proof_state = if failed_retrieval {
        "blocked"
    } else {
        "triaging"
    };
    let agent_runtime_contract = Some(state.agent_runtime_contract(
        &input.agent_id,
        "panoramic",
        proof_state,
        Some(total_modules as u64),
        input.scope.as_deref(),
        None,
    ));

    Ok(PanoramicOutput {
        modules,
        total_modules,
        critical_alerts,
        scope_applied,
        elapsed_ms: elapsed,
        proof_state: Some(proof_state.into()),
        next_suggested_tool: if failed_retrieval {
            Some("recovery_playbook".into())
        } else {
            None
        },
        next_step_hint: if failed_retrieval {
            Some(
                "Call recovery_playbook with the provided recovery.arguments payload before treating an empty panorama as true repo state.".into(),
            )
        } else {
            None
        },
        graph_state,
        recovery,
        agent_runtime_contract,
    })
}

fn normalize_panoramic_scope(scope: Option<&str>, ingest_roots: &[String]) -> Option<String> {
    normalize_scope_path(scope, ingest_roots).map(|scope| format!("file::{}", scope))
}

#[cfg(test)]
mod tests {
    use super::{handle_panoramic, handle_report};
    use crate::protocol::layers::{PanoramicInput, ReportInput};
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::{Graph, NodeProvenanceInput};
    use m1nd_core::types::NodeType;

    fn build_report_state(root: &std::path::Path) -> SessionState {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };

        let mut graph = Graph::new();
        let core = graph
            .add_node(
                "file::src/core.rs",
                "core.rs",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add core node");
        graph.set_node_provenance(
            core,
            NodeProvenanceInput {
                source_path: Some("src/core.rs"),
                line_start: Some(1),
                line_end: Some(10),
                excerpt: None,
                namespace: None,
                canonical: true,
            },
        );
        let ui = graph
            .add_node("file::src/ui.rs", "ui.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add ui node");
        graph.set_node_provenance(
            ui,
            NodeProvenanceInput {
                source_path: Some("src/ui.rs"),
                line_start: Some(1),
                line_end: Some(10),
                excerpt: None,
                namespace: None,
                canonical: true,
            },
        );
        graph.finalize().expect("finalize graph");

        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());
        state
    }

    fn build_empty_report_state(root: &std::path::Path) -> SessionState {
        let runtime_dir = root.join("runtime-empty");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };

        SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init empty session")
    }

    #[test]
    fn panoramic_resolves_absolute_scope_under_ingest_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_report_state(root);

        let output = handle_panoramic(
            &mut state,
            PanoramicInput {
                agent_id: "test".into(),
                scope: Some(root.join("src").to_string_lossy().to_string()),
                top_n: 10,
            },
        )
        .expect("panoramic should succeed");

        assert_eq!(output.total_modules, 2);
        assert!(!output.modules.is_empty());
        assert!(output
            .modules
            .iter()
            .all(|m| m.node_id.starts_with("file::src/")));
    }

    #[test]
    fn panoramic_empty_graph_points_to_recovery_playbook() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_empty_report_state(temp.path());

        let output = handle_panoramic(
            &mut state,
            PanoramicInput {
                agent_id: "test".into(),
                scope: Some(temp.path().join("src").to_string_lossy().to_string()),
                top_n: 10,
            },
        )
        .expect("panoramic should return a diagnostic output");

        assert_eq!(output.total_modules, 0);
        assert_eq!(output.proof_state.as_deref(), Some("blocked"));
        assert_eq!(
            output.next_suggested_tool.as_deref(),
            Some("recovery_playbook")
        );
        assert!(
            output.graph_state.is_some(),
            "empty panoramic output should include graph_state"
        );
        assert!(
            output.recovery.is_some(),
            "empty panoramic output should include recovery arguments"
        );
        assert_eq!(
            output
                .agent_runtime_contract
                .as_ref()
                .and_then(|contract| contract["schema"].as_str()),
            Some("m1nd-agent-runtime-contract-v0")
        );
        assert_eq!(
            output
                .agent_runtime_contract
                .as_ref()
                .and_then(|contract| contract["trust_mode"].as_str()),
            Some("wrong_workspace_binding")
        );
    }

    /// The read surface: `report` is the ONE verb that answers "which verbs are
    /// used, how often" — no second tool, and no `agent_id` filter, because the
    /// ledger has no agent dimension.
    #[test]
    fn verb_usage_report_surfaces_durable_counts_for_every_agent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_report_state(temp.path());

        crate::server::dispatch_generic_tool(
            &mut state,
            "health",
            &serde_json::json!({"agent_id": "agent-one"}),
        )
        .expect("health answers");
        crate::server::dispatch_generic_tool(
            &mut state,
            "ingest",
            &serde_json::json!({"agent_id": "agent-one", "mode": "merge", "paths": ["."]}),
        )
        .expect_err("elevated ingest refuses at the floor");

        // A DIFFERENT agent asks. The durable counters are the brain's, not the
        // caller's, so both calls above are visible here.
        let output = handle_report(
            &mut state,
            ReportInput {
                agent_id: "agent-two".into(),
                max_output_chars: None,
            },
        )
        .expect("report should succeed");

        let health = output
            .verb_usage
            .iter()
            .find(|entry| entry.verb == "health")
            .expect("health in the durable verb usage");
        assert_eq!(health.answered, 1);
        let ingest = output
            .verb_usage
            .iter()
            .find(|entry| entry.verb == "ingest")
            .expect("ingest in the durable verb usage");
        assert_eq!(ingest.refused_at_authority_floor, 1);
        assert_eq!(
            ingest.answered, 0,
            "report must not present a refusal as an answer"
        );
        assert_eq!(
            output.session_queries, 0,
            "this agent asked nothing this session — the durable counts are a \
             different fact from the session log"
        );
        assert!(output.markdown_summary.contains("Verb Usage"));
    }

    #[test]
    fn report_surfaces_heuristic_hotspots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_report_state(root);
        let now = 15_000.0;

        state
            .trust_ledger
            .record_defect("file::src/core.rs", now - 120.0);
        state
            .trust_ledger
            .record_defect("file::src/core.rs", now - 60.0);
        state
            .tremor_registry
            .record_observation("file::src/core.rs", 1.0, 4, now - 30.0);
        state
            .tremor_registry
            .record_observation("file::src/core.rs", 1.1, 4, now - 20.0);
        state
            .tremor_registry
            .record_observation("file::src/core.rs", 1.2, 4, now - 10.0);

        let output = handle_report(
            &mut state,
            ReportInput {
                agent_id: "test".into(),
                max_output_chars: None,
            },
        )
        .expect("report should succeed");

        assert!(!output.heuristic_hotspots.is_empty());
        assert_eq!(output.heuristic_hotspots[0].node_id, "file::src/core.rs");
        assert!(output.markdown_summary.contains("Heuristic Hotspots"));
    }
}
