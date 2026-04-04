// === m1nd-mcp/src/layer_handlers.rs ===
//
// Handler stubs for Layers 2-7 superpowers.
// Each handler: parse typed input -> call engine -> return typed output.
// All bodies are todo!() — to be filled by builder agents per-layer.
//
// Pattern: same as tools.rs / perspective_handlers.rs / lock_handlers.rs.

use crate::protocol::layers;
use crate::result_shaping::dedupe_ranked;
use crate::scope::normalize_scope_path;
use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::seed::source_path_bias;
use m1nd_core::types::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

// =========================================================================
// L2: Semantic Search — m1nd.seek + m1nd.scan
// =========================================================================

fn l2_dampened_trust_factor(raw_factor: f32) -> f32 {
    1.0 + (raw_factor - 1.0) * 0.2
}

fn l2_dampened_tremor_factor(alert: Option<&m1nd_core::tremor::TremorAlert>) -> f32 {
    1.0 + alert.map_or(0.0, |value| value.magnitude.min(1.0) * 0.1)
}

const L2_SEEK_STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "into", "its", "own", "codebase", "task",
    "validate", "using", "focus", "around", "where", "when", "what", "which", "how", "why", "does",
    "should", "could", "would", "about", "need", "needs", "want", "wants", "show", "tell", "there",
    "here", "really", "just", "like",
];

fn l2_seek_heuristic_reason(
    trust_factor: f32,
    tremor_factor: f32,
    tremor_observation_count: usize,
) -> String {
    let mut parts = Vec::new();
    if trust_factor > 1.01 {
        parts.push("low-trust risk prior");
    } else if trust_factor < 0.99 {
        parts.push("high-trust damping");
    }
    if tremor_factor > 1.01 && tremor_observation_count > 0 {
        parts.push("tremor acceleration");
    }
    if parts.is_empty() {
        "neutral heuristics".to_string()
    } else {
        parts.join(" + ")
    }
}

/// Handle m1nd.seek -- intent-aware semantic code search.
/// Finds code by PURPOSE, not text pattern. Combines keyword matching,
/// graph activation (PageRank), and trigram similarity for ranking.
///
/// V1: heuristic intent matching (trigram + identifier splitting) -- zero new deps.
/// V2 upgrade path: fastembed-rs with jina-embeddings-v2-base-code for real embeddings.
///   V2 score: embedding_similarity * 0.5 + graph_activation * 0.3 + temporal_recency * 0.2.
pub fn handle_seek(
    state: &mut SessionState,
    input: layers::SeekInput,
) -> M1ndResult<layers::SeekOutput> {
    let start = Instant::now();
    let query_tokens = l2_seek_tokenize(&input.query);
    let normalized_scope = input
        .scope
        .as_deref()
        .map(|scope| l7_normalize_path_hint(scope, &state.ingest_roots));

    // Split query tokens further via identifier splitting for better matching
    let mut all_tokens: Vec<String> = query_tokens.clone();
    for t in &query_tokens {
        for sub in l2_split_identifier(t) {
            if sub.len() > 1 && !all_tokens.contains(&sub) {
                all_tokens.push(sub);
            }
        }
    }

    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;

    if n == 0 || all_tokens.is_empty() {
        return Ok(layers::SeekOutput {
            query: input.query,
            results: vec![],
            total_candidates_scanned: 0,
            embeddings_used: false,
            proof_state: "blocked".into(),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            next_suggested_tool: None,
            next_suggested_target: None,
            next_step_hint: None,
        });
    }

    let type_filter: Vec<String> = input.node_types.iter().map(|t| t.to_lowercase()).collect();

    // Build reverse lookup: NodeId -> external_id string
    let mut node_to_ext: Vec<String> = vec![String::new(); n];
    for (interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
        }
    }

    // Phase 1: Score every node (keyword match + trigram + provenance path matching)
    let mut keyword_scores: Vec<f32> = vec![0.0; n];
    let mut trigram_scores: Vec<f32> = vec![0.0; n];
    let mut candidates_scanned = 0usize;

    for i in 0..n {
        let nt = &graph.nodes.node_type[i];
        let nt_str = l2_node_type_str(nt);

        // Scope filter: check external_id prefix
        if let Some(ref scope) = normalized_scope {
            let ext = l7_normalize_path_hint(&node_to_ext[i], &state.ingest_roots);
            if !ext.is_empty() && !ext.starts_with(scope.as_str()) {
                continue;
            }
        }

        // Node type filter
        if !type_filter.is_empty() && !type_filter.iter().any(|f| f == nt_str) {
            continue;
        }

        candidates_scanned += 1;

        let label = graph.strings.resolve(graph.nodes.label[i]);
        let label_lower = label.to_lowercase();
        let label_parts = l2_split_identifier(label);

        let prov = &graph.nodes.provenance[i];
        let source_path_lower: String = prov
            .source_path
            .and_then(|s| graph.strings.try_resolve(s))
            .unwrap_or("")
            .to_lowercase();
        // Keyword match score: fraction of query tokens that match this node
        let mut keyword_hits = 0usize;
        let total_tokens = all_tokens.len().max(1);

        for token in &all_tokens {
            if label_lower == *token {
                keyword_hits += 2; // bonus for exact
                continue;
            }
            if label_lower.contains(token.as_str()) {
                keyword_hits += 1;
                continue;
            }
            if label_parts.iter().any(|p| p == token) {
                keyword_hits += 1;
                continue;
            }
            let tag_match = graph.nodes.tags[i].iter().any(|&ti| {
                let tag = graph.strings.resolve(ti).to_lowercase();
                tag == *token || tag.contains(token.as_str())
            });
            if tag_match {
                keyword_hits += 1;
                continue;
            }
            if !source_path_lower.is_empty() && source_path_lower.contains(token.as_str()) {
                keyword_hits += 1;
            }
        }

        keyword_scores[i] = (keyword_hits as f32 / total_tokens as f32).min(1.0);
        trigram_scores[i] = l2_trigram_similarity(&input.query, &label_lower);
    }

    // Phase 2: SemanticEngine scores (trigram TF-IDF + co-occurrence).
    // Build a boost map from the SemanticEngine (char n-gram + DeepWalk-lite co-occurrence).
    let semantic_scores: HashMap<usize, f32> = {
        let sem_results = state
            .orchestrator
            .semantic
            .query(&graph, &input.query, input.top_k * 5)
            .unwrap_or_default();
        sem_results
            .into_iter()
            .map(|(nid, score)| (nid.as_usize(), score.get()))
            .collect()
    };
    let semantic_used = !semantic_scores.is_empty();

    // Phase 3: Combine with graph re-ranking.
    // V2 formula: keyword_match * 0.4 + semantic_embedding * 0.3 + graph_activation(PageRank) * 0.2 + trigram * 0.1
    struct BaseRankedNode {
        idx: usize,
        base_score: f32,
        keyword: f32,
        graph_act: f32,
        trigram: f32,
    }

    struct RankedNode {
        idx: usize,
        combined: f32,
        keyword: f32,
        graph_act: f32,
        trigram: f32,
        heuristic_signals: layers::HeuristicSignals,
    }

    let mut base_ranked: Vec<BaseRankedNode> = Vec::new();
    for i in 0..n {
        let kw = keyword_scores[i];
        let tri = trigram_scores[i];
        let sem = semantic_scores.get(&i).copied().unwrap_or(0.0);
        if kw < 0.01 && tri < 0.15 && sem < 0.05 {
            continue;
        }

        let graph_activation = if input.graph_rerank {
            graph.nodes.pagerank[i].get()
        } else {
            0.0
        };
        let label_lower = graph.strings.resolve(graph.nodes.label[i]).to_lowercase();
        let nt_str = l2_node_type_str(&graph.nodes.node_type[i]);

        let source_path_lower = graph.nodes.provenance[i]
            .source_path
            .and_then(|s| graph.strings.try_resolve(s))
            .unwrap_or("")
            .to_lowercase();
        let tag_terms: Vec<String> = graph.nodes.tags[i]
            .iter()
            .map(|&ti| graph.strings.resolve(ti).to_lowercase())
            .collect();

        let base_score = kw * 0.4
            + sem * 0.3
            + graph_activation * 0.2
            + tri * 0.1
            + source_path_bias(Some(source_path_lower.as_str()), &all_tokens)
            + l2_seek_anchor_bias(
                &all_tokens,
                &label_lower,
                source_path_lower.as_str(),
                &tag_terms,
                nt_str,
            );
        if base_score >= input.min_score {
            base_ranked.push(BaseRankedNode {
                idx: i,
                base_score,
                keyword: kw,
                graph_act: graph_activation,
                trigram: tri,
            });
        }
    }

    base_ranked.sort_by(|a, b| {
        b.base_score
            .partial_cmp(&a.base_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let heuristic_window = input.top_k.saturating_mul(4).max(input.top_k).min(128);
    base_ranked.truncate(heuristic_window);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let mut ranked: Vec<RankedNode> = base_ranked
        .into_iter()
        .map(|entry| {
            let stable_external_id = node_to_ext.get(entry.idx).cloned().unwrap_or_default();
            let external_id = if stable_external_id.is_empty() {
                graph
                    .strings
                    .resolve(graph.nodes.label[entry.idx])
                    .to_string()
            } else {
                stable_external_id.clone()
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
            let trust_factor = l2_dampened_trust_factor(raw_trust_factor);

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
            let tremor_factor = l2_dampened_tremor_factor(tremor_alert.as_ref());
            let heuristic_factor = trust_factor * tremor_factor;

            RankedNode {
                idx: entry.idx,
                combined: (entry.base_score * heuristic_factor).max(0.0),
                keyword: entry.keyword,
                graph_act: entry.graph_act,
                trigram: entry.trigram,
                heuristic_signals: layers::HeuristicSignals {
                    heuristic_factor,
                    trust_score: trust.trust_score,
                    trust_risk_multiplier: trust.risk_multiplier,
                    trust_tier: format!("{:?}", trust.tier),
                    tremor_magnitude: tremor_alert.as_ref().map(|alert| alert.magnitude),
                    tremor_observation_count,
                    tremor_risk_level: tremor_alert
                        .as_ref()
                        .map(|alert| format!("{:?}", alert.risk_level)),
                    reason: l2_seek_heuristic_reason(
                        trust_factor,
                        tremor_factor,
                        tremor_observation_count,
                    ),
                },
            }
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.combined
            .partial_cmp(&a.combined)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.graph_act.total_cmp(&a.graph_act))
            .then_with(|| a.idx.cmp(&b.idx))
    });
    ranked.truncate(input.top_k);

    // Phase 4: Build output
    let results: Vec<layers::SeekResultEntry> = ranked
        .into_iter()
        .map(|r| {
            let i = r.idx;
            let nid = NodeId::new(i as u32);
            let label = graph.strings.resolve(graph.nodes.label[i]).to_string();
            let nt = l2_node_type_str(&graph.nodes.node_type[i]);
            let ext_id = &node_to_ext[i];
            let prov = graph.resolve_node_provenance(nid);
            let tags: Vec<String> = graph.nodes.tags[i]
                .iter()
                .map(|&ti| graph.strings.resolve(ti).to_string())
                .collect();

            // Gather connections (outgoing edges, capped at 5)
            let mut connections = Vec::new();
            if graph.finalized {
                let out = graph.csr.out_range(nid);
                for j in out {
                    if connections.len() >= 5 {
                        break;
                    }
                    let target = graph.csr.targets[j];
                    let tidx = target.as_usize();
                    if tidx < n {
                        let rel = graph.strings.resolve(graph.csr.relations[j]).to_string();
                        let tlabel = graph.strings.resolve(graph.nodes.label[tidx]).to_string();
                        let text_id = if !node_to_ext[tidx].is_empty() {
                            node_to_ext[tidx].clone()
                        } else {
                            tlabel.clone()
                        };
                        connections.push(layers::SeekConnection {
                            node_id: text_id,
                            label: tlabel,
                            relation: rel,
                        });
                    }
                }
            }

            layers::SeekResultEntry {
                node_id: if ext_id.is_empty() {
                    label.clone()
                } else {
                    ext_id.clone()
                },
                label: label.clone(),
                node_type: nt.to_string(),
                score: r.combined,
                score_breakdown: layers::SeekScoreBreakdown {
                    embedding_similarity: r.keyword, // V1: keyword fills embedding slot. V2: fastembed cosine.
                    graph_activation: r.graph_act,
                    temporal_recency: r.trigram, // V1: trigram fills recency slot. V2: real recency.
                },
                heuristic_signals: Some(r.heuristic_signals),
                intent_summary: l2_intent_summary(&label, nt, &tags),
                file_path: prov.source_path,
                line_start: prov.line_start,
                line_end: prov.line_end,
                excerpt: prov.excerpt,
                connections,
            }
        })
        .collect();
    let results = dedupe_ranked(results, input.top_k);

    drop(graph);

    state.queries_processed += 1;
    if state.should_persist() {
        let _ = state.persist();
    }

    let (next_suggested_tool, next_suggested_target, next_step_hint) = l2_seek_next_step(&results);
    let proof_state = l2_seek_proof_state(&results);

    Ok(layers::SeekOutput {
        query: input.query,
        results,
        total_candidates_scanned: candidates_scanned,
        embeddings_used: semantic_used,
        proof_state,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    })
}

fn l2_seek_next_step(
    results: &[layers::SeekResultEntry],
) -> (Option<String>, Option<String>, Option<String>) {
    let Some(top) = results.first() else {
        return (None, None, None);
    };
    let path = top
        .file_path
        .clone()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| node_to_file_path(&top.node_id));
    let target = if path.is_empty() {
        top.node_id.clone()
    } else {
        path.clone()
    };
    let hint = if !path.is_empty() {
        format!("Open the top seek result next: {} in {}.", top.label, path)
    } else {
        format!("Open the top seek result next: {}.", top.label)
    };
    (Some("view".into()), Some(target), Some(hint))
}

/// Handle m1nd.scan -- pattern-aware structural code analysis.
/// Detects code quality issues using predefined patterns with graph-aware
/// cross-file validation. NOT a linter -- checks structural patterns.
///
/// V1: keyword-on-labels pattern matching + graph validation via CSR edges.
/// 8 predefined categories: error_handling, resource_cleanup, api_surface,
/// state_mutation, concurrency, auth_boundary, test_coverage, dependency_injection.
/// V2 upgrade path: replace label keyword matching with ast-grep-core structural patterns.
pub fn handle_scan(
    state: &mut SessionState,
    input: layers::ScanInput,
) -> M1ndResult<layers::ScanOutput> {
    let start = Instant::now();
    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    let normalized_scope = input
        .scope
        .as_deref()
        .map(|scope| l7_normalize_path_hint(scope, &state.ingest_roots));

    if n == 0 {
        return Ok(layers::ScanOutput {
            pattern: input.pattern,
            findings: vec![],
            files_scanned: 0,
            total_matches_raw: 0,
            total_matches_validated: 0,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    }

    // Resolve pattern: predefined or custom keyword list
    let predefined = l2_find_scan_pattern(&input.pattern);
    let (pattern_id, keywords, negations, base_severity, message_template) = match predefined {
        Some(p) => (
            p.id.to_string(),
            p.label_keywords
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            p.negation_keywords
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            p.base_severity,
            p.message_template.to_string(),
        ),
        None => {
            let kws: Vec<String> = input
                .pattern
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase())
                .collect();
            (
                input.pattern.clone(),
                kws,
                vec![],
                0.5,
                format!("Custom pattern match: {}", input.pattern),
            )
        }
    };

    // Build reverse lookup
    let mut node_to_ext: Vec<String> = vec![String::new(); n];
    for (interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
        }
    }

    // Phase 1: find raw matches
    let mut raw_matches: Vec<usize> = Vec::new();
    let mut files_scanned_set = std::collections::HashSet::new();

    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        if let Some(ref scope) = normalized_scope {
            let ext = l7_normalize_path_hint(&node_to_ext[i], &state.ingest_roots);
            if !ext.is_empty() && !ext.starts_with(scope.as_str()) {
                continue;
            }
        }

        let label = graph.strings.resolve(graph.nodes.label[i]).to_lowercase();
        let prov = &graph.nodes.provenance[i];
        let source_path = prov
            .source_path
            .and_then(|s| graph.strings.try_resolve(s))
            .unwrap_or("");
        if !source_path.is_empty() {
            files_scanned_set.insert(source_path.to_string());
        }

        for kw in &keywords {
            if label.contains(kw.as_str()) {
                let negated = negations.iter().any(|nk| label.contains(nk.as_str()));
                if !negated {
                    raw_matches.push(i);
                    break;
                }
            }
        }
    }

    let total_raw = raw_matches.len();

    // Phase 2: graph validation + severity filtering
    let neg_refs: Vec<&str> = negations.iter().map(|s| s.as_str()).collect();
    let mut findings: Vec<layers::ScanFinding> = Vec::new();

    for &node_idx in &raw_matches {
        if findings.len() >= input.limit {
            break;
        }

        let nid = NodeId::new(node_idx as u32);
        let label = graph
            .strings
            .resolve(graph.nodes.label[node_idx])
            .to_string();
        let prov = graph.resolve_node_provenance(nid);

        let (status, graph_context) = if input.graph_validate {
            l2_graph_validate(&graph, nid, &neg_refs, n, &node_to_ext)
        } else {
            ("confirmed", Vec::new())
        };

        let severity = match status {
            "mitigated" => base_severity * 0.4,
            "false_positive" => base_severity * 0.1,
            _ => base_severity,
        };
        if severity < input.severity_min {
            continue;
        }

        findings.push(layers::ScanFinding {
            pattern: pattern_id.clone(),
            status: status.to_string(),
            severity,
            node_id: if !node_to_ext[node_idx].is_empty() {
                node_to_ext[node_idx].clone()
            } else {
                label.clone()
            },
            label: label.clone(),
            file_path: prov.source_path.unwrap_or_default(),
            line: prov.line_start.unwrap_or(0),
            message: message_template.clone(),
            graph_context,
        });
    }

    let total_validated = findings.len();
    drop(graph);

    state.queries_processed += 1;
    if state.should_persist() {
        let _ = state.persist();
    }

    Ok(layers::ScanOutput {
        pattern: pattern_id,
        findings,
        files_scanned: files_scanned_set.len(),
        total_matches_raw: total_raw,
        total_matches_validated: total_validated,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

// =========================================================================
// L3: Temporal Intelligence — m1nd.timeline + m1nd.diverge
// =========================================================================

/// Handle m1nd.timeline — git-based temporal history for a node.
/// Returns change history, co-change partners, velocity, stability score.
///
/// V1 implementation: shells out to `git log` via std::process::Command.
/// CRITICAL: filters out auto-sync commits ("auto-sync from Mac") from co-change analysis.
/// Co-change coupling: co_changes(A,B) / max(changes(A), changes(B)).
pub fn handle_timeline(
    state: &mut SessionState,
    input: layers::TimelineInput,
) -> M1ndResult<layers::TimelineOutput> {
    let start = Instant::now();

    // --- Resolve node to file path ---
    let file_path = resolve_timeline_file_path(state, &input.node);
    let repo_root = discover_git_root(state)?;

    // --- Parse depth into git --after arg ---
    let after_arg = depth_to_after_arg(&input.depth);

    // --- Run git log ---
    let mut cmd = Command::new("git");
    cmd.current_dir(&repo_root);
    cmd.args(["log", "--follow", "--format=%H|%ai|%an|%s", "--numstat"]);
    if let Some(ref after) = after_arg {
        cmd.arg(format!("--after={}", after));
    }
    cmd.arg("--");
    cmd.arg(&file_path);

    let output = cmd.output().map_err(M1ndError::Io)?;

    if !output.status.success() {
        // Git command failed — likely not a git repo or file not tracked.
        // Return empty timeline gracefully.
        return Ok(layers::TimelineOutput {
            node: input.node.clone(),
            depth: input.depth.clone(),
            proof_state: "blocked".into(),
            changes: vec![],
            co_changed_with: vec![],
            velocity: "stable".into(),
            stability_score: 1.0,
            pattern: "dormant".into(),
            total_churn: layers::ChurnSummary {
                lines_added: 0,
                lines_deleted: 0,
            },
            commit_count_in_window: 0,
            next_suggested_tool: Some("view".into()),
            next_suggested_target: Some(file_path.clone()),
            next_step_hint: Some(format!(
                "No git history was found for {} in this window; inspect the current file directly.",
                file_path
            )),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    }

    let raw = String::from_utf8_lossy(&output.stdout);

    // --- Parse git log output into commit records ---
    let commits = parse_git_log_output(&raw);

    // --- Separate auto-sync commits: they count for the target file's own
    //     history but NOT for co-change coupling analysis ---
    let all_commits: &[GitCommitRecord] = &commits;
    let non_autosync_commits: Vec<&GitCommitRecord> = commits
        .iter()
        .filter(|c| !is_auto_sync_commit(&c.subject))
        .collect();

    // --- Build TimelineChange entries (from ALL commits, including auto-sync) ---
    let mut changes: Vec<layers::TimelineChange> = Vec::with_capacity(all_commits.len());
    let mut total_added: u32 = 0;
    let mut total_deleted: u32 = 0;

    for c in all_commits {
        let (added, deleted) = c.churn_for_file(&file_path);
        total_added += added;
        total_deleted += deleted;

        // co_changed list for this commit: other files in the same commit
        // (only from non-autosync, and only files within the same repo scope)
        let co_changed: Vec<String> = if is_auto_sync_commit(&c.subject) {
            vec![] // auto-sync commits get empty co-change list
        } else {
            c.files_changed
                .iter()
                .filter(|f| f.path != file_path)
                .map(|f| f.path.clone())
                .collect()
        };

        changes.push(layers::TimelineChange {
            date: c.date.clone(),
            commit: c.hash.clone(),
            author: c.author.clone(),
            subject: c.subject.clone(),
            delta: format!("+{}/-{}", added, deleted),
            co_changed,
        });
    }

    // --- Compute co-change partners (from NON-autosync commits only) ---
    let co_changed_with = if input.include_co_changes {
        compute_co_change_partners(
            &file_path,
            &non_autosync_commits,
            all_commits.len(),
            input.top_k,
        )
    } else {
        vec![]
    };

    // --- Compute velocity: compare recent half vs older half ---
    let commit_count = all_commits.len();
    let velocity = compute_velocity(all_commits);

    // --- Compute stability score ---
    // Inverse of change frequency, normalized 0-1.
    // Higher = more stable. 0 commits in window = perfect stability.
    let stability_score = if commit_count == 0 {
        1.0f32
    } else {
        // Map commit count to [0, 1]: 1 commit=0.95, 10=0.5, 50+=~0.05
        (1.0 / (1.0 + (commit_count as f32 / 10.0))).min(1.0)
    };

    // --- Compute pattern ---
    let pattern = compute_churn_pattern(total_added, total_deleted, commit_count, &velocity);
    let proof_state = timeline_proof_state(commit_count, &co_changed_with);
    let (next_suggested_tool, next_suggested_target, next_step_hint) =
        timeline_next_step(&file_path, commit_count, &co_changed_with);

    Ok(layers::TimelineOutput {
        node: input.node,
        depth: input.depth,
        proof_state,
        changes,
        co_changed_with,
        velocity,
        stability_score,
        pattern,
        total_churn: layers::ChurnSummary {
            lines_added: total_added,
            lines_deleted: total_deleted,
        },
        commit_count_in_window: commit_count,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn timeline_proof_state(
    commit_count: usize,
    co_changed_with: &[layers::CoChangePartner],
) -> String {
    if commit_count == 0 {
        return "blocked".into();
    }
    if commit_count > 1 || !co_changed_with.is_empty() {
        return "proving".into();
    }
    "triaging".into()
}

fn timeline_next_step(
    file_path: &str,
    commit_count: usize,
    co_changed_with: &[layers::CoChangePartner],
) -> (Option<String>, Option<String>, Option<String>) {
    if commit_count == 0 {
        return (
            Some("view".into()),
            Some(file_path.to_string()),
            Some(format!(
                "No timeline evidence was found; inspect {} directly to verify the current seam.",
                file_path
            )),
        );
    }

    if let Some(partner) = co_changed_with.first() {
        return (
            Some("view".into()),
            Some(partner.file.clone()),
            Some(format!(
                "Open {} next; it is the strongest co-change partner for {} in this window.",
                partner.file, file_path
            )),
        );
    }

    (
        Some("view".into()),
        Some(file_path.to_string()),
        Some(format!(
            "Open {} next and compare it against the recent commit subjects from this timeline.",
            file_path
        )),
    )
}

fn l2_seek_proof_state(results: &[layers::SeekResultEntry]) -> String {
    let Some(top) = results.first() else {
        return "blocked".into();
    };

    let target_path = top
        .file_path
        .clone()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| node_to_file_path(&top.node_id));
    let second_score = results.get(1).map(|entry| entry.score).unwrap_or(0.0);
    let margin = top.score - second_score;

    if target_path.is_empty() {
        return "triaging".into();
    }
    if top.score >= 0.85 && margin >= 0.25 && top.node_type == "file" {
        return "ready_to_edit".into();
    }
    if top.score >= 0.45 && (margin >= 0.02 || results.len() == 1) {
        return "proving".into();
    }
    "triaging".into()
}

/// Handle m1nd.diverge — structural drift between two points in time.
/// Compares graph state at baseline vs current, reports topology changes.
///
/// V1 implementation: uses `git log` + `git ls-tree` for date-based baselines,
/// or loads previous graph snapshot for "last_session" baseline.
/// Anomaly detection: test_deficit, velocity_spike.
pub fn handle_diverge(
    state: &mut SessionState,
    input: layers::DivergeInput,
) -> M1ndResult<layers::DivergeOutput> {
    let start = Instant::now();
    let repo_root = discover_git_root(state)?;
    let normalized_scope = input
        .scope
        .as_deref()
        .map(|scope| l7_normalize_path_hint(scope, &state.ingest_roots));

    // --- Collect current graph node set (file-type nodes only) ---
    let current_files: HashMap<String, u32> = {
        let graph = state.graph.read();
        collect_file_nodes(&graph, normalized_scope.as_deref())
    };

    // --- Resolve baseline file set ---
    let (baseline_files, baseline_commit) = resolve_baseline_files(
        &repo_root,
        &input.baseline,
        &state.graph_path,
        normalized_scope.as_deref(),
    )?;

    // --- Compute structural drift = 1.0 - jaccard(baseline, current) ---
    let baseline_set: std::collections::HashSet<&str> =
        baseline_files.keys().map(|s| s.as_str()).collect();
    let current_set: std::collections::HashSet<&str> =
        current_files.keys().map(|s| s.as_str()).collect();

    let intersection = baseline_set.intersection(&current_set).count();
    let union = baseline_set.union(&current_set).count();
    let structural_drift = if union == 0 {
        0.0f32
    } else {
        1.0 - (intersection as f32 / union as f32)
    };

    // --- New nodes: in current but not baseline ---
    let new_nodes: Vec<String> = current_set
        .difference(&baseline_set)
        .map(|s| s.to_string())
        .collect();

    // --- Removed nodes: in baseline but not current ---
    let removed_nodes: Vec<String> = baseline_set
        .difference(&current_set)
        .map(|s| s.to_string())
        .collect();

    // --- Modified nodes: in both, compute size delta via git ---
    let modified_nodes: Vec<layers::DivergeModifiedNode> = if input.baseline != "last_session" {
        compute_modified_nodes(&repo_root, &input.baseline, normalized_scope.as_deref())
    } else {
        // For last_session, use git diff against the snapshot's recorded state.
        // Fallback: report all files as unmodified.
        vec![]
    };

    // --- Coupling changes ---
    let coupling_changes: Vec<layers::CouplingChange> = if input.include_coupling_changes {
        compute_coupling_changes(&repo_root, &input.baseline, normalized_scope.as_deref())
    } else {
        vec![]
    };

    // --- Anomaly detection ---
    let anomalies: Vec<layers::DivergeAnomaly> = if input.include_anomalies {
        detect_anomalies(&new_nodes, &removed_nodes, &modified_nodes, &current_files)
    } else {
        vec![]
    };

    // --- Summary ---
    let summary = format!(
        "Drift {:.1}% from baseline '{}'. {} new, {} removed, {} modified files. {} anomalies.",
        structural_drift * 100.0,
        input.baseline,
        new_nodes.len(),
        removed_nodes.len(),
        modified_nodes.len(),
        anomalies.len(),
    );

    Ok(layers::DivergeOutput {
        baseline: input.baseline,
        baseline_commit,
        scope: normalized_scope,
        structural_drift,
        new_nodes,
        removed_nodes,
        modified_nodes,
        coupling_changes,
        anomalies,
        summary,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

// =========================================================================
// L3: Temporal Intelligence — helper functions + data types
// =========================================================================

// ---------------------------------------------------------------------------
// L3 internal data model
// ---------------------------------------------------------------------------

/// A single file-level stat line from `git log --numstat`.
#[derive(Debug, Clone)]
struct FileChurn {
    path: String,
    added: u32,
    deleted: u32,
}

/// One parsed commit from `git log --follow --format=%H|%ai|%an|%s --numstat`.
#[derive(Debug, Clone)]
struct GitCommitRecord {
    hash: String,
    date: String,
    author: String,
    subject: String,
    files_changed: Vec<FileChurn>,
}

impl GitCommitRecord {
    /// Return (added, deleted) for a specific file in this commit.
    fn churn_for_file(&self, target: &str) -> (u32, u32) {
        let normalized_target = l6_normalize_path(target);
        for f in &self.files_changed {
            if timeline_paths_match(&f.path, &normalized_target) {
                return (f.added, f.deleted);
            }
        }
        (0, 0)
    }
}

// ---------------------------------------------------------------------------
// L3 helper functions — used by handle_timeline and handle_diverge
// ---------------------------------------------------------------------------

/// Returns true if the commit subject matches the auto-sync pattern.
/// FM-L3-001: auto-sync commits bundle unrelated changes across projects,
/// creating false coupling. They are excluded from co-change analysis.
fn is_auto_sync_commit(subject: &str) -> bool {
    subject.starts_with("auto-sync from ")
}

fn timeline_paths_match(candidate: &str, normalized_target: &str) -> bool {
    let normalized_candidate = l6_normalize_path(candidate);
    if normalized_candidate == normalized_target {
        return true;
    }

    if !normalized_target.is_empty()
        && (normalized_candidate.ends_with(normalized_target)
            || normalized_target.ends_with(&normalized_candidate))
    {
        return true;
    }

    let target_name = Path::new(normalized_target)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let candidate_name = Path::new(&normalized_candidate)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    !target_name.is_empty() && target_name == candidate_name
}

/// Convert a node external_id (e.g. "file::backend/chat_handler.py") to a
/// relative file path (e.g. "backend/chat_handler.py").
fn node_to_file_path(node_id: &str) -> String {
    let trimmed = node_id.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let candidate = if let Some(idx) = trimmed.find("file::") {
        &trimmed[idx + "file::".len()..]
    } else {
        trimmed
    };

    let file_like = candidate.split("::").next().unwrap_or(candidate);
    if file_like.starts_with('/') {
        file_like.trim_end_matches('/').to_string()
    } else {
        file_like.trim_matches('/').to_string()
    }
}

/// Resolve a timeline target to the repo-relative file path `git log` expects.
///
/// This prefers graph provenance when possible so equivalent identities like:
/// - `file::src/main.rs`
/// - `file::src/main.rs::fn::boot`
/// - `repo::file::src/main.rs::fn::boot`
/// - `/abs/root/src/main.rs`
///
/// all converge on the same repo-relative file path.
fn resolve_timeline_file_path(state: &SessionState, node_id: &str) -> String {
    let raw_path = node_to_file_path(node_id);
    let normalized_hint = l7_normalize_path_hint(&raw_path, &state.ingest_roots);
    let fallback = if normalized_hint.is_empty() {
        raw_path.clone()
    } else {
        normalized_hint.clone()
    };

    let graph = state.graph.read();

    for candidate in [
        node_id.to_string(),
        raw_path.clone(),
        fallback.clone(),
        format!("file::{}", raw_path),
        format!("file::{}", fallback),
    ] {
        if candidate.is_empty() {
            continue;
        }
        if let Some(nid) = graph.resolve_id(&candidate) {
            let prov = graph.resolve_node_provenance(nid);
            if let Some(source_path) = prov.source_path {
                let normalized = l7_normalize_path_hint(&source_path, &state.ingest_roots);
                if !normalized.is_empty() {
                    return normalized;
                }
            }
        }
    }

    let normalized_fallback = l6_normalize_path(&fallback);
    let mut first_match: Option<String> = None;
    for i in 0..graph.num_nodes() as usize {
        let prov = &graph.nodes.provenance[i];
        if let Some(sp) = prov.source_path {
            if let Some(source_str) = graph.strings.try_resolve(sp) {
                let source_norm = l6_normalize_path(source_str);
                let paths_match = source_norm == normalized_fallback
                    || source_norm.ends_with(&normalized_fallback)
                    || normalized_fallback.ends_with(&source_norm);
                if !paths_match {
                    continue;
                }

                let normalized = l7_normalize_path_hint(source_str, &state.ingest_roots);
                if normalized.is_empty() {
                    continue;
                }

                if graph.nodes.node_type[i] == m1nd_core::types::NodeType::File {
                    return normalized;
                }
                first_match.get_or_insert(normalized);
            }
        }
    }

    first_match.unwrap_or(fallback)
}

/// Walk up from the first ingest root (or graph_path) to find the .git directory.
fn discover_git_root(state: &SessionState) -> M1ndResult<PathBuf> {
    // Try ingest_roots first
    for root in &state.ingest_roots {
        let p = PathBuf::from(root);
        if p.join(".git").exists() {
            return Ok(p);
        }
        // Walk upward
        let mut cur = p.as_path();
        while let Some(parent) = cur.parent() {
            if parent.join(".git").exists() {
                return Ok(parent.to_path_buf());
            }
            cur = parent;
        }
    }
    // Try graph_path parent
    if let Some(parent) = state.graph_path.parent() {
        let mut cur = parent;
        loop {
            if cur.join(".git").exists() {
                return Ok(cur.to_path_buf());
            }
            match cur.parent() {
                Some(p) => cur = p,
                None => break,
            }
        }
    }
    Err(M1ndError::InvalidParams {
        tool: "L3-temporal".into(),
        detail: "Could not discover git repository root from ingest roots or graph path".into(),
    })
}

/// Convert depth string ("7d", "30d", "90d", "all") into a `--after` git date argument.
fn depth_to_after_arg(depth: &str) -> Option<String> {
    let trimmed = depth.trim().to_lowercase();
    if trimmed == "all" || trimmed.is_empty() {
        return None;
    }
    // Parse Nd format
    let numeric = trimmed.trim_end_matches('d');
    if let Ok(days) = numeric.parse::<u32>() {
        Some(format!("{} days ago", days))
    } else {
        // Try as a date string directly
        Some(trimmed)
    }
}

/// Parse the raw output of `git log --follow --format=%H|%ai|%an|%s --numstat`.
///
/// Format:
/// ```text
/// <hash>|<date>|<author>|<subject>
/// <added>\t<deleted>\t<path>
/// <added>\t<deleted>\t<path>
///
/// <hash>|<date>|<author>|<subject>
/// ...
/// ```
fn parse_git_log_output(raw: &str) -> Vec<GitCommitRecord> {
    let mut commits = Vec::new();
    let mut current: Option<GitCommitRecord> = None;

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Try to parse as commit header: hash|date|author|subject
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() == 4
            && parts[0].len() >= 7
            && parts[0].chars().all(|c| c.is_ascii_hexdigit())
        {
            // Save previous commit
            if let Some(c) = current.take() {
                commits.push(c);
            }
            current = Some(GitCommitRecord {
                hash: parts[0].to_string(),
                date: parts[1].to_string(),
                author: parts[2].to_string(),
                subject: parts[3].to_string(),
                files_changed: Vec::new(),
            });
            continue;
        }

        // Try to parse as numstat line: <added>\t<deleted>\t<path>
        let tab_parts: Vec<&str> = line.split('\t').collect();
        if tab_parts.len() >= 3 {
            if let Some(ref mut c) = current {
                let added = tab_parts[0].parse::<u32>().unwrap_or(0);
                let deleted = tab_parts[1].parse::<u32>().unwrap_or(0);
                let path = normalize_numstat_path(tab_parts[2]);
                c.files_changed.push(FileChurn {
                    path,
                    added,
                    deleted,
                });
            }
        }
    }

    // Don't forget the last commit
    if let Some(c) = current {
        commits.push(c);
    }

    commits
}

/// Normalize numstat paths — handle renames shown as `{old => new}/path`.
fn normalize_numstat_path(raw: &str) -> String {
    // Git rename format: "prefix/{old_name => new_name}/suffix"
    // or "old_name => new_name"
    if let Some(arrow_pos) = raw.find(" => ") {
        // Find enclosing braces
        if let Some(brace_start) = raw[..arrow_pos].rfind('{') {
            if let Some(brace_end) = raw[arrow_pos..].find('}') {
                let prefix = &raw[..brace_start];
                let new_part = &raw[arrow_pos + 4..arrow_pos + brace_end];
                let suffix = &raw[arrow_pos + brace_end + 1..];
                return format!("{}{}{}", prefix, new_part, suffix);
            }
        }
        // Simple "a => b" without braces — take the new name
        return raw[arrow_pos + 4..].trim().to_string();
    }
    raw.to_string()
}

/// Compute co-change partners from non-autosync commits.
/// co_changes(A,B) / max(changes(A), changes(B))
fn compute_co_change_partners(
    target_file: &str,
    non_autosync_commits: &[&GitCommitRecord],
    total_commit_count: usize,
    top_k: usize,
) -> Vec<layers::CoChangePartner> {
    let mut co_change_count: HashMap<String, u32> = HashMap::new();
    let mut file_commit_count: HashMap<String, u32> = HashMap::new();

    // Count co-occurrences
    for commit in non_autosync_commits {
        let has_target = commit.files_changed.iter().any(|f| f.path == target_file);
        if !has_target {
            continue;
        }
        for fc in &commit.files_changed {
            if fc.path != target_file {
                *co_change_count.entry(fc.path.clone()).or_insert(0) += 1;
            }
        }
    }

    // Count per-file commit frequencies
    for commit in non_autosync_commits {
        let mut seen = std::collections::HashSet::new();
        for fc in &commit.files_changed {
            if seen.insert(fc.path.clone()) {
                *file_commit_count.entry(fc.path.clone()).or_insert(0) += 1;
            }
        }
    }

    let target_count = *file_commit_count
        .get(target_file)
        .unwrap_or(&(total_commit_count as u32));

    let mut partners: Vec<layers::CoChangePartner> = co_change_count
        .iter()
        .map(|(file, &times)| {
            let other_count = *file_commit_count.get(file).unwrap_or(&1);
            let max_count = target_count.max(other_count).max(1);
            layers::CoChangePartner {
                file: file.clone(),
                times,
                coupling_degree: times as f32 / max_count as f32,
            }
        })
        .collect();

    // Sort by coupling_degree descending
    partners.sort_by(|a, b| {
        b.coupling_degree
            .partial_cmp(&a.coupling_degree)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    partners.truncate(top_k);
    partners
}

/// Compute velocity: "accelerating", "decelerating", or "stable".
/// Compare commit density of recent half vs older half.
fn compute_velocity(commits: &[GitCommitRecord]) -> String {
    if commits.len() < 4 {
        return "stable".into();
    }
    let mid = commits.len() / 2;
    // commits are newest-first from git log
    let recent_count = mid;
    let older_count = commits.len() - mid;

    if older_count == 0 {
        return "stable".into();
    }

    let ratio = recent_count as f32 / older_count as f32;
    if ratio > 1.5 {
        "accelerating".into()
    } else if ratio < 0.67 {
        "decelerating".into()
    } else {
        "stable".into()
    }
}

/// Determine churn pattern: expanding, shrinking, churning, dormant, stable.
fn compute_churn_pattern(
    total_added: u32,
    total_deleted: u32,
    commit_count: usize,
    velocity: &str,
) -> String {
    if commit_count == 0 {
        return "dormant".into();
    }
    if commit_count <= 2 && total_added + total_deleted < 20 {
        return "stable".into();
    }

    let net = total_added as i64 - total_deleted as i64;
    let total = (total_added + total_deleted).max(1);
    let net_ratio = net.unsigned_abs() as f32 / total as f32;

    if net > 0 && net_ratio > 0.3 {
        "expanding".into()
    } else if net < 0 && net_ratio > 0.3 {
        "shrinking".into()
    } else if commit_count > 10 && velocity == "accelerating" {
        "churning".into()
    } else {
        "stable".into()
    }
}

/// Collect all file-type nodes from the graph, optionally filtered by scope.
/// Returns HashMap<external_id, node_count_placeholder>.
fn collect_file_nodes(
    graph: &m1nd_core::graph::Graph,
    scope: Option<&str>,
) -> HashMap<String, u32> {
    let n = graph.num_nodes() as usize;
    let mut result = HashMap::new();

    for (interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx >= n {
            continue;
        }
        if graph.nodes.node_type[idx] != NodeType::File {
            continue;
        }
        let ext_id = graph.strings.resolve(*interned).to_string();
        if let Some(s) = scope {
            if !path_matches_scope(&ext_id, s) {
                continue;
            }
        }
        result.insert(ext_id, 1);
    }
    result
}

/// Check if a path/external_id matches a given scope prefix.
fn path_matches_scope(ext_id: &str, scope: &str) -> bool {
    let path = normalize_scope_path(Some(ext_id), &[]);
    let scope = normalize_scope_path(Some(scope), &[]);

    match (path, scope) {
        (Some(path), Some(scope)) => path.starts_with(&scope),
        _ => false,
    }
}

/// Resolve baseline file set for diverge analysis.
/// Returns (HashMap<external_id, count>, Option<baseline_commit_hash>).
fn resolve_baseline_files(
    repo_root: &Path,
    baseline: &str,
    graph_path: &Path,
    scope: Option<&str>,
) -> M1ndResult<(HashMap<String, u32>, Option<String>)> {
    if baseline == "last_session" {
        return resolve_last_session_baseline(graph_path, scope);
    }

    // Date-based baseline: find latest commit before the date
    let commit = resolve_baseline_commit(repo_root, baseline)?;
    match commit {
        None => Ok((HashMap::new(), None)),
        Some(ref hash) => {
            // Use git ls-tree to get the file list at that commit
            let output = Command::new("git")
                .current_dir(repo_root)
                .args(["ls-tree", "-r", "--name-only", hash])
                .output()
                .map_err(M1ndError::Io)?;

            if !output.status.success() {
                return Ok((HashMap::new(), Some(hash.clone())));
            }

            let raw = String::from_utf8_lossy(&output.stdout);
            let mut files = HashMap::new();
            for line in raw.lines() {
                let path = line.trim();
                if path.is_empty() {
                    continue;
                }
                let ext_id = format!("file::{}", path);
                if let Some(s) = scope {
                    if !path_matches_scope(&ext_id, s) {
                        continue;
                    }
                }
                files.insert(ext_id, 1);
            }
            Ok((files, Some(hash.clone())))
        }
    }
}

/// Load previous graph snapshot and extract file nodes as the baseline.
fn resolve_last_session_baseline(
    graph_path: &Path,
    scope: Option<&str>,
) -> M1ndResult<(HashMap<String, u32>, Option<String>)> {
    // Try to load the graph snapshot from disk
    if !graph_path.exists() {
        return Ok((HashMap::new(), None));
    }

    // Read the JSON snapshot directly and extract file nodes
    let raw = std::fs::read_to_string(graph_path).map_err(M1ndError::Io)?;
    let mut files = HashMap::new();

    // Parse minimally: look for external IDs starting with "file::"
    // The snapshot format has node entries with external IDs.
    // Simple approach: scan for "file::" patterns in the JSON.
    for line in raw.lines() {
        let line = line.trim();
        // Look for quoted strings containing "file::"
        if let Some(start) = line.find("\"file::") {
            if let Some(end) = line[start + 1..].find('"') {
                let ext_id = &line[start + 1..start + 1 + end];
                if let Some(s) = scope {
                    if !path_matches_scope(ext_id, s) {
                        continue;
                    }
                }
                files.insert(ext_id.to_string(), 1);
            }
        }
    }
    Ok((files, None))
}

/// Find the latest commit hash before the given date.
fn resolve_baseline_commit(repo_root: &Path, date_str: &str) -> M1ndResult<Option<String>> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args([
            "log",
            "-1",
            "--format=%H",
            &format!("--before={}", date_str),
        ])
        .output()
        .map_err(M1ndError::Io)?;

    if !output.status.success() {
        return Ok(None);
    }

    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if hash.is_empty() {
        Ok(None)
    } else {
        Ok(Some(hash))
    }
}

/// Compute modified nodes by comparing git diff between baseline date and HEAD.
fn compute_modified_nodes(
    repo_root: &Path,
    baseline_date: &str,
    scope: Option<&str>,
) -> Vec<layers::DivergeModifiedNode> {
    // Find baseline commit
    let baseline_commit = match resolve_baseline_commit(repo_root, baseline_date) {
        Ok(Some(h)) => h,
        _ => return vec![],
    };

    // Run git diff --numstat between baseline and HEAD
    let output = match Command::new("git")
        .current_dir(repo_root)
        .args(["diff", "--numstat", &baseline_commit, "HEAD"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();

    for line in raw.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let added: u32 = parts[0].parse().unwrap_or(0);
        let deleted: u32 = parts[1].parse().unwrap_or(0);
        let file = normalize_numstat_path(parts[2]);

        if let Some(s) = scope {
            if !path_matches_scope(&format!("file::{}", file), s) {
                continue;
            }
        }

        let total = (added + deleted).max(1);
        let growth_ratio = (added as f32 - deleted as f32) / total as f32;

        result.push(layers::DivergeModifiedNode {
            file,
            delta: format!("+{}/-{}", added, deleted),
            growth_ratio,
        });
    }

    result
}

/// Compute coupling changes between baseline and current by comparing
/// co-change frequency distributions.
fn compute_coupling_changes(
    repo_root: &Path,
    baseline_date: &str,
    scope: Option<&str>,
) -> Vec<layers::CouplingChange> {
    // Get commits before baseline and after baseline
    let baseline_commit = match resolve_baseline_commit(repo_root, baseline_date) {
        Ok(Some(h)) => h,
        _ => return vec![],
    };

    // Build coupling map for pre-baseline
    let pre_commits = get_commits_in_range(repo_root, None, Some(&baseline_commit));
    let pre_coupling = build_coupling_map(&pre_commits, scope);

    // Build coupling map for post-baseline
    let post_commits = get_commits_in_range(repo_root, Some(&baseline_commit), None);
    let post_coupling = build_coupling_map(&post_commits, scope);

    // Find significant changes
    let mut changes = Vec::new();
    let mut seen_pairs = std::collections::HashSet::new();

    for (pair, &was) in &pre_coupling {
        if seen_pairs.insert(pair.clone()) {
            let now = *post_coupling.get(pair).unwrap_or(&0.0);
            let diff = (now - was).abs();
            if diff > 0.15 {
                let direction = if now > was {
                    "strengthened"
                } else {
                    "weakened"
                };
                changes.push(layers::CouplingChange {
                    pair: pair.clone(),
                    was,
                    now,
                    direction: direction.into(),
                });
            }
        }
    }

    for (pair, &now) in &post_coupling {
        if seen_pairs.insert(pair.clone()) {
            let was = *pre_coupling.get(pair).unwrap_or(&0.0);
            let diff = (now - was).abs();
            if diff > 0.15 {
                changes.push(layers::CouplingChange {
                    pair: pair.clone(),
                    was,
                    now,
                    direction: "new_coupling".into(),
                });
            }
        }
    }

    // Sort by absolute change descending
    changes.sort_by(|a, b| {
        let da = (a.now - a.was).abs();
        let db = (b.now - b.was).abs();
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });
    changes.truncate(20);
    changes
}

/// Get commit hashes in a range. Both bounds are optional.
fn get_commits_in_range(
    repo_root: &Path,
    after_commit: Option<&str>,
    before_commit: Option<&str>,
) -> Vec<GitCommitRecord> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_root);
    cmd.args([
        "log",
        "--format=%H|%ai|%an|%s",
        "--numstat",
        "--max-count=300",
    ]);

    // Build revision range
    match (after_commit, before_commit) {
        (Some(after), Some(before)) => {
            cmd.arg(format!("{}..{}", after, before));
        }
        (Some(after), None) => {
            cmd.arg(format!("{}..HEAD", after));
        }
        (None, Some(before)) => {
            cmd.arg(before);
        }
        (None, None) => {
            cmd.arg("HEAD");
        }
    }

    let output = match cmd.output() {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let raw = String::from_utf8_lossy(&output.stdout);
    let all = parse_git_log_output(&raw);
    // Filter out auto-sync commits for coupling analysis
    all.into_iter()
        .filter(|c| !is_auto_sync_commit(&c.subject))
        .collect()
}

/// Build a coupling map: { [file_a, file_b] -> coupling_degree }.
/// coupling = co_changes(A,B) / max(changes(A), changes(B))
fn build_coupling_map(
    commits: &[GitCommitRecord],
    scope: Option<&str>,
) -> HashMap<[String; 2], f32> {
    let mut co_change: HashMap<[String; 2], u32> = HashMap::new();
    let mut file_count: HashMap<String, u32> = HashMap::new();

    for commit in commits {
        let files: Vec<&str> = commit
            .files_changed
            .iter()
            .map(|f| f.path.as_str())
            .filter(|p| scope.is_none_or(|s| path_matches_scope(&format!("file::{}", p), s)))
            .collect();

        let mut seen = std::collections::HashSet::new();
        for &f in &files {
            if seen.insert(f) {
                *file_count.entry(f.to_string()).or_insert(0) += 1;
            }
        }

        // Pairwise co-change (sorted to avoid duplicates)
        for i in 0..files.len() {
            for j in (i + 1)..files.len() {
                let mut pair = [files[i].to_string(), files[j].to_string()];
                pair.sort();
                *co_change.entry(pair).or_insert(0) += 1;
            }
        }
    }

    let mut result = HashMap::new();
    for (pair, count) in co_change {
        let ca = *file_count.get(&pair[0]).unwrap_or(&1);
        let cb = *file_count.get(&pair[1]).unwrap_or(&1);
        let max_c = ca.max(cb).max(1);
        result.insert(pair, count as f32 / max_c as f32);
    }
    result
}

/// Detect structural anomalies: test_deficit, velocity_spike, etc.
fn detect_anomalies(
    new_nodes: &[String],
    _removed_nodes: &[String],
    modified_nodes: &[layers::DivergeModifiedNode],
    current_files: &HashMap<String, u32>,
) -> Vec<layers::DivergeAnomaly> {
    let mut anomalies = Vec::new();

    // --- test_deficit: new/modified code files without corresponding test growth ---
    let code_new: Vec<&str> = new_nodes
        .iter()
        .filter(|n| is_code_file(n) && !is_test_file(n))
        .map(|n| n.as_str())
        .collect();

    let test_new: Vec<&str> = new_nodes
        .iter()
        .filter(|n| is_test_file(n))
        .map(|n| n.as_str())
        .collect();

    if code_new.len() > 3 && test_new.is_empty() {
        anomalies.push(layers::DivergeAnomaly {
            anomaly_type: "test_deficit".into(),
            file: format!("{} new code files", code_new.len()),
            detail: format!(
                "{} new code files added but 0 new test files. Top: {}",
                code_new.len(),
                code_new
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            severity: "warning".into(),
        });
    }

    // --- velocity_spike: files with extreme growth ratio ---
    for m in modified_nodes {
        if m.growth_ratio > 3.0 {
            anomalies.push(layers::DivergeAnomaly {
                anomaly_type: "velocity_spike".into(),
                file: m.file.clone(),
                detail: format!(
                    "Growth ratio {:.1}x ({}) — possible scope explosion",
                    m.growth_ratio, m.delta
                ),
                severity: if m.growth_ratio > 5.0 {
                    "critical"
                } else {
                    "warning"
                }
                .into(),
            });
        }
    }

    // --- orphan_tests: test files without corresponding code file ---
    let code_files: std::collections::HashSet<&str> = current_files
        .keys()
        .filter(|k| is_code_file(k) && !is_test_file(k))
        .map(|k| k.as_str())
        .collect();

    for test_node in new_nodes.iter().filter(|n| is_test_file(n)) {
        let test_path = node_to_file_path(test_node);
        // Try to find the corresponding source file
        let expected_source = test_path
            .replace("tests/", "")
            .replace("test_", "")
            .replace(".test.", ".");
        let has_source = code_files.iter().any(|c| {
            let cp = node_to_file_path(c);
            cp.ends_with(&expected_source)
        });
        if !has_source {
            anomalies.push(layers::DivergeAnomaly {
                anomaly_type: "orphan_test".into(),
                file: test_node.clone(),
                detail: "Test file added with no matching source file".into(),
                severity: "info".into(),
            });
        }
    }

    anomalies
}

/// Check if a file path looks like a code file (not config, not docs).
fn is_code_file(path: &str) -> bool {
    let p = node_to_file_path(path);
    p.ends_with(".py")
        || p.ends_with(".rs")
        || p.ends_with(".ts")
        || p.ends_with(".tsx")
        || p.ends_with(".js")
        || p.ends_with(".jsx")
}

/// Check if a file path looks like a test file.
fn is_test_file(path: &str) -> bool {
    let p = node_to_file_path(path);
    p.contains("/test_")
        || p.contains("/tests/")
        || p.contains(".test.")
        || p.contains(".spec.")
        || p.contains("_test.rs")
}

// =========================================================================
// L4: Investigation Memory — m1nd.trail.*
// =========================================================================

// ---------------------------------------------------------------------------
// L4 internal data model — TrailData (persisted as JSON on disk)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TrailHypothesis {
    statement: String,
    confidence: f32,
    supporting_nodes: Vec<String>,
    contradicting_nodes: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TrailConclusion {
    statement: String,
    confidence: f32,
    from_hypotheses: Vec<String>,
    supporting_nodes: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TrailVisitedNode {
    node_external_id: String,
    annotation: Option<String>,
    relevance: f32,
}

/// Internal trail data persisted as JSON. NOT a protocol type.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TrailData {
    trail_id: String,
    label: String,
    agent_id: String,
    /// "active" | "saved" | "archived" | "stale" | "merged"
    status: String,
    visited_nodes: Vec<TrailVisitedNode>,
    hypotheses: Vec<TrailHypothesis>,
    conclusions: Vec<TrailConclusion>,
    open_questions: Vec<String>,
    tags: Vec<String>,
    summary: Option<String>,
    /// node_external_id -> boost weight [0.0, 1.0]
    activation_boosts: HashMap<String, f32>,
    /// Graph generation at time of save.
    graph_generation: u64,
    created_at_ms: u64,
    last_modified_ms: u64,
    /// If this trail was produced by merging others, record source trail IDs.
    #[serde(default)]
    source_trails: Vec<String>,
}

// ---------------------------------------------------------------------------
// L4 helpers
// ---------------------------------------------------------------------------

fn trail_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn trail_upsert_visited_node(
    visited_nodes: &mut Vec<TrailVisitedNode>,
    node_external_id: String,
    annotation: Option<String>,
    relevance: f32,
) {
    let normalized_relevance = relevance.clamp(0.0, 1.0);
    if let Some(existing) = visited_nodes
        .iter_mut()
        .find(|node| node.node_external_id == node_external_id)
    {
        existing.relevance = existing.relevance.max(normalized_relevance);
        if existing.annotation.is_none() {
            existing.annotation = annotation;
        }
        return;
    }

    visited_nodes.push(TrailVisitedNode {
        node_external_id,
        annotation,
        relevance: normalized_relevance,
    });
}

fn trail_seed_boost(boosts: &mut HashMap<String, f32>, node_external_id: &str, weight: f32) {
    let normalized = weight.clamp(0.0, 1.0);
    boosts
        .entry(node_external_id.to_string())
        .and_modify(|current| *current = current.max(normalized))
        .or_insert(normalized);
}

fn trail_resume_hints(
    trail: &TrailData,
    strongest_nodes: &[String],
    next_focus_node_id: Option<&String>,
    next_open_question: Option<&String>,
    next_suggested_tool: Option<&str>,
    max_hints: usize,
) -> Vec<String> {
    if max_hints == 0 {
        return Vec::new();
    }

    let mut hints = Vec::new();

    match (next_suggested_tool, next_focus_node_id, next_open_question) {
        (Some("timeline"), Some(node), Some(question)) => hints.push(format!(
            "Use timeline on {} to answer the carried-forward question: {}",
            node, question
        )),
        (Some("impact"), Some(node), Some(question)) => hints.push(format!(
            "Use impact on {} before changing it: {}",
            node, question
        )),
        (Some("hypothesize"), _, Some(question)) => hints.push(format!(
            "Use hypothesize to test the carried-forward structural claim: {}",
            question
        )),
        (Some("seek"), _, Some(question)) => hints.push(format!(
            "Use seek to relocate the answer path for: {}",
            question
        )),
        (Some("view"), Some(node), _) => hints.push(format!(
            "Re-open the current focus before branching: {}",
            node
        )),
        _ => {}
    }

    for question in trail.open_questions.iter().take(max_hints.min(2)) {
        let hint = format!("Continue with open question: {}", question);
        if !hints.iter().any(|existing| existing == &hint) {
            hints.push(hint);
        }
    }

    for node in strongest_nodes
        .iter()
        .take(max_hints.saturating_sub(hints.len()).min(2))
    {
        let hint = format!("Inspect neighborhood around {}", node);
        if !hints.iter().any(|existing| existing == &hint) {
            hints.push(hint);
        }
    }

    if hints.len() < max_hints && !trail.hypotheses.is_empty() {
        let hint = format!("Re-test hypothesis: {}", trail.hypotheses[0].statement);
        if !hints.iter().any(|existing| existing == &hint) {
            hints.push(hint);
        }
    }

    hints.truncate(max_hints);
    hints
}

fn trail_resume_suggested_tool(
    next_focus_node_id: Option<&String>,
    next_open_question: Option<&String>,
) -> Option<String> {
    if let Some(question) = next_open_question {
        let lower = question.to_lowercase();
        if ["changed", "change", "last", "history", "recent", "commit"]
            .iter()
            .any(|term| lower.contains(term))
            && next_focus_node_id.is_some()
        {
            return Some("timeline".into());
        }
        if next_focus_node_id.is_some()
            && ["impact", "blast", "break", "affected", "touch"]
                .iter()
                .any(|term| lower.contains(term))
        {
            return Some("impact".into());
        }
        if [
            "why",
            "proof",
            "prove",
            "evidence",
            "violation",
            "missing",
            "guard",
        ]
        .iter()
        .any(|term| lower.contains(term))
        {
            return Some("hypothesize".into());
        }
        if [
            "where",
            "which",
            "owner",
            "helper",
            "normalize",
            "canonical",
            "dispatch",
            "route",
        ]
        .iter()
        .any(|term| lower.contains(term))
        {
            return Some("seek".into());
        }
    }
    if next_focus_node_id.is_some() {
        return Some("view".into());
    }
    if next_open_question.is_some() {
        return Some("search".into());
    }
    None
}

/// Resolve the trails/ directory path from the graph snapshot path.
/// Creates the directory if it does not exist.
fn trails_dir(state: &SessionState) -> M1ndResult<PathBuf> {
    let dir = state
        .graph_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("trails");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Load a single trail from disk by ID.
fn load_trail(state: &SessionState, trail_id: &str) -> M1ndResult<TrailData> {
    let dir = trails_dir(state)?;
    let path = dir.join(format!("{}.json", trail_id));
    let data = std::fs::read_to_string(&path)?;
    let trail: TrailData = serde_json::from_str(&data)?;
    Ok(trail)
}

/// Save a trail to disk (atomic: write tmp then rename).
fn save_trail(state: &SessionState, trail: &TrailData) -> M1ndResult<()> {
    let dir = trails_dir(state)?;
    let path = dir.join(format!("{}.json", trail.trail_id));
    let tmp_path = dir.join(format!(".{}.json.tmp", trail.trail_id));
    let json = serde_json::to_string_pretty(trail)?;
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// List all trail files in the trails/ directory. Skips corrupt files silently.
fn list_all_trails(state: &SessionState) -> M1ndResult<Vec<TrailData>> {
    let dir = trails_dir(state)?;
    let mut trails = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(trails),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        match serde_json::from_str::<TrailData>(&data) {
            Ok(t) => trails.push(t),
            Err(_) => continue,
        }
    }
    Ok(trails)
}

/// Build a TrailSummaryOutput from a TrailData.
fn trail_to_summary(trail: &TrailData) -> layers::TrailSummaryOutput {
    layers::TrailSummaryOutput {
        trail_id: trail.trail_id.clone(),
        agent_id: trail.agent_id.clone(),
        label: trail.label.clone(),
        status: trail.status.clone(),
        created_at_ms: trail.created_at_ms,
        last_modified_ms: trail.last_modified_ms,
        node_count: trail.visited_nodes.len(),
        hypothesis_count: trail.hypotheses.len(),
        conclusion_count: trail.conclusions.len(),
        open_question_count: trail.open_questions.len(),
        tags: trail.tags.clone(),
        summary: trail.summary.clone(),
    }
}

/// Generate a short hash from a string (first 8 hex chars of FNV-1a).
fn trail_short_hash(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:08x}", hash & 0xFFFFFFFF)
}

// ---------------------------------------------------------------------------
// L4 handlers
// ---------------------------------------------------------------------------

/// Handle m1nd.trail.save — persist the current investigation state.
/// JSON files under `{graph_dir}/trails/`. Node refs use external_id.
/// Auto-captures visited nodes from active perspective state if none provided.
pub fn handle_trail_save(
    state: &mut SessionState,
    input: layers::TrailSaveInput,
) -> M1ndResult<layers::TrailSaveOutput> {
    let ts = trail_now_ms();

    // Generate trail_id: trail_{agent_id}_{counter}_{short_hash}
    let existing = list_all_trails(state)?;
    let agent_count = existing
        .iter()
        .filter(|t| t.agent_id == input.agent_id)
        .count();
    let counter = agent_count + 1;
    let hash_input = format!("{}:{}:{}", input.agent_id, input.label, ts);
    let trail_id = format!(
        "trail_{}_{:03}_{}",
        input.agent_id,
        counter,
        trail_short_hash(&hash_input)
    );

    // Collect visited nodes — from input or auto-capture from perspective state.
    let mut visited_nodes: Vec<TrailVisitedNode> = Vec::new();
    for node in &input.visited_nodes {
        trail_upsert_visited_node(
            &mut visited_nodes,
            node.node_external_id.clone(),
            node.annotation.clone(),
            node.relevance,
        );
    }

    // If no explicit visited nodes, capture from active perspectives for this agent.
    if visited_nodes.is_empty() {
        for ((agent, _persp_id), persp) in &state.perspectives {
            if agent == &input.agent_id && !persp.visited_nodes.is_empty() {
                for ext_id in &persp.visited_nodes {
                    trail_upsert_visited_node(&mut visited_nodes, ext_id.clone(), None, 0.5);
                }
            }
        }
    }

    for hypothesis in &input.hypotheses {
        for node in &hypothesis.supporting_nodes {
            trail_upsert_visited_node(
                &mut visited_nodes,
                node.clone(),
                Some("hypothesis support".into()),
                hypothesis.confidence.max(0.6),
            );
        }
        for node in &hypothesis.contradicting_nodes {
            trail_upsert_visited_node(
                &mut visited_nodes,
                node.clone(),
                Some("hypothesis contradiction".into()),
                (hypothesis.confidence * 0.8).max(0.45),
            );
        }
    }

    for conclusion in &input.conclusions {
        for node in &conclusion.supporting_nodes {
            trail_upsert_visited_node(
                &mut visited_nodes,
                node.clone(),
                Some("conclusion support".into()),
                conclusion.confidence.max(0.7),
            );
        }
    }

    let hypotheses: Vec<TrailHypothesis> = input
        .hypotheses
        .iter()
        .map(|h| TrailHypothesis {
            statement: h.statement.clone(),
            confidence: h.confidence,
            supporting_nodes: h.supporting_nodes.clone(),
            contradicting_nodes: h.contradicting_nodes.clone(),
        })
        .collect();

    let conclusions: Vec<TrailConclusion> = input
        .conclusions
        .iter()
        .map(|c| TrailConclusion {
            statement: c.statement.clone(),
            confidence: c.confidence,
            from_hypotheses: c.from_hypotheses.clone(),
            supporting_nodes: c.supporting_nodes.clone(),
        })
        .collect();

    let summary = input.summary.or_else(|| {
        Some(format!(
            "{}: {} nodes, {} hypotheses, {} conclusions, {} open questions",
            input.label,
            visited_nodes.len(),
            hypotheses.len(),
            conclusions.len(),
            input.open_questions.len()
        ))
    });

    let graph_gen = state.graph_generation;

    let mut activation_boosts = input.activation_boosts.clone();
    for node in &visited_nodes {
        trail_seed_boost(
            &mut activation_boosts,
            &node.node_external_id,
            0.2 + node.relevance.clamp(0.0, 1.0) * 0.5,
        );
    }
    for hypothesis in &hypotheses {
        for node in &hypothesis.supporting_nodes {
            trail_seed_boost(&mut activation_boosts, node, 0.7);
        }
        for node in &hypothesis.contradicting_nodes {
            trail_seed_boost(&mut activation_boosts, node, 0.45);
        }
    }
    for conclusion in &conclusions {
        for node in &conclusion.supporting_nodes {
            trail_seed_boost(&mut activation_boosts, node, 0.8);
        }
    }

    let trail = TrailData {
        trail_id: trail_id.clone(),
        label: input.label.clone(),
        agent_id: input.agent_id.clone(),
        status: "saved".to_string(),
        visited_nodes,
        hypotheses,
        conclusions,
        open_questions: input.open_questions.clone(),
        tags: input.tags.clone(),
        summary,
        activation_boosts,
        graph_generation: graph_gen,
        created_at_ms: ts,
        last_modified_ms: ts,
        source_trails: Vec::new(),
    };

    let nodes_saved = trail.visited_nodes.len();
    let hypotheses_saved = trail.hypotheses.len();
    let conclusions_saved = trail.conclusions.len();
    let open_questions_saved = trail.open_questions.len();

    save_trail(state, &trail)?;

    Ok(layers::TrailSaveOutput {
        trail_id,
        label: input.label,
        agent_id: input.agent_id,
        nodes_saved,
        hypotheses_saved,
        conclusions_saved,
        open_questions_saved,
        graph_generation_at_creation: graph_gen,
        created_at_ms: ts,
    })
}

/// Handle m1nd.trail.resume — restore a saved investigation.
/// Validates node existence, detects staleness, re-injects activation boosts.
/// >50% missing nodes + !force = error. Hypotheses with missing support are downgraded.
pub fn handle_trail_resume(
    state: &mut SessionState,
    input: layers::TrailResumeInput,
) -> M1ndResult<layers::TrailResumeOutput> {
    let start = Instant::now();
    let reactivated_limit = input.max_reactivated_nodes.clamp(0, 10);
    let hint_limit = input.max_resume_hints.clamp(0, 8);

    let mut trail = load_trail(state, &input.trail_id)?;

    let current_gen = state.graph_generation;
    let trail_gen = trail.graph_generation;
    let generations_behind = current_gen.saturating_sub(trail_gen);
    let stale = generations_behind > 0;

    // Validate node existence in current graph
    let mut missing_nodes: Vec<String> = Vec::new();
    let mut resolved_count: usize = 0;
    {
        let graph = state.graph.read();
        for vn in &trail.visited_nodes {
            if graph.resolve_id(&vn.node_external_id).is_some() {
                resolved_count += 1;
            } else {
                missing_nodes.push(vn.node_external_id.clone());
            }
        }
    }

    let total_nodes = trail.visited_nodes.len();
    let missing_ratio = if total_nodes > 0 {
        missing_nodes.len() as f64 / total_nodes as f64
    } else {
        0.0
    };

    if stale && missing_ratio > 0.5 && !input.force {
        return Err(M1ndError::InvalidParams {
            tool: "trail.resume".into(),
            detail: format!(
                "Trail {} is stale: {} of {} nodes missing ({:.0}%). Use force=true to resume.",
                input.trail_id,
                missing_nodes.len(),
                total_nodes,
                missing_ratio * 100.0
            ),
        });
    }

    // Re-inject activation boosts for nodes that still exist.
    let mut nodes_reactivated: usize = 0;
    let mut reactivated_nodes: Vec<(String, f32)> = Vec::new();
    if !trail.activation_boosts.is_empty() {
        let mut graph = state.graph.write();
        let n = graph.num_nodes() as usize;
        for (ext_id, &boost) in &trail.activation_boosts {
            if let Some(node_id) = graph.resolve_id(ext_id) {
                let idx = node_id.as_usize();
                if idx < n {
                    // Boost structural activation dimension (index 0)
                    let current = graph.nodes.activation[idx][0].get();
                    let new_val = (current + boost).min(1.0);
                    graph.nodes.activation[idx][0] = FiniteF32::new(new_val);
                    nodes_reactivated += 1;
                    reactivated_nodes.push((ext_id.clone(), boost));
                }
            }
        }
    } else {
        nodes_reactivated = resolved_count;
        for vn in &trail.visited_nodes {
            if !missing_nodes
                .iter()
                .any(|missing| missing == &vn.node_external_id)
            {
                reactivated_nodes.push((vn.node_external_id.clone(), vn.relevance));
            }
        }
    }

    reactivated_nodes.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    let reactivated_node_ids: Vec<String> = reactivated_nodes
        .iter()
        .map(|(node_id, _)| node_id.clone())
        .take(reactivated_limit)
        .collect();

    // Check hypotheses — downgrade those whose supporting nodes are mostly missing
    let mut hypotheses_downgraded: Vec<String> = Vec::new();
    {
        let graph = state.graph.read();
        for hyp in &trail.hypotheses {
            if hyp.supporting_nodes.is_empty() {
                continue;
            }
            let missing_support = hyp
                .supporting_nodes
                .iter()
                .filter(|n| graph.resolve_id(n).is_none())
                .count();
            let ratio = missing_support as f64 / hyp.supporting_nodes.len() as f64;
            if ratio > 0.5 {
                hypotheses_downgraded.push(hyp.statement.clone());
            }
        }
    }

    trail.status = if stale {
        "stale".to_string()
    } else {
        "active".to_string()
    };
    trail.last_modified_ms = trail_now_ms();
    save_trail(state, &trail)?;

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let next_focus_node_id = reactivated_node_ids.first().cloned();
    let next_open_question = trail.open_questions.first().cloned();
    let next_suggested_tool =
        trail_resume_suggested_tool(next_focus_node_id.as_ref(), next_open_question.as_ref());
    let resume_hints = trail_resume_hints(
        &trail,
        &reactivated_node_ids,
        next_focus_node_id.as_ref(),
        next_open_question.as_ref(),
        next_suggested_tool.as_deref(),
        hint_limit,
    );

    Ok(layers::TrailResumeOutput {
        trail_id: trail.trail_id.clone(),
        label: trail.label.clone(),
        stale,
        generations_behind,
        missing_nodes,
        nodes_reactivated,
        reactivated_node_ids,
        hypotheses_downgraded,
        next_focus_node_id,
        next_open_question,
        next_suggested_tool,
        resume_hints,
        trail: trail_to_summary(&trail),
        elapsed_ms,
    })
}

/// Handle m1nd.trail.merge — combine two or more investigation trails.
/// Union visited nodes (max relevance for duplicates).
/// Hypothesis conflicts detected via shared supporting nodes + confidence delta.
/// Shared nodes = connections. Bridge edges found via CSR traversal.
/// Source trails marked as status="merged" after merge.
pub fn handle_trail_merge(
    state: &mut SessionState,
    input: layers::TrailMergeInput,
) -> M1ndResult<layers::TrailMergeOutput> {
    let start = Instant::now();

    if input.trail_ids.len() < 2 {
        return Err(M1ndError::InvalidParams {
            tool: "trail.merge".into(),
            detail: "Trail merge requires at least 2 trail IDs".into(),
        });
    }

    let mut source_trails: Vec<TrailData> = Vec::with_capacity(input.trail_ids.len());
    for tid in &input.trail_ids {
        source_trails.push(load_trail(state, tid)?);
    }

    // --- Union visited nodes (max relevance for duplicates) ---
    let mut node_map: HashMap<String, TrailVisitedNode> = HashMap::new();
    for trail in &source_trails {
        for vn in &trail.visited_nodes {
            let entry = node_map
                .entry(vn.node_external_id.clone())
                .or_insert_with(|| vn.clone());
            if vn.relevance > entry.relevance {
                entry.relevance = vn.relevance;
                entry.annotation = vn.annotation.clone();
            }
        }
    }
    let merged_visited: Vec<TrailVisitedNode> = node_map.into_values().collect();

    // --- Merge hypotheses with conflict detection ---
    let mut all_hypotheses: Vec<TrailHypothesis> = Vec::new();
    let mut conflicts: Vec<layers::TrailMergeConflict> = Vec::new();

    let hyp_by_trail: Vec<Vec<&TrailHypothesis>> = source_trails
        .iter()
        .map(|t| t.hypotheses.iter().collect())
        .collect();

    for i in 0..source_trails.len() {
        for j in (i + 1)..source_trails.len() {
            for ha in &hyp_by_trail[i] {
                for hb in &hyp_by_trail[j] {
                    let shared: usize = ha
                        .supporting_nodes
                        .iter()
                        .filter(|n| hb.supporting_nodes.contains(n))
                        .count();
                    let max_support = ha.supporting_nodes.len().max(hb.supporting_nodes.len());
                    if max_support == 0 || shared == 0 {
                        continue;
                    }
                    let overlap = shared as f32 / max_support as f32;
                    if overlap < 0.3 {
                        continue;
                    }

                    let score_delta = (ha.confidence - hb.confidence).abs();
                    if score_delta < 0.2 {
                        conflicts.push(layers::TrailMergeConflict {
                            hypothesis_a: ha.statement.clone(),
                            hypothesis_b: hb.statement.clone(),
                            resolution: "unresolved".to_string(),
                            winner: None,
                            score_delta,
                        });
                    } else {
                        let winner = if ha.confidence > hb.confidence {
                            ha.statement.clone()
                        } else {
                            hb.statement.clone()
                        };
                        conflicts.push(layers::TrailMergeConflict {
                            hypothesis_a: ha.statement.clone(),
                            hypothesis_b: hb.statement.clone(),
                            resolution: "resolved".to_string(),
                            winner: Some(winner),
                            score_delta,
                        });
                    }
                }
            }
        }
    }

    for trail in &source_trails {
        for h in &trail.hypotheses {
            all_hypotheses.push(h.clone());
        }
    }

    // --- Merge conclusions ---
    let mut all_conclusions: Vec<TrailConclusion> = Vec::new();
    for trail in &source_trails {
        for c in &trail.conclusions {
            all_conclusions.push(c.clone());
        }
    }

    // --- Merge open questions (deduplicated) ---
    let mut all_questions: Vec<String> = Vec::new();
    for trail in &source_trails {
        for q in &trail.open_questions {
            if !all_questions.contains(q) {
                all_questions.push(q.clone());
            }
        }
    }

    // --- Merge tags (deduplicated) ---
    let mut all_tags: Vec<String> = Vec::new();
    for trail in &source_trails {
        for tag in &trail.tags {
            if !all_tags.contains(tag) {
                all_tags.push(tag.clone());
            }
        }
    }

    // --- Merge activation boosts (max for duplicates) ---
    let mut merged_boosts: HashMap<String, f32> = HashMap::new();
    for trail in &source_trails {
        for (k, &v) in &trail.activation_boosts {
            let entry = merged_boosts.entry(k.clone()).or_insert(0.0);
            if v > *entry {
                *entry = v;
            }
        }
    }

    // --- Discover connections between trails ---
    let mut connections: Vec<layers::TrailConnection> = Vec::new();

    // 1. Shared nodes
    let mut node_trail_index: HashMap<String, Vec<usize>> = HashMap::new();
    for (trail_idx, trail) in source_trails.iter().enumerate() {
        for vn in &trail.visited_nodes {
            node_trail_index
                .entry(vn.node_external_id.clone())
                .or_default()
                .push(trail_idx);
        }
    }
    for (ext_id, trail_indices) in &node_trail_index {
        if trail_indices.len() > 1 {
            let trail_labels: Vec<String> = trail_indices
                .iter()
                .map(|&idx| source_trails[idx].label.clone())
                .collect();
            connections.push(layers::TrailConnection {
                connection_type: "shared_node".to_string(),
                detail: format!(
                    "Node {} appears in trails: {}",
                    ext_id,
                    trail_labels.join(", ")
                ),
                from_node: Some(ext_id.clone()),
                to_node: None,
                weight: Some(trail_indices.len() as f32 / source_trails.len() as f32),
            });
        }
    }

    // 2. Bridge edges via graph CSR
    {
        let graph = state.graph.read();
        let n = graph.num_nodes() as usize;
        let mut node_ext_id = vec![String::new(); n];
        for (&interned, &node_id) in &graph.id_to_node {
            if let Some(s) = graph.strings.try_resolve(interned) {
                if node_id.as_usize() < n {
                    node_ext_id[node_id.as_usize()] = s.to_string();
                }
            }
        }

        for i in 0..source_trails.len() {
            for j in (i + 1)..source_trails.len() {
                let nodes_a: std::collections::HashSet<String> = source_trails[i]
                    .visited_nodes
                    .iter()
                    .map(|v| v.node_external_id.clone())
                    .collect();
                let nodes_b: std::collections::HashSet<String> = source_trails[j]
                    .visited_nodes
                    .iter()
                    .map(|v| v.node_external_id.clone())
                    .collect();

                for ext_a in &nodes_a {
                    if nodes_b.contains(ext_a) {
                        continue;
                    }
                    if let Some(node_a) = graph.resolve_id(ext_a) {
                        let range = graph.csr.out_range(node_a);
                        for k in range {
                            let target = graph.csr.targets[k];
                            let tgt_idx = target.as_usize();
                            if tgt_idx >= n {
                                continue;
                            }
                            let tgt_ext = &node_ext_id[tgt_idx];
                            if !tgt_ext.is_empty() && nodes_b.contains(tgt_ext) {
                                let rel = graph
                                    .strings
                                    .try_resolve(graph.csr.relations[k])
                                    .unwrap_or("edge");
                                connections.push(layers::TrailConnection {
                                    connection_type: "bridge_edge".to_string(),
                                    detail: format!(
                                        "{} --[{}]--> {} ({} -> {})",
                                        ext_a,
                                        rel,
                                        tgt_ext,
                                        source_trails[i].label,
                                        source_trails[j].label
                                    ),
                                    from_node: Some(ext_a.clone()),
                                    to_node: Some(tgt_ext.clone()),
                                    weight: Some(
                                        graph.csr.read_weight(EdgeIdx::new(k as u32)).get(),
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // --- Create the merged trail ---
    let ts = trail_now_ms();
    let merged_label = input.label.unwrap_or_else(|| {
        let labels: Vec<&str> = source_trails.iter().map(|t| t.label.as_str()).collect();
        format!("Merged: {}", labels.join(" + "))
    });
    let hash_input = format!("merge:{}:{}", input.trail_ids.join("+"), ts);
    let existing_count = list_all_trails(state)?
        .iter()
        .filter(|t| t.agent_id == input.agent_id)
        .count();
    let merged_trail_id = format!(
        "trail_{}_{:03}_{}",
        input.agent_id,
        existing_count + 1,
        trail_short_hash(&hash_input)
    );

    let merged_trail = TrailData {
        trail_id: merged_trail_id.clone(),
        label: merged_label.clone(),
        agent_id: input.agent_id.clone(),
        status: "saved".to_string(),
        visited_nodes: merged_visited,
        hypotheses: all_hypotheses,
        conclusions: all_conclusions,
        open_questions: all_questions,
        tags: all_tags,
        summary: Some(format!(
            "Merged from {} trails. {} connections discovered, {} conflicts.",
            source_trails.len(),
            connections.len(),
            conflicts.len()
        )),
        activation_boosts: merged_boosts,
        graph_generation: state.graph_generation,
        created_at_ms: ts,
        last_modified_ms: ts,
        source_trails: input.trail_ids.clone(),
    };

    let nodes_merged = merged_trail.visited_nodes.len();
    let hypotheses_merged = merged_trail.hypotheses.len();

    save_trail(state, &merged_trail)?;

    // Mark source trails as "merged"
    for tid in &input.trail_ids {
        if let Ok(mut src) = load_trail(state, tid) {
            src.status = "merged".to_string();
            src.last_modified_ms = ts;
            let _ = save_trail(state, &src);
        }
    }

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    Ok(layers::TrailMergeOutput {
        merged_trail_id,
        label: merged_label,
        source_trails: input.trail_ids,
        nodes_merged,
        hypotheses_merged,
        conflicts,
        connections_discovered: connections,
        elapsed_ms,
    })
}

/// Handle m1nd.trail.list — list trails matching filters.
/// Reads all JSON from `{graph_dir}/trails/`. Skips corrupt files.
/// Filters by agent_id, status, tags (any match). Sorted by last_modified desc.
pub fn handle_trail_list(
    state: &mut SessionState,
    input: layers::TrailListInput,
) -> M1ndResult<layers::TrailListOutput> {
    let all_trails = list_all_trails(state)?;

    let mut filtered: Vec<&TrailData> = all_trails.iter().collect();

    if let Some(ref filter_agent) = input.filter_agent_id {
        filtered.retain(|t| &t.agent_id == filter_agent);
    }

    if let Some(ref filter_status) = input.filter_status {
        filtered.retain(|t| &t.status == filter_status);
    }

    if !input.filter_tags.is_empty() {
        filtered.retain(|t| input.filter_tags.iter().any(|tag| t.tags.contains(tag)));
    }

    filtered.sort_by(|a, b| b.last_modified_ms.cmp(&a.last_modified_ms));

    let total_count = filtered.len();
    let trails: Vec<layers::TrailSummaryOutput> =
        filtered.iter().map(|t| trail_to_summary(t)).collect();

    Ok(layers::TrailListOutput {
        trails,
        total_count,
    })
}

// =========================================================================
// L5: Hypothesis Engine — m1nd.hypothesize + m1nd.differential
// =========================================================================

/// Handle m1nd.hypothesize — test a structural claim about the codebase.
/// Parses natural language claims into graph queries, searches for
/// supporting and contradicting evidence using budget-capped BFS.
///
/// 8 claim patterns via regex templates:
///   NEVER_CALLS, ALWAYS_BEFORE, DEPENDS_ON, NO_DEPENDENCY,
///   COUPLING, ISOLATED, GATEWAY, CIRCULAR.
/// Bayesian evidence aggregation: prior=0.5, update per evidence piece.
pub fn handle_hypothesize(
    state: &mut SessionState,
    input: layers::HypothesizeInput,
) -> M1ndResult<layers::HypothesizeOutput> {
    let start = Instant::now();
    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;

    if n == 0 {
        return Ok(layers::HypothesizeOutput {
            claim: input.claim.clone(),
            claim_type: "unknown".into(),
            subject_nodes: vec![],
            object_nodes: vec![],
            verdict: "inconclusive".into(),
            confidence: 0.5,
            proof_state: "blocked".into(),
            supporting_evidence: vec![],
            contradicting_evidence: vec![],
            partial_reach: None,
            paths_explored: 0,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            next_suggested_tool: None,
            next_suggested_target: None,
            next_step_hint: None,
        });
    }

    let node_to_ext = l5_build_node_to_ext_map(&graph);
    let parsed = l5_parse_claim(&input.claim);

    // Resolve subject and object nodes
    let subject_ids = l5_resolve_claim_nodes(&graph, &parsed.subject);
    let object_ids = l5_resolve_claim_nodes(&graph, &parsed.object);
    let subject_labels: Vec<String> = subject_ids
        .iter()
        .map(|&nid| node_to_ext[nid.as_usize()].clone())
        .collect();
    let object_labels: Vec<String> = object_ids
        .iter()
        .map(|&nid| node_to_ext[nid.as_usize()].clone())
        .collect();

    // Unresolvable subject -> early return
    if subject_ids.is_empty() && parsed.claim_type != L5ClaimType::Unknown {
        return Ok(layers::HypothesizeOutput {
            claim: input.claim.clone(),
            claim_type: parsed.claim_type.as_str().into(),
            subject_nodes: vec![parsed.subject.clone()],
            object_nodes: if parsed.object.is_empty() {
                vec![]
            } else {
                vec![parsed.object.clone()]
            },
            verdict: "inconclusive".into(),
            confidence: 0.5,
            proof_state: "blocked".into(),
            supporting_evidence: vec![],
            contradicting_evidence: vec![layers::HypothesisEvidence {
                evidence_type: "no_path".into(),
                description: format!(
                    "Subject '{}' could not be resolved to any graph node",
                    parsed.subject
                ),
                likelihood_factor: 1.0,
                nodes: vec![],
                relations: vec![],
                path_weight: None,
            }],
            partial_reach: None,
            paths_explored: 0,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            next_suggested_tool: None,
            next_suggested_target: None,
            next_step_hint: None,
        });
    }

    let max_hops = input.max_hops as usize;
    let budget = input.path_budget;
    let mut supporting = Vec::new();
    let mut contradicting = Vec::new();
    let mut paths_explored: usize = 0;
    let mut partial_reach_entries: Vec<layers::PartialReachEntry> = Vec::new();

    match parsed.claim_type {
        // --- NEVER_CALLS / NO_DEPENDENCY: NO path should exist ---
        L5ClaimType::NeverCalls | L5ClaimType::NoDependency => {
            for &src in &subject_ids {
                for &tgt in &object_ids {
                    let r = l5_bfs_path(&graph, src, tgt, max_hops, budget, &node_to_ext);
                    paths_explored += r.explored;
                    if r.found {
                        contradicting.push(layers::HypothesisEvidence {
                            evidence_type: "path_found".into(),
                            description: format!(
                                "Path found: '{}' -> '{}' ({} hops)",
                                node_to_ext[src.as_usize()],
                                node_to_ext[tgt.as_usize()],
                                r.path_nodes.len().saturating_sub(1)
                            ),
                            likelihood_factor: 0.2,
                            nodes: r.path_nodes,
                            relations: r.path_rels,
                            path_weight: Some(r.total_weight),
                        });
                    } else {
                        supporting.push(layers::HypothesisEvidence {
                            evidence_type: "no_path".into(),
                            description: format!(
                                "No path: '{}' -> '{}' (within {} hops)",
                                node_to_ext[src.as_usize()],
                                node_to_ext[tgt.as_usize()],
                                max_hops
                            ),
                            likelihood_factor: 2.0,
                            nodes: vec![
                                node_to_ext[src.as_usize()].clone(),
                                node_to_ext[tgt.as_usize()].clone(),
                            ],
                            relations: vec![],
                            path_weight: None,
                        });
                        if input.include_partial_flow {
                            partial_reach_entries.extend(r.partial);
                        }
                    }
                }
            }
        }

        // --- DEPENDS_ON / ALWAYS_BEFORE: path SHOULD exist ---
        L5ClaimType::DependsOn | L5ClaimType::AlwaysBefore => {
            for &src in &subject_ids {
                for &tgt in &object_ids {
                    let r = l5_bfs_path(&graph, src, tgt, max_hops, budget, &node_to_ext);
                    paths_explored += r.explored;
                    if r.found {
                        supporting.push(layers::HypothesisEvidence {
                            evidence_type: "path_found".into(),
                            description: format!(
                                "Dependency: '{}' -> '{}' ({} hops)",
                                node_to_ext[src.as_usize()],
                                node_to_ext[tgt.as_usize()],
                                r.path_nodes.len().saturating_sub(1)
                            ),
                            likelihood_factor: 2.0,
                            nodes: r.path_nodes,
                            relations: r.path_rels,
                            path_weight: Some(r.total_weight),
                        });
                    } else {
                        contradicting.push(layers::HypothesisEvidence {
                            evidence_type: "no_path".into(),
                            description: format!(
                                "No dependency: '{}' -> '{}'",
                                node_to_ext[src.as_usize()],
                                node_to_ext[tgt.as_usize()]
                            ),
                            likelihood_factor: 0.3,
                            nodes: vec![
                                node_to_ext[src.as_usize()].clone(),
                                node_to_ext[tgt.as_usize()].clone(),
                            ],
                            relations: vec![],
                            path_weight: None,
                        });
                        if input.include_partial_flow {
                            partial_reach_entries.extend(r.partial);
                        }
                    }
                }
            }
        }

        // --- COUPLING: community membership + direct edges ---
        L5ClaimType::Coupling => {
            let communities = state.topology.community_detector.detect(&graph);
            for &src in &subject_ids {
                for &tgt in &object_ids {
                    if l5_has_direct_edge(&graph, src, tgt) {
                        supporting.push(layers::HypothesisEvidence {
                            evidence_type: "causal_chain".into(),
                            description: format!(
                                "Direct edge: '{}' <-> '{}'",
                                node_to_ext[src.as_usize()],
                                node_to_ext[tgt.as_usize()]
                            ),
                            likelihood_factor: 2.0,
                            nodes: vec![
                                node_to_ext[src.as_usize()].clone(),
                                node_to_ext[tgt.as_usize()].clone(),
                            ],
                            relations: vec![],
                            path_weight: None,
                        });
                    }
                    if let Ok(ref c) = communities {
                        let (s, t) = (src.as_usize(), tgt.as_usize());
                        if s < c.assignments.len() && t < c.assignments.len() {
                            if c.assignments[s] == c.assignments[t] {
                                supporting.push(layers::HypothesisEvidence {
                                    evidence_type: "community_membership".into(),
                                    description: format!(
                                        "Same community (id={})",
                                        c.assignments[s].0
                                    ),
                                    likelihood_factor: 1.5,
                                    nodes: vec![node_to_ext[s].clone(), node_to_ext[t].clone()],
                                    relations: vec![],
                                    path_weight: None,
                                });
                            } else {
                                contradicting.push(layers::HypothesisEvidence {
                                    evidence_type: "community_membership".into(),
                                    description: format!(
                                        "Different communities ({} vs {})",
                                        c.assignments[s].0, c.assignments[t].0
                                    ),
                                    likelihood_factor: 0.5,
                                    nodes: vec![node_to_ext[s].clone(), node_to_ext[t].clone()],
                                    relations: vec![],
                                    path_weight: None,
                                });
                            }
                        }
                    }
                    paths_explored += 1;
                }
            }
        }

        // --- ISOLATED: zero or very low degree ---
        L5ClaimType::Isolated => {
            for &src in &subject_ids {
                let out_deg = graph.csr.out_range(src).len();
                let in_deg = graph.csr.in_range(src).len();
                let total = out_deg + in_deg;
                if total == 0 {
                    supporting.push(layers::HypothesisEvidence {
                        evidence_type: "activation_reach".into(),
                        description: format!(
                            "'{}' has degree 0 (isolated)",
                            node_to_ext[src.as_usize()]
                        ),
                        likelihood_factor: 2.0,
                        nodes: vec![node_to_ext[src.as_usize()].clone()],
                        relations: vec![],
                        path_weight: None,
                    });
                } else if total <= 2 {
                    supporting.push(layers::HypothesisEvidence {
                        evidence_type: "activation_reach".into(),
                        description: format!(
                            "'{}' has very low degree ({})",
                            node_to_ext[src.as_usize()],
                            total
                        ),
                        likelihood_factor: 1.5,
                        nodes: vec![node_to_ext[src.as_usize()].clone()],
                        relations: vec![],
                        path_weight: None,
                    });
                } else {
                    contradicting.push(layers::HypothesisEvidence {
                        evidence_type: "activation_reach".into(),
                        description: format!(
                            "'{}' has degree {} (out={}, in={}) -- not isolated",
                            node_to_ext[src.as_usize()],
                            total,
                            out_deg,
                            in_deg
                        ),
                        likelihood_factor: 0.3,
                        nodes: vec![node_to_ext[src.as_usize()].clone()],
                        relations: vec![],
                        path_weight: None,
                    });
                }
                paths_explored += 1;
            }
        }

        // --- GATEWAY: high centrality + counterfactual removal via RemovalMask ---
        L5ClaimType::Gateway => {
            for &src in &subject_ids {
                let out_deg = graph.csr.out_range(src).len();
                let in_deg = graph.csr.in_range(src).len();
                let pr = graph.nodes.pagerank[src.as_usize()].get();

                if pr > 0.5 || (out_deg > 5 && in_deg > 3) {
                    supporting.push(layers::HypothesisEvidence {
                        evidence_type: "counterfactual_impact".into(),
                        description: format!(
                            "High centrality: pagerank={:.3}, out={}, in={}",
                            pr, out_deg, in_deg
                        ),
                        likelihood_factor: 2.0,
                        nodes: vec![node_to_ext[src.as_usize()].clone()],
                        relations: vec![],
                        path_weight: Some(pr),
                    });
                } else {
                    contradicting.push(layers::HypothesisEvidence {
                        evidence_type: "counterfactual_impact".into(),
                        description: format!(
                            "Low centrality: pagerank={:.3}, out={}, in={}",
                            pr, out_deg, in_deg
                        ),
                        likelihood_factor: 0.4,
                        nodes: vec![node_to_ext[src.as_usize()].clone()],
                        relations: vec![],
                        path_weight: Some(pr),
                    });
                }

                // Counterfactual: remove subject and check if objects become unreachable
                if !object_ids.is_empty() {
                    let mut mask = m1nd_core::counterfactual::RemovalMask::new(
                        graph.num_nodes(),
                        graph.num_edges(),
                    );
                    mask.remove_node(&graph, src);
                    for &obj in &object_ids {
                        let reachable = l5_bfs_reachable_masked(&graph, obj, &mask, max_hops);
                        if !reachable {
                            supporting.push(layers::HypothesisEvidence {
                                evidence_type: "counterfactual_impact".into(),
                                description: format!(
                                    "Removing '{}' makes '{}' unreachable",
                                    node_to_ext[src.as_usize()],
                                    node_to_ext[obj.as_usize()]
                                ),
                                likelihood_factor: 2.0,
                                nodes: vec![
                                    node_to_ext[src.as_usize()].clone(),
                                    node_to_ext[obj.as_usize()].clone(),
                                ],
                                relations: vec![],
                                path_weight: None,
                            });
                        } else {
                            contradicting.push(layers::HypothesisEvidence {
                                evidence_type: "counterfactual_impact".into(),
                                description: format!(
                                    "'{}' still reachable after removing '{}'",
                                    node_to_ext[obj.as_usize()],
                                    node_to_ext[src.as_usize()]
                                ),
                                likelihood_factor: 0.5,
                                nodes: vec![
                                    node_to_ext[src.as_usize()].clone(),
                                    node_to_ext[obj.as_usize()].clone(),
                                ],
                                relations: vec![],
                                path_weight: None,
                            });
                        }
                        paths_explored += 1;
                    }
                }
                paths_explored += 1;
            }
        }

        // --- CIRCULAR: bidirectional path (A->B AND B->A) ---
        L5ClaimType::Circular => {
            for &src in &subject_ids {
                for &tgt in &object_ids {
                    let fwd = l5_bfs_path(&graph, src, tgt, max_hops, budget, &node_to_ext);
                    paths_explored += fwd.explored;
                    let rev = l5_bfs_path(&graph, tgt, src, max_hops, budget, &node_to_ext);
                    paths_explored += rev.explored;

                    if fwd.found && rev.found {
                        let mut all_nodes = fwd.path_nodes.clone();
                        all_nodes.extend(rev.path_nodes);
                        let mut all_rels = fwd.path_rels.clone();
                        all_rels.extend(rev.path_rels);
                        supporting.push(layers::HypothesisEvidence {
                            evidence_type: "causal_chain".into(),
                            description: format!(
                                "Cycle: '{}' -> '{}' AND back",
                                node_to_ext[src.as_usize()],
                                node_to_ext[tgt.as_usize()]
                            ),
                            likelihood_factor: 2.0,
                            nodes: all_nodes,
                            relations: all_rels,
                            path_weight: Some(fwd.total_weight + rev.total_weight),
                        });
                    } else if fwd.found || rev.found {
                        let dir = if fwd.found {
                            "forward only"
                        } else {
                            "reverse only"
                        };
                        contradicting.push(layers::HypothesisEvidence {
                            evidence_type: "causal_chain".into(),
                            description: format!(
                                "{} path between '{}' and '{}' -- not circular",
                                dir,
                                node_to_ext[src.as_usize()],
                                node_to_ext[tgt.as_usize()]
                            ),
                            likelihood_factor: 0.5,
                            nodes: if fwd.found {
                                fwd.path_nodes
                            } else {
                                rev.path_nodes
                            },
                            relations: if fwd.found {
                                fwd.path_rels
                            } else {
                                rev.path_rels
                            },
                            path_weight: Some(if fwd.found {
                                fwd.total_weight
                            } else {
                                rev.total_weight
                            }),
                        });
                    } else {
                        contradicting.push(layers::HypothesisEvidence {
                            evidence_type: "no_path".into(),
                            description: format!(
                                "No path in either direction: '{}' <-> '{}'",
                                node_to_ext[src.as_usize()],
                                node_to_ext[tgt.as_usize()]
                            ),
                            likelihood_factor: 0.2,
                            nodes: vec![
                                node_to_ext[src.as_usize()].clone(),
                                node_to_ext[tgt.as_usize()].clone(),
                            ],
                            relations: vec![],
                            path_weight: None,
                        });
                    }
                }
            }
        }

        // --- UNKNOWN: fuzzy seed-based exploration ---
        L5ClaimType::Unknown => {
            let subj_seeds = m1nd_core::seed::SeedFinder::find_seeds(&graph, &parsed.subject, 5)?;
            let obj_seeds = m1nd_core::seed::SeedFinder::find_seeds(&graph, &parsed.object, 5)?;
            for &(src, _) in &subj_seeds {
                for &(tgt, _) in &obj_seeds {
                    if src == tgt {
                        continue;
                    }
                    let r = l5_bfs_path(&graph, src, tgt, max_hops, budget, &node_to_ext);
                    paths_explored += r.explored;
                    if r.found {
                        supporting.push(layers::HypothesisEvidence {
                            evidence_type: "path_found".into(),
                            description: format!(
                                "Fuzzy: {} hops between matched nodes",
                                r.path_nodes.len().saturating_sub(1)
                            ),
                            likelihood_factor: 1.5,
                            nodes: r.path_nodes,
                            relations: r.path_rels,
                            path_weight: Some(r.total_weight),
                        });
                    }
                }
            }
            if supporting.is_empty() && !subj_seeds.is_empty() && !obj_seeds.is_empty() {
                contradicting.push(layers::HypothesisEvidence {
                    evidence_type: "no_path".into(),
                    description: "No relationship between fuzzy-matched nodes".into(),
                    likelihood_factor: 0.5,
                    nodes: vec![],
                    relations: vec![],
                    path_weight: None,
                });
            }
        }
    }

    // Bayesian confidence
    let confidence = l5_bayesian_confidence(&supporting, &contradicting);
    let verdict = if confidence > 0.8 {
        "likely_true"
    } else if confidence < 0.2 {
        "likely_false"
    } else {
        "inconclusive"
    };

    let partial_reach = if partial_reach_entries.is_empty() {
        None
    } else {
        Some(partial_reach_entries)
    };
    let proof_state = l5_hypothesize_proof_state(
        verdict,
        &supporting,
        &contradicting,
        partial_reach.as_deref(),
    );
    let (next_suggested_tool, next_suggested_target, next_step_hint) = l5_hypothesize_next_step(
        verdict,
        &supporting,
        &contradicting,
        partial_reach.as_deref(),
    );

    Ok(layers::HypothesizeOutput {
        claim: input.claim,
        claim_type: parsed.claim_type.as_str().into(),
        subject_nodes: subject_labels,
        object_nodes: object_labels,
        verdict: verdict.into(),
        confidence,
        proof_state,
        supporting_evidence: supporting,
        contradicting_evidence: contradicting,
        partial_reach,
        paths_explored,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
    })
}

fn l5_hypothesize_proof_state(
    verdict: &str,
    supporting: &[layers::HypothesisEvidence],
    contradicting: &[layers::HypothesisEvidence],
    partial_reach: Option<&[layers::PartialReachEntry]>,
) -> String {
    if (verdict == "likely_true" || verdict == "likely_false")
        && (!supporting.is_empty() || !contradicting.is_empty())
    {
        return "ready_to_edit".into();
    }
    if partial_reach
        .map(|entries| !entries.is_empty())
        .unwrap_or(false)
    {
        return "proving".into();
    }
    if !supporting.is_empty() || !contradicting.is_empty() {
        return "triaging".into();
    }
    "blocked".into()
}

fn l5_hypothesize_next_step(
    verdict: &str,
    supporting: &[layers::HypothesisEvidence],
    contradicting: &[layers::HypothesisEvidence],
    partial_reach: Option<&[layers::PartialReachEntry]>,
) -> (Option<String>, Option<String>, Option<String>) {
    let evidence = if verdict == "likely_false" {
        contradicting.first()
    } else {
        supporting.first().or_else(|| contradicting.first())
    };

    if let Some(evidence) = evidence {
        if let Some(target) = evidence.nodes.last() {
            return (
                Some("view".into()),
                Some(target.clone()),
                Some(format!(
                    "Open the strongest hypothesis evidence next: {}.",
                    target
                )),
            );
        }
    }

    if let Some(partial) = partial_reach.and_then(|entries| entries.first()) {
        return (
            Some("view".into()),
            Some(partial.node_id.clone()),
            Some(format!(
                "Open the furthest partial-reach node next: {}.",
                partial.node_id
            )),
        );
    }

    (None, None, None)
}

/// Handle m1nd.differential — focused structural diff between two graph snapshots.
/// Loads snapshots (or uses "current" for in-memory), computes node/edge/coupling deltas.
/// Filters by question keywords or focus_nodes neighborhood.
pub fn handle_differential(
    state: &mut SessionState,
    input: layers::DifferentialInput,
) -> M1ndResult<layers::DifferentialOutput> {
    let start = Instant::now();

    let graph_a = l5_load_snapshot_or_current(state, &input.snapshot_a)?;
    let graph_b = l5_load_snapshot_or_current(state, &input.snapshot_b)?;

    let ext_a = l5_collect_ext_ids(&graph_a);
    let ext_b = l5_collect_ext_ids(&graph_b);

    // Node deltas
    let mut new_nodes: Vec<String> = ext_b
        .iter()
        .filter(|id| !ext_a.contains(*id))
        .cloned()
        .collect();
    let mut removed_nodes: Vec<String> = ext_a
        .iter()
        .filter(|id| !ext_b.contains(*id))
        .cloned()
        .collect();

    // Edge deltas
    let edges_a = l5_collect_edges(&graph_a);
    let edges_b = l5_collect_edges(&graph_b);

    let mut new_edges: Vec<layers::DiffEdgeDelta> = Vec::new();
    let mut removed_edges: Vec<layers::DiffEdgeDelta> = Vec::new();
    let mut weight_changes: Vec<layers::DiffWeightDelta> = Vec::new();

    for (key, &wb) in &edges_b {
        if let Some(&wa) = edges_a.get(key) {
            let delta = wb - wa;
            if delta.abs() > 0.001 {
                weight_changes.push(layers::DiffWeightDelta {
                    source: key.0.clone(),
                    target: key.1.clone(),
                    relation: key.2.clone(),
                    old_weight: wa,
                    new_weight: wb,
                    delta,
                });
            }
        } else {
            new_edges.push(layers::DiffEdgeDelta {
                source: key.0.clone(),
                target: key.1.clone(),
                relation: key.2.clone(),
                weight: wb,
            });
        }
    }
    for (key, &wa) in &edges_a {
        if !edges_b.contains_key(key) {
            removed_edges.push(layers::DiffEdgeDelta {
                source: key.0.clone(),
                target: key.1.clone(),
                relation: key.2.clone(),
                weight: wa,
            });
        }
    }

    // Coupling deltas via community detection
    let coupling_deltas = l5_coupling_deltas(&graph_a, &graph_b, state);

    // Focus-node filtering
    if !input.focus_nodes.is_empty() {
        let focus = l5_build_focus_set(&graph_b, &input.focus_nodes);
        new_nodes.retain(|n| focus.contains(n));
        removed_nodes.retain(|n| focus.contains(n));
        new_edges.retain(|e| focus.contains(&e.source) || focus.contains(&e.target));
        removed_edges.retain(|e| focus.contains(&e.source) || focus.contains(&e.target));
        weight_changes.retain(|e| focus.contains(&e.source) || focus.contains(&e.target));
    } else if let Some(ref question) = input.question {
        // Question-keyword filtering
        let kws = l5_extract_keywords(question);
        if !kws.is_empty() {
            let m = |s: &str| {
                let lw = s.to_lowercase();
                kws.iter().any(|k| lw.contains(k))
            };
            new_nodes.retain(|n| m(n));
            removed_nodes.retain(|n| m(n));
            new_edges.retain(|e| m(&e.source) || m(&e.target) || m(&e.relation));
            removed_edges.retain(|e| m(&e.source) || m(&e.target) || m(&e.relation));
            weight_changes.retain(|e| m(&e.source) || m(&e.target) || m(&e.relation));
        }
    }

    let summary = format!(
        "+{} nodes, -{} nodes, +{} edges, -{} edges, ~{} weights, {} coupling deltas",
        new_nodes.len(),
        removed_nodes.len(),
        new_edges.len(),
        removed_edges.len(),
        weight_changes.len(),
        coupling_deltas.len()
    );

    Ok(layers::DifferentialOutput {
        snapshot_a: input.snapshot_a,
        snapshot_b: input.snapshot_b,
        new_edges,
        removed_edges,
        weight_changes,
        new_nodes,
        removed_nodes,
        coupling_deltas,
        summary,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

// =========================================================================
// L6: Execution Feedback — m1nd.trace + m1nd.validate_plan
// =========================================================================

/// Handle m1nd.trace — map runtime errors to structural root causes.
/// Parses stacktraces, maps frames to graph nodes, scores suspiciousness.
///
/// Implementation (L6-EXECUTION-FEEDBACK):
///   str-based parsing for Python, Rust, JS/TS, Go stacktraces.
///   Frame-to-node via existing NodeProvenance (source_path + line range).
///   Suspiciousness: trace_depth(0.40) + recency(0.35) + PageRank(0.25).
///   Co-change window scan for "hidden suspects" (V2 — needs git temporal data).
///   Purely additive — zero changes to existing tools.
pub fn handle_trace(
    state: &mut SessionState,
    input: layers::TraceInput,
) -> M1ndResult<layers::TraceOutput> {
    let start = Instant::now();
    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;

    // --- 1. Detect language ---
    let language = l6_detect_language(&input.error_text, input.language.as_deref());

    // --- 2. Extract error type + message ---
    let (error_type, error_message) = l6_extract_error_info(&input.error_text, &language);

    // --- 3. Parse stacktrace frames ---
    let raw_frames = l6_parse_frames(&input.error_text, &language);
    let frames_parsed = raw_frames.len();

    // Handle empty parse
    if frames_parsed == 0 {
        return Ok(layers::TraceOutput {
            language_detected: language,
            error_type,
            error_message,
            frames_parsed: 0,
            frames_mapped: 0,
            proof_state: "blocked".into(),
            suspects: vec![],
            co_change_suspects: vec![],
            causal_chain: vec![],
            fix_scope: layers::TraceFixScope {
                files_to_inspect: vec![],
                estimated_blast_radius: 0,
                risk_level: "low".into(),
            },
            next_suggested_tool: None,
            next_suggested_target: None,
            next_step_hint: None,
            unmapped_frames: vec![],
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    }

    // --- 4. Map frames to graph nodes via provenance ---
    let mut mapped: Vec<L6MappedFrame> = Vec::new();
    let mut unmapped: Vec<layers::TraceUnmappedFrame> = Vec::new();

    for frame in &raw_frames {
        match l6_resolve_frame(&graph, frame, n) {
            Some(node_id) => {
                mapped.push(L6MappedFrame {
                    node_id,
                    file: frame.file.clone(),
                    line: frame.line,
                    function: frame.function.clone(),
                });
            }
            None => {
                unmapped.push(layers::TraceUnmappedFrame {
                    file: frame.file.clone(),
                    line: frame.line,
                    function: frame.function.clone(),
                    reason: l6_classify_unmapped(&graph, &frame.file),
                });
            }
        }
    }

    let frames_mapped = mapped.len();

    // --- 5. Score suspiciousness ---
    let max_pagerank = {
        let mut mx = 0.0f32;
        for i in 0..n {
            let pr = graph.nodes.pagerank[i].get();
            if pr > mx {
                mx = pr;
            }
        }
        if mx <= 0.0 {
            1.0
        } else {
            mx
        }
    };

    let total_mapped = mapped.len();
    let mut suspects: Vec<layers::TraceSuspect> = Vec::with_capacity(total_mapped);

    for (depth_index, mf) in mapped.iter().enumerate() {
        let idx = mf.node_id.as_usize();

        // trace_depth_score: 1.0 for deepest (last in trace = highest index), linear decay
        let trace_depth_score = if total_mapped <= 1 {
            1.0
        } else {
            depth_index as f32 / (total_mapped - 1) as f32
        };

        // recency_score: placeholder 0.0 for V1 (V2 will use git modification time)
        let recency_score = 0.0f32;

        // centrality_score: normalized PageRank from graph
        let centrality_score = if idx < n {
            graph.nodes.pagerank[idx].get() / max_pagerank
        } else {
            0.0
        };

        let suspiciousness =
            trace_depth_score * 0.40 + recency_score * 0.35 + centrality_score * 0.25;

        // Gather label, type, provenance
        let (label, node_type_str, file_path, line_start, line_end) = if idx < n {
            let lbl = graph.strings.resolve(graph.nodes.label[idx]).to_string();
            let nt = format!("{:?}", graph.nodes.node_type[idx]);
            let prov = graph.resolve_node_provenance(mf.node_id);
            (lbl, nt, prov.source_path, prov.line_start, prov.line_end)
        } else {
            (format!("node_{}", idx), "Unknown".into(), None, None, None)
        };

        // Find callers via reverse CSR (up to 5)
        let related_callers = if idx < n && !graph.csr.rev_offsets.is_empty() {
            let range = graph.csr.in_range(mf.node_id);
            let mut callers = Vec::new();
            for j in range {
                let src = graph.csr.rev_sources[j];
                let src_idx = src.as_usize();
                if src_idx < n {
                    callers.push(
                        graph
                            .strings
                            .resolve(graph.nodes.label[src_idx])
                            .to_string(),
                    );
                }
                if callers.len() >= 5 {
                    break;
                }
            }
            callers
        } else {
            vec![]
        };

        let ext_id = l6_find_external_id(&graph, mf.node_id).unwrap_or_else(|| label.clone());

        suspects.push(layers::TraceSuspect {
            node_id: ext_id,
            label,
            node_type: node_type_str,
            suspiciousness,
            signals: layers::TraceSuspiciousnessSignals {
                trace_depth_score,
                recency_score,
                centrality_score,
            },
            file_path,
            line_start,
            line_end,
            related_callers,
        });
    }

    // Sort by suspiciousness descending, truncate to top_k
    suspects.sort_by(|a, b| {
        b.suspiciousness
            .partial_cmp(&a.suspiciousness)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suspects.truncate(input.top_k);

    // --- 6. Build causal chain (deepest frame backwards to outermost) ---
    let causal_chain: Vec<String> = mapped
        .iter()
        .rev()
        .filter_map(|mf| {
            let idx = mf.node_id.as_usize();
            if idx < n {
                Some(graph.strings.resolve(graph.nodes.label[idx]).to_string())
            } else {
                None
            }
        })
        .collect();

    // --- 7. Build fix_scope ---
    let mut files_to_inspect: Vec<String> = Vec::new();
    let mut seen_files = std::collections::HashSet::new();
    for s in &suspects {
        if let Some(ref fp) = s.file_path {
            if seen_files.insert(fp.clone()) {
                files_to_inspect.push(fp.clone());
            }
        }
    }

    // Estimate blast radius from top suspect via quick BFS
    let estimated_blast_radius = if let Some(top) = suspects.first() {
        if let Some(nid) = graph.resolve_id(&top.node_id) {
            l6_quick_blast_radius(&graph, nid, 2, n)
        } else {
            0
        }
    } else {
        0
    };

    let risk_level = match estimated_blast_radius {
        r if r >= 20 => "critical",
        r if r >= 10 => "high",
        r if r >= 5 => "medium",
        _ => "low",
    }
    .to_string();

    // --- 8. Co-change suspects (V1: empty — V2 uses git temporal window) ---
    let co_change_suspects: Vec<layers::TraceCoChangeSuspect> = vec![];
    let proof_state = l6_trace_proof_state(frames_mapped, &suspects, &causal_chain);

    let (next_suggested_tool, next_suggested_target, next_step_hint) =
        if let Some(top) = suspects.first() {
            let target = top.file_path.clone().unwrap_or_else(|| top.node_id.clone());
            let hint = format!(
                "Open the top suspect next: {} (suspiciousness {:.2})",
                target, top.suspiciousness
            );
            (Some("view".into()), Some(target), Some(hint))
        } else {
            (None, None, None)
        };

    Ok(layers::TraceOutput {
        language_detected: language,
        error_type,
        error_message,
        frames_parsed,
        frames_mapped,
        proof_state,
        suspects,
        co_change_suspects,
        causal_chain,
        fix_scope: layers::TraceFixScope {
            files_to_inspect,
            estimated_blast_radius,
            risk_level,
        },
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
        unmapped_frames: unmapped,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn l6_trace_proof_state(
    frames_mapped: usize,
    suspects: &[layers::TraceSuspect],
    causal_chain: &[String],
) -> String {
    let Some(top) = suspects.first() else {
        return "blocked".into();
    };
    if frames_mapped == 0 {
        return "blocked".into();
    }
    if top.suspiciousness >= 0.75 && !causal_chain.is_empty() {
        return "ready_to_edit".into();
    }
    if top.suspiciousness >= 0.4 {
        return "triaging".into();
    }
    "proving".into()
}

/// Handle m1nd.validate_plan — validate a modification plan against the graph.
/// Detects gaps (affected files not in plan), risk score, test coverage.
///
/// Implementation (L6-EXECUTION-FEEDBACK):
///   Entirely composed from existing primitives (resolve_id + CSR traversal).
///   For each action: resolve file to graph node, BFS blast radius.
///   Gaps = blast radius nodes NOT in the plan's file list.
///   Test coverage: heuristic test_* file matching in graph.
///   Risk = critical_gap_ratio * 0.4 + untested_ratio * 0.3 + blast_norm * 0.3.
pub fn handle_validate_plan(
    state: &mut SessionState,
    input: layers::ValidatePlanInput,
) -> M1ndResult<layers::ValidatePlanOutput> {
    let start = Instant::now();
    l6_vp_autowarm_plan_files(state, &input);
    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;

    let actions_analyzed = input.actions.len();
    if actions_analyzed == 0 {
        return Ok(layers::ValidatePlanOutput {
            actions_analyzed: 0,
            actions_resolved: 0,
            actions_unresolved: 0,
            gaps: vec![],
            risk_score: 0.0,
            risk_level: "low".into(),
            proof_state: "blocked".into(),
            test_coverage: layers::PlanTestCoverage {
                modified_files: 0,
                tested_files: 0,
                untested_files: vec![],
                coverage_ratio: 1.0,
            },
            suggested_additions: vec![],
            blast_radius_total: 0,
            heuristic_summary: None,
            next_suggested_tool: None,
            next_suggested_target: None,
            next_step_hint: None,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    }

    // --- 1. Build set of all files in the plan ---
    let plan_files: std::collections::HashSet<String> = input
        .actions
        .iter()
        .map(|a| l6_vp_normalize_path(&a.file_path, &state.ingest_roots))
        .collect();

    // --- 2. Resolve each action to a graph node ---
    let mut actions_resolved = 0usize;
    let mut actions_unresolved = 0usize;
    let mut resolved_nodes: Vec<(NodeId, String, String)> = Vec::new();
    let mut modified_file_paths: Vec<String> = Vec::new();

    for action in &input.actions {
        let norm_path = l6_vp_normalize_path(&action.file_path, &state.ingest_roots);
        let node_id = l6_vp_resolve_file(&graph, &action.file_path, &state.ingest_roots)
            .or_else(|| l6_vp_resolve_file(&graph, &norm_path, &state.ingest_roots));

        match node_id {
            Some(nid) => {
                actions_resolved += 1;
                resolved_nodes.push((nid, norm_path.clone(), action.action_type.clone()));
                if action.action_type != "test" {
                    modified_file_paths.push(norm_path);
                }
            }
            None => {
                actions_unresolved += 1;
                if action.action_type != "create" {
                    modified_file_paths.push(norm_path);
                }
            }
        }
    }

    // --- 3. Compute blast radius for all resolved nodes (BFS 3-hop) ---
    let mut blast_files: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut direct_deps: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut blast_radius_total = 0usize;

    for &(nid, ref _file_path, ref _action_type) in &resolved_nodes {
        let mut visited = vec![false; n];
        visited[nid.as_usize()] = true;
        let mut frontier = vec![nid];

        for hop in 0..3u32 {
            let mut next_frontier = Vec::new();
            for &node in &frontier {
                // Forward edges
                for j in graph.csr.out_range(node) {
                    let target = graph.csr.targets[j];
                    let tidx = target.as_usize();
                    if tidx < n && !visited[tidx] {
                        visited[tidx] = true;
                        next_frontier.push(target);
                        blast_radius_total += 1;
                        l6_vp_record_blast_file(
                            &graph,
                            target,
                            &plan_files,
                            &mut blast_files,
                            &mut direct_deps,
                            &state.ingest_roots,
                            hop,
                        );
                    }
                }
                // Reverse edges (dependents)
                for j in graph.csr.in_range(node) {
                    let src = graph.csr.rev_sources[j];
                    let sidx = src.as_usize();
                    if sidx < n && !visited[sidx] {
                        visited[sidx] = true;
                        next_frontier.push(src);
                        blast_radius_total += 1;
                        l6_vp_record_blast_file(
                            &graph,
                            src,
                            &plan_files,
                            &mut blast_files,
                            &mut direct_deps,
                            &state.ingest_roots,
                            hop,
                        );
                    }
                }
            }
            frontier = next_frontier;
            if frontier.is_empty() {
                break;
            }
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let mut heuristic_hotspots: Vec<(layers::PlanHeuristicHotspot, f32)> = resolved_nodes
        .iter()
        .filter(|(_, _, action_type)| action_type != "test")
        .map(|(nid, file_path, _)| {
            let external_id =
                l6_find_external_id(&graph, *nid).unwrap_or_else(|| format!("file::{}", file_path));
            l6_vp_build_heuristic_hotspot(state, file_path, &external_id, "planned", now)
        })
        .collect();

    // --- 4. Compute gaps ---
    let mut gaps: Vec<layers::PlanGap> = Vec::new();

    for gap_file in &blast_files {
        let gap_node = l6_vp_resolve_file(&graph, gap_file, &state.ingest_roots);
        let (node_id_str, signal) = match gap_node {
            Some(nid) => {
                let ext = l6_find_external_id(&graph, nid)
                    .unwrap_or_else(|| format!("file::{}", gap_file));
                (ext, graph.nodes.pagerank[nid.as_usize()].get())
            }
            None => (format!("file::{}", gap_file), 0.0),
        };
        let (hotspot, hotspot_risk) =
            l6_vp_build_heuristic_hotspot(state, gap_file, &node_id_str, "gap", now);
        let severity = if direct_deps.contains(gap_file)
            || hotspot.antibody_hits > 0
            || hotspot_risk >= 0.55
        {
            "critical"
        } else if hotspot_risk >= 0.25 {
            "warning"
        } else {
            "info"
        };

        let mut reason: String = if direct_deps.contains(gap_file) {
            "directly connected to modified file in plan".into()
        } else {
            "in blast radius of planned changes".into()
        };
        if hotspot.antibody_hits > 0 {
            reason.push_str(&format!(
                "; immune memory found {} relevant antibody match(es)",
                hotspot.antibody_hits
            ));
        }
        if hotspot.heuristic_signals.reason != "neutral heuristics" {
            reason.push_str(&format!("; {}", hotspot.heuristic_signals.reason));
        }

        gaps.push(layers::PlanGap {
            file_path: gap_file.clone(),
            node_id: node_id_str,
            reason,
            severity: severity.into(),
            signal_strength: (signal * hotspot.heuristic_signals.heuristic_factor).max(signal),
            antibody_hits: hotspot.antibody_hits,
            heuristic_signals: Some(hotspot.heuristic_signals.clone()),
            heuristics_surface_ref: Some(hotspot.heuristics_surface_ref.clone()),
        });
        heuristic_hotspots.push((hotspot, hotspot_risk));
    }

    // Sort: critical first, then by signal_strength descending
    gaps.sort_by(|a, b| {
        let sev = l6_severity_rank(&a.severity).cmp(&l6_severity_rank(&b.severity));
        if sev != std::cmp::Ordering::Equal {
            return sev;
        }
        b.signal_strength
            .partial_cmp(&a.signal_strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // --- 5. Test coverage ---
    let test_coverage = if input.include_test_impact {
        l6_vp_test_coverage(&graph, &modified_file_paths, n)
    } else {
        layers::PlanTestCoverage {
            modified_files: modified_file_paths.len(),
            tested_files: modified_file_paths.len(),
            untested_files: vec![],
            coverage_ratio: 1.0,
        }
    };

    // --- 6. Compute risk score ---
    heuristic_hotspots.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.file_path.cmp(&b.0.file_path))
    });

    let heuristic_risk = heuristic_hotspots
        .iter()
        .take(3)
        .map(|(_, risk)| *risk)
        .fold(0.0_f32, f32::max);
    let heuristic_summary = if heuristic_hotspots.is_empty() {
        None
    } else {
        Some(layers::PlanHeuristicSummary {
            heuristic_risk,
            hotspot_count: heuristic_hotspots.len(),
            low_trust_hotspots: heuristic_hotspots
                .iter()
                .filter(|(hotspot, _)| hotspot.heuristic_signals.trust_risk_multiplier > 1.05)
                .count(),
            tremor_hotspots: heuristic_hotspots
                .iter()
                .filter(|(hotspot, _)| {
                    hotspot.heuristic_signals.tremor_magnitude.unwrap_or(0.0) > 0.0
                })
                .count(),
            antibody_hotspots: heuristic_hotspots
                .iter()
                .filter(|(hotspot, _)| hotspot.antibody_hits > 0)
                .count(),
            hotspots: heuristic_hotspots
                .iter()
                .take(10)
                .map(|(hotspot, _)| hotspot.clone())
                .collect(),
        })
    };

    let (risk_score, risk_level) = if input.include_risk_score {
        let critical_gaps = gaps.iter().filter(|g| g.severity == "critical").count();
        let untested_ratio = if test_coverage.modified_files > 0 {
            1.0 - test_coverage.coverage_ratio
        } else {
            0.0
        };
        let blast_norm = if n > 0 {
            (blast_radius_total as f32 / n as f32).min(1.0)
        } else {
            0.0
        };

        let critical_gap_ratio = ((critical_gaps as f32) * 0.1).min(1.0);
        let score = (critical_gap_ratio * 0.30
            + untested_ratio * 0.25
            + blast_norm * 0.20
            + heuristic_risk * 0.25)
            .min(1.0);

        let level = match score {
            s if s >= 0.8 => "critical",
            s if s >= 0.6 => "high",
            s if s >= 0.3 => "medium",
            _ => "low",
        };
        (score, level.to_string())
    } else {
        (0.0, "low".into())
    };

    // --- 7. Suggested additions ---
    let mut suggested_additions: Vec<layers::PlanSuggestedAction> = Vec::new();

    for gap in &gaps {
        if gap.severity == "critical" {
            suggested_additions.push(layers::PlanSuggestedAction {
                action_type: "modify".into(),
                file_path: gap.file_path.clone(),
                reason: format!("Critical gap: {}", gap.reason),
            });
        }
    }
    for untested in &test_coverage.untested_files {
        suggested_additions.push(layers::PlanSuggestedAction {
            action_type: "test".into(),
            file_path: l6_vp_suggest_test_path(untested),
            reason: format!("No test coverage for modified file {}", untested),
        });
    }

    let top_hotspot = heuristic_summary
        .as_ref()
        .and_then(|summary| summary.hotspots.first());
    let (next_suggested_tool, next_suggested_target, next_step_hint) =
        if let Some(hotspot) = top_hotspot {
            (
                Some("heuristics_surface".into()),
                Some(hotspot.file_path.clone()),
                Some(format!(
                    "Inspect {} next: {}",
                    hotspot.file_path, hotspot.proof_hint
                )),
            )
        } else if let Some(gap) = gaps.iter().find(|gap| gap.severity == "critical") {
            (
                Some("view".into()),
                Some(gap.file_path.clone()),
                Some(format!(
                    "Open {} next because it is a critical gap: {}",
                    gap.file_path, gap.reason
                )),
            )
        } else {
            (None, None, None)
        };
    let proof_state = l6_vp_proof_state(
        actions_resolved,
        actions_unresolved,
        &gaps,
        heuristic_summary.as_ref(),
        &next_suggested_tool,
    );

    Ok(layers::ValidatePlanOutput {
        actions_analyzed,
        actions_resolved,
        actions_unresolved,
        gaps,
        risk_score,
        risk_level,
        proof_state,
        test_coverage,
        suggested_additions,
        blast_radius_total,
        heuristic_summary,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn l6_vp_proof_state(
    actions_resolved: usize,
    actions_unresolved: usize,
    gaps: &[layers::PlanGap],
    heuristic_summary: Option<&layers::PlanHeuristicSummary>,
    next_suggested_tool: &Option<String>,
) -> String {
    if actions_resolved == 0 && actions_unresolved > 0 {
        return "blocked".into();
    }
    if gaps.iter().any(|gap| gap.severity == "critical")
        || heuristic_summary
            .map(|summary| !summary.hotspots.is_empty())
            .unwrap_or(false)
    {
        return "proving".into();
    }
    if next_suggested_tool.is_some() {
        return "triaging".into();
    }
    "ready_to_edit".into()
}

fn l6_vp_autowarm_plan_files(state: &mut SessionState, input: &layers::ValidatePlanInput) {
    for action in &input.actions {
        if action.action_type == "create" || action.action_type == "delete" {
            continue;
        }

        let resolved_path = l6_vp_resolve_disk_path(&action.file_path, &state.ingest_roots);
        if !resolved_path.exists() {
            continue;
        }

        let resolved_path_str = resolved_path.to_string_lossy().to_string();
        let already_present = {
            let graph = state.graph.read();
            l6_vp_resolve_file(&graph, &resolved_path_str, &state.ingest_roots).is_some()
        };

        if already_present {
            continue;
        }

        let ingest_input = crate::protocol::IngestInput {
            path: resolved_path_str,
            agent_id: input.agent_id.clone(),
            mode: "merge".to_string(),
            incremental: true,
            adapter: "code".to_string(),
            namespace: None,
        };
        let _ = crate::tools::handle_ingest(state, ingest_input);
    }
}

// =========================================================================
// L6 Trace Helpers
// =========================================================================

/// Parsed raw stacktrace frame.
struct L6RawFrame {
    file: String,
    line: u32,
    function: String,
}

/// A frame resolved to a graph node.
struct L6MappedFrame {
    node_id: NodeId,
    file: String,
    line: u32,
    function: String,
}

/// Auto-detect language from error text patterns.
fn l6_detect_language(error_text: &str, hint: Option<&str>) -> String {
    if let Some(h) = hint {
        return h.to_lowercase();
    }
    if error_text.contains("Traceback") || error_text.contains("File \"") {
        return "python".into();
    }
    if error_text.contains("thread '") || error_text.contains("panicked at") {
        return "rust".into();
    }
    if error_text.contains("goroutine") || error_text.contains(".go:") {
        return "go".into();
    }
    if error_text.contains(".ts:") || error_text.contains(".tsx:") {
        return "typescript".into();
    }
    if error_text.contains(".js:") || error_text.contains("    at ") {
        return "javascript".into();
    }
    "unknown".into()
}

/// Extract error type and message from error text.
fn l6_extract_error_info(error_text: &str, language: &str) -> (String, String) {
    let lines: Vec<&str> = error_text.lines().collect();
    if lines.is_empty() {
        return ("UnknownError".into(), String::new());
    }
    match language {
        "python" => {
            let last = lines.last().unwrap_or(&"");
            if let Some(pos) = last.find(": ") {
                (
                    last[..pos].trim().to_string(),
                    last[pos + 2..].trim().to_string(),
                )
            } else {
                (last.trim().to_string(), String::new())
            }
        }
        "rust" => {
            for line in &lines {
                if let Some(p) = line.find("panicked at") {
                    let rest = &line[p + 11..];
                    let msg = rest.trim().trim_matches('\'').trim_matches(',');
                    let msg = msg.find(", ").map_or(msg, |c| &msg[..c]);
                    return ("panic".into(), msg.to_string());
                }
            }
            (
                "RuntimeError".into(),
                lines.first().unwrap_or(&"").trim().to_string(),
            )
        }
        "javascript" | "typescript" => {
            let first = lines.first().unwrap_or(&"");
            if let Some(pos) = first.find(": ") {
                (
                    first[..pos].trim().to_string(),
                    first[pos + 2..].trim().to_string(),
                )
            } else {
                (first.trim().to_string(), String::new())
            }
        }
        "go" => {
            for line in &lines {
                if let Some(idx) = line.find("panic:") {
                    return ("panic".into(), line[idx + 6..].trim().to_string());
                }
            }
            (
                "RuntimeError".into(),
                lines.first().unwrap_or(&"").trim().to_string(),
            )
        }
        _ => {
            let first = lines.first().unwrap_or(&"");
            if let Some(pos) = first.find(": ") {
                let etype = first[..pos].trim();
                if !etype.contains(' ') || etype.len() < 40 {
                    return (etype.to_string(), first[pos + 2..].trim().to_string());
                }
            }
            ("UnknownError".into(), first.trim().to_string())
        }
    }
}

/// Parse stacktrace frames from error text.
fn l6_parse_frames(error_text: &str, language: &str) -> Vec<L6RawFrame> {
    let mut frames = Vec::new();
    for line in error_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match language {
            "python" => {
                if let Some(rest) = trimmed.strip_prefix("File \"") {
                    if let Some(qe) = rest.find('"') {
                        let file = rest[..qe].to_string();
                        let after = &rest[qe + 1..];
                        if let Some(lp) = after.find("line ") {
                            let ns: String = after[lp + 5..]
                                .chars()
                                .take_while(|c| c.is_ascii_digit())
                                .collect();
                            let ln = ns.parse::<u32>().unwrap_or(0);
                            let func = after
                                .find(", in ")
                                .map(|p| after[p + 5..].trim().to_string())
                                .unwrap_or_else(|| "<module>".into());
                            frames.push(L6RawFrame {
                                file,
                                line: ln,
                                function: func,
                            });
                        }
                    }
                }
            }
            "rust" => {
                if let Some(at_pos) = trimmed.find("at ") {
                    let rest = &trimmed[at_pos + 3..];
                    if let Some((file, ln)) = l6_parse_path_line_col(rest) {
                        frames.push(L6RawFrame {
                            file,
                            line: ln,
                            function: String::new(),
                        });
                    }
                }
            }
            "javascript" | "typescript" => {
                if let Some(at_pos) = trimmed.find("at ") {
                    let rest = &trimmed[at_pos + 3..];
                    if let Some(ps) = rest.find('(') {
                        let func = rest[..ps].trim().to_string();
                        let inner = rest[ps + 1..].trim_end_matches(')');
                        if let Some((file, ln)) = l6_parse_path_line_col(inner) {
                            frames.push(L6RawFrame {
                                file,
                                line: ln,
                                function: func,
                            });
                        }
                    } else if let Some((file, ln)) = l6_parse_path_line_col(rest) {
                        frames.push(L6RawFrame {
                            file,
                            line: ln,
                            function: String::new(),
                        });
                    }
                }
            }
            "go" => {
                if trimmed.contains(".go:") {
                    if let Some((file, ln)) = l6_parse_go_frame(trimmed) {
                        frames.push(L6RawFrame {
                            file,
                            line: ln,
                            function: String::new(),
                        });
                    }
                }
            }
            _ => {
                if let Some(rest) = trimmed.strip_prefix("File \"") {
                    if let Some(qe) = rest.find('"') {
                        let file = rest[..qe].to_string();
                        let after = &rest[qe + 1..];
                        if let Some(lp) = after.find("line ") {
                            let ns: String = after[lp + 5..]
                                .chars()
                                .take_while(|c| c.is_ascii_digit())
                                .collect();
                            let ln = ns.parse::<u32>().unwrap_or(0);
                            let func = after
                                .find(", in ")
                                .map(|p| after[p + 5..].trim().to_string())
                                .unwrap_or_default();
                            frames.push(L6RawFrame {
                                file,
                                line: ln,
                                function: func,
                            });
                        }
                    }
                } else if let Some(at_pos) = trimmed.find("at ") {
                    let rest = &trimmed[at_pos + 3..];
                    if let Some(ps) = rest.find('(') {
                        let func = rest[..ps].trim().to_string();
                        let inner = rest[ps + 1..].trim_end_matches(')');
                        if let Some((f, l)) = l6_parse_path_line_col(inner) {
                            frames.push(L6RawFrame {
                                file: f,
                                line: l,
                                function: func,
                            });
                        }
                    } else if let Some((f, l)) = l6_parse_path_line_col(rest) {
                        frames.push(L6RawFrame {
                            file: f,
                            line: l,
                            function: String::new(),
                        });
                    }
                }
            }
        }
    }
    frames
}

/// Parse "path:line:col" or "path:line" into (path, line).
fn l6_parse_path_line_col(s: &str) -> Option<(String, u32)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.rsplitn(4, ':').collect();
    match parts.len() {
        3 => {
            let ln = parts[1].trim().parse::<u32>().ok()?;
            let file = parts[2].trim().to_string();
            if file.is_empty() {
                return None;
            }
            Some((file, ln))
        }
        2 => {
            let ln = parts[0].trim().parse::<u32>().ok()?;
            let file = parts[1].trim().to_string();
            if file.is_empty() {
                return None;
            }
            Some((file, ln))
        }
        4 => {
            let ln = parts[2].trim().parse::<u32>().ok()?;
            let file = parts[3].trim().to_string();
            if file.is_empty() {
                return None;
            }
            Some((file, ln))
        }
        _ => None,
    }
}

/// Parse Go frame: "path.go:line +offset".
fn l6_parse_go_frame(s: &str) -> Option<(String, u32)> {
    let s = s.trim().trim_start_matches('\t');
    let go_idx = s.find(".go:")?;
    let file = s[..go_idx + 3].to_string();
    let rest = &s[go_idx + 4..];
    let ns: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let ln = ns.parse::<u32>().ok()?;
    Some((file, ln))
}

/// Normalize a file path for provenance matching.
fn l6_normalize_path(path: &str) -> String {
    let p = path.trim().strip_prefix("./").unwrap_or(path.trim());
    for prefix in &["backend/", "frontend/", "mcp/", "src/"] {
        if let Some(idx) = p.find(prefix) {
            return p[idx..].to_string();
        }
    }
    p.to_string()
}

/// Resolve a parsed frame to its best-matching graph node via provenance.
fn l6_resolve_frame(
    graph: &m1nd_core::graph::Graph,
    frame: &L6RawFrame,
    n: usize,
) -> Option<NodeId> {
    let frame_path = l6_normalize_path(&frame.file);

    // Strategy 1: direct external_id lookup
    let ext_id = format!("file::{}", frame_path);
    if let Some(nid) = graph.resolve_id(&ext_id) {
        if frame.line > 0 {
            if let Some(specific) = l6_find_specific_node(graph, &frame_path, frame.line, n) {
                return Some(specific);
            }
        }
        return Some(nid);
    }

    // Strategy 2: scan all node provenance for path suffix match
    let mut best: Option<(NodeId, u32)> = None;
    for i in 0..n {
        let nid = NodeId::new(i as u32);
        let prov = &graph.nodes.provenance[i];
        if let Some(sp) = prov.source_path {
            if let Some(source_str) = graph.strings.try_resolve(sp) {
                let norm = l6_normalize_path(source_str);
                let paths_match = norm == frame_path
                    || norm.ends_with(&frame_path)
                    || frame_path.ends_with(&norm);
                if paths_match {
                    if frame.line > 0 && prov.line_start > 0 {
                        if frame.line >= prov.line_start && frame.line <= prov.line_end {
                            let range = prov.line_end - prov.line_start;
                            let score = 10000u32.saturating_sub(range);
                            if best.is_none_or(|(_, s)| score > s) {
                                best = Some((nid, score));
                            }
                        }
                    } else if prov.line_start == 0 && best.is_none() {
                        best = Some((nid, 0));
                    }
                }
            }
        }
    }
    best.map(|(nid, _)| nid)
}

/// Find a specific sub-file node (function/class) at a given line.
fn l6_find_specific_node(
    graph: &m1nd_core::graph::Graph,
    file_path: &str,
    line: u32,
    n: usize,
) -> Option<NodeId> {
    let mut best: Option<(NodeId, u32)> = None;
    for i in 0..n {
        let prov = &graph.nodes.provenance[i];
        if let Some(sp) = prov.source_path {
            if let Some(s) = graph.strings.try_resolve(sp) {
                let norm = l6_normalize_path(s);
                if (norm == file_path || norm.ends_with(file_path) || file_path.ends_with(&norm))
                    && prov.line_start > 0
                    && line >= prov.line_start
                    && line <= prov.line_end
                {
                    let range = prov.line_end - prov.line_start;
                    let score = 10000u32.saturating_sub(range);
                    if best.is_none_or(|(_, s)| score > s) {
                        best = Some((NodeId::new(i as u32), score));
                    }
                }
            }
        }
    }
    best.map(|(nid, _)| nid)
}

/// Classify why a frame couldn't be mapped.
fn l6_classify_unmapped(graph: &m1nd_core::graph::Graph, file: &str) -> String {
    if file.contains("site-packages/")
        || file.contains("node_modules/")
        || file.contains("/lib/python")
        || file.contains("/usr/lib/")
        || file.contains(".cargo/registry")
        || file.contains("/.rustup/")
    {
        return "stdlib/third-party".into();
    }
    let norm = l6_normalize_path(file);
    let ext_id = format!("file::{}", norm);
    if graph.resolve_id(&ext_id).is_some() {
        return "line outside any node range".into();
    }
    "file not in graph".into()
}

/// Find external_id string for a NodeId.
fn l6_find_external_id(graph: &m1nd_core::graph::Graph, node_id: NodeId) -> Option<String> {
    for (interned, &nid) in &graph.id_to_node {
        if nid == node_id {
            return Some(graph.strings.resolve(*interned).to_string());
        }
    }
    None
}

/// Quick BFS blast radius count.
fn l6_quick_blast_radius(
    graph: &m1nd_core::graph::Graph,
    start: NodeId,
    max_hops: u32,
    n: usize,
) -> usize {
    let mut visited = vec![false; n];
    visited[start.as_usize()] = true;
    let mut frontier = vec![start];
    let mut count = 0usize;
    for _hop in 0..max_hops {
        let mut next = Vec::new();
        for &node in &frontier {
            for j in graph.csr.out_range(node) {
                let t = graph.csr.targets[j];
                let ti = t.as_usize();
                if ti < n && !visited[ti] {
                    visited[ti] = true;
                    count += 1;
                    next.push(t);
                }
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
    count
}

// =========================================================================
// L6 Validate Plan Helpers
// =========================================================================

/// Normalize path-like input for scope and plan validation.
///
/// This accepts relative paths, absolute paths under an ingest root, and
/// `file::...` forms. It also collapses common repo prefixes via
/// `l6_normalize_path` so equivalent path shapes land on the same graph node.
fn l7_normalize_path_hint(path_like: &str, ingest_roots: &[String]) -> String {
    normalize_scope_path(Some(path_like), ingest_roots).unwrap_or_else(|| {
        path_like
            .trim()
            .strip_prefix("file::")
            .unwrap_or(path_like.trim())
            .strip_prefix("./")
            .unwrap_or(path_like.trim())
            .trim_matches('/')
            .to_string()
    })
}

/// Normalize path for plan validation.
fn l6_vp_normalize_path(path: &str, ingest_roots: &[String]) -> String {
    l7_normalize_path_hint(path, ingest_roots)
}

fn l6_vp_resolve_disk_path(path: &str, ingest_roots: &[String]) -> std::path::PathBuf {
    let trimmed = path.trim().strip_prefix("file::").unwrap_or(path.trim());
    let candidate = std::path::Path::new(trimmed);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }

    for root in ingest_roots.iter().rev() {
        let joined = std::path::Path::new(root).join(trimmed);
        if joined.exists() {
            return joined;
        }
    }

    if let Some(root) = ingest_roots.last() {
        return std::path::Path::new(root).join(trimmed);
    }

    candidate.to_path_buf()
}

/// Resolve a file path to its graph node.
fn l6_vp_resolve_file(
    graph: &m1nd_core::graph::Graph,
    path: &str,
    ingest_roots: &[String],
) -> Option<NodeId> {
    let normalized = l7_normalize_path_hint(path, ingest_roots);
    let normalized_fallback = l6_normalize_path(&normalized);
    let raw_trimmed = path.trim();
    let raw_slash_trimmed = raw_trimmed.trim_matches('/');
    let normalized_slash_trimmed = normalized.trim_matches('/');
    let normalized_fallback_slash_trimmed = normalized_fallback.trim_matches('/');

    for candidate in [
        normalized.as_str(),
        normalized_fallback.as_str(),
        normalized_slash_trimmed,
        normalized_fallback_slash_trimmed,
        raw_trimmed,
        raw_slash_trimmed,
    ] {
        if candidate.is_empty() {
            continue;
        }
        if let Some(nid) = graph.resolve_id(&format!("file::{}", candidate)) {
            return Some(nid);
        }
        if let Some(nid) = graph.resolve_id(candidate) {
            return Some(nid);
        }
    }

    // Fallback: resolve through node provenance, matching equivalent absolute
    // and repo-relative path suffixes the same way other file-centric handlers do.
    let n = graph.num_nodes() as usize;
    let mut first_match: Option<NodeId> = None;
    for i in 0..n {
        let nid = NodeId::new(i as u32);
        let prov = &graph.nodes.provenance[i];
        if let Some(sp) = prov.source_path {
            if let Some(source_str) = graph.strings.try_resolve(sp) {
                let source_norm = l6_normalize_path(source_str);
                let paths_match = source_norm == normalized_fallback
                    || source_norm.ends_with(&normalized_fallback)
                    || normalized_fallback.ends_with(&source_norm)
                    || source_norm == normalized
                    || source_norm.ends_with(&normalized)
                    || normalized.ends_with(&source_norm);
                if paths_match {
                    if graph.nodes.node_type[i] == m1nd_core::types::NodeType::File {
                        return Some(nid);
                    }
                    first_match.get_or_insert(nid);
                }
            }
        }
    }

    first_match
}

/// Record a blast radius file if it's not already in the plan.
fn l6_vp_record_blast_file(
    graph: &m1nd_core::graph::Graph,
    node: NodeId,
    plan_files: &std::collections::HashSet<String>,
    blast_files: &mut std::collections::HashSet<String>,
    direct_deps: &mut std::collections::HashSet<String>,
    ingest_roots: &[String],
    hop: u32,
) {
    let prov = graph.resolve_node_provenance(node);
    if let Some(ref sp) = prov.source_path {
        let norm = l6_vp_normalize_path(sp, ingest_roots);
        if norm.is_empty() {
            return;
        }
        if l6_vp_should_suppress_gap_candidate(&norm, plan_files) {
            return;
        }
        if !plan_files.contains(&norm) {
            blast_files.insert(norm.clone());
            if hop == 0 {
                direct_deps.insert(norm);
            }
        }
    }
}

fn l6_vp_should_suppress_gap_candidate(
    path: &str,
    plan_files: &std::collections::HashSet<String>,
) -> bool {
    if plan_files.contains(path) {
        return false;
    }

    let basename = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");

    matches!(basename, "Cargo.toml" | "Cargo.lock")
        || (basename.starts_with("test_") && path.ends_with(".md"))
        || path.contains("/target/")
        || path.contains("/node_modules/")
        || path.contains("/dist/")
}

/// Compute test coverage for modified files.
fn l6_vp_test_coverage(
    graph: &m1nd_core::graph::Graph,
    modified_files: &[String],
    n: usize,
) -> layers::PlanTestCoverage {
    if modified_files.is_empty() {
        return layers::PlanTestCoverage {
            modified_files: 0,
            tested_files: 0,
            untested_files: vec![],
            coverage_ratio: 1.0,
        };
    }

    let mut tested = 0usize;
    let mut untested: Vec<String> = Vec::new();

    for fp in modified_files {
        if l6_vp_has_test(graph, fp, n) {
            tested += 1;
        } else {
            untested.push(fp.clone());
        }
    }

    let ratio = tested as f32 / modified_files.len() as f32;
    layers::PlanTestCoverage {
        modified_files: modified_files.len(),
        tested_files: tested,
        untested_files: untested,
        coverage_ratio: ratio,
    }
}

/// Check if a test file exists for a given source file.
fn l6_vp_has_test(graph: &m1nd_core::graph::Graph, source_file: &str, n: usize) -> bool {
    for pat in &l6_vp_test_patterns(source_file) {
        if graph.resolve_id(&format!("file::{}", pat)).is_some() {
            return true;
        }
    }
    let basename = source_file.rsplit('/').next().unwrap_or(source_file);
    let stem = if let Some(dot) = basename.rfind('.') {
        &basename[..dot]
    } else {
        basename
    };
    let test_prefix = format!("test_{}", stem);
    for i in 0..n {
        let label = graph.strings.resolve(graph.nodes.label[i]);
        if label.contains(&test_prefix) {
            return true;
        }
    }
    false
}

/// Generate test file path patterns for a source file.
fn l6_vp_test_patterns(source_file: &str) -> Vec<String> {
    let mut pats = Vec::new();
    let basename = source_file.rsplit('/').next().unwrap_or(source_file);
    let dir = if source_file.contains('/') {
        &source_file[..source_file.len() - basename.len()]
    } else {
        ""
    };

    if let Some(stem) = basename.strip_suffix(".py") {
        pats.push(format!("{}tests/test_{}.py", dir, stem));
        pats.push(format!("{}test_{}.py", dir, stem));
        if dir.starts_with("backend/") {
            pats.push(format!("backend/tests/test_{}.py", stem));
        }
    } else if let Some(stem) = basename.strip_suffix(".tsx") {
        pats.push(format!("{}{}.test.tsx", dir, stem));
        pats.push(format!("{}{}.spec.tsx", dir, stem));
    } else if let Some(stem) = basename.strip_suffix(".ts") {
        pats.push(format!("{}{}.test.ts", dir, stem));
        pats.push(format!("{}{}.spec.ts", dir, stem));
    } else if let Some(stem) = basename.strip_suffix(".rs") {
        pats.push(format!("{}tests/{}.rs", dir, stem));
        pats.push(format!("{}tests/test_{}.rs", dir, stem));
    }
    pats
}

/// Suggest a test file path for an untested source file.
fn l6_vp_suggest_test_path(source_file: &str) -> String {
    let basename = source_file.rsplit('/').next().unwrap_or(source_file);
    let dir = if source_file.contains('/') {
        &source_file[..source_file.len() - basename.len()]
    } else {
        ""
    };
    if let Some(stem) = basename.strip_suffix(".py") {
        if dir.starts_with("backend/") && !dir.contains("tests/") {
            return format!("backend/tests/test_{}.py", stem);
        }
        return format!("{}tests/test_{}.py", dir, stem);
    }
    if let Some(stem) = basename.strip_suffix(".tsx") {
        return format!("{}{}.test.tsx", dir, stem);
    }
    if let Some(stem) = basename.strip_suffix(".ts") {
        return format!("{}{}.test.ts", dir, stem);
    }
    if let Some(stem) = basename.strip_suffix(".rs") {
        return format!("{}tests/{}.rs", dir, stem);
    }
    format!("{}test_{}", dir, basename)
}

fn l6_vp_antibody_hits(state: &SessionState, external_id: &str, normalized_path: &str) -> usize {
    let file_external_id = if external_id.starts_with("file::") {
        external_id.to_string()
    } else {
        format!("file::{}", normalized_path)
    };

    state
        .antibodies
        .iter()
        .filter(|antibody| antibody.enabled)
        .filter(|antibody| {
            antibody.source_nodes.iter().any(|source| {
                source == external_id
                    || source == &file_external_id
                    || source.strip_prefix("file::") == Some(normalized_path)
                    || source.ends_with(normalized_path)
            })
        })
        .count()
}

fn l6_vp_heuristic_reason(
    trust_factor: f32,
    tremor_factor: f32,
    tremor_observation_count: usize,
    antibody_hits: usize,
) -> String {
    let mut parts = Vec::new();
    if trust_factor > 1.01 {
        parts.push("low-trust risk prior".to_string());
    } else if trust_factor < 0.99 {
        parts.push("high-trust damping".to_string());
    }
    if tremor_factor > 1.01 && tremor_observation_count > 0 {
        parts.push("tremor acceleration".to_string());
    }
    if antibody_hits > 0 {
        parts.push(format!("immune-memory recurrence x{}", antibody_hits));
    }
    if parts.is_empty() {
        "neutral heuristics".to_string()
    } else {
        parts.join(" + ")
    }
}

fn l6_vp_proof_hint(
    file_path: &str,
    role: &str,
    heuristic_reason: &str,
    antibody_hits: usize,
) -> String {
    let mut hint = match role {
        "planned" => format!(
            "{} is already in the plan and carries heuristic risk",
            file_path
        ),
        "gap" => format!("{} is outside the plan but structurally risky", file_path),
        _ => format!("{} surfaced as a risky proof seam", file_path),
    };
    if heuristic_reason != "neutral heuristics" {
        hint.push_str(&format!(": {}", heuristic_reason));
    }
    if antibody_hits > 0 {
        hint.push_str(&format!("; antibody hits={}", antibody_hits));
    }
    hint
}

fn l6_vp_build_heuristic_hotspot(
    state: &SessionState,
    file_path: &str,
    external_id: &str,
    role: &str,
    now: f64,
) -> (layers::PlanHeuristicHotspot, f32) {
    let trust = state.trust_ledger.compute_trust(external_id, now);
    let raw_trust_factor = state.trust_ledger.adjust_prior(
        1.0,
        std::slice::from_ref(&external_id.to_string()),
        false,
        now,
    );
    let trust_factor = l2_dampened_trust_factor(raw_trust_factor);

    let tremor_observation_count = state.tremor_registry.observation_count(external_id);
    let tremor_alert = if tremor_observation_count < 3 {
        None
    } else {
        state
            .tremor_registry
            .analyze(
                m1nd_core::tremor::TremorWindow::All,
                0.0,
                1,
                Some(external_id),
                now,
                0,
            )
            .tremors
            .into_iter()
            .next()
    };
    let tremor_factor = l2_dampened_tremor_factor(tremor_alert.as_ref());
    let antibody_hits = l6_vp_antibody_hits(state, external_id, file_path);
    let antibody_factor = 1.0 + (antibody_hits.min(3) as f32 * 0.05);
    let heuristic_factor = trust_factor * tremor_factor * antibody_factor;

    let trust_risk = ((trust.risk_multiplier - 1.0) / 2.0).clamp(0.0, 1.0);
    let tremor_risk = tremor_alert
        .as_ref()
        .map(|alert| alert.magnitude.clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let antibody_risk = (antibody_hits.min(3) as f32 / 3.0).clamp(0.0, 1.0);
    let hotspot_risk = (trust_risk * 0.5 + tremor_risk * 0.3 + antibody_risk * 0.2).min(1.0);
    let heuristic_reason = l6_vp_heuristic_reason(
        trust_factor,
        tremor_factor,
        tremor_observation_count,
        antibody_hits,
    );

    (
        layers::PlanHeuristicHotspot {
            file_path: file_path.to_string(),
            node_id: external_id.to_string(),
            role: role.to_string(),
            antibody_hits,
            proof_hint: l6_vp_proof_hint(file_path, role, &heuristic_reason, antibody_hits),
            heuristic_signals: layers::HeuristicSignals {
                heuristic_factor,
                trust_score: trust.trust_score,
                trust_risk_multiplier: trust.risk_multiplier,
                trust_tier: format!("{:?}", trust.tier),
                tremor_magnitude: tremor_alert.as_ref().map(|alert| alert.magnitude),
                tremor_observation_count,
                tremor_risk_level: tremor_alert
                    .as_ref()
                    .map(|alert| format!("{:?}", alert.risk_level)),
                reason: heuristic_reason,
            },
            heuristics_surface_ref: layers::HeuristicsSurfaceRef {
                node_id: external_id.to_string(),
                file_path: file_path.to_string(),
            },
        },
        hotspot_risk,
    )
}

/// Map severity string to sort order (lower = higher priority).
fn l6_severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "warning" => 1,
        "info" => 2,
        _ => 3,
    }
}

// =========================================================================
// L7: Multi-Repository Federation — m1nd.federate
// =========================================================================

/// Handle m1nd.federate — ingest multiple repos into a unified federated graph.
/// Uses repo-prefixed external_ids so all existing tools work cross-repo.
///
/// Implementation notes (from L7-MULTI-REPO-FEDERATION):
///   Node IDs: `{repo}::file::path/to/file.py::fn::function_name`.
///   All 13 existing tools work cross-repo with ZERO query engine changes.
///   Per-repo subgraph caching for incremental re-ingest.
///   6 cross-repo edge types: shared_config, api_contract, package_dep,
///   shared_type, deployment_dep, mcp_contract.
///   Scale: 5 repos × ~10K nodes = ~50K nodes, ~20MB RAM, ~300ms PageRank.
pub fn handle_federate(
    state: &mut SessionState,
    input: layers::FederateInput,
) -> M1ndResult<layers::FederateOutput> {
    let start = Instant::now();

    // --- Empty repos: fast exit ---
    if input.repos.is_empty() {
        return Ok(layers::FederateOutput {
            repos_ingested: vec![],
            total_nodes: 0,
            total_edges: 0,
            cross_repo_edges: vec![],
            cross_repo_edge_count: 0,
            incremental: input.incremental,
            skipped_repos: vec![],
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    }

    // =========================================================================
    // Step 1: Per-repo ingestion with namespace prefix
    // =========================================================================
    let mut repo_results: Vec<layers::FederateRepoResult> = Vec::with_capacity(input.repos.len());
    let mut prefixed_graphs: Vec<(String, m1nd_core::graph::Graph)> = Vec::new();
    let mut skipped_repos: Vec<String> = Vec::new();

    for repo in &input.repos {
        let repo_path = PathBuf::from(&repo.path);

        if !repo_path.exists() {
            eprintln!(
                "[m1nd-federate] Skipping repo '{}': path does not exist: {}",
                repo.name, repo.path
            );
            skipped_repos.push(repo.name.clone());
            repo_results.push(layers::FederateRepoResult {
                name: repo.name.clone(),
                path: repo.path.clone(),
                node_count: 0,
                edge_count: 0,
                from_cache: false,
                ingest_ms: 0.0,
            });
            continue;
        }

        let repo_start = Instant::now();

        let config = m1nd_ingest::IngestConfig {
            root: repo_path,
            ..m1nd_ingest::IngestConfig::default()
        };
        let ingestor = m1nd_ingest::Ingestor::new(config);
        let (repo_graph, _stats) = match ingestor.ingest() {
            Ok(result) => result,
            Err(e) => {
                eprintln!(
                    "[m1nd-federate] Skipping repo '{}': ingest failed: {}",
                    repo.name, e
                );
                skipped_repos.push(repo.name.clone());
                repo_results.push(layers::FederateRepoResult {
                    name: repo.name.clone(),
                    path: repo.path.clone(),
                    node_count: 0,
                    edge_count: 0,
                    from_cache: false,
                    ingest_ms: repo_start.elapsed().as_secs_f64() * 1000.0,
                });
                continue;
            }
        };

        // Prefix ALL node external_ids with "{repo.name}::"
        let prefixed = l7_prefix_graph_nodes(&repo_graph, &repo.name)?;

        let node_count = prefixed.num_nodes();
        let edge_count = prefixed.num_edges() as u32;

        repo_results.push(layers::FederateRepoResult {
            name: repo.name.clone(),
            path: repo.path.clone(),
            node_count,
            edge_count,
            from_cache: false,
            ingest_ms: repo_start.elapsed().as_secs_f64() * 1000.0,
        });

        prefixed_graphs.push((repo.name.clone(), prefixed));
    }

    if prefixed_graphs.is_empty() {
        return Ok(layers::FederateOutput {
            repos_ingested: repo_results,
            total_nodes: 0,
            total_edges: 0,
            cross_repo_edges: vec![],
            cross_repo_edge_count: 0,
            incremental: input.incremental,
            skipped_repos,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    }

    // =========================================================================
    // Step 2: Merge all repo graphs into unified graph
    // =========================================================================
    let mut drain = prefixed_graphs.drain(..);
    let (_, mut merged) = drain.next().unwrap();
    for (_, overlay) in drain {
        merged = m1nd_ingest::merge::merge_graphs(&merged, &overlay)?;
    }

    // =========================================================================
    // Step 3: Cross-repo edge detection
    // =========================================================================
    let cross_repo_edges = if input.detect_cross_repo_edges && repo_results.len() > 1 {
        let repo_names: Vec<&str> = repo_results
            .iter()
            .filter(|r| r.node_count > 0)
            .map(|r| r.name.as_str())
            .collect();
        l7_detect_cross_repo_edges(&merged, &repo_names)
    } else {
        vec![]
    };

    // Add detected cross-repo edges to the merged graph
    for cr_edge in &cross_repo_edges {
        if let (Some(src), Some(tgt)) = (
            merged.resolve_id(&cr_edge.source_node),
            merged.resolve_id(&cr_edge.target_node),
        ) {
            let _ = merged.add_edge(
                src,
                tgt,
                &cr_edge.relation,
                FiniteF32::new(cr_edge.weight),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(cr_edge.causal_strength),
            );
        }
    }

    // =========================================================================
    // Step 4: Finalize + store in session state
    // =========================================================================
    if merged.num_nodes() > 0 && !merged.finalized {
        merged.finalize()?;
    }

    let total_nodes = merged.num_nodes();
    let total_edges = merged.num_edges() as u64;
    let cross_repo_edge_count = cross_repo_edges.len();

    {
        let mut graph = state.graph.write();
        *graph = merged;
    }

    state.rebuild_engines()?;

    if let Err(e) = state.persist() {
        eprintln!(
            "[m1nd-federate] auto-persist after federation failed: {}",
            e
        );
    }

    Ok(layers::FederateOutput {
        repos_ingested: repo_results,
        total_nodes,
        total_edges,
        cross_repo_edges,
        cross_repo_edge_count,
        incremental: input.incremental,
        skipped_repos,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

// =========================================================================
// L7 Helper Functions — Federation internals
// All prefixed with `l7_` to avoid collisions with L2/L3 helpers.
// =========================================================================

/// Rebuild a graph with all node external_ids prefixed by `{repo_name}::`.
/// E.g. "file::backend/config.py" -> "my-repo::file::backend/config.py"
/// Must happen BEFORE merge so nodes from different repos never collide.
fn l7_prefix_graph_nodes(
    source: &m1nd_core::graph::Graph,
    repo_name: &str,
) -> M1ndResult<m1nd_core::graph::Graph> {
    use m1nd_core::graph::{Graph, NodeProvenanceInput};

    let num_nodes = source.num_nodes() as usize;
    let num_edges = source.num_edges();
    let mut target = Graph::with_capacity(num_nodes, num_edges);

    let source_ext_ids = l7_graph_external_ids(source);

    // Add all nodes with prefixed external_ids
    #[allow(clippy::needless_range_loop)]
    for idx in 0..num_nodes {
        let old_ext_id = &source_ext_ids[idx];
        let new_ext_id = format!("{}::{}", repo_name, old_ext_id);
        let label = source.strings.resolve(source.nodes.label[idx]).to_string();
        let tags: Vec<String> = source.nodes.tags[idx]
            .iter()
            .map(|&tag| source.strings.resolve(tag).to_string())
            .collect();
        let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();

        let node_id = target.add_node(
            &new_ext_id,
            &label,
            source.nodes.node_type[idx],
            &tag_refs,
            source.nodes.last_modified[idx],
            source.nodes.change_frequency[idx].get(),
        )?;

        let prov = source.resolve_node_provenance(NodeId::new(idx as u32));
        target.set_node_provenance(
            node_id,
            NodeProvenanceInput {
                source_path: prov.source_path.as_deref(),
                line_start: prov.line_start,
                line_end: prov.line_end,
                excerpt: prov.excerpt.as_deref(),
                namespace: Some(repo_name),
                canonical: prov.canonical,
            },
        );
    }

    // Add edges — reconstruct from CSR for finalized graphs, pending_edges otherwise
    if source.finalized {
        for src_idx in 0..num_nodes {
            for edge_pos in source.csr.out_range(NodeId::new(src_idx as u32)) {
                let tgt_node = source.csr.targets[edge_pos];
                let direction = source.csr.directions[edge_pos];
                // Bidirectional: only emit once (src < tgt)
                if direction == EdgeDirection::Bidirectional && src_idx > tgt_node.as_usize() {
                    continue;
                }

                let relation = source
                    .strings
                    .resolve(source.csr.relations[edge_pos])
                    .to_string();
                let weight = source.csr.read_weight(EdgeIdx::new(edge_pos as u32)).get();
                let causal = source.csr.causal_strengths[edge_pos].get();
                let inhibitory = source.csr.inhibitory[edge_pos];

                let new_src_id = format!("{}::{}", repo_name, &source_ext_ids[src_idx]);
                let new_tgt_id = format!("{}::{}", repo_name, &source_ext_ids[tgt_node.as_usize()]);

                if let (Some(src), Some(tgt)) = (
                    target.resolve_id(&new_src_id),
                    target.resolve_id(&new_tgt_id),
                ) {
                    let _ = target.add_edge(
                        src,
                        tgt,
                        &relation,
                        FiniteF32::new(weight),
                        direction,
                        inhibitory,
                        FiniteF32::new(causal),
                    );
                }
            }
        }
    } else {
        for edge in &source.csr.pending_edges {
            let new_src_id = format!("{}::{}", repo_name, &source_ext_ids[edge.source.as_usize()]);
            let new_tgt_id = format!("{}::{}", repo_name, &source_ext_ids[edge.target.as_usize()]);
            if let (Some(src), Some(tgt)) = (
                target.resolve_id(&new_src_id),
                target.resolve_id(&new_tgt_id),
            ) {
                let _ = target.add_edge(
                    src,
                    tgt,
                    source.strings.resolve(edge.relation),
                    edge.weight,
                    edge.direction,
                    edge.inhibitory,
                    edge.causal_strength,
                );
            }
        }
    }

    if target.num_nodes() > 0 {
        target.finalize()?;
    }
    Ok(target)
}

/// Extract external_id strings for every node, indexed by node position.
fn l7_graph_external_ids(graph: &m1nd_core::graph::Graph) -> Vec<String> {
    let mut ids = vec![String::new(); graph.num_nodes() as usize];
    for (interned, &node_id) in &graph.id_to_node {
        let idx = node_id.as_usize();
        if idx < ids.len() {
            ids[idx] = graph.strings.resolve(*interned).to_string();
        }
    }
    ids
}

/// Detect cross-repo edges between nodes in a unified federated graph.
fn l7_detect_cross_repo_edges(
    graph: &m1nd_core::graph::Graph,
    repo_names: &[&str],
) -> Vec<layers::FederateCrossRepoEdge> {
    let mut edges: Vec<layers::FederateCrossRepoEdge> = Vec::new();
    let ext_ids = l7_graph_external_ids(graph);

    // Build per-repo node index: repo -> [(idx, ext_id, label)]
    let mut repo_nodes: HashMap<String, Vec<(usize, String, String)>> = HashMap::new();
    for (idx, ext_id) in ext_ids.iter().enumerate() {
        for &repo in repo_names {
            let prefix = format!("{}::", repo);
            if ext_id.starts_with(&prefix) {
                let label = graph.strings.resolve(graph.nodes.label[idx]).to_string();
                repo_nodes
                    .entry(repo.to_string())
                    .or_default()
                    .push((idx, ext_id.clone(), label));
                break;
            }
        }
    }

    if repo_nodes.keys().count() < 2 {
        return edges;
    }

    l7_detect_shared_config(&repo_nodes, &mut edges);
    l7_detect_api_contract(&repo_nodes, &mut edges);
    l7_detect_package_dep(&repo_nodes, repo_names, &mut edges);
    l7_detect_shared_type(graph, &repo_nodes, &mut edges);
    l7_detect_deployment_dep(&repo_nodes, &mut edges);
    l7_detect_mcp_contract(&repo_nodes, &mut edges);

    edges
}

// --- Type 1: Shared Config ---
fn l7_detect_shared_config(
    repo_nodes: &HashMap<String, Vec<(usize, String, String)>>,
    edges: &mut Vec<layers::FederateCrossRepoEdge>,
) {
    let mut config_labels: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (repo, nodes) in repo_nodes {
        for (_idx, ext_id, label) in nodes {
            let is_config = label.contains("ENV")
                || label.contains("VITE_")
                || ext_id.contains(".env")
                || ext_id.contains("config")
                || ext_id.contains("settings");
            if is_config {
                config_labels
                    .entry(label.to_uppercase())
                    .or_default()
                    .push((repo.clone(), ext_id.clone()));
            }
        }
    }
    for (label, occs) in &config_labels {
        if occs.len() < 2 {
            continue;
        }
        for i in 0..occs.len() {
            for j in (i + 1)..occs.len() {
                if occs[i].0 == occs[j].0 {
                    continue;
                }
                edges.push(layers::FederateCrossRepoEdge {
                    source_repo: occs[i].0.clone(),
                    target_repo: occs[j].0.clone(),
                    source_node: occs[i].1.clone(),
                    target_node: occs[j].1.clone(),
                    edge_type: "shared_config".into(),
                    relation: format!("shares_config::{}", label),
                    weight: 0.7,
                    causal_strength: 0.8,
                });
            }
        }
    }
}

// --- Type 2: API Contract ---
fn l7_detect_api_contract(
    repo_nodes: &HashMap<String, Vec<(usize, String, String)>>,
    edges: &mut Vec<layers::FederateCrossRepoEdge>,
) {
    let mut api_patterns: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (repo, nodes) in repo_nodes {
        for (_idx, ext_id, label) in nodes {
            let text = format!("{} {}", label, ext_id);
            for segment in text.split_whitespace() {
                if segment.starts_with("/api/") || segment.starts_with("api/") {
                    let normalized = l7_normalize_api_route(segment);
                    api_patterns
                        .entry(normalized)
                        .or_default()
                        .push((repo.clone(), ext_id.clone()));
                }
            }
        }
    }
    for (route, occs) in &api_patterns {
        if occs.len() < 2 {
            continue;
        }
        for i in 0..occs.len() {
            for j in (i + 1)..occs.len() {
                if occs[i].0 == occs[j].0 {
                    continue;
                }
                edges.push(layers::FederateCrossRepoEdge {
                    source_repo: occs[i].0.clone(),
                    target_repo: occs[j].0.clone(),
                    source_node: occs[i].1.clone(),
                    target_node: occs[j].1.clone(),
                    edge_type: "api_contract".into(),
                    relation: format!("api_contract::{}", route),
                    weight: 0.8,
                    causal_strength: 0.9,
                });
            }
        }
    }
}

fn l7_normalize_api_route(route: &str) -> String {
    let mut n = route.to_lowercase();
    if n.ends_with('/') {
        n.pop();
    }
    n.split('/')
        .map(|p| {
            if p.starts_with('{') && p.ends_with('}') {
                "_".to_string()
            } else {
                p.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

// --- Type 3: Package Dependency ---
fn l7_detect_package_dep(
    repo_nodes: &HashMap<String, Vec<(usize, String, String)>>,
    repo_names: &[&str],
    edges: &mut Vec<layers::FederateCrossRepoEdge>,
) {
    for (repo_a, nodes_a) in repo_nodes {
        for (_idx, ext_id_a, label_a) in nodes_a {
            for &repo_b in repo_names {
                if repo_a == repo_b {
                    continue;
                }
                let variants = l7_repo_name_variants(repo_b);
                let text = format!("{} {}", label_a, ext_id_a).to_lowercase();
                for variant in &variants {
                    if text.contains(variant) {
                        if let Some(nodes_b) = repo_nodes.get(repo_b) {
                            if let Some((_, ext_id_b, _)) = nodes_b.first() {
                                edges.push(layers::FederateCrossRepoEdge {
                                    source_repo: repo_a.clone(),
                                    target_repo: repo_b.to_string(),
                                    source_node: ext_id_a.clone(),
                                    target_node: ext_id_b.clone(),
                                    edge_type: "package_dep".into(),
                                    relation: format!("depends_on::{}", repo_b),
                                    weight: 0.6,
                                    causal_strength: 0.7,
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn l7_repo_name_variants(name: &str) -> Vec<String> {
    let lower = name.to_lowercase();
    let underscore = lower.replace('-', "_");
    let hyphen = lower.replace('_', "-");
    let mut v = vec![lower.clone()];
    if underscore != lower {
        v.push(underscore);
    }
    if hyphen != lower {
        v.push(hyphen);
    }
    v
}

// --- Type 4: Shared Type ---
fn l7_detect_shared_type(
    graph: &m1nd_core::graph::Graph,
    repo_nodes: &HashMap<String, Vec<(usize, String, String)>>,
    edges: &mut Vec<layers::FederateCrossRepoEdge>,
) {
    let common_exclusions = [
        "Config", "Error", "Result", "Status", "State", "Context", "Request", "Response",
        "Handler", "Manager", "Service", "Client", "Server", "Base", "Default", "Node", "Edge",
    ];
    let mut type_defs: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (repo, nodes) in repo_nodes {
        for (idx, ext_id, label) in nodes {
            let nt = graph.nodes.node_type[*idx];
            if !matches!(
                nt,
                NodeType::Class | NodeType::Struct | NodeType::Type | NodeType::Enum
            ) {
                continue;
            }
            if common_exclusions.iter().any(|&e| label == e) {
                continue;
            }
            if label.len() < 4 {
                continue;
            }
            type_defs
                .entry(label.clone())
                .or_default()
                .push((repo.clone(), ext_id.clone()));
        }
    }
    for (type_name, occs) in &type_defs {
        if occs.len() < 2 {
            continue;
        }
        for i in 0..occs.len() {
            for j in (i + 1)..occs.len() {
                if occs[i].0 == occs[j].0 {
                    continue;
                }
                edges.push(layers::FederateCrossRepoEdge {
                    source_repo: occs[i].0.clone(),
                    target_repo: occs[j].0.clone(),
                    source_node: occs[i].1.clone(),
                    target_node: occs[j].1.clone(),
                    edge_type: "shared_type".into(),
                    relation: format!("shared_type::{}", type_name),
                    weight: 0.5,
                    causal_strength: 0.6,
                });
            }
        }
    }
}

// --- Type 5: Deployment Dependency ---
fn l7_detect_deployment_dep(
    repo_nodes: &HashMap<String, Vec<(usize, String, String)>>,
    edges: &mut Vec<layers::FederateCrossRepoEdge>,
) {
    let deploy_patterns = [
        "docker",
        "compose",
        "dockerfile",
        "kubernetes",
        "k8s",
        "ci",
        "deploy",
    ];
    for (repo_a, nodes_a) in repo_nodes {
        for (_idx, ext_id_a, label_a) in nodes_a {
            let ext_lower = ext_id_a.to_lowercase();
            if !deploy_patterns.iter().any(|p| ext_lower.contains(p)) {
                continue;
            }
            for (repo_b, nodes_b) in repo_nodes {
                if repo_a == repo_b {
                    continue;
                }
                let variants = l7_repo_name_variants(repo_b);
                let text = format!("{} {}", label_a, ext_id_a).to_lowercase();
                for variant in &variants {
                    if text.contains(variant) {
                        if let Some((_, ext_id_b, _)) = nodes_b.first() {
                            edges.push(layers::FederateCrossRepoEdge {
                                source_repo: repo_a.clone(),
                                target_repo: repo_b.clone(),
                                source_node: ext_id_a.clone(),
                                target_node: ext_id_b.clone(),
                                edge_type: "deployment_dep".into(),
                                relation: format!("deploys::{}", repo_b),
                                weight: 0.4,
                                causal_strength: 0.5,
                            });
                            break;
                        }
                    }
                }
            }
        }
    }
}

// --- Type 6: MCP Contract ---
fn l7_detect_mcp_contract(
    repo_nodes: &HashMap<String, Vec<(usize, String, String)>>,
    edges: &mut Vec<layers::FederateCrossRepoEdge>,
) {
    let mut mcp_providers: Vec<(String, String)> = Vec::new();
    let mut mcp_consumers: Vec<(String, String)> = Vec::new();
    for (repo, nodes) in repo_nodes {
        for (_idx, ext_id, label) in nodes {
            let text = format!("{} {}", label, ext_id).to_lowercase();
            if text.contains("mcp")
                && (text.contains("server") || text.contains("handler") || text.contains("tool"))
            {
                mcp_providers.push((repo.clone(), ext_id.clone()));
            }
            if text.contains("mcp__") || text.contains("mcp_config") || text.contains("mcp-config")
            {
                mcp_consumers.push((repo.clone(), ext_id.clone()));
            }
        }
    }
    for (repo_p, ext_p) in &mcp_providers {
        for (repo_c, ext_c) in &mcp_consumers {
            if repo_p == repo_c {
                continue;
            }
            edges.push(layers::FederateCrossRepoEdge {
                source_repo: repo_c.clone(),
                target_repo: repo_p.clone(),
                source_node: ext_c.clone(),
                target_node: ext_p.clone(),
                edge_type: "mcp_contract".into(),
                relation: "uses_mcp_tool".into(),
                weight: 0.7,
                causal_strength: 0.8,
            });
        }
    }
}

// =========================================================================
// L2 Helper Functions — Semantic Search internals
// All prefixed with `l2_` to avoid collisions with L3/L7 helpers.
// V2 upgrade path: replace trigram_similarity with fastembed-rs cosine,
//   replace scan keyword matching with ast-grep-core structural patterns.
// =========================================================================

/// Tokenize a query: lowercase, split on whitespace/punctuation, filter short tokens.
fn l2_seek_tokenize(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for raw in query.to_lowercase().split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '?' | '!' | '.' | ',' | ':' | ';' | '(' | ')' | '{' | '}' | '[' | ']'
            )
    }) {
        let trimmed = raw.trim_matches(|c: char| matches!(c, '"' | '\'' | '`'));
        if trimmed.len() <= 2 || L2_SEEK_STOPWORDS.contains(&trimmed) {
            continue;
        }
        if !tokens.iter().any(|existing| existing == trimmed) {
            tokens.push(trimmed.to_string());
        }
        for part in l2_split_identifier(trimmed) {
            if part.len() > 2
                && !L2_SEEK_STOPWORDS.contains(&part.as_str())
                && !tokens.iter().any(|existing| existing == &part)
            {
                tokens.push(part);
            }
        }
    }
    tokens
}

/// Split a camelCase/snake_case identifier into lowercase sub-tokens.
fn l2_split_identifier(ident: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for part in ident.split(['_', '-', '/', '\\', ':']) {
        if part.is_empty() {
            continue;
        }
        let mut current = String::new();
        for ch in part.chars() {
            if ch.is_uppercase() && !current.is_empty() {
                tokens.push(current.to_lowercase());
                current = String::new();
            }
            current.push(ch);
        }
        if !current.is_empty() {
            tokens.push(current.to_lowercase());
        }
    }
    tokens
}

fn l2_seek_anchor_bias(
    query_tokens: &[String],
    label_lower: &str,
    source_path_lower: &str,
    tag_terms: &[String],
    node_type: &str,
) -> f32 {
    const DISPATCH_CLUSTER: &[&str] = &["alias", "canonical", "dispatch", "status", "tool", "name"];

    let query_anchor_hits: Vec<&str> = DISPATCH_CLUSTER
        .iter()
        .copied()
        .filter(|term| query_tokens.iter().any(|token| token == term))
        .collect();
    if query_anchor_hits.len() < 3 {
        return 0.0;
    }

    let mut matched = 0usize;
    for term in &query_anchor_hits {
        let tag_match = tag_terms
            .iter()
            .any(|tag| tag == term || tag.contains(term));
        if label_lower.contains(term) || source_path_lower.contains(term) || tag_match {
            matched += 1;
        }
    }

    if matched == 0 {
        return 0.0;
    }

    let coverage = matched as f32 / query_anchor_hits.len() as f32;
    let type_bias = match node_type {
        "function" => 0.06,
        "module" | "file" => 0.03,
        _ => 0.0,
    };
    let code_path_bias =
        if source_path_lower.contains("/src/") || source_path_lower.contains("src/") {
            0.02
        } else {
            0.0
        };
    let docs_penalty = if source_path_lower.contains("/docs/") || source_path_lower.ends_with(".md")
    {
        -0.04
    } else {
        0.0
    };

    (coverage * 0.12 + type_bias + code_path_bias + docs_penalty).max(0.0)
}

/// Trigram cosine similarity between two strings.
fn l2_trigram_similarity(a: &str, b: &str) -> f32 {
    let al = a.to_lowercase();
    let bl = b.to_lowercase();
    let ab = al.as_bytes();
    let bb = bl.as_bytes();
    if ab.len() < 3 || bb.len() < 3 {
        return 0.0;
    }

    let ta: Vec<[u8; 3]> = ab.windows(3).map(|w| [w[0], w[1], w[2]]).collect();
    let tb: Vec<[u8; 3]> = bb.windows(3).map(|w| [w[0], w[1], w[2]]).collect();

    let mut hits = 0usize;
    for t in &ta {
        if tb.contains(t) {
            hits += 1;
        }
    }
    if hits == 0 {
        return 0.0;
    }
    hits as f32 / ((ta.len() as f32).sqrt() * (tb.len() as f32).sqrt())
}

/// Map NodeType enum to a lowercase string for filtering and display.
fn l2_node_type_str(nt: &NodeType) -> &'static str {
    match nt {
        NodeType::File => "file",
        NodeType::Directory => "directory",
        NodeType::Function => "function",
        NodeType::Class => "class",
        NodeType::Struct => "struct",
        NodeType::Enum => "enum",
        NodeType::Type => "type",
        NodeType::Module => "module",
        NodeType::Reference => "reference",
        NodeType::Concept => "concept",
        NodeType::Material => "material",
        NodeType::Process => "process",
        NodeType::Product => "product",
        NodeType::Supplier => "supplier",
        NodeType::Regulatory => "regulatory",
        NodeType::System => "system",
        NodeType::Cost => "cost",
        NodeType::Custom(_) => "custom",
    }
}

/// Generate a heuristic intent summary from a node's label, type, and tags.
fn l2_intent_summary(label: &str, node_type: &str, tags: &[String]) -> String {
    if tags.is_empty() {
        format!("{} ({})", label, node_type)
    } else {
        format!("{} ({}) [{}]", label, node_type, tags.join(", "))
    }
}

// ---------------------------------------------------------------------------
// L2 Scan: predefined pattern definitions
// ---------------------------------------------------------------------------

/// A predefined scan pattern with label-matching heuristics.
struct L2ScanPattern {
    id: &'static str,
    label_keywords: &'static [&'static str],
    negation_keywords: &'static [&'static str],
    base_severity: f32,
    message_template: &'static str,
}

const L2_SCAN_PATTERNS: &[L2ScanPattern] = &[
    L2ScanPattern {
        id: "error_handling",
        label_keywords: &[
            "error",
            "exception",
            "panic",
            "unwrap",
            "expect",
            "catch",
            "raise",
            "throw",
        ],
        negation_keywords: &["test_error", "error_test", "mock_error"],
        base_severity: 0.6,
        message_template: "Potential error handling concern: node uses error-related pattern",
    },
    L2ScanPattern {
        id: "resource_cleanup",
        label_keywords: &[
            "open",
            "connect",
            "acquire",
            "lock",
            "socket",
            "file_handle",
            "cursor",
            "session",
        ],
        negation_keywords: &["close", "release", "cleanup", "dispose", "drop", "__exit__"],
        base_severity: 0.5,
        message_template: "Resource acquisition without visible cleanup in nearby graph structure",
    },
    L2ScanPattern {
        id: "api_surface",
        label_keywords: &[
            "route",
            "endpoint",
            "handler",
            "api",
            "router",
            "view",
            "controller",
        ],
        negation_keywords: &[],
        base_severity: 0.4,
        message_template: "API surface node -- verify auth, validation, and rate limiting coverage",
    },
    L2ScanPattern {
        id: "state_mutation",
        label_keywords: &[
            "set_", "update_", "mutate", "write", "delete", "remove", "insert", "push", "pop",
            "modify",
        ],
        negation_keywords: &["get_", "read_", "fetch_", "list_"],
        base_severity: 0.5,
        message_template:
            "State mutation detected -- verify transaction safety and concurrent access",
    },
    L2ScanPattern {
        id: "concurrency",
        label_keywords: &[
            "async",
            "await",
            "thread",
            "lock",
            "mutex",
            "semaphore",
            "atomic",
            "spawn",
            "pool",
            "queue",
        ],
        negation_keywords: &["test_async", "mock_thread"],
        base_severity: 0.7,
        message_template:
            "Concurrency primitive usage -- verify deadlock safety and proper synchronization",
    },
    L2ScanPattern {
        id: "auth_boundary",
        label_keywords: &[
            "auth",
            "login",
            "token",
            "session",
            "permission",
            "credential",
            "password",
            "secret",
            "jwt",
            "oauth",
        ],
        negation_keywords: &["test_auth", "mock_auth"],
        base_severity: 0.8,
        message_template: "Auth boundary -- verify token validation and access control",
    },
    L2ScanPattern {
        id: "test_coverage",
        label_keywords: &[
            "test_", "spec_", "_test", "_spec", "assert", "expect", "should",
        ],
        negation_keywords: &[],
        base_severity: 0.3,
        message_template: "Test node -- check coverage completeness for related production code",
    },
    L2ScanPattern {
        id: "dependency_injection",
        label_keywords: &[
            "inject",
            "provider",
            "factory",
            "registry",
            "container",
            "config",
            "settings",
            "env",
        ],
        negation_keywords: &[],
        base_severity: 0.4,
        message_template:
            "Dependency/config injection point -- verify indirection and override safety",
    },
];

/// Find a predefined scan pattern by ID.
fn l2_find_scan_pattern(pattern_id: &str) -> Option<&'static L2ScanPattern> {
    L2_SCAN_PATTERNS.iter().find(|p| p.id == pattern_id)
}

/// Graph validation for a scan finding.
/// Checks if connected nodes mitigate the issue (negation keywords, test nodes).
/// Returns (status, context_nodes).
fn l2_graph_validate(
    graph: &m1nd_core::graph::Graph,
    node: NodeId,
    negation_keywords: &[&str],
    n: usize,
    node_to_ext: &[String],
) -> (&'static str, Vec<layers::ScanContextNode>) {
    let mut context = Vec::new();
    if !graph.finalized {
        return ("confirmed", context);
    }
    let idx = node.as_usize();
    if idx >= n {
        return ("confirmed", context);
    }

    let mut has_mitigation = false;

    // Check outgoing edges
    let out = graph.csr.out_range(node);
    for j in out {
        let target = graph.csr.targets[j];
        let tidx = target.as_usize();
        if tidx >= n {
            continue;
        }

        let target_label = graph
            .strings
            .resolve(graph.nodes.label[tidx])
            .to_lowercase();
        let relation = graph.strings.resolve(graph.csr.relations[j]).to_string();

        let target_is_test = target_label.starts_with("test_") || target_label.contains("_test");
        let negates = negation_keywords.iter().any(|nk| target_label.contains(nk));

        if negates || target_is_test {
            has_mitigation = true;
            let tid = if !node_to_ext[tidx].is_empty() {
                node_to_ext[tidx].clone()
            } else {
                target_label.clone()
            };
            context.push(layers::ScanContextNode {
                node_id: tid,
                label: target_label,
                relation,
            });
        }
        if context.len() >= 3 {
            break;
        }
    }

    // Check incoming edges
    if !has_mitigation {
        let in_range = graph.csr.in_range(node);
        for j in in_range {
            let source = graph.csr.rev_sources[j];
            let sidx = source.as_usize();
            if sidx >= n {
                continue;
            }

            let source_label = graph
                .strings
                .resolve(graph.nodes.label[sidx])
                .to_lowercase();
            let edge_idx = graph.csr.rev_edge_idx[j];
            let relation = graph
                .strings
                .resolve(graph.csr.relations[edge_idx.as_usize()])
                .to_string();

            let source_is_test =
                source_label.starts_with("test_") || source_label.contains("_test");
            let negates = negation_keywords.iter().any(|nk| source_label.contains(nk));

            if negates || source_is_test {
                has_mitigation = true;
                let sid = if !node_to_ext[sidx].is_empty() {
                    node_to_ext[sidx].clone()
                } else {
                    source_label.clone()
                };
                context.push(layers::ScanContextNode {
                    node_id: sid,
                    label: source_label,
                    relation,
                });
            }
            if context.len() >= 3 {
                break;
            }
        }
    }

    if has_mitigation {
        ("mitigated", context)
    } else {
        ("confirmed", context)
    }
}

// =========================================================================
// L5 Helpers: Hypothesis Engine
// =========================================================================

#[derive(Debug, Clone, PartialEq)]
enum L5ClaimType {
    NeverCalls,
    AlwaysBefore,
    DependsOn,
    NoDependency,
    Coupling,
    Isolated,
    Gateway,
    Circular,
    Unknown,
}

impl L5ClaimType {
    fn as_str(&self) -> &'static str {
        match self {
            L5ClaimType::NeverCalls => "never_calls",
            L5ClaimType::AlwaysBefore => "always_before",
            L5ClaimType::DependsOn => "depends_on",
            L5ClaimType::NoDependency => "no_dependency",
            L5ClaimType::Coupling => "coupling",
            L5ClaimType::Isolated => "isolated",
            L5ClaimType::Gateway => "gateway",
            L5ClaimType::Circular => "circular",
            L5ClaimType::Unknown => "unknown",
        }
    }
}

struct L5ParsedClaim {
    claim_type: L5ClaimType,
    subject: String,
    object: String,
}

/// Parse a natural language claim into a typed pattern with subject/object.
fn l5_parse_claim(claim: &str) -> L5ParsedClaim {
    let lower = claim.to_lowercase();
    let lower = lower.trim();

    // NEVER_CALLS
    if let Some((s, o)) = l5_extract_binary(
        lower,
        &[
            "never calls",
            "never imports",
            "does not call",
            "doesn't call",
            "never touches",
            "never invokes",
            "does not import",
            "doesn't import",
            "never uses",
            "does not use",
            "doesn't use",
            "has no connection to",
        ],
    ) {
        return L5ParsedClaim {
            claim_type: L5ClaimType::NeverCalls,
            subject: s,
            object: o,
        };
    }
    // NO_DEPENDENCY
    if let Some((s, o)) = l5_extract_binary(
        lower,
        &[
            "is independent of",
            "has no dependency on",
            "does not depend on",
            "doesn't depend on",
            "is separate from",
            "is decoupled from",
        ],
    ) {
        return L5ParsedClaim {
            claim_type: L5ClaimType::NoDependency,
            subject: s,
            object: o,
        };
    }
    // DEPENDS_ON
    if let Some((s, o)) = l5_extract_binary(
        lower,
        &[
            "depends on",
            "requires",
            "imports",
            "calls",
            "uses",
            "invokes",
            "references",
            "relies on",
        ],
    ) {
        return L5ParsedClaim {
            claim_type: L5ClaimType::DependsOn,
            subject: s,
            object: o,
        };
    }
    // COUPLING
    if lower.contains("coupled")
        || lower.contains("tightly connected")
        || lower.contains("co-change")
    {
        if let Some((s, o)) = l5_extract_and_pair(lower) {
            return L5ParsedClaim {
                claim_type: L5ClaimType::Coupling,
                subject: s,
                object: o,
            };
        }
        if let Some((s, o)) = l5_extract_binary(
            lower,
            &[
                "is coupled to",
                "is coupled with",
                "is tightly connected to",
            ],
        ) {
            return L5ParsedClaim {
                claim_type: L5ClaimType::Coupling,
                subject: s,
                object: o,
            };
        }
    }
    // CIRCULAR
    if lower.contains("circular") || lower.contains("cycle") || lower.contains("cyclic") {
        if let Some((s, o)) = l5_extract_and_pair(lower) {
            return L5ParsedClaim {
                claim_type: L5ClaimType::Circular,
                subject: s,
                object: o,
            };
        }
        if let Some((s, o)) = l5_extract_binary(
            lower,
            &[
                "has circular dependency with",
                "has a cycle with",
                "has cyclic dependency with",
            ],
        ) {
            return L5ParsedClaim {
                claim_type: L5ClaimType::Circular,
                subject: s,
                object: o,
            };
        }
        if let Some(pos) = lower.find("between ") {
            let rest = &lower[pos + 8..];
            if let Some((s, o)) = l5_extract_and_pair(rest) {
                return L5ParsedClaim {
                    claim_type: L5ClaimType::Circular,
                    subject: s,
                    object: o,
                };
            }
        }
    }
    // GATEWAY
    if lower.contains("gateway") || lower.contains("bottleneck") || lower.contains("choke point") {
        return L5ParsedClaim {
            claim_type: L5ClaimType::Gateway,
            subject: l5_extract_unary_subject(lower),
            object: String::new(),
        };
    }
    if let Some((s, o)) = l5_extract_binary(lower, &["go through", "pass through", "route through"])
    {
        return L5ParsedClaim {
            claim_type: L5ClaimType::Gateway,
            subject: o,
            object: s,
        };
    }
    // ALWAYS_BEFORE
    if let Some((s, o)) = l5_extract_binary(
        lower,
        &[
            "always runs before",
            "is always called before",
            "always precedes",
            "runs before",
            "precedes",
            "executes before",
        ],
    ) {
        return L5ParsedClaim {
            claim_type: L5ClaimType::AlwaysBefore,
            subject: s,
            object: o,
        };
    }
    // ISOLATED
    if lower.contains("isolated")
        || lower.contains("no dependencies")
        || lower.contains("standalone")
        || lower.contains("self-contained")
    {
        return L5ParsedClaim {
            claim_type: L5ClaimType::Isolated,
            subject: l5_extract_unary_subject(lower),
            object: String::new(),
        };
    }

    // Unknown fallback
    let parts: Vec<&str> = lower.split_whitespace().collect();
    let (s, o) = if parts.len() >= 3 {
        (parts[0].to_string(), parts[parts.len() - 1].to_string())
    } else if !parts.is_empty() {
        (parts[0].to_string(), String::new())
    } else {
        (claim.to_string(), String::new())
    };
    L5ParsedClaim {
        claim_type: L5ClaimType::Unknown,
        subject: s,
        object: o,
    }
}

fn l5_extract_binary(text: &str, patterns: &[&str]) -> Option<(String, String)> {
    for &pat in patterns {
        if let Some(pos) = text.find(pat) {
            let subj = text[..pos].trim();
            let obj = text[pos + pat.len()..].trim();
            if !subj.is_empty() && !obj.is_empty() {
                return Some((l5_clean(subj), l5_clean(obj)));
            }
        }
    }
    None
}

fn l5_extract_and_pair(text: &str) -> Option<(String, String)> {
    if let Some(pos) = text.find(" and ") {
        let subj = text[..pos].trim();
        let obj = text[pos + 5..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim();
        if !subj.is_empty() && !obj.is_empty() {
            return Some((l5_clean(subj), l5_clean(obj)));
        }
    }
    None
}

fn l5_extract_unary_subject(text: &str) -> String {
    let t = text.trim_start_matches("all ").trim_start_matches("every ");
    for m in &[" is ", " has ", " are ", " should "] {
        if let Some(pos) = t.find(m) {
            let s = t[..pos].trim();
            if !s.is_empty() {
                return l5_clean(s);
            }
        }
    }
    l5_clean(t.split_whitespace().next().unwrap_or(t))
}

fn l5_clean(name: &str) -> String {
    name.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
        .trim_start_matches("the ")
        .trim_start_matches("a ")
        .trim_start_matches("all ")
        .trim()
        .to_string()
}

/// Resolve a claim node name to NodeIds. Tries exact, prefixed, then fuzzy.
fn l5_resolve_claim_nodes(graph: &m1nd_core::graph::Graph, name: &str) -> Vec<NodeId> {
    if name.is_empty() {
        return vec![];
    }

    if let Some(nid) = graph.resolve_id(name) {
        return vec![nid];
    }

    for prefix in &[
        "file::",
        "file::backend/",
        "file::backend/{}.py",
        "fn::",
        "class::",
        "mod::",
    ] {
        let try_id = if prefix.contains("{}") {
            prefix.replace("{}", name)
        } else {
            format!("{}{}", prefix, name)
        };
        if let Some(nid) = graph.resolve_id(&try_id) {
            return vec![nid];
        }
    }

    match m1nd_core::seed::SeedFinder::find_seeds(graph, name, 3) {
        Ok(seeds) if !seeds.is_empty() => seeds.into_iter().map(|(nid, _)| nid).collect(),
        _ => vec![],
    }
}

fn l5_build_node_to_ext_map(graph: &m1nd_core::graph::Graph) -> Vec<String> {
    let n = graph.num_nodes() as usize;
    let mut map = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            map[idx] = graph.strings.resolve(interned).to_string();
        }
    }
    for (i, entry) in map.iter_mut().enumerate().take(n) {
        if entry.is_empty() {
            *entry = graph.strings.resolve(graph.nodes.label[i]).to_string();
        }
    }
    map
}

/// Budget-capped BFS result.
struct L5BfsResult {
    found: bool,
    path_nodes: Vec<String>,
    path_rels: Vec<String>,
    total_weight: f32,
    explored: usize,
    partial: Vec<layers::PartialReachEntry>,
}

fn l5_bfs_path(
    graph: &m1nd_core::graph::Graph,
    source: NodeId,
    target: NodeId,
    max_hops: usize,
    budget: usize,
    node_to_ext: &[String],
) -> L5BfsResult {
    use std::collections::VecDeque;
    let n = graph.num_nodes() as usize;
    if source == target {
        return L5BfsResult {
            found: true,
            path_nodes: vec![node_to_ext[source.as_usize()].clone()],
            path_rels: vec![],
            total_weight: 0.0,
            explored: 0,
            partial: vec![],
        };
    }
    let mut parent: Vec<Option<(usize, usize)>> = vec![None; n];
    let mut visited = vec![false; n];
    let mut depth_at = vec![0usize; n];
    let mut queue = VecDeque::new();
    let mut explored = 0usize;

    visited[source.as_usize()] = true;
    queue.push_back(source);

    let mut found = false;
    while let Some(node) = queue.pop_front() {
        if node == target {
            found = true;
            break;
        }
        let d = depth_at[node.as_usize()];
        if d >= max_hops || explored >= budget {
            continue;
        }

        for j in graph.csr.out_range(node) {
            explored += 1;
            let tgt = graph.csr.targets[j];
            let ti = tgt.as_usize();
            if ti < n && !visited[ti] {
                visited[ti] = true;
                parent[ti] = Some((node.as_usize(), j));
                depth_at[ti] = d + 1;
                queue.push_back(tgt);
            }
            if explored >= budget {
                break;
            }
        }
        for j in graph.csr.in_range(node) {
            explored += 1;
            let src = graph.csr.rev_sources[j];
            let si = src.as_usize();
            let fwd = graph.csr.rev_edge_idx[j].as_usize();
            if si < n && !visited[si] {
                visited[si] = true;
                parent[si] = Some((node.as_usize(), fwd));
                depth_at[si] = d + 1;
                queue.push_back(src);
            }
            if explored >= budget {
                break;
            }
        }
    }

    if found {
        let mut pi = vec![target.as_usize()];
        let mut ei = Vec::new();
        let mut cur = target.as_usize();
        while let Some((prev, ej)) = parent[cur] {
            pi.push(prev);
            ei.push(ej);
            cur = prev;
            if cur == source.as_usize() {
                break;
            }
        }
        pi.reverse();
        ei.reverse();
        let pn: Vec<String> = pi.iter().map(|&i| node_to_ext[i].clone()).collect();
        let pr: Vec<String> = ei
            .iter()
            .map(|&j| graph.strings.resolve(graph.csr.relations[j]).to_string())
            .collect();
        let tw: f32 = ei
            .iter()
            .map(|&j| graph.csr.read_weight(EdgeIdx::new(j as u32)).get())
            .sum();
        L5BfsResult {
            found: true,
            path_nodes: pn,
            path_rels: pr,
            total_weight: tw,
            explored,
            partial: vec![],
        }
    } else {
        let mut partial: Vec<layers::PartialReachEntry> = visited
            .iter()
            .enumerate()
            .filter(|(i, &v)| v && *i != source.as_usize())
            .map(|(i, _)| layers::PartialReachEntry {
                node_id: node_to_ext[i].clone(),
                label: graph.strings.resolve(graph.nodes.label[i]).to_string(),
                hops_from_source: depth_at[i] as u8,
                activation_at_stop: 1.0 / (1.0 + depth_at[i] as f32),
            })
            .collect();
        partial.sort_by_key(|e| e.hops_from_source);
        partial.truncate(20);
        L5BfsResult {
            found: false,
            path_nodes: vec![],
            path_rels: vec![],
            total_weight: 0.0,
            explored,
            partial,
        }
    }
}

fn l5_has_direct_edge(graph: &m1nd_core::graph::Graph, a: NodeId, b: NodeId) -> bool {
    for j in graph.csr.out_range(a) {
        if graph.csr.targets[j] == b {
            return true;
        }
    }
    for j in graph.csr.out_range(b) {
        if graph.csr.targets[j] == a {
            return true;
        }
    }
    false
}

/// BFS reachability check with RemovalMask (for gateway counterfactual).
fn l5_bfs_reachable_masked(
    graph: &m1nd_core::graph::Graph,
    target: NodeId,
    mask: &m1nd_core::counterfactual::RemovalMask,
    max_hops: usize,
) -> bool {
    use std::collections::VecDeque;
    let n = graph.num_nodes() as usize;
    let ti = target.as_usize();
    if ti >= n || mask.is_node_removed(target) {
        return false;
    }

    // BFS backwards from target
    let mut visited = vec![false; n];
    visited[ti] = true;
    let mut queue = VecDeque::new();
    queue.push_back((target, 0usize));

    while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_hops {
            continue;
        }
        for j in graph.csr.in_range(node) {
            let src = graph.csr.rev_sources[j];
            let si = src.as_usize();
            let fe = graph.csr.rev_edge_idx[j];
            if si < n && !visited[si] && !mask.is_node_removed(src) && !mask.is_edge_removed(fe) {
                visited[si] = true;
                queue.push_back((src, depth + 1));
            }
        }
    }
    visited
        .iter()
        .enumerate()
        .any(|(i, &v)| v && i != ti && !mask.is_node_removed(NodeId::new(i as u32)))
}

fn l5_bayesian_confidence(
    supporting: &[layers::HypothesisEvidence],
    contradicting: &[layers::HypothesisEvidence],
) -> f32 {
    let mut odds = 1.0f32;
    for ev in supporting {
        odds *= ev.likelihood_factor;
    }
    for ev in contradicting {
        odds *= ev.likelihood_factor;
    }
    (odds / (1.0 + odds)).clamp(0.01, 0.99)
}

// =========================================================================
// L5 Helpers: Differential
// =========================================================================

fn l5_load_snapshot_or_current(
    state: &SessionState,
    snapshot_ref: &str,
) -> M1ndResult<m1nd_core::graph::Graph> {
    if snapshot_ref == "current" || snapshot_ref.is_empty() {
        let graph = state.graph.read();
        let tmp = std::env::temp_dir().join(format!("m1nd_diff_{}.json", std::process::id()));
        m1nd_core::snapshot::save_graph(&graph, &tmp)?;
        let loaded = m1nd_core::snapshot::load_graph(&tmp)?;
        let _ = std::fs::remove_file(&tmp);
        Ok(loaded)
    } else {
        let path = Path::new(snapshot_ref);
        if path.exists() {
            m1nd_core::snapshot::load_graph(path)
        } else {
            let parent = state.graph_path.parent().unwrap_or(Path::new("."));
            let resolved = parent.join(snapshot_ref);
            if resolved.exists() {
                m1nd_core::snapshot::load_graph(&resolved)
            } else {
                Err(M1ndError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "Snapshot not found: {} (tried {})",
                        snapshot_ref,
                        resolved.display()
                    ),
                )))
            }
        }
    }
}

fn l5_collect_ext_ids(graph: &m1nd_core::graph::Graph) -> std::collections::HashSet<String> {
    graph
        .id_to_node
        .keys()
        .map(|&i| graph.strings.resolve(i).to_string())
        .collect()
}

fn l5_collect_edges(graph: &m1nd_core::graph::Graph) -> HashMap<(String, String, String), f32> {
    let n = graph.num_nodes() as usize;
    let ext = l5_build_node_to_ext_map(graph);
    let mut edges = HashMap::new();
    for src in 0..n {
        for j in graph.csr.out_range(NodeId::new(src as u32)) {
            let tgt = graph.csr.targets[j].as_usize();
            let rel = graph.strings.resolve(graph.csr.relations[j]).to_string();
            let w = graph.csr.read_weight(EdgeIdx::new(j as u32)).get();
            edges.insert((ext[src].clone(), ext[tgt].clone(), rel), w);
        }
    }
    edges
}

fn l5_coupling_deltas(
    graph_a: &m1nd_core::graph::Graph,
    graph_b: &m1nd_core::graph::Graph,
    state: &SessionState,
) -> Vec<layers::DiffCouplingDelta> {
    let ca = state.topology.community_detector.detect(graph_a);
    let cb = state.topology.community_detector.detect(graph_b);
    let mut deltas = Vec::new();

    if let (Ok(ca), Ok(cb)) = (ca, cb) {
        let coupling_a = l5_inter_community_coupling(graph_a, &ca);
        let coupling_b = l5_inter_community_coupling(graph_b, &cb);

        let nodes_a = l5_community_nodes(graph_a, &ca);
        let nodes_b = l5_community_nodes(graph_b, &cb);
        let mapping = l5_map_communities(&nodes_a, &nodes_b);

        for ((caid, cbid), (la, lb)) in &mapping {
            let old = coupling_a.get(caid).copied().unwrap_or(0.0);
            let new = coupling_b.get(cbid).copied().unwrap_or(0.0);
            let delta = new - old;
            if delta.abs() > 0.01 {
                deltas.push(layers::DiffCouplingDelta {
                    community_a: la.clone(),
                    community_b: lb.clone(),
                    old_coupling: old,
                    new_coupling: new,
                    delta,
                });
            }
        }
    }
    deltas
}

fn l5_inter_community_coupling(
    graph: &m1nd_core::graph::Graph,
    communities: &m1nd_core::topology::CommunityResult,
) -> HashMap<u32, f32> {
    let n = graph.num_nodes() as usize;
    let mut cross: HashMap<u32, u32> = HashMap::new();
    let mut total: HashMap<u32, u32> = HashMap::new();
    for src in 0..n {
        let sc = communities.assignments[src].0;
        for j in graph.csr.out_range(NodeId::new(src as u32)) {
            let tgt = graph.csr.targets[j].as_usize();
            if tgt < communities.assignments.len() {
                *total.entry(sc).or_insert(0) += 1;
                if communities.assignments[tgt].0 != sc {
                    *cross.entry(sc).or_insert(0) += 1;
                }
            }
        }
    }
    total
        .iter()
        .map(|(&c, &t)| {
            (
                c,
                if t > 0 {
                    cross.get(&c).copied().unwrap_or(0) as f32 / t as f32
                } else {
                    0.0
                },
            )
        })
        .collect()
}

fn l5_community_nodes(
    graph: &m1nd_core::graph::Graph,
    communities: &m1nd_core::topology::CommunityResult,
) -> HashMap<u32, std::collections::HashSet<String>> {
    let ext = l5_build_node_to_ext_map(graph);
    let mut sets: HashMap<u32, std::collections::HashSet<String>> = HashMap::new();
    for (i, &c) in communities.assignments.iter().enumerate() {
        sets.entry(c.0).or_default().insert(ext[i].clone());
    }
    sets
}

fn l5_map_communities(
    a: &HashMap<u32, std::collections::HashSet<String>>,
    b: &HashMap<u32, std::collections::HashSet<String>>,
) -> Vec<((u32, u32), (String, String))> {
    let mut out = Vec::new();
    for (&caid, ca_nodes) in a {
        let mut best = (0, 0u32);
        for (&cbid, cb_nodes) in b {
            let overlap = ca_nodes.intersection(cb_nodes).count();
            if overlap > best.0 {
                best = (overlap, cbid);
            }
        }
        if best.0 > 0 {
            out.push((
                (caid, best.1),
                (
                    format!("community_{}", caid),
                    format!("community_{}", best.1),
                ),
            ));
        }
    }
    out
}

fn l5_build_focus_set(
    graph: &m1nd_core::graph::Graph,
    focus_nodes: &[String],
) -> std::collections::HashSet<String> {
    use std::collections::{HashSet, VecDeque};
    let ext = l5_build_node_to_ext_map(graph);
    let mut focus = HashSet::new();
    for name in focus_nodes {
        for nid in l5_resolve_claim_nodes(graph, name) {
            let mut vis = vec![false; graph.num_nodes() as usize];
            let mut q = VecDeque::new();
            vis[nid.as_usize()] = true;
            q.push_back((nid, 0usize));
            while let Some((node, depth)) = q.pop_front() {
                focus.insert(ext[node.as_usize()].clone());
                if depth >= 2 {
                    continue;
                }
                for j in graph.csr.out_range(node) {
                    let t = graph.csr.targets[j].as_usize();
                    if t < vis.len() && !vis[t] {
                        vis[t] = true;
                        q.push_back((graph.csr.targets[j], depth + 1));
                    }
                }
                for j in graph.csr.in_range(node) {
                    let s = graph.csr.rev_sources[j].as_usize();
                    if s < vis.len() && !vis[s] {
                        vis[s] = true;
                        q.push_back((graph.csr.rev_sources[j], depth + 1));
                    }
                }
            }
        }
    }
    focus
}

fn l5_extract_keywords(question: &str) -> Vec<String> {
    let stop = [
        "what", "which", "how", "why", "when", "where", "is", "are", "was", "were", "the", "a",
        "an", "and", "or", "not", "new", "old", "between", "from", "to", "in", "of", "has", "have",
        "been", "did", "does", "do", "that", "this", "with", "for", "modules", "became",
    ];
    question
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() > 2 && !stop.contains(w))
        .map(|w| w.to_string())
        .collect()
}

// =========================================================================
// Superpowers — Antibody / Flow / Epidemic / Tremor / Trust / Layers
// =========================================================================

/// Handle m1nd.antibody_scan — scan graph against stored bug antibodies.
pub fn handle_antibody_scan(
    state: &mut SessionState,
    input: layers::AntibodyScanInput,
) -> M1ndResult<serde_json::Value> {
    use m1nd_core::antibody::{self, AntibodySeverity};

    state.track_agent(&input.agent_id);

    let min_severity = match input.min_severity.to_lowercase().as_str() {
        "critical" => AntibodySeverity::Critical,
        "warning" => AntibodySeverity::Warning,
        _ => AntibodySeverity::Info,
    };

    let antibody_ids: Option<Vec<String>> = if input.antibody_ids.is_empty() {
        None
    } else {
        Some(input.antibody_ids.clone())
    };

    let max_per_ab = if input.max_matches_per_antibody > 0 {
        input.max_matches_per_antibody
    } else {
        50
    };

    let graph = state.graph.read();
    let result = antibody::scan_antibodies(
        &graph,
        &mut state.antibodies,
        &input.scope,
        state.last_antibody_scan_generation,
        input.max_matches,
        min_severity,
        antibody_ids.as_deref(),
        max_per_ab,
        &input.match_mode,
        input.similarity_threshold,
    );
    drop(graph);

    state.last_antibody_scan_generation = {
        let g = state.graph.read();
        g.generation.0
    };

    Ok(serde_json::json!({
        "matches": result.matches,
        "antibodies_checked": result.antibodies_checked,
        "nodes_scanned": result.nodes_scanned,
        "elapsed_ms": result.elapsed_ms,
        "scan_scope": result.scan_scope,
        "timed_out_antibodies": result.timed_out_antibodies,
        "auto_disabled_antibodies": result.auto_disabled_antibodies
    }))
}

/// Handle m1nd.antibody_list — list all stored bug antibodies.
pub fn handle_antibody_list(
    state: &mut SessionState,
    input: layers::AntibodyListInput,
) -> M1ndResult<serde_json::Value> {
    state.track_agent(&input.agent_id);

    let total = state.antibodies.len();
    let enabled_count = state.antibodies.iter().filter(|a| a.enabled).count();
    let disabled_count = total - enabled_count;

    let filtered: Vec<&m1nd_core::antibody::Antibody> = if input.include_disabled {
        state.antibodies.iter().collect()
    } else {
        state.antibodies.iter().filter(|a| a.enabled).collect()
    };

    Ok(serde_json::json!({
        "antibodies": filtered,
        "total": total,
        "enabled": enabled_count,
        "disabled": disabled_count
    }))
}

/// Handle m1nd.antibody_create — create/disable/enable/delete antibody.
pub fn handle_antibody_create(
    state: &mut SessionState,
    input: layers::AntibodyCreateInput,
) -> M1ndResult<serde_json::Value> {
    use m1nd_core::antibody::{
        self, Antibody, AntibodyPattern, AntibodySeverity, PatternEdge, PatternNode,
    };
    use m1nd_core::error::M1ndError;

    state.track_agent(&input.agent_id);

    match input.action.as_str() {
        "disable" => {
            let ab_id =
                input
                    .antibody_id
                    .as_deref()
                    .ok_or_else(|| M1ndError::AntibodyNotFound {
                        id: "missing antibody_id".into(),
                    })?;
            let ab = state
                .antibodies
                .iter_mut()
                .find(|a| a.id == ab_id)
                .ok_or_else(|| M1ndError::AntibodyNotFound {
                    id: ab_id.to_string(),
                })?;
            ab.enabled = false;
            Ok(serde_json::json!({ "success": true, "action": "disable", "antibody_id": ab_id }))
        }
        "enable" => {
            let ab_id =
                input
                    .antibody_id
                    .as_deref()
                    .ok_or_else(|| M1ndError::AntibodyNotFound {
                        id: "missing antibody_id".into(),
                    })?;
            let ab = state
                .antibodies
                .iter_mut()
                .find(|a| a.id == ab_id)
                .ok_or_else(|| M1ndError::AntibodyNotFound {
                    id: ab_id.to_string(),
                })?;
            ab.enabled = true;
            Ok(serde_json::json!({ "success": true, "action": "enable", "antibody_id": ab_id }))
        }
        "delete" => {
            let ab_id =
                input
                    .antibody_id
                    .as_deref()
                    .ok_or_else(|| M1ndError::AntibodyNotFound {
                        id: "missing antibody_id".into(),
                    })?;
            let before_len = state.antibodies.len();
            state.antibodies.retain(|a| a.id != ab_id);
            if state.antibodies.len() == before_len {
                return Err(M1ndError::AntibodyNotFound {
                    id: ab_id.to_string(),
                });
            }
            Ok(serde_json::json!({ "success": true, "action": "delete", "antibody_id": ab_id }))
        }
        _ => {
            // "create" action (default)
            if state.antibodies.len() >= antibody::MAX_ANTIBODIES {
                return Err(M1ndError::AntibodyLimitExceeded {
                    current: state.antibodies.len(),
                    limit: antibody::MAX_ANTIBODIES,
                });
            }

            let name = input.name.unwrap_or_else(|| "unnamed".to_string());
            let description = input.description.unwrap_or_default();
            let severity = match input.severity.to_lowercase().as_str() {
                "critical" => AntibodySeverity::Critical,
                "info" => AntibodySeverity::Info,
                _ => AntibodySeverity::Warning,
            };

            let pattern_input = input.pattern.ok_or_else(|| M1ndError::AntibodyNotFound {
                id: "pattern required for create".into(),
            })?;

            let pattern = AntibodyPattern {
                nodes: pattern_input
                    .nodes
                    .into_iter()
                    .map(|n| PatternNode {
                        role: n.role,
                        node_type: n.node_type,
                        required_tags: n.required_tags,
                        label_contains: n.label_contains,
                    })
                    .collect(),
                edges: pattern_input
                    .edges
                    .into_iter()
                    .map(|e| PatternEdge {
                        source_idx: e.source_idx,
                        target_idx: e.target_idx,
                        relation: e.relation,
                    })
                    .collect(),
                negative_edges: pattern_input
                    .negative_edges
                    .into_iter()
                    .map(|e| PatternEdge {
                        source_idx: e.source_idx,
                        target_idx: e.target_idx,
                        relation: e.relation,
                    })
                    .collect(),
            };

            let specificity = antibody::compute_specificity(&pattern);
            if specificity < antibody::MIN_SPECIFICITY {
                return Err(M1ndError::PatternTooBroad {
                    specificity,
                    minimum: antibody::MIN_SPECIFICITY,
                });
            }

            let mut warning: Option<String> = None;
            for existing in &state.antibodies {
                let sim = antibody::pattern_similarity(&pattern, &existing.pattern);
                if sim > antibody::DUPLICATE_SIMILARITY_THRESHOLD {
                    warning = Some(format!(
                        "Similar antibody exists: '{}' (id: {}, similarity: {:.2})",
                        existing.name, existing.id, sim
                    ));
                    break;
                }
            }

            if specificity < 0.3 && warning.is_none() {
                warning = Some(
                    "Pattern is very broad - may produce excessive false positives.".to_string(),
                );
            }

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);

            let id = format!(
                "ab-{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                (now.to_bits() >> 32) as u32,
                (now.to_bits() >> 16) as u16,
                ((now.to_bits() >> 8) & 0x0FFF | 0x4000) as u16,
                ((now.to_bits() & 0x3FFF) | 0x8000) as u16,
                now.to_bits() & 0xFFFFFFFFFFFF
            );

            let new_antibody = Antibody {
                id: id.clone(),
                name,
                description,
                pattern,
                severity,
                match_count: 0,
                created_at: now,
                last_match_at: None,
                created_by: input.agent_id.clone(),
                source_query: String::new(),
                source_nodes: Vec::new(),
                enabled: true,
                specificity,
            };

            let graph = state.graph.read();
            let initial_matches =
                antibody::match_antibody(&graph, &new_antibody, antibody::PATTERN_MATCH_TIMEOUT_MS)
                    .len();
            drop(graph);

            state.antibodies.push(new_antibody);

            Ok(serde_json::json!({
                "antibody_id": id,
                "specificity": specificity,
                "initial_matches": initial_matches,
                "warning": warning
            }))
        }
    }
}

/// Handle m1nd.flow_simulate — concurrent flow simulation for race detection.
///
/// Checks active advisory locks (`lock.create`) and injects protected node
/// labels into the flow config so that locked regions get reduced turbulence
/// scores (85% reduction), cutting false positives for nodes under active work.
pub fn handle_flow_simulate(
    state: &mut SessionState,
    input: layers::FlowSimulateInput,
) -> M1ndResult<serde_json::Value> {
    // Collect advisory-lock-protected nodes BEFORE taking graph read lock
    // (locks HashMap is on SessionState, not behind the graph RwLock).
    let advisory_lock_protected_nodes = {
        let mut map: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for lock in state.locks.values() {
            for node_label in &lock.baseline.nodes {
                map.entry(node_label.clone())
                    .or_default()
                    .push(lock.lock_id.clone());
            }
        }
        map
    };

    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    if n == 0 {
        return Err(M1ndError::NoEntryPoints);
    }

    let engine = m1nd_core::flow::FlowEngine::new();

    // Build config: merge user-provided patterns with defaults
    let lock_patterns = if input.lock_patterns.is_empty() {
        m1nd_core::flow::DEFAULT_LOCK_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        input.lock_patterns.clone()
    };
    let read_only_patterns = if input.read_only_patterns.is_empty() {
        m1nd_core::flow::DEFAULT_READ_ONLY_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        input.read_only_patterns.clone()
    };

    let config = m1nd_core::flow::FlowConfig {
        lock_patterns,
        read_only_patterns,
        max_depth: input.max_depth,
        turbulence_threshold: input.turbulence_threshold,
        include_paths: input.include_paths,
        max_total_steps: input.max_total_steps,
        scope_filter: input.scope_filter.clone(),
        advisory_lock_protected_nodes,
        ..m1nd_core::flow::FlowConfig::default()
    };

    // Resolve entry nodes: label strings -> NodeIds via SeedFinder
    let entry_nodes = if input.entry_nodes.is_empty() {
        // Auto-discovery mode (F8)
        let discovered = engine.discover_entry_points(&graph, 100);
        if discovered.is_empty() {
            return Err(M1ndError::NoEntryPoints);
        }
        discovered
    } else {
        let mut resolved = Vec::new();
        for label in &input.entry_nodes {
            match m1nd_core::seed::SeedFinder::find_seeds(&graph, label, 1) {
                Ok(seeds) if !seeds.is_empty() => {
                    resolved.push(seeds[0].0);
                }
                _ => {} // skip unresolved
            }
        }
        if resolved.is_empty() {
            return Err(M1ndError::NoEntryPoints);
        }
        resolved
    };

    let result = engine.simulate(&graph, &entry_nodes, input.num_particles, &config)?;

    Ok(serde_json::to_value(&result).unwrap_or_default())
}

/// Handle m1nd.epidemic — SIR bug propagation prediction.
pub fn handle_epidemic(
    state: &mut SessionState,
    input: layers::EpidemicInput,
) -> M1ndResult<serde_json::Value> {
    use m1nd_core::epidemic::{EpidemicConfig, EpidemicDirection, EpidemicEngine};

    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;

    // Parse direction
    let direction = match input.direction.to_lowercase().as_str() {
        "forward" => EpidemicDirection::Forward,
        "backward" => EpidemicDirection::Backward,
        _ => EpidemicDirection::Both,
    };

    // Resolve infected node IDs
    let mut infected_ids: Vec<m1nd_core::types::NodeId> = Vec::new();
    let mut unresolved_nodes: Vec<String> = Vec::new();
    for ext_id in &input.infected_nodes {
        if let Some(nid) = graph.resolve_id(ext_id) {
            infected_ids.push(nid);
        } else {
            unresolved_nodes.push(ext_id.clone());
        }
    }

    if infected_ids.is_empty() {
        return Err(M1ndError::NoValidInfectedNodes);
    }

    // Resolve recovered node IDs
    let mut recovered_ids: Vec<m1nd_core::types::NodeId> = Vec::new();
    for ext_id in &input.recovered_nodes {
        if let Some(nid) = graph.resolve_id(ext_id) {
            recovered_ids.push(nid);
        } else {
            unresolved_nodes.push(ext_id.clone());
        }
    }

    // Auto-calibrate: adjust infection_rate based on graph density
    // avg_degree = 2 * edges / nodes (each edge contributes to 2 nodes' degree)
    // effective_rate = rate / (avg_degree / 2.0) — normalizes so sparse and dense graphs behave similarly
    let effective_infection_rate = if input.auto_calibrate {
        input.infection_rate.map(|rate| {
            let total_edges = graph.num_edges() as f32;
            let total_nodes = graph.num_nodes().max(1) as f32;
            let avg_degree = 2.0 * total_edges / total_nodes;
            let normalizer = (avg_degree / 2.0).max(1.0);
            rate / normalizer
        })
    } else {
        input.infection_rate
    };

    // When auto_calibrate is on, use a high promotion threshold to prevent
    // cascade burnout on dense graphs. On a graph with avg_degree > 4, the
    // deterministic SIR model causes instant cascade because every touched node
    // becomes a spreader. By setting promotion_threshold = 1.0, only the original
    // seed nodes act as spreaders; other nodes accumulate probability but don't
    // re-spread. This gives a meaningful "blast radius" prediction.
    let promotion_threshold = if input.auto_calibrate {
        let total_edges = graph.num_edges() as f32;
        let total_nodes = graph.num_nodes().max(1) as f32;
        let avg_degree = 2.0 * total_edges / total_nodes;
        if avg_degree > 4.0 {
            // Dense graph: disable promotion entirely (seeds-only spreading)
            1.0
        } else {
            0.0
        }
    } else {
        0.0
    };

    let config = EpidemicConfig {
        infection_rate: effective_infection_rate,
        recovery_rate: input.recovery_rate,
        iterations: input.iterations,
        direction,
        top_k: input.top_k,
        burnout_threshold: m1nd_core::epidemic::BURNOUT_THRESHOLD,
        promotion_threshold,
    };

    let engine = EpidemicEngine::new();
    let mut result = engine.simulate(&graph, &infected_ids, &recovered_ids, &config)?;

    // Merge unresolved nodes from input resolution
    result.unresolved_nodes = unresolved_nodes;

    // Apply scope filter: restrict predictions to specific node types
    if input.scope != "all" {
        let scope_lower = input.scope.to_lowercase();
        result.predictions.retain(|p| match scope_lower.as_str() {
            "files" => p.node_type == "file",
            "functions" => p.node_type == "function",
            _ => true,
        });
    }

    // Apply min_probability filter
    if input.min_probability > 0.0 {
        result
            .predictions
            .retain(|p| p.infection_probability >= input.min_probability);
    }

    Ok(serde_json::to_value(&result).unwrap_or_default())
}

/// Handle m1nd.tremor — code tremor detection (second derivative).
pub fn handle_tremor(
    state: &mut SessionState,
    input: layers::TremorInput,
) -> M1ndResult<serde_json::Value> {
    use m1nd_core::tremor::TremorWindow;
    use std::str::FromStr as _;

    let window = TremorWindow::from_str(&input.window).unwrap_or(TremorWindow::All);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    // Apply sensitivity multiplier to threshold
    let effective_threshold = input.threshold / input.sensitivity.max(0.01);

    let result = state.tremor_registry.analyze(
        window,
        effective_threshold,
        input.top_k,
        input.node_filter.as_deref(),
        now,
        input.min_observations,
    );

    let tremors_json: Vec<serde_json::Value> = result
        .tremors
        .iter()
        .map(|alert| {
            serde_json::json!({
                "node_id": alert.node_id,
                "label": alert.label,
                "magnitude": alert.magnitude,
                "direction": alert.direction,
                "mean_acceleration": alert.mean_acceleration,
                "trend_slope": alert.trend_slope,
                "observation_count": alert.observation_count,
                "window_start": alert.window_start,
                "window_end": alert.window_end,
                "latest_velocity": alert.latest_velocity,
                "previous_velocity": alert.previous_velocity,
                "risk_level": alert.risk_level,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "tremors": tremors_json,
        "window": result.window,
        "threshold": result.threshold,
        "total_nodes_analyzed": result.total_nodes_analyzed,
        "nodes_with_sufficient_data": result.nodes_with_sufficient_data,
        "elapsed_ms": result.elapsed_ms,
    }))
}

/// Handle m1nd.trust — per-module trust scores from defect history.
pub fn handle_trust(
    state: &mut SessionState,
    input: layers::TrustInput,
) -> M1ndResult<serde_json::Value> {
    use m1nd_core::trust::TrustSortBy;
    use std::str::FromStr as _;

    let sort_by = TrustSortBy::from_str(&input.sort_by).unwrap_or(TrustSortBy::TrustAsc);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let half_life_hours = input.decay_half_life_days * 24.0;
    let result = state.trust_ledger.report(
        &input.scope,
        input.min_history,
        input.top_k,
        input.node_filter.as_deref(),
        sort_by,
        now,
        half_life_hours,
        input.risk_cap,
    );

    let scores_json: Vec<serde_json::Value> = result
        .trust_scores
        .iter()
        .map(|entry| {
            serde_json::json!({
                "node_id": entry.node_id,
                "label": entry.label,
                "trust_score": entry.trust_score,
                "defect_density": entry.defect_density,
                "risk_multiplier": entry.risk_multiplier,
                "recency_factor": entry.recency_factor,
                "defect_count": entry.defect_count,
                "false_alarm_count": entry.false_alarm_count,
                "partial_count": entry.partial_count,
                "total_learn_events": entry.total_learn_events,
                "last_defect_age_hours": entry.last_defect_age_hours,
                "tier": entry.tier,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "trust_scores": scores_json,
        "summary": {
            "total_nodes_with_history": result.summary.total_nodes_with_history,
            "high_risk_count": result.summary.high_risk_count,
            "medium_risk_count": result.summary.medium_risk_count,
            "low_risk_count": result.summary.low_risk_count,
            "unknown_count": result.summary.unknown_count,
            "mean_trust": result.summary.mean_trust,
        },
        "scope": result.scope,
        "elapsed_ms": result.elapsed_ms,
    }))
}

/// Handle m1nd.layers — auto-detect architectural layers.
pub fn handle_layers(
    state: &mut SessionState,
    input: layers::LayersInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();

    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    if n == 0 {
        return Err(M1ndError::EmptyGraph);
    }

    // Parse node type filters
    let node_type_filter: Vec<NodeType> = input
        .node_types
        .iter()
        .filter_map(|t| layer_parse_node_type(t))
        .collect();

    let normalized_scope = l7_normalize_layer_scope(input.scope.as_deref(), &state.ingest_roots);

    // Run layer detection
    let detector =
        m1nd_core::layer::LayerDetector::new(input.max_layers, input.min_nodes_per_layer);
    let result = detector.detect(
        &graph,
        normalized_scope.as_deref(),
        &node_type_filter,
        input.exclude_tests,
        &input.naming_strategy,
    )?;

    // Build reverse lookup: NodeId -> external_id
    let mut node_to_ext: Vec<String> = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(interned).to_string();
        }
    }

    // Convert to protocol output
    let layer_entries: Vec<serde_json::Value> = result
        .layers
        .iter()
        .map(|layer| {
            let nodes: Vec<serde_json::Value> = layer
                .nodes
                .iter()
                .enumerate()
                .map(|(i, &nid)| {
                    let idx = nid.as_usize();
                    let label = graph.strings.resolve(graph.nodes.label[idx]).to_string();
                    let nt = layer_node_type_str(&graph.nodes.node_type[idx]);
                    let out_range = graph.csr.out_range(nid);
                    let in_range = graph.csr.in_range(nid);
                    let confidence = layer.node_confidence.get(i).copied().unwrap_or(0.5);

                    serde_json::json!({
                        "node_id": node_to_ext[idx],
                        "label": label,
                        "type": nt,
                        "in_degree": in_range.len(),
                        "out_degree": out_range.len(),
                        "layer_confidence": confidence
                    })
                })
                .collect();

            serde_json::json!({
                "level": layer.level,
                "name": layer.name,
                "description": layer.description,
                "node_count": layer.nodes.len(),
                "nodes": nodes,
                "avg_pagerank": layer.avg_pagerank,
                "avg_out_degree": layer.avg_out_degree
            })
        })
        .collect();

    // Convert violations (only if requested), respect violation_limit
    let violation_limit = input.violation_limit;
    let violation_entries: Vec<serde_json::Value> = if input.include_violations {
        result
            .violations
            .iter()
            .take(violation_limit)
            .map(|v| {
                let src_ext = if (v.source.as_usize()) < n {
                    &node_to_ext[v.source.as_usize()]
                } else {
                    ""
                };
                let tgt_ext = if (v.target.as_usize()) < n {
                    &node_to_ext[v.target.as_usize()]
                } else {
                    ""
                };
                let severity_str = match v.severity {
                    m1nd_core::layer::ViolationSeverity::Low => "low",
                    m1nd_core::layer::ViolationSeverity::Medium => "medium",
                    m1nd_core::layer::ViolationSeverity::High => "high",
                    m1nd_core::layer::ViolationSeverity::Critical => "critical",
                };
                let vtype_str = match v.violation_type {
                    m1nd_core::layer::ViolationType::UpwardDependency => "upward_dependency",
                    m1nd_core::layer::ViolationType::SkipLayerDependency => "skip_layer_dependency",
                    m1nd_core::layer::ViolationType::CircularDependency => "circular_dependency",
                };

                serde_json::json!({
                    "source": src_ext,
                    "source_layer": v.source_layer,
                    "target": tgt_ext,
                    "target_layer": v.target_layer,
                    "edge_relation": v.edge_relation,
                    "severity": severity_str,
                    "violation_type": vtype_str,
                    "explanation": v.explanation
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    // Convert utility nodes
    let utility_entries: Vec<serde_json::Value> = result
        .utility_nodes
        .iter()
        .map(|u| {
            let ext = if (u.node.as_usize()) < n {
                &node_to_ext[u.node.as_usize()]
            } else {
                ""
            };
            let label = graph
                .strings
                .resolve(graph.nodes.label[u.node.as_usize()])
                .to_string();
            let class_str = match u.classification {
                m1nd_core::layer::UtilityClassification::CrossCutting => "cross_cutting",
                m1nd_core::layer::UtilityClassification::Bridge => "bridge",
                m1nd_core::layer::UtilityClassification::Orphan => "orphan",
            };

            serde_json::json!({
                "node_id": ext,
                "label": label,
                "used_by_layers": u.used_by_layers,
                "classification": class_str
            })
        })
        .collect();

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let output = serde_json::json!({
        "layers": layer_entries,
        "violations": violation_entries,
        "utility_nodes": utility_entries,
        "summary": {
            "total_nodes_classified": result.total_nodes_classified,
            "total_layers_detected": result.layers.len(),
            "total_violations": result.violations.len(),
            "total_utility_nodes": result.utility_nodes.len(),
            "layer_separation_score": result.layer_separation_score,
            "has_cycles": result.has_cycles
        },
        "elapsed_ms": elapsed
    });

    Ok(output)
}

/// Handle m1nd.layer_inspect — inspect a specific architectural layer.
pub fn handle_layer_inspect(
    state: &mut SessionState,
    input: layers::LayerInspectInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();

    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    if n == 0 {
        return Err(M1ndError::EmptyGraph);
    }

    let normalized_scope = l7_normalize_layer_scope(input.scope.as_deref(), &state.ingest_roots);

    // Parse node type filters from scope
    let node_type_filter: Vec<NodeType> = Vec::new();

    // Run layer detection to get current state
    let detector = m1nd_core::layer::LayerDetector::with_defaults();
    let result = detector.detect(
        &graph,
        normalized_scope.as_deref(),
        &node_type_filter,
        false,
        "auto",
    )?;

    // Find the requested layer
    let layer = result
        .layers
        .iter()
        .find(|l| l.level == input.level)
        .ok_or(M1ndError::LayerNotFound { level: input.level })?;

    // Compute health
    let health = detector.layer_health(&graph, &result, input.level)?;

    // Build reverse lookup
    let mut node_to_ext: Vec<String> = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(interned).to_string();
        }
    }

    // Build node_layer lookup for connections classification
    let mut node_layer_map: HashMap<NodeId, u8> = HashMap::new();
    for l in &result.layers {
        for &nid in &l.nodes {
            node_layer_map.insert(nid, l.level);
        }
    }
    let utility_set: HashSet<NodeId> = result.utility_nodes.iter().map(|u| u.node).collect();
    let layer_node_set: HashSet<NodeId> = layer.nodes.iter().copied().collect();

    // Count violations per node
    let mut violations_as_source: HashMap<NodeId, u32> = HashMap::new();
    let mut violations_as_target: HashMap<NodeId, u32> = HashMap::new();
    for v in &result.violations {
        *violations_as_source.entry(v.source).or_insert(0) += 1;
        *violations_as_target.entry(v.target).or_insert(0) += 1;
    }

    // Build node entries, sorted by PageRank descending
    let mut node_pr_pairs: Vec<(NodeId, usize, f32)> = layer
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &nid)| {
            let pr = graph.nodes.pagerank[nid.as_usize()].get();
            (nid, i, pr)
        })
        .collect();
    node_pr_pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Limit to top_k
    let top_k = input.top_k.min(node_pr_pairs.len());
    let node_entries: Vec<serde_json::Value> = node_pr_pairs[..top_k]
        .iter()
        .map(|&(nid, conf_idx, pr)| {
            let idx = nid.as_usize();
            let label = graph.strings.resolve(graph.nodes.label[idx]).to_string();
            let nt = layer_node_type_str(&graph.nodes.node_type[idx]);
            let out_range = graph.csr.out_range(nid);
            let in_range = graph.csr.in_range(nid);
            let confidence = layer.node_confidence.get(conf_idx).copied().unwrap_or(0.5);
            let v_src = violations_as_source.get(&nid).copied().unwrap_or(0);
            let v_tgt = violations_as_target.get(&nid).copied().unwrap_or(0);

            // Classify connections
            let mut connections_up: Vec<String> = Vec::new();
            let mut connections_down: Vec<String> = Vec::new();
            let mut connections_lateral: Vec<String> = Vec::new();

            for j in graph.csr.out_range(nid) {
                let target = graph.csr.targets[j];
                if utility_set.contains(&target) {
                    continue;
                }
                let tgt_ext = if target.as_usize() < n {
                    node_to_ext[target.as_usize()].clone()
                } else {
                    continue;
                };
                if let Some(&tgt_level) = node_layer_map.get(&target) {
                    if tgt_level < input.level {
                        connections_up.push(tgt_ext);
                    } else if tgt_level > input.level {
                        connections_down.push(tgt_ext);
                    } else {
                        connections_lateral.push(tgt_ext);
                    }
                }
            }
            // Also check incoming for up connections
            for j in graph.csr.in_range(nid) {
                let source = graph.csr.rev_sources[j];
                if utility_set.contains(&source) {
                    continue;
                }
                let src_ext = if source.as_usize() < n {
                    node_to_ext[source.as_usize()].clone()
                } else {
                    continue;
                };
                if let Some(&src_level) = node_layer_map.get(&source) {
                    if src_level < input.level && !connections_up.contains(&src_ext) {
                        connections_up.push(src_ext);
                    }
                }
            }

            serde_json::json!({
                "node_id": node_to_ext[idx],
                "label": label,
                "type": nt,
                "pagerank": pr,
                "in_degree": in_range.len(),
                "out_degree": out_range.len(),
                "layer_confidence": confidence,
                "violations_as_source": v_src,
                "violations_as_target": v_tgt,
                "connections_up": connections_up,
                "connections_down": connections_down,
                "connections_lateral": connections_lateral
            })
        })
        .collect();

    // Collect intra-layer edges
    let intra_edges: Vec<serde_json::Value> = if input.include_edges {
        let mut edges = Vec::new();
        for &nid in &layer.nodes {
            for j in graph.csr.out_range(nid) {
                let target = graph.csr.targets[j];
                if layer_node_set.contains(&target) && target != nid {
                    let rel = graph.strings.resolve(graph.csr.relations[j]).to_string();
                    let w = graph.csr.read_weight(EdgeIdx::new(j as u32)).get();
                    edges.push(serde_json::json!({
                        "source": node_to_ext[nid.as_usize()],
                        "target": node_to_ext[target.as_usize()],
                        "relation": rel,
                        "weight": w
                    }));
                }
            }
        }
        edges
    } else {
        Vec::new()
    };

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let output = serde_json::json!({
        "level": layer.level,
        "name": layer.name,
        "description": layer.description,
        "nodes": node_entries,
        "intra_layer_edges": intra_edges,
        "layer_health": {
            "cohesion": health.cohesion,
            "coupling_up": health.coupling_up,
            "coupling_down": health.coupling_down,
            "violation_density": health.violation_density
        },
        "elapsed_ms": elapsed
    });

    Ok(output)
}

/// Parse node type string to NodeType enum.
fn layer_parse_node_type(s: &str) -> Option<NodeType> {
    match s.to_lowercase().as_str() {
        "file" => Some(NodeType::File),
        "directory" | "dir" => Some(NodeType::Directory),
        "function" | "func" => Some(NodeType::Function),
        "class" => Some(NodeType::Class),
        "struct" => Some(NodeType::Struct),
        "enum" => Some(NodeType::Enum),
        "type" => Some(NodeType::Type),
        "module" | "mod" => Some(NodeType::Module),
        _ => None,
    }
}

/// Convert NodeType to display string.
fn layer_node_type_str(nt: &NodeType) -> &'static str {
    match nt {
        NodeType::File => "file",
        NodeType::Directory => "directory",
        NodeType::Function => "function",
        NodeType::Class => "class",
        NodeType::Struct => "struct",
        NodeType::Enum => "enum",
        NodeType::Type => "type",
        NodeType::Module => "module",
        NodeType::Reference => "reference",
        NodeType::Concept => "concept",
        NodeType::Material => "material",
        NodeType::Process => "process",
        NodeType::Product => "product",
        NodeType::Supplier => "supplier",
        NodeType::Regulatory => "regulatory",
        NodeType::System => "system",
        NodeType::Cost => "cost",
        NodeType::Custom(_) => "custom",
    }
}

fn l7_normalize_layer_scope(scope: Option<&str>, ingest_roots: &[String]) -> Option<String> {
    normalize_scope_path(scope, ingest_roots).map(|scope| format!("file::{}", scope))
}

// =========================================================================
// RETROBUILDER Handlers (RB-01 through RB-05)
// =========================================================================

/// RB-01: Ghost Edges — parse git history and inject temporal co-change edges.
pub fn handle_ghost_edges(
    state: &mut SessionState,
    input: layers::GhostEdgesInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();

    // Parse depth
    let depth = m1nd_core::git_history::GitDepth::parse(&input.depth)?;

    // Discover repo root
    let repo_root = discover_git_root(state)?;

    // Parse git history
    let commits = m1nd_core::git_history::parse_git_history(&repo_root, depth)?;

    // Inject into co-change matrix
    let graph = state.graph.read();
    let result = m1nd_core::git_history::inject_git_history(
        &graph,
        &mut state.orchestrator.temporal.co_change,
        &commits,
    )?;
    drop(graph);

    state.queries_processed += 1;
    if state.should_persist() {
        let _ = state.persist();
    }

    serde_json::to_value(serde_json::json!({
        "commits_parsed": result.commits_parsed,
        "co_change_pairs_injected": result.co_change_pairs_injected,
        "ghost_edges_found": result.ghost_edges_found,
        "depth": input.depth,
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
    }))
    .map_err(M1ndError::Serde)
}

/// RB-02: Taint Trace — taint propagation / graph fuzzing.
pub fn handle_taint_trace(
    state: &mut SessionState,
    input: layers::TaintTraceInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();
    let graph = state.graph.read();

    // Resolve entry node IDs to NodeIds
    let entry_node_ids: Vec<m1nd_core::types::NodeId> = input
        .entry_nodes
        .iter()
        .filter_map(|ext_id| graph.resolve_id(ext_id))
        .collect();

    if entry_node_ids.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "taint_trace".into(),
            detail: format!(
                "no entry nodes resolved from: {}",
                input.entry_nodes.join(", ")
            ),
        });
    }

    // Build taint type
    let taint_type = match input.taint_type.as_str() {
        "sensitive_data" => m1nd_core::taint::TaintType::SensitiveData,
        "custom" => m1nd_core::taint::TaintType::Custom {
            boundary_patterns: input.boundary_patterns,
        },
        _ => m1nd_core::taint::TaintType::UserInput,
    };

    let config = m1nd_core::taint::TaintConfig {
        max_depth: input.max_depth,
        min_probability: input.min_probability,
        taint_type,
        ..m1nd_core::taint::TaintConfig::default()
    };

    let result = m1nd_core::taint::TaintEngine::analyze(&graph, &entry_node_ids, &config)?;
    drop(graph);

    state.queries_processed += 1;
    if state.should_persist() {
        let _ = state.persist();
    }

    serde_json::to_value(serde_json::json!({
        "risk_score": result.risk_score,
        "summary": result.summary,
        "boundary_hits": result.boundary_hits,
        "boundary_misses": result.boundary_misses,
        "leaks": result.leaks,
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
    }))
    .map_err(M1ndError::Serde)
}

/// RB-03: Twins — find structural twins via topological signatures.
pub fn handle_twins(
    state: &mut SessionState,
    input: layers::TwinsInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();
    let graph = state.graph.read();

    // Convert string node types to NodeType enum
    let node_types: Vec<m1nd_core::types::NodeType> = input
        .node_types
        .iter()
        .filter_map(|s| match s.to_lowercase().as_str() {
            "function" => Some(m1nd_core::types::NodeType::Function),
            "class" => Some(m1nd_core::types::NodeType::Class),
            "struct" => Some(m1nd_core::types::NodeType::Struct),
            "file" => Some(m1nd_core::types::NodeType::File),
            "module" => Some(m1nd_core::types::NodeType::Module),
            _ => None,
        })
        .collect();

    let config = m1nd_core::twins::TwinConfig {
        similarity_threshold: input.similarity_threshold,
        top_k: input.top_k,
        scope: input.scope,
        node_types,
        use_edge_types: true,
    };

    let result = m1nd_core::twins::find_twins(&graph, &config)?;
    drop(graph);

    state.queries_processed += 1;
    if state.should_persist() {
        let _ = state.persist();
    }

    serde_json::to_value(serde_json::json!({
        "pairs": result.pairs,
        "nodes_analyzed": result.nodes_analyzed,
        "signatures_computed": result.signatures_computed,
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
    }))
    .map_err(M1ndError::Serde)
}

/// RB-04: Refactor Plan — community detection + counterfactual extraction.
pub fn handle_refactor_plan(
    state: &mut SessionState,
    input: layers::RefactorPlanInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();
    let graph = state.graph.read();

    let config = m1nd_core::refactor::RefactorConfig {
        max_communities: input.max_communities,
        min_community_size: input.min_community_size,
        scope: input.scope,
        ..m1nd_core::refactor::RefactorConfig::default()
    };

    let result = m1nd_core::refactor::plan_refactoring(&graph, &config)?;
    drop(graph);

    state.queries_processed += 1;
    if state.should_persist() {
        let _ = state.persist();
    }

    serde_json::to_value(serde_json::json!({
        "candidates": result.candidates,
        "graph_modularity": result.graph_modularity,
        "num_communities": result.num_communities,
        "nodes_analyzed": result.nodes_analyzed,
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
    }))
    .map_err(M1ndError::Serde)
}

/// RB-05: Runtime Overlay — ingest OTel spans and paint runtime heat.
pub fn handle_runtime_overlay(
    state: &mut SessionState,
    input: layers::RuntimeOverlayInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();

    // Convert input spans to OtelBatch
    let batch = m1nd_core::runtime_overlay::OtelBatch {
        spans: input
            .spans
            .into_iter()
            .map(|s| m1nd_core::runtime_overlay::OtelSpan {
                name: s.name,
                duration_us: s.duration_us,
                count: s.count,
                is_error: s.is_error,
                attributes: s.attributes,
                parent: s.parent,
            })
            .collect(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0),
        service_name: input.service_name,
    };

    // Parse mapping strategy
    let mapping_strategy = match input.mapping_strategy.as_str() {
        "code_attribute" => m1nd_core::runtime_overlay::MappingStrategy::CodeAttribute,
        "exact_id" => m1nd_core::runtime_overlay::MappingStrategy::ExactId,
        _ => m1nd_core::runtime_overlay::MappingStrategy::LabelMatch,
    };

    let overlay_config = m1nd_core::runtime_overlay::OverlayConfig {
        mapping_strategy,
        ..m1nd_core::runtime_overlay::OverlayConfig::default()
    };

    let mut overlay = m1nd_core::runtime_overlay::RuntimeOverlay::new(overlay_config);
    let graph = state.graph.read();
    let result = overlay.ingest(&graph, &batch)?;

    // Apply boosts to graph activation
    drop(graph);
    let boosts_applied = {
        let mut graph = state.graph.write();
        overlay.apply_boosts(&mut graph, input.boost_strength)
    };

    state.queries_processed += 1;
    if state.should_persist() {
        let _ = state.persist();
    }

    serde_json::to_value(serde_json::json!({
        "spans_processed": result.spans_processed,
        "spans_mapped": result.spans_mapped,
        "spans_unmapped": result.spans_unmapped,
        "hot_nodes": result.hot_nodes,
        "boosts_applied": boosts_applied,
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
    }))
    .map_err(M1ndError::Serde)
}

// =========================================================================
// v0.7.0: m1nd.metrics — Structural Codebase Metrics
// =========================================================================

/// Handle m1nd.metrics — per-node structural metrics (LOC, children, degree).
pub fn handle_metrics(
    state: &mut SessionState,
    input: layers::MetricsInput,
) -> M1ndResult<layers::MetricsOutput> {
    let start = Instant::now();

    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    if n == 0 {
        return Err(M1ndError::EmptyGraph);
    }

    let normalized_scope = l7_normalize_layer_scope(input.scope.as_deref(), &state.ingest_roots);

    // Parse requested node types
    let type_filters: Vec<NodeType> = input
        .node_types
        .iter()
        .filter_map(|t| layer_parse_node_type(t))
        .collect();

    // Build reverse lookup
    let mut node_to_ext: Vec<String> = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(interned).to_string();
        }
    }

    // Iterate all nodes, filtering by type and scope
    let mut entries: Vec<layers::MetricsEntry> = Vec::new();

    for (idx, ext_id) in node_to_ext.iter().enumerate().take(n) {
        let nid = NodeId::new(idx as u32);
        let nt = graph.nodes.node_type[idx];

        // Type filter
        if !type_filters.is_empty() && !type_filters.contains(&nt) {
            continue;
        }

        // Scope filter
        if let Some(ref scope) = normalized_scope {
            if !ext_id.starts_with(scope.as_str()) {
                continue;
            }
        }

        // Compute LOC from provenance (with fallback to child max line_end)
        let prov = &graph.nodes.provenance[idx];
        let prov_loc = if prov.line_end > 0 && prov.line_end >= prov.line_start {
            prov.line_end - prov.line_start + 1
        } else {
            0
        };

        // Count children by type (outgoing "contains"/"defines" edges)
        // Also compute fallback LOC from children's max line_end
        let mut func_count = 0u32;
        let mut struct_count = 0u32;
        let mut enum_count = 0u32;
        let mut class_count = 0u32;
        let mut max_child_line_end: u32 = 0;

        let out_range = graph.csr.out_range(nid);
        for j in out_range.clone() {
            let target = graph.csr.targets[j];
            let tgt_idx = target.as_usize();
            if tgt_idx >= n {
                continue;
            }
            let rel = graph.strings.resolve(graph.csr.relations[j]);
            if rel == "contains" || rel == "defines" || rel == "owned_by_impl" {
                match graph.nodes.node_type[tgt_idx] {
                    NodeType::Function => func_count += 1,
                    NodeType::Struct => struct_count += 1,
                    NodeType::Enum => enum_count += 1,
                    NodeType::Class => class_count += 1,
                    _ => {}
                }
                // Track max line_end among children for fallback LOC
                let child_prov = &graph.nodes.provenance[tgt_idx];
                if child_prov.line_end > max_child_line_end {
                    max_child_line_end = child_prov.line_end;
                }
            }
        }

        // Fallback LOC: when provenance is empty, try reading from disk
        let loc = if prov_loc > 0 {
            prov_loc
        } else if max_child_line_end > 0 {
            max_child_line_end
        } else if matches!(graph.nodes.node_type[idx], NodeType::File) {
            let rel_path = ext_id.strip_prefix("file::").unwrap_or(ext_id);
            let mut disk_loc = 0u32;
            for root in &state.ingest_roots {
                let full_path = std::path::Path::new(root).join(rel_path);
                if let Ok(content) = std::fs::read(&full_path) {
                    disk_loc = content.iter().filter(|&&b| b == b'\n').count() as u32;
                    break;
                }
            }
            disk_loc
        } else {
            0
        };

        let in_range = graph.csr.in_range(nid);
        let out_degree = out_range.len() as u32;
        let in_degree = in_range.len() as u32;
        let pagerank = graph.nodes.pagerank[idx].get();
        let total_children = func_count + struct_count + enum_count + class_count;
        let density = if loc > 0 {
            total_children as f32 / loc as f32
        } else {
            0.0
        };

        let file_path = prov
            .source_path
            .map(|interned| graph.strings.resolve(interned).to_string());

        let label = graph.strings.resolve(graph.nodes.label[idx]).to_string();

        entries.push(layers::MetricsEntry {
            node_id: ext_id.clone(),
            label,
            node_type: layer_node_type_str(&nt).to_string(),
            loc,
            function_count: func_count,
            struct_count,
            enum_count,
            class_count,
            out_degree,
            in_degree,
            pagerank,
            density,
            file_path,
        });
    }

    // Sort
    match input.sort.as_str() {
        "complexity_desc" => {
            entries.sort_by(|a, b| {
                let ca = a.out_degree + a.in_degree + a.function_count;
                let cb = b.out_degree + b.in_degree + b.function_count;
                cb.cmp(&ca)
            });
        }
        "name_asc" => {
            entries.sort_by(|a, b| a.label.cmp(&b.label));
        }
        _ => {
            // loc_desc
            entries.sort_by(|a, b| b.loc.cmp(&a.loc));
        }
    }

    // Compute summary before truncation
    let total_files = entries.len() as u32;
    let total_loc: u64 = entries.iter().map(|e| e.loc as u64).sum();
    let total_functions: u32 = entries.iter().map(|e| e.function_count).sum();
    let total_structs: u32 = entries.iter().map(|e| e.struct_count).sum();
    let total_enums: u32 = entries.iter().map(|e| e.enum_count).sum();
    let total_classes: u32 = entries.iter().map(|e| e.class_count).sum();
    let avg_loc = if total_files > 0 {
        total_loc as f32 / total_files as f32
    } else {
        0.0
    };
    let (max_file, max_loc) = entries
        .iter()
        .max_by_key(|e| e.loc)
        .map(|e| (e.label.clone(), e.loc))
        .unwrap_or_default();

    // Truncate
    entries.truncate(input.top_k);

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    Ok(layers::MetricsOutput {
        entries,
        summary: layers::MetricsSummary {
            total_files,
            total_loc,
            total_functions,
            total_structs,
            total_enums,
            total_classes,
            avg_loc_per_file: avg_loc,
            max_loc_file: max_file,
            max_loc,
        },
        elapsed_ms: elapsed,
    })
}

// =========================================================================
// v0.7.0: m1nd.type_trace — Cross-File Type Usage Tracing
// =========================================================================

/// Handle m1nd.type_trace — BFS from a type node to find all usage sites.
pub fn handle_type_trace(
    state: &mut SessionState,
    input: layers::TypeTraceInput,
) -> M1ndResult<layers::TypeTraceOutput> {
    let start = Instant::now();

    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    if n == 0 {
        return Err(M1ndError::EmptyGraph);
    }

    // Resolve target node — tiered resolution:
    // 1. Exact external_id match
    // 2. Label exact (case-insensitive) — prefer type-defining nodes
    // 3. External_id segment match (::Target or ::Target::) — prefer types
    // 4. Generic substring in external_id — prefer types
    let target_node = graph
        .resolve_id(&input.target)
        .or_else(|| {
            // Tier 2: Label exact match (case-insensitive)
            let mut best: Option<NodeId> = None;
            let mut best_is_type = false;
            for idx in 0..n {
                let label = graph.strings.resolve(graph.nodes.label[idx]);
                if label == input.target || label.eq_ignore_ascii_case(&input.target) {
                    let nt = graph.nodes.node_type[idx];
                    let is_type = matches!(
                        nt,
                        NodeType::Struct | NodeType::Enum | NodeType::Class | NodeType::Type
                    );
                    if is_type && !best_is_type {
                        best = Some(NodeId::new(idx as u32));
                        best_is_type = true;
                    } else if best.is_none() {
                        best = Some(NodeId::new(idx as u32));
                    }
                }
            }
            best
        })
        .or_else(|| {
            // Tier 3: Segment match in external_id
            let segment_suffix = format!("::{}", input.target);
            let segment_mid = format!("::{}::", input.target);
            let mut best: Option<NodeId> = None;
            let mut best_is_type = false;
            for (&interned, &nid) in &graph.id_to_node {
                let ext = graph.strings.resolve(interned);
                if ext.ends_with(&segment_suffix) || ext.contains(&segment_mid) {
                    let nt = graph.nodes.node_type[nid.as_usize()];
                    let is_type = matches!(
                        nt,
                        NodeType::Struct | NodeType::Enum | NodeType::Class | NodeType::Type
                    );
                    if is_type && !best_is_type {
                        best = Some(nid);
                        best_is_type = true;
                    } else if best.is_none() {
                        best = Some(nid);
                    }
                }
            }
            best
        })
        .or_else(|| {
            // Tier 4: Generic substring in external_id (loose fallback)
            let mut best: Option<NodeId> = None;
            let mut best_is_type = false;
            for (&interned, &nid) in &graph.id_to_node {
                let ext = graph.strings.resolve(interned);
                if ext.contains(&input.target) {
                    let nt = graph.nodes.node_type[nid.as_usize()];
                    let is_type = matches!(
                        nt,
                        NodeType::Struct | NodeType::Enum | NodeType::Class | NodeType::Type
                    );
                    if is_type && !best_is_type {
                        best = Some(nid);
                        best_is_type = true;
                    } else if best.is_none() {
                        best = Some(nid);
                    }
                }
            }
            best
        });

    let target_nid = match target_node {
        Some(nid) => nid,
        None => {
            return Ok(layers::TypeTraceOutput {
                target: input.target,
                target_label: String::new(),
                target_type: String::new(),
                direction: input.direction,
                max_hops_used: 0,
                usages: vec![],
                by_file: vec![],
                total_usages: 0,
                total_files: 0,
                elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            });
        }
    };

    let target_idx = target_nid.as_usize();
    let target_label = graph
        .strings
        .resolve(graph.nodes.label[target_idx])
        .to_string();
    let target_type = layer_node_type_str(&graph.nodes.node_type[target_idx]).to_string();

    // Build reverse lookup
    let mut node_to_ext: Vec<String> = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(interned).to_string();
        }
    }

    // BFS
    let use_forward = input.direction != "reverse";
    let use_reverse = input.direction != "forward";
    let max_hops = input.max_hops as usize;

    let mut visited = vec![false; n];
    visited[target_idx] = true;
    let mut queue: std::collections::VecDeque<(NodeId, u8, String)> =
        std::collections::VecDeque::new();

    // Seed BFS from target
    if use_forward {
        for j in graph.csr.in_range(target_nid) {
            let src = graph.csr.rev_sources[j];
            let fwd_edge = graph.csr.rev_edge_idx[j].as_usize();
            let rel = graph
                .strings
                .resolve(graph.csr.relations[fwd_edge])
                .to_string();
            if src.as_usize() < n && !visited[src.as_usize()] {
                visited[src.as_usize()] = true;
                queue.push_back((src, 1, rel));
            }
        }
    }
    if use_reverse {
        for j in graph.csr.out_range(target_nid) {
            let tgt = graph.csr.targets[j];
            let rel = graph.strings.resolve(graph.csr.relations[j]).to_string();
            if tgt.as_usize() < n && !visited[tgt.as_usize()] {
                visited[tgt.as_usize()] = true;
                queue.push_back((tgt, 1, rel));
            }
        }
    }

    let mut usages: Vec<layers::TypeTraceUsage> = Vec::new();

    while let Some((node, hops, relation)) = queue.pop_front() {
        let idx = node.as_usize();
        let prov = &graph.nodes.provenance[idx];
        let file_path = prov
            .source_path
            .map(|interned| graph.strings.resolve(interned).to_string());
        let line_start = if prov.line_start > 0 {
            Some(prov.line_start)
        } else {
            None
        };

        usages.push(layers::TypeTraceUsage {
            node_id: node_to_ext[idx].clone(),
            label: graph.strings.resolve(graph.nodes.label[idx]).to_string(),
            node_type: layer_node_type_str(&graph.nodes.node_type[idx]).to_string(),
            hops,
            relation,
            file_path,
            line_start,
        });

        // Continue BFS if under hop limit
        if (hops as usize) < max_hops {
            if use_forward {
                for j in graph.csr.in_range(node) {
                    let src = graph.csr.rev_sources[j];
                    if src.as_usize() < n && !visited[src.as_usize()] {
                        visited[src.as_usize()] = true;
                        let fwd_edge = graph.csr.rev_edge_idx[j].as_usize();
                        let rel = graph
                            .strings
                            .resolve(graph.csr.relations[fwd_edge])
                            .to_string();
                        queue.push_back((src, hops + 1, rel));
                    }
                }
            }
            if use_reverse {
                for j in graph.csr.out_range(node) {
                    let tgt = graph.csr.targets[j];
                    if tgt.as_usize() < n && !visited[tgt.as_usize()] {
                        visited[tgt.as_usize()] = true;
                        let rel = graph.strings.resolve(graph.csr.relations[j]).to_string();
                        queue.push_back((tgt, hops + 1, rel));
                    }
                }
            }
        }
    }

    // Sort by hops
    usages.sort_by_key(|u| u.hops);
    usages.truncate(input.top_k);

    // Group by file
    let mut file_groups: Vec<layers::TypeTraceFileGroup> = Vec::new();
    if input.group_by_file {
        let mut file_map: std::collections::HashMap<String, Vec<layers::TypeTraceUsage>> =
            std::collections::HashMap::new();
        for u in &usages {
            let key = u.file_path.clone().unwrap_or_else(|| "unknown".to_string());
            file_map.entry(key).or_default().push(u.clone());
        }
        for (file, group_usages) in file_map {
            let count = group_usages.len();
            file_groups.push(layers::TypeTraceFileGroup {
                file,
                usage_count: count,
                usages: group_usages,
            });
        }
        file_groups.sort_by(|a, b| b.usage_count.cmp(&a.usage_count));
    }

    let total_files = file_groups.len();
    let total_usages = usages.len();
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    Ok(layers::TypeTraceOutput {
        target: input.target,
        target_label,
        target_type,
        direction: input.direction,
        max_hops_used: input.max_hops,
        usages,
        by_file: file_groups,
        total_usages,
        total_files,
        elapsed_ms: elapsed,
    })
}

// =========================================================================
// v0.7.0: m1nd.diagram — Graph-to-Mermaid/DOT Export
// =========================================================================

struct DiagEdge {
    src_idx: usize,
    tgt_idx: usize,
    relation: String,
}

/// Handle m1nd.diagram — generate a Mermaid or DOT diagram from the graph.
pub fn handle_diagram(
    state: &mut SessionState,
    input: layers::DiagramInput,
) -> M1ndResult<layers::DiagramOutput> {
    let start = Instant::now();

    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    if n == 0 {
        return Err(M1ndError::EmptyGraph);
    }

    let normalized_scope = l7_normalize_layer_scope(input.scope.as_deref(), &state.ingest_roots);

    // Parse node type filters
    let type_filters: Vec<NodeType> = input
        .node_types
        .iter()
        .filter_map(|t| layer_parse_node_type(t))
        .collect();

    // Build reverse lookup
    let mut node_to_ext: Vec<String> = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(interned).to_string();
        }
    }

    // Determine center node and collect subgraph via BFS
    let center_node = input.center.as_ref().and_then(|c| {
        graph.resolve_id(c).or_else(|| {
            // Fuzzy label match
            for idx in 0..n {
                let label = graph.strings.resolve(graph.nodes.label[idx]);
                if label.contains(c.as_str()) {
                    return Some(NodeId::new(idx as u32));
                }
            }
            None
        })
    });

    let mut included: Vec<bool> = vec![false; n];
    let mut included_count: usize = 0;
    let max_nodes = input.max_nodes;

    if let Some(center) = center_node {
        // BFS from center
        let mut queue: std::collections::VecDeque<(NodeId, u8)> = std::collections::VecDeque::new();
        included[center.as_usize()] = true;
        included_count = 1;
        queue.push_back((center, 0));

        while let Some((node, depth)) = queue.pop_front() {
            if included_count >= max_nodes {
                break;
            }
            if depth >= input.depth {
                continue;
            }
            // Forward
            for j in graph.csr.out_range(node) {
                let tgt = graph.csr.targets[j];
                if tgt.as_usize() < n && !included[tgt.as_usize()] && included_count < max_nodes {
                    let passes_type = type_filters.is_empty()
                        || type_filters.contains(&graph.nodes.node_type[tgt.as_usize()]);
                    if passes_type {
                        included[tgt.as_usize()] = true;
                        included_count += 1;
                        queue.push_back((tgt, depth + 1));
                    }
                }
            }
            // Reverse
            for j in graph.csr.in_range(node) {
                let src = graph.csr.rev_sources[j];
                if src.as_usize() < n && !included[src.as_usize()] && included_count < max_nodes {
                    let passes_type = type_filters.is_empty()
                        || type_filters.contains(&graph.nodes.node_type[src.as_usize()]);
                    if passes_type {
                        included[src.as_usize()] = true;
                        included_count += 1;
                        queue.push_back((src, depth + 1));
                    }
                }
            }
        }
    } else {
        // No center — take top-N by PageRank, respecting scope and type
        let mut candidates: Vec<(usize, f32)> = (0..n)
            .filter(|&idx| {
                if !type_filters.is_empty() && !type_filters.contains(&graph.nodes.node_type[idx]) {
                    return false;
                }
                if let Some(ref scope) = normalized_scope {
                    let ext = &node_to_ext[idx];
                    if !ext.starts_with(scope.as_str()) {
                        return false;
                    }
                }
                true
            })
            .map(|idx| (idx, graph.nodes.pagerank[idx].get()))
            .collect();
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for &(idx, _) in candidates.iter().take(max_nodes) {
            included[idx] = true;
            included_count += 1;
        }
    }

    // Collect edges between included nodes
    let mut edges: Vec<DiagEdge> = Vec::new();

    for idx in 0..n {
        if !included[idx] {
            continue;
        }
        let nid = NodeId::new(idx as u32);
        for j in graph.csr.out_range(nid) {
            let tgt = graph.csr.targets[j];
            if tgt.as_usize() < n && included[tgt.as_usize()] {
                let rel = graph.strings.resolve(graph.csr.relations[j]).to_string();
                edges.push(DiagEdge {
                    src_idx: idx,
                    tgt_idx: tgt.as_usize(),
                    relation: rel,
                });
            }
        }
    }

    // Generate diagram source
    let is_mermaid = input.format != "dot";
    let source = if is_mermaid {
        generate_mermaid(&graph, &included, &edges, &node_to_ext, &input)
    } else {
        generate_dot(&graph, &included, &edges, &node_to_ext, &input)
    };

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    Ok(layers::DiagramOutput {
        source,
        format: if is_mermaid {
            "mermaid".to_string()
        } else {
            "dot".to_string()
        },
        node_count: included_count,
        edge_count: edges.len(),
        center_node: center_node.map(|nid| node_to_ext[nid.as_usize()].clone()),
        elapsed_ms: elapsed,
    })
}

fn mermaid_safe_id(ext_id: &str) -> String {
    ext_id.replace("::", "_").replace(['/', '.', '-', ' '], "_")
}

fn mermaid_shape(nt: &NodeType) -> (&str, &str) {
    match nt {
        NodeType::File => ("[", "]"),
        NodeType::Function => ("((", "))"),
        NodeType::Class | NodeType::Struct => ("{", "}"),
        NodeType::Enum => ("{{", "}}"),
        NodeType::Module | NodeType::Directory => ("([", "])"),
        _ => ("[", "]"),
    }
}

fn generate_mermaid(
    graph: &m1nd_core::graph::Graph,
    included: &[bool],
    edges: &[DiagEdge],
    node_to_ext: &[String],
    input: &layers::DiagramInput,
) -> String {
    let mut out = String::with_capacity(4096);
    let dir = &input.direction;
    out.push_str(&format!("graph {}\n", dir));

    let n = graph.num_nodes() as usize;

    // Nodes
    for idx in 0..n {
        if !included[idx] {
            continue;
        }
        let label = graph.strings.resolve(graph.nodes.label[idx]);
        let nt = &graph.nodes.node_type[idx];
        let id = mermaid_safe_id(&node_to_ext[idx]);
        let (open, close) = mermaid_shape(nt);
        let display = if input.show_pagerank {
            format!("{} (PR:{:.3})", label, graph.nodes.pagerank[idx].get())
        } else {
            label.to_string()
        };
        // Escape Mermaid special chars in label
        let display = display.replace('"', "'");
        out.push_str(&format!("    {}{}\"{}\"{};\n", id, open, display, close));
    }

    // Edges
    for edge in edges {
        let src_id = mermaid_safe_id(&node_to_ext[edge.src_idx]);
        let tgt_id = mermaid_safe_id(&node_to_ext[edge.tgt_idx]);
        if input.show_relations && !edge.relation.is_empty() {
            out.push_str(&format!(
                "    {} -->|{}| {};\n",
                src_id, edge.relation, tgt_id
            ));
        } else {
            out.push_str(&format!("    {} --> {};\n", src_id, tgt_id));
        }
    }

    out
}

fn generate_dot(
    graph: &m1nd_core::graph::Graph,
    included: &[bool],
    edges: &[DiagEdge],
    node_to_ext: &[String],
    input: &layers::DiagramInput,
) -> String {
    let mut out = String::with_capacity(4096);
    let rankdir = if input.direction == "LR" { "LR" } else { "TB" };
    out.push_str(&format!(
        "digraph m1nd {{\n    rankdir={};\n    node [shape=box, style=rounded];\n\n",
        rankdir
    ));

    let n = graph.num_nodes() as usize;

    // Nodes
    for idx in 0..n {
        if !included[idx] {
            continue;
        }
        let label = graph.strings.resolve(graph.nodes.label[idx]);
        let id = mermaid_safe_id(&node_to_ext[idx]);
        let nt = &graph.nodes.node_type[idx];
        let shape = match nt {
            NodeType::File => "box",
            NodeType::Function => "ellipse",
            NodeType::Class | NodeType::Struct => "record",
            NodeType::Enum => "diamond",
            NodeType::Module | NodeType::Directory => "folder",
            _ => "box",
        };
        let display = if input.show_pagerank {
            format!("{}\\nPR:{:.3}", label, graph.nodes.pagerank[idx].get())
        } else {
            label.to_string()
        };
        out.push_str(&format!(
            "    {} [label=\"{}\", shape={}];\n",
            id, display, shape
        ));
    }

    out.push('\n');

    // Edges
    for edge in edges {
        let src_id = mermaid_safe_id(&node_to_ext[edge.src_idx]);
        let tgt_id = mermaid_safe_id(&node_to_ext[edge.tgt_idx]);
        if input.show_relations && !edge.relation.is_empty() {
            out.push_str(&format!(
                "    {} -> {} [label=\"{}\"];\n",
                src_id, tgt_id, edge.relation
            ));
        } else {
            out.push_str(&format!("    {} -> {};\n", src_id, tgt_id));
        }
    }

    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::{handle_layers, handle_scan, handle_seek, handle_validate_plan, TrailData};
    use crate::protocol::layers::{
        LayersInput, PlannedAction, ScanInput, SeekInput, TrailConclusionInput, TrailResumeInput,
        TrailSaveInput, TrailVisitedNodeInput, ValidatePlanInput,
    };
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
    use std::collections::HashMap;

    fn build_layer_state(root: &std::path::Path) -> SessionState {
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
            .add_node(
                "file::src/core.rs",
                "core.rs",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add core node");
        let b = graph
            .add_node("file::src/ui.rs", "ui.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add ui node");
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

    fn build_layer_state_with_manifest_gap(root: &std::path::Path) -> SessionState {
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
        let ui = graph
            .add_node("file::src/ui.rs", "ui.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add ui node");
        let manifest = graph
            .add_node(
                "file::Cargo.toml",
                "Cargo.toml",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add manifest node");
        graph
            .add_edge(
                core,
                ui,
                "imports",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.8),
            )
            .expect("add core->ui edge");
        graph
            .add_edge(
                core,
                manifest,
                "workspace",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.6),
            )
            .expect("add core->manifest edge");
        graph.finalize().expect("finalize graph");

        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());
        state
    }

    fn build_seek_natural_language_state(root: &std::path::Path) -> SessionState {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };

        let mut graph = Graph::new();
        let dispatch = graph
            .add_node(
                "file::m1nd-mcp/src/server.rs::fn::normalize_dispatch_tool_name",
                "normalize_dispatch_tool_name",
                NodeType::Function,
                &["dispatch", "alias", "canonical", "status"],
                0.0,
                0.0,
            )
            .expect("add dispatch node");
        let docs = graph
            .add_node(
                "file::docs/dispatch-aliases.md",
                "dispatch aliases",
                NodeType::File,
                &["dispatch", "alias", "docs"],
                0.0,
                0.0,
            )
            .expect("add docs node");
        let distractor = graph
            .add_node(
                "file::m1nd-mcp/src/server.rs::fn::format_dispatch_status",
                "format_dispatch_status",
                NodeType::Function,
                &["dispatch", "status", "format"],
                0.0,
                0.0,
            )
            .expect("add distractor node");
        graph
            .add_edge(
                dispatch,
                docs,
                "documents",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.4),
            )
            .expect("add dispatch->docs edge");
        graph
            .add_edge(
                dispatch,
                distractor,
                "related",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.3),
            )
            .expect("add dispatch->distractor edge");
        graph.finalize().expect("finalize graph");

        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());
        state
    }

    fn run_seek(
        state: &mut SessionState,
        scope: Option<String>,
    ) -> crate::protocol::layers::SeekOutput {
        handle_seek(
            state,
            SeekInput {
                query: "core".into(),
                agent_id: "test".into(),
                top_k: 10,
                scope,
                node_types: vec![],
                min_score: 0.0,
                graph_rerank: true,
            },
        )
        .expect("seek should succeed")
    }

    fn run_validate_plan(
        state: &mut SessionState,
        file_path: String,
    ) -> crate::protocol::layers::ValidatePlanOutput {
        handle_validate_plan(
            state,
            ValidatePlanInput {
                agent_id: "test".into(),
                actions: vec![PlannedAction {
                    action_type: "modify".into(),
                    file_path,
                    description: Some("equivalence check".into()),
                    depends_on: vec![],
                }],
                include_test_impact: false,
                include_risk_score: false,
            },
        )
        .expect("validate_plan should succeed")
    }

    fn run_scan(
        state: &mut SessionState,
        scope: Option<String>,
    ) -> crate::protocol::layers::ScanOutput {
        handle_scan(
            state,
            ScanInput {
                agent_id: "test".into(),
                pattern: "core".into(),
                scope,
                limit: 10,
                severity_min: 0.0,
                graph_validate: false,
            },
        )
        .expect("scan should succeed")
    }

    #[test]
    fn trail_save_auto_derives_structural_boosts_from_context() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let output = super::handle_trail_save(
            &mut state,
            TrailSaveInput {
                agent_id: "test".into(),
                label: "dispatch continuity".into(),
                hypotheses: vec![],
                conclusions: vec![TrailConclusionInput {
                    statement: "ui depends on core".into(),
                    confidence: 0.9,
                    from_hypotheses: vec![],
                    supporting_nodes: vec!["file::src/ui.rs".into()],
                }],
                open_questions: vec![],
                tags: vec!["continuity".into()],
                summary: None,
                visited_nodes: vec![TrailVisitedNodeInput {
                    node_external_id: "file::src/core.rs".into(),
                    annotation: Some("entry file".into()),
                    relevance: 0.9,
                }],
                activation_boosts: HashMap::new(),
            },
        )
        .expect("trail save should succeed");

        let saved = super::load_trail(&state, &output.trail_id).expect("load saved trail");
        assert_eq!(saved.visited_nodes.len(), 2);
        assert!(saved
            .visited_nodes
            .iter()
            .any(|node| node.node_external_id == "file::src/ui.rs"));
        assert!(saved.activation_boosts.contains_key("file::src/core.rs"));
        assert!(saved.activation_boosts.contains_key("file::src/ui.rs"));
        assert!(saved.activation_boosts["file::src/ui.rs"] >= 0.8);
    }

    #[test]
    fn trail_resume_reactivates_auto_derived_structural_nodes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let saved = super::handle_trail_save(
            &mut state,
            TrailSaveInput {
                agent_id: "test".into(),
                label: "resume continuity".into(),
                hypotheses: vec![],
                conclusions: vec![TrailConclusionInput {
                    statement: "ui depends on core".into(),
                    confidence: 0.85,
                    from_hypotheses: vec![],
                    supporting_nodes: vec!["file::src/ui.rs".into()],
                }],
                open_questions: vec!["is there a test?".into()],
                tags: vec![],
                summary: None,
                visited_nodes: vec![TrailVisitedNodeInput {
                    node_external_id: "file::src/core.rs".into(),
                    annotation: None,
                    relevance: 0.8,
                }],
                activation_boosts: HashMap::new(),
            },
        )
        .expect("trail save should succeed");

        let resumed = super::handle_trail_resume(
            &mut state,
            TrailResumeInput {
                agent_id: "test".into(),
                trail_id: saved.trail_id,
                force: false,
                max_reactivated_nodes: 5,
                max_resume_hints: 4,
            },
        )
        .expect("trail resume should succeed");

        assert!(!resumed.stale);
        assert!(resumed.missing_nodes.is_empty());
        assert_eq!(resumed.nodes_reactivated, 2);
        assert_eq!(resumed.trail.node_count, 2);
        assert_eq!(resumed.reactivated_node_ids.len(), 2);
        assert!(resumed
            .reactivated_node_ids
            .iter()
            .any(|node| node == "file::src/ui.rs"));
        assert_eq!(
            resumed.next_focus_node_id.as_deref(),
            Some("file::src/ui.rs")
        );
        assert_eq!(
            resumed.next_open_question.as_deref(),
            Some("is there a test?")
        );
        assert_eq!(resumed.next_suggested_tool.as_deref(), Some("view"));
        assert!(resumed
            .resume_hints
            .iter()
            .any(|hint| hint.contains("open question")));
        assert!(resumed
            .resume_hints
            .iter()
            .any(|hint| hint.contains("Re-open the current focus")));
        assert!(resumed
            .resume_hints
            .iter()
            .any(|hint| hint.contains("file::src")));
    }

    #[test]
    fn trail_resume_respects_output_compaction_limits() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let saved = super::handle_trail_save(
            &mut state,
            TrailSaveInput {
                agent_id: "test".into(),
                label: "compact continuity".into(),
                hypotheses: vec![crate::protocol::layers::TrailHypothesisInput {
                    statement: "core reaches ui".into(),
                    confidence: 0.7,
                    supporting_nodes: vec!["file::src/core.rs".into()],
                    contradicting_nodes: vec![],
                }],
                conclusions: vec![TrailConclusionInput {
                    statement: "ui depends on core".into(),
                    confidence: 0.85,
                    from_hypotheses: vec![],
                    supporting_nodes: vec!["file::src/ui.rs".into()],
                }],
                open_questions: vec!["is there a test?".into(), "what changed last?".into()],
                tags: vec![],
                summary: None,
                visited_nodes: vec![TrailVisitedNodeInput {
                    node_external_id: "file::src/core.rs".into(),
                    annotation: None,
                    relevance: 0.8,
                }],
                activation_boosts: HashMap::new(),
            },
        )
        .expect("trail save should succeed");

        let resumed = super::handle_trail_resume(
            &mut state,
            TrailResumeInput {
                agent_id: "test".into(),
                trail_id: saved.trail_id,
                force: false,
                max_reactivated_nodes: 1,
                max_resume_hints: 1,
            },
        )
        .expect("trail resume should succeed");

        assert_eq!(resumed.reactivated_node_ids.len(), 1);
        assert_eq!(resumed.resume_hints.len(), 1);
        assert_eq!(resumed.next_suggested_tool.as_deref(), Some("view"));
    }

    #[test]
    fn trail_resume_suggests_timeline_for_temporal_follow_up_questions() {
        let suggested = super::trail_resume_suggested_tool(
            Some(&"file::src/core.rs".to_string()),
            Some(&"what changed last in this file?".to_string()),
        );

        assert_eq!(suggested.as_deref(), Some("timeline"));
    }

    #[test]
    fn trail_resume_suggests_impact_for_blast_radius_questions() {
        let suggested = super::trail_resume_suggested_tool(
            Some(&"file::src/core.rs".to_string()),
            Some(&"what breaks if we touch this file?".to_string()),
        );

        assert_eq!(suggested.as_deref(), Some("impact"));
    }

    #[test]
    fn trail_resume_suggests_hypothesize_for_structural_proof_questions() {
        let suggested = super::trail_resume_suggested_tool(
            Some(&"file::src/core.rs".to_string()),
            Some(&"why is this missing validation guard?".to_string()),
        );

        assert_eq!(suggested.as_deref(), Some("hypothesize"));
    }

    #[test]
    fn trail_resume_suggests_seek_for_locator_questions() {
        let suggested = super::trail_resume_suggested_tool(
            None,
            Some(&"which helper canonicalizes dispatch aliases?".to_string()),
        );

        assert_eq!(suggested.as_deref(), Some("seek"));
    }

    #[test]
    fn hypothesize_next_step_prefers_strongest_evidence_target() {
        let supporting = vec![crate::protocol::layers::HypothesisEvidence {
            evidence_type: "path_found".into(),
            description: "path".into(),
            likelihood_factor: 2.0,
            nodes: vec!["file::src/a.rs".into(), "file::src/b.rs".into()],
            relations: vec!["calls".into()],
            path_weight: Some(0.8),
        }];

        let (tool, target, hint) =
            super::l5_hypothesize_next_step("likely_true", &supporting, &[], None);

        assert_eq!(tool.as_deref(), Some("view"));
        assert_eq!(target.as_deref(), Some("file::src/b.rs"));
        assert!(
            hint.as_deref()
                .unwrap_or_default()
                .contains("strongest hypothesis evidence"),
            "hypothesize should guide the agent into the strongest evidence target"
        );
        assert_eq!(
            super::l5_hypothesize_proof_state("likely_true", &supporting, &[], None),
            "ready_to_edit"
        );
    }

    #[test]
    fn hypothesize_next_step_falls_back_to_partial_reach() {
        let partial = vec![crate::protocol::layers::PartialReachEntry {
            node_id: "file::src/reachable.rs".into(),
            label: "reachable".into(),
            hops_from_source: 2,
            activation_at_stop: 0.42,
        }];

        let (tool, target, hint) =
            super::l5_hypothesize_next_step("inconclusive", &[], &[], Some(&partial));

        assert_eq!(tool.as_deref(), Some("view"));
        assert_eq!(target.as_deref(), Some("file::src/reachable.rs"));
        assert!(
            hint.as_deref()
                .unwrap_or_default()
                .contains("partial-reach"),
            "hypothesize should still guide the next step when only partial reach exists"
        );
        assert_eq!(
            super::l5_hypothesize_proof_state("inconclusive", &[], &[], Some(&partial)),
            "proving"
        );
    }

    #[test]
    fn trace_proof_state_tracks_triage_strength() {
        let suspect = crate::protocol::layers::TraceSuspect {
            node_id: "file::src/core.rs".into(),
            label: "core".into(),
            node_type: "File".into(),
            suspiciousness: 0.62,
            signals: crate::protocol::layers::TraceSuspiciousnessSignals {
                trace_depth_score: 1.0,
                recency_score: 0.0,
                centrality_score: 0.4,
            },
            file_path: Some("src/core.rs".into()),
            line_start: None,
            line_end: None,
            related_callers: vec![],
        };

        assert_eq!(
            super::l6_trace_proof_state(1, std::slice::from_ref(&suspect), &[]),
            "triaging"
        );
        assert_eq!(
            super::l6_trace_proof_state(
                1,
                &[crate::protocol::layers::TraceSuspect {
                    suspiciousness: 0.81,
                    ..suspect
                }],
                &["core".into(), "leaf".into()]
            ),
            "ready_to_edit"
        );
    }

    #[test]
    fn timeline_proof_state_and_next_step_reflect_history_strength() {
        assert_eq!(super::timeline_proof_state(0, &[]), "blocked");
        assert_eq!(super::timeline_proof_state(1, &[]), "triaging");
        assert_eq!(
            super::timeline_proof_state(
                3,
                &[crate::protocol::layers::CoChangePartner {
                    file: "src/neighbor.rs".into(),
                    times: 2,
                    coupling_degree: 0.6,
                }]
            ),
            "proving"
        );

        let (tool, target, hint) = super::timeline_next_step(
            "src/core.rs",
            3,
            &[crate::protocol::layers::CoChangePartner {
                file: "src/neighbor.rs".into(),
                times: 2,
                coupling_degree: 0.6,
            }],
        );
        assert_eq!(tool.as_deref(), Some("view"));
        assert_eq!(target.as_deref(), Some("src/neighbor.rs"));
        assert!(hint
            .as_deref()
            .is_some_and(|value| value.contains("strongest co-change partner")));
    }

    #[test]
    fn trail_resume_hints_start_with_tool_specific_next_move() {
        let hints = super::trail_resume_hints(
            &TrailData {
                trail_id: "trail-1".into(),
                label: "continuity".into(),
                agent_id: "test".into(),
                status: "saved".into(),
                visited_nodes: vec![],
                activation_boosts: HashMap::new(),
                graph_generation: 1,
                created_at_ms: 0,
                last_modified_ms: 0,
                hypotheses: vec![],
                conclusions: vec![],
                open_questions: vec!["what changed last in this file?".into()],
                tags: vec![],
                summary: None,
                source_trails: vec![],
            },
            &["file::src/core.rs".into()],
            Some(&"file::src/core.rs".into()),
            Some(&"what changed last in this file?".into()),
            Some("timeline"),
            3,
        );

        assert_eq!(
            hints.first().map(String::as_str),
            Some("Use timeline on file::src/core.rs to answer the carried-forward question: what changed last in this file?")
        );
    }

    #[test]
    fn normalize_path_hint_equates_relative_absolute_and_file_forms() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let absolute = root.join("src/core.rs").to_string_lossy().to_string();
        assert_eq!(
            super::l6_vp_normalize_path("src/core.rs", &state.ingest_roots),
            "src/core.rs"
        );
        assert_eq!(
            super::l6_vp_normalize_path(&absolute, &state.ingest_roots),
            "src/core.rs"
        );
        assert_eq!(
            super::l6_vp_normalize_path("file::src/core.rs", &state.ingest_roots),
            "src/core.rs"
        );
        assert_eq!(
            super::l7_normalize_path_hint("src/core.rs", &state.ingest_roots),
            "src/core.rs"
        );
        assert_eq!(
            super::l7_normalize_path_hint(&absolute, &state.ingest_roots),
            "src/core.rs"
        );
        assert_eq!(
            super::l7_normalize_path_hint("file::src/core.rs", &state.ingest_roots),
            "src/core.rs"
        );
    }

    #[test]
    fn node_to_file_path_strips_repo_prefixes_and_symbol_suffixes() {
        assert_eq!(super::node_to_file_path("file::src/core.rs"), "src/core.rs");
        assert_eq!(
            super::node_to_file_path("file::src/core.rs::fn::boot"),
            "src/core.rs"
        );
        assert_eq!(
            super::node_to_file_path("m1nd::file::src/core.rs::fn::boot"),
            "src/core.rs"
        );
    }

    #[test]
    fn resolve_timeline_file_path_canonicalizes_equivalent_id_shapes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let state = build_layer_state(root);
        let absolute = root.join("src/core.rs").to_string_lossy().to_string();

        for candidate in [
            "file::src/core.rs",
            "file::src/core.rs::fn::boot",
            "m1nd::file::src/core.rs::fn::boot",
            absolute.as_str(),
        ] {
            assert_eq!(
                super::resolve_timeline_file_path(&state, candidate),
                "src/core.rs"
            );
        }
    }

    #[test]
    fn parse_git_log_output_preserves_commit_subjects() {
        let raw = "\
abc1234|2026-03-24 10:00:00 +0000|max kle1nz|fix: harden timeline proof path
12\t3\tsrc/core.rs

def5678|2026-03-23 09:00:00 +0000|max kle1nz|feat: add benchmark harness
4\t0\tdocs/benchmarks/README.md
";

        let commits = super::parse_git_log_output(raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "fix: harden timeline proof path");
        assert_eq!(commits[1].subject, "feat: add benchmark harness");
    }

    #[test]
    fn churn_for_file_matches_repo_relative_suffix_before_basename_fallback() {
        let commit = super::GitCommitRecord {
            hash: "abc1234".into(),
            date: "2026-03-24 10:00:00 +0000".into(),
            author: "max kle1nz".into(),
            subject: "fix: preserve recent proof".into(),
            files_changed: vec![
                super::FileChurn {
                    path: "m1nd-mcp/src/layer_handlers.rs".into(),
                    added: 17,
                    deleted: 2,
                },
                super::FileChurn {
                    path: "other/src/layer_handlers.rs".into(),
                    added: 99,
                    deleted: 1,
                },
            ],
        };

        assert_eq!(
            commit.churn_for_file("src/layer_handlers.rs"),
            (17, 2),
            "timeline should prefer the repo-relative suffix match over a same-basename distractor"
        );
    }

    #[test]
    fn seek_tokenize_dedupes_stopwords_and_identifier_parts() {
        let tokens = super::l2_seek_tokenize(
            "Where do we normalize `dispatch_aliases` into canonical dispatch status names?",
        );

        assert!(!tokens.iter().any(|token| token == "where"));
        assert!(!tokens.iter().any(|token| token == "do"));
        assert_eq!(
            tokens
                .iter()
                .filter(|token| token.as_str() == "dispatch")
                .count(),
            1
        );
        assert!(tokens.iter().any(|token| token == "aliases"));
        assert!(tokens.iter().any(|token| token == "canonical"));
        assert!(tokens.iter().any(|token| token == "status"));
    }

    #[test]
    fn seek_normalizes_relative_absolute_and_file_scopes_equivalently() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let file_scope = run_seek(&mut state, Some("file::src".into()));
        let absolute_scope = run_seek(
            &mut state,
            Some(root.join("src").to_string_lossy().to_string()),
        );
        let relative_scope = run_seek(&mut state, Some("src".into()));

        for output in [&file_scope, &absolute_scope, &relative_scope] {
            assert_eq!(output.results.len(), 1, "seek should narrow to one result");
            assert_eq!(output.results[0].node_id, "file::src/core.rs");
            assert_eq!(output.results[0].label, "core.rs");
            assert_eq!(output.total_candidates_scanned, 2);
            assert!(
                output.results[0].heuristic_signals.is_some(),
                "seek should surface heuristic metadata"
            );
        }

        assert_eq!(
            file_scope.results[0].node_id,
            absolute_scope.results[0].node_id
        );
        assert_eq!(
            file_scope.results[0].node_id,
            relative_scope.results[0].node_id
        );
        assert_eq!(file_scope.results[0].score, absolute_scope.results[0].score);
        assert_eq!(file_scope.results[0].score, relative_scope.results[0].score);
    }

    #[test]
    fn seek_handles_natural_language_query_for_dispatch_alias_normalization() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_seek_natural_language_state(root);

        let output = handle_seek(
            &mut state,
            SeekInput {
                query: "Where do we normalize alias tool names into the canonical dispatch status name?"
                    .into(),
                agent_id: "test".into(),
                top_k: 5,
                scope: None,
                node_types: vec![],
                min_score: 0.0,
                graph_rerank: true,
            },
        )
        .expect("seek should succeed");

        assert!(
            !output.results.is_empty(),
            "seek should find a dispatch target"
        );
        assert_eq!(
            output.results[0].node_id,
            "file::m1nd-mcp/src/server.rs::fn::normalize_dispatch_tool_name"
        );
        assert_eq!(output.results[0].node_type, "function");
    }

    #[test]
    fn seek_biases_alias_canonical_dispatch_cluster_toward_normalization_helper() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_seek_natural_language_state(root);

        let output = handle_seek(
            &mut state,
            SeekInput {
                query: "Which helper maps alias tool names into canonical dispatch status values before execution?"
                    .into(),
                agent_id: "test".into(),
                top_k: 5,
                scope: None,
                node_types: vec![],
                min_score: 0.0,
                graph_rerank: true,
            },
        )
        .expect("seek should succeed");

        assert!(
            !output.results.is_empty(),
            "seek should find a dispatch helper"
        );
        assert_eq!(
            output.results[0].node_id,
            "file::m1nd-mcp/src/server.rs::fn::normalize_dispatch_tool_name"
        );
        assert!(
            output
                .results
                .iter()
                .any(|result| result.node_id == "file::docs/dispatch-aliases.md"),
            "docs distractor should remain visible in the candidate set"
        );
        assert_eq!(output.next_suggested_tool.as_deref(), Some("view"));
        assert_eq!(
            output.next_suggested_target.as_deref(),
            Some("m1nd-mcp/src/server.rs")
        );
        assert!(
            output
                .next_step_hint
                .as_deref()
                .unwrap_or_default()
                .contains("normalize_dispatch_tool_name"),
            "seek should suggest opening the strongest result next"
        );
        assert_eq!(output.proof_state, "proving");
    }

    #[test]
    fn scan_normalizes_relative_absolute_and_file_scopes_equivalently() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let file_scope = run_scan(&mut state, Some("file::src".into()));
        let absolute_scope = run_scan(
            &mut state,
            Some(root.join("src").to_string_lossy().to_string()),
        );
        let relative_scope = run_scan(&mut state, Some("src".into()));

        for output in [&file_scope, &absolute_scope, &relative_scope] {
            assert_eq!(output.total_matches_raw, 1);
            assert_eq!(output.findings.len(), 1);
            assert_eq!(output.findings[0].node_id, "file::src/core.rs");
        }
    }

    #[test]
    fn validate_plan_normalizes_relative_absolute_and_file_paths_equivalently() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let relative_path = run_validate_plan(&mut state, "src/core.rs".into());
        let absolute_path = run_validate_plan(
            &mut state,
            root.join("src/core.rs").to_string_lossy().to_string(),
        );
        let file_uri_path = run_validate_plan(&mut state, "file::src/core.rs".into());

        for output in [&relative_path, &absolute_path, &file_uri_path] {
            assert_eq!(output.actions_analyzed, 1);
            assert_eq!(output.actions_resolved, 1);
            assert_eq!(output.actions_unresolved, 0);
        }

        assert_eq!(relative_path.gaps.len(), absolute_path.gaps.len());
        assert_eq!(relative_path.gaps.len(), file_uri_path.gaps.len());
        assert_eq!(
            relative_path.blast_radius_total,
            absolute_path.blast_radius_total
        );
        assert_eq!(
            relative_path.blast_radius_total,
            file_uri_path.blast_radius_total
        );
        assert!((relative_path.risk_score - absolute_path.risk_score).abs() < f32::EPSILON);
        assert!((relative_path.risk_score - file_uri_path.risk_score).abs() < f32::EPSILON);
    }

    #[test]
    fn layers_normalize_absolute_scope_under_ingest_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);
        assert_eq!(
            state.graph.read().num_nodes(),
            2,
            "test graph must be populated"
        );
        assert!(state.graph.read().finalized, "test graph must be finalized");
        let detector = m1nd_core::layer::LayerDetector::with_defaults();
        assert!(
            detector
                .detect(&state.graph.read(), None, &[], false, "auto")
                .is_ok(),
            "unscoped detector should succeed on a populated graph"
        );
        let normalized_scope = super::l7_normalize_layer_scope(
            Some(&root.join("src").to_string_lossy()),
            &state.ingest_roots,
        )
        .expect("normalized scope");
        assert_eq!(normalized_scope, "file::src");

        let input = LayersInput {
            agent_id: "test".into(),
            scope: Some(root.join("src").to_string_lossy().to_string()),
            max_layers: 8,
            include_violations: true,
            min_nodes_per_layer: 2,
            node_types: vec![],
            naming_strategy: "auto".into(),
            exclude_tests: false,
            violation_limit: 10,
        };

        let output = handle_layers(&mut state, input).expect("layers should succeed");
        let layers = output
            .get("layers")
            .and_then(|v| v.as_array())
            .expect("layers array");
        assert!(
            !layers.is_empty(),
            "absolute scope under ingest root should resolve to populated layers"
        );
        let summary = output.get("summary").expect("summary");
        assert!(
            summary
                .get("total_nodes_classified")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                > 0,
            "layer summary should classify at least one node"
        );
    }

    #[test]
    fn validate_plan_resolves_absolute_paths_under_ingest_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let output = handle_validate_plan(
            &mut state,
            ValidatePlanInput {
                agent_id: "test".into(),
                actions: vec![PlannedAction {
                    action_type: "modify".into(),
                    file_path: root.join("src/core.rs").to_string_lossy().to_string(),
                    description: Some("absolute path should normalize".into()),
                    depends_on: vec![],
                }],
                include_test_impact: false,
                include_risk_score: false,
            },
        )
        .expect("validate_plan should succeed");

        assert_eq!(output.actions_resolved, 1);
        assert_eq!(output.actions_unresolved, 0);
    }

    #[test]
    fn validate_plan_resolves_absolute_paths_via_provenance_suffix_fallback() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);

        let output = handle_validate_plan(
            &mut state,
            ValidatePlanInput {
                agent_id: "test".into(),
                actions: vec![PlannedAction {
                    action_type: "modify".into(),
                    file_path: root.join("src/core.rs").to_string_lossy().to_string(),
                    description: Some("absolute path should resolve from provenance".into()),
                    depends_on: vec![],
                }],
                include_test_impact: true,
                include_risk_score: true,
            },
        )
        .expect("validate_plan should succeed");

        assert_eq!(output.actions_resolved, 1);
        assert_eq!(output.actions_unresolved, 0);
        assert!(
            output.heuristic_summary.is_some(),
            "resolved plan should emit heuristic summary"
        );
    }

    #[test]
    fn validate_plan_surfaces_heuristic_hotspots_for_risky_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state(root);
        let now = 10_000.0;

        state
            .trust_ledger
            .record_defect("file::src/core.rs", now - 500.0);
        state
            .trust_ledger
            .record_defect("file::src/core.rs", now - 250.0);
        state
            .tremor_registry
            .record_observation("file::src/core.rs", 0.8, 3, now - 300.0);
        state
            .tremor_registry
            .record_observation("file::src/core.rs", 1.0, 4, now - 200.0);
        state
            .tremor_registry
            .record_observation("file::src/core.rs", 1.3, 5, now - 100.0);
        state.antibodies.push(m1nd_core::antibody::Antibody {
            id: "ab_test_core".into(),
            name: "core-risk".into(),
            description: "Test antibody for core.rs".into(),
            pattern: m1nd_core::antibody::AntibodyPattern {
                nodes: vec![m1nd_core::antibody::PatternNode {
                    role: "file".into(),
                    node_type: Some("file".into()),
                    required_tags: vec![],
                    label_contains: Some("core".into()),
                }],
                edges: vec![],
                negative_edges: vec![],
            },
            severity: m1nd_core::antibody::AntibodySeverity::Warning,
            match_count: 0,
            created_at: now,
            last_match_at: None,
            created_by: "test".into(),
            source_query: "core".into(),
            source_nodes: vec!["file::src/core.rs".into()],
            enabled: true,
            specificity: 0.8,
        });

        let output = handle_validate_plan(
            &mut state,
            ValidatePlanInput {
                agent_id: "test".into(),
                actions: vec![PlannedAction {
                    action_type: "modify".into(),
                    file_path: "src/core.rs".into(),
                    description: Some("heuristic hotspot".into()),
                    depends_on: vec![],
                }],
                include_test_impact: true,
                include_risk_score: true,
            },
        )
        .expect("validate_plan should succeed");

        let summary = output
            .heuristic_summary
            .expect("validate_plan should emit heuristic summary");
        assert!(
            summary.heuristic_risk > 0.0,
            "heuristic risk should contribute to validate_plan"
        );
        assert!(
            summary
                .hotspots
                .iter()
                .any(|hotspot| hotspot.file_path == "src/core.rs"
                    && hotspot.role == "planned"
                    && hotspot.antibody_hits == 1),
            "planned file should surface as a heuristic hotspot"
        );
        let hotspot = summary
            .hotspots
            .iter()
            .find(|hotspot| hotspot.file_path == "src/core.rs")
            .expect("planned hotspot ref");
        assert!(
            hotspot
                .proof_hint
                .contains("src/core.rs is already in the plan"),
            "validate_plan should emit a compact proof hint with the hotspot"
        );
        assert!(
            hotspot.proof_hint.contains("immune-memory recurrence"),
            "proof hint should carry the main heuristic reason"
        );
        assert_eq!(hotspot.heuristics_surface_ref.node_id, "file::src/core.rs");
        assert_eq!(hotspot.heuristics_surface_ref.file_path, "src/core.rs");
        assert_eq!(
            output.next_suggested_tool.as_deref(),
            Some("heuristics_surface")
        );
        assert_eq!(output.next_suggested_target.as_deref(), Some("src/core.rs"));
        assert_eq!(output.proof_state, "proving");
        assert!(output
            .next_step_hint
            .as_deref()
            .is_some_and(|hint| hint.contains("Inspect src/core.rs next")));
        assert!(output
            .gaps
            .iter()
            .all(|gap| gap.heuristics_surface_ref.is_some()));
        assert!(
            output.risk_score > 0.0,
            "risk score should include heuristic contribution"
        );
    }

    #[test]
    fn validate_plan_suppresses_manifest_noise_in_gap_suggestions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_layer_state_with_manifest_gap(root);

        let output = handle_validate_plan(
            &mut state,
            ValidatePlanInput {
                agent_id: "test".into(),
                actions: vec![PlannedAction {
                    action_type: "modify".into(),
                    file_path: "src/core.rs".into(),
                    description: Some("change core".into()),
                    depends_on: vec![],
                }],
                include_test_impact: false,
                include_risk_score: true,
            },
        )
        .expect("validate_plan should succeed");

        assert!(
            output.gaps.iter().all(|gap| gap.file_path != "Cargo.toml"),
            "validate_plan should suppress manifest-only noise by default"
        );
        assert!(
            output
                .suggested_additions
                .iter()
                .all(|item| item.file_path != "Cargo.toml"),
            "suggested additions should not reintroduce suppressed manifest noise"
        );
    }
}
