use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

fn build_state(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        runtime_dir: Some(root.to_path_buf()),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

fn call(state: &mut SessionState, tool: &str, params: serde_json::Value) -> serde_json::Value {
    dispatch_tool(state, tool, &params).expect("tool call")
}

fn search_count(state: &mut SessionState, query: &str) -> usize {
    call(
        state,
        "search",
        json!({"agent_id":"tester","query":query,"mode":"literal"}),
    )
    .get("results")
    .and_then(|value| value.as_array())
    .map(|value| value.len())
    .unwrap_or(0)
}

fn search_first_node_id(state: &mut SessionState, query: &str) -> String {
    call(
        state,
        "search",
        json!({"agent_id":"tester","query":query,"mode":"literal"}),
    )
    .get("results")
    .and_then(|value| value.as_array())
    .and_then(|value| value.first())
    .and_then(|value| value.get("node_id"))
    .and_then(|value| value.as_str())
    .unwrap_or("")
    .to_string()
}

fn search_file_paths(state: &mut SessionState, query: &str) -> Vec<String> {
    call(
        state,
        "search",
        json!({"agent_id":"tester","query":query,"mode":"literal"}),
    )
    .get("results")
    .and_then(|value| value.as_array())
    .map(|results| {
        results
            .iter()
            .filter_map(|entry| entry.get("file_path").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    })
    .unwrap_or_default()
}

fn wait_for_queue(state: &mut SessionState, expected_min: usize) {
    let mut last_queue_depth = 0;
    for _ in 0..50 {
        let status = call(state, "auto_ingest_status", json!({ "agent_id": "tester" }));
        let queue_depth = status
            .get("queue_depth")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        last_queue_depth = queue_depth;
        if queue_depth >= expected_min {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "timed out waiting for auto-ingest queue depth to reach at least {}; last observed queue depth was {} after 50 retries with 20ms sleep",
        expected_min,
        last_queue_depth
    );
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn light_doc(entity: &str, doi: &str) -> String {
    format!(
        r#"---
Protocol: L1GHT/1
Node: {entity}
State: active
Color: amber
Glyph: *
Completeness: draft
Proof: working
Depends on:
- {doi}
Next:
- validate
---
## Contract
[⍂ entity: {entity}]
[⟁ depends_on: {doi}]
[𝔻 evidence: ready]
"#
    )
}

fn pubmed_article(title: &str, doi: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<PubmedArticleSet>
<PubmedArticle>
  <MedlineCitation>
    <PMID>12345678</PMID>
    <Article>
      <ArticleTitle>{title}</ArticleTitle>
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
          <ArticleId IdType="doi">{doi}</ArticleId>
        </ArticleIdList>
      </Reference>
    </ReferenceList>
  </PubmedData>
</PubmedArticle>
</PubmedArticleSet>"#
    )
}

fn bibtex_entry(title: &str, doi: &str) -> String {
    format!(
        r#"@article{{shared2026,
  author = {{Doe, Jane}},
  title = {{{title}}},
  journal = {{Nature}},
  year = {{2026}},
  doi = {{{doi}}}
}}
"#
    )
}

fn crossref_work(title: &str, doi: &str) -> String {
    json!({
        "DOI": doi,
        "title": [title],
        "type": "journal-article",
        "publisher": "Nature",
        "author": [{"given": "Jane", "family": "Doe", "sequence": "first"}],
        "reference": []
    })
    .to_string()
}

fn plain_markdown(title: &str, body: &str) -> String {
    format!("# {}\n\n{}\n", title, body)
}

fn html_doc(title: &str, body: &str) -> String {
    format!(
        "<html><body><h1>{}</h1><p>{}</p><p><a href=\"https://example.com/docs\">Docs</a></p></body></html>",
        title, body
    )
}

#[test]
fn auto_ingest_light_file_lifecycle_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("docs");
    fs::create_dir_all(&docs_root).unwrap();
    let file = docs_root.join("notes.md");

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["light"],"debounce_ms":0}),
    );

    write(&file, &light_doc("AlphaNode", "10.1000/shared"));
    wait_for_queue(&mut state, 1);
    call(&mut state, "auto_ingest_tick", json!({"agent_id":"tester"}));

    assert!(
        search_count(&mut state, "AlphaNode") > 0,
        "light entity must be searchable after create"
    );

    write(&file, &light_doc("OmegaNode", "10.1000/shared"));
    wait_for_queue(&mut state, 1);
    call(&mut state, "auto_ingest_tick", json!({"agent_id":"tester"}));

    assert_eq!(
        search_count(&mut state, "AlphaNode"),
        0,
        "old entity should disappear after update"
    );

    assert!(
        search_count(&mut state, "OmegaNode") > 0,
        "new entity must be searchable after update"
    );

    fs::remove_file(&file).unwrap();
    wait_for_queue(&mut state, 1);
    call(&mut state, "auto_ingest_tick", json!({"agent_id":"tester"}));

    assert_eq!(
        search_count(&mut state, "OmegaNode"),
        0,
        "entity should disappear after delete"
    );
}

#[test]
fn auto_ingest_supports_article_bibtex_and_crossref() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("research");
    fs::create_dir_all(&docs_root).unwrap();
    let article = docs_root.join("paper.xml");
    let bib = docs_root.join("refs.bib");
    let crossref = docs_root.join("work.json");

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["article","bibtex","crossref"],"debounce_ms":0}),
    );

    write(&article, &pubmed_article("Test Article", "10.1000/shared"));
    write(&bib, &bibtex_entry("Shared Bibliography", "10.1000/shared"));
    write(&crossref, &crossref_work("Shared Work", "10.1000/shared"));
    wait_for_queue(&mut state, 3);
    call(&mut state, "auto_ingest_tick", json!({"agent_id":"tester"}));

    for needle in ["12345678", "Shared Bibliography", "Shared Work"] {
        assert!(
            search_count(&mut state, needle) > 0,
            "expected searchable result for {}",
            needle
        );
    }
}

#[test]
fn auto_ingest_mixed_domain_query_returns_multiple_formats() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("mixed");
    fs::create_dir_all(&docs_root).unwrap();

    write(
        &docs_root.join("spec.md"),
        &light_doc("BridgeStudy", "10.1000/shared"),
    );
    write(
        &docs_root.join("paper.xml"),
        &pubmed_article("Bridge Article", "10.1000/shared"),
    );
    write(
        &docs_root.join("refs.bib"),
        &bibtex_entry("Bridge Bibliography", "10.1000/shared"),
    );
    write(
        &docs_root.join("work.json"),
        &crossref_work("Bridge CrossRef", "10.1000/shared"),
    );

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"debounce_ms":0}),
    );

    let file_paths = search_file_paths(&mut state, "10.1000/shared");
    assert!(file_paths.iter().any(|path| path.ends_with(".md")));
    assert!(file_paths.iter().any(|path| path.ends_with(".xml")));
    assert!(
        file_paths.len() >= 3,
        "expected mixed-domain retrieval, got {:?}",
        file_paths
    );
}

#[test]
fn auto_ingest_lock_watch_observes_ingest_mutations() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("locks");
    fs::create_dir_all(&docs_root).unwrap();
    let file = docs_root.join("watch.md");
    write(&file, &light_doc("WatchedNode", "10.1000/watch"));

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["light"],"debounce_ms":0}),
    );

    let node_id = search_first_node_id(&mut state, "WatchedNode");
    assert!(!node_id.is_empty(), "watched node result");

    let created = call(
        &mut state,
        "lock_create",
        json!({"agent_id":"tester","scope":"node","root_nodes":[node_id]}),
    );
    let lock_id = created
        .get("lock_id")
        .and_then(|value| value.as_str())
        .expect("lock id")
        .to_string();

    call(
        &mut state,
        "lock_watch",
        json!({"agent_id":"tester","lock_id":lock_id,"strategy":"on_ingest"}),
    );

    write(&file, &light_doc("WatchedNodeV2", "10.1000/watch"));
    wait_for_queue(&mut state, 1);
    call(&mut state, "auto_ingest_tick", json!({"agent_id":"tester"}));

    let diff = call(
        &mut state,
        "lock_diff",
        json!({"agent_id":"tester","lock_id":lock_id}),
    );
    assert!(
        diff.get("watcher_events_drained")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            >= 1
    );
}

#[test]
fn auto_ingest_restart_skips_unchanged_files() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("restart");
    fs::create_dir_all(&docs_root).unwrap();
    let file = docs_root.join("restart.md");
    write(&file, &light_doc("RestartStudy", "10.1000/restart"));

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["light"],"debounce_ms":0}),
    );
    call(&mut state, "auto_ingest_stop", json!({"agent_id":"tester"}));

    let mut restarted = build_state(temp.path());
    let start = call(
        &mut restarted,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["light"],"debounce_ms":0}),
    );
    let skipped = start
        .get("bootstrap")
        .and_then(|value| value.get("skipped_paths"))
        .and_then(|value| value.as_array())
        .map(|value| value.len())
        .unwrap_or(0);
    assert!(skipped >= 1, "restart should skip unchanged file");

    write(&file, &light_doc("RestartStudyV2", "10.1000/restart"));
    wait_for_queue(&mut restarted, 1);
    let tick = call(
        &mut restarted,
        "auto_ingest_tick",
        json!({"agent_id":"tester"}),
    );
    let ingested = tick
        .get("ingested_paths")
        .and_then(|value| value.as_array())
        .map(|value| value.len())
        .unwrap_or(0);
    assert_eq!(ingested, 1, "only the changed file should be reingested");
}

#[test]
fn auto_ingest_hot_file_storm_converges_to_last_write() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("storm");
    fs::create_dir_all(&docs_root).unwrap();
    let file = docs_root.join("storm.md");

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["light"],"debounce_ms":0}),
    );

    for i in 0..50 {
        write(&file, &light_doc(&format!("Storm{}", i), "10.1000/storm"));
    }
    wait_for_queue(&mut state, 1);
    call(&mut state, "auto_ingest_tick", json!({"agent_id":"tester"}));

    assert!(
        search_count(&mut state, "Storm49") > 0,
        "final write should win"
    );
}

#[test]
fn auto_ingest_burst_stress_handles_many_documents() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("burst");
    fs::create_dir_all(&docs_root).unwrap();

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["light"],"debounce_ms":0}),
    );

    for round in 0..5 {
        for index in 0..200 {
            write(
                &docs_root.join(format!("doc-{}.md", index)),
                &light_doc(&format!("Burst{}-{}", round, index), "10.1000/burst"),
            );
        }
        thread::sleep(Duration::from_millis(40));
    }

    wait_for_queue(&mut state, 1);
    call(&mut state, "auto_ingest_tick", json!({"agent_id":"tester"}));

    assert!(
        search_count(&mut state, "Burst4-199") > 0,
        "burst stress should leave final graph queryable"
    );
}

#[test]
fn universal_ingest_writes_canonical_artifacts_and_resolves_document() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("universal");
    fs::create_dir_all(&docs_root).unwrap();
    let file = docs_root.join("notes.md");
    write(
        &file,
        &plain_markdown(
            "Universal Notes",
            "This document mentions TokenValidator and 10.1000/test.",
        ),
    );

    let mut state = build_state(temp.path());
    let ingest = call(
        &mut state,
        "ingest",
        json!({"agent_id":"tester","path":file.to_string_lossy().to_string(),"adapter":"universal","mode":"merge"}),
    );
    assert_eq!(
        ingest.get("adapter").and_then(|v| v.as_str()),
        Some("universal")
    );

    let resolved = call(
        &mut state,
        "document_resolve",
        json!({"agent_id":"tester","path":"notes.md"}),
    );
    let canonical_md = resolved
        .get("canonical_markdown_path")
        .and_then(|value| value.as_str())
        .unwrap();
    let canonical_json = resolved
        .get("canonical_json_path")
        .and_then(|value| value.as_str())
        .unwrap();
    let claims_json = resolved
        .get("claims_path")
        .and_then(|value| value.as_str())
        .unwrap();

    assert!(Path::new(canonical_md).exists());
    assert!(Path::new(canonical_json).exists());
    assert!(Path::new(claims_json).exists());
    assert!(search_count(&mut state, "TokenValidator") > 0);
}

#[test]
fn auto_ingest_universal_handles_markdown_and_html() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("universal-watch");
    fs::create_dir_all(&docs_root).unwrap();
    let md = docs_root.join("notes.md");
    let html = docs_root.join("page.html");

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["universal"],"debounce_ms":0}),
    );

    write(
        &md,
        &plain_markdown("Universal Watch", "TokenValidator appears here."),
    );
    write(&html, &html_doc("HTML Watch", "DesignSystem appears here."));
    wait_for_queue(&mut state, 2);
    let tick = call(&mut state, "auto_ingest_tick", json!({"agent_id":"tester"}));
    let ingested = tick
        .get("ingested_paths")
        .and_then(|value| value.as_array())
        .map(|value| value.len())
        .unwrap_or(0);
    assert_eq!(ingested, 2);

    assert!(search_count(&mut state, "Universal Watch") > 0);
    assert!(search_count(&mut state, "HTML Watch") > 0);

    let md_resolved = call(
        &mut state,
        "document_resolve",
        json!({"agent_id":"tester","path":"notes.md"}),
    );
    let html_resolved = call(
        &mut state,
        "document_resolve",
        json!({"agent_id":"tester","path":"page.html"}),
    );
    assert!(Path::new(md_resolved["canonical_markdown_path"].as_str().unwrap()).exists());
    assert!(Path::new(html_resolved["canonical_markdown_path"].as_str().unwrap()).exists());
}

#[test]
fn universal_document_bindings_and_drift_surface_work() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("semantic");
    fs::create_dir_all(&docs_root).unwrap();
    let file = docs_root.join("spec.md");
    write(
        &file,
        "# API\n\n`TokenValidator` must validate requests.\n\nSee `src/token_validator.rs`.\n",
    );

    let mut state = build_state(temp.path());
    {
        let mut graph = state.graph.write();
        graph
            .add_node(
                "file::src/token_validator.rs",
                "TokenValidator",
                m1nd_core::types::NodeType::File,
                &["code"],
                1.0,
                0.1,
            )
            .unwrap();
        graph.finalize().unwrap();
    }
    state.rebuild_engines().unwrap();

    call(
        &mut state,
        "ingest",
        json!({"agent_id":"tester","path":file.to_string_lossy().to_string(),"adapter":"universal","mode":"merge"}),
    );

    let bindings = call(
        &mut state,
        "document_bindings",
        json!({"agent_id":"tester","path":"spec.md","top_k":5}),
    );
    let bindings_len = bindings["bindings"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    assert!(bindings_len > 0);

    let drift = call(
        &mut state,
        "document_drift",
        json!({"agent_id":"tester","path":"spec.md"}),
    );
    assert!(drift.get("summary").is_some());
}

#[test]
fn auto_ingest_status_exposes_semantic_counts() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("status");
    fs::create_dir_all(&docs_root).unwrap();
    write(
        &docs_root.join("notes.md"),
        "# Overview\n\n`TokenValidator` must validate requests.\n10.1000/test\n",
    );

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["universal"],"debounce_ms":0}),
    );
    let status = call(
        &mut state,
        "auto_ingest_status",
        json!({"agent_id":"tester"}),
    );
    assert!(status["semantic_document_count"].as_u64().unwrap_or(0) >= 1);
    assert!(status["semantic_section_count"].as_u64().unwrap_or(0) >= 1);
    assert!(status["semantic_entity_count"].as_u64().unwrap_or(0) >= 1);
    assert!(status["semantic_claim_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(status["drift_document_count"].as_u64().unwrap_or(0), 1);
    assert_eq!(
        status["provider_route_counts"]["universal:internal"]
            .as_u64()
            .unwrap_or(0),
        1
    );
    assert_eq!(
        status["provider_fallback_counts"]["universal:internal"]
            .as_u64()
            .unwrap_or(0),
        1
    );
}

#[test]
fn auto_ingest_status_reflects_drift_after_explicit_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("status-drift");
    fs::create_dir_all(&docs_root).unwrap();
    let file = docs_root.join("spec.md");
    write(
        &file,
        "# API\n\n`TokenValidator` must validate requests.\n\nSee `src/token_validator.rs`.\n",
    );

    let mut state = build_state(temp.path());
    {
        let mut graph = state.graph.write();
        graph
            .add_node(
                "file::src/token_validator.rs",
                "TokenValidator",
                m1nd_core::types::NodeType::File,
                &["code"],
                1.0,
                0.1,
            )
            .unwrap();
        graph.finalize().unwrap();
    }
    state.bump_graph_generation();

    call(
        &mut state,
        "ingest",
        json!({"agent_id":"tester","path":file.to_string_lossy().to_string(),"adapter":"universal","mode":"merge"}),
    );

    let initial = call(
        &mut state,
        "auto_ingest_status",
        json!({"agent_id":"tester"}),
    );
    assert_eq!(initial["drift_document_count"].as_u64().unwrap_or(0), 0);

    {
        let mut graph = state.graph.write();
        let node = graph.resolve_id("file::src/token_validator.rs").unwrap();
        graph.nodes.last_modified[node.as_usize()] = 9999999999.0;
    }
    state.bump_graph_generation();

    let drift = call(
        &mut state,
        "document_drift",
        json!({"agent_id":"tester","path":"spec.md"}),
    );
    assert!(
        drift["summary"]["code_change_unreflected"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );

    let refreshed = call(
        &mut state,
        "auto_ingest_status",
        json!({"agent_id":"tester"}),
    );
    assert_eq!(refreshed["drift_document_count"].as_u64().unwrap_or(0), 1);
    assert!(
        refreshed["semantic_document_count"].as_u64().unwrap_or(0) >= 1,
        "semantic counts should remain populated after drift refresh"
    );
}

#[test]
fn provider_gated_docling_docx_flow_skips_without_provider_python() {
    let Some(provider_python) = std::env::var_os("M1ND_PROVIDER_PYTHON") else {
        eprintln!("SKIP: M1ND_PROVIDER_PYTHON not configured");
        return;
    };
    let provider_python = PathBuf::from(provider_python);
    if !provider_python.exists() {
        eprintln!("SKIP: configured provider python missing");
        return;
    }
    let Ok(docling_probe) = std::process::Command::new(&provider_python)
        .arg("-c")
        .arg("import docling")
        .output()
    else {
        eprintln!("SKIP: failed to spawn configured provider python");
        return;
    };
    if !docling_probe.status.success() {
        eprintln!("SKIP: docling not available in provider env");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("provider");
    fs::create_dir_all(&docs_root).unwrap();
    let docx = docs_root.join("provider.docx");

    let Ok(output) = std::process::Command::new(&provider_python)
        .arg("-c")
        .arg(format!(
            "from docx import Document; d=Document(); d.add_heading('Provider Docx', level=1); d.add_paragraph('DoclingKnowledge appears here.'); d.save(r'{}')",
            docx.display()
        ))
        .output()
    else {
        eprintln!("SKIP: failed to spawn configured provider python");
        return;
    };
    if !output.status.success() {
        eprintln!("SKIP: python-docx not available in provider env");
        return;
    }

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "auto_ingest_start",
        json!({"agent_id":"tester","roots":[docs_root.to_string_lossy().to_string()],"formats":["universal"],"debounce_ms":0}),
    );
    let resolved = call(
        &mut state,
        "document_resolve",
        json!({"agent_id":"tester","path":"provider.docx"}),
    );
    assert_eq!(resolved["producer"].as_str(), Some("universal:docling"));
    let source_copy = resolved["original_source_path"].as_str().unwrap();
    assert_eq!(fs::read(source_copy).unwrap(), fs::read(&docx).unwrap());
}

#[test]
fn provider_gated_trafilatura_html_flow_skips_without_provider_python() {
    let Some(provider_python) = std::env::var_os("M1ND_PROVIDER_PYTHON") else {
        eprintln!("SKIP: M1ND_PROVIDER_PYTHON not configured");
        return;
    };
    let provider_python = PathBuf::from(provider_python);
    if !provider_python.exists() {
        eprintln!("SKIP: configured provider python missing");
        return;
    }
    let Ok(trafilatura_probe) = std::process::Command::new(&provider_python)
        .arg("-c")
        .arg("import trafilatura")
        .output()
    else {
        eprintln!("SKIP: failed to spawn configured provider python");
        return;
    };
    if !trafilatura_probe.status.success() {
        eprintln!("SKIP: trafilatura not available in provider env");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let docs_root = temp.path().join("provider-html");
    fs::create_dir_all(&docs_root).unwrap();
    let page = docs_root.join("provider.html");
    write(
        &page,
        &html_doc("Provider Html", "SemanticBridge appears here."),
    );

    let mut state = build_state(temp.path());
    call(
        &mut state,
        "ingest",
        json!({"agent_id":"tester","path":page.to_string_lossy().to_string(),"adapter":"universal","mode":"merge"}),
    );
    let resolved = call(
        &mut state,
        "document_resolve",
        json!({"agent_id":"tester","path":"provider.html"}),
    );
    assert_eq!(resolved["producer"].as_str(), Some("universal:trafilatura"));
    assert!(search_count(&mut state, "SemanticBridge") > 0);
}
