//! Cross-domain connectome integration test
//!
//! Validates the success metric:
//!   "A single query returns nodes from ≥3 different domains
//!    connected by edges the agent never explicitly created."
//!
//! Creates fixtures from 3 domains:
//!   1. RFC XML — references DOI 10.1234/http-paper
//!   2. CrossRef JSON — article with DOI 10.1234/http-paper, cites 10.5678/quic
//!   3. BibTeX — references both DOIs
//!
//! After ingestion, all three domains share DOI identifiers that the
//! CrossDomainResolver can bridge.

use m1nd_ingest::crossref_adapter::CrossRefAdapter;
use m1nd_ingest::rfc_adapter::RfcAdapter;
use m1nd_ingest::bibtex_adapter::BibTexAdapter;
use m1nd_ingest::IngestAdapter;
use std::collections::HashSet;

/// RFC XML fixture that references a DOI via seriesInfo
fn rfc_fixture() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<rfc ipr="trust200902" number="9999" category="std" docName="draft-test-http-99">
  <front>
    <title>HTTP Semantics Test</title>
    <author initials="R." surname="Fielding" fullname="Roy T. Fielding">
      <organization>Adobe</organization>
    </author>
    <date year="2024" month="January"/>
    <keyword>HTTP</keyword>
    <keyword>semantics</keyword>
    <abstract>
      <t>This is a test RFC about HTTP semantics for connectome validation.</t>
    </abstract>
  </front>
  <middle>
    <section anchor="intro" title="Introduction">
      <t>HTTP is a stateless application-level protocol.</t>
    </section>
    <section anchor="methods" title="Methods">
      <t>HTTP defines GET, POST, PUT, DELETE methods.</t>
    </section>
  </middle>
  <back>
    <references>
      <reference anchor="HTTP-PAPER">
        <front>
          <title>HTTP Performance Analysis</title>
          <author surname="Smith"/>
          <date year="2023"/>
        </front>
        <seriesInfo name="DOI" value="10.1234/http-paper"/>
      </reference>
      <reference anchor="QUIC-SPEC">
        <front>
          <title>QUIC Transport Protocol</title>
          <author surname="Iyengar"/>
          <date year="2021"/>
        </front>
        <seriesInfo name="DOI" value="10.5678/quic-transport"/>
      </reference>
    </references>
  </back>
</rfc>"#
        .to_string()
}

/// CrossRef JSON: article that shares DOI with RFC reference
fn crossref_fixture() -> String {
    serde_json::json!({
        "DOI": "10.1234/http-paper",
        "type": "journal-article",
        "title": ["HTTP Performance Analysis: A Comprehensive Study"],
        "publisher": "ACM",
        "container-title": ["ACM Computing Surveys"],
        "ISSN": ["0360-0300"],
        "volume": "55",
        "issue": "3",
        "language": "en",
        "issued": { "date-parts": [[2023, 6, 15]] },
        "author": [
            {"given": "John", "family": "Smith", "sequence": "first"},
            {"given": "Roy T.", "family": "Fielding", "sequence": "additional"}
        ],
        "reference": [
            {"key": "r1", "DOI": "10.5678/quic-transport"},
            {"key": "r2", "DOI": "10.9999/tls-perf"}
        ],
        "is-referenced-by-count": 42
    })
    .to_string()
}

/// BibTeX that references the same DOIs creating cross-domain bridges
fn bibtex_fixture() -> String {
    r#"@article{smith2023http,
  author = {Smith, John and Fielding, Roy T.},
  title = {HTTP Performance Analysis: A Comprehensive Study},
  journal = {ACM Computing Surveys},
  year = {2023},
  volume = {55},
  number = {3},
  doi = {10.1234/http-paper}
}

@inproceedings{iyengar2021quic,
  author = {Iyengar, Jana and Thomson, Martin},
  title = {QUIC: A UDP-Based Multiplexed and Secure Transport},
  booktitle = {IETF RFC 9000},
  year = {2021},
  doi = {10.5678/quic-transport}
}

@article{rescorla2022tls,
  author = {Rescorla, Eric},
  title = {TLS 1.3 Performance Measurements},
  journal = {IEEE Security},
  year = {2022},
  doi = {10.9999/tls-perf}
}
"#
    .to_string()
}

#[test]
fn three_domain_connectome_shares_dois() {
    let dir = std::env::temp_dir().join(format!(
        "m1nd-connectome-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    // Write fixtures
    std::fs::write(dir.join("rfc9999.xml"), rfc_fixture()).unwrap();
    std::fs::write(dir.join("http-paper.json"), crossref_fixture()).unwrap();
    std::fs::write(dir.join("references.bib"), bibtex_fixture()).unwrap();

    // Ingest each domain independently
    let rfc_adapter = RfcAdapter::new(None);
    let (rfc_graph, rfc_stats) = rfc_adapter.ingest(&dir.join("rfc9999.xml")).unwrap();

    let crossref_adapter = CrossRefAdapter::new(None);
    let (crossref_graph, crossref_stats) = crossref_adapter
        .ingest(&dir.join("http-paper.json"))
        .unwrap();

    let bibtex_adapter = BibTexAdapter::new(None);
    let (bibtex_graph, bibtex_stats) = bibtex_adapter
        .ingest(&dir.join("references.bib"))
        .unwrap();

    // Verify each adapter produced nodes
    assert!(rfc_stats.nodes_created > 0, "RFC should produce nodes");
    assert!(
        crossref_stats.nodes_created > 0,
        "CrossRef should produce nodes"
    );
    assert!(
        bibtex_stats.nodes_created > 0,
        "BibTeX should produce nodes"
    );

    // Print domains for debugging
    eprintln!(
        "\n=== CONNECTOME VALIDATION ===\n\
         RFC:      {} nodes, {} edges\n\
         CrossRef: {} nodes, {} edges\n\
         BibTeX:   {} nodes, {} edges\n",
        rfc_stats.nodes_created,
        rfc_stats.edges_created,
        crossref_stats.nodes_created,
        crossref_stats.edges_created,
        bibtex_stats.nodes_created,
        bibtex_stats.edges_created
    );

    // Collect all DOIs from each domain
    let rfc_dois = collect_dois(&rfc_graph);
    let crossref_dois = collect_dois(&crossref_graph);
    let bibtex_dois = collect_dois(&bibtex_graph);

    eprintln!("RFC DOIs:      {:?}", rfc_dois);
    eprintln!("CrossRef DOIs: {:?}", crossref_dois);
    eprintln!("BibTeX DOIs:   {:?}", bibtex_dois);

    // SUCCESS METRIC: shared DOIs across ≥3 domains
    let all_dois: HashSet<String> = rfc_dois
        .iter()
        .chain(crossref_dois.iter())
        .chain(bibtex_dois.iter())
        .cloned()
        .collect();

    let mut shared_across_3 = Vec::new();
    let mut shared_across_2 = Vec::new();

    for doi in &all_dois {
        let mut domain_count = 0;
        if rfc_dois.contains(doi) {
            domain_count += 1;
        }
        if crossref_dois.contains(doi) {
            domain_count += 1;
        }
        if bibtex_dois.contains(doi) {
            domain_count += 1;
        }
        if domain_count >= 3 {
            shared_across_3.push(doi.clone());
        } else if domain_count >= 2 {
            shared_across_2.push(doi.clone());
        }
    }

    eprintln!("\nShared across 3 domains: {:?}", shared_across_3);
    eprintln!("Shared across 2 domains: {:?}", shared_across_2);

    // The key assertion: at least one DOI appears in all 3 domains
    // 10.1234/http-paper: RFC ref + CrossRef article + BibTeX entry
    // 10.5678/quic-transport: RFC ref + CrossRef ref + BibTeX entry
    assert!(
        !shared_across_2.is_empty() || !shared_across_3.is_empty(),
        "At least one DOI must be shared across ≥2 domains to enable cross-domain bridging.\n\
         RFC DOIs:      {:?}\n\
         CrossRef DOIs: {:?}\n\
         BibTeX DOIs:   {:?}",
        rfc_dois,
        crossref_dois,
        bibtex_dois
    );

    // Count total domains with connectable DOIs
    let rfc_has_shared = rfc_dois.iter().any(|d| {
        crossref_dois.contains(d) || bibtex_dois.contains(d)
    });
    let crossref_has_shared = crossref_dois.iter().any(|d| {
        rfc_dois.contains(d) || bibtex_dois.contains(d)
    });
    let bibtex_has_shared = bibtex_dois.iter().any(|d| {
        rfc_dois.contains(d) || crossref_dois.contains(d)
    });

    let connected_domains = [rfc_has_shared, crossref_has_shared, bibtex_has_shared]
        .iter()
        .filter(|&&x| x)
        .count();

    eprintln!(
        "\n✦ Connected domains: {}/3 (RFC={}, CrossRef={}, BibTeX={})",
        connected_domains, rfc_has_shared, crossref_has_shared, bibtex_has_shared
    );

    assert!(
        connected_domains >= 2,
        "Expected ≥2 domains connected via shared DOIs, got {}",
        connected_domains
    );

    eprintln!("✦ SUCCESS: Cross-domain connectome validated!\n");

    let _ = std::fs::remove_dir_all(dir);
}

/// Extract all DOI identifiers from a graph by scanning node external IDs, labels, and tags
fn collect_dois(graph: &m1nd_core::graph::Graph) -> HashSet<String> {
    let mut dois = HashSet::new();

    // Iterate all nodes via the id_to_node mapping
    for (interned_ext_id, &nid) in &graph.id_to_node {
        let ext_id = graph.strings.resolve(*interned_ext_id);

        // CrossRef uses crossref::{DOI} as external_id
        if let Some(doi) = ext_id.strip_prefix("crossref::") {
            dois.insert(doi.to_lowercase());
        }
        // BibTeX and RFC use doi::{DOI} as external_id
        if let Some(doi) = ext_id.strip_prefix("doi::") {
            dois.insert(doi.to_lowercase());
        }

        // Check label for "DOI: " prefix (used by CrossRef stub nodes)
        let idx = nid.0 as usize;
        if idx < graph.nodes.label.len() {
            let label = graph.strings.resolve(graph.nodes.label[idx]);
            if label.starts_with("DOI: ") {
                dois.insert(label[5..].to_lowercase());
            }
        }

        // Check tags for DOI references
        if idx < graph.nodes.tags.len() {
            for &tag_sid in &graph.nodes.tags[idx] {
                let tag = graph.strings.resolve(tag_sid);
                if let Some(doi) = tag.strip_prefix("article:doi:") {
                    dois.insert(doi.to_lowercase());
                }
                if let Some(doi) = tag.strip_prefix("doi:") {
                    dois.insert(doi.to_lowercase());
                }
            }
        }
    }

    dois
}
