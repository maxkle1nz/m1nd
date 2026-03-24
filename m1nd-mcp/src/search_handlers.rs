// === m1nd-mcp/src/search_handlers.rs ===
//
// v0.5.0: Handlers for m1nd.search, m1nd.glob, and m1nd.help.
// Search: literal/regex/semantic modes with graph context.
//   - v0.5.0: regex mode gets Phase 2 disk search (fixes CRITICAL gap)
//   - v0.5.0: multiline, invert, count_only, filename_pattern support
// Glob: graph-aware file pattern matching (replaces find/glob).
// Help: self-documenting tool reference with visual identity.

use crate::personality;
use crate::protocol::layers::{
    GlobFileEntry, GlobInput, GlobOutput, HelpInput, HelpOutput, SearchInput, SearchMode,
    SearchOutput, SearchResultEntry,
};
use crate::scope::normalize_scope_path;
use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Shared normalization helpers
// ---------------------------------------------------------------------------

/// Normalize a tool name for help lookup.
///
/// Accepts raw names (`activate`) plus common aliases (`m1nd_activate`,
/// `m1nd.activate`) and returns the canonical tool name used by `tool_docs()`.
pub(crate) fn normalize_help_tool_name(tool_name: &str) -> String {
    let trimmed = tool_name.trim();
    trimmed
        .strip_prefix("m1nd.")
        .or_else(|| trimmed.strip_prefix("m1nd_"))
        .unwrap_or(trimmed)
        .to_string()
}

/// Resolve a path hint against ingest roots.
///
/// This is intentionally small and reusable: later patches can swap the policy
/// without changing every handler that needs a canonical path primitive.
pub(crate) fn canonicalize_path_hint(path_hint: &str, ingest_roots: &[String]) -> PathBuf {
    let path = Path::new(path_hint);
    if path.is_absolute() {
        return path.to_path_buf();
    }

    if let Some(root) = ingest_roots.first() {
        return Path::new(root).join(path);
    }

    path.to_path_buf()
}

fn normalize_scope_hint(scope: Option<&str>, ingest_roots: &[String]) -> Option<String> {
    normalize_scope_path(scope, ingest_roots)
}

fn scope_matches_path(path_like: &str, scope: Option<&str>, ingest_roots: &[String]) -> bool {
    let Some(scope) = scope else {
        return true;
    };

    normalize_scope_path(Some(path_like), ingest_roots)
        .map(|path| path.starts_with(scope))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Shared matcher trait for Phase 2 file content search (fixes GAP 2)
// ---------------------------------------------------------------------------

/// Abstraction over literal and regex matching for file content search.
/// This enables Phase 2 (disk file search) to work for BOTH literal and regex modes.
trait LineMatcher {
    /// Returns true if the line matches the pattern.
    fn matches(&self, line: &str) -> bool;
}

/// Literal substring matcher (case-insensitive by default).
struct LiteralMatcher {
    pattern: String,
    case_sensitive: bool,
}

impl LineMatcher for LiteralMatcher {
    fn matches(&self, line: &str) -> bool {
        if self.case_sensitive {
            line.contains(&self.pattern)
        } else {
            line.to_lowercase().contains(&self.pattern)
        }
    }
}

/// Regex line-by-line matcher.
struct RegexMatcher {
    re: regex::Regex,
}

impl LineMatcher for RegexMatcher {
    fn matches(&self, line: &str) -> bool {
        self.re.is_match(line)
    }
}

#[derive(Clone, Debug)]
struct SearchFileCandidate {
    rel_path: String,
    full_path: PathBuf,
    graph_linked: bool,
}

#[derive(Clone, Debug)]
struct AutoIngestScopeCandidate {
    resolved_path: PathBuf,
    ingest_root: PathBuf,
    scope_override: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct AutoIngestSearchState {
    auto_ingested_paths: Vec<String>,
    scope_override: Option<String>,
}

#[derive(Clone, Copy)]
enum SearchRankingMode {
    Literal,
    Regex,
    Semantic,
}

// ---------------------------------------------------------------------------
// m1nd.search
// ---------------------------------------------------------------------------

pub fn handle_search(state: &mut SessionState, input: SearchInput) -> M1ndResult<SearchOutput> {
    let start = Instant::now();

    // Validate
    if input.query.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "m1nd_search".into(),
            detail: "query cannot be empty".into(),
        });
    }

    // Validate filename_pattern if provided
    let filename_glob = if let Some(ref pat) = input.filename_pattern {
        Some(
            glob::Pattern::new(pat).map_err(|e| M1ndError::InvalidParams {
                tool: "m1nd_search".into(),
                detail: format!("invalid filename pattern '{}': {}", pat, e),
            })?,
        )
    } else {
        None
    };

    // Clamp parameters (ADVERSARY S2: hard cap at 500)
    let top_k = (input.top_k as usize).clamp(1, 500);
    let context_lines = input.context_lines.clamp(0, 10);
    let auto_ingest_state = maybe_auto_ingest_search_scope(state, &input)?;
    let auto_ingested = !auto_ingest_state.auto_ingested_paths.is_empty();

    let graph = state.graph.read();
    let scope = auto_ingest_state
        .scope_override
        .clone()
        .or_else(|| normalize_scope_hint(input.scope.as_deref(), &state.ingest_roots));
    let scope = scope.as_deref();
    let scope_applied = scope.is_some();

    let mut results: Vec<SearchResultEntry> = Vec::new();
    let mut total_matches: usize = 0;
    let collect_limit = top_k.saturating_mul(8).clamp(32, 2000);

    match input.mode {
        SearchMode::Literal => {
            // Phase 1: Match node labels in graph
            let query_pattern = if input.case_sensitive {
                input.query.clone()
            } else {
                input.query.to_lowercase()
            };

            if !input.invert {
                // Normal (non-inverted) Phase 1: node label matching
                for (interned, &_nid) in graph.id_to_node.iter() {
                    let ext_id = graph.strings.resolve(*interned);

                    if !scope_matches_path(ext_id, scope, &state.ingest_roots) {
                        continue;
                    }

                    let match_target = if input.case_sensitive {
                        ext_id.to_string()
                    } else {
                        ext_id.to_lowercase()
                    };

                    if match_target.contains(&query_pattern) {
                        total_matches += 1;
                        if !input.count_only && results.len() < collect_limit {
                            let (file_path, line_number) = extract_provenance(&graph, ext_id);
                            let (ctx_before, ctx_after) =
                                get_context_lines(&file_path, line_number, context_lines);
                            results.push(SearchResultEntry {
                                node_id: ext_id.to_string(),
                                label: ext_id.to_string(),
                                node_type: guess_node_type(ext_id),
                                score: None,
                                file_path,
                                line_number,
                                matched_line: ext_id.to_string(),
                                context_before: ctx_before,
                                context_after: ctx_after,
                                graph_linked: true,
                                heuristic_signals: None,
                            });
                        }
                    }
                }
            }

            // Phase 2: Search file contents on disk (the real grep replacement)
            let matcher = LiteralMatcher {
                pattern: query_pattern,
                case_sensitive: input.case_sensitive,
            };
            search_file_contents(
                state,
                &graph,
                scope,
                &matcher,
                input.invert,
                input.count_only,
                collect_limit,
                context_lines,
                filename_glob.as_ref(),
                &mut results,
                &mut total_matches,
            );
        }
        SearchMode::Regex => {
            // Build regex (ADVERSARY S1: safe linear-time regex only)
            let pattern = if input.case_sensitive {
                input.query.clone()
            } else {
                format!("(?i){}", input.query)
            };

            // v0.5.0: multiline support via RegexBuilder
            let re = if input.multiline {
                regex::RegexBuilder::new(&pattern)
                    .dot_matches_new_line(true)
                    .multi_line(true)
                    .build()
            } else {
                regex::Regex::new(&pattern)
            }
            .map_err(|e| M1ndError::InvalidParams {
                tool: "m1nd_search".into(),
                detail: format!("invalid regex: {}", e),
            })?;

            // Phase 1: Match node labels in graph (non-inverted only)
            if !input.invert {
                for (interned, &_nid) in graph.id_to_node.iter() {
                    let ext_id = graph.strings.resolve(*interned);

                    if !scope_matches_path(ext_id, scope, &state.ingest_roots) {
                        continue;
                    }

                    if re.is_match(ext_id) {
                        total_matches += 1;
                        if !input.count_only && results.len() < collect_limit {
                            let (file_path, line_number) = extract_provenance(&graph, ext_id);
                            let (ctx_before, ctx_after) =
                                get_context_lines(&file_path, line_number, context_lines);

                            results.push(SearchResultEntry {
                                node_id: ext_id.to_string(),
                                label: ext_id.to_string(),
                                node_type: guess_node_type(ext_id),
                                score: None,
                                file_path,
                                line_number,
                                matched_line: ext_id.to_string(),
                                context_before: ctx_before,
                                context_after: ctx_after,
                                graph_linked: true,
                                heuristic_signals: None,
                            });
                        }
                    }
                }
            }

            // v0.5.0 FIX (CRITICAL GAP 2): Phase 2 for regex mode
            // Multiline regex searches whole file content; line-by-line regex uses RegexMatcher
            if input.multiline {
                // Multiline: read entire file as one string, find all matches
                search_file_contents_multiline(
                    state,
                    &graph,
                    scope,
                    &re,
                    input.invert,
                    input.count_only,
                    collect_limit,
                    context_lines,
                    filename_glob.as_ref(),
                    &mut results,
                    &mut total_matches,
                );
            } else {
                // Line-by-line regex (same as literal but with regex matcher)
                let matcher = RegexMatcher { re };
                search_file_contents(
                    state,
                    &graph,
                    scope,
                    &matcher,
                    input.invert,
                    input.count_only,
                    collect_limit,
                    context_lines,
                    filename_glob.as_ref(),
                    &mut results,
                    &mut total_matches,
                );
            }
        }
        SearchMode::Semantic => {
            // Delegate to existing seek logic via orchestrator
            drop(graph); // Release read lock before calling orchestrator
            let seek_input = crate::protocol::layers::SeekInput {
                agent_id: input.agent_id.clone(),
                query: input.query.clone(),
                top_k,
                scope: normalize_scope_hint(input.scope.as_deref(), &state.ingest_roots),
                node_types: vec![],
                min_score: 0.0,
                graph_rerank: true,
            };
            let seek_result = crate::layer_handlers::handle_seek(state, seek_input)?;

            // Convert seek results to search format
            total_matches = seek_result.results.len();
            for item in seek_result.results.into_iter().take(collect_limit) {
                let matched_line = item
                    .excerpt
                    .as_deref()
                    .and_then(|excerpt| excerpt.lines().next())
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| item.intent_summary.clone());
                results.push(SearchResultEntry {
                    file_path: item
                        .file_path
                        .clone()
                        .unwrap_or_else(|| item.node_id.clone()),
                    line_number: item.line_start.unwrap_or(1),
                    matched_line,
                    context_before: vec![],
                    context_after: vec![],
                    graph_linked: true,
                    score: Some(item.score),
                    heuristic_signals: item.heuristic_signals,
                    node_id: item.node_id,
                    label: item.label,
                    node_type: item.node_type,
                });
            }

            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            return Ok(SearchOutput {
                query: input.query,
                mode: "semantic".into(),
                results,
                total_matches,
                scope_applied,
                elapsed_ms: elapsed,
                auto_ingested,
                match_count: None,
                auto_ingested_paths: auto_ingest_state.auto_ingested_paths,
            });
        }
    }

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    // v0.5.0: count_only — clear results, set match_count
    let match_count = if input.count_only {
        Some(total_matches)
    } else {
        None
    };
    let final_results = if input.count_only {
        vec![]
    } else {
        let ranking_mode = match input.mode {
            SearchMode::Literal => SearchRankingMode::Literal,
            SearchMode::Regex => SearchRankingMode::Regex,
            SearchMode::Semantic => SearchRankingMode::Semantic,
        };
        rank_search_results(&input.query, ranking_mode, &mut results);
        results.truncate(top_k);
        results
    };

    Ok(SearchOutput {
        query: input.query,
        mode: format!("{:?}", input.mode).to_lowercase(),
        results: final_results,
        total_matches,
        scope_applied,
        elapsed_ms: elapsed,
        auto_ingested,
        match_count,
        auto_ingested_paths: auto_ingest_state.auto_ingested_paths,
    })
}

fn maybe_auto_ingest_search_scope(
    state: &mut SessionState,
    input: &SearchInput,
) -> M1ndResult<AutoIngestSearchState> {
    if !input.auto_ingest {
        return Ok(AutoIngestSearchState::default());
    }

    let Some(scope) = input.scope.as_deref().map(str::trim) else {
        return Ok(AutoIngestSearchState::default());
    };

    let scope = scope.strip_prefix("file::").unwrap_or(scope);

    let mut candidates = resolve_auto_ingest_scope_candidates(state, scope);
    if candidates.is_empty() {
        return Ok(AutoIngestSearchState::default());
    }

    if candidates.len() > 1 {
        let candidates_list = candidates
            .iter()
            .map(|path| path.resolved_path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(M1ndError::InvalidParams {
            tool: "m1nd_search".into(),
            detail: format!(
                "scope '{}' resolves to {} candidate paths: [{}]. Set a more specific scope.",
                scope,
                candidates.len(),
                candidates_list
            ),
        });
    }

    let candidate = candidates.remove(0);
    let scope_path = candidate.resolved_path;

    if !scope_path.exists() || path_within_roots(state, &scope_path) {
        return Ok(AutoIngestSearchState::default());
    }

    let ingest_target = candidate.ingest_root;

    crate::tools::handle_ingest(
        state,
        crate::protocol::IngestInput {
            path: ingest_target.to_string_lossy().to_string(),
            agent_id: input.agent_id.clone(),
            mode: "merge".to_string(),
            incremental: true,
            adapter: "code".to_string(),
            namespace: None,
        },
    )?;

    Ok(AutoIngestSearchState {
        auto_ingested_paths: vec![ingest_target.to_string_lossy().to_string()],
        scope_override: candidate.scope_override,
    })
}

fn resolve_auto_ingest_scope_candidates(
    state: &SessionState,
    scope: &str,
) -> Vec<AutoIngestScopeCandidate> {
    use std::collections::HashSet;

    let candidates = Path::new(scope);

    if candidates.is_absolute() {
        let ingest_root = if candidates.is_dir() {
            candidates.to_path_buf()
        } else {
            candidates
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| candidates.to_path_buf())
        };
        return vec![AutoIngestScopeCandidate {
            resolved_path: candidates.to_path_buf(),
            ingest_root: ingest_root.clone(),
            scope_override: candidates
                .strip_prefix(&ingest_root)
                .ok()
                .and_then(normalize_relative_scope_for_search),
        }];
    }

    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    let first_component = candidates
        .components()
        .next()
        .map(|component| component.as_os_str().to_os_string());

    for root in &state.ingest_roots {
        let base_root = Path::new(root);
        let resolved_path = base_root.join(scope);
        if resolved_path.exists() && seen.insert(resolved_path.clone()) {
            let ingest_root = first_component
                .as_ref()
                .map(|component| base_root.join(component))
                .unwrap_or_else(|| resolved_path.clone());
            resolved.push(AutoIngestScopeCandidate {
                resolved_path,
                ingest_root,
                scope_override: candidates
                    .strip_prefix(
                        first_component
                            .as_ref()
                            .map(PathBuf::from)
                            .as_deref()
                            .unwrap_or(candidates),
                    )
                    .ok()
                    .and_then(normalize_relative_scope_for_search),
            });
        }
    }

    if let Some(workspace_root) = &state.workspace_root {
        let base_root = Path::new(workspace_root);
        let resolved_path = base_root.join(scope);
        if resolved_path.exists() && seen.insert(resolved_path.clone()) {
            let ingest_root = first_component
                .as_ref()
                .map(|component| base_root.join(component))
                .unwrap_or_else(|| resolved_path.clone());
            resolved.push(AutoIngestScopeCandidate {
                resolved_path,
                ingest_root,
                scope_override: candidates
                    .strip_prefix(
                        first_component
                            .as_ref()
                            .map(PathBuf::from)
                            .as_deref()
                            .unwrap_or(candidates),
                    )
                    .ok()
                    .and_then(normalize_relative_scope_for_search),
            });
        }
    }

    resolved
}

fn normalize_relative_scope_for_search(path: &Path) -> Option<String> {
    let value = path.to_string_lossy().trim_matches('/').to_string();
    if value.is_empty() || value == "." {
        None
    } else {
        Some(value)
    }
}

// ---------------------------------------------------------------------------
// Phase 2: Shared file content search (fixes GAP 2 — works for literal+regex)
// ---------------------------------------------------------------------------

/// Collect unique file:: nodes from the graph, resolve to disk paths,
/// and search their contents line-by-line using the provided matcher.
/// Supports invert, count_only, filename_pattern filtering.
#[allow(clippy::too_many_arguments)]
fn search_file_contents(
    state: &SessionState,
    graph: &m1nd_core::graph::Graph,
    scope: Option<&str>,
    matcher: &dyn LineMatcher,
    invert: bool,
    count_only: bool,
    top_k: usize,
    context_lines: u32,
    filename_glob: Option<&glob::Pattern>,
    results: &mut Vec<SearchResultEntry>,
    total_matches: &mut usize,
) {
    let candidates = collect_search_files(state, graph, scope, filename_glob);

    for candidate in &candidates {
        if !count_only && results.len() >= top_k {
            break;
        }

        if let Ok(content) = std::fs::read_to_string(&candidate.full_path) {
            for (line_idx, line) in content.lines().enumerate() {
                let is_match = matcher.matches(line);
                let include = if invert { !is_match } else { is_match };

                if include {
                    *total_matches += 1;
                    if !count_only && results.len() < top_k {
                        let ln = (line_idx + 1) as u32;
                        let fp = candidate.full_path.to_string_lossy().to_string();
                        let (ctx_before, ctx_after) = get_context_lines(&fp, ln, context_lines);
                        results.push(SearchResultEntry {
                            node_id: format!("file::{}", candidate.rel_path),
                            label: candidate.rel_path.clone(),
                            node_type: "FileContent".into(),
                            score: None,
                            file_path: fp,
                            line_number: ln,
                            matched_line: line.to_string(),
                            context_before: ctx_before,
                            context_after: ctx_after,
                            graph_linked: candidate.graph_linked,
                            heuristic_signals: None,
                        });
                    }
                }
            }
        }
    }
}

/// Multiline regex search: reads entire file content as one string,
/// finds all regex matches that may span multiple lines.
#[allow(clippy::too_many_arguments)]
fn search_file_contents_multiline(
    state: &SessionState,
    graph: &m1nd_core::graph::Graph,
    scope: Option<&str>,
    re: &regex::Regex,
    invert: bool,
    count_only: bool,
    top_k: usize,
    context_lines: u32,
    filename_glob: Option<&glob::Pattern>,
    results: &mut Vec<SearchResultEntry>,
    total_matches: &mut usize,
) {
    let candidates = collect_search_files(state, graph, scope, filename_glob);

    for candidate in &candidates {
        if !count_only && results.len() >= top_k {
            break;
        }

        if let Ok(content) = std::fs::read_to_string(&candidate.full_path) {
            if invert {
                // Invert multiline: count lines NOT in any match span
                let match_ranges: Vec<(usize, usize)> = re
                    .find_iter(&content)
                    .map(|m| (m.start(), m.end()))
                    .collect();
                for (line_idx, line) in content.lines().enumerate() {
                    let line_start = content
                        .lines()
                        .take(line_idx)
                        .map(|l| l.len() + 1) // +1 for newline
                        .sum::<usize>();
                    let line_end = line_start + line.len();
                    let in_match = match_ranges
                        .iter()
                        .any(|&(ms, me)| line_start < me && line_end > ms);
                    if !in_match {
                        *total_matches += 1;
                        if !count_only && results.len() < top_k {
                            let ln = (line_idx + 1) as u32;
                            let fp = candidate.full_path.to_string_lossy().to_string();
                            let (ctx_before, ctx_after) = get_context_lines(&fp, ln, context_lines);
                            results.push(SearchResultEntry {
                                node_id: format!("file::{}", candidate.rel_path),
                                label: candidate.rel_path.clone(),
                                node_type: "FileContent".into(),
                                score: None,
                                file_path: fp,
                                line_number: ln,
                                matched_line: line.to_string(),
                                context_before: ctx_before,
                                context_after: ctx_after,
                                graph_linked: candidate.graph_linked,
                                heuristic_signals: None,
                            });
                        }
                    }
                }
            } else {
                // Normal multiline: find all matches, report each
                for mat in re.find_iter(&content) {
                    *total_matches += 1;
                    if !count_only && results.len() < top_k {
                        // Calculate start line number
                        let start_byte = mat.start();
                        let line_number =
                            content[..start_byte].chars().filter(|&c| c == '\n').count() as u32 + 1;
                        let matched_text = mat.as_str().to_string();
                        // Truncate very long multiline matches to 500 chars
                        let display_text = if matched_text.len() > 500 {
                            format!("{}...[truncated]", &matched_text[..500])
                        } else {
                            matched_text
                        };
                        let fp = candidate.full_path.to_string_lossy().to_string();
                        let (ctx_before, ctx_after) =
                            get_context_lines(&fp, line_number, context_lines);
                        results.push(SearchResultEntry {
                            node_id: format!("file::{}", candidate.rel_path),
                            label: candidate.rel_path.clone(),
                            node_type: "FileContent".into(),
                            score: None,
                            file_path: fp,
                            line_number,
                            matched_line: display_text,
                            context_before: ctx_before,
                            context_after: ctx_after,
                            graph_linked: candidate.graph_linked,
                            heuristic_signals: None,
                        });
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers for file collection and path resolution
// ---------------------------------------------------------------------------

/// Collect search candidates from graph-linked files first, then append
/// controlled disk fallback files that are still outside the current graph coverage.
fn collect_search_files(
    state: &SessionState,
    graph: &m1nd_core::graph::Graph,
    scope: Option<&str>,
    filename_glob: Option<&glob::Pattern>,
) -> Vec<SearchFileCandidate> {
    let mut candidates = collect_graph_files(state, graph, scope, filename_glob);
    let mut seen: HashSet<PathBuf> = candidates.iter().map(|c| c.full_path.clone()).collect();

    for candidate in collect_disk_fallback_files(state, scope, filename_glob, &seen) {
        seen.insert(candidate.full_path.clone());
        candidates.push(candidate);
    }

    candidates
}

/// Collect unique file-level nodes from the graph, filtered by scope and filename pattern.
fn collect_graph_files(
    state: &SessionState,
    graph: &m1nd_core::graph::Graph,
    scope: Option<&str>,
    filename_glob: Option<&glob::Pattern>,
) -> Vec<SearchFileCandidate> {
    let mut seen_files: Vec<SearchFileCandidate> = Vec::new();
    let mut seen_set: HashSet<String> = HashSet::new();

    for (interned, &_nid) in graph.id_to_node.iter() {
        let ext_id = graph.strings.resolve(*interned);
        if ext_id.starts_with("file::") {
            let path = ext_id.strip_prefix("file::").unwrap_or(ext_id);
            // Only take file-level nodes (no ::fn:: or ::class:: sub-nodes)
            if !path.contains("::") && seen_set.insert(path.to_string()) {
                // Apply scope filter
                if !scope_matches_path(path, scope, &state.ingest_roots) {
                    continue;
                }
                // Apply filename_pattern filter
                if let Some(glob_pat) = filename_glob {
                    let filename = std::path::Path::new(path)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(path);
                    if !glob_pat.matches(filename) {
                        continue;
                    }
                }
                seen_files.push(SearchFileCandidate {
                    rel_path: path.to_string(),
                    full_path: resolve_full_path(state, path),
                    graph_linked: true,
                });
            }
        }
    }

    seen_files
}

fn collect_disk_fallback_files(
    state: &SessionState,
    scope: Option<&str>,
    filename_glob: Option<&glob::Pattern>,
    seen: &HashSet<PathBuf>,
) -> Vec<SearchFileCandidate> {
    let mut files = Vec::new();
    let roots = candidate_search_roots(state, scope);

    for root in roots {
        collect_disk_fallback_files_recursive(
            state,
            &root,
            &root,
            scope,
            filename_glob,
            seen,
            &mut files,
        );
    }

    files
}

fn candidate_search_roots(state: &SessionState, scope: Option<&str>) -> Vec<PathBuf> {
    if let Some(scope_value) = scope {
        let scope_path = Path::new(scope_value);
        if scope_path.is_absolute() {
            return vec![scope_path.to_path_buf()];
        }

        let mut roots = Vec::new();
        for root in &state.ingest_roots {
            let candidate = Path::new(root).join(scope_value);
            if candidate.exists() {
                roots.push(candidate);
            }
        }

        if roots.is_empty() {
            roots.extend(state.ingest_roots.iter().map(PathBuf::from));
        }

        return roots;
    }

    state.ingest_roots.iter().map(PathBuf::from).collect()
}

fn collect_disk_fallback_files_recursive(
    state: &SessionState,
    root: &Path,
    current: &Path,
    scope: Option<&str>,
    filename_glob: Option<&glob::Pattern>,
    seen: &HashSet<PathBuf>,
    files: &mut Vec<SearchFileCandidate>,
) {
    let entries = match std::fs::read_dir(current) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            if should_skip_disk_dir(&path) {
                continue;
            }
            collect_disk_fallback_files_recursive(
                state,
                root,
                &path,
                scope,
                filename_glob,
                seen,
                files,
            );
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let Some(candidate) =
            build_disk_fallback_candidate(state, root, &path, scope, filename_glob)
        else {
            continue;
        };

        if seen.contains(&candidate.full_path) {
            continue;
        }

        files.push(candidate);
    }
}

fn build_disk_fallback_candidate(
    state: &SessionState,
    root: &Path,
    full_path: &Path,
    scope: Option<&str>,
    filename_glob: Option<&glob::Pattern>,
) -> Option<SearchFileCandidate> {
    if !path_within_roots(state, full_path) {
        return None;
    }

    let rel_path = full_path
        .strip_prefix(root)
        .ok()
        .and_then(|p| relativize_against_ingest_roots_slice(&state.ingest_roots, &root.join(p)))
        .or_else(|| relativize_against_ingest_roots_slice(&state.ingest_roots, full_path))
        .unwrap_or_else(|| full_path.to_string_lossy().to_string());

    if !scope_matches_path(&rel_path, scope, &state.ingest_roots)
        && !scope_matches_path(&full_path.to_string_lossy(), scope, &state.ingest_roots)
    {
        return None;
    }

    if let Some(glob_pat) = filename_glob {
        let filename = full_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(&rel_path);
        if !glob_pat.matches(filename) {
            return None;
        }
    }

    Some(SearchFileCandidate {
        rel_path,
        full_path: full_path.to_path_buf(),
        graph_linked: false,
    })
}

fn relativize_against_ingest_roots_slice(
    ingest_roots: &[String],
    full_path: &Path,
) -> Option<String> {
    for root in ingest_roots {
        let root_path = Path::new(root);
        if let Ok(rel) = full_path.strip_prefix(root_path) {
            return Some(rel.to_string_lossy().to_string());
        }
    }

    None
}

fn relativize_against_ingest_roots(state: &SessionState, full_path: &Path) -> Option<String> {
    relativize_against_ingest_roots_slice(&state.ingest_roots, full_path)
}

fn path_within_roots(state: &SessionState, full_path: &Path) -> bool {
    if state.ingest_roots.is_empty() {
        return true;
    }

    state
        .ingest_roots
        .iter()
        .map(Path::new)
        .any(|root| full_path.starts_with(root))
}

fn should_skip_disk_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git")
            | Some(".hg")
            | Some(".svn")
            | Some("node_modules")
            | Some("target")
            | Some("dist")
            | Some("build")
            | Some(".vault")
            | Some(".roomanizer")
    )
}

fn rank_search_results(query: &str, mode: SearchRankingMode, results: &mut Vec<SearchResultEntry>) {
    let mut seen: HashSet<(String, u32, String)> = HashSet::new();
    results.retain(|entry| {
        seen.insert((
            entry.file_path.clone(),
            entry.line_number,
            entry.matched_line.clone(),
        ))
    });

    let query_lower = query.to_lowercase();
    results.sort_by(|a, b| {
        let a_score = result_rank(a, &query_lower, mode);
        let b_score = result_rank(b, &query_lower, mode);
        b_score
            .cmp(&a_score)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.line_number.cmp(&b.line_number))
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
}

fn result_rank(entry: &SearchResultEntry, query_lower: &str, mode: SearchRankingMode) -> i32 {
    let matched_line = entry.matched_line.to_lowercase();
    let label = entry.label.to_lowercase();
    let file_path = entry.file_path.to_lowercase();
    let node_id = entry.node_id.to_lowercase();

    let mut score = 0;

    if entry.node_type == "FileContent" {
        score += 200;
    }
    if entry.graph_linked {
        score += 30;
    }
    if exact_token_match(&matched_line, query_lower) {
        score += 180;
    } else if matched_line.contains(query_lower) {
        score += 120;
    }

    if exact_token_match(&label, query_lower) || exact_token_match(&file_path, query_lower) {
        score += 90;
    } else if label.contains(query_lower) || file_path.contains(query_lower) {
        score += 45;
    }

    if entry.line_number > 1 {
        score += 25;
    }

    if is_plain_file_node(&node_id) {
        score += 220;
        if label.contains(query_lower) || file_path.contains(query_lower) {
            score += 180;
        }
    } else if is_symbol_subnode(&node_id) {
        score -= 220;
    }

    if matches!(mode, SearchRankingMode::Regex) {
        score -= 20;
    }
    if matches!(mode, SearchRankingMode::Semantic) {
        score += 10;
    }
    if matches!(mode, SearchRankingMode::Literal) {
        score -= fixture_noise_penalty(entry, query_lower);
    }

    score
}

fn fixture_noise_penalty(entry: &SearchResultEntry, query_lower: &str) -> i32 {
    let file_path = entry.file_path.to_lowercase();
    let matched_line = entry.matched_line.to_lowercase();

    let fixture_like_path = [
        "/tests/",
        "/test/",
        "/fixtures/",
        "/fixture/",
        "/mocks/",
        "/mock/",
        "/examples/",
        "/docs/",
        "/samples/",
    ]
    .iter()
    .any(|needle| file_path.contains(needle))
        || file_path.ends_with("_test.rs")
        || file_path.ends_with("_test.py")
        || file_path.contains("fixture")
        || file_path.contains("mock");

    if !fixture_like_path {
        return 0;
    }

    let hardcoded_identity_like = matched_line.contains("file::")
        || matched_line.contains("node_")
        || matched_line.contains("/src/")
        || matched_line.contains("::fn::")
        || matched_line.contains("::class::");

    if hardcoded_identity_like && matched_line.contains(query_lower) {
        260
    } else {
        80
    }
}

fn exact_token_match(haystack: &str, needle: &str) -> bool {
    if haystack == needle {
        return true;
    }

    haystack
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|part| !part.is_empty() && part == needle)
}

fn is_plain_file_node(node_id: &str) -> bool {
    node_id.starts_with("file::") && !node_id["file::".len()..].contains("::")
}

fn is_symbol_subnode(node_id: &str) -> bool {
    node_id.starts_with("file::") && node_id["file::".len()..].contains("::")
}

/// Resolve a relative graph path to a full filesystem path using ingest roots.
fn resolve_full_path(state: &SessionState, rel_path: &str) -> std::path::PathBuf {
    let path = Path::new(rel_path);
    if path.is_absolute() {
        return path.to_path_buf();
    }

    for root in &state.ingest_roots {
        let candidate = canonicalize_path_hint(rel_path, std::slice::from_ref(root));
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(rel_path)
}

/// Extract file path and line number from a node's external_id / provenance.
fn extract_provenance(graph: &m1nd_core::graph::Graph, ext_id: &str) -> (String, u32) {
    // External IDs are typically like "file::path/to/file.py" or "func::path::name"
    let default_path = if ext_id.starts_with("file::") {
        ext_id.strip_prefix("file::").unwrap_or(ext_id).to_string()
    } else if let Some(pos) = ext_id.find("::") {
        ext_id[pos + 2..].to_string()
    } else {
        ext_id.to_string()
    };

    // Try to get provenance from graph
    if let Some(interned) = graph.strings.lookup(ext_id) {
        if let Some(&nid) = graph.id_to_node.get(&interned) {
            let resolved = graph.resolve_node_provenance(nid);
            let path = resolved.source_path.unwrap_or(default_path.clone());
            let line = resolved.line_start.unwrap_or(1);
            if line > 0 {
                return (path, line);
            }
        }
    }

    (default_path, 1)
}

/// Get context lines around a match from the filesystem.
fn get_context_lines(
    file_path: &str,
    line_number: u32,
    context_lines: u32,
) -> (Vec<String>, Vec<String>) {
    if context_lines == 0 || line_number == 0 {
        return (vec![], vec![]);
    }

    // Try to read the file
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return (vec![], vec![]),
    };

    let lines: Vec<&str> = content.lines().collect();
    let line_idx = (line_number as usize).saturating_sub(1);

    let before_start = line_idx.saturating_sub(context_lines as usize);
    let before: Vec<String> = lines[before_start..line_idx]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let after_end = (line_idx + 1 + context_lines as usize).min(lines.len());
    let after: Vec<String> = if line_idx + 1 < lines.len() {
        lines[line_idx + 1..after_end]
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![]
    };

    (before, after)
}

/// Guess node type from external_id prefix.
fn guess_node_type(ext_id: &str) -> String {
    if ext_id.starts_with("file::") {
        "File".into()
    } else if ext_id.starts_with("func::") || ext_id.starts_with("function::") {
        "Function".into()
    } else if ext_id.starts_with("class::") {
        "Class".into()
    } else if ext_id.starts_with("module::") {
        "Module".into()
    } else {
        "File".into()
    }
}

// ---------------------------------------------------------------------------
// m1nd.glob — Graph-Aware File Glob
// ---------------------------------------------------------------------------

pub fn handle_glob(state: &mut SessionState, input: GlobInput) -> M1ndResult<GlobOutput> {
    let start = Instant::now();

    if input.pattern.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "m1nd_glob".into(),
            detail: "pattern cannot be empty".into(),
        });
    }

    let glob_pattern =
        glob::Pattern::new(&input.pattern).map_err(|e| M1ndError::InvalidParams {
            tool: "m1nd_glob".into(),
            detail: format!("invalid glob pattern '{}': {}", input.pattern, e),
        })?;

    let top_k = (input.top_k as usize).clamp(1, 10_000);
    let scope = normalize_scope_hint(input.scope.as_deref(), &state.ingest_roots);
    let scope = scope.as_deref();
    let scope_applied = scope.is_some();

    let graph = state.graph.read();

    let mut files: Vec<GlobFileEntry> = Vec::new();
    let mut total_matches: usize = 0;

    // Iterate all file:: nodes in the graph
    for (interned, &nid) in graph.id_to_node.iter() {
        let ext_id = graph.strings.resolve(*interned);
        if !ext_id.starts_with("file::") {
            continue;
        }
        let path = ext_id.strip_prefix("file::").unwrap_or(ext_id);
        // Only file-level nodes (no ::fn:: sub-nodes)
        if path.contains("::") {
            continue;
        }

        // Scope filter
        if !scope_matches_path(path, scope, &state.ingest_roots) {
            continue;
        }

        // Glob match against relative path
        if !glob_pattern.matches(path) {
            continue;
        }

        total_matches += 1;

        if files.len() < top_k {
            // Extract metadata from graph
            let extension = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();

            // Check if node has outgoing edges (safe: only if CSR is finalized)
            let has_connections = if !graph.csr.offsets.is_empty() {
                let range = graph.csr.out_range(nid);
                !range.is_empty()
            } else {
                false
            };

            // Try to get line count from provenance metadata
            let line_count = {
                let prov = graph.resolve_node_provenance(nid);
                prov.line_end.unwrap_or(0)
            };

            files.push(GlobFileEntry {
                node_id: ext_id.to_string(),
                file_path: path.to_string(),
                extension,
                line_count,
                has_connections,
            });
        }
    }

    // Sort based on requested order
    match input.sort {
        crate::protocol::layers::GlobSort::Path => {
            files.sort_by(|a, b| a.file_path.cmp(&b.file_path));
        }
        crate::protocol::layers::GlobSort::Activation => {
            // Sort by connection count descending as a proxy for activation
            files.sort_by(|a, b| b.has_connections.cmp(&a.has_connections));
        }
    }

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    Ok(GlobOutput {
        pattern: input.pattern,
        files,
        total_matches,
        scope_applied,
        elapsed_ms: elapsed,
    })
}

// ---------------------------------------------------------------------------
// m1nd.help
// ---------------------------------------------------------------------------

pub fn handle_help(_state: &mut SessionState, input: HelpInput) -> M1ndResult<HelpOutput> {
    let tool_name = input.tool_name.as_deref();

    match tool_name {
        None => {
            // Full index
            let formatted = personality::format_help_index();
            Ok(HelpOutput {
                formatted,
                tool: None,
                found: true,
                suggestions: vec![],
            })
        }
        Some("about") => {
            let formatted = personality::format_about();
            Ok(HelpOutput {
                formatted,
                tool: Some("about".into()),
                found: true,
                suggestions: vec![],
            })
        }
        Some(name) => {
            // Normalize aliases like `m1nd_activate` and `m1nd.activate`
            // to the canonical raw tool name used in `tool_docs()`.
            let normalized = normalize_help_tool_name(name);

            let docs = personality::tool_docs();
            if let Some(doc) = docs.iter().find(|d| d.name == normalized) {
                let formatted = personality::format_tool_help(doc);
                Ok(HelpOutput {
                    formatted,
                    tool: Some(normalized),
                    found: true,
                    suggestions: vec![],
                })
            } else {
                // Unknown tool -- find similar (ADVERSARY H2)
                let suggestions = personality::find_similar_tools(name);
                let formatted = format!(
                    "{}tool '{}' not found.{}\n{}did you mean: {}?{}\n",
                    personality::ANSI_RED,
                    name,
                    personality::ANSI_RESET,
                    personality::ANSI_DIM,
                    suggestions.join(", "),
                    personality::ANSI_RESET,
                );
                Ok(HelpOutput {
                    formatted,
                    tool: Some(name.to_string()),
                    found: false,
                    suggestions,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonicalize_path_hint, handle_glob, handle_help, handle_search, normalize_help_tool_name,
        normalize_scope_hint, rank_search_results, scope_matches_path, SearchRankingMode,
    };
    use crate::protocol::layers::{
        GlobInput, GlobSort, SearchInput, SearchMode, SearchResultEntry,
    };
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::NodeType;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn build_state(root: &std::path::Path) -> SessionState {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };

        let mut state =
            SessionState::initialize(Graph::default(), &config, DomainConfig::code()).unwrap();
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());
        state
    }

    fn add_file_node(state: &mut SessionState, rel_path: &str) {
        let mut graph = state.graph.write();
        graph
            .add_node(
                &format!("file::{}", rel_path),
                rel_path,
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add file node");
    }

    fn assert_search_equivalence(state: &mut SessionState, scopes: &[String], expected_path: &str) {
        let mut baseline: Option<(usize, usize, String, String)> = None;

        for scope in scopes {
            let input = SearchInput {
                agent_id: "jimi-codex".into(),
                query: "MAGIC_TOKEN".into(),
                mode: SearchMode::Literal,
                scope: Some(scope.clone()),
                top_k: 10,
                case_sensitive: true,
                context_lines: 0,
                invert: false,
                count_only: false,
                multiline: false,
                auto_ingest: false,
                filename_pattern: Some("*.rs".into()),
            };

            let output = handle_search(state, input).expect("search output");
            assert_eq!(
                output.total_matches, 1,
                "scope {scope} should find one match"
            );
            assert_eq!(
                output.results.len(),
                1,
                "scope {scope} should return one result"
            );
            assert_eq!(output.results[0].file_path, expected_path);
            assert_eq!(output.results[0].label, "src/example.rs");
            let sig = (
                output.total_matches,
                output.results.len(),
                output.results[0].file_path.clone(),
                output.results[0].matched_line.clone(),
            );
            if let Some(prev) = &baseline {
                assert_eq!(&sig, prev, "scope {scope} should be equivalent");
            } else {
                baseline = Some(sig);
            }
        }
    }

    fn assert_glob_equivalence(state: &mut SessionState, scopes: &[String]) {
        let mut baseline: Option<(usize, Vec<String>)> = None;

        for scope in scopes {
            let input = GlobInput {
                agent_id: "jimi-codex".into(),
                pattern: "**/*.rs".into(),
                sort: GlobSort::Path,
                scope: Some(scope.clone()),
                top_k: 10,
            };

            let output = handle_glob(state, input).expect("glob output");
            assert_eq!(
                output.total_matches, 1,
                "scope {scope} should find one match"
            );
            assert_eq!(
                output.files.len(),
                1,
                "scope {scope} should return one file"
            );
            assert_eq!(output.files[0].file_path, "src/example.rs");
            let sig = (
                output.total_matches,
                output
                    .files
                    .iter()
                    .map(|f| f.file_path.clone())
                    .collect::<Vec<_>>(),
            );
            if let Some(prev) = &baseline {
                assert_eq!(&sig, prev, "scope {scope} should be equivalent");
            } else {
                baseline = Some(sig);
            }
        }
    }

    #[test]
    fn search_falls_back_to_disk_for_files_missing_from_graph() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir");
        let file_path = src_dir.join("fallback.rs");
        std::fs::write(
            &file_path,
            "pub const FALLBACK_TOKEN: &str = \"disk-search-still-works\";\n",
        )
        .expect("write file");

        let mut state = build_state(&root);
        let input = SearchInput {
            agent_id: "jimi-codex".into(),
            query: "FALLBACK_TOKEN".into(),
            mode: SearchMode::Literal,
            scope: Some("src".into()),
            top_k: 10,
            case_sensitive: false,
            context_lines: 0,
            invert: false,
            count_only: false,
            multiline: false,
            auto_ingest: false,
            filename_pattern: Some("*.rs".into()),
        };

        let output = handle_search(&mut state, input).expect("search output");
        assert_eq!(output.total_matches, 1);
        assert_eq!(output.results.len(), 1);
        assert_eq!(
            output.results[0].matched_line,
            "pub const FALLBACK_TOKEN: &str = \"disk-search-still-works\";"
        );
        assert_eq!(output.results[0].file_path, file_path.to_string_lossy());
        assert!(!output.results[0].graph_linked);
    }

    #[test]
    fn search_auto_ingests_absolute_scope_outside_existing_roots() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("workspace");
        std::fs::create_dir_all(&root).expect("workspace dir");
        let external = root.join("external-site");
        let outside = temp.path().join("outside-site");
        let src_dir = outside.join("src");
        std::fs::create_dir_all(&src_dir).expect("external src dir");
        std::fs::write(
            src_dir.join("site.rs"),
            "pub const MADE_IN_ITALY: &str = \"madeinitalycars.com\";\n",
        )
        .expect("write file");

        let mut state = build_state(&root);
        let input = SearchInput {
            agent_id: "jimi-codex".into(),
            query: "madeinitalycars.com".into(),
            mode: SearchMode::Literal,
            scope: Some(outside.to_string_lossy().to_string()),
            top_k: 10,
            case_sensitive: false,
            context_lines: 0,
            invert: false,
            count_only: false,
            multiline: false,
            auto_ingest: true,
            filename_pattern: Some("*.rs".into()),
        };

        let output = handle_search(&mut state, input).expect("search output");
        assert!(output.auto_ingested);
        assert_eq!(
            output.auto_ingested_paths,
            vec![outside.to_string_lossy().to_string()]
        );
        assert_eq!(output.total_matches, 1);
        assert_eq!(output.results.len(), 1);
        assert!(state
            .ingest_roots
            .contains(&outside.to_string_lossy().to_string()));
    }

    #[test]
    fn search_auto_ingests_relative_scope_resolved_from_workspace_root() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("project");
        let workspace = temp.path();
        let external = workspace.join("site-out");
        let src_dir = external.join("src");
        std::fs::create_dir_all(&src_dir).expect("external src dir");
        std::fs::write(
            src_dir.join("site.rs"),
            "pub const AUTO_SCOPE_REL: &str = \"relative-auto\";\n",
        )
        .expect("write file");

        let mut state = build_state(&root);
        state.workspace_root = Some(workspace.to_string_lossy().to_string());

        let input = SearchInput {
            agent_id: "jimi-codex".into(),
            query: "relative-auto".into(),
            mode: SearchMode::Literal,
            scope: Some("site-out/src".into()),
            top_k: 10,
            case_sensitive: false,
            context_lines: 0,
            invert: false,
            count_only: false,
            multiline: false,
            auto_ingest: true,
            filename_pattern: Some("*.rs".into()),
        };

        let output = handle_search(&mut state, input).expect("search output");
        assert!(output.auto_ingested);
        assert_eq!(
            output.auto_ingested_paths,
            vec![external.to_string_lossy().to_string()]
        );
        assert_eq!(output.total_matches, 1);
        assert_eq!(output.results.len(), 1);
        assert!(state
            .ingest_roots
            .contains(&external.to_string_lossy().to_string()));
    }

    #[test]
    fn search_auto_ingest_fails_for_ambiguous_relative_scope_resolution() {
        let temp = tempdir().expect("tempdir");
        let root_a = temp.path().join("project-a");
        let root_b = temp.path().join("project-b");
        let workspace = temp.path();
        let shared = "overlap/src";

        for root in [&root_a, &root_b] {
            let src_dir = root.join("overlap").join("src");
            std::fs::create_dir_all(&src_dir).expect("overlap src dir");
            std::fs::write(
                src_dir.join("site.rs"),
                "pub const AMBIG_SCOPE: &str = \"ambiguous\";\n",
            )
            .expect("write file");
        }

        let mut state = build_state(&root_a);
        state.ingest_roots = vec![
            root_a.to_string_lossy().to_string(),
            root_b.to_string_lossy().to_string(),
        ];
        state.workspace_root = Some(workspace.to_string_lossy().to_string());

        let input = SearchInput {
            agent_id: "jimi-codex".into(),
            query: "ambiguous".into(),
            mode: SearchMode::Literal,
            scope: Some(shared.into()),
            top_k: 10,
            case_sensitive: false,
            context_lines: 0,
            invert: false,
            count_only: false,
            multiline: false,
            auto_ingest: true,
            filename_pattern: Some("*.rs".into()),
        };

        let err = handle_search(&mut state, input).unwrap_err();
        let err_msg = err.to_string();
        assert!(err_msg.contains("scope 'overlap/src' resolves to 2 candidate paths"));
        assert!(err_msg.contains("project-a"));
        assert!(err_msg.contains("project-b"));
    }

    #[test]
    fn help_tool_name_normalization_accepts_common_aliases() {
        assert_eq!(normalize_help_tool_name("activate"), "activate");
        assert_eq!(normalize_help_tool_name("m1nd_activate"), "activate");
        assert_eq!(normalize_help_tool_name("m1nd.activate"), "activate");
        assert_eq!(normalize_help_tool_name("  m1nd.activate  "), "activate");
    }

    #[test]
    fn canonicalize_path_hint_resolves_absolute_and_relative_inputs() {
        let roots = vec!["/workspace".to_string()];
        assert_eq!(
            canonicalize_path_hint("/abs/path/file.rs", &roots),
            PathBuf::from("/abs/path/file.rs")
        );
        assert_eq!(
            canonicalize_path_hint("src/main.rs", &roots),
            PathBuf::from("/workspace/src/main.rs")
        );
    }

    #[test]
    fn help_handler_resolves_aliases_to_canonical_tool_names() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_state(&root);

        let input = crate::protocol::layers::HelpInput {
            agent_id: "jimi-codex".into(),
            tool_name: Some("m1nd.activate".into()),
        };

        let output = handle_help(&mut state, input).expect("help output");
        assert!(output.found);
        assert_eq!(output.tool.as_deref(), Some("activate"));
        assert!(output.formatted.contains("PARAMS"));
    }

    #[test]
    fn help_handler_surfaces_decision_sections_for_known_tools() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_state(&root);

        let input = crate::protocol::layers::HelpInput {
            agent_id: "jimi-codex".into(),
            tool_name: Some("surgical_context_v2".into()),
        };

        let output = handle_help(&mut state, input).expect("help output");
        assert!(output.found);
        assert!(output.formatted.contains("WHEN TO USE"));
        assert!(output.formatted.contains("AVOID WHEN"));
        assert!(output.formatted.contains("AGENT NOTES"));
        assert!(output.formatted.contains("proof_focused=true"));
    }

    #[test]
    fn help_index_includes_short_decision_guide() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let mut state = build_state(&root);

        let input = crate::protocol::layers::HelpInput {
            agent_id: "jimi-codex".into(),
            tool_name: None,
        };

        let output = handle_help(&mut state, input).expect("help output");
        assert!(output.found);
        assert!(output.formatted.contains("decision guide"));
        assert!(output.formatted.contains("search=text"));
        assert!(output.formatted.contains("seek=intent"));
    }

    #[test]
    fn ranking_prefers_file_content_over_symbol_noise_for_literal_search() {
        let mut results = vec![
            SearchResultEntry {
                node_id: "file::m1nd/m1nd-mcp/src/session.rs::fn::persist_boot_memory".into(),
                label: "file::m1nd/m1nd-mcp/src/session.rs::fn::persist_boot_memory".into(),
                node_type: "File".into(),
                score: None,
                file_path: "m1nd/m1nd-mcp/src/session.rs::fn::persist_boot_memory".into(),
                line_number: 1,
                matched_line: "file::m1nd/m1nd-mcp/src/session.rs::fn::persist_boot_memory".into(),
                context_before: vec![],
                context_after: vec![],
                graph_linked: true,
                heuristic_signals: None,
            },
            SearchResultEntry {
                node_id: "file::m1nd/m1nd-mcp/src/boot_memory_handlers.rs".into(),
                label: "m1nd/m1nd-mcp/src/boot_memory_handlers.rs".into(),
                node_type: "FileContent".into(),
                score: None,
                file_path: "/abs/m1nd/m1nd-mcp/src/boot_memory_handlers.rs".into(),
                line_number: 12,
                matched_line: "pub struct BootMemoryInput {".into(),
                context_before: vec![],
                context_after: vec![],
                graph_linked: true,
                heuristic_signals: None,
            },
        ];

        rank_search_results("boot_memory", SearchRankingMode::Literal, &mut results);

        assert_eq!(
            results[0].node_id,
            "file::m1nd/m1nd-mcp/src/boot_memory_handlers.rs"
        );
    }

    #[test]
    fn scope_normalization_equates_relative_absolute_and_file_forms() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let absolute = root.join("src");
        let scopes = [
            "src".to_string(),
            absolute.to_string_lossy().to_string(),
            format!("file::src"),
            format!("file::{}", absolute.to_string_lossy()),
        ];

        for scope in scopes {
            let normalized =
                normalize_scope_hint(Some(&scope), &[root.to_string_lossy().to_string()]);
            assert_eq!(normalized.as_deref(), Some("src"));
        }
    }

    #[test]
    fn scope_matches_path_uses_prefix_semantics_after_normalization() {
        let roots = vec!["/workspace".to_string()];

        assert!(scope_matches_path(
            "file::src/main.rs::fn::boot",
            Some("src"),
            &roots
        ));
        assert!(scope_matches_path(
            "/workspace/src/main.rs",
            Some("src"),
            &roots
        ));
        assert!(!scope_matches_path(
            "file::docs/src-notes.md",
            Some("src"),
            &roots
        ));
    }

    #[test]
    fn search_scope_forms_are_equivalent() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir");
        let file_path = src_dir.join("example.rs");
        std::fs::write(
            &file_path,
            "pub const MAGIC_TOKEN: &str = \"equivalent\";\n",
        )
        .expect("write file");

        let mut state = build_state(&root);
        add_file_node(&mut state, "src/example.rs");

        let scopes = vec![
            "src".to_string(),
            src_dir.to_string_lossy().to_string(),
            "file::src".to_string(),
            format!("file::{}", src_dir.to_string_lossy()),
        ];

        assert_search_equivalence(&mut state, &scopes, &file_path.to_string_lossy());
    }

    #[test]
    fn glob_scope_forms_are_equivalent() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir");

        let mut state = build_state(&root);
        add_file_node(&mut state, "src/example.rs");
        add_file_node(&mut state, "src/other.txt");
        add_file_node(&mut state, "docs/ignore.rs");

        let scopes = vec![
            "src".to_string(),
            src_dir.to_string_lossy().to_string(),
            "file::src".to_string(),
            format!("file::{}", src_dir.to_string_lossy()),
        ];

        assert_glob_equivalence(&mut state, &scopes);
    }

    #[test]
    fn ranking_prefers_plain_file_node_over_symbol_subnode_for_literal_search() {
        let mut results = vec![
            SearchResultEntry {
                node_id: "file::m1nd/m1nd-mcp/src/session.rs::fn::load_boot_memory".into(),
                label: "file::m1nd/m1nd-mcp/src/session.rs::fn::load_boot_memory".into(),
                node_type: "File".into(),
                score: None,
                file_path: "m1nd/m1nd-mcp/src/session.rs::fn::load_boot_memory".into(),
                line_number: 1,
                matched_line: "file::m1nd/m1nd-mcp/src/session.rs::fn::load_boot_memory".into(),
                context_before: vec![],
                context_after: vec![],
                graph_linked: true,
                heuristic_signals: None,
            },
            SearchResultEntry {
                node_id: "file::m1nd/m1nd-mcp/src/boot_memory_handlers.rs".into(),
                label: "file::m1nd/m1nd-mcp/src/boot_memory_handlers.rs".into(),
                node_type: "File".into(),
                score: None,
                file_path: "m1nd/m1nd-mcp/src/boot_memory_handlers.rs".into(),
                line_number: 1,
                matched_line: "file::m1nd/m1nd-mcp/src/boot_memory_handlers.rs".into(),
                context_before: vec![],
                context_after: vec![],
                graph_linked: true,
                heuristic_signals: None,
            },
        ];

        rank_search_results("boot_memory", SearchRankingMode::Literal, &mut results);

        assert_eq!(
            results[0].node_id,
            "file::m1nd/m1nd-mcp/src/boot_memory_handlers.rs"
        );
    }

    #[test]
    fn ranking_demotes_fixture_like_literal_identity_noise() {
        let mut results = vec![
            SearchResultEntry {
                node_id: "file::tests/fixtures/continuity_fixture.rs".into(),
                label: "tests/fixtures/continuity_fixture.rs".into(),
                node_type: "FileContent".into(),
                score: None,
                file_path: "/abs/tests/fixtures/continuity_fixture.rs".into(),
                line_number: 18,
                matched_line:
                    "let saved = \"file::m1nd/m1nd-mcp/src/session.rs::fn::persist_boot_memory\";"
                        .into(),
                context_before: vec![],
                context_after: vec![],
                graph_linked: false,
                heuristic_signals: None,
            },
            SearchResultEntry {
                node_id: "file::m1nd/m1nd-mcp/src/session.rs".into(),
                label: "session.rs".into(),
                node_type: "FileContent".into(),
                score: None,
                file_path: "/abs/m1nd/m1nd-mcp/src/session.rs".into(),
                line_number: 42,
                matched_line: "pub fn persist_boot_memory(state: &SessionState) {".into(),
                context_before: vec![],
                context_after: vec![],
                graph_linked: true,
                heuristic_signals: None,
            },
        ];

        rank_search_results(
            "persist_boot_memory",
            SearchRankingMode::Literal,
            &mut results,
        );

        assert_eq!(results[0].node_id, "file::m1nd/m1nd-mcp/src/session.rs");
    }
}
