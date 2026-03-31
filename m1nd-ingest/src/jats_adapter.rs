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

// ─── Format Detection ─────────────────────────────────────────────

#[derive(Default, Debug, Clone, Copy, PartialEq)]
enum ArticleFormat {
    #[default]
    Unknown,
    PubMedNlm, // <PubmedArticleSet> / <PubmedArticle>
    Jats,      // <article> (NISO JATS Z39.96)
}

// ─── Extracted Record ──────────────────────────────────────────────

#[derive(Default, Debug)]
struct ArticleRecord {
    // Identity
    pmid: String,
    doi: String,
    pmc_id: String,
    title: String,
    // Metadata
    journal: String,
    year: String,
    abstract_text: String,
    // Entities
    authors: Vec<String>,
    keywords: Vec<String>,
    // References
    citations: Vec<CitationRef>,
    // Detected format
    format: ArticleFormat,
}

#[derive(Default, Debug, Clone)]
struct CitationRef {
    doi: String,
    pmid: String,
    pmc_id: String,
    title: String,
}

impl CitationRef {
    fn best_id(&self) -> Option<String> {
        if !self.doi.is_empty() {
            Some(format!("doi::{}", self.doi))
        } else if !self.pmid.is_empty() {
            Some(format!("pmid::{}", self.pmid))
        } else if !self.pmc_id.is_empty() {
            Some(format!("pmc::{}", self.pmc_id))
        } else {
            None
        }
    }
}

impl ArticleRecord {
    fn external_id(&self) -> String {
        if !self.doi.is_empty() {
            format!("doi::{}", self.doi)
        } else if !self.pmid.is_empty() {
            format!("pmid::{}", self.pmid)
        } else if !self.pmc_id.is_empty() {
            format!("pmc::{}", self.pmc_id)
        } else {
            format!("article::{}", self.title.replace(' ', "_").to_lowercase())
        }
    }

    fn label(&self) -> String {
        if !self.title.is_empty() {
            self.title.clone()
        } else {
            self.external_id()
        }
    }

    fn state(&self) -> &'static str {
        // Scientific articles are published once they have a PMID/DOI
        if !self.pmid.is_empty() || !self.doi.is_empty() {
            "published"
        } else {
            "draft"
        }
    }

    fn excerpt(&self) -> Option<String> {
        if self.abstract_text.is_empty() {
            None
        } else {
            Some(self.abstract_text.chars().take(220).collect())
        }
    }
}

// ─── Adapter ───────────────────────────────────────────────────────

pub struct JatsArticleAdapter {
    namespace: String,
}

impl JatsArticleAdapter {
    pub fn new(namespace: Option<String>) -> Self {
        let namespace = namespace
            .unwrap_or_else(|| "article".to_string())
            .trim()
            .to_lowercase();
        let namespace = if namespace.is_empty() {
            "article".to_string()
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
            Some(ext) if matches!(ext.as_str(), "xml" | "nxml")
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

    /// Parse articles from XML content.
    /// Handles both PubMed NLM and JATS formats.
    fn parse_articles(xml: &str) -> Vec<ArticleRecord> {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();
        let mut path: Vec<String> = Vec::new();
        let mut text_buf = String::new();

        let mut records: Vec<ArticleRecord> = Vec::new();
        let mut current: Option<ArticleRecord> = None;
        let mut current_citation: Option<CitationRef> = None;

        // State flags
        let mut in_title = false;
        let mut in_abstract = false;
        let mut in_journal_title = false;
        let mut in_author_name = false;
        let mut in_keyword = false;
        let mut in_ref = false;
        let mut author_parts: Vec<String> = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    path.push(name.clone());

                    match name.as_str() {
                        // ── PubMed NLM format ──
                        "PubmedArticle" => {
                            let rec = ArticleRecord {
                                format: ArticleFormat::PubMedNlm,
                                ..ArticleRecord::default()
                            };
                            current = Some(rec);
                        }
                        // ── JATS format ──
                        "article" => {
                            if current.is_none() {
                                let rec = ArticleRecord {
                                    format: ArticleFormat::Jats,
                                    ..ArticleRecord::default()
                                };
                                current = Some(rec);
                            }
                        }
                        // ── Title ──
                        "ArticleTitle" | "article-title" => in_title = true,
                        // ── Abstract ──
                        "AbstractText" | "Abstract" | "abstract" => in_abstract = true,
                        // ── Journal ──
                        "Title" if path.iter().any(|p| p == "Journal") => {
                            in_journal_title = true;
                        }
                        "journal-title" => in_journal_title = true,
                        // ── Authors ──
                        "Author" | "contrib" => {
                            // Check contrib is an author
                            let is_author = if name == "contrib" {
                                e.attributes().filter_map(|a| a.ok()).any(|a| {
                                    String::from_utf8_lossy(a.key.as_ref()) == "contrib-type"
                                        && String::from_utf8_lossy(&a.value) == "author"
                                })
                            } else {
                                true
                            };
                            if is_author {
                                author_parts.clear();
                            }
                        }
                        "LastName" | "surname" => in_author_name = true,
                        "ForeName" | "given-names" => in_author_name = true,
                        // ── Keywords ──
                        "Keyword" | "kwd" => in_keyword = true,
                        // ── References ──
                        "Reference" | "ref" => {
                            in_ref = true;
                            current_citation = Some(CitationRef::default());
                        }
                        "ArticleId" | "pub-id" => {
                            if in_ref {
                                // Read id-type attribute
                                let id_type: String = e
                                    .attributes()
                                    .filter_map(|a| a.ok())
                                    .find(|a| {
                                        let key = String::from_utf8_lossy(a.key.as_ref());
                                        key == "IdType" || key == "pub-id-type"
                                    })
                                    .map(|a| String::from_utf8_lossy(&a.value).to_string())
                                    .unwrap_or_default();
                                // Store type in text_buf prefix so we know on End
                                text_buf = format!("__idtype__{}__", id_type);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    match name.as_str() {
                        "PubmedArticle" | "article" => {
                            if let Some(mut rec) = current.take() {
                                if !rec.title.is_empty()
                                    || !rec.pmid.is_empty()
                                    || !rec.doi.is_empty()
                                {
                                    records.push(rec);
                                }
                            }
                        }
                        "ArticleTitle" | "article-title" => {
                            in_title = false;
                            if let Some(ref mut rec) = current {
                                if rec.title.is_empty() && !text_buf.is_empty() {
                                    rec.title = text_buf.trim().to_string();
                                }
                            }
                            text_buf.clear();
                        }
                        "AbstractText" => {
                            in_abstract = false;
                            if let Some(ref mut rec) = current {
                                if !text_buf.is_empty() {
                                    if !rec.abstract_text.is_empty() {
                                        rec.abstract_text.push(' ');
                                    }
                                    rec.abstract_text.push_str(text_buf.trim());
                                }
                            }
                            text_buf.clear();
                        }
                        "Abstract" | "abstract" => {
                            in_abstract = false;
                            // JATS abstract may have nested <p> tags with text
                            if let Some(ref mut rec) = current {
                                if !text_buf.is_empty() && rec.abstract_text.is_empty() {
                                    rec.abstract_text = text_buf.trim().to_string();
                                }
                            }
                            text_buf.clear();
                        }
                        "Title" if in_journal_title => {
                            in_journal_title = false;
                            if let Some(ref mut rec) = current {
                                if rec.journal.is_empty() && !text_buf.is_empty() {
                                    rec.journal = text_buf.trim().to_string();
                                }
                            }
                            text_buf.clear();
                        }
                        "journal-title" => {
                            in_journal_title = false;
                            if let Some(ref mut rec) = current {
                                if rec.journal.is_empty() && !text_buf.is_empty() {
                                    rec.journal = text_buf.trim().to_string();
                                }
                            }
                            text_buf.clear();
                        }
                        "PMID" => {
                            if let Some(ref mut rec) = current {
                                if !in_ref && rec.pmid.is_empty() && !text_buf.is_empty() {
                                    rec.pmid = text_buf.trim().to_string();
                                }
                            }
                            text_buf.clear();
                        }
                        "Year" => {
                            if let Some(ref mut rec) = current {
                                if rec.year.is_empty() && !text_buf.is_empty() {
                                    rec.year = text_buf.trim().to_string();
                                }
                            }
                            text_buf.clear();
                        }
                        "LastName" | "surname" => {
                            in_author_name = false;
                            if !text_buf.is_empty() {
                                author_parts.push(text_buf.trim().to_string());
                            }
                            text_buf.clear();
                        }
                        "ForeName" | "given-names" => {
                            in_author_name = false;
                            if !text_buf.is_empty() {
                                author_parts.push(text_buf.trim().to_string());
                            }
                            text_buf.clear();
                        }
                        "Author" | "contrib" => {
                            if let Some(ref mut rec) = current {
                                if !author_parts.is_empty() {
                                    rec.authors.push(author_parts.join(" "));
                                    author_parts.clear();
                                }
                            }
                        }
                        "Keyword" | "kwd" => {
                            in_keyword = false;
                            if let Some(ref mut rec) = current {
                                if !text_buf.is_empty() {
                                    rec.keywords.push(text_buf.trim().to_string());
                                }
                            }
                            text_buf.clear();
                        }
                        "ArticleId" | "pub-id" => {
                            if in_ref {
                                if let Some(ref mut cit) = current_citation {
                                    // Extract id type from prefix
                                    let val = if text_buf.starts_with("__idtype__") {
                                        let parts: Vec<&str> = text_buf.splitn(3, "__").collect();
                                        // parts: ["", "idtype", "<type>__<value>"]
                                        if parts.len() >= 3 {
                                            let rest = &parts[2..].join("__");
                                            let type_and_val: Vec<&str> =
                                                rest.splitn(2, "__").collect();
                                            if type_and_val.len() == 2 {
                                                let id_type = type_and_val[0];
                                                let id_val = type_and_val[1].trim();
                                                match id_type {
                                                    "doi" => cit.doi = id_val.to_string(),
                                                    "pubmed" => cit.pmid = id_val.to_string(),
                                                    "pmc" => cit.pmc_id = id_val.to_string(),
                                                    _ => {}
                                                }
                                            }
                                        }
                                        String::new()
                                    } else {
                                        text_buf.trim().to_string()
                                    };
                                }
                            } else if let Some(ref mut rec) = current {
                                // Article-level IDs
                                if text_buf.starts_with("__idtype__") {
                                    let combined = text_buf.replace("__idtype__", "");
                                    let parts: Vec<&str> = combined.splitn(2, "__").collect();
                                    if parts.len() == 2 {
                                        match parts[0] {
                                            "doi" => {
                                                if rec.doi.is_empty() {
                                                    rec.doi = parts[1].trim().to_string();
                                                }
                                            }
                                            "pmc" | "pmc-id" => {
                                                if rec.pmc_id.is_empty() {
                                                    rec.pmc_id = parts[1].trim().to_string();
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            text_buf.clear();
                        }
                        "Reference" | "ref" => {
                            in_ref = false;
                            if let Some(ref mut rec) = current {
                                if let Some(cit) = current_citation.take() {
                                    if cit.best_id().is_some() {
                                        rec.citations.push(cit);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }

                    path.pop();
                }
                Ok(Event::Text(e)) => {
                    if in_title || in_abstract || in_journal_title || in_author_name || in_keyword {
                        if let Ok(t) = e.unescape() {
                            text_buf.push_str(&t);
                        }
                    } else {
                        let current_tag = path.last().map(|s| s.as_str()).unwrap_or("");
                        if matches!(
                            current_tag,
                            "PMID" | "Year" | "ArticleId" | "pub-id" | "year"
                        ) {
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

        records
    }
}

impl IngestAdapter for JatsArticleAdapter {
    fn domain(&self) -> &str {
        "article"
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

            let records = Self::parse_articles(&content);

            for record in records {
                let ext_id = record.external_id();
                if !node_ids.insert(ext_id.clone()) {
                    continue;
                }

                let state = record.state();
                let mut tags = vec![
                    "article".to_string(),
                    format!("article:state:{}", state),
                    format!("namespace:{}", self.namespace),
                ];
                if !record.year.is_empty() {
                    tags.push(format!("article:year:{}", record.year));
                }
                if !record.journal.is_empty() {
                    tags.push(format!(
                        "article:journal:{}",
                        record.journal.to_lowercase().replace(' ', "_")
                    ));
                }
                for kw in &record.keywords {
                    tags.push(format!("article:keyword:{}", kw.to_lowercase()));
                }
                match record.format {
                    ArticleFormat::PubMedNlm => tags.push("article:format:pubmed".to_string()),
                    ArticleFormat::Jats => tags.push("article:format:jats".to_string()),
                    _ => {}
                }

                // Article node
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

                // Citation edges
                for cit in &record.citations {
                    if let Some(target_id) = cit.best_id() {
                        let key = (ext_id.clone(), target_id.clone(), "cites".to_string());
                        if edge_keys.insert(key) {
                            if node_ids.insert(target_id.clone()) {
                                let cit_label = if !cit.title.is_empty() {
                                    cit.title.clone()
                                } else {
                                    target_id.clone()
                                };
                                nodes.push(NodeRecord {
                                    id: target_id.clone(),
                                    label: cit_label,
                                    node_type: NodeType::Reference,
                                    tags: vec!["article".to_string(), "article:cited".to_string()],
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
                }

                // Author nodes + edges
                for author in &record.authors {
                    let author_id = format!(
                        "article::author::{}",
                        author.to_lowercase().replace(' ', "_")
                    );
                    if node_ids.insert(author_id.clone()) {
                        nodes.push(NodeRecord {
                            id: author_id.clone(),
                            label: author.clone(),
                            node_type: NodeType::Concept,
                            tags: vec!["article".to_string(), "article:author".to_string()],
                            timestamp,
                            source_path: rel_path.clone(),
                            excerpt: None,
                            namespace: self.namespace.clone(),
                        });
                    }
                    let key = (ext_id.clone(), author_id.clone(), "authored_by".to_string());
                    if edge_keys.insert(key) {
                        edges.push(EdgeRecord {
                            source: ext_id.clone(),
                            target: author_id,
                            relation: "authored_by".to_string(),
                            weight: 1.0,
                        });
                    }
                }

                // Journal node + edge
                if !record.journal.is_empty() {
                    let journal_id = format!(
                        "article::journal::{}",
                        record.journal.to_lowercase().replace(' ', "_")
                    );
                    if node_ids.insert(journal_id.clone()) {
                        nodes.push(NodeRecord {
                            id: journal_id.clone(),
                            label: record.journal.clone(),
                            node_type: NodeType::Concept,
                            tags: vec!["article".to_string(), "article:journal".to_string()],
                            timestamp,
                            source_path: rel_path.clone(),
                            excerpt: None,
                            namespace: self.namespace.clone(),
                        });
                    }
                    let key = (
                        ext_id.clone(),
                        journal_id.clone(),
                        "published_in".to_string(),
                    );
                    if edge_keys.insert(key) {
                        edges.push(EdgeRecord {
                            source: ext_id.clone(),
                            target: journal_id,
                            relation: "published_in".to_string(),
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
                0.5,
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

    #[test]
    fn parses_pubmed_article() {
        let xml = r#"<?xml version="1.0" ?>
<PubmedArticleSet>
<PubmedArticle>
  <MedlineCitation>
    <PMID Version="1">33611339</PMID>
    <Article>
      <Journal>
        <JournalIssue><PubDate><Year>2021</Year></PubDate></JournalIssue>
        <Title>Signal transduction and targeted therapy</Title>
      </Journal>
      <ArticleTitle>The role of m6A modification in biological functions</ArticleTitle>
      <AuthorList>
        <Author><ForeName>Xiulin</ForeName><LastName>Jiang</LastName></Author>
        <Author><ForeName>Baiyang</ForeName><LastName>Liu</LastName></Author>
      </AuthorList>
    </Article>
  </MedlineCitation>
  <PubmedData>
    <ReferenceList>
      <Reference>
        <ArticleIdList>
          <ArticleId IdType="doi">10.1038/nrg.2016.93</ArticleId>
          <ArticleId IdType="pubmed">27629931</ArticleId>
        </ArticleIdList>
      </Reference>
      <Reference>
        <ArticleIdList>
          <ArticleId IdType="doi">10.1038/s41467-019-11713-9</ArticleId>
        </ArticleIdList>
      </Reference>
    </ReferenceList>
  </PubmedData>
</PubmedArticle>
</PubmedArticleSet>"#;

        let records = JatsArticleAdapter::parse_articles(xml);
        assert_eq!(records.len(), 1);
        let rec = &records[0];
        assert_eq!(rec.pmid, "33611339");
        assert_eq!(rec.format, ArticleFormat::PubMedNlm);
        assert_eq!(
            rec.title,
            "The role of m6A modification in biological functions"
        );
        assert_eq!(rec.journal, "Signal transduction and targeted therapy");
        assert_eq!(rec.year, "2021");
        assert_eq!(rec.authors.len(), 2);
        assert!(rec.authors[0].contains("Jiang"));
        assert_eq!(rec.citations.len(), 2);
        assert_eq!(rec.citations[0].doi, "10.1038/nrg.2016.93");
    }

    #[test]
    fn ingest_creates_graph() {
        let adapter = JatsArticleAdapter::new(None);
        let dir = std::env::temp_dir().join("jats-test-basic");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("sample.xml"),
            r#"<?xml version="1.0"?>
<PubmedArticleSet>
<PubmedArticle>
  <MedlineCitation>
    <PMID>12345678</PMID>
    <Article>
      <ArticleTitle>Test Article</ArticleTitle>
      <Journal><Title>Nature</Title></Journal>
      <AuthorList>
        <Author><ForeName>Jane</ForeName><LastName>Doe</LastName></Author>
      </AuthorList>
    </Article>
  </MedlineCitation>
  <PubmedData>
    <ReferenceList>
      <Reference>
        <ArticleIdList>
          <ArticleId IdType="doi">10.1000/test</ArticleId>
        </ArticleIdList>
      </Reference>
    </ReferenceList>
  </PubmedData>
</PubmedArticle>
</PubmedArticleSet>"#,
        )
        .unwrap();

        let (graph, stats) = adapter.ingest(&dir).expect("ingest failed");
        assert!(stats.nodes_created >= 4); // article + author + journal + citation
        assert!(stats.edges_created >= 3); // cites + authored_by + published_in
        assert!(graph.resolve_id("pmid::12345678").is_some());
        assert!(graph.resolve_id("article::author::jane_doe").is_some());
        assert!(graph.resolve_id("article::journal::nature").is_some());

        std::fs::remove_dir_all(&dir).ok();
    }
}
