// Integration tests for the JATS Article Adapter
// Tests with both synthetic and real PubMed/PMC data

use m1nd_ingest::jats_adapter::JatsArticleAdapter;
use m1nd_ingest::IngestAdapter;
use std::path::Path;

// ─── Real PubMed Data ──────────────────────────────────────────────

#[test]
fn ingest_real_pubmed_article() {
    let path = Path::new("/tmp/patent-test/pubmed_abstract.xml");
    if !path.exists() {
        println!("SKIP: real PubMed data not available");
        return;
    }

    let adapter = JatsArticleAdapter::new(None);
    let (graph, stats) = adapter.ingest(path).expect("ingest failed");

    println!("=== Real PubMed: PMID 33611339 ===");
    println!("  nodes: {}", stats.nodes_created);
    println!("  edges: {}", stats.edges_created);
    println!("  elapsed: {:.2}ms", stats.elapsed_ms);

    // Should have the main article
    assert!(
        graph.resolve_id("pmid::33611339").is_some(),
        "PMID 33611339 not found"
    );

    // Check provenance
    let nid = graph.resolve_id("pmid::33611339").unwrap();
    let prov = graph.resolve_node_provenance(nid);
    assert!(prov.excerpt.is_some(), "should have abstract as excerpt");
    assert_eq!(prov.namespace.as_deref(), Some("article"));
    println!("  excerpt: {}...", &prov.excerpt.unwrap()[..60]);

    // Should have many citation nodes
    assert!(
        stats.nodes_created >= 50,
        "expected >= 50 nodes (article + authors + journal + many citations), got {}",
        stats.nodes_created
    );

    // Check some specific citations by DOI
    assert!(
        graph.resolve_id("doi::10.1038/nrg.2016.93").is_some(),
        "citation doi not found"
    );

    println!("  PASS ✓");
}

#[test]
fn ingest_real_pmc_jats_article() {
    let path = Path::new("/tmp/patent-test/jats_full_article.xml");
    if !path.exists() {
        println!("SKIP: real PMC JATS data not available");
        return;
    }

    let adapter = JatsArticleAdapter::new(None);
    let (graph, stats) = adapter.ingest(path).expect("ingest failed");

    println!("=== Real PMC JATS Article ===");
    println!("  nodes: {}", stats.nodes_created);
    println!("  edges: {}", stats.edges_created);
    println!("  elapsed: {:.2}ms", stats.elapsed_ms);

    assert!(stats.nodes_created >= 5, "expected >= 5 nodes");
    println!("  PASS ✓");
}

// ─── Edge Cases ────────────────────────────────────────────────────

#[test]
fn jats_empty_xml() {
    let adapter = JatsArticleAdapter::new(None);
    let dir = std::env::temp_dir().join("jats-empty");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("empty.xml"), "").unwrap();

    let (_, stats) = adapter.ingest(&dir).expect("should not crash");
    assert_eq!(stats.nodes_created, 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn jats_non_article_xml() {
    let adapter = JatsArticleAdapter::new(None);
    let dir = std::env::temp_dir().join("jats-other");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.xml"), "<config><debug/></config>").unwrap();

    let (_, stats) = adapter.ingest(&dir).expect("should not crash");
    assert_eq!(stats.nodes_created, 0);
    std::fs::remove_dir_all(&dir).ok();
}

// ─── Performance ───────────────────────────────────────────────────

#[test]
fn jats_performance_100x() {
    let path = Path::new("/tmp/patent-test/pubmed_abstract.xml");
    if !path.exists() {
        println!("SKIP");
        return;
    }

    let adapter = JatsArticleAdapter::new(None);
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = adapter.ingest(path).expect("ingest failed");
    }
    let elapsed = start.elapsed();
    let per_run = elapsed.as_millis() as f64 / 100.0;
    println!(
        "  100x PubMed ingest: {:.0}ms total, {:.2}ms/run",
        elapsed.as_millis(),
        per_run
    );
    // 109KB file with 257 refs should stay under 100ms per run
    assert!(per_run < 100.0, "too slow: {:.2}ms/run", per_run);
}
