use crate::protocol::layers;
use crate::session::{DaemonAlert, DaemonTrackedFile, FileInventoryEntry, SessionState};
use m1nd_core::error::{M1ndError, M1ndResult};
use serde_json::json;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

fn simple_content_hash(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

fn extension_language(extension: Option<&str>) -> String {
    match extension.unwrap_or_default() {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "jsx" => "javascript",
        "ts" => "typescript",
        "tsx" => "typescript",
        "go" => "go",
        "java" => "java",
        "md" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "sh" => "bash",
        _ => "text",
    }
    .to_string()
}

fn inventory_from_watch_paths(watch_paths: &[String]) -> HashMap<String, FileInventoryEntry> {
    let mut inventory = HashMap::new();

    for root in watch_paths {
        let root_path = PathBuf::from(root);
        if !root_path.exists() {
            continue;
        }

        if root_path.is_file() {
            let Ok(metadata) = std::fs::metadata(&root_path) else {
                continue;
            };
            let extension = root_path.extension().and_then(|ext| ext.to_str());
            let external_id = root_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("file::{}", name))
                .unwrap_or_else(|| format!("file::{}", root_path.to_string_lossy()));
            inventory.insert(
                external_id.clone(),
                FileInventoryEntry {
                    external_id,
                    file_path: root_path.to_string_lossy().to_string(),
                    size_bytes: metadata.len(),
                    last_modified_ms: metadata
                        .modified()
                        .ok()
                        .and_then(|ts| ts.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|value| value.as_millis() as u64)
                        .unwrap_or(0),
                    language: extension_language(extension),
                    commit_count: 0,
                    loc: None,
                    sha256: simple_content_hash(&root_path),
                },
            );
            continue;
        }

        let config = m1nd_ingest::IngestConfig {
            root: root_path.clone(),
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
                    external_id,
                    file_path: file.path.to_string_lossy().to_string(),
                    size_bytes: file.size_bytes,
                    last_modified_ms: (file.last_modified * 1000.0).round() as u64,
                    language: extension_language(file.extension.as_deref()),
                    commit_count: file.commit_count,
                    loc: None,
                    sha256: simple_content_hash(&file.path),
                },
            );
        }
    }

    inventory
}

fn tracked_files_from_inventory(
    inventory: &HashMap<String, FileInventoryEntry>,
) -> HashMap<String, DaemonTrackedFile> {
    inventory
        .iter()
        .map(|(external_id, entry)| {
            (
                external_id.clone(),
                DaemonTrackedFile {
                    external_id: external_id.clone(),
                    file_path: entry.file_path.clone(),
                    last_modified_ms: entry.last_modified_ms,
                    size_bytes: entry.size_bytes,
                    sha256: entry.sha256.clone(),
                },
            )
        })
        .collect()
}

fn git_root_for_watch_paths(watch_paths: &[String]) -> Option<PathBuf> {
    for raw_path in watch_paths {
        let path = PathBuf::from(raw_path);
        let root_hint = if path.is_dir() {
            path
        } else {
            path.parent().map(Path::to_path_buf).unwrap_or(path)
        };

        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&root_hint)
            .output()
            .ok()?;
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

fn git_head_ref(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn git_changed_absolute_paths(
    root: &Path,
    since_ref: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let mut changed = Vec::new();
    let diff_args: Vec<&str> = if let Some(reference) = since_ref {
        vec!["diff", "--name-only", reference, "--"]
    } else {
        vec!["status", "--porcelain"]
    };
    let output = Command::new("git")
        .args(&diff_args)
        .current_dir(root)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let rel = if since_ref.is_some() {
            line.to_string()
        } else {
            line.get(3..).unwrap_or(line).trim().to_string()
        };
        if rel.is_empty() {
            continue;
        }
        changed.push(root.join(rel));
    }

    Ok(changed)
}

fn git_operation_in_progress(root: &Path) -> Option<String> {
    let git_dir = root.join(".git");
    let checks = [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("BISECT_LOG", "bisect"),
        ("index.lock", "index-lock"),
    ];
    for (relative, kind) in checks {
        if git_dir.join(relative).exists() {
            return Some(kind.to_string());
        }
    }
    None
}

pub fn handle_daemon_start(
    state: &mut SessionState,
    input: layers::DaemonStartInput,
) -> M1ndResult<serde_json::Value> {
    let started_at_ms = now_ms();
    let watch_paths = if input.watch_paths.is_empty() {
        state.ingest_roots.clone()
    } else {
        input.watch_paths
    };
    let initial_inventory = inventory_from_watch_paths(&watch_paths);
    state.daemon_state.active = true;
    state.daemon_state.started_at_ms = Some(started_at_ms);
    state.daemon_state.last_tick_ms = Some(started_at_ms);
    state.daemon_state.last_tick_trigger = None;
    state.daemon_state.watch_paths = watch_paths;
    state.daemon_state.poll_interval_ms = input.poll_interval_ms;
    state.daemon_state.coalesce_window_ms = 75;
    state.daemon_state.pending_rerun = false;
    state.daemon_state.tick_in_flight = false;
    state.daemon_state.last_coalesced_event_ms = None;
    state.daemon_state.coalesced_event_count = 0;
    state.daemon_state.tracked_files = tracked_files_from_inventory(&initial_inventory);
    state.daemon_state.tick_count = 0;
    state.daemon_state.last_tick_duration_ms = None;
    state.daemon_state.last_tick_changed_files = 0;
    state.daemon_state.last_tick_deleted_files = 0;
    state.daemon_state.last_tick_alerts_emitted = 0;
    state.daemon_state.idle_streak = 0;
    state.daemon_state.max_backoff_multiplier = 8;
    state.daemon_state.watch_backend = "polling".into();
    state.daemon_state.watch_backend_error = None;
    state.daemon_state.watch_events_seen = 0;
    state.daemon_state.watch_events_dropped = 0;
    state.daemon_state.last_watch_event_ms = None;
    state.daemon_state.git_root = git_root_for_watch_paths(&state.daemon_state.watch_paths)
        .map(|root| root.to_string_lossy().to_string());
    state.daemon_state.git_since_ref = state
        .daemon_state
        .git_root
        .as_deref()
        .and_then(|root| git_head_ref(Path::new(root)));
    state.daemon_state.last_git_scan_ms = None;
    state.daemon_state.last_git_changed_files = 0;
    state.daemon_state.git_backend_error = None;
    state.daemon_state.git_operation_in_progress = false;
    state.daemon_state.git_operation_kind = None;
    state.daemon_state.deferred_ticks = 0;
    if state.daemon_state.git_root.is_some() {
        state.daemon_state.watch_backend = "git_native_fs".into();
    }
    state.persist_daemon_state()?;
    Ok(json!({
        "status": "started",
        "active": true,
        "started_at_ms": started_at_ms,
        "watch_paths": state.daemon_state.watch_paths,
        "poll_interval_ms": state.daemon_state.poll_interval_ms,
        "coalesce_window_ms": state.daemon_state.coalesce_window_ms,
        "tracked_files": state.daemon_state.tracked_files.len(),
        "watch_backend": state.daemon_state.watch_backend,
        "git_root": state.daemon_state.git_root,
        "git_since_ref": state.daemon_state.git_since_ref,
        "git_operation_in_progress": state.daemon_state.git_operation_in_progress,
        "git_operation_kind": state.daemon_state.git_operation_kind,
    }))
}

pub fn handle_daemon_stop(
    state: &mut SessionState,
    _input: layers::DaemonStopInput,
) -> M1ndResult<serde_json::Value> {
    state.daemon_state.active = false;
    state.daemon_state.last_tick_ms = Some(now_ms());
    state.persist_daemon_state()?;
    Ok(json!({
        "status": "stopped",
        "active": false,
        "started_at_ms": state.daemon_state.started_at_ms,
        "last_tick_ms": state.daemon_state.last_tick_ms,
        "watch_backend": state.daemon_state.watch_backend,
    }))
}

pub fn handle_daemon_status(
    state: &mut SessionState,
    _input: layers::DaemonStatusInput,
) -> M1ndResult<serde_json::Value> {
    let now = now_ms();
    let next_tick_due_ms = if state.daemon_state.active && state.daemon_state.poll_interval_ms > 0 {
        state
            .daemon_state
            .last_tick_ms
            .map(|last| last.saturating_add(state.daemon_state.poll_interval_ms))
    } else {
        None
    };
    let overdue_ms = next_tick_due_ms.map(|due| now.saturating_sub(due));
    let effective_poll_interval_ms = state.daemon_state.poll_interval_ms.saturating_mul(
        2u64.pow(
            state
                .daemon_state
                .idle_streak
                .min(state.daemon_state.max_backoff_multiplier.saturating_sub(1)),
        ),
    );
    Ok(json!({
        "active": state.daemon_state.active,
        "started_at_ms": state.daemon_state.started_at_ms,
        "last_tick_ms": state.daemon_state.last_tick_ms,
        "last_tick_trigger": state.daemon_state.last_tick_trigger,
        "next_tick_due_ms": next_tick_due_ms,
        "overdue_ms": overdue_ms,
        "watch_paths": state.daemon_state.watch_paths,
        "poll_interval_ms": state.daemon_state.poll_interval_ms,
        "effective_poll_interval_ms": effective_poll_interval_ms,
        "coalesce_window_ms": state.daemon_state.coalesce_window_ms,
        "watch_backend": state.daemon_state.watch_backend,
        "watch_backend_error": state.daemon_state.watch_backend_error,
        "watch_events_seen": state.daemon_state.watch_events_seen,
        "watch_events_dropped": state.daemon_state.watch_events_dropped,
        "last_watch_event_ms": state.daemon_state.last_watch_event_ms,
        "git_root": state.daemon_state.git_root,
        "git_since_ref": state.daemon_state.git_since_ref,
        "last_git_scan_ms": state.daemon_state.last_git_scan_ms,
        "last_git_changed_files": state.daemon_state.last_git_changed_files,
        "git_backend_error": state.daemon_state.git_backend_error,
        "git_operation_in_progress": state.daemon_state.git_operation_in_progress,
        "git_operation_kind": state.daemon_state.git_operation_kind,
        "deferred_ticks": state.daemon_state.deferred_ticks,
        "pending_rerun": state.daemon_state.pending_rerun,
        "tick_in_flight": state.daemon_state.tick_in_flight,
        "last_coalesced_event_ms": state.daemon_state.last_coalesced_event_ms,
        "coalesced_event_count": state.daemon_state.coalesced_event_count,
        "alert_count": state.daemon_alerts.len(),
        "tracked_files": state.daemon_state.tracked_files.len(),
        "tick_count": state.daemon_state.tick_count,
        "last_tick_duration_ms": state.daemon_state.last_tick_duration_ms,
        "last_tick_changed_files": state.daemon_state.last_tick_changed_files,
        "last_tick_deleted_files": state.daemon_state.last_tick_deleted_files,
        "last_tick_alerts_emitted": state.daemon_state.last_tick_alerts_emitted,
        "idle_streak": state.daemon_state.idle_streak,
        "max_backoff_multiplier": state.daemon_state.max_backoff_multiplier,
        "runtime_root": state.runtime_root,
        "graph_generation": state.graph_generation,
        "cache_generation": state.cache_generation,
    }))
}

pub fn handle_daemon_tick(
    state: &mut SessionState,
    input: layers::DaemonTickInput,
) -> M1ndResult<serde_json::Value> {
    let start = std::time::Instant::now();
    if !state.daemon_state.active {
        return Err(M1ndError::InvalidParams {
            tool: "daemon_tick".into(),
            detail: "Start the daemon before ticking it.".into(),
        });
    }

    let live_inventory = inventory_from_watch_paths(&state.daemon_state.watch_paths);
    let mut changed_entries = Vec::new();
    let mut deleted_entries = Vec::new();

    if state.daemon_state.watch_backend == "git_native_fs" {
        if let Some(root) = state.daemon_state.git_root.clone() {
            if let Some(kind) = git_operation_in_progress(Path::new(&root)) {
                state.daemon_state.git_operation_in_progress = true;
                state.daemon_state.git_operation_kind = Some(kind);
                state.daemon_state.deferred_ticks =
                    state.daemon_state.deferred_ticks.saturating_add(1);
                state.daemon_state.last_tick_trigger = Some("reconciliation".into());
                state.daemon_state.last_tick_ms = Some(now_ms());
                state.daemon_state.tick_count = state.daemon_state.tick_count.saturating_add(1);
                state.daemon_state.last_tick_duration_ms =
                    Some(start.elapsed().as_secs_f64() * 1000.0);
                state.daemon_state.last_tick_changed_files = 0;
                state.daemon_state.last_tick_deleted_files = 0;
                state.daemon_state.last_tick_alerts_emitted = 0;
                state.persist_daemon_state()?;
                return Ok(json!({
                    "active": true,
                    "status": "deferred",
                    "deferred_reason": state.daemon_state.git_operation_kind,
                    "changed_files_detected": 0,
                    "deleted_files_detected": 0,
                    "files_reingested": 0,
                    "ingested_files": [],
                    "deleted_files": [],
                    "alerts_emitted": 0,
                    "alert_ids": [],
                }));
            }
            state.daemon_state.git_operation_in_progress = false;
            state.daemon_state.git_operation_kind = None;
            match git_changed_absolute_paths(
                Path::new(&root),
                state.daemon_state.git_since_ref.as_deref(),
            ) {
                Ok(paths) => {
                    state.daemon_state.last_git_scan_ms = Some(now_ms());
                    state.daemon_state.last_git_changed_files = paths.len();
                    state.daemon_state.git_backend_error = None;
                    for path in paths {
                        let path_str = path.to_string_lossy().to_string();
                        if let Some(entry) = live_inventory
                            .values()
                            .find(|entry| entry.file_path == path_str)
                            .cloned()
                        {
                            changed_entries.push(entry);
                        }
                    }
                    state.daemon_state.git_since_ref =
                        git_head_ref(Path::new(&root)).or(state.daemon_state.git_since_ref.clone());
                }
                Err(error) => {
                    state.daemon_state.git_backend_error = Some(error);
                    for (external_id, live_entry) in &live_inventory {
                        let changed = state
                            .daemon_state
                            .tracked_files
                            .get(external_id)
                            .is_none_or(|known| {
                                known.last_modified_ms != live_entry.last_modified_ms
                                    || known.size_bytes != live_entry.size_bytes
                                    || known.sha256 != live_entry.sha256
                            });
                        if changed {
                            changed_entries.push(live_entry.clone());
                        }
                    }
                }
            }
        } else {
            for (external_id, live_entry) in &live_inventory {
                let changed = state
                    .daemon_state
                    .tracked_files
                    .get(external_id)
                    .is_none_or(|known| {
                        known.last_modified_ms != live_entry.last_modified_ms
                            || known.size_bytes != live_entry.size_bytes
                            || known.sha256 != live_entry.sha256
                    });
                if changed {
                    changed_entries.push(live_entry.clone());
                }
            }
        }
    } else {
        for (external_id, live_entry) in &live_inventory {
            let changed = state
                .daemon_state
                .tracked_files
                .get(external_id)
                .is_none_or(|known| {
                    known.last_modified_ms != live_entry.last_modified_ms
                        || known.size_bytes != live_entry.size_bytes
                        || known.sha256 != live_entry.sha256
                });
            if changed {
                changed_entries.push(live_entry.clone());
            }
        }
    }

    for (external_id, known_entry) in &state.daemon_state.tracked_files {
        if !live_inventory.contains_key(external_id) {
            deleted_entries.push(FileInventoryEntry {
                external_id: known_entry.external_id.clone(),
                file_path: known_entry.file_path.clone(),
                size_bytes: known_entry.size_bytes,
                last_modified_ms: known_entry.last_modified_ms,
                language: extension_language(
                    Path::new(&known_entry.file_path)
                        .extension()
                        .and_then(|ext| ext.to_str()),
                ),
                commit_count: 0,
                loc: None,
                sha256: known_entry.sha256.clone(),
            });
        }
    }

    changed_entries.sort_by(|a, b| b.last_modified_ms.cmp(&a.last_modified_ms));
    changed_entries.truncate(input.max_files);

    let mut ingested_files = Vec::new();
    let mut heuristic_alerts_emitted = 0usize;
    for entry in &changed_entries {
        let ingest_result = crate::tools::handle_ingest(
            state,
            crate::protocol::IngestInput {
                path: entry.file_path.clone(),
                agent_id: input.agent_id.clone(),
                mode: "merge".into(),
                incremental: true,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )?;
        state.record_file_inventory([entry.clone()]);
        state.daemon_state.tracked_files.insert(
            entry.external_id.clone(),
            DaemonTrackedFile {
                external_id: entry.external_id.clone(),
                file_path: entry.file_path.clone(),
                last_modified_ms: entry.last_modified_ms,
                size_bytes: entry.size_bytes,
                sha256: entry.sha256.clone(),
            },
        );
        let proactive_insights = crate::surgical_handlers::daemon_proactive_insights_for_file(
            state,
            &entry.file_path,
            None,
        );
        heuristic_alerts_emitted += crate::surgical_handlers::persist_daemon_alerts_from_insights(
            state,
            &proactive_insights,
            Some(&entry.file_path),
            Some(&entry.external_id),
        );
        ingested_files.push(json!({
            "file_path": entry.file_path,
            "external_id": entry.external_id,
            "nodes_created": ingest_result.get("nodes_created").cloned().unwrap_or(json!(0)),
            "edges_created": ingest_result.get("edges_created").cloned().unwrap_or(json!(0)),
            "proactive_insight_kinds": proactive_insights.iter().map(|insight| insight.kind.clone()).collect::<Vec<_>>(),
        }));
    }

    let mut emitted_alert_ids = Vec::new();
    for entry in &deleted_entries {
        let alert = make_daemon_alert(DaemonAlertSeed {
            severity: "warning".into(),
            kind: "graph_vs_disk_drift".into(),
            message: format!(
                "Watched file disappeared from disk after ingest: {}",
                entry.file_path
            ),
            confidence: 0.86,
            evidence: vec![
                entry.external_id.clone(),
                entry.file_path.clone(),
                "daemon_tick detected file deletion under a watched root".into(),
            ],
            suggested_tool: Some("cross_verify".into()),
            suggested_target: Some(entry.file_path.clone()),
            file_path: Some(entry.file_path.clone()),
            node_id: Some(entry.external_id.clone()),
        });
        emitted_alert_ids.push(alert.alert_id.clone());
        state.record_daemon_alert(alert);
        state.daemon_state.tracked_files.remove(&entry.external_id);
        state.file_inventory.remove(&entry.external_id);
    }

    let tick_ms = now_ms();
    let emitted_alerts_total = emitted_alert_ids.len() + heuristic_alerts_emitted;
    state.daemon_state.last_tick_ms = Some(tick_ms);
    state.daemon_state.tick_count = state.daemon_state.tick_count.saturating_add(1);
    state.daemon_state.last_tick_duration_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
    state.daemon_state.last_tick_changed_files = changed_entries.len();
    state.daemon_state.last_tick_deleted_files = deleted_entries.len();
    state.daemon_state.last_tick_alerts_emitted = emitted_alerts_total;
    if changed_entries.is_empty() && deleted_entries.is_empty() && emitted_alerts_total == 0 {
        state.daemon_state.idle_streak = state.daemon_state.idle_streak.saturating_add(1);
    } else {
        state.daemon_state.idle_streak = 0;
    }
    state.persist_daemon_state()?;
    state.persist_daemon_alerts()?;

    Ok(json!({
        "active": true,
        "tick_at_ms": tick_ms,
        "watch_paths": state.daemon_state.watch_paths,
        "changed_files_detected": changed_entries.len(),
        "deleted_files_detected": deleted_entries.len(),
        "files_reingested": ingested_files.len(),
        "ingested_files": ingested_files,
        "deleted_files": deleted_entries.into_iter().map(|entry| json!({
            "file_path": entry.file_path,
            "external_id": entry.external_id,
        })).collect::<Vec<_>>(),
        "alerts_emitted": emitted_alerts_total,
        "alert_ids": emitted_alert_ids,
    }))
}

pub fn handle_alerts_list(
    state: &mut SessionState,
    input: layers::AlertsListInput,
) -> M1ndResult<serde_json::Value> {
    let mut alerts = state
        .daemon_alerts
        .iter()
        .filter(|alert| input.include_acked || !alert.acked)
        .cloned()
        .collect::<Vec<_>>();
    alerts.sort_by(|a, b| {
        b.created_at_ms
            .cmp(&a.created_at_ms)
            .then_with(|| a.alert_id.cmp(&b.alert_id))
    });
    alerts.truncate(input.limit);
    Ok(json!({
        "alerts": alerts,
        "total": alerts.len(),
        "active": state.daemon_state.active,
    }))
}

pub fn handle_alerts_ack(
    state: &mut SessionState,
    input: layers::AlertsAckInput,
) -> M1ndResult<serde_json::Value> {
    if input.alert_ids.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "alerts_ack".into(),
            detail: "Provide at least one alert_id.".into(),
        });
    }
    let acked_at_ms = now_ms();
    let mut acked = 0usize;
    for alert in &mut state.daemon_alerts {
        if input.alert_ids.iter().any(|id| id == &alert.alert_id) && !alert.acked {
            alert.acked = true;
            alert.acked_at_ms = Some(acked_at_ms);
            acked += 1;
        }
    }
    state.persist_daemon_alerts()?;
    Ok(json!({
        "acked": acked,
        "requested": input.alert_ids.len(),
        "acked_at_ms": acked_at_ms,
    }))
}

pub struct DaemonAlertSeed {
    pub severity: String,
    pub kind: String,
    pub message: String,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub suggested_tool: Option<String>,
    pub suggested_target: Option<String>,
    pub file_path: Option<String>,
    pub node_id: Option<String>,
}

pub fn make_daemon_alert(seed: DaemonAlertSeed) -> DaemonAlert {
    let created_at_ms = now_ms();
    DaemonAlert {
        alert_id: format!("alert-{}-{}", seed.kind, created_at_ms),
        severity: seed.severity,
        kind: seed.kind,
        message: seed.message,
        confidence: seed.confidence,
        evidence: seed.evidence,
        suggested_tool: seed.suggested_tool,
        suggested_target: seed.suggested_target,
        file_path: seed.file_path,
        node_id: seed.node_id,
        created_at_ms,
        acked: false,
        acked_at_ms: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpConfig;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use std::process::Command;

    fn build_state() -> (tempfile::TempDir, SessionState) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        (temp, state)
    }

    #[test]
    fn daemon_lifecycle_and_alert_ack_roundtrip() {
        let (_temp, mut state) = build_state();

        let started = handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec!["/tmp/watch".into()],
                poll_interval_ms: 750,
            },
        )
        .expect("daemon start");
        assert_eq!(started["active"], true);
        assert_eq!(started["poll_interval_ms"], 750);

        let seeded = make_daemon_alert(DaemonAlertSeed {
            severity: "warning".into(),
            kind: "trust_drop".into(),
            message: "trust dropped".into(),
            confidence: 0.82,
            evidence: vec!["file::src/core.py".into()],
            suggested_tool: Some("trust".into()),
            suggested_target: Some("file::src/core.py".into()),
            file_path: Some("/tmp/watch/src/core.py".into()),
            node_id: Some("file::src/core.py".into()),
        });
        let seeded_id = seeded.alert_id.clone();
        state.record_daemon_alert(seeded);
        state
            .persist_daemon_alerts()
            .expect("persist daemon alerts");

        let listed = handle_alerts_list(
            &mut state,
            layers::AlertsListInput {
                agent_id: "test".into(),
                include_acked: false,
                limit: 10,
            },
        )
        .expect("alerts list");
        assert_eq!(listed["total"], 1);
        assert_eq!(listed["alerts"][0]["alert_id"], seeded_id);

        let acked = handle_alerts_ack(
            &mut state,
            layers::AlertsAckInput {
                agent_id: "test".into(),
                alert_ids: vec![seeded_id.clone()],
            },
        )
        .expect("alerts ack");
        assert_eq!(acked["acked"], 1);

        let hidden = handle_alerts_list(
            &mut state,
            layers::AlertsListInput {
                agent_id: "test".into(),
                include_acked: false,
                limit: 10,
            },
        )
        .expect("alerts list hidden");
        assert_eq!(hidden["total"], 0);

        let visible = handle_alerts_list(
            &mut state,
            layers::AlertsListInput {
                agent_id: "test".into(),
                include_acked: true,
                limit: 10,
            },
        )
        .expect("alerts list visible");
        assert_eq!(visible["total"], 1);
        assert_eq!(visible["alerts"][0]["acked"], true);

        let status = handle_daemon_status(
            &mut state,
            layers::DaemonStatusInput {
                agent_id: "test".into(),
            },
        )
        .expect("daemon status");
        assert_eq!(status["active"], true);
        assert_eq!(status["alert_count"], 1);
        assert_eq!(status["tick_count"], 0);
        assert!(status["next_tick_due_ms"].as_u64().is_some());
        assert_eq!(status["overdue_ms"], 0);
        assert_eq!(status["idle_streak"], 0);
        assert_eq!(status["coalesce_window_ms"], 75);
        assert_eq!(status["pending_rerun"], false);
        assert_eq!(status["tick_in_flight"], false);
        assert_eq!(status["watch_backend"], "polling");
        assert_eq!(status["watch_events_seen"], 0);
        assert_eq!(status["watch_events_dropped"], 0);

        let stopped = handle_daemon_stop(
            &mut state,
            layers::DaemonStopInput {
                agent_id: "test".into(),
            },
        )
        .expect("daemon stop");
        assert_eq!(stopped["active"], false);
    }

    #[test]
    fn daemon_tick_reingests_changed_files() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file_path = repo.join("src/core.py");
        std::fs::write(&file_path, "def core():\n    return 1\n").expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "test".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("initial ingest");

        handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 500,
            },
        )
        .expect("daemon start");

        let noop = handle_daemon_tick(
            &mut state,
            layers::DaemonTickInput {
                agent_id: "test".into(),
                max_files: 8,
            },
        )
        .expect("noop tick");
        assert_eq!(noop["changed_files_detected"], 0);
        assert_eq!(noop["files_reingested"], 0);

        std::fs::write(&file_path, "def core():\n    return 2\n").expect("rewrite file");

        let ticked = handle_daemon_tick(
            &mut state,
            layers::DaemonTickInput {
                agent_id: "test".into(),
                max_files: 8,
            },
        )
        .expect("changed tick");
        assert_eq!(ticked["changed_files_detected"], 1);
        assert_eq!(ticked["files_reingested"], 1);
        assert_eq!(ticked["alerts_emitted"], 0);
        assert!(ticked["ingested_files"][0]["file_path"]
            .as_str()
            .is_some_and(|path| path.ends_with("src/core.py")));
        let status = handle_daemon_status(
            &mut state,
            layers::DaemonStatusInput {
                agent_id: "test".into(),
            },
        )
        .expect("daemon status after tick");
        assert_eq!(status["tick_count"], 2);
        assert_eq!(status["last_tick_changed_files"], 1);
        assert_eq!(status["last_tick_deleted_files"], 0);
        assert!(status["next_tick_due_ms"].as_u64().is_some());
        assert_eq!(status["idle_streak"], 0);
        assert_eq!(status["pending_rerun"], false);
        assert_eq!(status["tick_in_flight"], false);
        assert_eq!(status["watch_backend"], "polling");
    }

    #[test]
    fn daemon_tick_surfaces_proactive_alerts_for_risky_changed_file() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file_path = repo.join("src/core.py");
        std::fs::write(&file_path, "def core():\n    return 1\n").expect("write file");
        std::fs::write(
            repo.join("src/test_core.py"),
            "def test_core():\n    assert True\n",
        )
        .expect("write companion test");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "test".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("initial ingest");

        handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 500,
            },
        )
        .expect("daemon start");

        state
            .trust_ledger
            .record_defect(&format!("file::{}", file_path.to_string_lossy()), 100.0);
        state
            .trust_ledger
            .record_defect(&format!("file::{}", file_path.to_string_lossy()), 200.0);
        state.tremor_registry.record_observation(
            &format!("file::{}", file_path.to_string_lossy()),
            1.0,
            4,
            300.0,
        );
        state.tremor_registry.record_observation(
            &format!("file::{}", file_path.to_string_lossy()),
            1.1,
            4,
            400.0,
        );
        state.tremor_registry.record_observation(
            &format!("file::{}", file_path.to_string_lossy()),
            1.2,
            4,
            500.0,
        );

        std::fs::write(&file_path, "def core():\n    return 3\n").expect("rewrite file");

        let ticked = handle_daemon_tick(
            &mut state,
            layers::DaemonTickInput {
                agent_id: "test".into(),
                max_files: 8,
            },
        )
        .expect("risky changed tick");
        let kinds = ticked["ingested_files"][0]["proactive_insight_kinds"]
            .as_array()
            .expect("proactive insight kinds");
        assert!(
            kinds.iter().any(|value| {
                value.as_str() == Some("trust_drop")
                    || value.as_str() == Some("tremor_hotspot")
                    || value.as_str() == Some("untouched_test_companion")
            }),
            "daemon tick should surface the same proactive heuristics as write paths"
        );
        assert!(
            state.daemon_alerts.iter().any(|alert| {
                alert.kind == "trust_drop"
                    || alert.kind == "tremor_hotspot"
                    || alert.kind == "untouched_test_companion"
            }),
            "daemon tick should persist heuristic alerts for risky changed files"
        );
        let status = handle_daemon_status(
            &mut state,
            layers::DaemonStatusInput {
                agent_id: "test".into(),
            },
        )
        .expect("daemon status after risky tick");
        assert_eq!(status["last_tick_changed_files"], 1);
        assert!(
            status["last_tick_alerts_emitted"].as_u64().unwrap_or(0) >= 1,
            "risky daemon tick should emit at least one alert"
        );
        assert_eq!(status["idle_streak"], 0);
        assert_eq!(status["pending_rerun"], false);
        assert_eq!(status["tick_in_flight"], false);
        assert_eq!(status["watch_backend"], "polling");
    }

    #[test]
    fn daemon_tick_emits_drift_alert_for_deleted_file() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file_path = repo.join("src/core.py");
        std::fs::write(&file_path, "def core():\n    return 1\n").expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "test".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("initial ingest");

        handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 500,
            },
        )
        .expect("daemon start");

        std::fs::remove_file(&file_path).expect("remove file");

        let ticked = handle_daemon_tick(
            &mut state,
            layers::DaemonTickInput {
                agent_id: "test".into(),
                max_files: 8,
            },
        )
        .expect("deleted tick");
        assert_eq!(ticked["deleted_files_detected"], 1);
        assert_eq!(ticked["alerts_emitted"], 1);
        assert!(state
            .daemon_alerts
            .iter()
            .any(|alert| alert.kind == "graph_vs_disk_drift"));
        let status = handle_daemon_status(
            &mut state,
            layers::DaemonStatusInput {
                agent_id: "test".into(),
            },
        )
        .expect("daemon status after delete tick");
        assert_eq!(status["last_tick_deleted_files"], 1);
        assert_eq!(status["last_tick_alerts_emitted"], 1);
        assert!(status["last_tick_duration_ms"].as_f64().is_some());
        assert!(status["next_tick_due_ms"].as_u64().is_some());
        assert_eq!(status["idle_streak"], 0);
        assert_eq!(status["pending_rerun"], false);
        assert_eq!(status["tick_in_flight"], false);
        assert_eq!(status["watch_backend"], "polling");
    }

    #[test]
    fn daemon_start_detects_git_root_and_head() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(repo.join("src/core.py"), "def core():\n    return 1\n").expect("write");

        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo)
            .output()
            .expect("git email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo)
            .output()
            .expect("git name");
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .expect("git commit");

        let started = handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 200,
            },
        )
        .expect("daemon start");

        assert_eq!(started["watch_backend"], "git_native_fs");
        assert!(started["git_root"].as_str().is_some());
        assert!(started["git_since_ref"].as_str().is_some());
    }

    #[test]
    fn daemon_tick_uses_git_changed_set_when_available() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file_path = repo.join("src/core.py");
        std::fs::write(&file_path, "def core():\n    return 1\n").expect("write");

        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo)
            .output()
            .expect("git email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo)
            .output()
            .expect("git name");
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .expect("git commit");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "test".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("initial ingest");

        handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 200,
            },
        )
        .expect("daemon start");

        std::fs::write(&file_path, "def core():\n    return 2\n").expect("rewrite");

        let ticked = handle_daemon_tick(
            &mut state,
            layers::DaemonTickInput {
                agent_id: "test".into(),
                max_files: 8,
            },
        )
        .expect("git tick");

        assert_eq!(state.daemon_state.watch_backend, "git_native_fs");
        assert_eq!(ticked["changed_files_detected"], 1);
        assert_eq!(ticked["files_reingested"], 1);
        assert_eq!(state.daemon_state.last_git_changed_files, 1);
        assert!(state.daemon_state.last_git_scan_ms.is_some());
        assert!(state.daemon_state.git_backend_error.is_none());
    }

    #[test]
    fn daemon_tick_defers_when_git_operation_is_in_progress() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file_path = repo.join("src/core.py");
        std::fs::write(&file_path, "def core():\n    return 1\n").expect("write");

        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo)
            .output()
            .expect("git email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo)
            .output()
            .expect("git name");
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .expect("git commit");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "test".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("initial ingest");

        handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 200,
            },
        )
        .expect("daemon start");

        std::fs::write(repo.join(".git").join("MERGE_HEAD"), "deadbeef\n").expect("merge head");

        let ticked = handle_daemon_tick(
            &mut state,
            layers::DaemonTickInput {
                agent_id: "test".into(),
                max_files: 8,
            },
        )
        .expect("deferred tick");

        assert_eq!(state.daemon_state.watch_backend, "git_native_fs");
        assert_eq!(ticked["status"], "deferred");
        assert_eq!(ticked["files_reingested"], 0);
        assert_eq!(state.daemon_state.git_operation_in_progress, true);
        assert_eq!(
            state.daemon_state.git_operation_kind.as_deref(),
            Some("merge")
        );
        assert!(state.daemon_state.deferred_ticks >= 1);
    }
}
