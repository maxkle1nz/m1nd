// === Battery: a runtime living AT the ingest root never ingests itself ===
//
// THE DEFECT, measured on a real birth ceremony with `runtime == repo root`
// (the `a_runtime_at_the_repo_root_is_home_too…` layout): the source walk
// swallowed the runtime's own state — 32 of 39 graph nodes were checkpoint
// blobs, lease files and boot sidecars, not the user's code — and the
// walk↔`require_complete` freshness digest then raced the live lease/daemon
// writers, dying with `FullReindexRequired: VCS/file-metadata context changed
// since extraction` whenever one of those files moved between the two walks
// (a race ubuntu loses and macOS happened to win).
//
// `path_policy::RUNTIME_ARTIFACT_FILE_NAMES` was born to filter exactly this
// class and aged silently: nothing ever validated it against what a session
// really writes, so ten names filtered and twenty did not. These tests are the
// missing validator plus the exclusion's own contract.

use crate::server::McpConfig;
use crate::session::SessionState;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use std::path::Path;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

/// A session whose runtime root IS the fixture root — the layout under test.
fn make_state_at_root(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        registry_dir: Some(root.join("registry")),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

fn write_fixture_crate(root: &Path) {
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"repo-gamma\"\nversion = \"0.0.0\"\n",
    );
    write(
        &root.join("src/lib.rs"),
        "pub fn alpha() -> u32 { 1 }\npub fn beta() -> u32 { alpha() + 1 }\n",
    );
}

fn ingest_root(state: &mut SessionState, root: &Path) -> serde_json::Value {
    crate::tools::handle_ingest(
        state,
        crate::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "runtime-exclusion-battery".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            adapter: "code".to_string(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            project_root: None,
        },
    )
    .expect("ingest")
}

/// The exclusion contract: state files the runtime plants at the scanned root
/// never become graph nodes, whether they were there before the walk (this
/// test) or appear between extraction and revalidation (the ubuntu race the
/// e2e ceremony test covers).
#[test]
fn a_runtime_living_at_the_ingest_root_never_ingests_its_own_state() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    write_fixture_crate(root);
    let mut state = make_state_at_root(root);

    // Plant the exact artifacts a booting owner leaves at its runtime root —
    // the ones the old name list missed, measured from a real ceremony.
    write(&root.join("boot_config_v1.json"), "{}");
    write(&root.join("daemon_state.json"), "{}");
    write(&root.join("temporal_state_v1.json"), "{}");
    write(&root.join("checkpoint-working-set-v1.json"), "{}");
    write(&root.join("checkpoint-store/CURRENT"), "cp");
    write(
        &root.join("checkpoint-store/checkpoints/aa/manifest.json"),
        "{}",
    );
    write(&root.join("registry/leases/deadbeef.owner.lock"), "{}");
    write(&root.join("registry/instances/inst_1.json"), "{}");

    let out = ingest_root(&mut state, root);
    let node_count = out.get("node_count").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(
        node_count > 0,
        "the fixture itself must produce a graph: {out}"
    );

    let graph = state.graph.read();
    let markers = [
        "boot_config",
        "daemon_state",
        "temporal_state",
        "checkpoint-working-set",
        "checkpoint-store",
        "registry/",
        "owner.lock",
    ];
    let mut polluted: Vec<String> = Vec::new();
    for interned in graph.id_to_node.keys() {
        let ext_id = graph.strings.resolve(*interned);
        if markers.iter().any(|marker| ext_id.contains(marker)) {
            polluted.push(ext_id.to_string());
        }
    }
    assert!(
        polluted.is_empty(),
        "the runtime's own state entered the graph as source: {polluted:?}"
    );
}

/// The anti-aging gate the old list never had: boot a REAL session over its own
/// root, ingest, persist, and require every file the session actually wrote to
/// be covered by the exclusion (hidden, walk-level noise, or the owned lists in
/// `tools.rs`). When a new sidecar lands at the runtime root, this test fails
/// NAMING it — grow `RUNTIME_OWNED_ROOT_FILES`/`_DIRS` then, never from memory.
#[test]
fn the_runtime_owned_list_covers_what_a_real_session_writes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    write_fixture_crate(root);
    let mut state = make_state_at_root(root);
    let _ = ingest_root(&mut state, root);
    state.persist().expect("persist session state");

    let fixture_entries = ["Cargo.toml", "src"];
    let mut uncovered: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(root).expect("read runtime root") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().to_string();
        // A rename-based backup of covered state is covered state.
        let base = name.strip_suffix(".bak").unwrap_or(&name).to_string();
        let covered = name.starts_with('.')
            || fixture_entries.contains(&name.as_str())
            || m1nd_ingest::path_policy::is_runtime_artifact_file_name(&base)
            || crate::tools::RUNTIME_OWNED_ROOT_DIRS.contains(&base.as_str())
            || crate::tools::RUNTIME_OWNED_ROOT_FILES.contains(&base.as_str());
        if !covered {
            uncovered.push(name);
        }
    }
    uncovered.sort();
    assert!(
        uncovered.is_empty(),
        "the session wrote state at its runtime root that no exclusion covers — \
         add these to RUNTIME_OWNED_ROOT_FILES/_DIRS in tools.rs (or the walk-level \
         policy) so the runtime-at-root layout cannot ingest them: {uncovered:?}"
    );
}
