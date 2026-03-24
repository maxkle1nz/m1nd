// === m1nd-mcp/src/report_handlers.rs ===
//
// v0.4.0: Handlers for m1nd.report, m1nd.panoramic, m1nd.savings.

use crate::personality;
use crate::protocol::layers::{
    PanoramicAlert, PanoramicInput, PanoramicModule, PanoramicOutput, ReportHeuristicHotspot,
    ReportInput, ReportOutput, ReportQueryEntry, SavingsInput, SavingsOutput, SavingsSessionRecord,
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

    // Savings from tracker
    let tokens_saved_session = state.savings_tracker.tokens_saved;
    let tokens_saved_global = state.global_savings.total_tokens_saved + tokens_saved_session;
    let co2_saved_grams = (tokens_saved_global as f64) * 0.0002;

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
         | Tokens saved (session) | {} |\n\
         | Tokens saved (global) | {} |\n\
         | CO2 saved | {:.2}g |\n\
         | Graph nodes | {} |\n\
         | Graph edges | {} |\n\n\
         ### Recent Queries\n{}\n\
         ### Heuristic Hotspots\n{}",
        uptime,
        session_queries,
        session_elapsed_ms,
        tokens_saved_session,
        tokens_saved_global,
        co2_saved_grams,
        node_count,
        edge_count,
        recent_queries
            .iter()
            .map(|q| format!("- **{}** `{}` ({:.0}ms)\n", q.tool, q.query, q.elapsed_ms))
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

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    Ok(ReportOutput {
        agent_id: input.agent_id,
        session_queries,
        session_elapsed_ms,
        queries_answered,
        tokens_saved_session,
        tokens_saved_global,
        co2_saved_grams,
        recent_queries,
        heuristic_hotspots,
        markdown_summary,
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
        return Ok(PanoramicOutput {
            modules: vec![],
            total_modules: 0,
            critical_alerts: vec![],
            scope_applied,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
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

        // Only file-level nodes for panoramic
        if !ext_id.starts_with("file::") {
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

    Ok(PanoramicOutput {
        modules,
        total_modules,
        critical_alerts,
        scope_applied,
        elapsed_ms: elapsed,
    })
}

fn normalize_panoramic_scope(scope: Option<&str>, ingest_roots: &[String]) -> Option<String> {
    normalize_scope_path(scope, ingest_roots).map(|scope| format!("file::{}", scope))
}

// ---------------------------------------------------------------------------
// m1nd.savings
// ---------------------------------------------------------------------------

pub fn handle_savings(state: &mut SessionState, input: SavingsInput) -> M1ndResult<SavingsOutput> {
    let start = Instant::now();

    let session_tokens_saved = state.savings_tracker.tokens_saved;
    let global_tokens_saved = state.global_savings.total_tokens_saved + session_tokens_saved;
    let global_co2_grams = (global_tokens_saved as f64) * 0.0002;
    let cost_saved_usd = (global_tokens_saved as f64) * 0.000003; // $0.003/1K tokens

    let session_queries: u32 = state.savings_tracker.queries_by_tool.values().sum::<u64>() as u32;

    let session_start_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
        - (state.uptime_seconds() * 1000.0) as u64;

    let recent_sessions = vec![SavingsSessionRecord {
        agent_id: input.agent_id.clone(),
        session_start_ms,
        queries: session_queries,
        tokens_saved: session_tokens_saved,
        co2_grams: (session_tokens_saved as f64) * 0.0002,
    }];

    // Formatted summary with visual identity
    let formatted_summary = format!(
        "{}{} m1nd efficiency report{}\n\n\
         {}session:{} {} queries, {} tokens saved\n\
         {}global:{}  {} tokens saved, ${:.4} USD, {:.2}g CO2\n\n\
         {}every query that didn't burn tokens is a gift to the planet.{}\n",
        personality::ANSI_BOLD,
        personality::ANSI_GREEN,
        personality::ANSI_RESET,
        personality::ANSI_CYAN,
        personality::ANSI_RESET,
        session_queries,
        session_tokens_saved,
        personality::ANSI_GOLD,
        personality::ANSI_RESET,
        global_tokens_saved,
        cost_saved_usd,
        global_co2_grams,
        personality::ANSI_DIM,
        personality::ANSI_RESET,
    );

    Ok(SavingsOutput {
        session_tokens_saved,
        global_tokens_saved,
        global_co2_grams,
        cost_saved_usd,
        recent_sessions,
        formatted_summary,
    })
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
            },
        )
        .expect("report should succeed");

        assert!(!output.heuristic_hotspots.is_empty());
        assert_eq!(output.heuristic_hotspots[0].node_id, "file::src/core.rs");
        assert!(output.markdown_summary.contains("Heuristic Hotspots"));
    }
}
