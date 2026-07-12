use crate::protocol::layers;
use crate::scope::normalize_path_text;
use crate::session::{DaemonAlert, DaemonTrackedFile, FileInventoryEntry, SessionState};
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde_json::json;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Gardener v1 — burst coalescing numbers (verdict item 7), REGISTERED with
/// their justification so the choice is auditable:
///
/// * `BURST_COALESCE_WINDOW_MS = 500`. The stdio event loop closes a watch-event
///   burst after this much SILENCE (sliding window). The old 75 ms fragmented a
///   branch checkout into several ticks: git writes files in dense sub-ms
///   batches but pauses for index/lock/pack work in the low hundreds of ms, so
///   75 ms of silence regularly fired mid-checkout. 500 ms spans those pauses —
///   thousands of events become ONE detection — while adding at most half a
///   second of latency to a single-file save (invisible for a background vigil
///   whose poll intervals are measured in seconds).
/// * `BURST_COALESCE_CAP_MS = 5_000`. A sliding silence window alone can starve
///   under CONTINUOUS churn (events forever < window apart). The cap bounds one
///   coalescing pass: after 5 s of sustained events the tick runs anyway, so the
///   graph keeps advancing during a storm instead of waiting for a quiet that
///   never comes.
pub const BURST_COALESCE_WINDOW_MS: u64 = 500;
pub const BURST_COALESCE_CAP_MS: u64 = 5_000;

/// Gardener v1 — the auto-reconcile QUIET WINDOW (verdict item 5), REGISTERED:
/// 45 s sits inside the verdict's 30–60 s band. Long enough that one logical
/// burst — a checkout plus the seconds of follow-up churn (build artifacts,
/// editor saves, hook output) — collapses into ONE window (every activity tick
/// PUSHES the deadline, so a storm coalesces instead of firing per wave); short
/// enough that the block map refreshes within a minute of the repo going quiet.
pub const AUTO_RECONCILE_QUIET_WINDOW_MS: u64 = 45_000;

/// Outcome of the 1-retry OCC policy around an auto-reconcile write.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OccOutcome {
    /// The reconcile applied (dirty = something actually moved).
    Ran { dirty: bool },
    /// Two attempts, two `Conflict`s — give up and ALERT, never loop
    /// (verdict item 5: "1 retry → alert, nunca loop").
    ConflictExhausted,
    /// A non-OCC failure (no store, io, ...): fail-open, log-and-drop.
    Failed(String),
}

/// The 1-retry OCC policy (verdict item 5). Each attempt must read its OWN
/// fresh `expected_store_version` (the closure owns that); a `Conflict` earns
/// exactly ONE more attempt, anything else settles immediately. Pure policy —
/// unit-testable with an injected attempt, which is the only deterministic way
/// to exercise the conflict arm (a real conflict needs a concurrent writer).
pub(crate) fn occ_retry_outcome(
    mut attempt: impl FnMut() -> Result<bool, crate::system_blocks::SeedError>,
) -> OccOutcome {
    for _ in 0..2 {
        match attempt() {
            Ok(dirty) => return OccOutcome::Ran { dirty },
            Err(crate::system_blocks::SeedError::Conflict { .. }) => continue,
            Err(other) => return OccOutcome::Failed(other.to_string()),
        }
    }
    OccOutcome::ConflictExhausted
}

/// Settle an auto-reconcile OCC outcome onto the session: bump the honest
/// counters on success, record the conflict ALERT on exhaustion (the existing
/// alert lane — no new surface), log-and-drop a plain failure. Returns the
/// tick-output label plus the alert id (so the tick's totals stay truthful).
fn settle_auto_reconcile_outcome(
    state: &mut SessionState,
    outcome: OccOutcome,
) -> (&'static str, Option<String>) {
    match outcome {
        OccOutcome::Ran { dirty } => {
            state.daemon_state.last_auto_reconcile_ms = Some(now_ms());
            state.daemon_state.auto_reconcile_runs =
                state.daemon_state.auto_reconcile_runs.saturating_add(1);
            (if dirty { "ran_dirty" } else { "ran_clean" }, None)
        }
        OccOutcome::ConflictExhausted => {
            let alert = make_daemon_alert(DaemonAlertSeed {
                severity: "warning".into(),
                kind: "auto_reconcile_conflict".into(),
                message: "Auto-reconcile hit an OCC conflict twice in a row — \
                          another writer is active on the system-blocks store. \
                          Reconcile manually when the store settles."
                    .into(),
                confidence: 0.9,
                evidence: vec![
                    "daemon auto-reconcile (quiet window) — 1 OCC retry exhausted".into(),
                ],
                suggested_tool: Some("system_blocks_reconcile".into()),
                suggested_target: None,
                file_path: None,
                node_id: None,
            });
            let id = alert.alert_id.clone();
            state.record_daemon_alert(alert);
            ("conflict_alert", Some(id))
        }
        OccOutcome::Failed(error) => {
            eprintln!("[m1nd] gardener: auto-reconcile failed (fail-open): {error}");
            ("reconcile_error", None)
        }
    }
}

/// GARDENER v1 — one auto-reconcile pass (verdict item 5), called from a QUIET
/// tick whose deadline elapsed. Fail-open by construction: every branch returns
/// a label, never an error. The laws, in order:
/// 1. no store → nothing owed;
/// 2. reconcile refreshes the RATIFIED store — a candidate skeleton is another
///    cycle (candidate freshness re-scan), outside this arc: skip;
/// 3. a LIVE candidate_lease → VOLUNTARY YIELD + reschedule (the lease is
///    advisory by ratified law — it cannot block us, so WE cede);
/// 4. fresh `expected_store_version` per attempt, 1 OCC retry, then alert.
fn run_auto_reconcile(state: &mut SessionState) -> (&'static str, Option<String>) {
    use crate::system_blocks::{self, SeedSkeletonState, SystemBlockStore};
    let dir = crate::system_blocks_handlers::store_dir(state);
    let store = match SystemBlockStore::load(&dir) {
        Ok(Some(store)) => store,
        Ok(None) => {
            state.daemon_state.reconcile_due_at_ms = None;
            return ("no_store", None);
        }
        Err(error) => {
            eprintln!("[m1nd] gardener: auto-reconcile store load failed (fail-open): {error}");
            state.daemon_state.reconcile_due_at_ms = None;
            return ("store_load_error", None);
        }
    };
    if store.skeleton.state != SeedSkeletonState::Ratified {
        state.daemon_state.reconcile_due_at_ms = None;
        return ("skeleton_not_ratified", None);
    }
    let now = now_ms();
    let now_iso = crate::system_blocks_handlers::iso8601_from_ms(now);
    if store.lease_is_live(&now_iso) {
        state.daemon_state.reconcile_due_at_ms =
            Some(now.saturating_add(AUTO_RECONCILE_QUIET_WINDOW_MS));
        return ("yielded_to_lease", None);
    }
    let Some(root) = state.code_root_path() else {
        state.daemon_state.reconcile_due_at_ms = None;
        return ("no_code_root", None);
    };
    let file_list = match system_blocks::repo_file_list(Path::new(&root)) {
        Ok(list) => list,
        Err(error) => {
            eprintln!("[m1nd] gardener: auto-reconcile file list failed (fail-open): {error}");
            state.daemon_state.reconcile_due_at_ms = None;
            return ("file_list_error", None);
        }
    };
    let outcome = occ_retry_outcome(|| {
        // FRESH expected_store_version per attempt (verdict item 5).
        let fresh = SystemBlockStore::load(&dir)
            .ok()
            .flatten()
            .map(|s| s.store_version)
            .unwrap_or(0);
        system_blocks::reconcile_in_dir(&dir, fresh, &file_list).map(|(_, report)| report.dirty)
    });
    state.daemon_state.reconcile_due_at_ms = None;
    settle_auto_reconcile_outcome(state, outcome)
}

fn simple_content_hash(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

fn join_repo_relative(root: &Path, rel: &str) -> PathBuf {
    normalize_path_text(rel)
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .fold(root.to_path_buf(), |mut acc, part| {
            acc.push(part);
            acc
        })
}

fn same_path_text(left: &str, right: &str) -> bool {
    let left = normalize_path_text(left);
    let right = normalize_path_text(right);
    if left == right {
        return true;
    }
    let left_canonical = Path::new(&left)
        .canonicalize()
        .ok()
        .map(|path| normalize_path_text(&path.to_string_lossy()));
    let right_canonical = Path::new(&right)
        .canonicalize()
        .ok()
        .map(|path| normalize_path_text(&path.to_string_lossy()));
    if let (Some(left), Some(right)) = (left_canonical, right_canonical) {
        #[cfg(windows)]
        {
            return left.eq_ignore_ascii_case(&right);
        }
        #[cfg(not(windows))]
        {
            return left == right;
        }
    }
    #[cfg(windows)]
    {
        left.eq_ignore_ascii_case(&right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
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

fn git_upstream_ref(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ])
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

fn git_merge_base(root: &Path, lhs: &str, rhs: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["merge-base", lhs, rhs])
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

fn git_initial_baseline(root: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let head = git_head_ref(root);
    let upstream = git_upstream_ref(root);
    if let (Some(head_ref), Some(upstream_ref)) = (head.as_deref(), upstream.as_deref()) {
        if let Some(merge_base) = git_merge_base(root, head_ref, upstream_ref) {
            return (Some(merge_base), Some("merge_base".to_string()), upstream);
        }
    }

    (head, Some("head".to_string()), upstream)
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
        changed.push(join_repo_relative(root, &rel));
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
    state.daemon_state.coalesce_window_ms = BURST_COALESCE_WINDOW_MS;
    state.daemon_state.pending_rerun = false;
    state.daemon_state.tick_in_flight = false;
    state.daemon_state.pending_backlog = Vec::new();
    state.daemon_state.reconcile_due_at_ms = None;
    state.daemon_state.last_auto_reconcile_ms = None;
    state.daemon_state.auto_reconcile_runs = 0;
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
    let (git_baseline_ref, git_baseline_kind, _git_upstream_ref) = state
        .daemon_state
        .git_root
        .as_deref()
        .map(|root| git_initial_baseline(Path::new(root)))
        .unwrap_or((None, None, None));
    let git_head_ref = state
        .daemon_state
        .git_root
        .as_deref()
        .and_then(|root| git_head_ref(Path::new(root)));
    state.daemon_state.git_baseline_ref = git_baseline_ref.clone();
    state.daemon_state.git_baseline_kind = git_baseline_kind;
    state.daemon_state.git_since_ref = git_baseline_ref.clone();
    state.daemon_state.git_head_ref = git_head_ref;
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
        "git_baseline_ref": state.daemon_state.git_baseline_ref,
        "git_baseline_kind": state.daemon_state.git_baseline_kind,
        "git_since_ref": state.daemon_state.git_since_ref,
        "git_head_ref": state.daemon_state.git_head_ref,
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
        "git_baseline_ref": state.daemon_state.git_baseline_ref,
        "git_baseline_kind": state.daemon_state.git_baseline_kind,
        "git_since_ref": state.daemon_state.git_since_ref,
        "git_head_ref": state.daemon_state.git_head_ref,
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
        "pending_backlog_len": state.daemon_state.pending_backlog.len(),
        "reconcile_due_at_ms": state.daemon_state.reconcile_due_at_ms,
        "last_auto_reconcile_ms": state.daemon_state.last_auto_reconcile_ms,
        "auto_reconcile_runs": state.daemon_state.auto_reconcile_runs,
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
                    let current_head = git_head_ref(Path::new(&root));
                    state.daemon_state.last_git_scan_ms = Some(now_ms());
                    state.daemon_state.last_git_changed_files = paths.len();
                    state.daemon_state.git_backend_error = None;
                    for path in paths {
                        let path_str = path.to_string_lossy().to_string();
                        if let Some(entry) = live_inventory
                            .values()
                            .find(|entry| same_path_text(&entry.file_path, &path_str))
                            .cloned()
                        {
                            changed_entries.push(entry);
                        }
                    }
                    state.daemon_state.git_head_ref = current_head.clone();
                    state.daemon_state.git_since_ref =
                        current_head.or(state.daemon_state.git_since_ref.clone());
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

    changed_entries.sort_by_key(|entry| std::cmp::Reverse(entry.last_modified_ms));
    let newly_detected = changed_entries.len();

    // BURST BACKLOG (gardener v1, verdict item 7). The old shape truncated the
    // changed set to `max_files` AND (on the git backend) advanced
    // `git_since_ref` past the whole diff — a thousand-file checkout re-ingested
    // 32 files and silently LOST the tail forever. Now: every detected id is
    // pushed onto the persisted backlog (dedup — the polling backend re-detects
    // un-ingested files every tick), and the tick drains a bounded slice from
    // the FRONT (FIFO: completeness, no starvation; a single burst lands in one
    // detection anyway, newest-first within the batch). Each drained id is
    // resolved against the LIVE inventory so the ingest always reads fresh
    // content; an id that vanished from disk simply drops (the deletion lane
    // below owns the alert for tracked files).
    for entry in &changed_entries {
        if !state
            .daemon_state
            .pending_backlog
            .iter()
            .any(|id| id == &entry.external_id)
        {
            state
                .daemon_state
                .pending_backlog
                .push(entry.external_id.clone());
        }
    }
    let backlog = std::mem::take(&mut state.daemon_state.pending_backlog);
    let mut drained_entries: Vec<FileInventoryEntry> = Vec::new();
    let mut kept_backlog: Vec<String> = Vec::new();
    for id in backlog {
        if drained_entries.len() >= input.max_files {
            kept_backlog.push(id);
            continue;
        }
        if let Some(entry) = live_inventory.get(&id) {
            drained_entries.push(entry.clone());
        }
    }
    state.daemon_state.pending_backlog = kept_backlog;
    let changed_entries = drained_entries;

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
                project_root: None,
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

    // GARDENER v1 — auto-reconcile scheduling (verdict item 5). Every tick with
    // ACTIVITY pushes the quiet-window deadline out (one window per burst — a
    // checkout's thousands of events coalesce into ONE deadline, never one per
    // wave). A QUIET tick — nothing new, nothing drained, backlog empty — whose
    // deadline elapsed runs the reconcile: refresh the RATIFIED store against
    // the real file list, yielding voluntarily to a live candidate_lease and
    // giving OCC exactly one retry before alerting. Fail-open throughout: no
    // arm of this block can fail the tick.
    let mut auto_reconcile_outcome: Option<&'static str> = None;
    {
        let now = now_ms();
        let had_activity =
            newly_detected > 0 || !changed_entries.is_empty() || !deleted_entries.is_empty();
        if had_activity {
            state.daemon_state.reconcile_due_at_ms =
                Some(now.saturating_add(AUTO_RECONCILE_QUIET_WINDOW_MS));
        } else if state.daemon_state.pending_backlog.is_empty()
            && state
                .daemon_state
                .reconcile_due_at_ms
                .is_some_and(|due| now >= due)
        {
            let (label, alert_id) = run_auto_reconcile(state);
            if let Some(id) = alert_id {
                emitted_alert_ids.push(id);
            }
            auto_reconcile_outcome = Some(label);
        }
    }

    let tick_ms = now_ms();
    let emitted_alerts_total = emitted_alert_ids.len() + heuristic_alerts_emitted;
    state.daemon_state.last_tick_ms = Some(tick_ms);
    state.daemon_state.tick_count = state.daemon_state.tick_count.saturating_add(1);

    // Periodically garbage-collect dead lease/instance entries so crashed
    // instances don't leak registry files forever. Cheap (a directory scan +
    // `kill -0` per entry) and only removes provably-dead entries, so it is
    // safe to run alongside live instances. Throttled to every Nth tick.
    const GC_EVERY_N_TICKS: u64 = 50;
    if state
        .daemon_state
        .tick_count
        .is_multiple_of(GC_EVERY_N_TICKS)
    {
        let registry_root = state.instance.registry_root();
        let _ = crate::instance_registry::gc_dead_leases(&registry_root);
    }
    state.daemon_state.last_tick_duration_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
    state.daemon_state.last_tick_changed_files = changed_entries.len();
    state.daemon_state.last_tick_deleted_files = deleted_entries.len();
    state.daemon_state.last_tick_alerts_emitted = emitted_alerts_total;
    // A tick with a non-empty remaining backlog is NOT idle — drain work remains,
    // and backing off would stretch the burst's tail out for no reason.
    if changed_entries.is_empty()
        && deleted_entries.is_empty()
        && emitted_alerts_total == 0
        && state.daemon_state.pending_backlog.is_empty()
    {
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
        "changed_files_detected": newly_detected,
        "deleted_files_detected": deleted_entries.len(),
        "files_reingested": ingested_files.len(),
        "backlog_len": state.daemon_state.pending_backlog.len(),
        "auto_reconcile": auto_reconcile_outcome,
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
                poll_interval_ms: 60_000,
            },
        )
        .expect("daemon start");
        assert_eq!(started["active"], true);
        assert_eq!(started["poll_interval_ms"], 60_000);

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
        assert_eq!(status["coalesce_window_ms"], BURST_COALESCE_WINDOW_MS);
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

    /// HTTP status honesty (gardener v1, verdict item 4): a persisted
    /// `watch_backend: "native_fs"` asserts a LIVE notify watcher, which only the
    /// stdio serve() loop owns. A freshly booted state (any transport; on the HTTP
    /// owner, forever) has no such consumer — resuming the label verbatim would
    /// make `daemon_status` LIE. RED without the load-time downgrade to "polling".
    /// `git_native_fs` must survive: it names per-tick git-diff detection, true on
    /// every transport.
    #[test]
    fn resumed_status_never_claims_a_dead_notify_watcher() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        // The exact disk shape a stdio owner leaves behind: armed, live-watcher
        // label, and the mid-tick reentrancy flags.
        let persisted = crate::session::DaemonRuntimeState {
            active: true,
            watch_backend: "native_fs".into(),
            tick_in_flight: true,
            pending_rerun: true,
            ..Default::default()
        };
        std::fs::write(
            runtime_dir.join("daemon_state.json"),
            serde_json::to_string_pretty(&persisted).expect("serialize"),
        )
        .expect("write daemon state");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir.clone()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        let status = handle_daemon_status(
            &mut state,
            layers::DaemonStatusInput {
                agent_id: "test".into(),
            },
        )
        .expect("daemon status");
        assert_eq!(status["active"], true, "the armed daemon resumes active");
        assert_eq!(
            status["watch_backend"], "polling",
            "a resumed status must not claim a notify watcher that no longer exists"
        );
        assert_eq!(status["tick_in_flight"], false);
        assert_eq!(status["pending_rerun"], false);

        // The honest label that DOES survive: per-tick git detection.
        let persisted_git = crate::session::DaemonRuntimeState {
            active: true,
            watch_backend: "git_native_fs".into(),
            git_root: Some("/tmp/somewhere".into()),
            ..Default::default()
        };
        std::fs::write(
            runtime_dir.join("daemon_state.json"),
            serde_json::to_string_pretty(&persisted_git).expect("serialize"),
        )
        .expect("write daemon state");
        let state2 = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session 2");
        assert_eq!(
            state2.daemon_state.watch_backend, "git_native_fs",
            "git_native_fs names per-tick detection and survives a resume"
        );
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
                project_root: None,
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
        // Ingest now populates node provenance (source_path/line), which revives the
        // surgical proactive-insight engine: `find_nodes_for_file` keys file lookup
        // on `source_path`, so while nodes had no provenance it always returned
        // empty and the daemon surfaced ZERO insights. With provenance populated, a
        // re-ingested change now legitimately surfaces co-change insights, each
        // emitted as an alert.
        let alerts = ticked["alerts_emitted"]
            .as_u64()
            .expect("alerts_emitted is a number");
        let kinds = ticked["ingested_files"][0]["proactive_insight_kinds"]
            .as_array()
            .expect("proactive_insight_kinds array");
        assert!(
            alerts >= 1,
            "proactive-insight engine should surface an insight"
        );
        assert!(
            kinds.iter().any(|k| k == "co_change_prediction"),
            "expected a co-change insight, got {kinds:?}"
        );
        assert!(ticked["ingested_files"][0]["file_path"]
            .as_str()
            .map(normalize_path_text)
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
                project_root: None,
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
                project_root: None,
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

    /// A complete ratified seed (copied from the system_blocks fixture — the
    /// validated shape) so daemon tests can stage a real store in the brain dir.
    fn gardener_fixture_seed() -> &'static str {
        r#"{
  "schema": "m1nd-system-block-seed-v0",
  "repo": { "repo_id": "repo_a", "root": ".", "source_commit": "abc123" },
  "skeleton": {
    "skeleton_id": "sk_repo_a_seed_2026_07",
    "version": 1,
    "state": "ratified",
    "ratification": {
      "method": "pr_merge",
      "ratifier": "owner",
      "ratified_at": "2026-07-07T00:00:00Z",
      "commit": "abc123"
    }
  },
  "blocks": [
    {
      "block_id": "sb_core",
      "name": "Core",
      "purpose": "Core graph responsibilities.",
      "kind": "scanned",
      "state": "ratified",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "ratified",
      "membership": [
        { "path": "src/**", "role": "primary" }
      ],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": {
        "version": 1,
        "required": [],
        "optional": [],
        "waived": [],
        "declared_by": "owner",
        "declared_at": "2026-07-07T00:00:00Z"
      },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    }
  ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}"#
    }

    /// Stage a real system-blocks store (ratified skeleton) in the brain's
    /// runtime dir — the store the auto-reconcile refreshes.
    fn stage_ratified_store(state: &SessionState) -> crate::system_blocks::SystemBlockStore {
        let seed =
            crate::system_blocks::load_seed(gardener_fixture_seed()).expect("fixture seed parses");
        let store = crate::system_blocks::SystemBlockStore::from_seed(seed);
        store
            .save(&crate::system_blocks_handlers::store_dir(state))
            .expect("save staged store");
        store
    }

    /// Build a state + watched non-git repo + armed daemon (polling backend),
    /// the common auto-reconcile test stage.
    fn build_reconcile_stage() -> (tempfile::TempDir, SessionState, PathBuf) {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(repo.join("src/a.py"), "def a():\n    return 1\n").expect("write a.py");
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
                project_root: None,
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
        (temp, state, repo)
    }

    fn tick(state: &mut SessionState) -> serde_json::Value {
        handle_daemon_tick(
            state,
            layers::DaemonTickInput {
                agent_id: "test".into(),
                max_files: 8,
            },
        )
        .expect("tick")
    }

    /// AUTO-RECONCILE (gardener v1, verdict item 5) — the quiet window COALESCES
    /// a burst (activity pushes the deadline; an elapsed deadline NEVER fires on
    /// an activity tick) and a quiet tick past the deadline reconciles the
    /// ratified store with a fresh OCC key.
    #[test]
    fn quiet_window_coalesces_bursts_then_reconciles() {
        let (_temp, mut state, repo) = build_reconcile_stage();
        stage_ratified_store(&state);
        let dir = crate::system_blocks_handlers::store_dir(&state);

        // Quiet tick, nothing owed: no deadline, no reconcile.
        let t1 = tick(&mut state);
        assert!(t1["auto_reconcile"].is_null());
        assert!(state.daemon_state.reconcile_due_at_ms.is_none());

        // Activity: a new file → the deadline is SET, the reconcile does NOT run.
        std::fs::write(repo.join("src/b.py"), "def b():\n    return 2\n").expect("write b.py");
        let t2 = tick(&mut state);
        assert_eq!(t2["files_reingested"], 1);
        assert!(t2["auto_reconcile"].is_null(), "no reconcile mid-burst");
        let due_after_first = state
            .daemon_state
            .reconcile_due_at_ms
            .expect("activity schedules the quiet window");

        // COALESCING LAW: even an ELAPSED deadline never fires on an activity
        // tick — the burst pushes the window out instead (one window per burst).
        state.daemon_state.reconcile_due_at_ms = Some(1);
        std::fs::write(repo.join("src/c.py"), "def c():\n    return 3\n").expect("write c.py");
        let t3 = tick(&mut state);
        assert!(
            t3["auto_reconcile"].is_null(),
            "an activity tick must push the deadline, never reconcile"
        );
        let due_after_second = state
            .daemon_state
            .reconcile_due_at_ms
            .expect("the deadline was pushed, not consumed");
        assert!(
            due_after_second >= due_after_first,
            "the pushed deadline moves forward"
        );
        assert_eq!(state.daemon_state.auto_reconcile_runs, 0);

        // QUIET + elapsed deadline → the reconcile RUNS with a fresh OCC key.
        let version_before = crate::system_blocks::SystemBlockStore::load(&dir)
            .expect("load")
            .expect("store")
            .store_version;
        state.daemon_state.reconcile_due_at_ms = Some(1);
        let t4 = tick(&mut state);
        assert_eq!(
            t4["auto_reconcile"], "ran_dirty",
            "the first reconcile records baselines — a real write"
        );
        assert_eq!(state.daemon_state.auto_reconcile_runs, 1);
        assert!(state.daemon_state.last_auto_reconcile_ms.is_some());
        assert!(
            state.daemon_state.reconcile_due_at_ms.is_none(),
            "a settled deadline clears"
        );
        let store_after = crate::system_blocks::SystemBlockStore::load(&dir)
            .expect("load")
            .expect("store");
        assert!(
            store_after.store_version > version_before,
            "the ratified store was refreshed (OCC bump)"
        );
    }

    /// LEASE YIELD (gardener v1, verdict item 5): the candidate_lease is advisory
    /// by ratified law — it cannot block anyone. The auto-reconciler CEDES
    /// voluntarily: a live lease → skip + reschedule; a released lease → run.
    #[test]
    fn auto_reconcile_yields_to_a_live_lease_and_reschedules() {
        use crate::system_blocks::{LeaseAction, SystemBlockStore};
        let (_temp, mut state, _repo) = build_reconcile_stage();
        stage_ratified_store(&state);
        let dir = crate::system_blocks_handlers::store_dir(&state);

        // A hand holds a LIVE lease.
        let now = now_ms();
        let now_iso = crate::system_blocks_handlers::iso8601_from_ms(now);
        let until_iso = crate::system_blocks_handlers::iso8601_from_ms(now + 900_000);
        let mut store = SystemBlockStore::load(&dir).expect("load").expect("store");
        store
            .apply_lease(LeaseAction::Acquire, "hand-1", &now_iso, &until_iso)
            .expect("acquire lease");
        store.save(&dir).expect("save leased store");

        // Elapsed deadline + quiet tick → VOLUNTARY YIELD + reschedule.
        state.daemon_state.reconcile_due_at_ms = Some(1);
        let t = tick(&mut state);
        assert_eq!(t["auto_reconcile"], "yielded_to_lease");
        assert_eq!(state.daemon_state.auto_reconcile_runs, 0, "nothing ran");
        let rescheduled = state
            .daemon_state
            .reconcile_due_at_ms
            .expect("the yield RESCHEDULES, never drops the debt");
        assert!(rescheduled > now, "rescheduled into the future");
        let untouched = SystemBlockStore::load(&dir).expect("load").expect("store");
        assert_eq!(untouched.store_version, 1, "the leased store was not touched");

        // The hand releases → the next elapsed quiet tick RUNS.
        let mut store = SystemBlockStore::load(&dir).expect("load").expect("store");
        store
            .apply_lease(LeaseAction::Release, "hand-1", &now_iso, &until_iso)
            .expect("release lease");
        store.save(&dir).expect("save released store");
        state.daemon_state.reconcile_due_at_ms = Some(1);
        let t = tick(&mut state);
        assert_eq!(t["auto_reconcile"], "ran_dirty");
        assert_eq!(state.daemon_state.auto_reconcile_runs, 1);
    }

    /// RATIFIED-ONLY LAW (gardener v1, verdict item 5): reconcile refreshes the
    /// RATIFIED store; candidate freshness is ANOTHER cycle (the candidate
    /// re-scan), outside this arc — a candidate skeleton is skipped, debt cleared.
    #[test]
    fn auto_reconcile_skips_a_candidate_skeleton() {
        use crate::system_blocks::{SeedSkeletonState, SystemBlockStore};
        let (_temp, mut state, _repo) = build_reconcile_stage();
        let mut store = stage_ratified_store(&state);
        let dir = crate::system_blocks_handlers::store_dir(&state);
        store.skeleton.state = SeedSkeletonState::Candidate;
        store.save(&dir).expect("save candidate store");

        state.daemon_state.reconcile_due_at_ms = Some(1);
        let t = tick(&mut state);
        assert_eq!(t["auto_reconcile"], "skeleton_not_ratified");
        assert_eq!(state.daemon_state.auto_reconcile_runs, 0);
        assert!(
            state.daemon_state.reconcile_due_at_ms.is_none(),
            "a candidate skeleton clears the debt — its freshness is another cycle"
        );
        let untouched = SystemBlockStore::load(&dir).expect("load").expect("store");
        assert_eq!(untouched.store_version, 1);
    }

    /// OCC RETRY POLICY (gardener v1, verdict item 5): a Conflict earns exactly
    /// ONE retry, then the alert — never a loop. The conflict arm is exercised
    /// with an injected attempt (the only deterministic way: a real conflict
    /// needs a concurrent writer), and the exhausted outcome must record the
    /// alert on the existing daemon-alert lane.
    #[test]
    fn occ_conflict_gets_one_retry_then_an_alert_never_a_loop() {
        use crate::system_blocks::SeedError;

        // Conflict, conflict → exhausted after EXACTLY two attempts.
        let mut calls = 0;
        let out = occ_retry_outcome(|| {
            calls += 1;
            Err(SeedError::Conflict {
                expected: 1,
                actual: 2,
            })
        });
        assert_eq!(out, OccOutcome::ConflictExhausted);
        assert_eq!(calls, 2, "1 retry, never a loop");

        // Conflict, then success → ran on the retry.
        let mut calls = 0;
        let out = occ_retry_outcome(|| {
            calls += 1;
            if calls == 1 {
                Err(SeedError::Conflict {
                    expected: 1,
                    actual: 2,
                })
            } else {
                Ok(false)
            }
        });
        assert_eq!(out, OccOutcome::Ran { dirty: false });
        assert_eq!(calls, 2);

        // Immediate success → one attempt.
        let mut calls = 0;
        let out = occ_retry_outcome(|| {
            calls += 1;
            Ok(true)
        });
        assert_eq!(out, OccOutcome::Ran { dirty: true });
        assert_eq!(calls, 1);

        // A non-OCC failure settles immediately (fail-open, no retry).
        let mut calls = 0;
        let out = occ_retry_outcome(|| {
            calls += 1;
            Err(SeedError::NoStore)
        });
        assert!(matches!(out, OccOutcome::Failed(_)));
        assert_eq!(calls, 1);

        // Exhaustion RECORDS the alert on the existing lane, with the tick label.
        let (_temp, mut state) = build_state();
        let (label, alert_id) =
            settle_auto_reconcile_outcome(&mut state, OccOutcome::ConflictExhausted);
        assert_eq!(label, "conflict_alert");
        let alert_id = alert_id.expect("the conflict alert id rides the tick totals");
        assert!(state
            .daemon_alerts
            .iter()
            .any(|a| a.alert_id == alert_id && a.kind == "auto_reconcile_conflict"));
    }

    /// BURST COALESCING (gardener v1, verdict item 7) — the checkout shape on the
    /// git backend. A burst bigger than one tick's `max_files` budget used to be
    /// truncated while `git_since_ref` advanced past the WHOLE diff: the tail was
    /// silently lost forever (20-file burst, budget 6 → 6 ingested, 14 never).
    /// Now the burst is ONE detection that fills the persisted backlog, and
    /// bounded drain ticks re-ingest every file. RED under the old truncate:
    /// ticks after the first find changed_files 0 and the total stalls at 6.
    #[test]
    fn burst_bigger_than_tick_budget_drains_completely_without_losing_the_tail() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        const N: usize = 20;
        const BUDGET: usize = 6;
        for i in 0..N {
            std::fs::write(
                repo.join(format!("src/f{i:02}.py")),
                format!("def f{i:02}():\n    return {i}\n"),
            )
            .expect("write file");
        }

        let git = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .expect("spawn git");
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["add", "."]);
        git(&["commit", "-m", "init"]);

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
                project_root: None,
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
        assert_eq!(state.daemon_state.watch_backend, "git_native_fs");

        // THE BURST: every file changes and the head MOVES (the checkout shape —
        // this is exactly what advanced since_ref past the tail before).
        for i in 0..N {
            std::fs::write(
                repo.join(format!("src/f{i:02}.py")),
                format!("def f{i:02}():\n    return {i} + 100\n"),
            )
            .expect("rewrite file");
        }
        git(&["add", "."]);
        git(&["commit", "-m", "burst"]);

        // Tick 1: ONE detection of the whole burst, a bounded drain, the rest
        // owned by the persisted backlog while since_ref advances.
        let tick1 = handle_daemon_tick(
            &mut state,
            layers::DaemonTickInput {
                agent_id: "test".into(),
                max_files: BUDGET,
            },
        )
        .expect("burst tick");
        assert_eq!(tick1["changed_files_detected"], N, "one detection sees ALL");
        assert_eq!(tick1["files_reingested"], BUDGET);
        assert_eq!(tick1["backlog_len"], N - BUDGET);
        assert_eq!(
            state.daemon_state.git_since_ref, state.daemon_state.git_head_ref,
            "since_ref advances immediately — the backlog owns the tail"
        );

        // Drain ticks: git reports nothing new (detection already happened), the
        // backlog empties, and EVERY file of the burst gets re-ingested.
        let mut total_reingested = tick1["files_reingested"].as_u64().unwrap() as usize;
        for _ in 0..10 {
            if state.daemon_state.pending_backlog.is_empty() {
                break;
            }
            let tick = handle_daemon_tick(
                &mut state,
                layers::DaemonTickInput {
                    agent_id: "test".into(),
                    max_files: BUDGET,
                },
            )
            .expect("drain tick");
            assert_eq!(
                tick["changed_files_detected"], 0,
                "drain ticks detect nothing new — the burst was ONE detection"
            );
            total_reingested += tick["files_reingested"].as_u64().unwrap() as usize;
        }
        assert!(
            state.daemon_state.pending_backlog.is_empty(),
            "the backlog must drain completely"
        );
        assert_eq!(
            total_reingested, N,
            "every file of the burst is re-ingested — no tail is lost"
        );
    }

    /// HONEST COST BENCH (gardener v1 gate): measures the daemon tick against a
    /// burst of N changed files on the git backend — the number the verdict
    /// demands BEFORE any aggressive default. Ignored in CI (wall-clock noise);
    /// run manually, release, with RSS captured externally:
    ///   /usr/bin/time -l cargo test -p m1nd-mcp --release --lib -- \
    ///     bench_daemon_tick_burst --ignored --nocapture
    /// Numbers are recorded in docs/voice/GARDENER-V1.md.
    #[test]
    #[ignore = "cost bench — run manually, numbers recorded in the arc doc"]
    fn bench_daemon_tick_burst() {
        for n in [10usize, 100, 1000] {
            let (temp, mut state) = build_state();
            let repo = temp.path().join("repo");
            std::fs::create_dir_all(repo.join("src")).expect("repo src");
            for i in 0..n {
                std::fs::write(
                    repo.join(format!("src/f{i:04}.py")),
                    format!("def f{i:04}():\n    return {i}\n"),
                )
                .expect("write file");
            }
            let git = |args: &[&str]| {
                let out = Command::new("git")
                    .args(args)
                    .current_dir(&repo)
                    .output()
                    .expect("spawn git");
                assert!(out.status.success(), "git {args:?} failed");
            };
            git(&["init", "-q"]);
            git(&["config", "user.email", "bench@example.com"]);
            git(&["config", "user.name", "Bench"]);
            git(&["add", "."]);
            git(&["commit", "-q", "-m", "init"]);

            crate::tools::handle_ingest(
                &mut state,
                crate::protocol::IngestInput {
                    path: repo.to_string_lossy().to_string(),
                    agent_id: "bench".into(),
                    mode: "replace".into(),
                    incremental: false,
                    adapter: "code".into(),
                    namespace: None,
                    include_dotfiles: false,
                    dotfile_patterns: Vec::new(),
                    project_root: None,
                },
            )
            .expect("initial ingest");
            handle_daemon_start(
                &mut state,
                layers::DaemonStartInput {
                    agent_id: "bench".into(),
                    watch_paths: vec![repo.to_string_lossy().to_string()],
                    poll_interval_ms: 200,
                },
            )
            .expect("daemon start");

            // THE BURST: every file changes, head moves (the checkout shape).
            for i in 0..n {
                std::fs::write(
                    repo.join(format!("src/f{i:04}.py")),
                    format!("def f{i:04}():\n    return {i} + 1\n"),
                )
                .expect("rewrite file");
            }
            git(&["add", "."]);
            git(&["commit", "-q", "-m", "burst"]);

            // Detection tick (default drain budget 32) + drain to empty.
            let t0 = std::time::Instant::now();
            let first = handle_daemon_tick(
                &mut state,
                layers::DaemonTickInput {
                    agent_id: "bench".into(),
                    max_files: 32,
                },
            )
            .expect("detection tick");
            let detection_ms = t0.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(first["changed_files_detected"], n);

            let mut drain_ticks = 0usize;
            let t1 = std::time::Instant::now();
            while !state.daemon_state.pending_backlog.is_empty() {
                handle_daemon_tick(
                    &mut state,
                    layers::DaemonTickInput {
                        agent_id: "bench".into(),
                        max_files: 32,
                    },
                )
                .expect("drain tick");
                drain_ticks += 1;
            }
            let drain_ms = t1.elapsed().as_secs_f64() * 1000.0;
            let total_ms = detection_ms + drain_ms;
            println!(
                "bench_daemon_tick_burst N={n:>5}: detection_tick={detection_ms:>9.1}ms \
                 drain_ticks={drain_ticks:>3} drain={drain_ms:>9.1}ms \
                 total={total_ms:>9.1}ms per_file={:>7.2}ms",
                total_ms / n as f64
            );
        }
    }

    /// Backward compatibility (gardener v1): a pre-gardener daemon_state.json
    /// (no `pending_backlog` field) must keep deserializing with `active`
    /// preserved. A parse failure would fall back to Default and silently
    /// DISARM every resumed daemon on upgrade.
    #[test]
    fn pre_gardener_daemon_state_still_deserializes_and_stays_armed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        // A faithful pre-gardener shape: serialize the current struct, then
        // DELETE the new field from the JSON before writing it to disk.
        let old = crate::session::DaemonRuntimeState {
            active: true,
            watch_paths: vec!["/tmp/watch".into()],
            poll_interval_ms: 60_000,
            ..Default::default()
        };
        let mut value = serde_json::to_value(&old).expect("serialize");
        value
            .as_object_mut()
            .expect("object")
            .remove("pending_backlog")
            .expect("the new field exists on the current struct");
        std::fs::write(
            runtime_dir.join("daemon_state.json"),
            serde_json::to_string_pretty(&value).expect("stringify"),
        )
        .expect("write old-shape state");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir.clone()),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        assert!(
            state.daemon_state.active,
            "an old-shape daemon_state must resume armed — a parse fallback \
             to Default would silently disarm it"
        );
        assert!(state.daemon_state.pending_backlog.is_empty());
        assert_eq!(state.daemon_state.poll_interval_ms, 60_000);
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
        assert!(started["git_baseline_ref"].as_str().is_some());
        assert!(started["git_head_ref"].as_str().is_some());
        assert_eq!(started["git_baseline_kind"], "head");
        assert_eq!(started["git_since_ref"], started["git_baseline_ref"]);
    }

    #[test]
    fn daemon_start_prefers_merge_base_when_upstream_exists() {
        let (temp, mut state) = build_state();
        let remote = temp.path().join("remote.git");
        let seed = temp.path().join("seed");
        std::fs::create_dir_all(seed.join("src")).expect("seed src");
        std::fs::write(seed.join("src/core.py"), "def core():\n    return 1\n").expect("write");

        Command::new("git")
            .args(["init", "--bare", remote.to_string_lossy().as_ref()])
            .output()
            .expect("bare init");

        Command::new("git")
            .args(["init"])
            .current_dir(&seed)
            .output()
            .expect("git init seed");
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&seed)
            .output()
            .expect("git email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&seed)
            .output()
            .expect("git name");
        Command::new("git")
            .args(["add", "."])
            .current_dir(&seed)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&seed)
            .output()
            .expect("git commit");
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&seed)
            .output()
            .expect("branch main");
        Command::new("git")
            .args(["remote", "add", "origin", remote.to_string_lossy().as_ref()])
            .current_dir(&seed)
            .output()
            .expect("remote add");
        Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(&seed)
            .output()
            .expect("push main");

        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(&seed)
            .output()
            .expect("feature branch");
        Command::new("git")
            .args(["branch", "--set-upstream-to", "origin/main"])
            .current_dir(&seed)
            .output()
            .expect("set upstream");
        std::fs::write(seed.join("src/core.py"), "def core():\n    return 2\n").expect("rewrite");
        Command::new("git")
            .args(["add", "."])
            .current_dir(&seed)
            .output()
            .expect("add feature");
        Command::new("git")
            .args(["commit", "-m", "feature"])
            .current_dir(&seed)
            .output()
            .expect("commit feature");

        let started = handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![seed.to_string_lossy().to_string()],
                poll_interval_ms: 200,
            },
        )
        .expect("daemon start");

        assert_eq!(started["watch_backend"], "git_native_fs");
        assert_eq!(started["git_baseline_kind"], "merge_base");
        assert!(started["git_baseline_ref"].as_str().is_some());
        assert!(started["git_since_ref"].as_str().is_some());
        assert!(started["git_head_ref"].as_str().is_some());
        assert_eq!(started["git_since_ref"], started["git_baseline_ref"]);
        assert_ne!(started["git_head_ref"], started["git_baseline_ref"]);
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
                project_root: None,
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
        assert!(state.daemon_state.git_head_ref.is_some());
        assert_eq!(
            state.daemon_state.git_since_ref,
            state.daemon_state.git_head_ref
        );
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
                project_root: None,
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
        assert!(state.daemon_state.git_operation_in_progress);
        assert_eq!(
            state.daemon_state.git_operation_kind.as_deref(),
            Some("merge")
        );
        assert!(state.daemon_state.deferred_ticks >= 1);
    }
}
