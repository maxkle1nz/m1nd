use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    NativeLight,
    NativeArticle,
    NativeBibtex,
    NativeCrossref,
    NativeRfc,
    NativePatent,
    Markdown,
    Text,
    Html,
    Pdf,
    Docx,
    Pptx,
    Xlsx,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    Explicit,
    #[default]
    Parsed,
    Inferred,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentBlockKind {
    Heading,
    Paragraph,
    ListItem,
    Code,
    Quote,
    Table,
    Link,
    Text,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DocumentSectionKind {
    Overview,
    Api,
    Constraints,
    Tests,
    Rollout,
    Reference,
    Appendix,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DocumentEntityKind {
    Symbol,
    FilePath,
    ToolId,
    CitationId,
    CodeRef,
    NamedTerm,
    TestName,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DocumentClaimKind {
    Requirement,
    Invariant,
    Warning,
    Decision,
    TestExpectation,
    Capability,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClaimModality {
    Must,
    Should,
    May,
    Is,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub doi: Option<String>,
    pub published_at: Option<String>,
    pub language: Option<String>,
    pub extra: std::collections::HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProvenanceSpan {
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub excerpt: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DocumentSpan {
    pub text: String,
    pub kind: String,
    pub confidence: ConfidenceLevel,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentBlock {
    pub block_id: String,
    pub kind: DocumentBlockKind,
    pub text: String,
    pub confidence: ConfidenceLevel,
    pub provenance: ProvenanceSpan,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub spans: Vec<DocumentSpan>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentSection {
    pub section_id: String,
    pub heading: String,
    pub level: u8,
    #[serde(default)]
    pub kind: DocumentSectionKind,
    #[serde(default)]
    pub parent_section_id: Option<String>,
    pub blocks: Vec<DocumentBlock>,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DocumentTableCell {
    pub text: String,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DocumentTableRow {
    pub cells: Vec<DocumentTableCell>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DocumentTable {
    pub table_id: String,
    pub headers: Vec<String>,
    pub rows: Vec<DocumentTableRow>,
    pub confidence: ConfidenceLevel,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentLink {
    pub label: String,
    pub target: String,
    pub confidence: ConfidenceLevel,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentCitation {
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub citation_kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub venue: Option<String>,
    #[serde(default)]
    pub year: Option<String>,
    pub confidence: ConfidenceLevel,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentEntityCandidate {
    pub label: String,
    pub kind: DocumentEntityKind,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub confidence: ConfidenceLevel,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentClaimCandidate {
    pub claim_id: String,
    pub label: String,
    #[serde(default)]
    pub kind: DocumentClaimKind,
    #[serde(default)]
    pub modality: ClaimModality,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub predicate: Option<String>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub negated: bool,
    pub confidence: ConfidenceLevel,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DocumentCodeCandidate {
    pub label: String,
    pub candidate_kind: DocumentEntityKind,
    pub confidence: ConfidenceLevel,
    pub provenance: ProvenanceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CanonicalDocument {
    pub doc_id: String,
    pub source_path: String,
    pub source_kind: SourceKind,
    pub detected_type: String,
    pub producer: String,
    pub content_hash: String,
    pub title: String,
    pub plain_text: String,
    pub metadata: DocumentMetadata,
    pub sections: Vec<DocumentSection>,
    #[serde(default)]
    pub tables: Vec<DocumentTable>,
    pub links: Vec<DocumentLink>,
    pub citations: Vec<DocumentCitation>,
    pub entities: Vec<DocumentEntityCandidate>,
    pub claims: Vec<DocumentClaimCandidate>,
    #[serde(default)]
    pub code_candidates: Vec<DocumentCodeCandidate>,
    pub confidence: ConfidenceLevel,
    #[serde(default)]
    pub structured_origin: serde_json::Value,
}

pub fn short_hash(input: &str) -> String {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn short_hash_bytes(input: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn source_key(source_path: &str) -> String {
    short_hash(source_path)
}

pub fn source_kind_from_extension(path: &Path) -> SourceKind {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("md" | "markdown") => SourceKind::Markdown,
        Some("txt" | "rst" | "adoc") => SourceKind::Text,
        Some("html" | "htm") => SourceKind::Html,
        Some("pdf") => SourceKind::Pdf,
        Some("docx") => SourceKind::Docx,
        Some("pptx") => SourceKind::Pptx,
        Some("xlsx") => SourceKind::Xlsx,
        _ => SourceKind::Unknown,
    }
}
