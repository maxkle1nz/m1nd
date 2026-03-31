use crate::{IngestAdapter, IngestStats};
use m1nd_core::error::M1ndResult;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

// ─── Extracted Record ──────────────────────────────────────────────

#[derive(Default, Debug)]
struct BibRecord {
    entry_type: String, // article, inproceedings, book, etc.
    cite_key: String,   // vaswani2017attention
    title: String,
    authors: Vec<String>,
    year: String,
    journal: String, // or booktitle for inproceedings
    doi: String,
    abstract_text: String,
    keywords: Vec<String>,
}

impl BibRecord {
    fn external_id(&self) -> String {
        if !self.doi.is_empty() {
            format!("doi::{}", self.doi)
        } else {
            format!("bibtex::{}", self.cite_key)
        }
    }

    fn label(&self) -> String {
        if !self.title.is_empty() {
            self.title.clone()
        } else {
            self.cite_key.clone()
        }
    }

    fn venue(&self) -> &str {
        &self.journal
    }
}

// ─── BibTeX Parser ─────────────────────────────────────────────────

fn parse_bibtex(content: &str) -> Vec<BibRecord> {
    let mut records = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut pos = 0;

    while pos < len {
        // Find @type{key,
        if chars[pos] == '@' {
            pos += 1;
            // Read entry type
            let type_start = pos;
            while pos < len && chars[pos] != '{' && !chars[pos].is_whitespace() {
                pos += 1;
            }
            let entry_type = chars[type_start..pos]
                .iter()
                .collect::<String>()
                .to_lowercase();

            // Skip comment/string/preamble
            if matches!(entry_type.as_str(), "comment" | "string" | "preamble") {
                // Skip to matching brace
                if let Some(end) = find_matching_brace(&chars, pos) {
                    pos = end + 1;
                } else {
                    pos += 1;
                }
                continue;
            }

            // Skip whitespace and opening brace
            while pos < len && (chars[pos].is_whitespace() || chars[pos] == '{') {
                pos += 1;
            }

            // Read cite key
            let key_start = pos;
            while pos < len && chars[pos] != ',' && !chars[pos].is_whitespace() {
                pos += 1;
            }
            let cite_key = chars[key_start..pos].iter().collect::<String>();
            if pos < len && chars[pos] == ',' {
                pos += 1;
            }

            // Find end of entry (matching brace)
            let entry_end = if let Some(end) =
                find_matching_brace(&chars, pos - (cite_key.len() + 2).min(pos))
            {
                end
            } else {
                continue;
            };

            // Parse fields between pos and entry_end
            let fields = parse_fields(&chars[pos..entry_end]);

            let mut rec = BibRecord {
                entry_type,
                cite_key,
                ..Default::default()
            };

            for (key, val) in &fields {
                match key.as_str() {
                    "title" => rec.title = clean_bibtex_value(val),
                    "author" => rec.authors = parse_authors(val),
                    "year" => rec.year = val.trim().to_string(),
                    "journal" | "journaltitle" => rec.journal = clean_bibtex_value(val),
                    "booktitle" => {
                        if rec.journal.is_empty() {
                            rec.journal = clean_bibtex_value(val);
                        }
                    }
                    "doi" => rec.doi = val.trim().to_string(),
                    "abstract" => rec.abstract_text = clean_bibtex_value(val),
                    "keywords" => {
                        rec.keywords = val
                            .split([',', ';'])
                            .map(|k| k.trim().to_string())
                            .filter(|k| !k.is_empty())
                            .collect();
                    }
                    _ => {}
                }
            }

            if !rec.title.is_empty() || !rec.cite_key.is_empty() {
                records.push(rec);
            }

            pos = entry_end + 1;
        } else {
            pos += 1;
        }
    }

    records
}

fn find_matching_brace(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 0;
    let mut pos = start;
    while pos < chars.len() {
        match chars[pos] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

fn parse_fields(chars: &[char]) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    let content: String = chars.iter().collect();

    // Simple field parser: key = {value} or key = "value" or key = number
    let mut remaining = content.as_str().trim();

    while !remaining.is_empty() {
        // Skip whitespace and commas
        remaining = remaining.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if remaining.is_empty() {
            break;
        }

        // Find key
        let eq_pos = match remaining.find('=') {
            Some(p) => p,
            None => break,
        };
        let key = remaining[..eq_pos].trim().to_lowercase();
        remaining = remaining[eq_pos + 1..].trim();

        // Read value
        let (val, rest) = if remaining.starts_with('{') {
            read_braced_value(remaining)
        } else if remaining.starts_with('"') {
            read_quoted_value(remaining)
        } else {
            // Bare value (number or macro)
            let end = remaining.find([',', '}']).unwrap_or(remaining.len());
            let val = remaining[..end].trim().to_string();
            let rest = if end < remaining.len() {
                &remaining[end..]
            } else {
                ""
            };
            (val, rest)
        };

        if !key.is_empty() {
            fields.push((key, val));
        }
        remaining = rest;
    }

    fields
}

fn read_braced_value(s: &str) -> (String, &str) {
    let chars: Vec<char> = s.chars().collect();
    let mut depth = 0;
    let mut pos = 0;
    let mut start = 0;

    while pos < chars.len() {
        match chars[pos] {
            '{' => {
                if depth == 0 {
                    start = pos + 1;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let byte_start = chars[..start].iter().collect::<String>().len();
                    let byte_end = chars[..pos].iter().collect::<String>().len();
                    return (s[byte_start..byte_end].to_string(), &s[byte_end + 1..]);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    (String::new(), "")
}

fn read_quoted_value(s: &str) -> (String, &str) {
    if let Some(end) = s[1..].find('"') {
        (s[1..end + 1].to_string(), &s[end + 2..])
    } else {
        (String::new(), "")
    }
}

fn clean_bibtex_value(s: &str) -> String {
    s.replace(['{', '}'], "")
        .replace('\n', " ")
        .replace("  ", " ")
        .trim()
        .to_string()
}

fn parse_authors(s: &str) -> Vec<String> {
    s.split(" and ")
        .map(|a| {
            let cleaned = clean_bibtex_value(a.trim());
            // Handle "Last, First" format → "First Last"
            if let Some(comma_pos) = cleaned.find(',') {
                let last = cleaned[..comma_pos].trim();
                let first = cleaned[comma_pos + 1..].trim();
                format!("{} {}", first, last)
            } else {
                cleaned
            }
        })
        .filter(|a| !a.is_empty())
        .collect()
}

// ─── Adapter ───────────────────────────────────────────────────────

pub struct BibTexAdapter {
    namespace: String,
}

impl BibTexAdapter {
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
            Some(ext) if matches!(ext.as_str(), "bib" | "bibtex")
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

impl IngestAdapter for BibTexAdapter {
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
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let ts = Self::file_timestamp(path);

            let records = parse_bibtex(&content);

            for rec in records {
                let ext_id = rec.external_id();
                if !node_ids.insert(ext_id.clone()) {
                    continue;
                }

                let mut tags = vec![
                    "article".to_string(),
                    format!("article:type:{}", rec.entry_type),
                    "article:state:published".to_string(),
                    format!("namespace:{}", self.namespace),
                ];
                if !rec.year.is_empty() {
                    tags.push(format!("article:year:{}", rec.year));
                }
                if !rec.journal.is_empty() {
                    tags.push(format!(
                        "article:venue:{}",
                        rec.journal.to_lowercase().replace(' ', "_")
                    ));
                }
                for kw in &rec.keywords {
                    tags.push(format!("article:keyword:{}", kw.to_lowercase()));
                }
                tags.push("article:format:bibtex".to_string());

                let excerpt = if !rec.abstract_text.is_empty() {
                    Some(rec.abstract_text.chars().take(220).collect())
                } else {
                    None
                };

                nodes.push(NodeRec {
                    id: ext_id.clone(),
                    label: rec.label(),
                    ntype: NodeType::File,
                    tags,
                    ts,
                    src: rel.clone(),
                    excerpt,
                    ns: self.namespace.clone(),
                });

                // Author nodes + edges
                for author in &rec.authors {
                    let aid = format!(
                        "article::author::{}",
                        author.to_lowercase().replace(' ', "_")
                    );
                    if node_ids.insert(aid.clone()) {
                        nodes.push(NodeRec {
                            id: aid.clone(),
                            label: author.clone(),
                            ntype: NodeType::Concept,
                            tags: vec!["article".to_string(), "article:author".to_string()],
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

                // Venue node + edge
                let venue = rec.venue();
                if !venue.is_empty() {
                    let vid = format!("article::venue::{}", venue.to_lowercase().replace(' ', "_"));
                    if node_ids.insert(vid.clone()) {
                        nodes.push(NodeRec {
                            id: vid.clone(),
                            label: venue.to_string(),
                            ntype: NodeType::Concept,
                            tags: vec!["article".to_string(), "article:venue".to_string()],
                            ts,
                            src: rel.clone(),
                            excerpt: None,
                            ns: self.namespace.clone(),
                        });
                    }
                    let key = (ext_id.clone(), vid.clone(), "published_in".to_string());
                    if edge_keys.insert(key) {
                        edges.push(EdgeRec {
                            source: ext_id.clone(),
                            target: vid,
                            rel: "published_in".to_string(),
                            w: 0.9,
                        });
                    }
                }
            }

            stats.files_parsed += 1;
        }

        // Build graph
        let mut graph = Graph::with_capacity(nodes.len(), edges.len());
        for n in &nodes {
            let tags: Vec<&str> = n.tags.iter().map(String::as_str).collect();
            if let Ok(nid) = graph.add_node(&n.id, &n.label, n.ntype, &tags, n.ts, 0.5) {
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
    fn parses_basic_bibtex() {
        let bib = r#"
@article{vaswani2017attention,
  title={Attention Is All You Need},
  author={Vaswani, Ashish and Shazeer, Noam and Parmar, Niki},
  journal={NeurIPS},
  year={2017},
  doi={10.48550/arXiv.1706.03762}
}

@inproceedings{devlin2019bert,
  title={BERT: Pre-training of Deep Bidirectional Transformers},
  author={Devlin, Jacob and Chang, Ming-Wei},
  booktitle={NAACL},
  year={2019}
}
"#;
        let records = parse_bibtex(bib);
        assert_eq!(records.len(), 2);

        assert_eq!(records[0].title, "Attention Is All You Need");
        assert_eq!(records[0].cite_key, "vaswani2017attention");
        assert_eq!(records[0].year, "2017");
        assert_eq!(records[0].journal, "NeurIPS");
        assert_eq!(records[0].doi, "10.48550/arXiv.1706.03762");
        assert_eq!(records[0].authors.len(), 3);
        assert!(records[0].authors[0].contains("Ashish"));

        assert_eq!(records[1].entry_type, "inproceedings");
        assert_eq!(records[1].journal, "NAACL"); // booktitle fallback
        assert_eq!(records[1].authors.len(), 2);
    }

    #[test]
    fn ingest_creates_graph() {
        let adapter = BibTexAdapter::new(None);
        let dir = std::env::temp_dir().join("bibtex-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("refs.bib"),
            r#"@article{test2024,
  title={Test Article},
  author={Smith, John and Doe, Jane},
  journal={Nature},
  year={2024},
  doi={10.1000/test}
}"#,
        )
        .unwrap();

        let (graph, stats) = adapter.ingest(&dir).expect("ingest failed");
        assert!(stats.nodes_created >= 4); // article + 2 authors + venue
        assert!(stats.edges_created >= 3); // 2 authored_by + published_in
        assert!(graph.resolve_id("doi::10.1000/test").is_some());
        assert!(graph.resolve_id("article::author::john_smith").is_some());
        assert!(graph.resolve_id("article::venue::nature").is_some());

        std::fs::remove_dir_all(&dir).ok();
    }
}
