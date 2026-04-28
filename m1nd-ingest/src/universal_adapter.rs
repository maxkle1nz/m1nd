use crate::canonical::{
    short_hash, short_hash_bytes, source_kind_from_extension, CanonicalDocument, ClaimModality,
    ConfidenceLevel, DocumentBlock, DocumentBlockKind, DocumentCitation, DocumentClaimCandidate,
    DocumentClaimKind, DocumentCodeCandidate, DocumentEntityCandidate, DocumentEntityKind,
    DocumentLink, DocumentMetadata, DocumentSection, DocumentSectionKind, DocumentSpan,
    DocumentTable, DocumentTableCell, DocumentTableRow, ProvenanceSpan, SourceKind,
};
use crate::{extension_of, relative_source_path};
use crate::{
    BibTexAdapter, CrossRefAdapter, IngestAdapter, IngestStats, JatsArticleAdapter,
    L1ghtIngestAdapter, PatentIngestAdapter, RfcAdapter,
};
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProviderAvailability {
    pub magika: bool,
    pub docling: bool,
    pub markitdown: bool,
    pub trafilatura: bool,
    pub grobid: bool,
    pub marker: bool,
    pub mineru: bool,
}

pub struct UniversalIngestBundle {
    pub graph: Graph,
    pub stats: IngestStats,
    pub documents: Vec<CanonicalDocument>,
}

pub struct UniversalIngestAdapter {
    namespace: String,
}

impl UniversalIngestAdapter {
    pub fn new(namespace: Option<String>) -> Self {
        Self {
            namespace: namespace.unwrap_or_else(|| "universal".to_string()),
        }
    }

    pub fn provider_availability() -> ProviderAvailability {
        ProviderAvailability {
            magika: python_module_available("magika"),
            docling: python_module_available("docling"),
            markitdown: python_module_available("markitdown"),
            trafilatura: python_module_available("trafilatura"),
            grobid: grobid_configured(),
            marker: command_available("marker"),
            mineru: command_available("mineru"),
        }
    }

    pub fn provider_python_command() -> String {
        provider_python()
    }

    pub fn can_handle_path(path: &Path) -> bool {
        let ext = extension_of(path);
        matches!(
            ext.as_str(),
            "md" | "markdown"
                | "txt"
                | "rst"
                | "adoc"
                | "html"
                | "htm"
                | "pdf"
                | "docx"
                | "pptx"
                | "xlsx"
                | "xml"
                | "nxml"
                | "json"
                | "bib"
                | "bibtex"
        )
    }

    pub fn ingest_bundle(&self, root: &Path) -> M1ndResult<UniversalIngestBundle> {
        let start = std::time::Instant::now();
        let mut stats = IngestStats::default();
        let files = collect_candidate_files(root);
        stats.files_scanned = files.len() as u64;

        let mut documents = Vec::new();
        for path in files {
            if let Some(document) = self.canonicalize_path(root, &path)? {
                stats.files_parsed += 1;
                documents.push(document);
            }
        }

        let graph = graphify_documents(&documents, &self.namespace)?;
        stats.nodes_created = graph.num_nodes() as u64;
        stats.edges_created = graph.num_edges() as u64;
        stats.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(UniversalIngestBundle {
            graph,
            stats,
            documents,
        })
    }

    fn canonicalize_path(&self, root: &Path, path: &Path) -> M1ndResult<Option<CanonicalDocument>> {
        let rel_path = relative_source_path(root, path);
        let source_kind = source_kind_from_extension(path);
        let availability = Self::provider_availability();
        let bytes = fs::read(path).map_err(|error| M1ndError::InvalidParams {
            tool: "universal_ingest".into(),
            detail: format!("failed to read {}: {}", path.display(), error),
        })?;
        let source_text = String::from_utf8_lossy(&bytes).to_string();

        let mut document = match source_kind {
            SourceKind::Markdown | SourceKind::Text => {
                canonicalize_plain_text(&rel_path, source_kind, "universal:internal", &source_text)
            }
            SourceKind::Html => {
                if availability.trafilatura {
                    canonicalize_html_with_fallback(
                        &rel_path,
                        "universal:trafilatura",
                        "universal:internal-html",
                        trafilatura_extract(path).as_deref().unwrap_or(&source_text),
                    )
                } else if availability.docling {
                    let extracted = docling_extract(path);
                    canonicalize_html_with_fallback(
                        &rel_path,
                        "universal:docling",
                        "universal:internal-html",
                        extracted.as_deref().unwrap_or(&source_text),
                    )
                } else {
                    canonicalize_html_with_fallback(
                        &rel_path,
                        "universal:internal-html",
                        "universal:internal-html",
                        &source_text,
                    )
                }
            }
            SourceKind::Pdf | SourceKind::Docx | SourceKind::Pptx | SourceKind::Xlsx
                if availability.docling || availability.markitdown =>
            {
                let extracted = if matches!(source_kind, SourceKind::Pdf) && availability.grobid {
                    grobid_extract(path)
                } else if availability.docling {
                    docling_extract(path)
                } else if availability.markitdown {
                    markitdown_extract(path)
                } else {
                    None
                };
                canonicalize_binary_placeholder(
                    &rel_path,
                    source_kind.clone(),
                    if matches!(source_kind, SourceKind::Pdf) && availability.grobid {
                        "universal:grobid"
                    } else if availability.docling {
                        "universal:docling"
                    } else {
                        "universal:markitdown"
                    },
                    extracted.as_deref().unwrap_or(&source_text),
                )
            }
            SourceKind::Pdf | SourceKind::Docx | SourceKind::Pptx | SourceKind::Xlsx => {
                return Ok(None);
            }
            SourceKind::Unknown => {
                if let Some(native) =
                    self.wrap_native_document(root, path, &rel_path, &source_text)?
                {
                    native
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        };
        document.content_hash = short_hash_bytes(&bytes);

        Ok(Some(document))
    }

    fn wrap_native_document(
        &self,
        _root: &Path,
        path: &Path,
        rel_path: &str,
        source_text: &str,
    ) -> M1ndResult<Option<CanonicalDocument>> {
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        let native_kind = if matches!(ext.as_str(), "md" | "markdown")
            && L1ghtIngestAdapter::looks_like_l1ght(source_text)
        {
            Some(SourceKind::NativeLight)
        } else if matches!(ext.as_str(), "bib" | "bibtex") {
            Some(SourceKind::NativeBibtex)
        } else if matches!(ext.as_str(), "xml" | "nxml") {
            if source_text.contains("<PubmedArticle")
                || source_text.contains("<PubmedArticleSet")
                || source_text.contains("NLM//DTD")
                || (source_text.contains("<article") && source_text.contains("dtd-version"))
            {
                Some(SourceKind::NativeArticle)
            } else if source_text.contains("<rfc ") || source_text.contains("<rfc>") {
                Some(SourceKind::NativeRfc)
            } else if source_text.contains("<us-patent-grant")
                || source_text.contains("<us-patent-application")
                || source_text.contains("<ep-patent-document")
            {
                Some(SourceKind::NativePatent)
            } else {
                None
            }
        } else if ext == "json"
            && source_text.contains("\"DOI\"")
            && source_text.contains("\"publisher\"")
            && source_text.contains("\"type\"")
        {
            Some(SourceKind::NativeCrossref)
        } else {
            None
        };

        let Some(native_kind) = native_kind else {
            return Ok(None);
        };

        Ok(Some(canonicalize_plain_text(
            rel_path,
            native_kind,
            "universal:native-wrap",
            source_text,
        )))
    }
}

impl IngestAdapter for UniversalIngestAdapter {
    fn domain(&self) -> &str {
        "universal"
    }

    fn ingest(&self, root: &Path) -> M1ndResult<(Graph, IngestStats)> {
        let bundle = self.ingest_bundle(root)?;
        Ok((bundle.graph, bundle.stats))
    }
}

fn python_module_available(module_name: &str) -> bool {
    let output = Command::new(provider_python())
        .arg("-c")
        .arg(format!("import importlib.util; print('1' if importlib.util.find_spec('{module_name}') else '0')"))
        .output();
    matches!(output, Ok(result) if String::from_utf8_lossy(&result.stdout).trim() == "1")
}

fn command_available(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn grobid_configured() -> bool {
    std::env::var("M1ND_GROBID_URL")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn python_inline(script: &str, arg: &Path) -> Option<String> {
    let output = Command::new(provider_python())
        .arg("-c")
        .arg(script)
        .arg(arg)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn provider_python() -> String {
    std::env::var("M1ND_PROVIDER_PYTHON").unwrap_or_else(|_| "python3".to_string())
}

pub fn provider_python_command() -> String {
    provider_python()
}

fn docling_extract(path: &Path) -> Option<String> {
    python_inline(
        r#"
import sys
from docling.document_converter import DocumentConverter
converter = DocumentConverter()
result = converter.convert(sys.argv[1])
doc = getattr(result, 'document', None)
if doc is not None and hasattr(doc, 'export_to_markdown'):
    text = doc.export_to_markdown()
    if text:
        print(text)
"#,
        path,
    )
}

fn trafilatura_extract(path: &Path) -> Option<String> {
    python_inline(
        r#"
import sys, pathlib, trafilatura
path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding='utf-8', errors='ignore')
extracted = trafilatura.extract(text, output_format='markdown', include_links=True, include_tables=True)
if extracted:
    print(extracted)
"#,
        path,
    )
}

fn markitdown_extract(path: &Path) -> Option<String> {
    python_inline(
        r#"
import sys
from markitdown import MarkItDown
md = MarkItDown()
result = md.convert(sys.argv[1])
text = getattr(result, 'text_content', None) or getattr(result, 'markdown', None) or str(result)
if text:
    print(text)
"#,
        path,
    )
}

fn grobid_extract(path: &Path) -> Option<String> {
    let url = std::env::var("M1ND_GROBID_URL").ok()?;
    let script = format!(
        r#"
import sys, requests
path = sys.argv[1]
url = {url_repr}.rstrip('/') + '/api/processFulltextDocument'
with open(path, 'rb') as fh:
    resp = requests.post(url, files={{'input': fh}}, timeout=30)
resp.raise_for_status()
text = resp.text.strip()
if text:
    print(text)
"#,
        url_repr = serde_json::to_string(&url).ok()?
    );
    python_inline(&script, path)
}

fn collect_candidate_files(root: &Path) -> Vec<PathBuf> {
    if root.is_file() {
        return (!is_universal_noise_path(root))
            .then(|| root.to_path_buf())
            .into_iter()
            .collect();
    }

    WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| !is_universal_noise_path(entry.path()))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect()
}

fn is_universal_noise_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    name == "node_modules"
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
}

fn slugify(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn content_hash(input: &str) -> String {
    short_hash(input)
}

fn default_title(source_path: &str, plain_text: &str) -> String {
    plain_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| {
            Path::new(source_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(source_path)
                .to_string()
        })
}

fn section_id(source_path: &str, heading: &str, index: usize) -> String {
    format!(
        "section::{}::{}-{}",
        short_hash(source_path),
        slugify(heading),
        index
    )
}

fn block_id(source_path: &str, heading: &str, index: usize) -> String {
    format!(
        "block::{}::{}-{}",
        short_hash(source_path),
        slugify(heading),
        index
    )
}

fn claim_id(source_path: &str, label: &str, index: usize) -> String {
    format!(
        "claim::{}::{}-{}",
        short_hash(source_path),
        slugify(label),
        index
    )
}

fn classify_section_kind(heading: &str) -> DocumentSectionKind {
    let lower = heading.to_ascii_lowercase();
    if lower.contains("api") || lower.contains("contract") || lower.contains("interface") {
        DocumentSectionKind::Api
    } else if lower.contains("constraint") || lower.contains("invariant") || lower.contains("guard")
    {
        DocumentSectionKind::Constraints
    } else if lower.contains("test") || lower.contains("verification") {
        DocumentSectionKind::Tests
    } else if lower.contains("rollout") || lower.contains("migration") {
        DocumentSectionKind::Rollout
    } else if lower.contains("reference") || lower.contains("bibliography") {
        DocumentSectionKind::Reference
    } else if lower.contains("appendix") {
        DocumentSectionKind::Appendix
    } else if lower.contains("overview")
        || lower.contains("introduction")
        || lower.contains("summary")
    {
        DocumentSectionKind::Overview
    } else {
        DocumentSectionKind::Unknown
    }
}

fn classify_entity_kind(label: &str) -> DocumentEntityKind {
    if label.contains("m1nd.") {
        DocumentEntityKind::ToolId
    } else if label.contains('/')
        || label.contains(".rs")
        || label.contains(".py")
        || label.contains(".ts")
        || label.contains(".md")
    {
        DocumentEntityKind::FilePath
    } else if label.contains("::") || label.contains('.') {
        DocumentEntityKind::Symbol
    } else if label.to_ascii_lowercase().contains("test") {
        DocumentEntityKind::TestName
    } else {
        DocumentEntityKind::NamedTerm
    }
}

fn classify_claim(line: &str) -> (DocumentClaimKind, ClaimModality, bool) {
    let lower = line.to_ascii_lowercase();
    let modality = if lower.contains(" must ") || lower.starts_with("must ") {
        ClaimModality::Must
    } else if lower.contains(" should ") || lower.starts_with("should ") {
        ClaimModality::Should
    } else if lower.contains(" may ") || lower.starts_with("may ") {
        ClaimModality::May
    } else if lower.contains(" is ") || lower.starts_with("is ") {
        ClaimModality::Is
    } else {
        ClaimModality::Unknown
    };
    let negated = lower.contains(" not ") || lower.contains(" no ");
    let kind = if lower.contains("warning") || lower.contains("danger") || lower.contains("risk") {
        DocumentClaimKind::Warning
    } else if lower.contains("test") || lower.contains("assert") || lower.contains("expect") {
        DocumentClaimKind::TestExpectation
    } else if matches!(modality, ClaimModality::Must | ClaimModality::Should) {
        DocumentClaimKind::Requirement
    } else if lower.contains("decision") || lower.contains("we choose") || lower.contains("chosen")
    {
        DocumentClaimKind::Decision
    } else if lower.contains("always") || lower.contains("never") || lower.contains("invariant") {
        DocumentClaimKind::Invariant
    } else if lower.contains("can ") || lower.contains("supports") || lower.contains("capability") {
        DocumentClaimKind::Capability
    } else {
        DocumentClaimKind::Unknown
    };
    (kind, modality, negated)
}

fn canonicalize_plain_text(
    source_path: &str,
    source_kind: SourceKind,
    producer: &str,
    raw_text: &str,
) -> CanonicalDocument {
    let heading_re = Regex::new(r"^(#{1,6})\s+(.+?)\s*$").unwrap();
    let link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    let doi_re = Regex::new(r"(10\.\d{4,9}/[-._;()/:A-Za-z0-9]+)").unwrap();
    let code_ref_re = Regex::new(r"`([^`]+)`").unwrap();
    let entity_re = Regex::new(r"\b([A-Z][A-Za-z0-9_.:-]{3,})\b").unwrap();

    let mut sections = Vec::new();
    let mut links = Vec::new();
    let mut citations = Vec::new();
    let mut entities = Vec::new();
    let mut claims = Vec::new();
    let mut code_candidates = Vec::new();
    let mut tables = Vec::new();
    let mut current_heading = "Document".to_string();
    let mut current_level = 1u8;
    let mut current_parent_section_id: Option<String> = None;
    let mut current_blocks = Vec::new();
    let mut section_index = 0usize;
    let mut block_index = 0usize;
    let mut claim_index = 0usize;
    let mut seen_entities = HashSet::new();
    let mut seen_links = HashSet::new();
    let mut seen_citations = HashSet::new();
    let mut seen_code_candidates = HashSet::new();
    let mut section_stack: Vec<(u8, String)> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang: Option<String> = None;

    let mut flush_section = |sections: &mut Vec<DocumentSection>,
                             current_heading: &mut String,
                             current_level: &mut u8,
                             current_parent_section_id: &mut Option<String>,
                             current_blocks: &mut Vec<DocumentBlock>,
                             section_index: &mut usize| {
        if current_blocks.is_empty() {
            return;
        }
        *section_index += 1;
        sections.push(DocumentSection {
            section_id: section_id(source_path, current_heading, *section_index),
            heading: current_heading.clone(),
            level: *current_level,
            kind: classify_section_kind(current_heading),
            parent_section_id: current_parent_section_id.clone(),
            blocks: std::mem::take(current_blocks),
            provenance: ProvenanceSpan {
                line_start: None,
                line_end: None,
                excerpt: Some(current_heading.clone()),
            },
        });
    };

    for (idx, line) in raw_text.lines().enumerate() {
        let line_no = idx as u32 + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = heading_re.captures(trimmed) {
            flush_section(
                &mut sections,
                &mut current_heading,
                &mut current_level,
                &mut current_parent_section_id,
                &mut current_blocks,
                &mut section_index,
            );
            current_heading = caps.get(2).unwrap().as_str().trim().to_string();
            current_level = caps.get(1).unwrap().as_str().len() as u8;
            while section_stack
                .last()
                .is_some_and(|(level, _)| *level >= current_level)
            {
                section_stack.pop();
            }
            current_parent_section_id = section_stack.last().map(|(_, id)| id.clone());
            section_stack.push((
                current_level,
                section_id(source_path, &current_heading, section_index + 1),
            ));
            continue;
        }

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            code_lang = trimmed
                .strip_prefix("```")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string());
        }

        let is_table_line = trimmed.contains('|')
            && trimmed.matches('|').count() >= 2
            && !trimmed.starts_with("http");
        if is_table_line {
            let cells = trimmed
                .trim_matches('|')
                .split('|')
                .map(|cell| DocumentTableCell {
                    text: cell.trim().to_string(),
                    provenance: ProvenanceSpan {
                        line_start: Some(line_no),
                        line_end: Some(line_no),
                        excerpt: Some(trimmed.to_string()),
                    },
                })
                .collect::<Vec<_>>();
            let table_id = format!("table::{}::{}", short_hash(source_path), tables.len() + 1);
            let headers = if tables.last().is_none() {
                cells.iter().map(|cell| cell.text.clone()).collect()
            } else {
                Vec::new()
            };
            tables.push(DocumentTable {
                table_id,
                headers,
                rows: vec![DocumentTableRow { cells }],
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan {
                    line_start: Some(line_no),
                    line_end: Some(line_no),
                    excerpt: Some(trimmed.to_string()),
                },
            });
        }

        let kind = if in_code_block {
            DocumentBlockKind::Code
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            DocumentBlockKind::ListItem
        } else if trimmed.starts_with('>') {
            DocumentBlockKind::Quote
        } else if is_table_line {
            DocumentBlockKind::Table
        } else {
            DocumentBlockKind::Paragraph
        };

        let mut spans = Vec::new();
        block_index += 1;
        current_blocks.push(DocumentBlock {
            block_id: block_id(source_path, &current_heading, block_index),
            kind,
            text: trimmed.to_string(),
            confidence: ConfidenceLevel::Parsed,
            provenance: ProvenanceSpan {
                line_start: Some(line_no),
                line_end: Some(line_no),
                excerpt: Some(trimmed.chars().take(200).collect()),
            },
            language: code_lang.clone().filter(|_| in_code_block),
            spans: Vec::new(),
        });

        for caps in link_re.captures_iter(trimmed) {
            let label = caps.get(1).unwrap().as_str().to_string();
            let target = caps.get(2).unwrap().as_str().to_string();
            let key = format!("{label}|{target}");
            if seen_links.insert(key) {
                links.push(DocumentLink {
                    label,
                    target: target.clone(),
                    confidence: ConfidenceLevel::Parsed,
                    provenance: ProvenanceSpan {
                        line_start: Some(line_no),
                        line_end: Some(line_no),
                        excerpt: Some(trimmed.to_string()),
                    },
                });
            }
            spans.push(DocumentSpan {
                text: target,
                kind: "link".into(),
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan {
                    line_start: Some(line_no),
                    line_end: Some(line_no),
                    excerpt: Some(trimmed.to_string()),
                },
            });
        }

        for caps in doi_re.captures_iter(trimmed) {
            let target = caps.get(1).unwrap().as_str().to_string();
            if seen_citations.insert(target.clone()) {
                citations.push(DocumentCitation {
                    label: target.clone(),
                    target: target.clone(),
                    citation_kind: "paper".into(),
                    title: None,
                    authors: Vec::new(),
                    venue: None,
                    year: None,
                    confidence: ConfidenceLevel::Parsed,
                    provenance: ProvenanceSpan {
                        line_start: Some(line_no),
                        line_end: Some(line_no),
                        excerpt: Some(trimmed.to_string()),
                    },
                });
            }
            spans.push(DocumentSpan {
                text: target,
                kind: "doi".into(),
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan {
                    line_start: Some(line_no),
                    line_end: Some(line_no),
                    excerpt: Some(trimmed.to_string()),
                },
            });
        }

        for caps in code_ref_re.captures_iter(trimmed) {
            let label = caps.get(1).unwrap().as_str().trim().to_string();
            if !label.is_empty() && seen_entities.insert(label.clone()) {
                entities.push(DocumentEntityCandidate {
                    label: label.clone(),
                    kind: DocumentEntityKind::CodeRef,
                    aliases: Vec::new(),
                    confidence: ConfidenceLevel::Parsed,
                    provenance: ProvenanceSpan {
                        line_start: Some(line_no),
                        line_end: Some(line_no),
                        excerpt: Some(trimmed.to_string()),
                    },
                });
            }
            if !label.is_empty() && seen_code_candidates.insert(label.clone()) {
                code_candidates.push(DocumentCodeCandidate {
                    label: label.clone(),
                    candidate_kind: classify_entity_kind(&label),
                    confidence: ConfidenceLevel::Parsed,
                    provenance: ProvenanceSpan {
                        line_start: Some(line_no),
                        line_end: Some(line_no),
                        excerpt: Some(trimmed.to_string()),
                    },
                });
            }
            spans.push(DocumentSpan {
                text: label,
                kind: "code_ref".into(),
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan {
                    line_start: Some(line_no),
                    line_end: Some(line_no),
                    excerpt: Some(trimmed.to_string()),
                },
            });
        }

        for caps in entity_re.captures_iter(trimmed) {
            let label = caps.get(1).unwrap().as_str().trim().to_string();
            if seen_entities.insert(label.clone()) {
                entities.push(DocumentEntityCandidate {
                    label,
                    kind: classify_entity_kind(caps.get(1).unwrap().as_str().trim()),
                    aliases: Vec::new(),
                    confidence: ConfidenceLevel::Inferred,
                    provenance: ProvenanceSpan {
                        line_start: Some(line_no),
                        line_end: Some(line_no),
                        excerpt: Some(trimmed.to_string()),
                    },
                });
            }
        }

        if trimmed.ends_with('.') && trimmed.len() > 20 {
            claim_index += 1;
            let (kind, modality, negated) = classify_claim(trimmed);
            let subject = code_ref_re
                .captures(trimmed)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
                .or_else(|| {
                    entity_re
                        .captures(trimmed)
                        .and_then(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
                });
            let predicate = match modality {
                ClaimModality::Must => Some("must".to_string()),
                ClaimModality::Should => Some("should".to_string()),
                ClaimModality::May => Some("may".to_string()),
                ClaimModality::Is => Some("is".to_string()),
                ClaimModality::Unknown => None,
            };
            let object = predicate.as_ref().and_then(|verb| {
                let needle = format!(" {} ", verb);
                trimmed
                    .to_ascii_lowercase()
                    .find(&needle)
                    .map(|idx| {
                        trimmed[idx + needle.len()..]
                            .trim_end_matches('.')
                            .trim()
                            .to_string()
                    })
                    .filter(|value| !value.is_empty())
            });
            claims.push(DocumentClaimCandidate {
                claim_id: claim_id(source_path, trimmed, claim_index),
                label: trimmed.to_string(),
                kind,
                modality,
                subject,
                predicate,
                object,
                negated,
                confidence: ConfidenceLevel::Parsed,
                provenance: ProvenanceSpan {
                    line_start: Some(line_no),
                    line_end: Some(line_no),
                    excerpt: Some(trimmed.to_string()),
                },
            });
        }

        if let Some(last) = current_blocks.last_mut() {
            last.spans = spans;
        }
    }

    flush_section(
        &mut sections,
        &mut current_heading,
        &mut current_level,
        &mut current_parent_section_id,
        &mut current_blocks,
        &mut section_index,
    );

    let title = default_title(source_path, raw_text);
    let mut metadata = DocumentMetadata {
        title: Some(title.clone()),
        ..Default::default()
    };
    if let Some(first) = citations.first() {
        metadata.doi = Some(first.target.clone());
    }

    CanonicalDocument {
        doc_id: format!("canon::{}", short_hash(source_path)),
        source_path: source_path.to_string(),
        source_kind: source_kind.clone(),
        detected_type: match source_kind {
            SourceKind::Markdown => "markdown".into(),
            SourceKind::Text => "text".into(),
            SourceKind::Html => "html".into(),
            SourceKind::NativeLight => "light".into(),
            SourceKind::NativeArticle => "article".into(),
            SourceKind::NativeBibtex => "bibtex".into(),
            SourceKind::NativeCrossref => "crossref".into(),
            SourceKind::NativeRfc => "rfc".into(),
            SourceKind::NativePatent => "patent".into(),
            other => format!("{:?}", other).to_lowercase(),
        },
        producer: producer.to_string(),
        content_hash: content_hash(raw_text),
        title,
        plain_text: raw_text.to_string(),
        metadata,
        sections,
        tables,
        links,
        citations,
        entities,
        claims,
        code_candidates,
        confidence: ConfidenceLevel::Parsed,
        structured_origin: serde_json::json!({ "producer": producer }),
    }
}

fn strip_html_tags(input: &str) -> String {
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let heading_re = Regex::new(r"(?i)<h([1-6])[^>]*>(.*?)</h[1-6]>").unwrap();
    let mut text = input.to_string();
    for caps in heading_re.captures_iter(input) {
        let level = caps.get(1).unwrap().as_str();
        let body = tag_re.replace_all(caps.get(2).unwrap().as_str(), "");
        text = text.replace(
            caps.get(0).unwrap().as_str(),
            &format!(
                "\n{} {}\n",
                "#".repeat(level.parse::<usize>().unwrap_or(1)),
                body.trim()
            ),
        );
    }
    tag_re.replace_all(&text, " ").to_string()
}

fn canonicalize_html_with_fallback(
    source_path: &str,
    producer: &str,
    fallback_producer: &str,
    raw_text: &str,
) -> CanonicalDocument {
    let text = strip_html_tags(raw_text);
    let producer = if text.trim().is_empty() {
        fallback_producer
    } else {
        producer
    };
    canonicalize_plain_text(source_path, SourceKind::Html, producer, &text)
}

fn canonicalize_binary_placeholder(
    source_path: &str,
    source_kind: SourceKind,
    producer: &str,
    source_text: &str,
) -> CanonicalDocument {
    let fallback = format!(
        "# Imported Document\n\nSource: {}\n\nThis document was detected and reserved for optional provider-based canonicalization.\n",
        source_path
    );
    let text = if source_text.trim().is_empty() {
        fallback
    } else {
        source_text.to_string()
    };
    canonicalize_plain_text(source_path, source_kind, producer, &text)
}

pub fn graphify_documents(documents: &[CanonicalDocument], namespace: &str) -> M1ndResult<Graph> {
    let mut graph = Graph::with_capacity(documents.len() * 24, documents.len() * 48);
    let mut entity_nodes = HashSet::new();
    let mut citation_nodes = HashSet::new();
    let mut binding_nodes = HashSet::new();

    for document in documents {
        let doc_id = format!("universal::{}::doc::{}", namespace, document.doc_id);
        let doc_tags = [
            "universal".to_string(),
            format!("universal:type:{}", document.detected_type),
            format!("namespace:{}", namespace),
            format!("producer:{}", document.producer),
        ];
        let doc_tag_refs: Vec<&str> = doc_tags.iter().map(String::as_str).collect();
        let doc_node = graph.add_node(
            &doc_id,
            &document.title,
            NodeType::File,
            &doc_tag_refs,
            0.0,
            0.5,
        )?;
        graph.set_node_provenance(
            doc_node,
            NodeProvenanceInput {
                source_path: Some(&document.source_path),
                excerpt: Some(&document.title),
                namespace: Some(namespace),
                canonical: true,
                ..Default::default()
            },
        );

        for section in &document.sections {
            let section_id = format!("universal::{}::{}", namespace, section.section_id);
            let section_tags = [
                "universal".to_string(),
                "universal:section".to_string(),
                format!("section:kind:{:?}", section.kind).to_lowercase(),
            ];
            let section_refs: Vec<&str> = section_tags.iter().map(String::as_str).collect();
            let section_node = graph.add_node(
                &section_id,
                &section.heading,
                NodeType::Module,
                &section_refs,
                0.0,
                0.4,
            )?;
            graph.set_node_provenance(
                section_node,
                NodeProvenanceInput {
                    source_path: Some(&document.source_path),
                    line_start: section.provenance.line_start,
                    line_end: section.provenance.line_end,
                    excerpt: section.provenance.excerpt.as_deref(),
                    namespace: Some(namespace),
                    canonical: true,
                },
            );
            graph.add_edge(
                doc_node,
                section_node,
                "contains_section",
                FiniteF32::ONE,
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.8),
            )?;
            if let Some(parent_id) = &section.parent_section_id {
                let graph_parent_id = format!("universal::{}::{}", namespace, parent_id);
                if let Some(parent_node) = graph.resolve_id(&graph_parent_id) {
                    graph.add_edge(
                        section_node,
                        parent_node,
                        "subsection_of",
                        FiniteF32::new(0.7),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.5),
                    )?;
                }
            }

            for block in &section.blocks {
                let block_node_id = format!("universal::{}::{}", namespace, block.block_id);
                let (node_type, relation, kind_tag) = match block.kind {
                    DocumentBlockKind::Code => {
                        (NodeType::Module, "contains_code", "universal:code")
                    }
                    DocumentBlockKind::Table => {
                        (NodeType::System, "contains_table", "universal:table")
                    }
                    _ => (NodeType::Concept, "contains_block", "universal:block"),
                };
                let block_tags = [
                    "universal".to_string(),
                    kind_tag.to_string(),
                    format!("confidence:{:?}", block.confidence).to_lowercase(),
                ];
                let block_refs: Vec<&str> = block_tags.iter().map(String::as_str).collect();
                let block_node = graph.add_node(
                    &block_node_id,
                    &block.text.chars().take(80).collect::<String>(),
                    node_type,
                    &block_refs,
                    0.0,
                    0.3,
                )?;
                graph.set_node_provenance(
                    block_node,
                    NodeProvenanceInput {
                        source_path: Some(&document.source_path),
                        line_start: block.provenance.line_start,
                        line_end: block.provenance.line_end,
                        excerpt: block.provenance.excerpt.as_deref(),
                        namespace: Some(namespace),
                        canonical: true,
                    },
                );
                graph.add_edge(
                    section_node,
                    block_node,
                    relation,
                    FiniteF32::new(0.9),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.6),
                )?;
            }
        }

        for table in &document.tables {
            let table_id = format!("universal::{}::{}", namespace, table.table_id);
            let table_tags = ["universal", "universal:table"];
            let table_node = graph.add_node(
                &table_id,
                &format!("Table {}", table.table_id),
                NodeType::System,
                &table_tags,
                0.0,
                0.35,
            )?;
            graph.set_node_provenance(
                table_node,
                NodeProvenanceInput {
                    source_path: Some(&document.source_path),
                    line_start: table.provenance.line_start,
                    line_end: table.provenance.line_end,
                    excerpt: table.provenance.excerpt.as_deref(),
                    namespace: Some(namespace),
                    canonical: true,
                },
            );
            graph.add_edge(
                doc_node,
                table_node,
                "contains_table",
                FiniteF32::new(0.85),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.6),
            )?;
        }

        for entity in &document.entities {
            let entity_id = format!(
                "universal::{}::entity::{}",
                namespace,
                slugify(&entity.label)
            );
            if entity_nodes.insert(entity_id.clone()) {
                let tags = [
                    "universal".to_string(),
                    format!("entity:kind:{:?}", entity.kind).to_lowercase(),
                    format!("confidence:{:?}", entity.confidence).to_lowercase(),
                ];
                let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
                let entity_node = graph.add_node(
                    &entity_id,
                    &entity.label,
                    NodeType::Concept,
                    &tag_refs,
                    0.0,
                    0.35,
                )?;
                graph.set_node_provenance(
                    entity_node,
                    NodeProvenanceInput {
                        source_path: Some(&document.source_path),
                        line_start: entity.provenance.line_start,
                        line_end: entity.provenance.line_end,
                        excerpt: entity.provenance.excerpt.as_deref(),
                        namespace: Some(namespace),
                        canonical: true,
                    },
                );
            }

            if let Some(entity_node) = graph.resolve_id(&entity_id) {
                graph.add_edge(
                    doc_node,
                    entity_node,
                    "declares_entity",
                    FiniteF32::new(0.9),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.7),
                )?;
            }
        }

        for citation in &document.citations {
            let citation_id = format!(
                "universal::{}::citation::{}",
                namespace,
                slugify(&citation.target)
            );
            if citation_nodes.insert(citation_id.clone()) {
                let tags = [
                    "universal".to_string(),
                    "universal:citation".to_string(),
                    format!("target:{}", citation.target),
                ];
                let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
                let citation_node = graph.add_node(
                    &citation_id,
                    &citation.label,
                    NodeType::Reference,
                    &tag_refs,
                    0.0,
                    0.25,
                )?;
                graph.set_node_provenance(
                    citation_node,
                    NodeProvenanceInput {
                        source_path: Some(&document.source_path),
                        line_start: citation.provenance.line_start,
                        line_end: citation.provenance.line_end,
                        excerpt: citation.provenance.excerpt.as_deref(),
                        namespace: Some(namespace),
                        canonical: true,
                    },
                );
            }
            if let Some(citation_node) = graph.resolve_id(&citation_id) {
                graph.add_edge(
                    doc_node,
                    citation_node,
                    "references",
                    FiniteF32::new(0.8),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.6),
                )?;
            }
        }

        for link in &document.links {
            let link_id = format!("universal::{}::link::{}", namespace, slugify(&link.target));
            if graph.resolve_id(&link_id).is_none() {
                let tags = ["universal".to_string(), "universal:link".to_string()];
                let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
                let link_node = graph.add_node(
                    &link_id,
                    &link.label,
                    NodeType::Reference,
                    &tag_refs,
                    0.0,
                    0.2,
                )?;
                graph.set_node_provenance(
                    link_node,
                    NodeProvenanceInput {
                        source_path: Some(&document.source_path),
                        line_start: link.provenance.line_start,
                        line_end: link.provenance.line_end,
                        excerpt: link.provenance.excerpt.as_deref(),
                        namespace: Some(namespace),
                        canonical: true,
                    },
                );
            }
            if let Some(link_node) = graph.resolve_id(&link_id) {
                graph.add_edge(
                    doc_node,
                    link_node,
                    "binds_to",
                    FiniteF32::new(0.7),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.5),
                )?;
            }
        }

        for claim in &document.claims {
            let claim_id = format!("universal::{}::{}", namespace, claim.claim_id);
            if graph.resolve_id(&claim_id).is_none() {
                let tags = [
                    "universal".to_string(),
                    format!("confidence:{:?}", claim.confidence).to_lowercase(),
                    "universal:claim".to_string(),
                    format!("claim:kind:{:?}", claim.kind).to_lowercase(),
                    format!("claim:modality:{:?}", claim.modality).to_lowercase(),
                ];
                let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
                let claim_node = graph.add_node(
                    &claim_id,
                    &claim.label.chars().take(80).collect::<String>(),
                    NodeType::Concept,
                    &tag_refs,
                    0.0,
                    0.25,
                )?;
                graph.set_node_provenance(
                    claim_node,
                    NodeProvenanceInput {
                        source_path: Some(&document.source_path),
                        line_start: claim.provenance.line_start,
                        line_end: claim.provenance.line_end,
                        excerpt: claim.provenance.excerpt.as_deref(),
                        namespace: Some(namespace),
                        canonical: true,
                    },
                );
            }
            if let Some(claim_node) = graph.resolve_id(&claim_id) {
                graph.add_edge(
                    doc_node,
                    claim_node,
                    "declares_claim",
                    FiniteF32::new(0.8),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.6),
                )?;
                for citation in &document.citations {
                    let excerpt = claim.provenance.excerpt.as_deref().unwrap_or(&claim.label);
                    if !excerpt.contains(&citation.target) && !excerpt.contains(&citation.label) {
                        continue;
                    }
                    let citation_id = format!(
                        "universal::{}::citation::{}",
                        namespace,
                        slugify(&citation.target)
                    );
                    if let Some(citation_node) = graph.resolve_id(&citation_id) {
                        graph.add_edge(
                            claim_node,
                            citation_node,
                            "supports",
                            FiniteF32::new(0.5),
                            EdgeDirection::Forward,
                            false,
                            FiniteF32::new(0.4),
                        )?;
                    }
                }
            }
        }

        for candidate in &document.code_candidates {
            let binding_id = format!(
                "universal::{}::binding::{}",
                namespace,
                slugify(&candidate.label)
            );
            if binding_nodes.insert(binding_id.clone()) {
                let tags = [
                    "universal".to_string(),
                    "universal:binding".to_string(),
                    format!("candidate:{:?}", candidate.candidate_kind).to_lowercase(),
                ];
                let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
                let binding_node = graph.add_node(
                    &binding_id,
                    &candidate.label,
                    NodeType::Reference,
                    &tag_refs,
                    0.0,
                    0.25,
                )?;
                graph.set_node_provenance(
                    binding_node,
                    NodeProvenanceInput {
                        source_path: Some(&document.source_path),
                        line_start: candidate.provenance.line_start,
                        line_end: candidate.provenance.line_end,
                        excerpt: candidate.provenance.excerpt.as_deref(),
                        namespace: Some(namespace),
                        canonical: true,
                    },
                );
            }
            if let Some(binding_node) = graph.resolve_id(&binding_id) {
                graph.add_edge(
                    doc_node,
                    binding_node,
                    "mentions_symbol",
                    FiniteF32::new(0.75),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.5),
                )?;
            }
        }
    }

    if graph.num_nodes() > 0 {
        graph.finalize()?;
    }
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_markdown_without_l1ght() {
        let doc = canonicalize_plain_text(
            "docs/example.md",
            SourceKind::Markdown,
            "test",
            "# API\n\n`TokenValidator` must validate requests.\n\nSee [Docs](https://example.com).\n\n`TokenValidator`\n",
        );
        assert_eq!(doc.detected_type, "markdown");
        assert!(!doc.sections.is_empty());
        assert!(matches!(doc.sections[0].kind, DocumentSectionKind::Api));
        assert!(!doc.links.is_empty());
        assert!(!doc.entities.is_empty());
        assert!(!doc.claims.is_empty());
        assert!(!doc.code_candidates.is_empty());
        assert!(matches!(doc.claims[0].kind, DocumentClaimKind::Requirement));
        assert!(matches!(doc.claims[0].modality, ClaimModality::Must));
    }

    #[test]
    fn graphify_documents_creates_sections_entities_and_refs() {
        let doc = canonicalize_plain_text(
            "docs/example.md",
            SourceKind::Markdown,
            "test",
            "# Overview\n\nHello World.\n\nSee [Docs](https://example.com).\n\n`TokenValidator`\n10.1000/test\n",
        );
        let graph = graphify_documents(&[doc], "universal").unwrap();
        assert!(graph.num_nodes() >= 6);
        assert!(graph.num_edges() >= 5);
    }

    #[test]
    fn universal_candidate_filter_skips_runtime_state_artifacts() {
        assert!(is_universal_noise_path(Path::new("/tmp/node_modules")));
        assert!(is_universal_noise_path(Path::new("/tmp/.m1nd-runtime-ila")));
        assert!(is_universal_noise_path(Path::new(
            "/tmp/plasticity_state.json"
        )));
        assert!(!is_universal_noise_path(Path::new("/tmp/docs/notes.md")));
    }

    #[test]
    fn canonicalize_extracts_simple_tables() {
        let doc = canonicalize_plain_text(
            "docs/table.md",
            SourceKind::Markdown,
            "test",
            "# Overview\n\n| Name | Value |\n| A | B |\n",
        );
        assert!(!doc.tables.is_empty());
        assert_eq!(doc.tables[0].rows.len(), 1);
    }

    #[test]
    fn graphify_documents_emits_code_table_and_subsection_edges() {
        let doc = canonicalize_plain_text(
            "docs/semantic.md",
            SourceKind::Markdown,
            "test",
            "# API\n\n```rust\nfn validate() {}\n```\n\n## Tables\n\n| Name | Value |\n| A | B |\n",
        );
        let graph = graphify_documents(&[doc], "universal").unwrap();
        let mut contains_code = 0;
        let mut contains_table = 0;
        let mut subsection_of = 0;
        for src in 0..graph.num_nodes() as usize {
            for edge_idx in graph
                .csr
                .out_range(m1nd_core::types::NodeId::new(src as u32))
            {
                let rel = graph.strings.resolve(graph.csr.relations[edge_idx]);
                match rel {
                    "contains_code" => contains_code += 1,
                    "contains_table" => contains_table += 1,
                    "subsection_of" => subsection_of += 1,
                    _ => {}
                }
            }
        }
        assert!(contains_code >= 1);
        assert!(contains_table >= 1);
        assert!(subsection_of >= 1);
    }

    #[test]
    fn claim_support_edges_require_local_citation_evidence() {
        let doc = canonicalize_plain_text(
            "docs/evidence.md",
            SourceKind::Markdown,
            "test",
            "# API\n\nTokenValidator must validate requests.\n\n10.1000/alpha\n10.1000/beta\n",
        );
        let graph = graphify_documents(&[doc], "universal").unwrap();
        let mut support_edges = 0;
        for src in 0..graph.num_nodes() as usize {
            for edge_idx in graph
                .csr
                .out_range(m1nd_core::types::NodeId::new(src as u32))
            {
                let rel = graph.strings.resolve(graph.csr.relations[edge_idx]);
                if rel == "supports" {
                    support_edges += 1;
                }
            }
        }
        assert_eq!(support_edges, 0);
    }

    #[test]
    fn provider_probe_is_stable() {
        let providers = UniversalIngestAdapter::provider_availability();
        let _ = serde_json::to_string(&providers).unwrap();
    }

    #[test]
    fn content_hash_tracks_original_source_bytes_for_html_documents() {
        let temp = std::env::temp_dir().join(format!(
            "m1nd-universal-hash-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("page.html");
        let raw = "<html><body><h1>Hash Title</h1><p>TokenValidator must validate requests.</p></body></html>";
        std::fs::write(&file, raw).unwrap();

        let adapter = UniversalIngestAdapter::new(Some("universal".into()));
        let bundle = adapter.ingest_bundle(&file).unwrap();
        let document = bundle.documents.first().unwrap();

        assert_eq!(document.content_hash, short_hash_bytes(raw.as_bytes()));
        assert_ne!(document.content_hash, short_hash(&document.plain_text));
        let _ = std::fs::remove_dir_all(&temp);
    }
}
