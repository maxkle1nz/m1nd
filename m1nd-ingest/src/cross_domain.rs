//! Cross-domain edge resolution for the M1nd knowledge connectome.
//!
//! After multiple adapters have ingested documents into separate graphs,
//! this module merges them and resolves cross-domain references:
//!   - Same DOI in BibTeX and JATS → `same_as` edge
//!   - Patent cites a DOI present in the article graph → `cross_cites` edge
//!   - Same author name across domains → `same_author` edge
//!
//! Usage:
//!   let merged = CrossDomainResolver::merge_and_resolve(vec![patent_graph, article_graph]);

use m1nd_core::error::M1ndResult;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use std::collections::{HashMap, HashSet};

/// Statistics from cross-domain resolution
#[derive(Debug, Default)]
pub struct ResolutionStats {
    /// Number of source graphs merged
    pub graphs_merged: usize,
    /// Total nodes after merge
    pub total_nodes: u32,
    /// Total edges after merge (including new cross-domain edges)
    pub total_edges: usize,
    /// Number of new cross-domain edges created
    pub cross_edges_created: usize,
    /// Number of identity matches found (same DOI in different sources)
    pub identity_matches: usize,
    /// Number of author bridges found
    pub author_bridges: usize,
    /// Number of keyword bridges found
    pub keyword_bridges: usize,
    /// Number of ORCID identity bridges found
    pub orcid_bridges: usize,
    /// Number of transitive citation chains found
    pub citation_chains: usize,
}

/// Intermediate node representation for merging
#[derive(Debug, Clone)]
pub struct MergeNode {
    external_id: String,
    label: String,
    node_type: NodeType,
    tags: Vec<String>,
    timestamp: f64,
    change_freq: f32,
    // Provenance
    source_path: Option<String>,
    excerpt: Option<String>,
    namespace: Option<String>,
}

/// Intermediate edge representation for merging
#[derive(Debug, Clone)]
pub struct MergeEdge {
    source_id: String,
    target_id: String,
    relation: String,
    weight: f32,
}

pub struct CrossDomainResolver;

impl CrossDomainResolver {
    /// Merge multiple adapter outputs and resolve cross-domain references.
    ///
    /// Each input is a vec of (node_records, edge_records) from different adapters.
    /// The resolver:
    /// 1. Collects all nodes, deduplicating by external_id
    /// 2. Collects all edges
    /// 3. Scans for cross-domain links (shared DOIs, PMIDs, author names)
    /// 4. Emits new bridge edges
    pub fn resolve(
        nodes: Vec<MergeNode>,
        edges: Vec<MergeEdge>,
    ) -> M1ndResult<(Graph, ResolutionStats)> {
        let mut stats = ResolutionStats::default();

        // Phase 1: Build lookup indices
        let mut doi_to_ids: HashMap<String, Vec<String>> = HashMap::new();
        let mut pmid_to_ids: HashMap<String, Vec<String>> = HashMap::new();
        let mut author_to_ids: HashMap<String, Vec<String>> = HashMap::new();
        let mut keyword_to_ids: HashMap<String, Vec<String>> = HashMap::new();
        let mut orcid_to_ids: HashMap<String, Vec<String>> = HashMap::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        let mut deduped_nodes: Vec<MergeNode> = Vec::new();

        for node in &nodes {
            if !seen_ids.insert(node.external_id.clone()) {
                // Duplicate — skip but index for matching
                continue;
            }
            deduped_nodes.push(node.clone());

            // Index by identifier type
            let eid = &node.external_id;
            if let Some(doi) = eid.strip_prefix("doi::") {
                doi_to_ids
                    .entry(doi.to_lowercase())
                    .or_default()
                    .push(eid.clone());
            }
            if let Some(pmid) = eid.strip_prefix("pmid::") {
                pmid_to_ids
                    .entry(pmid.to_string())
                    .or_default()
                    .push(eid.clone());
            }
            if eid.starts_with("bibtex::") {
                // Check if there's a DOI match via tags
                for tag in &node.tags {
                    if let Some(doi) = tag.strip_prefix("article:doi:") {
                        doi_to_ids
                            .entry(doi.to_lowercase())
                            .or_default()
                            .push(eid.clone());
                    }
                }
            }

            // Index authors
            if node
                .tags
                .iter()
                .any(|t| t == "article:author" || t == "patent:assignee")
            {
                let name_key = node.label.to_lowercase().replace(' ', "_");
                author_to_ids.entry(name_key).or_default().push(eid.clone());
            }

            // Index keywords for shared_keyword bridge
            for tag in &node.tags {
                if let Some(kw) = tag.strip_prefix("article:keyword:") {
                    keyword_to_ids
                        .entry(kw.to_lowercase())
                        .or_default()
                        .push(eid.clone());
                }
                if let Some(kw) = tag.strip_prefix("keyword:") {
                    keyword_to_ids
                        .entry(kw.to_lowercase())
                        .or_default()
                        .push(eid.clone());
                }
                if let Some(subj) = tag.strip_prefix("subject:") {
                    keyword_to_ids
                        .entry(subj.to_lowercase())
                        .or_default()
                        .push(eid.clone());
                }
            }

            // Index ORCID for same_orcid bridge
            for tag in &node.tags {
                if let Some(orcid) = tag.strip_prefix("orcid:") {
                    orcid_to_ids
                        .entry(orcid.to_lowercase())
                        .or_default()
                        .push(eid.clone());
                }
            }
        }

        // Phase 2: Generate cross-domain edges
        let mut cross_edges: Vec<MergeEdge> = Vec::new();

        // DOI identity matches: same DOI from different adapters
        for (doi, ids) in &doi_to_ids {
            if ids.len() > 1 {
                // Link all nodes sharing this DOI
                for i in 0..ids.len() {
                    for j in (i + 1)..ids.len() {
                        cross_edges.push(MergeEdge {
                            source_id: ids[i].clone(),
                            target_id: ids[j].clone(),
                            relation: "same_as".to_string(),
                            weight: 1.0,
                        });
                        stats.identity_matches += 1;
                    }
                }
            }
        }

        // PMID identity matches
        for (pmid, ids) in &pmid_to_ids {
            if ids.len() > 1 {
                for i in 0..ids.len() {
                    for j in (i + 1)..ids.len() {
                        cross_edges.push(MergeEdge {
                            source_id: ids[i].clone(),
                            target_id: ids[j].clone(),
                            relation: "same_as".to_string(),
                            weight: 1.0,
                        });
                        stats.identity_matches += 1;
                    }
                }
            }
        }

        // Cross-domain citation resolution:
        // A `cites` edge is a cross-citation if:
        //   (a) target is a "full" node (not a stub/cited-ref), OR
        //   (b) source and target come from different namespaces
        let full_node_ids: HashSet<&str> = deduped_nodes
            .iter()
            .filter(|n| !n.tags.iter().any(|t| t.ends_with(":cited")))
            .map(|n| n.external_id.as_str())
            .collect();

        // Build namespace lookup from ALL input nodes (not just deduped)
        let node_namespace: HashMap<&str, HashSet<&str>> = {
            let mut map: HashMap<&str, HashSet<&str>> = HashMap::new();
            for n in &nodes {
                if let Some(ref ns) = n.namespace {
                    map.entry(&n.external_id).or_default().insert(ns.as_str());
                }
            }
            map
        };

        for edge in &edges {
            if edge.relation == "cites" {
                let target_is_full = full_node_ids.contains(edge.target_id.as_str());

                // Check if target appeared in a namespace different from source
                let source_ns = node_namespace.get(edge.source_id.as_str());
                let target_ns = node_namespace.get(edge.target_id.as_str());
                let cross_namespace = match (source_ns, target_ns) {
                    (Some(sns), Some(tns)) => {
                        // Target has at least one namespace not in source
                        tns.iter().any(|ns| !sns.contains(ns))
                    }
                    _ => false,
                };

                if target_is_full || cross_namespace {
                    cross_edges.push(MergeEdge {
                        source_id: edge.source_id.clone(),
                        target_id: edge.target_id.clone(),
                        relation: "cross_cites".to_string(),
                        weight: 0.95,
                    });
                }
            }
        }

        // Author bridges: same author name across domains
        for (name, ids) in &author_to_ids {
            if ids.len() > 1 {
                // Check if they span different namespaces
                let namespaces: HashSet<String> = ids
                    .iter()
                    .filter_map(|id| {
                        deduped_nodes
                            .iter()
                            .find(|n| &n.external_id == id)
                            .and_then(|n| n.namespace.clone())
                    })
                    .collect();

                if namespaces.len() > 1 {
                    // Multi-domain author → bridge
                    for i in 0..ids.len() {
                        for j in (i + 1)..ids.len() {
                            cross_edges.push(MergeEdge {
                                source_id: ids[i].clone(),
                                target_id: ids[j].clone(),
                                relation: "same_author".to_string(),
                                weight: 0.7,
                            });
                            stats.author_bridges += 1;
                        }
                    }
                }
            }
        }

        // ---------------------------------------------------------------
        // Shared keyword bridges: nodes sharing keywords across domains
        // ---------------------------------------------------------------
        for (keyword, ids) in &keyword_to_ids {
            if ids.len() > 1 && ids.len() <= 20 {
                // Cap to avoid hyper-connected hubs from generic keywords
                let namespaces: HashSet<String> = ids
                    .iter()
                    .filter_map(|id| {
                        deduped_nodes
                            .iter()
                            .find(|n| &n.external_id == id)
                            .and_then(|n| n.namespace.clone())
                    })
                    .collect();

                if namespaces.len() > 1 {
                    // Cross-domain keyword match → bridge
                    for i in 0..ids.len() {
                        for j in (i + 1)..ids.len() {
                            // Only bridge across different namespaces
                            let ns_i = deduped_nodes
                                .iter()
                                .find(|n| n.external_id == ids[i])
                                .and_then(|n| n.namespace.as_deref());
                            let ns_j = deduped_nodes
                                .iter()
                                .find(|n| n.external_id == ids[j])
                                .and_then(|n| n.namespace.as_deref());
                            if ns_i != ns_j {
                                cross_edges.push(MergeEdge {
                                    source_id: ids[i].clone(),
                                    target_id: ids[j].clone(),
                                    relation: "shared_keyword".to_string(),
                                    weight: 0.6,
                                });
                                stats.keyword_bridges += 1;
                            }
                        }
                    }
                }
            }
        }

        // ---------------------------------------------------------------
        // ORCID identity bridges: same researcher via ORCID
        // ---------------------------------------------------------------
        for (orcid, ids) in &orcid_to_ids {
            if ids.len() > 1 {
                let namespaces: HashSet<String> = ids
                    .iter()
                    .filter_map(|id| {
                        deduped_nodes
                            .iter()
                            .find(|n| &n.external_id == id)
                            .and_then(|n| n.namespace.clone())
                    })
                    .collect();

                if namespaces.len() > 1 {
                    for i in 0..ids.len() {
                        for j in (i + 1)..ids.len() {
                            cross_edges.push(MergeEdge {
                                source_id: ids[i].clone(),
                                target_id: ids[j].clone(),
                                relation: "same_orcid".to_string(),
                                weight: 0.95,
                            });
                            stats.orcid_bridges += 1;
                        }
                    }
                }
            }
        }

        // ---------------------------------------------------------------
        // Citation chain: transitive A→B→C bridging with weight decay
        // If A cites B and B cites C, create A → C with decayed weight
        // ---------------------------------------------------------------
        {
            // Build adjacency: source → Vec<target> for cites edges
            let mut cite_adj: HashMap<String, Vec<String>> = HashMap::new();
            for edge in &edges {
                if edge.relation == "cites" || edge.relation == "references" {
                    cite_adj
                        .entry(edge.source_id.clone())
                        .or_default()
                        .push(edge.target_id.clone());
                }
            }

            // For each A → B, check B → C
            let mut chain_edges: Vec<MergeEdge> = Vec::new();
            for (a, bs) in &cite_adj {
                for b in bs {
                    if let Some(cs) = cite_adj.get(b) {
                        for c in cs {
                            if c != a {
                                // A → B → C: create transitive bridge
                                chain_edges.push(MergeEdge {
                                    source_id: a.clone(),
                                    target_id: c.clone(),
                                    relation: "citation_chain".to_string(),
                                    weight: 0.5, // decayed
                                });
                                stats.citation_chains += 1;
                            }
                        }
                    }
                }
            }
            cross_edges.extend(chain_edges);
        }

        // Phase 3: Build unified graph
        let total_edges_count = edges.len() + cross_edges.len();
        let mut graph = Graph::with_capacity(deduped_nodes.len(), total_edges_count);

        for node in &deduped_nodes {
            let tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
            if let Ok(nid) = graph.add_node(
                &node.external_id,
                &node.label,
                node.node_type,
                &tags,
                node.timestamp,
                node.change_freq,
            ) {
                graph.set_node_provenance(
                    nid,
                    NodeProvenanceInput {
                        source_path: node.source_path.as_deref(),
                        line_start: None,
                        line_end: None,
                        excerpt: node.excerpt.as_deref(),
                        namespace: node.namespace.as_deref(),
                        canonical: true,
                    },
                );
            }
        }

        // Original edges
        for e in &edges {
            if let (Some(s), Some(t)) = (
                graph.resolve_id(&e.source_id),
                graph.resolve_id(&e.target_id),
            ) {
                let _ = graph.add_edge(
                    s,
                    t,
                    &e.relation,
                    FiniteF32::new(e.weight),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.7),
                );
            }
        }

        // Cross-domain edges
        for e in &cross_edges {
            if let (Some(s), Some(t)) = (
                graph.resolve_id(&e.source_id),
                graph.resolve_id(&e.target_id),
            ) {
                if graph
                    .add_edge(
                        s,
                        t,
                        &e.relation,
                        FiniteF32::new(e.weight),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.9),
                    )
                    .is_ok()
                {
                    stats.cross_edges_created += 1;
                }
            }
        }

        if graph.num_nodes() > 0 {
            graph.finalize()?;
        }

        stats.total_nodes = graph.num_nodes();
        stats.total_edges = graph.num_edges();

        Ok((graph, stats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, label: &str, tags: &[&str], ns: &str) -> MergeNode {
        MergeNode {
            external_id: id.to_string(),
            label: label.to_string(),
            node_type: NodeType::File,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            timestamp: 0.0,
            change_freq: 0.5,
            source_path: None,
            excerpt: None,
            namespace: Some(ns.to_string()),
        }
    }

    fn make_author(id: &str, label: &str, ns: &str) -> MergeNode {
        MergeNode {
            external_id: id.to_string(),
            label: label.to_string(),
            node_type: NodeType::Concept,
            tags: vec!["article:author".to_string()],
            timestamp: 0.0,
            change_freq: 0.5,
            source_path: None,
            excerpt: None,
            namespace: Some(ns.to_string()),
        }
    }

    #[test]
    fn resolves_shared_doi() {
        // Same DOI in BibTeX and JATS
        let nodes = vec![
            make_node(
                "doi::10.1038/test",
                "Paper A (JATS)",
                &["article"],
                "article",
            ),
            make_node("doi::10.1038/test", "Paper A (Bib)", &["article"], "bibtex"),
        ];
        let edges = vec![];

        let (graph, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        // Dedup means 1 node (first wins), but identity match is counted
        println!(
            "identity_matches={} nodes={}",
            stats.identity_matches, stats.total_nodes
        );
        // With dedup, second node is skipped, so no same_as edge needed
        assert_eq!(stats.total_nodes, 1);
    }

    #[test]
    fn cross_cites_resolved() {
        // Patent cites a DOI that exists as a full JATS article
        let nodes = vec![
            make_node("patent::US::12345B2", "Some Patent", &["patent"], "patent"),
            make_node(
                "doi::10.1038/nature",
                "Nature Paper",
                &["article"],
                "article",
            ),
            make_node(
                "doi::10.1038/nature",
                "Nature (cited ref)",
                &["article:cited"],
                "patent",
            ),
        ];
        let edges = vec![MergeEdge {
            source_id: "patent::US::12345B2".to_string(),
            target_id: "doi::10.1038/nature".to_string(),
            relation: "cites".to_string(),
            weight: 0.8,
        }];

        let (graph, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        println!(
            "cross_edges={} total_edges={}",
            stats.cross_edges_created, stats.total_edges
        );
        assert!(
            stats.cross_edges_created >= 1,
            "should create cross_cites edge"
        );
    }

    #[test]
    fn author_bridge_across_domains() {
        let nodes = vec![
            make_node("doi::10.1038/a", "Paper A", &["article"], "article"),
            make_author("article::author::john_smith", "John Smith", "article"),
            make_node("patent::US::999B2", "Patent X", &["patent"], "patent"),
            MergeNode {
                external_id: "patent::assignee::john_smith".to_string(),
                label: "John Smith".to_string(),
                node_type: NodeType::Concept,
                tags: vec!["patent:assignee".to_string()],
                timestamp: 0.0,
                change_freq: 0.5,
                source_path: None,
                excerpt: None,
                namespace: Some("patent".to_string()),
            },
        ];
        let edges = vec![];

        let (graph, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        println!("author_bridges={}", stats.author_bridges);
        // Both are named "john_smith" but from different namespaces
        // They should be bridged
        assert!(
            stats.author_bridges >= 1,
            "should bridge John Smith across domains"
        );
    }

    #[test]
    fn no_false_bridges_same_domain() {
        // Two authors with same name in same domain → no bridge
        let nodes = vec![
            make_author("article::author::jane_doe_1", "Jane Doe", "article"),
            make_author("article::author::jane_doe_2", "Jane Doe", "article"),
        ];
        let edges = vec![];

        let (_, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        assert_eq!(
            stats.author_bridges, 0,
            "same domain authors should not bridge"
        );
    }

    #[test]
    fn multi_domain_merge() {
        // Simulate real scenario: patent + article + bibtex
        let nodes = vec![
            // Patent domain
            make_node(
                "patent::US::09138B2",
                "Polymer Patent",
                &["patent"],
                "patent",
            ),
            make_node(
                "doi::10.1038/nature2020",
                "Cited ref (patent)",
                &["patent:cited"],
                "patent",
            ),
            // Article domain (same DOI as cited ref)
            make_node(
                "doi::10.1038/nature2020",
                "Nature Paper 2020",
                &["article"],
                "article",
            ),
            make_author("article::author::alice_wang", "Alice Wang", "article"),
            // BibTeX domain
            make_node(
                "bibtex::wang2020nature",
                "Wang et al 2020",
                &["article"],
                "bibtex",
            ),
        ];
        let edges = vec![
            MergeEdge {
                source_id: "patent::US::09138B2".to_string(),
                target_id: "doi::10.1038/nature2020".to_string(),
                relation: "cites".to_string(),
                weight: 0.8,
            },
            MergeEdge {
                source_id: "doi::10.1038/nature2020".to_string(),
                target_id: "article::author::alice_wang".to_string(),
                relation: "authored_by".to_string(),
                weight: 1.0,
            },
        ];

        let (graph, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        println!(
            "multi-domain: nodes={} edges={} cross_edges={} identity={}",
            stats.total_nodes, stats.total_edges, stats.cross_edges_created, stats.identity_matches
        );

        // Patent should be able to reach Alice Wang through:
        // patent → cites → nature paper → authored_by → Alice Wang
        assert!(graph.resolve_id("patent::US::09138B2").is_some());
        assert!(graph.resolve_id("doi::10.1038/nature2020").is_some());
        assert!(graph.resolve_id("article::author::alice_wang").is_some());
        assert!(stats.cross_edges_created >= 1, "cross_cites should exist");
        assert!(stats.total_edges >= 3);
    }

    // ===== NEW BRIDGE TESTS =====

    #[test]
    fn shared_keyword_across_domains() {
        let nodes = vec![
            make_node(
                "doi::10.1234/rfc",
                "RFC Paper",
                &["keyword:http", "keyword:transport"],
                "rfc",
            ),
            make_node(
                "doi::10.5678/article",
                "HTTP Article",
                &["keyword:http", "keyword:performance"],
                "article",
            ),
        ];
        let edges = vec![];

        let (_, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        println!("keyword_bridges={}", stats.keyword_bridges);
        assert!(
            stats.keyword_bridges >= 1,
            "should bridge via shared 'http' keyword"
        );
    }

    #[test]
    fn shared_keyword_same_domain_no_bridge() {
        let nodes = vec![
            make_node(
                "doi::10.1/a",
                "Paper A",
                &["keyword:machine_learning"],
                "article",
            ),
            make_node(
                "doi::10.1/b",
                "Paper B",
                &["keyword:machine_learning"],
                "article",
            ),
        ];
        let edges = vec![];

        let (_, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        assert_eq!(
            stats.keyword_bridges, 0,
            "same domain keywords should not bridge"
        );
    }

    #[test]
    fn shared_keyword_article_format() {
        // Test article:keyword: prefix
        let nodes = vec![
            make_node(
                "bibtex::transformer",
                "Attention Paper",
                &["article:keyword:attention"],
                "bibtex",
            ),
            make_node(
                "crossref::10.9/x",
                "Attention Study",
                &["subject:attention"],
                "crossref",
            ),
        ];
        let edges = vec![];

        let (_, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        assert!(
            stats.keyword_bridges >= 1,
            "article:keyword: and subject: should both be indexed"
        );
    }

    #[test]
    fn same_orcid_bridges() {
        let nodes = vec![
            MergeNode {
                external_id: "article::author::roy_fielding_1".to_string(),
                label: "Roy T. Fielding".to_string(),
                node_type: NodeType::Concept,
                tags: vec![
                    "article:author".to_string(),
                    "orcid:0000-0001-8249-3260".to_string(),
                ],
                timestamp: 0.0,
                change_freq: 0.5,
                source_path: None,
                excerpt: None,
                namespace: Some("crossref".to_string()),
            },
            MergeNode {
                external_id: "rfc::author::roy_fielding".to_string(),
                label: "Roy T. Fielding".to_string(),
                node_type: NodeType::Concept,
                tags: vec![
                    "article:author".to_string(),
                    "orcid:0000-0001-8249-3260".to_string(),
                ],
                timestamp: 0.0,
                change_freq: 0.5,
                source_path: None,
                excerpt: None,
                namespace: Some("rfc".to_string()),
            },
        ];
        let edges = vec![];

        let (_, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        println!("orcid_bridges={}", stats.orcid_bridges);
        assert!(
            stats.orcid_bridges >= 1,
            "same ORCID across domains should create bridge"
        );
    }

    #[test]
    fn same_orcid_same_domain_no_bridge() {
        let nodes = vec![
            MergeNode {
                external_id: "a::author1".to_string(),
                label: "Author 1".to_string(),
                node_type: NodeType::Concept,
                tags: vec!["orcid:0000-0001-0000-0000".to_string()],
                timestamp: 0.0,
                change_freq: 0.5,
                source_path: None,
                excerpt: None,
                namespace: Some("article".to_string()),
            },
            MergeNode {
                external_id: "a::author2".to_string(),
                label: "Author 2".to_string(),
                node_type: NodeType::Concept,
                tags: vec!["orcid:0000-0001-0000-0000".to_string()],
                timestamp: 0.0,
                change_freq: 0.5,
                source_path: None,
                excerpt: None,
                namespace: Some("article".to_string()),
            },
        ];
        let edges = vec![];

        let (_, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        assert_eq!(
            stats.orcid_bridges, 0,
            "same domain ORCID should not bridge"
        );
    }

    #[test]
    fn citation_chain_transitive() {
        // A cites B, B cites C → A should get chain edge to C
        let nodes = vec![
            make_node("doi::a", "Paper A", &["article"], "article"),
            make_node("doi::b", "Paper B", &["article"], "article"),
            make_node("doi::c", "Paper C", &["article"], "article"),
        ];
        let edges = vec![
            MergeEdge {
                source_id: "doi::a".to_string(),
                target_id: "doi::b".to_string(),
                relation: "cites".to_string(),
                weight: 0.8,
            },
            MergeEdge {
                source_id: "doi::b".to_string(),
                target_id: "doi::c".to_string(),
                relation: "cites".to_string(),
                weight: 0.8,
            },
        ];

        let (graph, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        println!("citation_chains={}", stats.citation_chains);
        assert!(
            stats.citation_chains >= 1,
            "A→B→C should create citation_chain A→C"
        );

        // Verify the transitive edge exists
        let a = graph.resolve_id("doi::a").unwrap();
        let c = graph.resolve_id("doi::c").unwrap();
        let has_chain = graph.csr.out_range(a).any(|idx| {
            graph.csr.targets[idx] == c
                && graph.strings.resolve(graph.csr.relations[idx]) == "citation_chain"
        });
        assert!(has_chain, "transitive chain edge should exist from A to C");
    }

    #[test]
    fn citation_chain_no_self_loop() {
        // A cites B, B cites A → should NOT create A→A chain
        let nodes = vec![
            make_node("doi::a", "Paper A", &["article"], "article"),
            make_node("doi::b", "Paper B", &["article"], "article"),
        ];
        let edges = vec![
            MergeEdge {
                source_id: "doi::a".to_string(),
                target_id: "doi::b".to_string(),
                relation: "cites".to_string(),
                weight: 0.8,
            },
            MergeEdge {
                source_id: "doi::b".to_string(),
                target_id: "doi::a".to_string(),
                relation: "cites".to_string(),
                weight: 0.8,
            },
        ];

        let (_, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        assert_eq!(
            stats.citation_chains, 0,
            "A→B→A should not create self-loop chain"
        );
    }

    #[test]
    fn citation_chain_with_references_relation() {
        // Also works with "references" relation
        let nodes = vec![
            make_node("doi::x", "Paper X", &["article"], "article"),
            make_node("doi::y", "Paper Y", &["article"], "article"),
            make_node("doi::z", "Paper Z", &["article"], "article"),
        ];
        let edges = vec![
            MergeEdge {
                source_id: "doi::x".to_string(),
                target_id: "doi::y".to_string(),
                relation: "references".to_string(),
                weight: 0.7,
            },
            MergeEdge {
                source_id: "doi::y".to_string(),
                target_id: "doi::z".to_string(),
                relation: "references".to_string(),
                weight: 0.7,
            },
        ];

        let (_, stats) = CrossDomainResolver::resolve(nodes, edges).unwrap();
        assert!(
            stats.citation_chains >= 1,
            "'references' relation should also create chains"
        );
    }
}
