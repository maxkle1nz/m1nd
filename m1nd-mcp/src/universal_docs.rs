use crate::protocol::auto_ingest::{
    DocumentBindingEntry, DocumentBindingsInput, DocumentBindingsOutput, DocumentDriftFinding,
    DocumentDriftInput, DocumentDriftOutput, DocumentDriftSummary, DocumentProviderHealthEntry,
    DocumentProviderHealthInput, DocumentProviderHealthOutput, DocumentResolveInput,
    DocumentResolveOutput,
};
use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::NodeId;
use m1nd_ingest::canonical::{
    source_key, CanonicalDocument, ConfidenceLevel, DocumentCodeCandidate, DocumentEntityCandidate,
};
use m1nd_ingest::universal_adapter::{ProviderAvailability, UniversalIngestAdapter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentCacheEntry {
    pub source_path: String,
    pub source_key: String,
    pub source_kind: String,
    pub detected_type: String,
    pub producer: String,
    pub node_ids: Vec<String>,
    pub original_source_path: String,
    pub canonical_markdown_path: String,
    pub canonical_json_path: String,
    pub claims_path: String,
    pub metadata_path: String,
    pub confidence_summary: HashMap<String, usize>,
    pub section_count: usize,
    pub entity_count: usize,
    pub claim_count: usize,
    pub citation_count: usize,
    pub updated_at_ms: u64,
    pub last_binding_count: usize,
    pub last_drift_findings: usize,
    #[serde(default)]
    pub binding_preview: Vec<DocumentBindingEntry>,
    #[serde(default)]
    pub drift_summary: DocumentDriftSummary,
    #[serde(default)]
    pub last_binding_refresh_generation: u64,
    #[serde(default)]
    pub last_drift_refresh_generation: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DocumentCacheState {
    pub entries: HashMap<String, DocumentCacheEntry>,
}

pub struct UniversalArtifacts {
    pub entries: Vec<DocumentCacheEntry>,
}

pub fn cache_root(runtime_root: &Path) -> PathBuf {
    runtime_root.join("l1ght-cache").join("sources")
}

pub fn cache_index_path(runtime_root: &Path) -> PathBuf {
    runtime_root.join("document_cache_index.json")
}

pub fn ensure_cache_root_in_ingest_roots(state: &mut SessionState) {
    let cache_root = cache_root(&state.runtime_root)
        .to_string_lossy()
        .to_string();
    if let Some(pos) = state
        .ingest_roots
        .iter()
        .position(|root| root == &cache_root)
    {
        let root = state.ingest_roots.remove(pos);
        state.ingest_roots.push(root);
    } else {
        state.ingest_roots.push(cache_root);
    }
}

pub fn load_document_cache(runtime_root: &Path) -> DocumentCacheState {
    fs::read_to_string(cache_index_path(runtime_root))
        .ok()
        .and_then(|content| serde_json::from_str::<DocumentCacheState>(&content).ok())
        .unwrap_or_default()
}

pub fn persist_document_cache(runtime_root: &Path, state: &DocumentCacheState) -> M1ndResult<()> {
    save_json_atomic(&cache_index_path(runtime_root), state)
}

pub fn provider_availability() -> ProviderAvailability {
    UniversalIngestAdapter::provider_availability()
}

pub fn provider_health(_input: DocumentProviderHealthInput) -> M1ndResult<serde_json::Value> {
    let availability = provider_availability();
    let python = m1nd_ingest::UniversalIngestAdapter::provider_python_command();
    let providers = vec![
        DocumentProviderHealthEntry {
            name: "magika".into(),
            available: availability.magika,
            mode: "type-detection".into(),
            detail: None,
            install_hint: (!availability.magika).then_some("Install the Python package `magika` into the provider environment.".into()),
        },
        DocumentProviderHealthEntry {
            name: "trafilatura".into(),
            available: availability.trafilatura,
            mode: "html/wiki extraction".into(),
            detail: None,
            install_hint: (!availability.trafilatura)
                .then_some("Install the Python package `trafilatura` into the provider environment.".into()),
        },
        DocumentProviderHealthEntry {
            name: "markitdown".into(),
            available: availability.markitdown,
            mode: "office/pdf fallback".into(),
            detail: None,
            install_hint: (!availability.markitdown)
                .then_some("Install `markitdown` (and extras like `markitdown[docx]`) into the provider environment.".into()),
        },
        DocumentProviderHealthEntry {
            name: "docling".into(),
            available: availability.docling,
            mode: "broad-spectrum canonicalizer".into(),
            detail: None,
            install_hint: (!availability.docling)
                .then_some("Install the Python package `docling` into the provider environment.".into()),
        },
        DocumentProviderHealthEntry {
            name: "grobid".into(),
            available: availability.grobid,
            mode: "scholarly pdf lane".into(),
            detail: std::env::var("M1ND_GROBID_URL").ok(),
            install_hint: (!availability.grobid)
                .then_some("Set `M1ND_GROBID_URL` to a reachable GROBID service.".into()),
        },
        DocumentProviderHealthEntry {
            name: "marker".into(),
            available: availability.marker,
            mode: "premium pdf lane".into(),
            detail: None,
            install_hint: (!availability.marker).then_some("Install the `marker` CLI and expose it on PATH.".into()),
        },
        DocumentProviderHealthEntry {
            name: "mineru".into(),
            available: availability.mineru,
            mode: "ocr/layout premium lane".into(),
            detail: None,
            install_hint: (!availability.mineru).then_some("Install the `mineru` CLI and expose it on PATH.".into()),
        },
    ];

    serde_json::to_value(DocumentProviderHealthOutput { python, providers })
        .map_err(M1ndError::Serde)
}

pub fn write_canonical_artifacts(
    runtime_root: &Path,
    documents: &[CanonicalDocument],
    namespace: &str,
) -> M1ndResult<UniversalArtifacts> {
    write_canonical_artifacts_with_source_root(runtime_root, None, documents, namespace)
}

pub fn write_canonical_artifacts_with_source_root(
    runtime_root: &Path,
    source_root: Option<&Path>,
    documents: &[CanonicalDocument],
    namespace: &str,
) -> M1ndResult<UniversalArtifacts> {
    let mut entries = Vec::new();
    for document in documents {
        let key = source_key(&document.source_path);
        let dir = cache_root(runtime_root).join(&key);
        fs::create_dir_all(&dir)?;

        let ext = Path::new(&document.source_path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("txt");
        let source_copy = dir.join(format!("source.{}", ext));
        let canonical_md = dir.join("canonical.md");
        let canonical_json = dir.join("canonical.json");
        let claims_json = dir.join("claims.json");
        let metadata_json = dir.join("metadata.json");
        let original_source_bytes = read_original_source_bytes(source_root, &document.source_path);
        let preserved_original_source = original_source_bytes.is_some();
        let source_bytes =
            original_source_bytes.unwrap_or_else(|| document.plain_text.as_bytes().to_vec());

        fs::write(&source_copy, &source_bytes)?;
        fs::write(&canonical_md, render_markdown(document))?;
        save_json_atomic(&canonical_json, document)?;
        save_json_atomic(
            &claims_json,
            &serde_json::json!({
                "entities": document.entities,
                "claims": document.claims,
                "citations": document.citations,
                "links": document.links,
                "sections": document.sections,
            }),
        )?;
        save_json_atomic(
            &metadata_json,
            &serde_json::json!({
                "doc_id": document.doc_id,
                "source_path": document.source_path,
                "source_kind": format!("{:?}", document.source_kind).to_lowercase(),
                "detected_type": document.detected_type,
                "producer": document.producer,
                "content_hash": document.content_hash,
                "source_size_bytes": source_bytes.len(),
                "preserved_original_source": preserved_original_source,
                "namespace": namespace,
            }),
        )?;

        entries.push(DocumentCacheEntry {
            source_path: document.source_path.clone(),
            source_key: key,
            source_kind: format!("{:?}", document.source_kind).to_lowercase(),
            detected_type: document.detected_type.clone(),
            producer: document.producer.clone(),
            node_ids: expected_node_ids(document, namespace),
            original_source_path: source_copy.to_string_lossy().to_string(),
            canonical_markdown_path: canonical_md.to_string_lossy().to_string(),
            canonical_json_path: canonical_json.to_string_lossy().to_string(),
            claims_path: claims_json.to_string_lossy().to_string(),
            metadata_path: metadata_json.to_string_lossy().to_string(),
            confidence_summary: confidence_summary(document),
            section_count: document.sections.len(),
            entity_count: document.entities.len(),
            claim_count: document.claims.len(),
            citation_count: document.citations.len(),
            updated_at_ms: now_ms(),
            last_binding_count: 0,
            last_drift_findings: 0,
            binding_preview: Vec::new(),
            drift_summary: DocumentDriftSummary::default(),
            last_binding_refresh_generation: 0,
            last_drift_refresh_generation: 0,
        });
    }

    Ok(UniversalArtifacts { entries })
}

pub fn remove_artifacts_for_source(runtime_root: &Path, source_path: &str) -> M1ndResult<()> {
    let dir = cache_root(runtime_root).join(source_key(source_path));
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn resolve_source_input_path(source_root: &Path, source_path: &str) -> PathBuf {
    let source = Path::new(source_path);
    if source.is_absolute() {
        return source.to_path_buf();
    }
    if source_root.is_file() {
        return source_root
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(source);
    }
    source_root.join(source)
}

fn read_original_source_bytes(source_root: Option<&Path>, source_path: &str) -> Option<Vec<u8>> {
    let root = source_root?;
    fs::read(resolve_source_input_path(root, source_path)).ok()
}

pub fn rewrite_graph_provenance_to_canonical(
    graph: &mut Graph,
    entries: &[DocumentCacheEntry],
    namespace: &str,
) {
    let mut by_source = HashMap::new();
    for entry in entries {
        by_source.insert(
            entry.source_path.clone(),
            entry.canonical_markdown_path.clone(),
        );
    }

    for idx in 0..graph.num_nodes() as usize {
        let node = NodeId::new(idx as u32);
        let provenance = graph.resolve_node_provenance(node);
        let Some(source_path) = provenance.source_path.as_ref() else {
            continue;
        };
        let Some(canonical_path) = by_source.get(source_path) else {
            continue;
        };
        graph.set_node_provenance(
            node,
            NodeProvenanceInput {
                source_path: Some(canonical_path),
                line_start: provenance.line_start,
                line_end: provenance.line_end,
                excerpt: provenance.excerpt.as_deref(),
                namespace: provenance.namespace.as_deref().or(Some(namespace)),
                canonical: provenance.canonical,
            },
        );
    }
}

fn should_refresh_document_semantics(entry: &DocumentCacheEntry, graph_generation: u64) -> bool {
    entry.last_binding_refresh_generation < graph_generation
        || entry.last_drift_refresh_generation < graph_generation
        || (entry.last_binding_refresh_generation == 0 && entry.last_drift_refresh_generation == 0)
}

fn refresh_document_cache_entry(state: &mut SessionState, source_path: &str) -> M1ndResult<()> {
    let Some(snapshot) = state.document_cache.entries.get(source_path).cloned() else {
        return Ok(());
    };
    if !should_refresh_document_semantics(&snapshot, state.graph_generation) {
        return Ok(());
    }

    let bindings = compute_bindings(state, &snapshot, 8)?;
    let drift = compute_drift(state, &snapshot, &bindings)?;
    if let Some(entry) = state.document_cache.entries.get_mut(source_path) {
        entry.last_binding_count = bindings.len();
        entry.binding_preview = bindings.into_iter().take(8).collect();
        entry.last_drift_findings = drift.summary.total_findings;
        entry.drift_summary = drift.summary;
        entry.last_binding_refresh_generation = state.graph_generation;
        entry.last_drift_refresh_generation = state.graph_generation;
    }
    Ok(())
}

pub fn refresh_all_document_semantics(state: &mut SessionState) {
    let stale_sources = state
        .document_cache
        .entries
        .values()
        .filter(|entry| should_refresh_document_semantics(entry, state.graph_generation))
        .map(|entry| entry.source_path.clone())
        .collect::<Vec<_>>();
    for source_path in stale_sources {
        let _ = refresh_document_cache_entry(state, &source_path);
    }
}

pub fn resolve_document(
    state: &mut SessionState,
    input: DocumentResolveInput,
) -> M1ndResult<serde_json::Value> {
    let source_path = resolve_entry_source_path(
        &state.document_cache,
        "document_resolve",
        input.path,
        input.node_id,
    )?;

    refresh_document_cache_entry(state, &source_path)?;
    let entry = state
        .document_cache
        .entries
        .get(&source_path)
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "document_resolve".into(),
            detail: "document cache entry disappeared during refresh".into(),
        })?;

    serde_json::to_value(DocumentResolveOutput {
        source_path: entry.source_path.clone(),
        source_key: entry.source_key.clone(),
        original_source_path: entry.original_source_path.clone(),
        canonical_markdown_path: entry.canonical_markdown_path.clone(),
        canonical_json_path: entry.canonical_json_path.clone(),
        claims_path: entry.claims_path.clone(),
        metadata_path: entry.metadata_path.clone(),
        detected_type: entry.detected_type.clone(),
        producer: entry.producer.clone(),
        node_ids: entry.node_ids.clone(),
        confidence_summary: entry.confidence_summary.clone(),
        section_count: entry.section_count,
        claim_count: entry.claim_count,
        entity_count: entry.entity_count,
        citation_count: entry.citation_count,
        binding_count: entry.last_binding_count,
        binding_preview: entry.binding_preview.iter().take(3).cloned().collect(),
        drift_summary: entry.drift_summary.clone(),
    })
    .map_err(M1ndError::Serde)
}

pub fn document_bindings(
    state: &mut SessionState,
    input: DocumentBindingsInput,
) -> M1ndResult<serde_json::Value> {
    let source_path = resolve_entry_source_path(
        &state.document_cache,
        "document_bindings",
        input.path,
        input.node_id,
    )?;
    let entry = cache_entry_clone(&state.document_cache, "document_bindings", &source_path)?;
    let bindings = compute_bindings(state, &entry, input.top_k)?;
    if let Some(cache_entry) = state.document_cache.entries.get_mut(&source_path) {
        cache_entry.last_binding_count = bindings.len();
        cache_entry.binding_preview = bindings.iter().take(8).cloned().collect();
        cache_entry.last_binding_refresh_generation = state.graph_generation;
    }
    serde_json::to_value(DocumentBindingsOutput {
        source_path,
        bindings,
    })
    .map_err(M1ndError::Serde)
}

pub fn document_drift(
    state: &mut SessionState,
    input: DocumentDriftInput,
) -> M1ndResult<serde_json::Value> {
    let source_path = resolve_entry_source_path(
        &state.document_cache,
        "document_drift",
        input.path,
        input.node_id,
    )?;
    let entry = cache_entry_clone(&state.document_cache, "document_drift", &source_path)?;
    let bindings = compute_bindings(state, &entry, 16)?;
    let drift = compute_drift(state, &entry, &bindings)?;
    if let Some(cache_entry) = state.document_cache.entries.get_mut(&source_path) {
        cache_entry.last_binding_count = bindings.len();
        cache_entry.binding_preview = bindings.iter().take(8).cloned().collect();
        cache_entry.last_drift_findings = drift.summary.total_findings;
        cache_entry.drift_summary = drift.summary.clone();
        cache_entry.last_binding_refresh_generation = state.graph_generation;
        cache_entry.last_drift_refresh_generation = state.graph_generation;
    }
    serde_json::to_value(drift).map_err(M1ndError::Serde)
}

fn resolve_by_path<'a>(
    cache: &'a DocumentCacheState,
    path: &str,
) -> Option<&'a DocumentCacheEntry> {
    cache.entries.get(path).or_else(|| {
        let matches = cache
            .entries
            .values()
            .filter(|entry| {
                entry.source_path.ends_with(path) || entry.canonical_markdown_path == path
            })
            .collect::<Vec<_>>();
        (matches.len() == 1).then(|| matches[0])
    })
}

fn resolve_by_node_id<'a>(
    cache: &'a DocumentCacheState,
    node_id: &str,
) -> Option<&'a DocumentCacheEntry> {
    cache
        .entries
        .values()
        .find(|entry| entry.node_ids.iter().any(|value| value == node_id))
}

fn resolve_entry_source_path(
    cache: &DocumentCacheState,
    tool: &str,
    path: Option<String>,
    node_id: Option<String>,
) -> M1ndResult<String> {
    if let Some(path) = path {
        if let Some(entry) = cache.entries.get(&path) {
            return Ok(entry.source_path.clone());
        }
        let matches = cache
            .entries
            .values()
            .filter(|entry| {
                entry.source_path.ends_with(&path) || entry.canonical_markdown_path == path
            })
            .collect::<Vec<_>>();
        return match matches.len() {
            1 => Ok(matches[0].source_path.clone()),
            0 => Err(M1ndError::InvalidParams {
                tool: tool.into(),
                detail: format!("no document cache entry found for path '{}'", path),
            }),
            _ => Err(M1ndError::InvalidParams {
                tool: tool.into(),
                detail: format!(
                    "ambiguous document path '{}'; provide a more specific path",
                    path
                ),
            }),
        };
    }
    if let Some(node_id) = node_id {
        return resolve_by_node_id(cache, &node_id)
            .map(|entry| entry.source_path.clone())
            .ok_or_else(|| M1ndError::InvalidParams {
                tool: tool.into(),
                detail: format!("no document cache entry found for node_id '{}'", node_id),
            });
    }
    Err(M1ndError::InvalidParams {
        tool: tool.into(),
        detail: "path or node_id is required".into(),
    })
}

fn cache_entry_clone(
    cache: &DocumentCacheState,
    tool: &str,
    source_path: &str,
) -> M1ndResult<DocumentCacheEntry> {
    cache
        .entries
        .get(source_path)
        .cloned()
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: tool.into(),
            detail: "document cache entry disappeared".into(),
        })
}

fn confidence_summary(document: &CanonicalDocument) -> HashMap<String, usize> {
    let mut summary = HashMap::new();
    for confidence in document
        .entities
        .iter()
        .map(|value| &value.confidence)
        .chain(document.claims.iter().map(|value| &value.confidence))
        .chain(document.citations.iter().map(|value| &value.confidence))
        .chain(document.links.iter().map(|value| &value.confidence))
    {
        let key = match confidence {
            ConfidenceLevel::Explicit => "explicit",
            ConfidenceLevel::Parsed => "parsed",
            ConfidenceLevel::Inferred => "inferred",
        };
        *summary.entry(key.to_string()).or_insert(0) += 1;
    }
    summary
}

pub fn aggregate_semantic_metrics(
    state: &SessionState,
) -> (usize, usize, usize, usize, usize, usize) {
    let document_count = state.document_cache.entries.len();
    let section_count = state
        .document_cache
        .entries
        .values()
        .map(|entry| entry.section_count)
        .sum();
    let claim_count = state
        .document_cache
        .entries
        .values()
        .map(|entry| entry.claim_count)
        .sum();
    let entity_count = state
        .document_cache
        .entries
        .values()
        .map(|entry| entry.entity_count)
        .sum();
    let citation_count = state
        .document_cache
        .entries
        .values()
        .map(|entry| entry.citation_count)
        .sum();

    let drift_document_count = state
        .document_cache
        .entries
        .values()
        .filter(|entry| entry.drift_summary.total_findings > 0)
        .count();

    (
        document_count,
        section_count,
        claim_count,
        entity_count,
        citation_count,
        drift_document_count,
    )
}

fn is_fallback_route(producer: &str) -> bool {
    matches!(
        producer,
        "universal:internal" | "universal:internal-html" | "universal:markitdown"
    )
}

pub fn provider_route_metrics(
    state: &SessionState,
) -> (HashMap<String, usize>, HashMap<String, usize>) {
    let mut route_counts = HashMap::new();
    let mut fallback_counts = HashMap::new();
    for entry in state.document_cache.entries.values() {
        *route_counts.entry(entry.producer.clone()).or_insert(0) += 1;
        if is_fallback_route(&entry.producer) {
            *fallback_counts.entry(entry.producer.clone()).or_insert(0) += 1;
        }
    }
    (route_counts, fallback_counts)
}

fn load_canonical_document(entry: &DocumentCacheEntry) -> M1ndResult<CanonicalDocument> {
    let content = fs::read_to_string(&entry.canonical_json_path)?;
    serde_json::from_str(&content).map_err(M1ndError::Serde)
}

fn collect_binding_candidates(
    document: &CanonicalDocument,
) -> Vec<(String, String, ConfidenceLevel)> {
    let mut out = Vec::new();
    for candidate in &document.code_candidates {
        out.push((
            candidate.label.clone(),
            format!("candidate:{:?}", candidate.candidate_kind).to_lowercase(),
            candidate.confidence.clone(),
        ));
    }
    for entity in &document.entities {
        out.push((
            entity.label.clone(),
            format!("entity:{:?}", entity.kind).to_lowercase(),
            entity.confidence.clone(),
        ));
    }
    out
}

fn score_binding(
    candidate: &str,
    relation_hint: &str,
    ext_id: &str,
    label: &str,
    file_path: Option<&str>,
) -> Option<(f32, String, String)> {
    if candidate.is_empty() {
        return None;
    }
    if ext_id == candidate || ext_id.ends_with(candidate) {
        return Some((
            1.0,
            infer_relation(relation_hint, candidate),
            "exact external id match".into(),
        ));
    }
    if label == candidate {
        return Some((
            0.92,
            infer_relation(relation_hint, candidate),
            "exact label match".into(),
        ));
    }
    if let Some(path) = file_path {
        if path.ends_with(candidate) || candidate.ends_with(path) || path.contains(candidate) {
            return Some((0.88, "mentions_file".into(), "file path match".into()));
        }
    }
    if candidate.starts_with("m1nd.") && (label.contains(candidate) || ext_id.contains(candidate)) {
        return Some((0.9, "mentions_tool".into(), "tool id match".into()));
    }
    if (candidate.contains("::") || candidate.contains('.'))
        && (label.contains(candidate) || ext_id.contains(candidate))
    {
        return Some((
            0.8,
            infer_relation(relation_hint, candidate),
            "symbol-like substring match".into(),
        ));
    }
    None
}

fn infer_relation(relation_hint: &str, candidate: &str) -> String {
    if relation_hint.contains("filepath") {
        "mentions_file".into()
    } else if relation_hint.contains("tool") || candidate.starts_with("m1nd.") {
        "mentions_tool".into()
    } else if relation_hint.contains("test") || candidate.to_ascii_lowercase().contains("test") {
        "tests".into()
    } else {
        "mentions_symbol".into()
    }
}

fn compute_bindings(
    state: &SessionState,
    entry: &DocumentCacheEntry,
    top_k: usize,
) -> M1ndResult<Vec<DocumentBindingEntry>> {
    let document = load_canonical_document(entry)?;
    let candidates = collect_binding_candidates(&document);
    let graph = state.graph.read();
    let mut bindings = Vec::new();
    for (interned, &node_id) in &graph.id_to_node {
        let ext_id = graph.strings.resolve(*interned);
        if ext_id.starts_with("universal::") {
            continue;
        }
        let idx = node_id.as_usize();
        let label = graph.strings.resolve(graph.nodes.label[idx]);
        let provenance = graph.resolve_node_provenance(node_id);
        let file_path = provenance.source_path.clone();
        if file_path
            .as_deref()
            .is_some_and(|path| path == entry.canonical_markdown_path || path == entry.source_path)
        {
            continue;
        }
        for (candidate, relation_hint, confidence) in &candidates {
            if let Some((score, relation, reason)) = score_binding(
                candidate,
                relation_hint,
                ext_id,
                label,
                file_path.as_deref(),
            ) {
                bindings.push(DocumentBindingEntry {
                    target_node_id: ext_id.to_string(),
                    target_label: label.to_string(),
                    relation,
                    score,
                    confidence: format!("{:?}", confidence).to_lowercase(),
                    reason,
                    file_path: file_path.clone(),
                });
            }
        }
    }
    bindings.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bindings.dedup_by(|a, b| a.target_node_id == b.target_node_id && a.relation == b.relation);
    bindings.truncate(top_k);
    Ok(bindings)
}

fn compute_drift(
    state: &SessionState,
    entry: &DocumentCacheEntry,
    bindings: &[DocumentBindingEntry],
) -> M1ndResult<DocumentDriftOutput> {
    let document = load_canonical_document(entry)?;
    let mut findings = Vec::new();
    let mut summary = DocumentDriftSummary::default();
    let graph = state.graph.read();

    if !document.code_candidates.is_empty() && bindings.is_empty() {
        summary.unbacked_claims += document.claims.len();
        findings.push(DocumentDriftFinding {
            class: "doc_claim_unbacked".into(),
            message: "document has code-oriented candidates but no resolved bindings".into(),
            confidence: "parsed".into(),
            heuristic: "zero_bindings_with_candidates".into(),
        });
    }

    for candidate in &document.code_candidates {
        let exact_matches = bindings
            .iter()
            .filter(|binding| {
                binding.target_label == candidate.label
                    || binding.target_node_id.ends_with(&candidate.label)
            })
            .map(|binding| binding.target_node_id.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len();
        if exact_matches == 0 {
            summary.missing_targets += 1;
            findings.push(DocumentDriftFinding {
                class: "binding_missing".into(),
                message: format!("no binding target resolved for {}", candidate.label),
                confidence: format!("{:?}", candidate.confidence).to_lowercase(),
                heuristic: "candidate_unresolved".into(),
            });
        } else if exact_matches > 1 {
            summary.ambiguous_targets += 1;
            findings.push(DocumentDriftFinding {
                class: "binding_ambiguous".into(),
                message: format!("multiple binding targets resolved for {}", candidate.label),
                confidence: format!("{:?}", candidate.confidence).to_lowercase(),
                heuristic: "candidate_multiple_matches".into(),
            });
        }
    }

    let mut seen_targets = std::collections::HashSet::new();
    for binding in bindings {
        if !seen_targets.insert(binding.target_node_id.clone()) {
            continue;
        }
        if let Some(node_id) = graph.resolve_id(&binding.target_node_id) {
            let idx = node_id.as_usize();
            let modified_ms = (graph.nodes.last_modified[idx] * 1000.0) as u64;
            if modified_ms > entry.updated_at_ms {
                summary.stale_bindings += 1;
                summary.code_change_unreflected += 1;
                findings.push(DocumentDriftFinding {
                    class: "code_change_unreflected".into(),
                    message: format!(
                        "bound target {} changed after document ingest",
                        binding.target_label
                    ),
                    confidence: binding.confidence.clone(),
                    heuristic: "target_newer_than_document".into(),
                });
            }
        } else {
            summary.missing_targets += 1;
            findings.push(DocumentDriftFinding {
                class: "binding_moved".into(),
                message: format!(
                    "binding target {} no longer resolves",
                    binding.target_node_id
                ),
                confidence: binding.confidence.clone(),
                heuristic: "resolved_binding_missing".into(),
            });
        }
    }

    summary.total_findings = findings.len();
    Ok(DocumentDriftOutput {
        source_path: entry.source_path.clone(),
        findings,
        summary,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn expected_node_ids(document: &CanonicalDocument, namespace: &str) -> Vec<String> {
    let mut ids = vec![format!(
        "universal::{}::doc::{}",
        namespace, document.doc_id
    )];
    ids.extend(
        document
            .sections
            .iter()
            .map(|section| format!("universal::{}::{}", namespace, section.section_id)),
    );
    ids
}

fn render_markdown(document: &CanonicalDocument) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(&document.title);
    out.push_str("\n\n");
    out.push_str("> Source: ");
    out.push_str(&document.source_path);
    out.push_str("\n\n");
    for section in &document.sections {
        out.push_str(&"#".repeat(section.level as usize));
        out.push(' ');
        out.push_str(&section.heading);
        out.push_str("\n\n");
        for block in &section.blocks {
            out.push_str(&block.text);
            out.push_str("\n\n");
        }
    }
    if document.sections.is_empty() {
        out.push_str(&document.plain_text);
    }
    out
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
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use m1nd_ingest::canonical::{
        CanonicalDocument, ClaimModality, ConfidenceLevel, DocumentClaimCandidate,
        DocumentClaimKind, DocumentCodeCandidate, DocumentEntityCandidate, DocumentEntityKind,
        DocumentMetadata, DocumentSection, DocumentSectionKind, ProvenanceSpan, SourceKind,
    };

    #[test]
    fn writes_and_resolves_document_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let doc = CanonicalDocument {
            doc_id: "canon::1".into(),
            source_path: "docs/example.md".into(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".into(),
            producer: "test".into(),
            content_hash: "hash".into(),
            title: "Example".into(),
            plain_text: "# Example\nHello".into(),
            metadata: DocumentMetadata::default(),
            sections: vec![DocumentSection {
                section_id: "section::1".into(),
                heading: "Example".into(),
                level: 1,
                kind: DocumentSectionKind::Overview,
                parent_section_id: None,
                blocks: vec![],
                provenance: ProvenanceSpan::default(),
            }],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![DocumentEntityCandidate {
                label: "TokenValidator".into(),
                kind: DocumentEntityKind::Symbol,
                aliases: vec![],
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            claims: vec![DocumentClaimCandidate {
                claim_id: "claim::1".into(),
                label: "TokenValidator must validate requests.".into(),
                kind: DocumentClaimKind::Requirement,
                modality: ClaimModality::Must,
                subject: None,
                predicate: None,
                object: None,
                negated: false,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            code_candidates: vec![DocumentCodeCandidate {
                label: "TokenValidator".into(),
                candidate_kind: DocumentEntityKind::Symbol,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = write_canonical_artifacts(temp.path(), &[doc], "universal").unwrap();
        assert_eq!(artifacts.entries.len(), 1);
        assert!(Path::new(&artifacts.entries[0].canonical_markdown_path).exists());
        assert!(Path::new(&artifacts.entries[0].canonical_json_path).exists());
        assert_eq!(artifacts.entries[0].section_count, 1);
        assert_eq!(artifacts.entries[0].entity_count, 1);
    }

    #[test]
    fn preserves_original_source_bytes_when_available() {
        let temp = tempfile::tempdir().unwrap();
        let docs_root = temp.path().join("docs");
        fs::create_dir_all(&docs_root).unwrap();
        let source = docs_root.join("provider.docx");
        let bytes = b"PK\x03\x04binary-docx-fixture";
        fs::write(&source, bytes).unwrap();

        let doc = CanonicalDocument {
            doc_id: "canon::source".into(),
            source_path: "provider.docx".into(),
            source_kind: SourceKind::Docx,
            detected_type: "docx".into(),
            producer: "test".into(),
            content_hash: "raw-hash".into(),
            title: "Provider".into(),
            plain_text: "Converted provider text".into(),
            metadata: DocumentMetadata::default(),
            sections: vec![],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![],
            claims: vec![],
            code_candidates: vec![],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };

        let artifacts = write_canonical_artifacts_with_source_root(
            temp.path(),
            Some(&docs_root),
            &[doc],
            "universal",
        )
        .unwrap();
        let entry = artifacts.entries.first().unwrap();
        assert_eq!(fs::read(&entry.original_source_path).unwrap(), bytes);

        let metadata: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&entry.metadata_path).unwrap()).unwrap();
        assert_eq!(
            metadata["source_size_bytes"].as_u64().unwrap_or_default(),
            bytes.len() as u64
        );
        assert_eq!(metadata["preserved_original_source"].as_bool(), Some(true));
    }

    #[test]
    fn provider_health_serializes() {
        let value = provider_health(DocumentProviderHealthInput {
            agent_id: "tester".into(),
        })
        .unwrap();
        assert!(value.get("providers").and_then(|v| v.as_array()).is_some());
        assert!(value.get("python").and_then(|v| v.as_str()).is_some());
    }

    #[test]
    fn resolve_entry_source_path_rejects_ambiguous_suffix_matches() {
        let mut cache = DocumentCacheState::default();
        let entry_a = DocumentCacheEntry {
            source_path: "docs/spec.md".into(),
            source_key: "a".into(),
            source_kind: "markdown".into(),
            detected_type: "markdown".into(),
            producer: "test".into(),
            node_ids: vec![],
            original_source_path: "a".into(),
            canonical_markdown_path: "cache/a/canonical.md".into(),
            canonical_json_path: "cache/a/canonical.json".into(),
            claims_path: "cache/a/claims.json".into(),
            metadata_path: "cache/a/metadata.json".into(),
            confidence_summary: HashMap::new(),
            section_count: 0,
            entity_count: 0,
            claim_count: 0,
            citation_count: 0,
            updated_at_ms: 0,
            last_binding_count: 0,
            last_drift_findings: 0,
            binding_preview: vec![],
            drift_summary: DocumentDriftSummary::default(),
            last_binding_refresh_generation: 0,
            last_drift_refresh_generation: 0,
        };
        let mut entry_b = entry_a.clone();
        entry_b.source_path = "guides/spec.md".into();
        entry_b.source_key = "b".into();
        cache.entries.insert(entry_a.source_path.clone(), entry_a);
        cache.entries.insert(entry_b.source_path.clone(), entry_b);

        let err =
            resolve_entry_source_path(&cache, "document_resolve", Some("spec.md".into()), None)
                .unwrap_err();
        assert!(err
            .to_string()
            .contains("ambiguous document path 'spec.md'"));
    }

    fn build_state(root: &Path) -> SessionState {
        let config = McpConfig {
            graph_source: root.join("graph_snapshot.json"),
            plasticity_state: root.join("plasticity_state.json"),
            runtime_dir: Some(root.to_path_buf()),
            ..McpConfig::default()
        };
        SessionState::initialize(Graph::new(), &config, DomainConfig::code()).unwrap()
    }

    #[test]
    fn bindings_and_drift_work_for_cached_document() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = build_state(temp.path());
        {
            let mut graph = state.graph.write();
            graph
                .add_node(
                    "file::src/token_validator.rs",
                    "TokenValidator",
                    m1nd_core::types::NodeType::File,
                    &["code"],
                    10.0,
                    0.1,
                )
                .unwrap();
            graph.finalize().unwrap();
        }
        let doc = CanonicalDocument {
            doc_id: "canon::1".into(),
            source_path: "docs/example.md".into(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".into(),
            producer: "test".into(),
            content_hash: "hash".into(),
            title: "Example".into(),
            plain_text: "# Example\nTokenValidator must validate requests.".into(),
            metadata: DocumentMetadata::default(),
            sections: vec![],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![DocumentEntityCandidate {
                label: "TokenValidator".into(),
                kind: DocumentEntityKind::Symbol,
                aliases: vec![],
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            claims: vec![DocumentClaimCandidate {
                claim_id: "claim::1".into(),
                label: "TokenValidator must validate requests.".into(),
                kind: DocumentClaimKind::Requirement,
                modality: ClaimModality::Must,
                subject: None,
                predicate: None,
                object: None,
                negated: false,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            code_candidates: vec![DocumentCodeCandidate {
                label: "TokenValidator".into(),
                candidate_kind: DocumentEntityKind::Symbol,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = write_canonical_artifacts(temp.path(), &[doc], "universal").unwrap();
        let entry = artifacts.entries.first().unwrap().clone();
        state
            .document_cache
            .entries
            .insert(entry.source_path.clone(), entry.clone());

        let bindings = compute_bindings(&state, &entry, 5).unwrap();
        assert!(!bindings.is_empty());
        let drift = compute_drift(&state, &entry, &bindings).unwrap();
        assert_eq!(drift.source_path, "docs/example.md");
    }

    #[test]
    fn compute_bindings_ignores_universal_self_nodes() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = build_state(temp.path());
        {
            let mut graph = state.graph.write();
            graph
                .add_node(
                    "file::src/token_validator.rs",
                    "TokenValidator",
                    m1nd_core::types::NodeType::File,
                    &["code"],
                    10.0,
                    0.1,
                )
                .unwrap();
            graph
                .add_node(
                    "universal::universal::entity::tokenvalidator",
                    "TokenValidator",
                    m1nd_core::types::NodeType::Concept,
                    &["universal"],
                    10.0,
                    0.1,
                )
                .unwrap();
            graph.finalize().unwrap();
        }
        let doc = CanonicalDocument {
            doc_id: "canon::2".into(),
            source_path: "docs/spec.md".into(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".into(),
            producer: "test".into(),
            content_hash: "hash2".into(),
            title: "Spec".into(),
            plain_text: "`TokenValidator`".into(),
            metadata: DocumentMetadata::default(),
            sections: vec![],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![DocumentEntityCandidate {
                label: "TokenValidator".into(),
                kind: DocumentEntityKind::Symbol,
                aliases: vec![],
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            claims: vec![],
            code_candidates: vec![DocumentCodeCandidate {
                label: "TokenValidator".into(),
                candidate_kind: DocumentEntityKind::Symbol,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = write_canonical_artifacts(temp.path(), &[doc], "universal").unwrap();
        let entry = artifacts.entries.first().unwrap().clone();
        state
            .document_cache
            .entries
            .insert(entry.source_path.clone(), entry.clone());

        let bindings = compute_bindings(&state, &entry, 5).unwrap();
        assert!(bindings
            .iter()
            .all(|binding| !binding.target_node_id.starts_with("universal::")));
        assert!(bindings
            .iter()
            .any(|binding| binding.target_node_id == "file::src/token_validator.rs"));
    }

    #[test]
    fn drift_reports_missing_binding_when_target_absent() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = build_state(temp.path());
        let doc = CanonicalDocument {
            doc_id: "canon::3".into(),
            source_path: "docs/spec.md".into(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".into(),
            producer: "test".into(),
            content_hash: "hash3".into(),
            title: "Spec".into(),
            plain_text: "`MissingThing` must exist.".into(),
            metadata: DocumentMetadata::default(),
            sections: vec![],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![],
            claims: vec![DocumentClaimCandidate {
                claim_id: "claim::3".into(),
                label: "MissingThing must exist.".into(),
                kind: DocumentClaimKind::Requirement,
                modality: ClaimModality::Must,
                subject: Some("MissingThing".into()),
                predicate: Some("must".into()),
                object: Some("exist".into()),
                negated: false,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            code_candidates: vec![DocumentCodeCandidate {
                label: "MissingThing".into(),
                candidate_kind: DocumentEntityKind::Symbol,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = write_canonical_artifacts(temp.path(), &[doc], "universal").unwrap();
        let entry = artifacts.entries.first().unwrap().clone();
        let bindings = compute_bindings(&state, &entry, 5).unwrap();
        assert!(bindings.is_empty());
        let drift = compute_drift(&state, &entry, &bindings).unwrap();
        assert!(drift.summary.missing_targets >= 1);
    }

    #[test]
    fn drift_reports_code_change_unreflected_for_newer_bound_target() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = build_state(temp.path());
        {
            let mut graph = state.graph.write();
            graph
                .add_node(
                    "file::src/token_validator.rs",
                    "TokenValidator",
                    m1nd_core::types::NodeType::File,
                    &["code"],
                    9999999999.0,
                    0.1,
                )
                .unwrap();
            graph.finalize().unwrap();
        }
        let doc = CanonicalDocument {
            doc_id: "canon::4".into(),
            source_path: "docs/spec.md".into(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".into(),
            producer: "test".into(),
            content_hash: "hash4".into(),
            title: "Spec".into(),
            plain_text: "`src/token_validator.rs` should stay aligned.".into(),
            metadata: DocumentMetadata::default(),
            sections: vec![],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![],
            claims: vec![],
            code_candidates: vec![DocumentCodeCandidate {
                label: "src/token_validator.rs".into(),
                candidate_kind: DocumentEntityKind::FilePath,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = write_canonical_artifacts(temp.path(), &[doc], "universal").unwrap();
        let mut entry = artifacts.entries.first().unwrap().clone();
        entry.updated_at_ms = 1;
        let bindings = compute_bindings(&state, &entry, 5).unwrap();
        let drift = compute_drift(&state, &entry, &bindings).unwrap();
        assert!(drift.summary.code_change_unreflected >= 1);
    }

    #[test]
    fn drift_reports_ambiguous_binding_when_multiple_targets_match() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = build_state(temp.path());
        {
            let mut graph = state.graph.write();
            graph
                .add_node(
                    "file::src/token_validator.rs",
                    "TokenValidator",
                    m1nd_core::types::NodeType::File,
                    &["code"],
                    10.0,
                    0.1,
                )
                .unwrap();
            graph
                .add_node(
                    "file::src/token_validator_v2.rs",
                    "TokenValidator",
                    m1nd_core::types::NodeType::File,
                    &["code"],
                    10.0,
                    0.1,
                )
                .unwrap();
            graph.finalize().unwrap();
        }
        let doc = CanonicalDocument {
            doc_id: "canon::5".into(),
            source_path: "docs/spec.md".into(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".into(),
            producer: "test".into(),
            content_hash: "hash5".into(),
            title: "Spec".into(),
            plain_text: "`TokenValidator` must validate requests.".into(),
            metadata: DocumentMetadata::default(),
            sections: vec![],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![],
            claims: vec![],
            code_candidates: vec![DocumentCodeCandidate {
                label: "TokenValidator".into(),
                candidate_kind: DocumentEntityKind::Symbol,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan::default(),
            }],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = write_canonical_artifacts(temp.path(), &[doc], "universal").unwrap();
        let entry = artifacts.entries.first().unwrap().clone();
        let bindings = compute_bindings(&state, &entry, 5).unwrap();
        let drift = compute_drift(&state, &entry, &bindings).unwrap();
        assert!(drift.summary.ambiguous_targets >= 1);
    }

    #[test]
    fn drift_does_not_flag_ambiguous_when_one_target_has_multiple_relations() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = build_state(temp.path());
        {
            let mut graph = state.graph.write();
            graph
                .add_node(
                    "file::src/token_validator.rs",
                    "TokenValidator",
                    m1nd_core::types::NodeType::File,
                    &["code"],
                    10.0,
                    0.1,
                )
                .unwrap();
            graph.finalize().unwrap();
        }
        let doc = CanonicalDocument {
            doc_id: "canon::one-target".into(),
            source_path: "docs/spec.md".into(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".into(),
            producer: "test".into(),
            content_hash: "hash-one-target".into(),
            title: "Spec".into(),
            plain_text: "`TokenValidator` must validate requests.\nSee `src/token_validator.rs`."
                .into(),
            metadata: DocumentMetadata::default(),
            sections: vec![],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![],
            claims: vec![],
            code_candidates: vec![
                DocumentCodeCandidate {
                    label: "TokenValidator".into(),
                    candidate_kind: DocumentEntityKind::Symbol,
                    confidence: ConfidenceLevel::Parsed,
                    provenance: ProvenanceSpan::default(),
                },
                DocumentCodeCandidate {
                    label: "src/token_validator.rs".into(),
                    candidate_kind: DocumentEntityKind::FilePath,
                    confidence: ConfidenceLevel::Parsed,
                    provenance: ProvenanceSpan::default(),
                },
            ],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = write_canonical_artifacts(temp.path(), &[doc], "universal").unwrap();
        let entry = artifacts.entries.first().unwrap().clone();
        let bindings = compute_bindings(&state, &entry, 8).unwrap();
        let drift = compute_drift(&state, &entry, &bindings).unwrap();
        assert_eq!(drift.summary.ambiguous_targets, 0);
    }

    #[test]
    fn drift_reports_binding_moved_for_stale_binding_target() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = build_state(temp.path());
        let doc = CanonicalDocument {
            doc_id: "canon::6".into(),
            source_path: "docs/spec.md".into(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".into(),
            producer: "test".into(),
            content_hash: "hash6".into(),
            title: "Spec".into(),
            plain_text: "`TokenValidator` must validate requests.".into(),
            metadata: DocumentMetadata::default(),
            sections: vec![],
            tables: vec![],
            links: vec![],
            citations: vec![],
            entities: vec![],
            claims: vec![],
            code_candidates: vec![],
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = write_canonical_artifacts(temp.path(), &[doc], "universal").unwrap();
        let entry = artifacts.entries.first().unwrap().clone();
        let bindings = vec![DocumentBindingEntry {
            target_node_id: "file::src/token_validator.rs".into(),
            target_label: "TokenValidator".into(),
            relation: "mentions_symbol".into(),
            score: 1.0,
            confidence: "parsed".into(),
            reason: "stale".into(),
            file_path: Some("src/token_validator.rs".into()),
        }];
        let drift = compute_drift(&state, &entry, &bindings).unwrap();
        assert!(drift.summary.missing_targets >= 1);
        assert!(drift
            .findings
            .iter()
            .any(|finding| finding.class == "binding_moved"));
    }
}
