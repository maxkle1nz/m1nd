use crate::{IngestAdapter, IngestStats};
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;
use walkdir::WalkDir;

// ---------------------------------------------------------------------------
// CrossRefAdapter — ingests CrossRef API JSON (works endpoint)
//
// Handles:
//   - Single-work envelope: {"status":"ok","message-type":"work","message":{...}}
//   - Raw work object:      {"DOI":"...", "title":[...], ...}
//   - Directory of .json files (walks recursively)
//
// Node taxonomy:
//   crossref::{DOI}           — article/work node
//   journal::{ISSN}           — journal node (container)
//   author::{Family}, {Given} — author node
//
// Edge semantics:
//   authored_by   article → author    0.8
//   references    article → cited DOI 0.7
//   published_in  article → journal   0.9
// ---------------------------------------------------------------------------

pub struct CrossRefAdapter {
    namespace: Option<String>,
}

impl CrossRefAdapter {
    pub fn new(namespace: Option<String>) -> Self {
        Self { namespace }
    }

    fn ns_prefix(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{}::", ns),
            None => String::new(),
        }
    }
}

impl IngestAdapter for CrossRefAdapter {
    fn domain(&self) -> &str {
        "crossref"
    }

    fn ingest(&self, root: &Path) -> M1ndResult<(Graph, IngestStats)> {
        let start = Instant::now();
        let mut stats = IngestStats::default();
        let mut records: Vec<CrossRefRecord> = Vec::new();

        if root.is_file() {
            stats.files_scanned = 1;
            let content = std::fs::read_to_string(root).map_err(|e| M1ndError::InvalidParams {
                tool: "crossref_ingest".into(),
                detail: format!("Failed to read {}: {}", root.display(), e),
            })?;
            if let Some(record) = parse_crossref_json(&content) {
                records.push(record);
                stats.files_parsed = 1;
            }
        } else if root.is_dir() {
            for entry in WalkDir::new(root)
                .follow_links(true)
                .max_depth(10)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "json" {
                    continue;
                }
                stats.files_scanned += 1;
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Some(record) = parse_crossref_json(&content) {
                        records.push(record);
                        stats.files_parsed += 1;
                    }
                }
            }
        }

        let prefix = self.ns_prefix();
        let (graph, node_count, edge_count) = build_graph(&prefix, &records);

        stats.nodes_created = node_count;
        stats.edges_created = edge_count;
        stats.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok((graph, stats))
    }
}

// ---------------------------------------------------------------------------
// Parsed record
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct CrossRefRecord {
    doi: String,
    title: String,
    work_type: String,
    publisher: String,
    container_title: String, // journal name
    issn: Vec<String>,
    volume: String,
    issue: String,
    page: String,
    language: String,
    issued_date: String, // YYYY-MM-DD or partial
    authors: Vec<AuthorEntry>,
    reference_dois: Vec<String>,
    citation_count: u64,
    abstract_text: String,
    subject: Vec<String>,
}

#[derive(Debug, Default, Clone)]
struct AuthorEntry {
    given: String,
    family: String,
    sequence: String, // "first" | "additional"
    orcid: Option<String>,
}

impl AuthorEntry {
    fn external_id(&self) -> String {
        let name = if self.given.is_empty() {
            self.family.clone()
        } else {
            format!("{}, {}", self.family, self.given)
        };
        format!("author::{}", name)
    }

    fn label(&self) -> String {
        if self.given.is_empty() {
            self.family.clone()
        } else {
            format!("{} {}", self.given, self.family)
        }
    }
}

// ---------------------------------------------------------------------------
// JSON parser — unwraps CrossRef envelope if present
// ---------------------------------------------------------------------------

fn parse_crossref_json(content: &str) -> Option<CrossRefRecord> {
    let root: Value = serde_json::from_str(content).ok()?;

    // Unwrap envelope: {"status":"ok","message":{...}}
    let work = if root.get("message-type").is_some() {
        root.get("message")?
    } else if root.get("DOI").is_some() {
        &root
    } else {
        return None;
    };

    let doi = get_str(work, "DOI")?;
    if doi.is_empty() {
        return None;
    }

    let title = work
        .get("title")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let abstract_text = get_str_or(work, "abstract", "");

    let publisher = get_str_or(work, "publisher", "");
    let work_type = get_str_or(work, "type", "journal-article");
    let volume = get_str_or(work, "volume", "");
    let issue = get_str_or(work, "issue", "");
    let page = get_str_or(work, "page", "");
    let language = get_str_or(work, "language", "");

    let container_title = work
        .get("container-title")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let issn: Vec<String> = work
        .get("ISSN")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let issued_date = work
        .get("issued")
        .and_then(|v| v.get("date-parts"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|parts| parts.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.as_u64().map(|n| n.to_string()))
                .collect::<Vec<_>>()
                .join("-")
        })
        .unwrap_or_default();

    // Authors
    let authors: Vec<AuthorEntry> = work
        .get("author")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let family = a.get("family")?.as_str()?.to_string();
                    let given = a
                        .get("given")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let sequence = a
                        .get("sequence")
                        .and_then(|v| v.as_str())
                        .unwrap_or("additional")
                        .to_string();
                    let orcid = a.get("ORCID").and_then(|v| v.as_str()).map(String::from);
                    Some(AuthorEntry {
                        given,
                        family,
                        sequence,
                        orcid,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // References — extract DOIs
    let reference_dois: Vec<String> = work
        .get("reference")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r.get("DOI").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let citation_count = work
        .get("is-referenced-by-count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let subject: Vec<String> = work
        .get("subject")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Some(CrossRefRecord {
        doi,
        title,
        work_type,
        publisher,
        container_title,
        issn,
        volume,
        issue,
        page,
        language,
        issued_date,
        authors,
        reference_dois,
        citation_count,
        abstract_text,
        subject,
    })
}

fn get_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn get_str_or(v: &Value, key: &str, default: &str) -> String {
    v.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

// ---------------------------------------------------------------------------
// Graph builder
// ---------------------------------------------------------------------------

fn build_graph(prefix: &str, records: &[CrossRefRecord]) -> (Graph, u64, u64) {
    let mut graph = Graph::new();
    let mut node_count: u64 = 0;
    let mut edge_count: u64 = 0;
    let mut seen_authors: HashSet<String> = HashSet::new();
    let mut seen_journals: HashSet<String> = HashSet::new();

    for record in records {
        // --- Article node ---
        let article_id = format!("{}crossref::{}", prefix, record.doi);
        let article_label = if record.title.is_empty() {
            format!("DOI: {}", record.doi)
        } else {
            record.title.clone()
        };

        let mut tags = vec![
            format!("doi:{}", record.doi),
            format!("type:{}", record.work_type),
        ];
        if !record.publisher.is_empty() {
            tags.push(format!("publisher:{}", record.publisher));
        }
        if !record.issued_date.is_empty() {
            tags.push(format!("date:{}", record.issued_date));
        }
        if !record.volume.is_empty() {
            tags.push(format!("volume:{}", record.volume));
        }
        if !record.issue.is_empty() {
            tags.push(format!("issue:{}", record.issue));
        }
        if !record.language.is_empty() {
            tags.push(format!("language:{}", record.language));
        }
        if record.citation_count > 0 {
            tags.push(format!("citations:{}", record.citation_count));
        }
        for subj in &record.subject {
            tags.push(format!("subject:{}", subj));
        }

        let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
        if graph
            .add_node(
                &article_id,
                &article_label,
                NodeType::Module, // scholarly work → module-level entity
                &tag_refs,
                0.0,
                0.0,
            )
            .is_ok()
        {
            node_count += 1;
        }

        // --- Author nodes + edges ---
        for author in &record.authors {
            let author_id = format!("{}{}", prefix, author.external_id());
            if seen_authors.insert(author_id.clone()) {
                let mut author_tags: Vec<String> = vec!["domain:crossref".to_string()];
                if let Some(ref orcid) = author.orcid {
                    author_tags.push(format!("orcid:{}", orcid));
                }
                let tag_refs: Vec<&str> = author_tags.iter().map(String::as_str).collect();
                if graph
                    .add_node(
                        &author_id,
                        &author.label(),
                        NodeType::Module,
                        &tag_refs,
                        0.0,
                        0.0,
                    )
                    .is_ok()
                {
                    node_count += 1;
                }
            }
            let author_nid = format!("{}{}", prefix, author.external_id());
            if let (Some(src), Some(tgt)) =
                (graph.resolve_id(&article_id), graph.resolve_id(&author_nid))
            {
                let weight = if author.sequence == "first" { 0.9 } else { 0.8 };
                if graph
                    .add_edge(
                        src,
                        tgt,
                        "authored_by",
                        FiniteF32::new(weight),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.0),
                    )
                    .is_ok()
                {
                    edge_count += 1;
                }
            }
        }

        // --- Journal node + published_in edge ---
        if !record.container_title.is_empty() {
            let journal_id = if let Some(issn) = record.issn.first() {
                format!("{}journal::{}", prefix, issn)
            } else {
                format!(
                    "{}journal::{}",
                    prefix,
                    record.container_title.to_lowercase().replace(' ', "_")
                )
            };

            if seen_journals.insert(journal_id.clone()) {
                let journal_tags: Vec<String> = record
                    .issn
                    .iter()
                    .map(|issn| format!("issn:{}", issn))
                    .collect();
                let tag_refs: Vec<&str> = journal_tags.iter().map(String::as_str).collect();
                if graph
                    .add_node(
                        &journal_id,
                        &record.container_title,
                        NodeType::Module,
                        &tag_refs,
                        0.0,
                        0.0,
                    )
                    .is_ok()
                {
                    node_count += 1;
                }
            }

            if let (Some(src), Some(tgt)) =
                (graph.resolve_id(&article_id), graph.resolve_id(&journal_id))
            {
                if graph
                    .add_edge(
                        src,
                        tgt,
                        "published_in",
                        FiniteF32::new(0.9),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.0),
                    )
                    .is_ok()
                {
                    edge_count += 1;
                }
            }
        }

        // --- Reference edges ---
        for ref_doi in &record.reference_dois {
            let ref_id = format!("{}crossref::{}", prefix, ref_doi);

            // Create stub node for referenced DOI if doesn't exist
            if graph.resolve_id(&ref_id).is_none() {
                let ref_label = format!("DOI: {}", ref_doi);
                let ref_tags = [format!("doi:{}", ref_doi), "stub:true".to_string()];
                let tag_refs: Vec<&str> = ref_tags.iter().map(String::as_str).collect();
                if graph
                    .add_node(&ref_id, &ref_label, NodeType::Module, &tag_refs, 0.0, 0.0)
                    .is_ok()
                {
                    node_count += 1;
                }
            }

            if let (Some(src), Some(tgt)) =
                (graph.resolve_id(&article_id), graph.resolve_id(&ref_id))
            {
                if graph
                    .add_edge(
                        src,
                        tgt,
                        "references",
                        FiniteF32::new(0.7),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.0),
                    )
                    .is_ok()
                {
                    edge_count += 1;
                }
            }
        }
    }

    // Finalize graph
    let _ = graph.finalize();
    (graph, node_count, edge_count)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn nature_fixture() -> String {
        // Minimal CrossRef JSON simulating api.crossref.org/works/10.1038/nature12373
        serde_json::json!({
            "status": "ok",
            "message-type": "work",
            "message-version": "1.0.0",
            "message": {
                "DOI": "10.1038/nature12373",
                "type": "journal-article",
                "title": ["Nanometre-scale thermometry in a living cell"],
                "publisher": "Springer Science and Business Media LLC",
                "container-title": ["Nature"],
                "ISSN": ["0028-0836", "1476-4687"],
                "volume": "500",
                "issue": "7460",
                "page": "54-58",
                "language": "en",
                "issued": {
                    "date-parts": [[2013, 7, 31]]
                },
                "author": [
                    {"given": "G.", "family": "Kucsko", "sequence": "first"},
                    {"given": "P. C.", "family": "Maurer", "sequence": "additional"},
                    {"given": "M. D.", "family": "Lukin", "sequence": "additional"}
                ],
                "reference": [
                    {"key": "ref1", "DOI": "10.3402/nano.v3i0.11586"},
                    {"key": "ref2", "DOI": "10.1038/nature03509"},
                    {"key": "ref3"} // no DOI — should be skipped
                ],
                "is-referenced-by-count": 1742,
                "abstract": "We demonstrate a new method for temperature sensing."
            }
        })
        .to_string()
    }

    #[test]
    fn parse_crossref_envelope() {
        let record = parse_crossref_json(&nature_fixture()).unwrap();
        assert_eq!(record.doi, "10.1038/nature12373");
        assert_eq!(record.title, "Nanometre-scale thermometry in a living cell");
        assert_eq!(record.work_type, "journal-article");
        assert_eq!(record.publisher, "Springer Science and Business Media LLC");
        assert_eq!(record.container_title, "Nature");
        assert_eq!(record.issn, vec!["0028-0836", "1476-4687"]);
        assert_eq!(record.volume, "500");
        assert_eq!(record.issue, "7460");
        assert_eq!(record.issued_date, "2013-7-31");
        assert_eq!(record.authors.len(), 3);
        assert_eq!(record.authors[0].family, "Kucsko");
        assert_eq!(record.authors[0].sequence, "first");
        assert_eq!(record.reference_dois.len(), 2); // ref3 has no DOI
        assert_eq!(record.citation_count, 1742);
    }

    #[test]
    fn parse_raw_work_object() {
        // No envelope, just a raw work object
        let json = serde_json::json!({
            "DOI": "10.1234/test",
            "title": ["Test Article"],
            "publisher": "Test Publisher",
            "type": "journal-article",
            "author": [{"given": "A.", "family": "Author", "sequence": "first"}],
            "reference": []
        })
        .to_string();

        let record = parse_crossref_json(&json).unwrap();
        assert_eq!(record.doi, "10.1234/test");
        assert_eq!(record.title, "Test Article");
    }

    #[test]
    fn build_graph_creates_correct_topology() {
        let record = parse_crossref_json(&nature_fixture()).unwrap();
        let (graph, nodes, edges) = build_graph("", &[record]);

        // Expected nodes:
        // 1 article + 3 authors + 1 journal + 2 reference stubs = 7
        assert_eq!(nodes, 7, "expected 7 nodes");

        // Expected edges:
        // 3 authored_by + 1 published_in + 2 references = 6
        assert_eq!(edges, 6, "expected 6 edges");

        // Verify key nodes exist
        assert!(graph.resolve_id("crossref::10.1038/nature12373").is_some());
        assert!(graph.resolve_id("author::Kucsko, G.").is_some());
        assert!(graph.resolve_id("journal::0028-0836").is_some());
        assert!(graph
            .resolve_id("crossref::10.3402/nano.v3i0.11586")
            .is_some());
    }

    #[test]
    fn adapter_ingests_single_file() {
        let dir = std::env::temp_dir().join(format!(
            "m1nd-crossref-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("nature12373.json");
        fs::write(&file, nature_fixture()).unwrap();

        let adapter = CrossRefAdapter::new(None);
        let (graph, stats) = adapter.ingest(&file).unwrap();

        assert_eq!(stats.files_scanned, 1);
        assert_eq!(stats.files_parsed, 1);
        assert_eq!(stats.nodes_created, 7);
        assert_eq!(stats.edges_created, 6);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn adapter_ingests_directory() {
        let dir = std::env::temp_dir().join(format!(
            "m1nd-crossref-dir-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        // Write two work files
        let work1 = serde_json::json!({
            "DOI": "10.1234/work1",
            "title": ["Work One"],
            "type": "journal-article",
            "publisher": "Pub A",
            "author": [{"given": "A.", "family": "Alpha", "sequence": "first"}],
            "reference": []
        });
        let work2 = serde_json::json!({
            "DOI": "10.1234/work2",
            "title": ["Work Two"],
            "type": "journal-article",
            "publisher": "Pub B",
            "author": [{"given": "A.", "family": "Alpha", "sequence": "first"}],
            "reference": [{"key": "r1", "DOI": "10.1234/work1"}]
        });
        fs::write(dir.join("work1.json"), work1.to_string()).unwrap();
        fs::write(dir.join("work2.json"), work2.to_string()).unwrap();

        let adapter = CrossRefAdapter::new(None);
        let (graph, stats) = adapter.ingest(&dir).unwrap();

        assert_eq!(stats.files_scanned, 2);
        assert_eq!(stats.files_parsed, 2);
        // work1: 1 article + 1 author = 2 nodes, 1 authored_by
        // work2: 1 article + 0 author (dedup) + 1 ref stub = 2 nodes, 1 authored_by + 1 references
        // But work1 already exists as full node, so ref stub is not created
        // Total: 3 articles (work1 + work2 + stub that becomes work1) + 1 author
        // Actually: work1 = real node, work2 = real node, work2's ref to work1
        //   = stub crossref::10.1234/work1 BUT work1 is ingested second (or first)
        //   ... depends on WalkDir order, but dedup handles it
        assert!(stats.nodes_created >= 3);
        assert!(stats.edges_created >= 2);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn namespace_prefix_applied() {
        let record = parse_crossref_json(&nature_fixture()).unwrap();
        let (graph, _, _) = build_graph("scholar::", &[record]);

        assert!(graph
            .resolve_id("scholar::crossref::10.1038/nature12373")
            .is_some());
        assert!(graph.resolve_id("scholar::author::Kucsko, G.").is_some());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_crossref_json("not json at all").is_none());
        assert!(parse_crossref_json("{}").is_none()); // no DOI
        assert!(parse_crossref_json("{\"DOI\":\"\"}").is_none()); // empty DOI
    }
}
