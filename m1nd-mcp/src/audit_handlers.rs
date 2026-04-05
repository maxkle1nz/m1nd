use crate::protocol::{self, layers};
use crate::report_handlers;
use crate::scope::normalize_scope_path;
use crate::session::{FileInventoryEntry, SessionState};
use crate::{layer_handlers, tools};
use m1nd_core::error::{M1ndError, M1ndResult};
use regex::Regex;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const DEFAULT_SCAN_PATTERNS: &[&str] = &[
    "error_handling",
    "resource_cleanup",
    "api_surface",
    "state_mutation",
    "concurrency",
    "auth_boundary",
    "test_coverage",
    "dependency_injection",
];

const COORDINATION_SCAN_PATTERNS: &[&str] = &["test_coverage", "api_surface"];
const SECURITY_SCAN_PATTERNS: &[&str] = &[
    "auth_boundary",
    "api_surface",
    "state_mutation",
    "dependency_injection",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuditFileKind {
    Code,
    Script,
    Config,
    Doc,
    BenchmarkArtifact,
    Generated,
    Asset,
    Unknown,
}

fn truncate_text(
    content: String,
    max_output_chars: Option<usize>,
    label: &str,
) -> (String, bool, Option<String>) {
    let Some(limit) = max_output_chars else {
        return (content, false, None);
    };
    if content.chars().count() <= limit {
        return (content, false, None);
    }
    let truncated: String = content.chars().take(limit).collect();
    let summary = format!(
        "{} output exceeded {} chars and was truncated inline. Refine scope/depth or raise max_output_chars for the full narrative.",
        label, limit
    );
    (truncated, true, Some(summary))
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

fn classify_file_kind(file_path: &str, language: &str) -> AuditFileKind {
    let lower = file_path.to_lowercase();
    let file_name = Path::new(file_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let ext = Path::new(file_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();

    if lower.contains("/target/")
        || lower.contains("/dist/")
        || lower.contains("/build/")
        || lower.contains("/.next/")
        || lower.contains("/docs/wiki-build/")
        || lower.contains("/wiki-build/")
    {
        return AuditFileKind::Generated;
    }

    if lower.contains("/docs/benchmarks/runs/")
        || lower.contains("/docs/benchmarks/events/")
        || lower.contains("/docs/benchmarks/scenarios/")
    {
        return AuditFileKind::BenchmarkArtifact;
    }

    if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "woff" | "woff2" | "ttf" | "ico"
    ) {
        return AuditFileKind::Asset;
    }

    if matches!(
        ext.as_str(),
        "rs" | "py" | "pyi" | "ts" | "tsx" | "js" | "jsx" | "go" | "java"
    ) || matches!(language, "rust" | "python" | "typescript" | "go" | "java")
    {
        return AuditFileKind::Code;
    }

    if matches!(ext.as_str(), "sh" | "bash" | "zsh")
        || file_name.ends_with(".plist")
        || file_name == "dockerfile"
    {
        return AuditFileKind::Script;
    }

    if matches!(
        file_name.as_str(),
        "cargo.toml" | "package.json" | "package-lock.json" | "pyproject.toml" | "deno.json"
    ) || matches!(
        ext.as_str(),
        "toml" | "yaml" | "yml" | "json" | "ini" | "cfg" | "conf"
    ) {
        return AuditFileKind::Config;
    }

    if matches!(ext.as_str(), "md" | "txt" | "rst" | "adoc") {
        return AuditFileKind::Doc;
    }

    AuditFileKind::Unknown
}

fn counts_for_orphan_detection(kind: AuditFileKind, profile: &str) -> bool {
    let _ = profile;
    matches!(kind, AuditFileKind::Code)
}

fn is_auxiliary_code_path(file_path: &str) -> bool {
    let lower = file_path.to_lowercase();
    lower.contains("/tests/")
        || lower.contains("/test_")
        || lower.contains("/fixtures/")
        || lower.contains("/mocks/")
        || lower.contains("/examples/")
        || lower.contains("/scripts/")
        || lower.contains("/bench")
        || lower.contains("/benchmark")
        || lower.contains("/m1nd-demo/")
        || lower.contains("/m1nd-ui/")
        || lower.contains("/m1nd-viz/")
}

fn is_placeholder_external_path(path: &Path) -> bool {
    let value = path.to_string_lossy();
    value.starts_with("/your/")
        || value == "/your/project"
        || value == "/your/docs"
        || value == "/your/domain.json"
        || value.starts_with("/path/")
        || value.starts_with("/path/to/")
        || value.starts_with("/app/")
        || value.starts_with("/project/")
        || value.starts_with("/workspace/")
}

fn is_system_path(path: &Path) -> bool {
    let value = path.to_string_lossy();
    value.starts_with("/usr/")
        || value.starts_with("/dev/")
        || value.starts_with("/bin/")
        || value.starts_with("/sbin/")
        || value.starts_with("/System/")
        || value.starts_with("/Library/")
        || value.starts_with("/opt/homebrew/")
}

fn is_plausible_external_path(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let value = path.to_string_lossy();
    if value == "/" || value == "//" || value.len() < 4 {
        return false;
    }
    if value.contains('[')
        || value.contains(']')
        || value.contains('{')
        || value.contains('}')
        || value.contains('\\')
    {
        return false;
    }
    let component_count = path.components().count();
    component_count >= 2 && value.chars().any(|ch| ch.is_ascii_alphanumeric())
}

fn counts_for_grading(kind: AuditFileKind, profile: &str) -> bool {
    match profile {
        "coordination" => matches!(
            kind,
            AuditFileKind::Code
                | AuditFileKind::Config
                | AuditFileKind::Script
                | AuditFileKind::Doc
        ),
        _ => matches!(
            kind,
            AuditFileKind::Code | AuditFileKind::Config | AuditFileKind::Script
        ),
    }
}

fn supports_external_reference_scan(kind: AuditFileKind) -> bool {
    matches!(
        kind,
        AuditFileKind::Code | AuditFileKind::Config | AuditFileKind::Script | AuditFileKind::Doc
    )
}

fn simple_content_hash(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

fn loc_map_from_graph(state: &SessionState) -> HashMap<String, u32> {
    let graph = state.graph.read();
    let mut loc_by_external_id = HashMap::new();
    for (interned, &nid) in &graph.id_to_node {
        let ext_id = graph.strings.resolve(*interned).to_string();
        if !ext_id.starts_with("file::") {
            continue;
        }
        let prov = graph.resolve_node_provenance(nid);
        if let Some(loc) = prov
            .line_end
            .zip(prov.line_start)
            .map(|(end, start)| end.saturating_sub(start).saturating_add(1))
            .filter(|loc| *loc > 0)
        {
            loc_by_external_id
                .entry(ext_id)
                .and_modify(|current: &mut u32| *current = (*current).max(loc))
                .or_insert(loc);
        }
    }
    loc_by_external_id
}

fn inventory_from_roots(
    state: &SessionState,
    include_dotfiles: bool,
    dotfile_patterns: &[String],
) -> HashMap<String, FileInventoryEntry> {
    let mut inventory = HashMap::new();
    let loc_by_external_id = loc_map_from_graph(state);

    for root in &state.ingest_roots {
        let root_path = PathBuf::from(root);
        if !root_path.exists() {
            continue;
        }
        if root_path.is_file() {
            let extension = root_path.extension().and_then(|ext| ext.to_str());
            let metadata = match std::fs::metadata(&root_path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let external_id = root_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("file::{}", name))
                .unwrap_or_else(|| format!("file::{}", root_path.to_string_lossy()));
            inventory.insert(
                external_id.clone(),
                FileInventoryEntry {
                    external_id: external_id.clone(),
                    file_path: root_path.to_string_lossy().to_string(),
                    size_bytes: metadata.len(),
                    last_modified_ms: metadata
                        .modified()
                        .ok()
                        .and_then(|ts| ts.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                    language: extension_language(extension),
                    commit_count: 0,
                    loc: loc_by_external_id.get(&external_id).copied(),
                    sha256: simple_content_hash(&root_path),
                },
            );
            continue;
        }

        let config = m1nd_ingest::IngestConfig {
            root: root_path.clone(),
            include_dotfiles,
            dotfile_patterns: dotfile_patterns.to_vec(),
            ..m1nd_ingest::IngestConfig::default()
        };
        let walker = m1nd_ingest::walker::DirectoryWalker::new(
            config.skip_dirs.clone(),
            config.skip_files.clone(),
            config.include_dotfiles,
            config.dotfile_patterns.clone(),
        );
        let Ok(walk) = walker.walk(&root_path) else {
            continue;
        };
        for file in walk.files {
            let external_id = format!("file::{}", file.relative_path);
            inventory.insert(
                external_id.clone(),
                FileInventoryEntry {
                    external_id: external_id.clone(),
                    file_path: file.path.to_string_lossy().to_string(),
                    size_bytes: file.size_bytes,
                    last_modified_ms: (file.last_modified * 1000.0).round() as u64,
                    language: extension_language(file.extension.as_deref()),
                    commit_count: file.commit_count,
                    loc: loc_by_external_id.get(&external_id).copied(),
                    sha256: simple_content_hash(&file.path),
                },
            );
        }
    }

    inventory
}

fn filter_inventory_by_scope(
    state: &SessionState,
    inventory: &HashMap<String, FileInventoryEntry>,
    scope: Option<&str>,
) -> Vec<FileInventoryEntry> {
    let normalized_scope = normalize_scope_path(scope, &state.ingest_roots);
    let mut entries: Vec<FileInventoryEntry> = inventory
        .values()
        .filter(|entry| {
            if let Some(ref scope) = normalized_scope {
                entry
                    .external_id
                    .strip_prefix("file::")
                    .is_some_and(|path| path.starts_with(scope))
            } else {
                true
            }
        })
        .cloned()
        .collect();
    entries.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    entries
}

fn inventory_breakdown(entries: &[FileInventoryEntry]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in entries {
        let label = match classify_file_kind(&entry.file_path, &entry.language) {
            AuditFileKind::Code => "code",
            AuditFileKind::Script => "script",
            AuditFileKind::Config => "config",
            AuditFileKind::Doc => "doc",
            AuditFileKind::BenchmarkArtifact => "benchmark_artifact",
            AuditFileKind::Generated => "generated",
            AuditFileKind::Asset => "asset",
            AuditFileKind::Unknown => "unknown",
        };
        *counts.entry(label.to_string()).or_insert(0) += 1;
    }
    counts
}

pub fn resolve_git_root_from_state(state: &SessionState) -> Option<PathBuf> {
    let candidates = state
        .ingest_roots
        .iter()
        .rev()
        .map(PathBuf::from)
        .chain(state.workspace_root.clone().into_iter().map(PathBuf::from))
        .chain(state.graph_path.parent().map(Path::to_path_buf));

    for candidate in candidates {
        let path = if candidate.is_file() {
            candidate
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(candidate)
        } else {
            candidate
        };
        if let Ok(output) = Command::new("git")
            .current_dir(&path)
            .args(["rev-parse", "--show-toplevel"])
            .output()
        {
            if output.status.success() {
                let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !root.is_empty() {
                    return Some(PathBuf::from(root));
                }
            }
        }
    }

    None
}

pub fn collect_git_state(state: &SessionState, recent_commit_limit: usize) -> serde_json::Value {
    let Some(root) = resolve_git_root_from_state(state) else {
        return json!({
            "available": false,
            "branch": null,
            "clean": null,
            "head": null,
            "recent_commits": [],
            "uncommitted_files": [],
        });
    };

    let read_git = |args: &[&str]| -> Option<String> {
        let output = Command::new("git")
            .current_dir(&root)
            .args(args)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    };

    let branch = read_git(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let head = read_git(&["rev-parse", "HEAD"]);
    let status_porcelain = read_git(&["status", "--porcelain"]).unwrap_or_default();
    let uncommitted_files: Vec<String> = status_porcelain
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                None
            } else {
                Some(line[3..].trim().to_string())
            }
        })
        .collect();
    let recent_commits_raw = read_git(&[
        "log",
        &format!("-{}", recent_commit_limit.max(1)),
        "--pretty=format:%h %s",
    ])
    .unwrap_or_default();
    let recent_commits: Vec<String> = recent_commits_raw
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    json!({
        "available": true,
        "root": root.to_string_lossy(),
        "branch": branch,
        "clean": uncommitted_files.is_empty(),
        "head": head,
        "recent_commits": recent_commits,
        "recent_commit_count": recent_commits.len(),
        "uncommitted_files": uncommitted_files,
    })
}

pub fn handle_scan_all(
    state: &mut SessionState,
    input: layers::ScanAllInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();
    let patterns: Vec<String> = if input.patterns.is_empty() {
        DEFAULT_SCAN_PATTERNS
            .iter()
            .map(|value| value.to_string())
            .collect()
    } else {
        input.patterns.clone()
    };

    let mut total_findings = 0usize;
    let mut by_pattern = Vec::new();
    for pattern in patterns {
        let output = layer_handlers::handle_scan(
            state,
            layers::ScanInput {
                pattern: pattern.clone(),
                agent_id: input.agent_id.clone(),
                scope: input.scope.clone(),
                severity_min: input.severity_min,
                graph_validate: input.graph_validate,
                limit: input.limit_per_pattern,
            },
        )?;
        total_findings += output.findings.len();
        by_pattern.push(layers::ScanAllPatternOutput {
            pattern: output.pattern,
            findings: output.findings,
            files_scanned: output.files_scanned,
            total_matches_raw: output.total_matches_raw,
            total_matches_validated: output.total_matches_validated,
        });
    }

    serde_json::to_value(layers::ScanAllOutput {
        patterns_run: by_pattern.len(),
        total_findings,
        by_pattern,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
    .map_err(M1ndError::Serde)
}

pub fn handle_cross_verify(
    state: &mut SessionState,
    input: layers::CrossVerifyInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();
    let checks: BTreeSet<String> = if input.check.is_empty() {
        ["existence", "loc", "hash"]
            .into_iter()
            .map(|value| value.to_string())
            .collect()
    } else {
        input.check.iter().cloned().collect()
    };

    let stored_inventory = state.file_inventory.clone();
    let live_inventory =
        inventory_from_roots(state, input.include_dotfiles, &input.dotfile_patterns);
    let disk_entries = filter_inventory_by_scope(state, &live_inventory, input.scope.as_deref());
    let disk_map: HashMap<String, FileInventoryEntry> = disk_entries
        .iter()
        .cloned()
        .map(|entry| (entry.external_id.clone(), entry))
        .collect();

    let graph = state.graph.read();
    let normalized_scope = normalize_scope_path(input.scope.as_deref(), &state.ingest_roots);
    let mut graph_file_ids = BTreeSet::new();
    let mut graph_loc = HashMap::new();
    for (interned, &nid) in &graph.id_to_node {
        let ext_id = graph.strings.resolve(*interned).to_string();
        if !ext_id.starts_with("file::") {
            continue;
        }
        if graph.nodes.node_type[nid.as_usize()] != m1nd_core::types::NodeType::File {
            continue;
        }
        if let Some(ref scope) = normalized_scope {
            if !ext_id
                .strip_prefix("file::")
                .is_some_and(|path| path.starts_with(scope))
            {
                continue;
            }
        }
        graph_file_ids.insert(ext_id.clone());
        let prov = graph.resolve_node_provenance(nid);
        if let Some(loc) = prov
            .line_end
            .zip(prov.line_start)
            .map(|(end, start)| end.saturating_sub(start).saturating_add(1))
            .filter(|loc| *loc > 0)
        {
            graph_loc.insert(ext_id.clone(), loc);
        }
    }
    drop(graph);

    let missing_from_graph: Vec<serde_json::Value> = if checks.contains("existence") {
        disk_entries
            .iter()
            .filter(|entry| !graph_file_ids.contains(&entry.external_id))
            .map(|entry| {
                json!({
                    "external_id": entry.external_id,
                    "file_path": entry.file_path,
                    "size_bytes": entry.size_bytes,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    let missing_from_disk: Vec<serde_json::Value> = if checks.contains("existence") {
        graph_file_ids
            .iter()
            .filter(|external_id| !disk_map.contains_key(*external_id))
            .map(|external_id| json!({ "external_id": external_id }))
            .collect()
    } else {
        Vec::new()
    };

    let loc_drift: Vec<serde_json::Value> = if checks.contains("loc") {
        graph_file_ids
            .iter()
            .filter_map(|external_id| {
                let disk = disk_map.get(external_id)?;
                let graph_loc = graph_loc.get(external_id)?;
                let disk_loc = disk.loc?;
                if *graph_loc == disk_loc {
                    return None;
                }
                Some(json!({
                    "external_id": external_id,
                    "file_path": disk.file_path,
                    "graph_loc": graph_loc,
                    "disk_loc": disk_loc,
                    "delta": (disk_loc as i64 - *graph_loc as i64),
                }))
            })
            .collect()
    } else {
        Vec::new()
    };

    let hash_mismatches: Vec<serde_json::Value> = if checks.contains("hash") {
        disk_entries
            .iter()
            .filter_map(|entry| {
                let current_hash = simple_content_hash(Path::new(&entry.file_path))?;
                let known_hash = stored_inventory
                    .get(&entry.external_id)
                    .and_then(|item| item.sha256.clone())?;
                if current_hash == known_hash {
                    return None;
                }
                Some(json!({
                    "external_id": entry.external_id,
                    "file_path": entry.file_path,
                    "known_sha256": known_hash,
                    "disk_sha256": current_hash,
                }))
            })
            .collect()
    } else {
        Vec::new()
    };

    let drift_items = missing_from_graph.len()
        + missing_from_disk.len()
        + loc_drift.len()
        + hash_mismatches.len();
    let denominator = graph_file_ids.len().max(disk_entries.len()).max(1) as f32;
    let stale_confidence = (drift_items as f32 / denominator).min(1.0);

    Ok(json!({
        "scope": normalized_scope,
        "checks_run": checks.into_iter().collect::<Vec<_>>(),
        "graph_vs_disk": {
            "missing_from_graph": missing_from_graph,
            "missing_from_disk": missing_from_disk,
            "loc_drift": loc_drift,
            "hash_mismatches": hash_mismatches,
        },
        "stale_confidence": stale_confidence,
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
    }))
}

pub fn handle_coverage_session(
    state: &mut SessionState,
    input: layers::CoverageSessionInput,
) -> M1ndResult<serde_json::Value> {
    let session = state
        .coverage_sessions
        .get(&input.agent_id)
        .cloned()
        .unwrap_or_default();
    let total_files = state.file_inventory.len();
    let visited = session.visited_files.len();
    let unread_files: Vec<String> = state
        .file_inventory
        .values()
        .filter(|entry| !session.visited_files.contains(&entry.file_path))
        .map(|entry| entry.file_path.clone())
        .collect();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let duration_ms = if session.started_at_ms > 0 {
        now_ms.saturating_sub(session.started_at_ms)
    } else {
        0
    };

    Ok(json!({
        "agent_id": input.agent_id,
        "total_files": total_files,
        "visited": visited,
        "unvisited": unread_files,
        "coverage_pct": if total_files == 0 { 0.0 } else { (visited as f64 / total_files as f64) * 100.0 },
        "visited_nodes": session.visited_nodes.len(),
        "tools_used": session.tools_used,
        "session_duration_ms": duration_ms,
    }))
}

pub fn handle_external_references(
    state: &mut SessionState,
    input: layers::ExternalReferencesInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();
    let inventory = if state.file_inventory.is_empty() {
        inventory_from_roots(state, false, &[])
    } else {
        state.file_inventory.clone()
    };
    let disk_entries = filter_inventory_by_scope(state, &inventory, input.scope.as_deref());
    let roots: Vec<PathBuf> = state.ingest_roots.iter().map(PathBuf::from).collect();
    let markdown_link_regex =
        Regex::new(r#"\[[^\]]*\]\((/[^)\s]+)\)"#).map_err(|error| M1ndError::InvalidParams {
            tool: "external_references".into(),
            detail: error.to_string(),
        })?;
    let keyed_path_regex = Regex::new(
        r#"(?i)(path|root|repo|workspace|graph_source|plasticity_state|runtime_dir)[^=\n:]*[:=]\s*["']?(/[^"'\s]+)"#,
    )
    .map_err(|error| M1ndError::InvalidParams {
        tool: "external_references".into(),
        detail: error.to_string(),
    })?;
    let quoted_path_regex =
        Regex::new(r#"["'](/[^"'\s]+)["']"#).map_err(|error| M1ndError::InvalidParams {
            tool: "external_references".into(),
            detail: error.to_string(),
        })?;
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();

    for entry in disk_entries {
        let kind = classify_file_kind(&entry.file_path, &entry.language);
        if !supports_external_reference_scan(kind) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&entry.file_path) else {
            continue;
        };
        let scans: [(&Regex, &str, &str); 3] = [
            (&markdown_link_regex, "markdown_link", "high"),
            (&keyed_path_regex, "keyed_assignment", "high"),
            (&quoted_path_regex, "quoted_path", "medium"),
        ];
        for (regex, evidence_type, confidence) in scans {
            for capture in regex.captures_iter(&content) {
                let Some(matched) = capture.get(capture.len() - 1) else {
                    continue;
                };
                let raw_path = matched.as_str().trim().trim_matches('"').trim_matches('\'');
                let path = PathBuf::from(raw_path);
                if !is_plausible_external_path(&path) {
                    continue;
                }
                if is_placeholder_external_path(&path) {
                    continue;
                }
                if is_system_path(&path) {
                    continue;
                }
                if roots.iter().any(|root| path.starts_with(root)) {
                    continue;
                }
                let key = format!(
                    "{}::{}::{}",
                    entry.external_id,
                    evidence_type,
                    path.display()
                );
                if !seen.insert(key) {
                    continue;
                }
                let exists = path.exists();
                let suggested_action = if exists {
                    "consider federate or audit with external_refs enabled"
                } else {
                    "reference points to a missing path on disk"
                };
                results.push(json!({
                    "source_node": entry.external_id,
                    "file_path": entry.file_path,
                    "external_path": path,
                    "exists": exists,
                    "evidence_type": evidence_type,
                    "confidence": confidence,
                    "suggested_action": suggested_action,
                }));
            }
        }
    }

    results.sort_by(|a, b| {
        let confidence_rank = |value: &serde_json::Value| match value
            .get("confidence")
            .and_then(|item| item.as_str())
        {
            Some("high") => 0,
            Some("medium") => 1,
            _ => 2,
        };
        confidence_rank(a).cmp(&confidence_rank(b)).then_with(|| {
            b.get("exists")
                .and_then(|item| item.as_bool())
                .cmp(&a.get("exists").and_then(|item| item.as_bool()))
        })
    });

    Ok(json!({
        "results": results,
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
    }))
}

fn detect_profile(path: &Path, requested_profile: &str) -> String {
    if requested_profile != "auto" {
        return requested_profile.to_string();
    }

    let mut markdown_files = 0usize;
    let mut total_files = 0usize;
    let mut has_manifest = false;
    let mut has_external_refs = false;

    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if matches!(
                    name.as_str(),
                    ".git" | "target" | "node_modules" | "dist" | "build" | ".next" | ".venv"
                ) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            total_files += 1;
            if name.ends_with(".md") {
                markdown_files += 1;
            }
            if matches!(
                name.as_str(),
                "Cargo.toml" | "go.mod" | "package.json" | "pyproject.toml"
            ) {
                has_manifest = true;
            }
            let path_str = path.to_string_lossy();
            if path_str.contains(".codex")
                || path_str.contains(".omx")
                || path_str.contains(".github")
            {
                has_external_refs = true;
            }
            if total_files >= 2000 {
                break;
            }
        }
        if total_files >= 2000 {
            break;
        }
    }

    if total_files > 0 && markdown_files * 100 / total_files >= 60 {
        "coordination".to_string()
    } else if has_manifest {
        "production".to_string()
    } else if has_external_refs {
        "coordination".to_string()
    } else {
        "quick".to_string()
    }
}

fn profile_patterns(profile: &str, scan_patterns: &str) -> Vec<String> {
    if scan_patterns != "all" && scan_patterns != "default" {
        return scan_patterns
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect();
    }

    match profile {
        "coordination" => COORDINATION_SCAN_PATTERNS
            .iter()
            .map(|value| value.to_string())
            .collect(),
        "security" => SECURITY_SCAN_PATTERNS
            .iter()
            .map(|value| value.to_string())
            .collect(),
        "quick" => Vec::new(),
        _ => DEFAULT_SCAN_PATTERNS
            .iter()
            .map(|value| value.to_string())
            .collect(),
    }
}

fn compute_orphan_nodes(state: &SessionState, profile: &str) -> Vec<String> {
    let graph = state.graph.read();
    let mut nodes = Vec::new();
    for (interned, &nid) in &graph.id_to_node {
        let ext_id = graph.strings.resolve(*interned).to_string();
        if !ext_id.starts_with("file::") {
            continue;
        }
        if graph.nodes.node_type[nid.as_usize()] != m1nd_core::types::NodeType::File {
            continue;
        }
        let file_path = ext_id.trim_start_matches("file::");
        let kind = classify_file_kind(
            file_path,
            &extension_language(Path::new(file_path).extension().and_then(|e| e.to_str())),
        );
        if !counts_for_orphan_detection(kind, profile) {
            continue;
        }
        if is_auxiliary_code_path(file_path) {
            continue;
        }
        let total_degree = graph.csr.out_range(nid).len() + graph.csr.in_range(nid).len();
        if total_degree == 0 {
            nodes.push(ext_id);
        }
    }
    nodes.sort();
    nodes
}

fn grade_from_ratio(ratio: f64) -> &'static str {
    if ratio <= 0.05 {
        "A"
    } else if ratio <= 0.15 {
        "B"
    } else if ratio <= 0.30 {
        "C"
    } else if ratio <= 0.50 {
        "D"
    } else {
        "F"
    }
}

fn grade_or_na(ratio: Option<f64>) -> String {
    ratio.map(grade_from_ratio).unwrap_or("N/A").to_string()
}

fn tasknotes_summary(root: &Path) -> serde_json::Value {
    let path = root.join("docs/AGENT-TASKNOTES.md");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return json!({
            "available": false,
            "path": path.to_string_lossy(),
        });
    };

    let mut open = 0usize;
    let mut resolved = 0usize;
    let mut mode = "";
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "## Open Notes" {
            mode = "open";
            continue;
        }
        if trimmed == "## Resolved Notes" {
            mode = "resolved";
            continue;
        }
        if trimmed.starts_with("### ") {
            match mode {
                "open" => open += 1,
                "resolved" => resolved += 1,
                _ => {}
            }
        }
    }

    json!({
        "available": true,
        "path": path.to_string_lossy(),
        "open_notes": open,
        "resolved_notes": resolved,
    })
}

fn release_metadata(root: &Path, git_state: &serde_json::Value) -> serde_json::Value {
    let crates = [
        ("m1nd-core", root.join("m1nd-core/Cargo.toml")),
        ("m1nd-ingest", root.join("m1nd-ingest/Cargo.toml")),
        ("m1nd-mcp", root.join("m1nd-mcp/Cargo.toml")),
    ];
    let mut versions = BTreeMap::new();
    let version_regex = Regex::new(r#"^version\s*=\s*"([^"]+)""#).ok();
    for (name, path) in crates {
        if let (Ok(content), Some(regex)) = (std::fs::read_to_string(&path), version_regex.as_ref())
        {
            if let Some(captures) = content.lines().find_map(|line| regex.captures(line)) {
                if let Some(value) = captures.get(1) {
                    versions.insert(name.to_string(), value.as_str().to_string());
                }
            }
        }
    }

    json!({
        "crate_versions": versions,
        "head": git_state.get("head"),
        "branch": git_state.get("branch"),
        "clean": git_state.get("clean"),
    })
}

fn trail_summary(state: &SessionState) -> serde_json::Value {
    let dir = state.runtime_root.join("trails");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return json!({
            "available": false,
            "path": dir.to_string_lossy(),
        });
    };

    let mut count = 0usize;
    let mut latest = Vec::new();
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    for path in &files {
        count += 1;
    }
    for path in files.into_iter().rev().take(5) {
        if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
            latest.push(name.to_string());
        }
    }
    json!({
        "available": true,
        "path": dir.to_string_lossy(),
        "trail_count": count,
        "latest": latest,
    })
}

fn filter_external_reference_results_for_profile(
    profile: &str,
    results: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    results
        .iter()
        .filter(|entry| {
            let file_path = entry
                .get("file_path")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let kind = classify_file_kind(
                file_path,
                &extension_language(Path::new(file_path).extension().and_then(|e| e.to_str())),
            );
            let exists = entry
                .get("exists")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            match profile {
                "coordination" => supports_external_reference_scan(kind),
                _ => {
                    matches!(
                        kind,
                        AuditFileKind::Code | AuditFileKind::Config | AuditFileKind::Script
                    ) && !is_auxiliary_code_path(file_path)
                        && exists
                }
            }
        })
        .cloned()
        .collect()
}

pub fn handle_audit(
    state: &mut SessionState,
    input: layers::AuditInput,
) -> M1ndResult<serde_json::Value> {
    let start = Instant::now();
    let path = PathBuf::from(&input.path);
    let effective_profile = detect_profile(&path, &input.profile);
    let ingest_adapter = if effective_profile == "coordination" {
        "memory"
    } else {
        "code"
    };
    let dotfile_patterns = if input.include_config {
        vec![
            ".codex/**".to_string(),
            ".omx/**".to_string(),
            ".github/**".to_string(),
        ]
    } else {
        Vec::new()
    };

    let _ = tools::handle_ingest(
        state,
        protocol::IngestInput {
            path: input.path.clone(),
            agent_id: input.agent_id.clone(),
            incremental: false,
            adapter: ingest_adapter.into(),
            mode: "replace".into(),
            namespace: None,
            include_dotfiles: input.include_config,
            dotfile_patterns: dotfile_patterns.clone(),
        },
    )?;

    let health = tools::handle_health(
        state,
        protocol::HealthInput {
            agent_id: input.agent_id.clone(),
        },
    )?;
    let graph_is_empty = health.node_count == 0;
    let panoramic = if graph_is_empty {
        layers::PanoramicOutput {
            modules: Vec::new(),
            total_modules: 0,
            critical_alerts: Vec::new(),
            scope_applied: false,
            elapsed_ms: 0.0,
        }
    } else {
        report_handlers::handle_panoramic(
            state,
            layers::PanoramicInput {
                agent_id: input.agent_id.clone(),
                scope: None,
                top_n: 25,
            },
        )?
    };
    let layers = if graph_is_empty {
        json!({ "layers": [], "violations": [] })
    } else {
        layer_handlers::handle_layers(
            state,
            layers::LayersInput {
                agent_id: input.agent_id.clone(),
                scope: None,
                max_layers: 8,
                include_violations: true,
                min_nodes_per_layer: 2,
                node_types: Vec::new(),
                naming_strategy: "auto".into(),
                exclude_tests: false,
                violation_limit: 100,
            },
        )?
    };

    let scan_patterns = profile_patterns(&effective_profile, &input.scan_patterns);
    let scan_results = if graph_is_empty || scan_patterns.is_empty() {
        json!({
            "patterns_run": 0,
            "total_findings": 0,
            "by_pattern": [],
        })
    } else {
        handle_scan_all(
            state,
            layers::ScanAllInput {
                agent_id: input.agent_id.clone(),
                scope: None,
                severity_min: 0.3,
                graph_validate: true,
                limit_per_pattern: 50,
                patterns: scan_patterns.clone(),
            },
        )?
    };

    let cross_verify = if input.cross_verify {
        handle_cross_verify(
            state,
            layers::CrossVerifyInput {
                agent_id: input.agent_id.clone(),
                scope: None,
                check: vec!["existence".into(), "loc".into(), "hash".into()],
                include_dotfiles: input.include_config,
                dotfile_patterns: dotfile_patterns.clone(),
            },
        )?
    } else {
        json!({ "graph_vs_disk": {}, "stale_confidence": 0.0 })
    };

    let external_references = if input.external_refs {
        handle_external_references(
            state,
            layers::ExternalReferencesInput {
                agent_id: input.agent_id.clone(),
                scope: None,
            },
        )?
    } else {
        json!({ "results": [] })
    };
    let filtered_external_references = json!({
        "results": filter_external_reference_results_for_profile(
            &effective_profile,
            external_references
                .get("results")
                .and_then(|value| value.as_array())
                .map(|value| value.as_slice())
                .unwrap_or(&[]),
        )
    });

    let fingerprint = if graph_is_empty {
        json!({ "equivalent_pairs": [] })
    } else {
        tools::handle_fingerprint(
            state,
            protocol::FingerprintInput {
                target_node: None,
                agent_id: input.agent_id.clone(),
                similarity_threshold: 0.85,
                probe_queries: None,
            },
        )?
    };
    let orphan_nodes = if graph_is_empty {
        Vec::new()
    } else {
        compute_orphan_nodes(state, &effective_profile)
    };
    let harmonic_center = if graph_is_empty {
        None
    } else {
        panoramic.modules.first().map(|module| {
            json!({
                "node": module.node_id,
                "amplitude": module.combined_risk,
            })
        })
    };

    let inventory_entries = filter_inventory_by_scope(
        state,
        &if state.file_inventory.is_empty() {
            inventory_from_roots(state, input.include_config, &dotfile_patterns)
        } else {
            state.file_inventory.clone()
        },
        None,
    );
    let inventory: Vec<serde_json::Value> = inventory_entries
        .iter()
        .map(|entry| {
            json!({
                "external_id": entry.external_id,
                "file_path": entry.file_path,
                "kind": match classify_file_kind(&entry.file_path, &entry.language) {
                    AuditFileKind::Code => "code",
                    AuditFileKind::Script => "script",
                    AuditFileKind::Config => "config",
                    AuditFileKind::Doc => "doc",
                    AuditFileKind::BenchmarkArtifact => "benchmark_artifact",
                    AuditFileKind::Generated => "generated",
                    AuditFileKind::Asset => "asset",
                    AuditFileKind::Unknown => "unknown",
                },
                "loc": entry.loc,
                "language": entry.language,
                "size_bytes": entry.size_bytes,
                "last_modified_ms": entry.last_modified_ms,
                "commit_count": entry.commit_count,
            })
        })
        .collect();
    let actionable_for_grades: Vec<&FileInventoryEntry> = inventory_entries
        .iter()
        .filter(|entry| {
            counts_for_grading(
                classify_file_kind(&entry.file_path, &entry.language),
                &effective_profile,
            )
        })
        .collect();
    let core_actionable_for_connectivity: Vec<&FileInventoryEntry> = actionable_for_grades
        .iter()
        .copied()
        .filter(|entry| !is_auxiliary_code_path(&entry.file_path))
        .collect();

    let git_state = if input.include_git {
        collect_git_state(state, 20)
    } else {
        json!({ "available": false })
    };
    let temporal_intelligence = if graph_is_empty {
        json!({
            "trust": null,
            "tremor": null,
            "ghost_edges": null,
        })
    } else {
        let trust = if matches!(
            effective_profile.as_str(),
            "production" | "security" | "migration"
        ) {
            layer_handlers::handle_trust(
                state,
                layers::TrustInput {
                    agent_id: input.agent_id.clone(),
                    scope: "file".into(),
                    min_history: 1,
                    top_k: 10,
                    node_filter: None,
                    sort_by: "trust_asc".into(),
                    decay_half_life_days: 30.0,
                    risk_cap: 3.0,
                },
            )?
        } else {
            json!({ "available": false })
        };
        let tremor = if matches!(
            effective_profile.as_str(),
            "production" | "security" | "migration"
        ) {
            layer_handlers::handle_tremor(
                state,
                layers::TremorInput {
                    agent_id: input.agent_id.clone(),
                    window: "30d".into(),
                    threshold: 0.1,
                    top_k: 10,
                    node_filter: None,
                    include_history: false,
                    min_observations: 3,
                    sensitivity: 1.0,
                },
            )?
        } else {
            json!({ "available": false })
        };
        let ghost_edges = if input.include_git {
            layer_handlers::handle_ghost_edges(
                state,
                layers::GhostEdgesInput {
                    agent_id: input.agent_id.clone(),
                    depth: "30d".into(),
                    scope: None,
                    top_k: 20,
                },
            )?
        } else {
            json!({ "available": false })
        };
        json!({
            "trust": trust,
            "tremor": tremor,
            "ghost_edges": ghost_edges,
        })
    };

    let scan_total_findings = scan_results
        .get("total_findings")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as f64;
    let missing_from_graph_count = cross_verify
        .pointer("/graph_vs_disk/missing_from_graph")
        .and_then(|value| value.as_array())
        .map_or(0, |value| value.len()) as f64;
    let missing_from_disk_count = cross_verify
        .pointer("/graph_vs_disk/missing_from_disk")
        .and_then(|value| value.as_array())
        .map_or(0, |value| value.len()) as f64;
    let total_inventory = inventory.len().max(1) as f64;
    let actionable_inventory = actionable_for_grades.len();
    let critical_modules = panoramic.critical_alerts.len() as f64;
    let actionable_orphans = orphan_nodes.len();
    let test_coverage_findings = scan_results
        .get("by_pattern")
        .and_then(|value| value.as_array())
        .and_then(|patterns| {
            patterns.iter().find(|entry| {
                entry.get("pattern").and_then(|value| value.as_str()) == Some("test_coverage")
            })
        })
        .and_then(|entry| entry.get("findings"))
        .and_then(|value| value.as_array())
        .map_or(0, |value| value.len()) as f64;
    let external_reference_count = filtered_external_references
        .get("results")
        .and_then(|value| value.as_array())
        .map_or(0, |value| value.len()) as f64;

    let health_grades = json!({
        "connectivity": grade_or_na(if actionable_inventory > 0 {
            let denom = core_actionable_for_connectivity.len();
            if denom > 0 {
                Some(actionable_orphans as f64 / denom as f64)
            } else {
                None
            }
        } else {
            None
        }),
        "test_coverage": grade_or_na(if actionable_inventory > 0 {
            Some(test_coverage_findings / actionable_inventory as f64)
        } else {
            None
        }),
        "duplication": grade_or_na(Some(
            fingerprint
                .get("equivalent_pairs")
                .and_then(|value| value.as_array())
                .map_or(0, |pairs| pairs.len()) as f64 / total_inventory,
        )),
        "risk_concentration": grade_or_na(if actionable_inventory > 0 {
            Some(critical_modules / actionable_inventory as f64)
        } else {
            None
        }),
        "staleness": grade_or_na(if actionable_inventory > 0 {
            Some((missing_from_graph_count + missing_from_disk_count) / actionable_inventory as f64)
        } else {
            None
        }),
        "coordination_truth": grade_or_na(match effective_profile.as_str() {
            "coordination" => Some(external_reference_count / total_inventory),
            _ => None,
        }),
    });

    let mut recommendations = Vec::new();
    if scan_total_findings > 0.0 {
        let next_target = scan_results
            .get("by_pattern")
            .and_then(|value| value.as_array())
            .and_then(|patterns| {
                patterns.iter().find_map(|pattern| {
                    pattern
                        .get("findings")
                        .and_then(|value| value.as_array())
                        .and_then(|findings| findings.first())
                        .and_then(|finding| {
                            finding
                                .get("file_path")
                                .and_then(|value| value.as_str())
                                .filter(|value| !value.is_empty())
                                .map(|value| value.to_string())
                                .or_else(|| {
                                    finding
                                        .get("node_id")
                                        .and_then(|value| value.as_str())
                                        .filter(|value| !value.is_empty())
                                        .map(|value| value.to_string())
                                })
                        })
                })
            });
        recommendations.push(json!({
            "priority": "high",
            "category": "scan",
            "description": "Triage grouped scan findings before broad refactors; the audit found structural issues worth resolving first.",
            "next_step_tool": "batch_view",
            "next_target": next_target,
            "confidence": "high",
            "expected_payoff": "high",
        }));
    }
    if !orphan_nodes.is_empty() {
        recommendations.push(json!({
            "priority": "medium",
            "category": "integrity",
            "description": "Review isolated file nodes and confirm whether they are intentionally disconnected or stale.",
            "affected_nodes": orphan_nodes,
            "next_step_tool": "batch_view",
            "confidence": "medium",
            "expected_payoff": "medium",
        }));
    }
    if filtered_external_references
        .get("results")
        .and_then(|value| value.as_array())
        .is_some_and(|results| !results.is_empty())
    {
        recommendations.push(json!({
            "priority": "medium",
            "category": "federation",
            "description": "Consider federating or explicitly tracking external repositories referenced by this workspace.",
            "next_step_tool": "external_references",
            "confidence": "medium",
            "expected_payoff": "high",
        }));
    }
    if actionable_inventory > 0 && test_coverage_findings > 0.0 {
        recommendations.push(json!({
            "priority": "medium",
            "category": "tests",
            "description": "The audit found test-coverage gaps on actionable files; inspect the worst findings before treating this surface as release-ready.",
            "expected_payoff": "medium",
            "next_step_tool": "batch_view",
            "confidence": "medium",
        }));
    }
    let system_context = json!({
        "boot_memory": {
            "available": !state.boot_memory.is_empty(),
            "count": state.boot_memory.len(),
            "keys": state.boot_memory.keys().take(10).cloned().collect::<Vec<_>>(),
        },
        "tasknotes": tasknotes_summary(&path),
        "trail_summary": trail_summary(state),
        "release_metadata": release_metadata(&path, &git_state),
    });

    let report = json!({
        "identity": {
            "path": input.path,
            "profile": effective_profile,
            "depth": input.depth,
            "branch": git_state.get("branch"),
            "head": git_state.get("head"),
            "clean": git_state.get("clean"),
            "status": health.status,
        },
        "inventory": {
            "files": inventory,
            "kind_breakdown": inventory_breakdown(&inventory_entries),
        },
        "topology": {
            "nodes": health.node_count,
            "edges": health.edge_count,
            "layers": layers.get("layers").cloned().unwrap_or(json!([])),
            "violations": layers.get("violations").cloned().unwrap_or(json!([])),
            "risk_modules": panoramic.modules,
            "critical_alerts": panoramic.critical_alerts,
        },
        "structural_integrity": {
            "orphan_nodes": orphan_nodes,
            "equivalent_pairs": fingerprint.get("equivalent_pairs").cloned().unwrap_or(json!([])),
            "harmonic_center": harmonic_center,
        },
        "scan_results": scan_results,
        "git_state": git_state,
        "filesystem_verification": cross_verify,
        "external_references": filtered_external_references,
        "system_context": system_context,
        "health_grades": health_grades,
        "recommendations": recommendations,
        "planes": {
            "repo_truth": {
                "identity": {
                    "branch": git_state.get("branch"),
                    "head": git_state.get("head"),
                    "clean": git_state.get("clean"),
                },
                "inventory_summary": inventory_breakdown(&inventory_entries),
                "filesystem_verification": cross_verify,
                "config_visibility": {
                    "include_config": input.include_config,
                    "dotfile_patterns": dotfile_patterns,
                },
            },
            "structural_topology": {
                "topology": {
                    "nodes": health.node_count,
                    "edges": health.edge_count,
                    "critical_alerts": panoramic.critical_alerts,
                },
                "structural_integrity": {
                    "orphan_nodes": orphan_nodes,
                    "equivalent_pairs": fingerprint.get("equivalent_pairs").cloned().unwrap_or(json!([])),
                    "harmonic_center": harmonic_center,
                },
            },
            "temporal_intelligence": {
                "git_state": git_state,
                "signals": temporal_intelligence,
            },
            "runtime_evidence": {
                "available": false,
                "reason": "no runtime overlay input was provided to this audit",
            },
            "security_flow": {
                "scan_results": scan_results,
            },
            "agent_action": {
                "recommendations": recommendations,
                "health_grades": health_grades,
            }
        },
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
    });

    if input.report_format == "json" {
        return Ok(report);
    }

    let markdown = format!(
        "# m1nd Audit\n\n- Profile: `{}`\n- Nodes: `{}`\n- Edges: `{}`\n- Critical modules: `{}`\n- Orphan nodes: `{}`\n- Scan findings: `{}`\n- Stale confidence: `{:.2}`\n",
        effective_profile,
        health.node_count,
        health.edge_count,
        panoramic.critical_alerts.len(),
        orphan_nodes.len(),
        scan_total_findings as usize,
        cross_verify
            .get("stale_confidence")
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0)
    );
    let (report_markdown, truncated, inline_summary) =
        truncate_text(markdown, input.max_output_chars, "audit");
    Ok(json!({
        "report_format": "markdown",
        "profile": effective_profile,
        "report_markdown": report_markdown,
        "truncated": truncated,
        "inline_summary": inline_summary,
        "report": report,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::{Graph, NodeProvenanceInput};
    use m1nd_core::types::NodeType;

    fn build_empty_state(root: &Path) -> SessionState {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let graph = Graph::new();
        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());
        state
    }

    #[test]
    fn collect_git_state_reports_clean_repo() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("README.md"), "# test\n").expect("write readme");
        Command::new("git")
            .current_dir(root)
            .args(["init"])
            .output()
            .expect("git init");
        Command::new("git")
            .current_dir(root)
            .args(["config", "user.email", "test@example.com"])
            .output()
            .expect("config email");
        Command::new("git")
            .current_dir(root)
            .args(["config", "user.name", "Test"])
            .output()
            .expect("config name");
        Command::new("git")
            .current_dir(root)
            .args(["add", "."])
            .output()
            .expect("git add");
        Command::new("git")
            .current_dir(root)
            .args(["commit", "-m", "init"])
            .output()
            .expect("git commit");

        let state = build_empty_state(root);
        let git = collect_git_state(&state, 5);
        assert_eq!(git["available"], true);
        assert_eq!(git["clean"], true);
        assert!(git["branch"].as_str().is_some());
    }

    #[test]
    fn cross_verify_reports_files_missing_from_disk() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(root.join("src/lib.rs"), "pub fn ok() {}\n").expect("write lib");

        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut graph = Graph::new();
        let file_node = graph
            .add_node("file::src/lib.rs", "lib.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add file node");
        graph.set_node_provenance(
            file_node,
            NodeProvenanceInput {
                source_path: Some("src/lib.rs"),
                line_start: Some(1),
                line_end: Some(1),
                excerpt: None,
                namespace: None,
                canonical: true,
            },
        );
        graph.finalize().expect("finalize");
        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());
        std::fs::remove_file(root.join("src/lib.rs")).expect("remove lib");

        let output = handle_cross_verify(
            &mut state,
            layers::CrossVerifyInput {
                agent_id: "test".into(),
                scope: None,
                check: vec!["existence".into()],
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("cross_verify");

        assert!(output["graph_vs_disk"]["missing_from_disk"]
            .as_array()
            .is_some_and(|items| items
                .iter()
                .any(|item| item["external_id"] == "file::src/lib.rs")));
    }

    #[test]
    fn audit_auto_detects_coordination_profile_for_doc_heavy_repo() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("docs")).expect("docs dir");
        std::fs::write(root.join("README.md"), "# intro\n").expect("readme");
        std::fs::write(root.join("docs/plan.md"), "# plan\n").expect("plan");
        std::fs::write(root.join("docs/runbook.md"), "# runbook\n").expect("runbook");
        std::fs::write(root.join("notes.md"), "# notes\n").expect("notes");

        let mut state = build_empty_state(&root);
        let output = handle_audit(
            &mut state,
            layers::AuditInput {
                agent_id: "test".into(),
                path: root.to_string_lossy().to_string(),
                profile: "auto".into(),
                depth: "quick".into(),
                cross_verify: true,
                include_git: false,
                include_config: false,
                scan_patterns: "default".into(),
                external_refs: false,
                report_format: "json".into(),
                max_output_chars: None,
            },
        )
        .expect("audit");

        assert_eq!(output["identity"]["profile"], "coordination");
        assert!(output["inventory"]["files"].as_array().is_some());
    }

    #[test]
    fn audit_orphan_detection_ignores_non_code_orphans() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(runtime_dir.clone()).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };

        let mut graph = Graph::new();
        let code = graph
            .add_node(
                "file::src/orphan.rs",
                "orphan.rs",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("code");
        graph.set_node_provenance(
            code,
            NodeProvenanceInput {
                source_path: Some("src/orphan.rs"),
                line_start: Some(1),
                line_end: Some(10),
                excerpt: None,
                namespace: None,
                canonical: true,
            },
        );
        let config_node = graph
            .add_node(
                "file::package.json",
                "package.json",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("config");
        graph.set_node_provenance(
            config_node,
            NodeProvenanceInput {
                source_path: Some("package.json"),
                line_start: Some(1),
                line_end: Some(1),
                excerpt: None,
                namespace: None,
                canonical: true,
            },
        );
        graph.finalize().expect("finalize");

        let mut state =
            SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session");
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        state.workspace_root = Some(root.to_string_lossy().to_string());

        let orphans = compute_orphan_nodes(&state, "production");
        assert_eq!(orphans, vec!["file::src/orphan.rs"]);
    }

    #[test]
    fn audit_filters_placeholder_and_system_external_references_for_production() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        let external_root = temp.path().join("external-repo");
        std::fs::create_dir_all(&external_root).expect("external root");
        let external_path = external_root.join("src/lib.rs");
        std::fs::create_dir_all(external_path.parent().expect("parent")).expect("parent");
        std::fs::write(&external_path, "pub fn ext() {}\n").expect("write external");

        let source = format!(
            "const A: &str = \"/usr/lib/\";\nconst B: &str = \"/your/project\";\nconst C: &str = \"{}\";\n",
            external_path.to_string_lossy()
        );
        std::fs::write(root.join("src/lib.rs"), source).expect("write source");

        let mut state = build_empty_state(&root);
        let output = handle_audit(
            &mut state,
            layers::AuditInput {
                agent_id: "test".into(),
                path: root.to_string_lossy().to_string(),
                profile: "production".into(),
                depth: "quick".into(),
                cross_verify: true,
                include_git: false,
                include_config: false,
                scan_patterns: "default".into(),
                external_refs: true,
                report_format: "json".into(),
                max_output_chars: None,
            },
        )
        .expect("audit");

        let results = output["external_references"]["results"]
            .as_array()
            .expect("external reference results");
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0]["external_path"].as_str(),
            Some(external_path.to_string_lossy().as_ref())
        );
    }
}
