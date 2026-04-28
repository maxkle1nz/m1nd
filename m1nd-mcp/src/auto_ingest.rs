use crate::protocol::auto_ingest::{
    AutoIngestEventSummary, AutoIngestStartInput, AutoIngestStartOutput, AutoIngestStatusInput,
    AutoIngestStatusOutput, AutoIngestStopInput, AutoIngestStopOutput, AutoIngestTickInput,
    AutoIngestTickOutput,
};
use crate::session::SessionState;
use crate::universal_docs;
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_ingest::document_router::{DocumentFormat, DocumentRouter};
use m1nd_ingest::merge::{collect_source_claims, prune_source_claims, SourceClaims};
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

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0)
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
            timestamp_ms: Self::now_ms(),
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
        let now_ms = Self::now_ms();
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
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        name.ends_with('~')
            || name.ends_with(".swp")
            || name.ends_with(".tmp")
            || name == ".DS_Store"
            || name == "node_modules"
            || name == "target"
            || name == "dist"
            || name == "build"
            || name == ".next"
            || name == ".turbo"
            || name.starts_with(".m1nd-runtime")
            || matches!(
                name,
                "plasticity_state.json"
                    | "graph_snapshot.json"
                    | "ingest_roots.json"
                    | "auto_ingest_state.json"
                    | "auto_ingest_events.jsonl"
                    | "daemon_state.json"
                    | "daemon_alerts.json"
                    | "boot_memory_state.json"
                    | "document_cache_index.json"
                    | "tremor_state.json"
                    | "trust_state.json"
            )
            || name.starts_with(".#")
            || name.starts_with("4913")
    }

    fn canonicalize_path(path: &Path) -> Option<PathBuf> {
        path.canonicalize().ok()
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

    fn take_ready_changes(&mut self, force: bool) -> Vec<PendingChange> {
        let now_ms = Self::now_ms();
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
                    let kind = if canonical.exists() {
                        PendingChangeKind::Upsert
                    } else {
                        PendingChangeKind::Delete
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
        if let Some(first_root) = self.persistent.roots.first() {
            state.workspace_root = Some(first_root.clone());
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
                    if self.persistent.manifest.contains_key(&path) {
                        let claims = self
                            .persistent
                            .manifest
                            .iter()
                            .map(|(source, claims)| (source.clone(), claims.claims.clone()))
                            .collect::<HashMap<_, _>>();
                        let current = state.graph.read();
                        let pruned = prune_source_claims(&current, &path, &claims)?;
                        drop(current);
                        Self::replace_graph(state, pruned)?;
                        self.persistent.manifest.remove(&path);
                        state.document_cache.entries.remove(&path);
                        let _ =
                            universal_docs::remove_artifacts_for_source(&state.runtime_root, &path);
                        self.persistent.removals_applied += 1;
                        removed_paths.push(path.clone());
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
                        if self.persistent.manifest.contains_key(&path) {
                            let claims = self
                                .persistent
                                .manifest
                                .iter()
                                .map(|(source, claims)| (source.clone(), claims.claims.clone()))
                                .collect::<HashMap<_, _>>();
                            let current = state.graph.read();
                            let pruned = prune_source_claims(&current, &path, &claims)?;
                            drop(current);
                            Self::replace_graph(state, pruned)?;
                            self.persistent.manifest.remove(&path);
                            state.document_cache.entries.remove(&path);
                            let _ = universal_docs::remove_artifacts_for_source(
                                &state.runtime_root,
                                &path,
                            );
                            self.persistent.removals_applied += 1;
                            removed_paths.push(path.clone());
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
                            last_ingested_ms: Self::now_ms(),
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

        self.persistent.last_tick_ms = Some(Self::now_ms());
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
        "auto_ingest_start" | "auto_ingest_stop" | "auto_ingest_status" | "auto_ingest_tick"
    ) {
        return Ok(());
    }
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
    fn noise_paths_are_ignored() {
        assert!(AutoIngestState::is_noise_path(Path::new(
            "/tmp/file.md.swp"
        )));
        assert!(AutoIngestState::is_noise_path(Path::new("/tmp/.DS_Store")));
        assert!(AutoIngestState::is_noise_path(Path::new(
            "/tmp/node_modules"
        )));
        assert!(AutoIngestState::is_noise_path(Path::new(
            "/tmp/.m1nd-runtime-ila"
        )));
        assert!(AutoIngestState::is_noise_path(Path::new(
            "/tmp/plasticity_state.json"
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
}
