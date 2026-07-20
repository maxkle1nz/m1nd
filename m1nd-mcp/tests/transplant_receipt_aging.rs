// === Battery: D5b — the transplant receipt-aging event ===
//
// The ratified decision (PRD §10 D5 option b, §5.B3-rewritten): a transplant
// between two files that BOTH belong to ratified SystemBlocks changes NO
// membership (`membership_fingerprint` hashes only the ordered PATH set), so
// `reconcile` alone would leave the blocks' receipts GREEN while a symbol
// crossed a ratified boundary — a structural lie-window.
//
// The fix (reuse, never a parallel mechanism): after a successful transplant
// write, every SystemBlock whose membership CLAIMS a touched file has its
// `boundary_version` bumped through the store's OCC path — which, by the
// EXISTING rollup + `stale_scope` law, makes every receipt earned against the
// older boundary stale by scope. The receipt records `blocks_touched[]` so the
// verb never silently ages a boundary.
//
// These COMPILE now and FAIL until the aging exists (the RED half): the verb
// runs, but the boundaries do NOT move and `blocks_touched` is absent.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use m1nd_mcp::system_blocks::{
    import_receipt_in_dir, import_seed_into_dir, ratify_in_dir, recompute_in_dir, Receipt,
    ReceiptEmitter, ReceiptEmitterKind, ReceiptEvidence, ReceiptScope, ReceiptType,
    ReceiptValidity, SystemBlockStore,
};
use std::path::Path;

// ===========================================================================
// Shared fixture (mirrors tests/transplant_battery.rs — the canonical scenario)
// ===========================================================================

fn make_state(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        ..McpConfig::default()
    };
    let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
        .expect("init session");
    state.ingest_roots = vec![root.to_string_lossy().to_string()];
    state
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

const ALPHA: &str = r#"//! Alpha: the transplant SOURCE file.

/// Doc comment that must TRAVEL with the item (trivia-ownership law).
pub fn move_me(x: u32) -> u32 {
    let base = private_helper(x);
    shared_helper(base) + 1
}

fn private_helper(x: u32) -> u32 {
    x * 2
}

fn shared_helper(x: u32) -> u32 {
    x + 10
}

pub fn stay_here(x: u32) -> u32 {
    shared_helper(x)
}
"#;

const BETA: &str = r#"//! Beta: the transplant DESTINATION file.

pub fn existing_resident(x: u32) -> u32 {
    x - 1
}
"#;

const GAMMA: &str = r#"//! Gamma: an external REFERENCER of the moved symbol.

use crate::alpha::move_me;

pub fn call_it() -> u32 {
    move_me(21)
}
"#;

fn seed_fixture(state: &mut SessionState, root: &Path) {
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"fixture-transplant\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod gamma;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(&root.join("src/gamma.rs"), GAMMA);

    let ingest = m1nd_mcp::tools::handle_ingest(
        state,
        m1nd_mcp::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "aging".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            adapter: "code".to_string(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            project_root: None,
        },
    )
    .expect("fixture ingest");
    let nodes = ingest
        .get("node_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(nodes >= 5, "fixture must populate the graph, got {nodes}");
}

fn transplant_params(root: &Path) -> serde_json::Value {
    serde_json::json!({
        "agent_id": "aging",
        "symbol": "move_me",
        "source_file": root.join("src/alpha.rs").to_string_lossy(),
        "dest_file": root.join("src/beta.rs").to_string_lossy(),
    })
}

/// A ratifiable, anti-poison-clean `test` receipt scoped to `(block, boundary,
/// contract)` — the shape `import_receipt` accepts (full execution identity).
fn earned_receipt(block_id: &str, boundary: u32, contract: u32) -> Receipt {
    Receipt {
        type_: ReceiptType::Test,
        emitter: ReceiptEmitter {
            kind: ReceiptEmitterKind::Ci,
            id: "ci-aging".to_string(),
        },
        scope: ReceiptScope {
            block_id: block_id.to_string(),
            boundary_version: boundary,
            contract_version: contract,
            resolution_hash: "sha256:res".to_string(),
        },
        evidence: ReceiptEvidence {
            command: Some("cargo test".to_string()),
            cwd: Some(".".to_string()),
            exit_status: Some(0),
            started_at: Some("2026-07-09T00:00:00Z".to_string()),
            ended_at: Some("2026-07-09T00:01:00Z".to_string()),
            artifact_hash: "sha256:art".to_string(),
            stdout_excerpt: Some("test result: ok".to_string()),
            evidence_refs: vec!["artifacts/x.txt".to_string()],
        },
        validity: ReceiptValidity {
            expires_on: None,
            stales_on: Vec::new(),
        },
    }
}

/// A two-block seed. `sb_source` claims `src/alpha.rs` (the transplant's source
/// file), `sb_dest` claims `src/beta.rs` (its dest). Both candidate → the battery
/// ratifies them. `src/gamma.rs` (the referencer) is claimed by NEITHER.
fn two_block_seed_claiming_moved_files() -> &'static str {
    r#"{
  "schema": "m1nd-system-block-seed-v0",
  "repo": { "repo_id": "repo_x", "root": ".", "source_commit": "x" },
  "skeleton": {
    "skeleton_id": "sk_x",
    "version": 1,
    "state": "candidate",
    "ratification": { "method": "", "ratifier": "", "ratified_at": "", "commit": "" }
  },
  "blocks": [
    {
      "block_id": "sb_source",
      "name": "Source",
      "purpose": "Owns the transplant source file.",
      "kind": "scanned",
      "state": "candidate",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "proposed",
      "membership": [ { "path": "src/alpha.rs", "role": "primary" } ],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    },
    {
      "block_id": "sb_dest",
      "name": "Dest",
      "purpose": "Owns the transplant dest file.",
      "kind": "scanned",
      "state": "candidate",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "proposed",
      "membership": [ { "path": "src/beta.rs", "role": "primary" } ],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    }
  ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}"#
}

/// A two-block seed whose blocks claim files the canonical transplant NEVER
/// touches (`other/**`, `docs/**`) — the negative control.
fn two_block_seed_claiming_unrelated_files() -> &'static str {
    r#"{
  "schema": "m1nd-system-block-seed-v0",
  "repo": { "repo_id": "repo_y", "root": ".", "source_commit": "y" },
  "skeleton": {
    "skeleton_id": "sk_y",
    "version": 1,
    "state": "candidate",
    "ratification": { "method": "", "ratifier": "", "ratified_at": "", "commit": "" }
  },
  "blocks": [
    {
      "block_id": "sb_docs",
      "name": "Docs",
      "purpose": "Owns docs, untouched by any code move.",
      "kind": "scanned",
      "state": "candidate",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "proposed",
      "membership": [ { "path": "docs/**", "role": "docs" } ],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    },
    {
      "block_id": "sb_other",
      "name": "Other",
      "purpose": "Owns an unrelated subtree.",
      "kind": "scanned",
      "state": "candidate",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "proposed",
      "membership": [ { "path": "other/**", "role": "primary" } ],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    }
  ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}"#
}

// ===========================================================================
// D5b positive — a transplant between two ratified blocks ages BOTH boundaries
// and stales the source block's receipt by scope.
// ===========================================================================

#[test]
fn d5b_transplant_between_ratified_blocks_ages_both_boundaries_and_stales_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);

    // The store lives in the brain runtime dir (= runtime_root, here `root`).
    let store_dir = state.runtime_root.clone();
    let outcome = import_seed_into_dir(&store_dir, two_block_seed_claiming_moved_files(), false)
        .expect("seed the 2-block store");
    let v = outcome.store.store_version; // 1

    // Attach a fresh receipt to the SOURCE block at boundary 1 / contract 1.
    let store = import_receipt_in_dir(
        &store_dir,
        v,
        "sb_source",
        earned_receipt("sb_source", 1, 1),
    )
    .expect("earn a receipt on the source block");
    let v = store.store_version;

    // Ratify both blocks (candidate → ratified; boundary_version is untouched).
    let (store, _) = ratify_in_dir(&store_dir, v, None, "owner", "2026-07-20T00:00:00Z")
        .expect("ratify the blocks");
    assert_eq!(
        store.blocks[0].boundary_version, 1,
        "ratify never bumps boundary"
    );
    let version_before = store.store_version;

    // Before the transplant the receipt is FRESH (scope still binds boundary 1).
    let before = recompute_in_dir(&store_dir, Some("sb_source"), "2999-01-01T00:00:00Z")
        .expect("recompute before");
    assert_eq!(
        before.fresh_count, 1,
        "receipt fresh before the boundary moves"
    );

    // THE VERB: move `move_me` alpha → beta. Both files belong to ratified blocks.
    let receipt = dispatch_tool(&mut state, "transplant", &transplant_params(root))
        .expect("transplant succeeds");

    // The receipt names the aged blocks — never a silent boundary move.
    let touched: Vec<String> = receipt
        .get("blocks_touched")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        touched.contains(&"sb_source".to_string()) && touched.contains(&"sb_dest".to_string()),
        "the receipt must record BOTH aged blocks in blocks_touched, got {touched:?}"
    );

    // Both boundaries moved 1 → 2 (the symbol crossed a ratified boundary).
    let store = SystemBlockStore::load(&store_dir).unwrap().unwrap();
    let src_block = store
        .blocks
        .iter()
        .find(|b| b.block_id == "sb_source")
        .unwrap();
    let dst_block = store
        .blocks
        .iter()
        .find(|b| b.block_id == "sb_dest")
        .unwrap();
    assert_eq!(
        src_block.boundary_version, 2,
        "source block boundary bumped"
    );
    assert_eq!(dst_block.boundary_version, 2, "dest block boundary bumped");
    assert!(
        store.store_version > version_before,
        "the aging bumped the OCC counter once"
    );

    // The receipt (scoped boundary 1) is now STALE by scope — the EXISTING
    // stale_scope law cascaded from the bump, with zero receipt-specific code.
    let after = recompute_in_dir(&store_dir, Some("sb_source"), "2999-01-01T00:00:00Z")
        .expect("recompute after");
    assert_eq!(after.stale_count, 1, "the earned receipt is now stale");
    assert_eq!(
        after.receipts[0].reason.as_deref(),
        Some("boundary"),
        "stale by boundary (the ratified fronteira moved)"
    );
}

// ===========================================================================
// D5b negative — a transplant touching NO block files bumps NOTHING.
// ===========================================================================

#[test]
fn d5b_transplant_touching_no_block_files_ages_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);

    let store_dir = state.runtime_root.clone();
    let outcome =
        import_seed_into_dir(&store_dir, two_block_seed_claiming_unrelated_files(), false)
            .expect("seed the unrelated 2-block store");
    let v = outcome.store.store_version;

    let store = import_receipt_in_dir(&store_dir, v, "sb_docs", earned_receipt("sb_docs", 1, 1))
        .expect("earn a receipt");
    let v = store.store_version;
    let (store, _) =
        ratify_in_dir(&store_dir, v, None, "owner", "2026-07-20T00:00:00Z").expect("ratify");
    let version_before = store.store_version;

    // The transplant touches only alpha/beta/gamma — none claimed by these blocks.
    let receipt = dispatch_tool(&mut state, "transplant", &transplant_params(root))
        .expect("transplant succeeds");
    let touched: Vec<String> = receipt
        .get("blocks_touched")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        touched.is_empty(),
        "no block claims a touched file — blocks_touched must be empty, got {touched:?}"
    );

    let store = SystemBlockStore::load(&store_dir).unwrap().unwrap();
    for b in &store.blocks {
        assert_eq!(
            b.boundary_version, 1,
            "no boundary may move: {}",
            b.block_id
        );
    }
    assert_eq!(
        store.store_version, version_before,
        "an aging that ages nothing never bumps the OCC counter"
    );

    // The receipt is still fresh — no boundary moved under it.
    let after =
        recompute_in_dir(&store_dir, Some("sb_docs"), "2999-01-01T00:00:00Z").expect("recompute");
    assert_eq!(after.fresh_count, 1, "receipt stays fresh");
}
