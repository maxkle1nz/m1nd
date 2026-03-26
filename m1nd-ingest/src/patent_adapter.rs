use crate::{IngestAdapter, IngestStats};
use m1nd_core::error::M1ndResult;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

/// Maps a patent kind code to a L1GHT state.
fn kind_to_state(kind: &str) -> &'static str {
    match kind {
        "A1" | "A2" | "A9" => "draft",         // Published application
        "B1" | "B2" => "active",               // Granted patent
        "C1" | "C2" | "C3" => "active",        // Reexamination certificate
        "P1" | "P2" | "P3" | "P4" => "active", // Plant patent
        "S1" => "active",                      // Design patent
        "E1" => "active",                      // Reissue
        "H1" => "deprecated",                  // Statutory invention registration
        _ => "active",
    }
}

/// Extracted fields from a patent XML document.
#[derive(Default, Debug)]
struct PatentRecord {
    doc_number: String,
    kind: String,
    country: String,
    title: String,
    pub_date: String,
    app_date: String,
    assignees: Vec<String>,
    inventors: Vec<String>,
    abstract_text: String,
    classifications: Vec<String>,
    citations: Vec<String>,
    /// Source format detected
    format: PatentFormat,
}

#[derive(Default, Debug, Clone, Copy)]
enum PatentFormat {
    #[default]
    Unknown,
    UsRedBook,    // <us-patent-grant>
    UsYellowBook, // <us-patent-application>
    EpoDocDb,     // <ep-patent-document>
}

impl PatentRecord {
    fn external_id(&self) -> String {
        let num = if self.doc_number.is_empty() {
            "UNKNOWN"
        } else {
            &self.doc_number
        };
        let country = if self.country.is_empty() {
            "XX"
        } else {
            &self.country
        };
        let kind = if self.kind.is_empty() { "" } else { &self.kind };
        format!("patent::{}::{}{}", country, num, kind)
    }

    fn label(&self) -> String {
        if self.title.is_empty() {
            self.external_id()
        } else {
            format!(
                "{}{}{} — {}",
                self.country, self.doc_number, self.kind, self.title
            )
        }
    }

    fn state(&self) -> &'static str {
        kind_to_state(&self.kind)
    }

    fn excerpt(&self) -> Option<String> {
        if self.abstract_text.is_empty() {
            None
        } else {
            Some(self.abstract_text.chars().take(220).collect())
        }
    }
}

pub struct PatentIngestAdapter {
    namespace: String,
}

impl PatentIngestAdapter {
    pub fn new(namespace: Option<String>) -> Self {
        let namespace = namespace
            .unwrap_or_else(|| "patent".to_string())
            .trim()
            .to_lowercase();
        let namespace = if namespace.is_empty() {
            "patent".to_string()
        } else {
            namespace
        };
        Self { namespace }
    }

    fn accepted_extension(path: &Path) -> bool {
        matches!(
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase()),
            Some(ext) if matches!(ext.as_str(), "xml")
        )
    }

    fn collect_files(&self, root: &Path) -> Vec<PathBuf> {
        if root.is_file() {
            return if Self::accepted_extension(root) {
                vec![root.to_path_buf()]
            } else {
                vec![]
            };
        }

        WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file() && Self::accepted_extension(entry.path()))
            .map(|entry| entry.into_path())
            .collect()
    }

    fn file_timestamp(path: &Path) -> f64 {
        std::fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
            })
    }

    /// Split concatenated XML documents (USPTO bulk files).
    /// USPTO ships files with multiple `<?xml` declarations concatenated.
    fn split_concatenated_xml(content: &str) -> Vec<&str> {
        let mut docs = Vec::new();
        let mut start = 0;

        for (i, _) in content.match_indices("<?xml") {
            if i > start && i > 0 {
                let chunk = content[start..i].trim();
                if !chunk.is_empty() {
                    docs.push(chunk);
                }
            }
            start = i;
        }

        // Last document
        if start < content.len() {
            let chunk = content[start..].trim();
            if !chunk.is_empty() {
                docs.push(chunk);
            }
        }

        // If no <?xml found, treat whole content as one doc
        if docs.is_empty() && !content.trim().is_empty() {
            docs.push(content);
        }

        docs
    }

    /// Parse a single patent XML document into a PatentRecord.
    fn parse_patent_xml(xml: &str) -> Option<PatentRecord> {
        let mut reader = Reader::from_str(xml);

        let mut record = PatentRecord::default();
        let mut path: Vec<String> = Vec::new();
        let mut buf = Vec::new();
        let mut text_buf = String::new();
        let mut in_abstract = false;
        let mut in_title = false;
        let mut in_citation = false;
        let mut current_citation_doc = String::new();
        let mut in_classification = false;
        let mut in_orgname = false;
        let mut in_inventor_name = false;
        let mut inventor_parts: Vec<String> = Vec::new();
        // EPO B-series SDOBI flags
        let mut in_b541 = false; // EPO title
        let mut in_b721 = false; // EPO inventor block
        let mut in_b731 = false; // EPO assignee block
        let mut in_b561 = false; // EPO citation block
        let mut in_pdat = false; // EPO inline data
        let mut in_snm = false; // EPO surname
        let mut in_fnm = false; // EPO first name

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    // Detect format from root element
                    if path.is_empty() {
                        match name.as_str() {
                            "us-patent-grant" => record.format = PatentFormat::UsRedBook,
                            "us-patent-application" => {
                                record.format = PatentFormat::UsYellowBook;
                            }
                            "ep-patent-document" => {
                                record.format = PatentFormat::EpoDocDb;
                                // EPO stores country/doc-number/kind as attributes
                                for attr in e.attributes().filter_map(|a| a.ok()) {
                                    let key = String::from_utf8_lossy(attr.key.as_ref());
                                    let val = String::from_utf8_lossy(&attr.value);
                                    match key.as_ref() {
                                        "country" => {
                                            if record.country.is_empty() {
                                                record.country = val.to_string();
                                            }
                                        }
                                        "doc-number" => {
                                            if record.doc_number.is_empty() {
                                                record.doc_number = val.to_string();
                                            }
                                        }
                                        "kind" => {
                                            if record.kind.is_empty() {
                                                record.kind = val.to_string();
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    path.push(name.clone());

                    match name.as_str() {
                        "abstract" => in_abstract = true,
                        "invention-title" => in_title = true,
                        "orgname" => in_orgname = true,
                        "patcit" | "us-citation" => in_citation = true,
                        "classification-ipcr" | "classification-cpc-text" => {
                            in_classification = true;
                        }
                        "given-name" | "family-name" | "last-name" | "first-name" => {
                            if path
                                .iter()
                                .any(|p| p == "inventors" || p == "inventor" || p == "applicants")
                            {
                                in_inventor_name = true;
                            }
                        }
                        // EPO B-series SDOBI tags
                        "B541" => in_b541 = true,
                        "B721" => in_b721 = true,
                        "B731" => in_b731 = true,
                        "B561" => in_b561 = true,
                        "pdat" => in_pdat = true,
                        "snm" => in_snm = true,
                        "fnm" => in_fnm = true,
                        "pcit" => {
                            if in_b561 {
                                in_citation = true;
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    match name.as_str() {
                        "abstract" => {
                            in_abstract = false;
                            if !text_buf.is_empty() {
                                record.abstract_text = text_buf.trim().to_string();
                                text_buf.clear();
                            }
                        }
                        "invention-title" => {
                            in_title = false;
                            if !text_buf.is_empty() {
                                record.title = text_buf.trim().to_string();
                                text_buf.clear();
                            }
                        }
                        "orgname" => {
                            in_orgname = false;
                            if !text_buf.is_empty() {
                                let parent_is_assignee =
                                    path.iter().any(|p| p == "assignee" || p == "assignees");
                                if parent_is_assignee {
                                    record.assignees.push(text_buf.trim().to_string());
                                }
                                text_buf.clear();
                            }
                        }
                        "given-name" | "family-name" | "last-name" | "first-name" => {
                            if in_inventor_name && !text_buf.is_empty() {
                                inventor_parts.push(text_buf.trim().to_string());
                                text_buf.clear();
                            }
                            in_inventor_name = false;
                        }
                        "inventor" => {
                            if !inventor_parts.is_empty() {
                                record.inventors.push(inventor_parts.join(" "));
                                inventor_parts.clear();
                            }
                        }
                        // EPO B-series end tags
                        "B541" => {
                            in_b541 = false;
                            if !text_buf.is_empty() && record.title.is_empty() {
                                record.title = text_buf.trim().to_string();
                                text_buf.clear();
                            }
                        }
                        "B721" => {
                            in_b721 = false;
                            if !inventor_parts.is_empty() {
                                record.inventors.push(inventor_parts.join(" "));
                                inventor_parts.clear();
                            }
                        }
                        "B731" => {
                            in_b731 = false;
                            if !text_buf.is_empty() {
                                record.assignees.push(text_buf.trim().to_string());
                                text_buf.clear();
                            }
                        }
                        "B561" => {
                            in_b561 = false;
                        }
                        "pdat" => {
                            in_pdat = false;
                        }
                        "snm" => {
                            if in_snm && !text_buf.is_empty() {
                                if in_b721 {
                                    inventor_parts.push(text_buf.trim().to_string());
                                    text_buf.clear();
                                }
                                // For B731 (assignee), don't clear — let B731 close handle it
                            }
                            in_snm = false;
                        }
                        "fnm" => {
                            if in_fnm && !text_buf.is_empty() {
                                if in_b721 {
                                    inventor_parts.push(text_buf.trim().to_string());
                                }
                                text_buf.clear();
                            }
                            in_fnm = false;
                        }
                        "pcit" => {
                            if in_citation {
                                if !current_citation_doc.is_empty() {
                                    record.citations.push(current_citation_doc.clone());
                                    current_citation_doc.clear();
                                }
                                in_citation = false;
                            }
                        }
                        "patcit" | "us-citation" => {
                            in_citation = false;
                            if !current_citation_doc.is_empty() {
                                record.citations.push(current_citation_doc.clone());
                                current_citation_doc.clear();
                            }
                        }
                        "classification-ipcr" | "classification-cpc-text" => {
                            in_classification = false;
                            if !text_buf.is_empty() {
                                record.classifications.push(text_buf.trim().to_string());
                                text_buf.clear();
                            }
                        }
                        "country" => {
                            if !text_buf.is_empty() {
                                // Country in publication-reference context
                                if path
                                    .iter()
                                    .any(|p| p == "publication-reference" || p == "document-id")
                                {
                                    if record.country.is_empty() {
                                        record.country = text_buf.trim().to_string();
                                    }
                                    if in_citation {
                                        current_citation_doc.push_str(text_buf.trim());
                                    }
                                }
                                text_buf.clear();
                            }
                        }
                        "doc-number" => {
                            if !text_buf.is_empty() {
                                if in_citation {
                                    current_citation_doc.push_str(text_buf.trim());
                                } else if record.doc_number.is_empty() {
                                    record.doc_number = text_buf.trim().to_string();
                                }
                                text_buf.clear();
                            }
                        }
                        "kind" => {
                            if !text_buf.is_empty() {
                                if in_citation {
                                    current_citation_doc.push_str(text_buf.trim());
                                } else if record.kind.is_empty() {
                                    record.kind = text_buf.trim().to_string();
                                }
                                text_buf.clear();
                            }
                        }
                        "date" => {
                            if !text_buf.is_empty() {
                                if path.iter().any(|p| p == "publication-reference")
                                    && record.pub_date.is_empty()
                                {
                                    record.pub_date = text_buf.trim().to_string();
                                } else if path.iter().any(|p| p == "application-reference")
                                    && record.app_date.is_empty()
                                {
                                    record.app_date = text_buf.trim().to_string();
                                }
                                text_buf.clear();
                            }
                        }
                        _ => {}
                    }

                    path.pop();
                }
                Ok(Event::Text(e)) => {
                    if in_abstract
                        || in_title
                        || in_orgname
                        || in_inventor_name
                        || in_classification
                        || in_citation
                        || in_pdat
                        || in_snm
                        || in_fnm
                    {
                        if let Ok(t) = e.unescape() {
                            text_buf.push_str(&t);
                        }
                    } else {
                        // Capture text for country/doc-number/kind/date
                        let current = path.last().map(|s| s.as_str()).unwrap_or("");
                        if matches!(current, "country" | "doc-number" | "kind" | "date") {
                            if let Ok(t) = e.unescape() {
                                text_buf.push_str(&t);
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        if record.doc_number.is_empty() {
            return None;
        }

        Some(record)
    }
}

impl IngestAdapter for PatentIngestAdapter {
    fn domain(&self) -> &str {
        "patent"
    }

    fn ingest(&self, root: &Path) -> M1ndResult<(Graph, IngestStats)> {
        let start = Instant::now();
        let files = self.collect_files(root);
        let mut stats = IngestStats {
            files_scanned: files.len() as u64,
            ..Default::default()
        };

        let mut node_ids: HashSet<String> = HashSet::new();
        let mut edge_keys: HashSet<(String, String, String)> = HashSet::new();

        struct NodeRecord {
            id: String,
            label: String,
            node_type: NodeType,
            tags: Vec<String>,
            timestamp: f64,
            source_path: String,
            excerpt: Option<String>,
            namespace: String,
        }

        struct EdgeRecord {
            source: String,
            target: String,
            relation: String,
            weight: f32,
        }

        let mut nodes: Vec<NodeRecord> = Vec::new();
        let mut edges: Vec<EdgeRecord> = Vec::new();

        for path in &files {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let timestamp = Self::file_timestamp(path);

            let xml_docs = Self::split_concatenated_xml(&content);

            for xml_doc in xml_docs {
                let record = match Self::parse_patent_xml(xml_doc) {
                    Some(r) => r,
                    None => continue,
                };

                let ext_id = record.external_id();
                if !node_ids.insert(ext_id.clone()) {
                    continue; // duplicate
                }

                let state = record.state();
                let mut tags = vec![
                    "patent".to_string(),
                    format!("patent:country:{}", record.country),
                    format!("patent:state:{}", state),
                    format!("namespace:{}", self.namespace),
                ];
                if !record.kind.is_empty() {
                    tags.push(format!("patent:kind:{}", record.kind));
                }
                for cls in &record.classifications {
                    tags.push(format!("patent:class:{}", cls));
                }

                // Patent node
                nodes.push(NodeRecord {
                    id: ext_id.clone(),
                    label: record.label(),
                    node_type: NodeType::File,
                    tags: tags.clone(),
                    timestamp,
                    source_path: rel_path.clone(),
                    excerpt: record.excerpt(),
                    namespace: self.namespace.clone(),
                });

                // Citation edges (depends_on)
                for citation in &record.citations {
                    let target_id = format!("patent::{}", citation.replace(' ', ""));
                    let key = (ext_id.clone(), target_id.clone(), "cites".to_string());
                    if edge_keys.insert(key) {
                        // Create a reference node for the cited patent
                        // (may be resolved later if the cited patent is also ingested)
                        if node_ids.insert(target_id.clone()) {
                            nodes.push(NodeRecord {
                                id: target_id.clone(),
                                label: citation.clone(),
                                node_type: NodeType::Reference,
                                tags: vec!["patent".to_string(), "patent:cited".to_string()],
                                timestamp,
                                source_path: rel_path.clone(),
                                excerpt: None,
                                namespace: self.namespace.clone(),
                            });
                        }
                        edges.push(EdgeRecord {
                            source: ext_id.clone(),
                            target: target_id,
                            relation: "cites".to_string(),
                            weight: 0.8,
                        });
                    }
                }

                // Assignee nodes
                for assignee in &record.assignees {
                    let assignee_id = format!(
                        "patent::assignee::{}",
                        assignee.to_lowercase().replace(' ', "_")
                    );
                    if node_ids.insert(assignee_id.clone()) {
                        nodes.push(NodeRecord {
                            id: assignee_id.clone(),
                            label: assignee.clone(),
                            node_type: NodeType::Concept,
                            tags: vec!["patent".to_string(), "patent:assignee".to_string()],
                            timestamp,
                            source_path: rel_path.clone(),
                            excerpt: None,
                            namespace: self.namespace.clone(),
                        });
                    }
                    let key = (
                        ext_id.clone(),
                        assignee_id.clone(),
                        "assigned_to".to_string(),
                    );
                    if edge_keys.insert(key) {
                        edges.push(EdgeRecord {
                            source: ext_id.clone(),
                            target: assignee_id,
                            relation: "assigned_to".to_string(),
                            weight: 1.0,
                        });
                    }
                }

                // Classification nodes (IPC/CPC)
                for cls in &record.classifications {
                    let cls_id = format!(
                        "patent::class::{}",
                        cls.to_lowercase().replace(' ', "").replace('/', "_")
                    );
                    if node_ids.insert(cls_id.clone()) {
                        nodes.push(NodeRecord {
                            id: cls_id.clone(),
                            label: cls.clone(),
                            node_type: NodeType::Concept,
                            tags: vec!["patent".to_string(), "patent:classification".to_string()],
                            timestamp,
                            source_path: rel_path.clone(),
                            excerpt: None,
                            namespace: self.namespace.clone(),
                        });
                    }
                    let key = (ext_id.clone(), cls_id.clone(), "classified_as".to_string());
                    if edge_keys.insert(key) {
                        edges.push(EdgeRecord {
                            source: ext_id.clone(),
                            target: cls_id,
                            relation: "classified_as".to_string(),
                            weight: 0.9,
                        });
                    }
                }
            }

            stats.files_parsed += 1;
        }

        // Build graph
        let mut graph = Graph::with_capacity(nodes.len(), edges.len());

        for node in &nodes {
            let tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
            if let Ok(node_id) = graph.add_node(
                &node.id,
                &node.label,
                node.node_type,
                &tags,
                node.timestamp,
                0.5, // change_frequency
            ) {
                graph.set_node_provenance(
                    node_id,
                    NodeProvenanceInput {
                        source_path: Some(&node.source_path),
                        line_start: None,
                        line_end: None,
                        excerpt: node.excerpt.as_deref(),
                        namespace: Some(&node.namespace),
                        canonical: true,
                    },
                );
                stats.nodes_created += 1;
            }
        }

        for edge in &edges {
            if let (Some(source), Some(target)) = (
                graph.resolve_id(&edge.source),
                graph.resolve_id(&edge.target),
            ) {
                if graph
                    .add_edge(
                        source,
                        target,
                        &edge.relation,
                        FiniteF32::new(edge.weight),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.7),
                    )
                    .is_ok()
                {
                    stats.edges_created += 1;
                }
            }
        }

        if graph.num_nodes() > 0 {
            graph.finalize()?;
        }
        stats.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok((graph, stats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_USPTO_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<us-patent-grant>
  <us-bibliographic-data-grant>
    <publication-reference>
      <document-id>
        <country>US</country>
        <doc-number>11636241</doc-number>
        <kind>B2</kind>
        <date>20230425</date>
      </document-id>
    </publication-reference>
    <application-reference>
      <document-id>
        <country>US</country>
        <doc-number>17387082</doc-number>
        <date>20210728</date>
      </document-id>
    </application-reference>
    <invention-title>Physical device optimization with reduced memory footprint</invention-title>
    <assignees>
      <assignee>
        <orgname>Luminous Computing Inc</orgname>
      </assignee>
    </assignees>
    <inventors>
      <inventor>
        <given-name>John</given-name>
        <family-name>Doe</family-name>
      </inventor>
    </inventors>
    <us-references-cited>
      <us-citation>
        <patcit>
          <document-id>
            <country>US</country>
            <doc-number>9977644</doc-number>
            <kind>B2</kind>
          </document-id>
        </patcit>
      </us-citation>
      <us-citation>
        <patcit>
          <document-id>
            <country>US</country>
            <doc-number>5774693</doc-number>
            <kind>A</kind>
          </document-id>
        </patcit>
      </us-citation>
    </us-references-cited>
    <classification-ipcr>G06F 30/23</classification-ipcr>
  </us-bibliographic-data-grant>
  <abstract>
    <p>A method for physical device optimization using time reversal at absorbing boundaries to reduce memory usage during adjoint simulations.</p>
  </abstract>
</us-patent-grant>"#;

    #[test]
    fn parses_single_patent() {
        let record = PatentIngestAdapter::parse_patent_xml(SAMPLE_USPTO_XML).unwrap();
        assert_eq!(record.doc_number, "11636241");
        assert_eq!(record.kind, "B2");
        assert_eq!(record.country, "US");
        assert_eq!(record.state(), "active");
        assert_eq!(
            record.title,
            "Physical device optimization with reduced memory footprint"
        );
        assert_eq!(record.assignees, vec!["Luminous Computing Inc"]);
        assert_eq!(record.inventors, vec!["John Doe"]);
        assert_eq!(record.citations.len(), 2);
        assert!(!record.abstract_text.is_empty());
    }

    #[test]
    fn splits_concatenated_xml() {
        let bulk = format!(
            "{}\n{}\n{}",
            SAMPLE_USPTO_XML,
            SAMPLE_USPTO_XML.replace("11636241", "99999999"),
            SAMPLE_USPTO_XML.replace("11636241", "88888888"),
        );
        let docs = PatentIngestAdapter::split_concatenated_xml(&bulk);
        assert_eq!(docs.len(), 3);
    }

    #[test]
    fn kind_codes_map_correctly() {
        assert_eq!(kind_to_state("A1"), "draft");
        assert_eq!(kind_to_state("B2"), "active");
        assert_eq!(kind_to_state("H1"), "deprecated");
    }

    #[test]
    fn ingest_sample_creates_graph() {
        let dir = std::env::temp_dir().join(format!(
            "m1nd-patent-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sample.xml"), SAMPLE_USPTO_XML).unwrap();

        let adapter = PatentIngestAdapter::new(None);
        let (graph, stats) = adapter.ingest(&dir).unwrap();

        // 1 patent + 2 citations + 1 assignee + 1 classification = 5 nodes
        assert!(stats.nodes_created >= 5);
        // cites(2) + assigned_to(1) + classified_as(1) = 4 edges
        assert!(stats.edges_created >= 4);
        assert!(graph.resolve_id("patent::US::11636241B2").is_some());

        let _ = std::fs::remove_dir_all(dir);
    }
}
