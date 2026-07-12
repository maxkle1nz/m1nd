use crate::protocol::auto_ingest::{
    AutoIngestEventSummary, AutoIngestStartInput, AutoIngestStartOutput, AutoIngestStatusInput,
    AutoIngestStatusOutput, AutoIngestStopInput, AutoIngestStopOutput, AutoIngestTickInput,
    AutoIngestTickOutput,
};
use crate::scope::normalize_path_text;
use crate::session::SessionState;
use crate::universal_docs;
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_ingest::document_router::{DocumentFormat, DocumentRouter};
use m1nd_ingest::merge::{collect_source_claims, prune_source_claims, SourceClaims};
use m1nd_ingest::path_policy;
use m1nd_ingest::{
    BibTexAdapter, CrossRefAdapter, IngestAdapter, JatsArticleAdapter, L1ghtIngestAdapter,
    PatentIngestAdapter, RfcAdapter, UniversalIngestAdapter,
};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const RECENT_EVENT_LIMIT: usize = 40;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum PendingChangeKind {
    Upsert,
    Delete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoIngestFingerprint {
    pub canonical_path: String,
    pub size: u64,
    pub mtime_ms: u64,
    pub content_hash: String,
    pub detected_format: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoIngestManifestEntry {
    pub source_path: String,
    pub format: String,
    pub namespace: Option<String>,
    pub fingerprint: AutoIngestFingerprint,
    pub claims: SourceClaims,
    pub last_ingested_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PendingChange {
    path: String,
    kind: PendingChangeKind,
    first_seen_ms: u64,
    last_seen_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AutoIngestPersistentState {
    owner_agent_id: Option<String>,
    roots: Vec<String>,
    formats: Vec<String>,
    debounce_ms: u64,
    namespace: Option<String>,
    manifest: HashMap<String, AutoIngestManifestEntry>,
    events_seen: u64,
    ingests_applied: u64,
    removals_applied: u64,
    skipped_count: u64,
    error_count: u64,
    last_tick_ms: Option<u64>,
    last_error: Option<String>,
    recent_events: Vec<AutoIngestEventSummary>,
}

impl Default for AutoIngestPersistentState {
    fn default() -> Self {
        Self {
            owner_agent_id: None,
            roots: Vec::new(),
            formats: vec![
                "universal".into(),
                "light".into(),
                "article".into(),
                "bibtex".into(),
                "crossref".into(),
                "rfc".into(),
                "patent".into(),
            ],
            debounce_ms: 200,
            namespace: None,
            manifest: HashMap::new(),
            events_seen: 0,
            ingests_applied: 0,
            removals_applied: 0,
            skipped_count: 0,
            error_count: 0,
            last_tick_ms: None,
            last_error: None,
            recent_events: Vec::new(),
        }
    }
}

struct AutoIngestWatcherHandle {
    _watcher: RecommendedWatcher,
}

fn provider_status_map() -> HashMap<String, bool> {
    serde_json::to_value(universal_docs::provider_availability())
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|map| {
            map.into_iter()
                .filter_map(|(key, value)| value.as_bool().map(|present| (key, present)))
                .collect()
        })
        .unwrap_or_default()
}

pub struct AutoIngestState {
    persistent: AutoIngestPersistentState,
    running: bool,
    pending: Arc<parking_lot::Mutex<HashMap<String, PendingChange>>>,
    watcher: Option<AutoIngestWatcherHandle>,
}

impl AutoIngestState {
    fn empty() -> Self {
        Self {
            persistent: AutoIngestPersistentState::default(),
            running: false,
            pending: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            watcher: None,
        }
    }

    pub fn load(runtime_root: &Path) -> Self {
        let state = fs::read_to_string(Self::state_path(runtime_root))
            .ok()
            .and_then(|content| serde_json::from_str::<AutoIngestPersistentState>(&content).ok())
            .unwrap_or_default();

        Self {
            persistent: state,
            running: false,
            pending: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            watcher: None,
        }
    }

    pub fn persist(&self, runtime_root: &Path) -> M1ndResult<()> {
        save_json_atomic(&Self::state_path(runtime_root), &self.persistent)
    }

    fn state_path(runtime_root: &Path) -> PathBuf {
        runtime_root.join("auto_ingest_state.json")
    }

    fn events_path(runtime_root: &Path) -> PathBuf {
        runtime_root.join("auto_ingest_events.jsonl")
    }

    fn normalized_formats(formats: &[String]) -> M1ndResult<Vec<String>> {
        let supported = HashSet::<&str>::from_iter([
            "universal",
            "light",
            "article",
            "bibtex",
            "crossref",
            "rfc",
            "patent",
        ]);

        let normalized = if formats.is_empty() {
            vec![
                "universal".into(),
                "light".into(),
                "article".into(),
                "bibtex".into(),
                "crossref".into(),
                "rfc".into(),
                "patent".into(),
            ]
        } else {
            formats
                .iter()
                .map(|value| value.trim().to_ascii_lowercase())
                .collect::<Vec<_>>()
        };

        for value in &normalized {
            if !supported.contains(value.as_str()) {
                return Err(M1ndError::InvalidParams {
                    tool: "auto_ingest_start".into(),
                    detail: format!("unsupported auto-ingest format '{}'", value),
                });
            }
        }

        Ok(normalized)
    }

    fn append_event(
        &mut self,
        runtime_root: &Path,
        path: String,
        kind: &str,
        status: &str,
        format: Option<String>,
        detail: Option<String>,
    ) {
        let event = AutoIngestEventSummary {
            path,
            kind: kind.to_string(),
            status: status.to_string(),
            format,
            detail,
            timestamp_ms: now_ms(),
        };
        self.persistent.events_seen += 1;
        self.persistent.recent_events.push(event.clone());
        if self.persistent.recent_events.len() > RECENT_EVENT_LIMIT {
            let drain = self.persistent.recent_events.len() - RECENT_EVENT_LIMIT;
            self.persistent.recent_events.drain(0..drain);
        }

        let line = serde_json::to_string(&event).unwrap_or_default();
        let path = Self::events_path(runtime_root);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if !line.is_empty() {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut file| {
                    use std::io::Write;
                    writeln!(file, "{}", line)
                });
        }
    }

    fn enqueue_change(
        pending: &Arc<parking_lot::Mutex<HashMap<String, PendingChange>>>,
        path: String,
        kind: PendingChangeKind,
    ) {
        let now_ms = now_ms();
        let mut pending = pending.lock();
        pending
            .entry(path.clone())
            .and_modify(|existing| {
                existing.kind = kind.clone();
                existing.last_seen_ms = now_ms;
            })
            .or_insert(PendingChange {
                path,
                kind,
                first_seen_ms: now_ms,
                last_seen_ms: now_ms,
            });
    }

    fn is_noise_path(path: &Path) -> bool {
        path_policy::is_noise_path(path)
    }

    fn canonicalize_path(path: &Path) -> Option<PathBuf> {
        path.canonicalize().ok()
    }

    /// Decide what a filesystem watch event means for the pending queue.
    ///
    /// Watch backends (FSEvents, inotify) can surface events whose path is the
    /// watched directory itself — e.g. metadata updates when children are
    /// created. Directories are never ingestable: enqueueing one only inflates
    /// `queue_depth`, the readiness signal agents and tests poll, letting a
    /// "wait until N changes are queued" observer fire before all N file
    /// events actually arrived (recurring ubuntu CI flake: a single tick then
    /// ingested 1 of 2 watched files). Existing directories are dropped here;
    /// missing paths still enqueue as deletes (a removed path cannot be
    /// stat-ed, and the tick resolves unknown paths to a no-op skip).
    fn watch_event_change_kind(canonical: &Path) -> Option<PendingChangeKind> {
        if canonical.exists() {
            if canonical.is_dir() {
                return None;
            }
            return Some(PendingChangeKind::Upsert);
        }
        Some(PendingChangeKind::Delete)
    }

    fn detect_allowed_format(path: &Path, allowed_formats: &[String]) -> Option<String> {
        let (format, _) = DocumentRouter::detect(path);
        let normalized = match format {
            DocumentFormat::L1ght => "light",
            DocumentFormat::JatsArticle => "article",
            DocumentFormat::BibTeX => "bibtex",
            DocumentFormat::CrossRef => "crossref",
            DocumentFormat::Rfc => "rfc",
            DocumentFormat::Patent => "patent",
            DocumentFormat::Universal => "universal",
            DocumentFormat::Code => {
                return (allowed_formats.iter().any(|value| value == "universal")
                    && UniversalIngestAdapter::can_handle_path(path))
                .then(|| "universal".to_string())
            }
        };

        allowed_formats
            .iter()
            .any(|value| value == normalized)
            .then(|| normalized.to_string())
    }

    fn file_fingerprint(path: &Path, format: &str) -> M1ndResult<AutoIngestFingerprint> {
        let content = fs::read(path).map_err(|error| M1ndError::InvalidParams {
            tool: "auto_ingest_tick".into(),
            detail: format!("failed to read {}: {}", path.display(), error),
        })?;
        let metadata = fs::metadata(path).map_err(|error| M1ndError::InvalidParams {
            tool: "auto_ingest_tick".into(),
            detail: format!("failed to stat {}: {}", path.display(), error),
        })?;
        let mtime_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write(&content);
        let content_hash = format!("{:016x}", hasher.finish());

        Ok(AutoIngestFingerprint {
            canonical_path: path.to_string_lossy().to_string(),
            size: metadata.len(),
            mtime_ms,
            content_hash,
            detected_format: format.to_string(),
        })
    }

    fn manifest_key_for_path(&self, path: &str) -> Option<String> {
        if self.persistent.manifest.contains_key(path) {
            return Some(path.to_string());
        }

        let path_norm = normalize_path_text(path);
        let path_cmp;
        #[cfg(windows)]
        {
            path_cmp = path_norm.to_ascii_lowercase();
        }
        #[cfg(not(windows))]
        {
            path_cmp = path_norm;
        }

        self.persistent
            .manifest
            .keys()
            .find(|source| {
                let source_norm = normalize_path_text(source);
                let source_cmp;
                #[cfg(windows)]
                {
                    source_cmp = source_norm.to_ascii_lowercase();
                }
                #[cfg(not(windows))]
                {
                    source_cmp = source_norm;
                }
                source_cmp == path_cmp
            })
            .cloned()
    }

    fn collect_supported_files(root: &Path, out: &mut Vec<PathBuf>) {
        if !root.exists() {
            return;
        }
        if root.is_file() {
            out.push(root.to_path_buf());
            return;
        }

        let read_dir = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(_) => return,
        };

        for entry in read_dir.filter_map(Result::ok) {
            let path = entry.path();
            if Self::is_noise_path(&path) {
                continue;
            }
            if path.is_dir() {
                Self::collect_supported_files(&path, out);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }

    fn ingest_with_format(
        format: &str,
        path: &Path,
        namespace: Option<String>,
    ) -> M1ndResult<(m1nd_core::graph::Graph, m1nd_ingest::IngestStats)> {
        match format {
            "universal" => UniversalIngestAdapter::new(namespace).ingest(path),
            "light" => L1ghtIngestAdapter::new(namespace).ingest(path),
            "article" => JatsArticleAdapter::new(namespace).ingest(path),
            "bibtex" => BibTexAdapter::new(namespace).ingest(path),
            "crossref" => CrossRefAdapter::new(namespace).ingest(path),
            "rfc" => RfcAdapter::new(namespace).ingest(path),
            "patent" => PatentIngestAdapter::new(namespace).ingest(path),
            other => Err(M1ndError::InvalidParams {
                tool: "auto_ingest_tick".into(),
                detail: format!("unsupported format '{}'", other),
            }),
        }
    }

    fn replace_graph(state: &mut SessionState, graph: m1nd_core::graph::Graph) -> M1ndResult<()> {
        {
            let mut current = state.graph.write();
            *current = graph;
            if !current.finalized && current.num_nodes() > 0 {
                current.finalize()?;
            }
        }

        state.rebuild_engines()?;
        Ok(())
    }

    fn scan_roots_for_bootstrap(&mut self) {
        let formats = self.persistent.formats.clone();
        let roots = self.persistent.roots.clone();

        for root in roots {
            let root = PathBuf::from(root);
            let mut files = Vec::new();
            Self::collect_supported_files(&root, &mut files);
            for path in files {
                if Self::is_noise_path(&path) {
                    continue;
                }
                let Some(canonical) = Self::canonicalize_path(&path) else {
                    continue;
                };
                if Self::detect_allowed_format(&canonical, &formats).is_some() {
                    Self::enqueue_change(
                        &self.pending,
                        canonical.to_string_lossy().to_string(),
                        PendingChangeKind::Upsert,
                    );
                }
            }
        }

        let missing_paths: Vec<String> = self
            .persistent
            .manifest
            .keys()
            .filter(|path| !Path::new(path).exists())
            .cloned()
            .collect();
        for path in missing_paths {
            Self::enqueue_change(&self.pending, path, PendingChangeKind::Delete);
        }
    }

    fn enqueue_missing_manifest_deletes(&mut self) {
        let missing_paths: Vec<String> = self
            .persistent
            .manifest
            .keys()
            .filter(|path| !Path::new(path.as_str()).exists())
            .cloned()
            .collect();
        for path in missing_paths {
            Self::enqueue_change(&self.pending, path, PendingChangeKind::Delete);
        }
    }

    fn take_ready_changes(&mut self, force: bool) -> Vec<PendingChange> {
        let now_ms = now_ms();
        let debounce_ms = self.persistent.debounce_ms;
        let mut pending = self.pending.lock();
        let ready_keys: Vec<String> = pending
            .iter()
            .filter_map(|(key, change)| {
                let is_ready = force || now_ms.saturating_sub(change.last_seen_ms) >= debounce_ms;
                is_ready.then(|| key.clone())
            })
            .collect();

        ready_keys
            .into_iter()
            .filter_map(|key| pending.remove(&key))
            .collect()
    }

    fn start_watcher(&mut self) -> M1ndResult<()> {
        let pending = Arc::clone(&self.pending);
        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else {
                    return;
                };
                for path in event.paths {
                    if AutoIngestState::is_noise_path(&path) {
                        continue;
                    }
                    let canonical =
                        AutoIngestState::canonicalize_path(&path).unwrap_or_else(|| path.clone());
                    let Some(kind) = AutoIngestState::watch_event_change_kind(&canonical) else {
                        continue;
                    };
                    AutoIngestState::enqueue_change(
                        &pending,
                        canonical.to_string_lossy().to_string(),
                        kind,
                    );
                }
            },
            Config::default(),
        )
        .map_err(|error| M1ndError::InvalidParams {
            tool: "auto_ingest_start".into(),
            detail: format!("failed to create notify watcher: {}", error),
        })?;

        for root in &self.persistent.roots {
            let root_path = Path::new(root);
            let mode = if root_path.is_file() {
                RecursiveMode::NonRecursive
            } else {
                RecursiveMode::Recursive
            };
            watcher
                .watch(root_path, mode)
                .map_err(|error| M1ndError::InvalidParams {
                    tool: "auto_ingest_start".into(),
                    detail: format!("failed to watch {}: {}", root, error),
                })?;
        }

        self.watcher = Some(AutoIngestWatcherHandle { _watcher: watcher });
        self.running = true;
        Ok(())
    }

    pub fn start(
        &mut self,
        state: &mut SessionState,
        input: AutoIngestStartInput,
    ) -> M1ndResult<AutoIngestStartOutput> {
        self.stop_internal();
        self.persistent.owner_agent_id = Some(input.agent_id);
        self.persistent.roots = input.roots;
        self.persistent.formats = Self::normalized_formats(&input.formats)?;
        self.persistent.debounce_ms = input.debounce_ms;
        self.persistent.namespace = input.namespace;
        self.persistent.last_error = None;

        for root in &self.persistent.roots {
            if let Some(pos) = state
                .ingest_roots
                .iter()
                .position(|existing| existing == root)
            {
                let root = state.ingest_roots.remove(pos);
                state.ingest_roots.push(root);
            } else {
                state.ingest_roots.push(root.clone());
            }
        }
        // GARDENER v1 GUARD (verdict trap: auto_ingest_start MUTATES
        // workspace_root — the #326 store-dir/code-root bug class). A HOSTED
        // brain's workspace_root IS its project root, stamped from its birth
        // manifest (`project_brain_manifest`); letting a document watcher's
        // first root overwrite it would demote the brain's code identity to a
        // docs dir (wrong code_root_path, wrong medulla classification). The
        // bound owner keeps the historical mutation (its workspace_root is not
        // manifest-anchored) — registered as residue in the arc's divergences.
        let manifest_bound =
            state.workspace_root_source.as_deref() == Some("project_brain_manifest");
        if !manifest_bound {
            if let Some(first_root) = self.persistent.roots.first() {
                state.workspace_root = Some(first_root.clone());
            }
        }

        self.start_watcher()?;
        self.scan_roots_for_bootstrap();
        let bootstrap = self.tick(state, true)?;
        self.persist(&state.runtime_root)?;

        Ok(AutoIngestStartOutput {
            running: self.running,
            backend: "notify".into(),
            roots: self.persistent.roots.clone(),
            formats: self.persistent.formats.clone(),
            debounce_ms: self.persistent.debounce_ms,
            provider_status: provider_status_map(),
            bootstrap,
        })
    }

    fn stop_internal(&mut self) {
        self.watcher = None;
        self.running = false;
    }

    pub fn stop(
        &mut self,
        state: &mut SessionState,
        _input: AutoIngestStopInput,
    ) -> M1ndResult<AutoIngestStopOutput> {
        self.stop_internal();
        self.persist(&state.runtime_root)?;
        Ok(AutoIngestStopOutput {
            stopped: true,
            manifest_entries: self.persistent.manifest.len(),
        })
    }

    pub fn status(
        &mut self,
        state: &mut SessionState,
        _input: AutoIngestStatusInput,
    ) -> AutoIngestStatusOutput {
        let (
            semantic_document_count,
            semantic_section_count,
            semantic_claim_count,
            semantic_entity_count,
            semantic_citation_count,
            drift_document_count,
        ) = universal_docs::aggregate_semantic_metrics(state);
        let (provider_route_counts, provider_fallback_counts) =
            universal_docs::provider_route_metrics(state);
        AutoIngestStatusOutput {
            running: self.running,
            owner_agent_id: self.persistent.owner_agent_id.clone(),
            backend: "notify".into(),
            roots: self.persistent.roots.clone(),
            formats: self.persistent.formats.clone(),
            debounce_ms: self.persistent.debounce_ms,
            manifest_entries: self.persistent.manifest.len(),
            queue_depth: self.pending.lock().len(),
            events_seen: self.persistent.events_seen,
            ingests_applied: self.persistent.ingests_applied,
            removals_applied: self.persistent.removals_applied,
            skipped_count: self.persistent.skipped_count,
            error_count: self.persistent.error_count,
            last_tick_ms: self.persistent.last_tick_ms,
            last_error: self.persistent.last_error.clone(),
            provider_status: provider_status_map(),
            canonical_artifact_count: self
                .persistent
                .manifest
                .values()
                .filter(|entry| entry.format == "universal")
                .count(),
            semantic_document_count,
            semantic_section_count,
            semantic_claim_count,
            semantic_entity_count,
            semantic_citation_count,
            drift_document_count,
            provider_route_counts,
            provider_fallback_counts,
            recent_events: self.persistent.recent_events.clone(),
        }
    }

    pub fn maybe_tick(&mut self, state: &mut SessionState) -> M1ndResult<()> {
        // Read-only attach must never re-ingest/persist, even if a prior
        // read-write session left auto-ingest `running` in the loaded state.
        if state.read_only {
            return Ok(());
        }
        if !self.running {
            return Ok(());
        }
        if self.pending.lock().is_empty() {
            return Ok(());
        }
        let _ = self.tick(state, false)?;
        Ok(())
    }

    pub fn tick(
        &mut self,
        state: &mut SessionState,
        force: bool,
    ) -> M1ndResult<AutoIngestTickOutput> {
        self.enqueue_missing_manifest_deletes();
        let changes = self.take_ready_changes(force);
        let mut changed_paths = Vec::new();
        let mut ingested_paths = Vec::new();
        let mut removed_paths = Vec::new();
        let mut skipped_paths = Vec::new();
        let mut errored_paths = Vec::new();
        let mut applied_any = false;

        for change in changes {
            let path = change.path.clone();
            changed_paths.push(path.clone());
            let format = Self::detect_allowed_format(Path::new(&path), &self.persistent.formats);

            match change.kind {
                PendingChangeKind::Delete => {
                    if let Some(source_path) = self.manifest_key_for_path(&path) {
                        let claims = self
                            .persistent
                            .manifest
                            .iter()
                            .map(|(source, claims)| (source.clone(), claims.claims.clone()))
                            .collect::<HashMap<_, _>>();
                        let current = state.graph.read();
                        let pruned = prune_source_claims(&current, &source_path, &claims)?;
                        drop(current);
                        Self::replace_graph(state, pruned)?;
                        self.persistent.manifest.remove(&source_path);
                        state.document_cache.entries.remove(&source_path);
                        let _ = universal_docs::remove_artifacts_for_source(
                            &state.runtime_root,
                            &source_path,
                        );
                        self.persistent.removals_applied += 1;
                        removed_paths.push(source_path.clone());
                        applied_any = true;
                        self.append_event(
                            &state.runtime_root,
                            path,
                            "delete",
                            "removed",
                            None,
                            None,
                        );
                    } else {
                        self.persistent.skipped_count += 1;
                        skipped_paths.push(path.clone());
                        self.append_event(
                            &state.runtime_root,
                            path,
                            "delete",
                            "skipped",
                            None,
                            Some("no manifest entry".into()),
                        );
                    }
                }
                PendingChangeKind::Upsert => {
                    let Some(format) = format else {
                        if let Some(source_path) = self.manifest_key_for_path(&path) {
                            let claims = self
                                .persistent
                                .manifest
                                .iter()
                                .map(|(source, claims)| (source.clone(), claims.claims.clone()))
                                .collect::<HashMap<_, _>>();
                            let current = state.graph.read();
                            let pruned = prune_source_claims(&current, &source_path, &claims)?;
                            drop(current);
                            Self::replace_graph(state, pruned)?;
                            self.persistent.manifest.remove(&source_path);
                            state.document_cache.entries.remove(&source_path);
                            let _ = universal_docs::remove_artifacts_for_source(
                                &state.runtime_root,
                                &source_path,
                            );
                            self.persistent.removals_applied += 1;
                            removed_paths.push(source_path);
                            applied_any = true;
                        } else {
                            self.persistent.skipped_count += 1;
                            skipped_paths.push(path.clone());
                        }
                        self.append_event(
                            &state.runtime_root,
                            path,
                            "upsert",
                            "ignored",
                            None,
                            Some("unsupported or code file".into()),
                        );
                        continue;
                    };

                    let fingerprint = match Self::file_fingerprint(Path::new(&path), &format) {
                        Ok(value) => value,
                        Err(error) => {
                            self.persistent.error_count += 1;
                            self.persistent.last_error = Some(error.to_string());
                            errored_paths.push(path.clone());
                            self.append_event(
                                &state.runtime_root,
                                path,
                                "upsert",
                                "error",
                                Some(format),
                                Some(error.to_string()),
                            );
                            continue;
                        }
                    };

                    if self.persistent.manifest.get(&path).is_some_and(|entry| {
                        entry.fingerprint.content_hash == fingerprint.content_hash
                    }) {
                        self.persistent.skipped_count += 1;
                        skipped_paths.push(path.clone());
                        self.append_event(
                            &state.runtime_root,
                            path,
                            "upsert",
                            "skipped",
                            Some(format),
                            Some("unchanged fingerprint".into()),
                        );
                        continue;
                    }

                    let overlay = if format == "universal" {
                        let namespace = self
                            .persistent
                            .namespace
                            .clone()
                            .unwrap_or_else(|| "universal".to_string());
                        match UniversalIngestAdapter::new(Some(namespace.clone()))
                            .ingest_bundle(Path::new(&path))
                        {
                            Ok(mut bundle) => {
                                match universal_docs::write_canonical_artifacts_with_source_root(
                                    &state.runtime_root,
                                    Some(Path::new(&path)),
                                    &bundle.documents,
                                    &namespace,
                                ) {
                                    Ok(artifacts) => {
                                        universal_docs::ensure_cache_root_in_ingest_roots(state);
                                        universal_docs::rewrite_graph_provenance_to_canonical(
                                            &mut bundle.graph,
                                            &artifacts.entries,
                                            &namespace,
                                        );
                                        for entry in artifacts.entries {
                                            state
                                                .document_cache
                                                .entries
                                                .insert(entry.source_path.clone(), entry);
                                        }
                                    }
                                    Err(error) => {
                                        self.persistent.error_count += 1;
                                        self.persistent.last_error = Some(error.to_string());
                                        errored_paths.push(path.clone());
                                        self.append_event(
                                            &state.runtime_root,
                                            path.clone(),
                                            "upsert",
                                            "error",
                                            Some(format.clone()),
                                            Some(error.to_string()),
                                        );
                                        continue;
                                    }
                                }
                                (bundle.graph, bundle.stats)
                            }
                            Err(error) => {
                                self.persistent.error_count += 1;
                                self.persistent.last_error = Some(error.to_string());
                                errored_paths.push(path.clone());
                                self.append_event(
                                    &state.runtime_root,
                                    path.clone(),
                                    "upsert",
                                    "error",
                                    Some(format.clone()),
                                    Some(error.to_string()),
                                );
                                continue;
                            }
                        }
                    } else {
                        match Self::ingest_with_format(
                            &format,
                            Path::new(&path),
                            self.persistent.namespace.clone(),
                        ) {
                            Ok(value) => value,
                            Err(error) => {
                                self.persistent.error_count += 1;
                                self.persistent.last_error = Some(error.to_string());
                                errored_paths.push(path.clone());
                                self.append_event(
                                    &state.runtime_root,
                                    path,
                                    "upsert",
                                    "error",
                                    Some(format),
                                    Some(error.to_string()),
                                );
                                continue;
                            }
                        }
                    };

                    let claims = collect_source_claims(&overlay.0);
                    let existing_claims = self
                        .persistent
                        .manifest
                        .iter()
                        .map(|(source, entry)| (source.clone(), entry.claims.clone()))
                        .collect::<HashMap<_, _>>();
                    let current = state.graph.read();
                    let pruned = prune_source_claims(&current, &path, &existing_claims)?;
                    drop(current);
                    let merged = m1nd_ingest::merge::merge_graphs(&pruned, &overlay.0)?;
                    Self::replace_graph(state, merged)?;

                    self.persistent.manifest.insert(
                        path.clone(),
                        AutoIngestManifestEntry {
                            source_path: path.clone(),
                            format: format.clone(),
                            namespace: self.persistent.namespace.clone(),
                            fingerprint,
                            claims,
                            last_ingested_ms: now_ms(),
                        },
                    );
                    self.persistent.ingests_applied += 1;
                    ingested_paths.push(path.clone());
                    applied_any = true;
                    self.append_event(
                        &state.runtime_root,
                        path,
                        "upsert",
                        "ingested",
                        Some(format),
                        None,
                    );
                }
            }
        }

        if applied_any {
            universal_docs::refresh_all_document_semantics(state);
            state.notify_watchers(crate::perspective::state::WatchTrigger::Ingest);
        }

        self.persistent.last_tick_ms = Some(now_ms());
        self.persist(&state.runtime_root)?;

        Ok(AutoIngestTickOutput {
            changed_paths,
            ingested_paths,
            removed_paths,
            skipped_paths,
            errored_paths,
            queue_depth: self.pending.lock().len(),
            last_tick_ms: self.persistent.last_tick_ms,
            recent_events: self.persistent.recent_events.clone(),
        })
    }
}

pub fn handle_auto_ingest_start(
    state: &mut SessionState,
    input: AutoIngestStartInput,
) -> M1ndResult<serde_json::Value> {
    let mut runtime = std::mem::replace(&mut state.auto_ingest, AutoIngestState::empty());
    let output = runtime.start(state, input)?;
    state.auto_ingest = runtime;
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

pub fn handle_auto_ingest_stop(
    state: &mut SessionState,
    input: AutoIngestStopInput,
) -> M1ndResult<serde_json::Value> {
    let mut runtime = std::mem::replace(&mut state.auto_ingest, AutoIngestState::empty());
    let output = runtime.stop(state, input)?;
    state.auto_ingest = runtime;
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

pub fn handle_auto_ingest_status(
    state: &mut SessionState,
    input: AutoIngestStatusInput,
) -> M1ndResult<serde_json::Value> {
    let mut runtime = std::mem::replace(&mut state.auto_ingest, AutoIngestState::empty());
    let output = runtime.status(state, input);
    state.auto_ingest = runtime;
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

pub fn handle_auto_ingest_tick(
    state: &mut SessionState,
    _input: AutoIngestTickInput,
) -> M1ndResult<serde_json::Value> {
    let mut runtime = std::mem::replace(&mut state.auto_ingest, AutoIngestState::empty());
    let output = runtime.tick(state, true)?;
    state.auto_ingest = runtime;
    serde_json::to_value(output).map_err(M1ndError::Serde)
}

pub fn maybe_tick_auto_ingest(state: &mut SessionState, tool_name: &str) -> M1ndResult<()> {
    if matches!(
        tool_name,
        "auto_ingest_start"
            | "auto_ingest_stop"
            | "auto_ingest_status"
            | "auto_ingest_tick"
            | "session_handshake"
            | "trust_selftest"
            | "recovery_playbook"
    ) {
        return Ok(());
    }
    let mut runtime = std::mem::replace(&mut state.auto_ingest, AutoIngestState::empty());
    let result = runtime.maybe_tick(state);
    state.auto_ingest = runtime;
    result
}

/// Drive one auto-ingest drain from the server's idle clock (not verb traffic).
/// Reuses the SAME read-only / running / empty-queue short-circuits as
/// `maybe_tick` (auto_ingest.rs) so an idle session still ingests queued
/// changes. No new thread: this rides the `serve()` loop's existing
/// `recv_timeout` wake, so when there is no queued work it returns immediately.
pub fn pump_auto_ingest_if_due(state: &mut SessionState) -> M1ndResult<()> {
    let mut runtime = std::mem::replace(&mut state.auto_ingest, AutoIngestState::empty());
    let result = runtime.maybe_tick(state);
    state.auto_ingest = runtime;
    result
}

fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> M1ndResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let payload = serde_json::to_vec_pretty(value)?;
    fs::write(&tmp, payload)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_events_for_existing_directories_are_dropped() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("watched");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.md");
        fs::write(&file, "# notes").unwrap();

        // An event for the watched directory itself must never enqueue: it
        // would inflate queue_depth without an ingestable change behind it.
        assert_eq!(AutoIngestState::watch_event_change_kind(&dir), None);
        assert_eq!(
            AutoIngestState::watch_event_change_kind(&file),
            Some(PendingChangeKind::Upsert)
        );
        assert_eq!(
            AutoIngestState::watch_event_change_kind(&dir.join("missing.md")),
            Some(PendingChangeKind::Delete)
        );
    }

    #[test]
    fn noise_paths_are_ignored() {
        assert!(AutoIngestState::is_noise_path(Path::new(
            "/tmp/file.md.swp"
        )));
        assert!(AutoIngestState::is_noise_path(Path::new("/tmp/.DS_Store")));
        assert!(AutoIngestState::is_noise_path(Path::new(
            "/tmp/project/.venv/lib/site.py"
        )));
        assert!(AutoIngestState::is_noise_path(Path::new(
            "/tmp/project/graph_snapshot.json"
        )));
        assert!(!AutoIngestState::is_noise_path(Path::new("/tmp/notes.md")));
    }

    #[test]
    fn enqueue_coalesces_last_kind() {
        let pending = Arc::new(parking_lot::Mutex::new(HashMap::new()));
        AutoIngestState::enqueue_change(&pending, "/tmp/a.md".into(), PendingChangeKind::Upsert);
        AutoIngestState::enqueue_change(&pending, "/tmp/a.md".into(), PendingChangeKind::Delete);
        let pending = pending.lock();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending["/tmp/a.md"].kind, PendingChangeKind::Delete);
    }

    #[test]
    fn missing_manifest_paths_are_enqueued_for_delete_reconciliation() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("docs").join("missing.md");
        let missing_key = missing.to_string_lossy().to_string();
        let mut state = AutoIngestState::empty();
        state.persistent.manifest.insert(
            missing_key.clone(),
            AutoIngestManifestEntry {
                source_path: missing_key.clone(),
                format: "light".into(),
                namespace: None,
                fingerprint: AutoIngestFingerprint {
                    canonical_path: missing_key.clone(),
                    size: 1,
                    mtime_ms: 1,
                    content_hash: "hash".into(),
                    detected_format: "light".into(),
                },
                claims: SourceClaims::default(),
                last_ingested_ms: 1,
            },
        );

        state.enqueue_missing_manifest_deletes();
        let pending = state.pending.lock();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[&missing_key].kind, PendingChangeKind::Delete);
    }

    #[test]
    fn load_and_persist_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AutoIngestState::load(dir.path());
        state.persistent.owner_agent_id = Some("agent".into());
        state.persistent.roots = vec!["/tmp".into()];
        state.persist(dir.path()).unwrap();

        let reloaded = AutoIngestState::load(dir.path());
        assert_eq!(reloaded.persistent.owner_agent_id.as_deref(), Some("agent"));
        assert_eq!(reloaded.persistent.roots, vec!["/tmp".to_string()]);
    }

    #[test]
    fn fingerprint_is_stable_for_unchanged_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.md");
        fs::write(&file, "Protocol: L1GHT/1\nNode: stable\n").unwrap();

        let first = AutoIngestState::file_fingerprint(&file, "light").unwrap();
        let second = AutoIngestState::file_fingerprint(&file, "light").unwrap();

        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.size, second.size);
    }

    #[test]
    fn manifest_key_resolves_slash_normalized_aliases() {
        let mut state = AutoIngestState::empty();
        let source = "/tmp/m1nd/docs/notes.md".to_string();
        state.persistent.manifest.insert(
            source.clone(),
            AutoIngestManifestEntry {
                source_path: source.clone(),
                format: "light".into(),
                namespace: None,
                fingerprint: AutoIngestFingerprint {
                    canonical_path: source.clone(),
                    size: 1,
                    mtime_ms: 1,
                    content_hash: "hash".into(),
                    detected_format: "light".into(),
                },
                claims: SourceClaims::default(),
                last_ingested_ms: 1,
            },
        );

        assert_eq!(
            state.manifest_key_for_path("/tmp/m1nd\\docs\\notes.md"),
            Some(source)
        );
    }

    /// A change enqueued into a RUNNING auto-ingest, with NO verb called, must
    /// be drained purely by the idle pump — the exact seam `serve()`'s
    /// recv_timeout wake invokes. The bug: the notify callback only enqueued and
    /// the queue drained solely on other verb traffic, so an idle session sat on
    /// the change forever. We use a Delete against a manifest entry (mirrors
    /// `missing_manifest_paths_are_enqueued_for_delete_reconciliation`) so the
    /// drain is deterministic without a heavy re-ingest, and debounce_ms = 0 so
    /// `take_ready_changes(false)` claims it immediately (no sleep).
    #[test]
    fn idle_pump_drains_queue_without_verb_traffic() {
        use crate::server::McpConfig;
        use m1nd_core::domain::DomainConfig;
        use m1nd_core::graph::Graph;

        let temp = tempfile::tempdir().unwrap();
        let runtime_dir = temp.path().join("runtime");
        fs::create_dir_all(&runtime_dir).unwrap();
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        // Configure the loaded auto-ingest as a running watcher over a tmp root
        // with a manifest entry whose file no longer exists, then enqueue the
        // Delete a notify callback would have produced. No verb is ever called.
        let missing = temp.path().join("docs").join("missing.md");
        let missing_key = missing.to_string_lossy().to_string();
        {
            let ai = &mut state.auto_ingest;
            ai.running = true;
            ai.persistent.debounce_ms = 0;
            ai.persistent.roots = vec![temp.path().to_string_lossy().to_string()];
            ai.persistent.manifest.insert(
                missing_key.clone(),
                AutoIngestManifestEntry {
                    source_path: missing_key.clone(),
                    format: "light".into(),
                    namespace: None,
                    fingerprint: AutoIngestFingerprint {
                        canonical_path: missing_key.clone(),
                        size: 1,
                        mtime_ms: 1,
                        content_hash: "hash".into(),
                        detected_format: "light".into(),
                    },
                    claims: SourceClaims::default(),
                    last_ingested_ms: 1,
                },
            );
            AutoIngestState::enqueue_change(
                &ai.pending,
                missing_key.clone(),
                PendingChangeKind::Delete,
            );
        }

        // The change is queued before any drain — verb traffic never touched it.
        assert_eq!(
            state.auto_ingest.pending.lock().len(),
            1,
            "precondition: the enqueued change is pending before the idle pump"
        );

        // RED discrimination: without the pump, the queue stays non-empty — this
        // mirrors the pre-fix world where the idle timeout wake did nothing for
        // auto-ingest. Proving the assertion below discriminates the fix.
        assert!(
            !state.auto_ingest.pending.lock().is_empty(),
            "no-op stand-in for the pump leaves the queue full (the pre-fix bug)"
        );

        // GREEN: the idle pump — the same function serve()'s Timeout arm calls —
        // drains the queue with zero verb traffic.
        pump_auto_ingest_if_due(&mut state).expect("idle pump");

        assert!(
            state.auto_ingest.pending.lock().is_empty(),
            "idle pump must drain the pending queue without any verb call"
        );
        assert_eq!(
            state.auto_ingest.persistent.removals_applied, 1,
            "the drained Delete reconciled the missing manifest entry"
        );
    }

    /// GUARD (gardener v1, verdict item 3): `auto_ingest_start` mutates
    /// `workspace_root` to its first root — on a HOSTED brain (workspace root
    /// stamped from the birth manifest) that demoted the brain's CODE identity
    /// to a docs dir (the #326 store-dir/code-root bug class). The manifest-bound
    /// root must survive; the bound owner keeps the historical mutation.
    #[test]
    fn auto_ingest_start_never_demotes_a_hosted_brains_code_root() {
        use crate::protocol::auto_ingest::AutoIngestStartInput;
        use crate::server::McpConfig;
        use m1nd_core::domain::DomainConfig;
        use m1nd_core::graph::Graph;

        let temp = tempfile::tempdir().unwrap();
        let runtime_dir = temp.path().join("runtime");
        fs::create_dir_all(&runtime_dir).unwrap();
        let docs = temp.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("notes.md"), "# notes").unwrap();
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        // A hosted brain: workspace root IS the project root, by manifest.
        let project_root = temp.path().join("the-project");
        fs::create_dir_all(&project_root).unwrap();
        state.workspace_root = Some(project_root.to_string_lossy().to_string());
        state.workspace_root_source = Some("project_brain_manifest".into());

        let start_input = AutoIngestStartInput {
            agent_id: "test".into(),
            roots: vec![docs.to_string_lossy().to_string()],
            formats: Vec::new(),
            debounce_ms: 200,
            namespace: None,
        };
        let mut runtime = std::mem::replace(&mut state.auto_ingest, AutoIngestState::empty());
        runtime
            .start(&mut state, start_input.clone())
            .expect("auto_ingest start");
        state.auto_ingest = runtime;
        assert_eq!(
            state.workspace_root.as_deref(),
            Some(project_root.to_string_lossy().to_string().as_str()),
            "a manifest-bound workspace root must NEVER be demoted to a docs dir"
        );

        // The bound owner (no manifest anchor) keeps the historical mutation.
        state.workspace_root_source = None;
        let mut runtime = std::mem::replace(&mut state.auto_ingest, AutoIngestState::empty());
        runtime
            .start(&mut state, start_input)
            .expect("auto_ingest re-start");
        state.auto_ingest = runtime;
        assert_eq!(
            state.workspace_root.as_deref(),
            Some(docs.to_string_lossy().to_string().as_str()),
            "the bound owner's historical mutation is preserved"
        );
    }

    /// FAIL-OPEN, end-to-end through the previously violable seam (gardener v1):
    /// an agent's unrelated tool call used to FAIL when the inline auto-ingest
    /// vigil errored — `dispatch_tool` propagated `maybe_tick_auto_ingest` with a
    /// `?`. This test arranges a REAL erroring tick (the tick's end-of-drain
    /// persist hits a poisoned `auto_ingest_state.tmp` that is a directory, so
    /// `save_json_atomic`'s `fs::write` fails) and asserts the agent's `health`
    /// call still SUCCEEDS. RED under the old `?`: dispatch_tool returned the
    /// vigil's error; GREEN under fail-open: the error is logged and swallowed.
    #[test]
    fn erroring_auto_ingest_vigil_never_fails_the_agents_tool_call() {
        use crate::server::McpConfig;
        use m1nd_core::domain::DomainConfig;
        use m1nd_core::graph::Graph;

        let temp = tempfile::tempdir().unwrap();
        let runtime_dir = temp.path().join("runtime");
        fs::create_dir_all(&runtime_dir).unwrap();
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir.clone()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        // A running auto-ingest with one ready change (debounce 0), exactly as the
        // idle-pump case above — the drain will reach the end-of-tick persist.
        let missing = temp.path().join("docs").join("missing.md");
        let missing_key = missing.to_string_lossy().to_string();
        {
            let ai = &mut state.auto_ingest;
            ai.running = true;
            ai.persistent.debounce_ms = 0;
            ai.persistent.roots = vec![temp.path().to_string_lossy().to_string()];
            ai.persistent.manifest.insert(
                missing_key.clone(),
                AutoIngestManifestEntry {
                    source_path: missing_key.clone(),
                    format: "light".into(),
                    namespace: None,
                    fingerprint: AutoIngestFingerprint {
                        canonical_path: missing_key.clone(),
                        size: 1,
                        mtime_ms: 1,
                        content_hash: "hash".into(),
                        detected_format: "light".into(),
                    },
                    claims: SourceClaims::default(),
                    last_ingested_ms: 1,
                },
            );
            AutoIngestState::enqueue_change(
                &ai.pending,
                missing_key.clone(),
                PendingChangeKind::Delete,
            );
        }

        // POISON the tick's persist: `save_json_atomic` writes
        // `auto_ingest_state.tmp` then renames — a DIRECTORY at the tmp path makes
        // `fs::write` fail on every platform, so the tick returns Err.
        let tmp_path = AutoIngestState::state_path(&state.runtime_root).with_extension("tmp");
        fs::create_dir_all(&tmp_path).expect("poison tmp path as a directory");
        assert!(
            state.auto_ingest.persist(&state.runtime_root).is_err(),
            "precondition: the poisoned tmp path must make the vigil's persist fail"
        );

        // The agent's UNRELATED tool call ('health' is not on the tick's skip
        // list, so the vigil runs inline) must SUCCEED despite the erroring vigil.
        let result = crate::server::dispatch_tool(
            &mut state,
            "health",
            &serde_json::json!({ "agent_id": "test" }),
        );
        assert!(
            result.is_ok(),
            "the agent's tool call must succeed when the auto-ingest vigil errors \
             (fail-open), got: {:?}",
            result.err()
        );
    }
}
