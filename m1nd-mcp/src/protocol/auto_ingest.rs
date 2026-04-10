use serde::{Deserialize, Serialize};

fn default_auto_ingest_formats() -> Vec<String> {
    vec![
        "universal".into(),
        "light".into(),
        "article".into(),
        "bibtex".into(),
        "crossref".into(),
        "rfc".into(),
        "patent".into(),
    ]
}

fn default_debounce_ms() -> u64 {
    200
}

fn default_top_k_10() -> usize {
    10
}

#[derive(Clone, Debug, Deserialize)]
pub struct AutoIngestStartInput {
    pub agent_id: String,
    pub roots: Vec<String>,
    #[serde(default = "default_auto_ingest_formats")]
    pub formats: Vec<String>,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default)]
    pub namespace: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AutoIngestStopInput {
    pub agent_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AutoIngestStatusInput {
    pub agent_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AutoIngestTickInput {
    pub agent_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DocumentResolveInput {
    pub agent_id: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DocumentProviderHealthInput {
    pub agent_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DocumentBindingsInput {
    pub agent_id: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default = "default_top_k_10")]
    pub top_k: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DocumentDriftInput {
    pub agent_id: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoIngestEventSummary {
    pub path: String,
    pub kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutoIngestStartOutput {
    pub running: bool,
    pub backend: String,
    pub roots: Vec<String>,
    pub formats: Vec<String>,
    pub debounce_ms: u64,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub provider_status: std::collections::HashMap<String, bool>,
    pub bootstrap: AutoIngestTickOutput,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutoIngestStopOutput {
    pub stopped: bool,
    pub manifest_entries: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutoIngestStatusOutput {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_agent_id: Option<String>,
    pub backend: String,
    pub roots: Vec<String>,
    pub formats: Vec<String>,
    pub debounce_ms: u64,
    pub manifest_entries: usize,
    pub queue_depth: usize,
    pub events_seen: u64,
    pub ingests_applied: u64,
    pub removals_applied: u64,
    pub skipped_count: u64,
    pub error_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tick_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub provider_status: std::collections::HashMap<String, bool>,
    #[serde(default)]
    pub canonical_artifact_count: usize,
    #[serde(default)]
    pub semantic_document_count: usize,
    #[serde(default)]
    pub semantic_section_count: usize,
    #[serde(default)]
    pub semantic_claim_count: usize,
    #[serde(default)]
    pub semantic_entity_count: usize,
    #[serde(default)]
    pub semantic_citation_count: usize,
    #[serde(default)]
    pub drift_document_count: usize,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub provider_route_counts: std::collections::HashMap<String, usize>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub provider_fallback_counts: std::collections::HashMap<String, usize>,
    pub recent_events: Vec<AutoIngestEventSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutoIngestTickOutput {
    pub changed_paths: Vec<String>,
    pub ingested_paths: Vec<String>,
    pub removed_paths: Vec<String>,
    pub skipped_paths: Vec<String>,
    pub errored_paths: Vec<String>,
    pub queue_depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tick_ms: Option<u64>,
    pub recent_events: Vec<AutoIngestEventSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentResolveOutput {
    pub source_path: String,
    pub source_key: String,
    pub original_source_path: String,
    pub canonical_markdown_path: String,
    pub canonical_json_path: String,
    pub claims_path: String,
    pub metadata_path: String,
    pub detected_type: String,
    pub producer: String,
    pub node_ids: Vec<String>,
    pub confidence_summary: std::collections::HashMap<String, usize>,
    pub section_count: usize,
    pub claim_count: usize,
    pub entity_count: usize,
    pub citation_count: usize,
    pub binding_count: usize,
    pub binding_preview: Vec<DocumentBindingEntry>,
    pub drift_summary: DocumentDriftSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentProviderHealthEntry {
    pub name: String,
    pub available: bool,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentProviderHealthOutput {
    pub python: String,
    pub providers: Vec<DocumentProviderHealthEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentBindingEntry {
    pub target_node_id: String,
    pub target_label: String,
    pub relation: String,
    pub score: f32,
    pub confidence: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DocumentDriftFinding {
    pub class: String,
    pub message: String,
    pub confidence: String,
    pub heuristic: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DocumentDriftSummary {
    pub total_findings: usize,
    pub stale_bindings: usize,
    pub missing_targets: usize,
    pub ambiguous_targets: usize,
    pub unbacked_claims: usize,
    pub code_change_unreflected: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentBindingsOutput {
    pub source_path: String,
    pub bindings: Vec<DocumentBindingEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentDriftOutput {
    pub source_path: String,
    pub findings: Vec<DocumentDriftFinding>,
    pub summary: DocumentDriftSummary,
}
