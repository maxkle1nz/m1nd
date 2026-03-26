use crate::{IngestAdapter, IngestStats};
use m1nd_core::error::M1ndResult;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

// ─── Extracted Record ──────────────────────────────────────────────

#[derive(Default, Debug)]
struct RfcRecord {
    number: String,       // 9110
    title: String,        // HTTP Semantics
    authors: Vec<String>, // Roy T. Fielding, ...
    year: String,
    category: String, // std, info, bcp, exp
    keywords: Vec<String>,
    abstract_text: String,
    obsoletes: Vec<String>, // RFC numbers this obsoletes
    updates: Vec<String>,   // RFC numbers this updates
    references: Vec<RfcRef>,
}

#[derive(Default, Debug, Clone)]
struct RfcRef {
    rfc_number: String, // if seriesInfo name=RFC
    doi: String,
    title: String,
    target: String, // URL
}

impl RfcRecord {
    fn external_id(&self) -> String {
        format!("rfc::{}", self.number)
    }
}

impl RfcRef {
    fn best_id(&self) -> Option<String> {
        if !self.rfc_number.is_empty() {
            Some(format!("rfc::{}", self.rfc_number))
        } else if !self.doi.is_empty() {
            Some(format!("doi::{}", self.doi))
        } else {
            None
        }
    }
}

// ─── Parser ────────────────────────────────────────────────────────

fn parse_rfc_xml(xml: &str) -> Vec<RfcRecord> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut text_buf = String::new();

    let mut records: Vec<RfcRecord> = Vec::new();
    let mut current: Option<RfcRecord> = None;
    let mut current_ref: Option<RfcRef> = None;

    let mut in_title = false;
    let mut in_abstract = false;
    let mut in_keyword = false;
    let mut in_ref_title = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                path.push(name.clone());

                match name.as_str() {
                    "rfc" => {
                        let mut rec = RfcRecord::default();
                        for attr in e.attributes().filter_map(|a| a.ok()) {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            match key.as_str() {
                                "number" => rec.number = val,
                                "category" => rec.category = val,
                                "obsoletes" => {
                                    rec.obsoletes = val
                                        .split(", ")
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect();
                                }
                                "updates" => {
                                    rec.updates = val
                                        .split(", ")
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect();
                                }
                                _ => {}
                            }
                        }
                        current = Some(rec);
                    }
                    "title"
                        if path.iter().any(|p| p == "front")
                            && !path.iter().any(|p| p == "reference") =>
                    {
                        in_title = true;
                    }
                    "title" if path.iter().any(|p| p == "reference") => {
                        in_ref_title = true;
                    }
                    "abstract" => in_abstract = true,
                    "keyword" => in_keyword = true,
                    "date"
                        if path.iter().any(|p| p == "front")
                            && !path.iter().any(|p| p == "reference") =>
                    {
                        if let Some(ref mut rec) = current {
                            for attr in e.attributes().filter_map(|a| a.ok()) {
                                if String::from_utf8_lossy(attr.key.as_ref()) == "year" {
                                    rec.year = String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        }
                    }
                    "author"
                        if path.iter().any(|p| p == "front")
                            && !path.iter().any(|p| p == "reference") =>
                    {
                        if let Some(ref mut rec) = current {
                            for attr in e.attributes().filter_map(|a| a.ok()) {
                                if String::from_utf8_lossy(attr.key.as_ref()) == "fullname" {
                                    let name = String::from_utf8_lossy(&attr.value).to_string();
                                    if !name.is_empty() {
                                        rec.authors.push(name);
                                    }
                                }
                            }
                        }
                    }
                    "reference" => {
                        let mut rf = RfcRef::default();
                        for attr in e.attributes().filter_map(|a| a.ok()) {
                            if String::from_utf8_lossy(attr.key.as_ref()) == "target" {
                                rf.target = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        current_ref = Some(rf);
                    }
                    "seriesInfo" if path.iter().any(|p| p == "reference") => {
                        if let Some(ref mut rf) = current_ref {
                            let mut sname = String::new();
                            let mut sval = String::new();
                            for attr in e.attributes().filter_map(|a| a.ok()) {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => sname = val,
                                    "value" => sval = val,
                                    _ => {}
                                }
                            }
                            match sname.as_str() {
                                "RFC" => rf.rfc_number = sval,
                                "DOI" => rf.doi = sval,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match name.as_str() {
                    "rfc" => {
                        if let Some(rec) = current.take() {
                            if !rec.number.is_empty() {
                                records.push(rec);
                            }
                        }
                    }
                    "title" if in_title => {
                        in_title = false;
                        if let Some(ref mut rec) = current {
                            if rec.title.is_empty() {
                                rec.title = text_buf.trim().to_string();
                            }
                        }
                        text_buf.clear();
                    }
                    "title" if in_ref_title => {
                        in_ref_title = false;
                        if let Some(ref mut rf) = current_ref {
                            rf.title = text_buf.trim().to_string();
                        }
                        text_buf.clear();
                    }
                    "abstract" => {
                        in_abstract = false;
                        if let Some(ref mut rec) = current {
                            if !text_buf.is_empty() {
                                rec.abstract_text = text_buf.trim().to_string();
                            }
                        }
                        text_buf.clear();
                    }
                    "keyword" => {
                        in_keyword = false;
                        if let Some(ref mut rec) = current {
                            if !text_buf.is_empty() {
                                rec.keywords.push(text_buf.trim().to_string());
                            }
                        }
                        text_buf.clear();
                    }
                    "reference" => {
                        if let Some(ref mut rec) = current {
                            if let Some(rf) = current_ref.take() {
                                if rf.best_id().is_some() {
                                    rec.references.push(rf);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                path.pop();
            }
            Ok(Event::Text(e)) => {
                if in_title || in_abstract || in_keyword || in_ref_title {
                    if let Ok(t) = e.unescape() {
                        text_buf.push_str(&t);
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "author"
                        if path.iter().any(|p| p == "front")
                            && !path.iter().any(|p| p == "reference") =>
                    {
                        if let Some(ref mut rec) = current {
                            for attr in e.attributes().filter_map(|a| a.ok()) {
                                if String::from_utf8_lossy(attr.key.as_ref()) == "fullname" {
                                    let name = String::from_utf8_lossy(&attr.value).to_string();
                                    if !name.is_empty() {
                                        rec.authors.push(name);
                                    }
                                }
                            }
                        }
                    }
                    "date"
                        if path.iter().any(|p| p == "front")
                            && !path.iter().any(|p| p == "reference") =>
                    {
                        if let Some(ref mut rec) = current {
                            for attr in e.attributes().filter_map(|a| a.ok()) {
                                if String::from_utf8_lossy(attr.key.as_ref()) == "year" {
                                    rec.year = String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        }
                    }
                    "seriesInfo" if path.iter().any(|p| p == "reference") => {
                        if let Some(ref mut rf) = current_ref {
                            let mut sname = String::new();
                            let mut sval = String::new();
                            for attr in e.attributes().filter_map(|a| a.ok()) {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => sname = val,
                                    "value" => sval = val,
                                    _ => {}
                                }
                            }
                            match sname.as_str() {
                                "RFC" => rf.rfc_number = sval,
                                "DOI" => rf.doi = sval,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
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

// ─── Adapter ───────────────────────────────────────────────────────

pub struct RfcAdapter {
    namespace: String,
}

impl RfcAdapter {
    pub fn new(namespace: Option<String>) -> Self {
        let namespace = namespace
            .unwrap_or_else(|| "rfc".to_string())
            .trim()
            .to_lowercase();
        let namespace = if namespace.is_empty() {
            "rfc".to_string()
        } else {
            namespace
        };
        Self { namespace }
    }

    fn accepted_extension(path: &Path) -> bool {
        matches!(
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase()),
            Some(ext) if ext == "xml"
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
            .filter(|e| e.file_type().is_file() && Self::accepted_extension(e.path()))
            .map(|e| e.into_path())
            .collect()
    }

    fn file_timestamp(path: &Path) -> f64 {
        std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
}

impl IngestAdapter for RfcAdapter {
    fn domain(&self) -> &str {
        "rfc"
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

        struct NodeRec {
            id: String,
            label: String,
            ntype: NodeType,
            tags: Vec<String>,
            ts: f64,
            src: String,
            excerpt: Option<String>,
            ns: String,
        }
        struct EdgeRec {
            source: String,
            target: String,
            rel: String,
            w: f32,
        }

        let mut nodes: Vec<NodeRec> = Vec::new();
        let mut edges: Vec<EdgeRec> = Vec::new();

        for path in &files {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            // Quick check: must contain <rfc
            if !content.contains("<rfc") {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let ts = Self::file_timestamp(path);
            let records = parse_rfc_xml(&content);

            for rec in records {
                let ext_id = rec.external_id();
                if !node_ids.insert(ext_id.clone()) {
                    continue;
                }

                let mut tags = vec![
                    "rfc".to_string(),
                    format!("rfc:category:{}", rec.category),
                    "rfc:state:published".to_string(),
                    format!("namespace:{}", self.namespace),
                ];
                if !rec.year.is_empty() {
                    tags.push(format!("rfc:year:{}", rec.year));
                }
                for kw in &rec.keywords {
                    tags.push(format!("rfc:keyword:{}", kw.to_lowercase()));
                }

                let excerpt = if !rec.abstract_text.is_empty() {
                    Some(rec.abstract_text.chars().take(220).collect())
                } else {
                    None
                };

                nodes.push(NodeRec {
                    id: ext_id.clone(),
                    label: format!("RFC {} — {}", rec.number, rec.title),
                    ntype: NodeType::File,
                    tags,
                    ts,
                    src: rel.clone(),
                    excerpt,
                    ns: self.namespace.clone(),
                });

                // References
                for rf in &rec.references {
                    if let Some(target_id) = rf.best_id() {
                        let key = (ext_id.clone(), target_id.clone(), "references".to_string());
                        if edge_keys.insert(key) {
                            if node_ids.insert(target_id.clone()) {
                                let lbl = if !rf.title.is_empty() {
                                    rf.title.clone()
                                } else {
                                    target_id.clone()
                                };
                                nodes.push(NodeRec {
                                    id: target_id.clone(),
                                    label: lbl,
                                    ntype: NodeType::Reference,
                                    tags: vec!["rfc".to_string(), "rfc:cited".to_string()],
                                    ts,
                                    src: rel.clone(),
                                    excerpt: None,
                                    ns: self.namespace.clone(),
                                });
                            }
                            edges.push(EdgeRec {
                                source: ext_id.clone(),
                                target: target_id,
                                rel: "references".to_string(),
                                w: 0.8,
                            });
                        }
                    }
                }

                // Obsoletes edges
                for obs in &rec.obsoletes {
                    let target_id = format!("rfc::{}", obs);
                    let key = (ext_id.clone(), target_id.clone(), "obsoletes".to_string());
                    if edge_keys.insert(key) {
                        if node_ids.insert(target_id.clone()) {
                            nodes.push(NodeRec {
                                id: target_id.clone(),
                                label: format!("RFC {}", obs),
                                ntype: NodeType::Reference,
                                tags: vec!["rfc".to_string(), "rfc:state:obsoleted".to_string()],
                                ts,
                                src: rel.clone(),
                                excerpt: None,
                                ns: self.namespace.clone(),
                            });
                        }
                        edges.push(EdgeRec {
                            source: ext_id.clone(),
                            target: target_id,
                            rel: "obsoletes".to_string(),
                            w: 1.0,
                        });
                    }
                }

                // Updates edges
                for upd in &rec.updates {
                    let target_id = format!("rfc::{}", upd);
                    let key = (ext_id.clone(), target_id.clone(), "updates".to_string());
                    if edge_keys.insert(key) {
                        if node_ids.insert(target_id.clone()) {
                            nodes.push(NodeRec {
                                id: target_id.clone(),
                                label: format!("RFC {}", upd),
                                ntype: NodeType::Reference,
                                tags: vec!["rfc".to_string()],
                                ts,
                                src: rel.clone(),
                                excerpt: None,
                                ns: self.namespace.clone(),
                            });
                        }
                        edges.push(EdgeRec {
                            source: ext_id.clone(),
                            target: target_id,
                            rel: "updates".to_string(),
                            w: 0.9,
                        });
                    }
                }

                // Author nodes
                for author in &rec.authors {
                    let aid = format!("rfc::author::{}", author.to_lowercase().replace(' ', "_"));
                    if node_ids.insert(aid.clone()) {
                        nodes.push(NodeRec {
                            id: aid.clone(),
                            label: author.clone(),
                            ntype: NodeType::Concept,
                            tags: vec!["rfc".to_string(), "rfc:author".to_string()],
                            ts,
                            src: rel.clone(),
                            excerpt: None,
                            ns: self.namespace.clone(),
                        });
                    }
                    let key = (ext_id.clone(), aid.clone(), "authored_by".to_string());
                    if edge_keys.insert(key) {
                        edges.push(EdgeRec {
                            source: ext_id.clone(),
                            target: aid,
                            rel: "authored_by".to_string(),
                            w: 1.0,
                        });
                    }
                }
            }

            stats.files_parsed += 1;
        }

        // Build graph
        let mut graph = Graph::with_capacity(nodes.len(), edges.len());
        for n in &nodes {
            let tags_ref: Vec<&str> = n.tags.iter().map(String::as_str).collect();
            if let Ok(nid) = graph.add_node(&n.id, &n.label, n.ntype, &tags_ref, n.ts, 0.5) {
                graph.set_node_provenance(
                    nid,
                    NodeProvenanceInput {
                        source_path: Some(&n.src),
                        line_start: None,
                        line_end: None,
                        excerpt: n.excerpt.as_deref(),
                        namespace: Some(&n.ns),
                        canonical: true,
                    },
                );
                stats.nodes_created += 1;
            }
        }
        for e in &edges {
            if let (Some(s), Some(t)) = (graph.resolve_id(&e.source), graph.resolve_id(&e.target)) {
                if graph
                    .add_edge(
                        s,
                        t,
                        &e.rel,
                        FiniteF32::new(e.w),
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
    fn parses_rfc_xml() {
        let xml = r#"<?xml version="1.0"?>
<rfc number="9110" category="std" obsoletes="7230, 7231" updates="3864">
  <front>
    <title>HTTP Semantics</title>
    <author fullname="Roy T. Fielding"/>
    <author fullname="Mark Nottingham"/>
    <date year="2022" month="06"/>
    <keyword>HTTP</keyword>
    <keyword>protocol</keyword>
    <abstract><t>The Hypertext Transfer Protocol is stateless.</t></abstract>
  </front>
  <back>
    <references>
      <reference target="https://www.rfc-editor.org/info/rfc9111">
        <front><title>HTTP Caching</title></front>
        <seriesInfo name="RFC" value="9111"/>
      </reference>
      <reference>
        <front><title>Some DOI paper</title></front>
        <seriesInfo name="DOI" value="10.1000/test"/>
      </reference>
    </references>
  </back>
</rfc>"#;

        let records = parse_rfc_xml(xml);
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.number, "9110");
        assert_eq!(r.title, "HTTP Semantics");
        assert_eq!(r.authors.len(), 2);
        assert_eq!(r.category, "std");
        assert_eq!(r.obsoletes, vec!["7230", "7231"]);
        assert_eq!(r.updates, vec!["3864"]);
        assert_eq!(r.keywords.len(), 2);
        assert_eq!(r.references.len(), 2);
        assert_eq!(r.references[0].rfc_number, "9111");
        assert_eq!(r.references[1].doi, "10.1000/test");
    }

    #[test]
    fn ingest_creates_graph() {
        let adapter = RfcAdapter::new(None);
        let dir = std::env::temp_dir().join("rfc-test-basic");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("rfc.xml"),
            r#"<?xml version="1.0"?>
<rfc number="7231" category="std">
  <front>
    <title>HTTP/1.1 Semantics</title>
    <author fullname="Roy Fielding"/>
    <date year="2014"/>
  </front>
  <back>
    <references>
      <reference><front><title>URI</title></front>
        <seriesInfo name="RFC" value="3986"/>
      </reference>
    </references>
  </back>
</rfc>"#,
        )
        .unwrap();

        let (graph, stats) = adapter.ingest(&dir).expect("ingest failed");
        assert!(stats.nodes_created >= 3); // rfc + author + reference
        assert!(stats.edges_created >= 2); // references + authored_by
        assert!(graph.resolve_id("rfc::7231").is_some());
        assert!(graph.resolve_id("rfc::3986").is_some());
        assert!(graph.resolve_id("rfc::author::roy_fielding").is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ingest_real_rfc() {
        let path = Path::new("/tmp/patent-test/rfc9110.xml");
        if !path.exists() {
            println!("SKIP: real RFC data not available");
            return;
        }

        let adapter = RfcAdapter::new(None);
        let (graph, stats) = adapter.ingest(path).expect("ingest failed");

        println!("=== Real RFC 9110 (HTTP Semantics, 1.2MB) ===");
        println!("  nodes: {}", stats.nodes_created);
        println!("  edges: {}", stats.edges_created);
        println!("  elapsed: {:.2}ms", stats.elapsed_ms);

        assert!(graph.resolve_id("rfc::9110").is_some());
        assert!(graph.resolve_id("rfc::author::roy_t._fielding").is_some());
        // Should reference RFC 9111 (HTTP Caching)
        assert!(graph.resolve_id("rfc::9111").is_some());
        assert!(
            stats.nodes_created >= 50,
            "expected many nodes from 84 references"
        );

        println!("  PASS ✓");
    }
}
