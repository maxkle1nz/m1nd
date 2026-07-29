//! SPEC-1 — `graph.ingest.refresh_declared_root`, the freshness door.
//!
//! The normative document is `docs/GENESIS-INGEST-CONSUMERS-SPEC.md` (RATIFIED,
//! owner, 2026-07-29, all four §6 items). This file IS its §5 acceptance list —
//! written before the implementation, born RED against today's binary, and
//! never edited afterwards to make the implementation pass.
//!
//! What the door is, in one line: the ONE opening in the authority wall. It
//! re-ingests a root the bound brain has ALREADY declared. It never creates a
//! brain, never adds a root, never crosses to another brain's territory.
//!
//! Numbering below follows the spec's own §5 list so a reader can check the
//! battery against the document item by item. §5.7 (birth) and the birth half of
//! §5.8 belong to SPEC-2 and are deliberately absent — SPEC-2 is not this PR.
//!
//! WHY THE POLICY GATE IS IN EVERY CALL. `enforce_generic_action_policy` is the
//! pure, pre-brain admission seam (spec R-I) — the door's lock. Driving
//! `dispatch_tool` without it would test the handler while leaving the lock
//! untested, and on today's binary it would run a DESTRUCTIVE replace, because
//! `normalized_ingest_mode` maps an unknown mode to `"replace"`. So every call
//! here goes through `admit_then_dispatch`, exactly as both transports do.

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, enforce_generic_action_policy, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::path::{Path, PathBuf};

const AGENT: &str = "spec1-refresh-probe";

/// A brain whose runtime lives under `runtime`, with no roots declared yet.
fn build_state(runtime: &Path) -> SessionState {
    std::fs::create_dir_all(runtime).expect("runtime dir");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

/// A small, deterministic Rust crate. `extra` adds one more module so a later
/// call can absorb a "new commit" and the node count moves for a reason.
fn write_repo(root: &Path, extra: Option<(&str, &str)>) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"spec1fixture\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod helper;\npub fn top() -> i64 { helper::help() + 1 }\n\
         pub struct Top { pub v: i64 }\npub fn second() -> i64 { 2 }\n",
    )
    .expect("lib.rs");
    std::fs::write(
        root.join("src/helper.rs"),
        "pub fn help() -> i64 { 41 }\npub struct Helper { pub v: i64 }\n\
         pub fn helper_two() -> i64 { 7 }\npub fn helper_three() -> i64 { 8 }\n",
    )
    .expect("helper.rs");
    if let Some((name, body)) = extra {
        std::fs::write(root.join("src").join(name), body).expect("extra module");
    }
}

/// Seed the brain the way the field seeds one: the trusted library ingest that
/// production `ingest` itself uses, then declare the root. The public generic
/// route cannot do this — that is the wall SPEC-1 opens exactly one door in.
fn seed_declared_root(state: &mut SessionState, repo: &Path) {
    let (graph, _) = m1nd_ingest::Ingestor::new(m1nd_ingest::IngestConfig {
        root: repo.to_path_buf(),
        parallelism: 1,
        ..m1nd_ingest::IngestConfig::default()
    })
    .ingest()
    .expect("trusted fixture ingest");
    {
        let mut live = state.graph.write();
        *live = graph;
        if !live.finalized {
            live.finalize().expect("finalize seeded graph");
        }
    }
    state.rebuild_engines().expect("rebuild engines");
    let declared = canonical(repo);
    state.ingest_roots = vec![declared.clone()];
    state.workspace_root = Some(declared);
    state.persist().expect("persist the seeded brain");
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn node_count(state: &SessionState) -> u32 {
    state.graph.read().num_nodes()
}

/// The exact sequence both transports run: the pure policy gate first, then
/// dispatch. Returns the gate's refusal as `Err`, the handler's payload as `Ok`.
fn admit_then_dispatch(
    state: &mut SessionState,
    tool: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    enforce_generic_action_policy(tool, params).map_err(|error| error.to_string())?;
    dispatch_tool(state, tool, params).map_err(|error| error.to_string())
}

fn refresh_params(path: &str) -> serde_json::Value {
    json!({ "path": path, "agent_id": AGENT, "mode": "refresh" })
}

// ---------------------------------------------------------------------------
// §5.1 — the hijack stays dead
// ---------------------------------------------------------------------------

/// Opening ONE door must not widen the wall. A foreign-root `replace` — the R-D
/// hijack class, two bound-brain replacements in 24h on the deployed 1.4.x owner
/// — keeps today's refusal, verbatim, including the floor it names.
#[test]
fn spec1_5_1_foreign_root_replace_keeps_todays_refusal_bytes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);
    let foreign = temp.path().join("foreign");
    std::fs::create_dir_all(&foreign).expect("foreign root");

    let refusal = admit_then_dispatch(
        &mut state,
        "ingest",
        &json!({ "path": canonical(&foreign), "agent_id": AGENT, "mode": "replace" }),
    )
    .expect_err("a foreign-root replace must stay refused");

    assert_eq!(
        refusal,
        "invalid params for ingest: generic_action_authority_required: \
         semantic_action=graph.ingest.replace authority_floor=POSITIVE_SOVEREIGN \
         cannot use generic REST/MCP dispatch; no exact typed G2/G3 lease \
         consumer is installed for this action"
    );
}

// ---------------------------------------------------------------------------
// §5.2 — the verdict's kill-shot, the first RED case
// ---------------------------------------------------------------------------

/// `covers_root` is PREFIX (spec R-F), so a caller at `<root>/m1nd-ui` is
/// "covered" by the brain at `<root>`. The refresh must NOT reuse it: SPEC-1.2
/// admits only EQUALITY of canonical keys. A descendant refuses.
#[test]
fn spec1_5_2_descendant_root_refresh_refuses_root_not_exact() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);
    let descendant = repo.join("m1nd-ui");
    std::fs::create_dir_all(&descendant).expect("descendant dir");
    let before = node_count(&state);

    state.caller_root = Some(canonical(&descendant));
    let payload = admit_then_dispatch(
        &mut state,
        "ingest",
        &refresh_params(&canonical(&descendant)),
    )
    .expect("the refresh action must be admitted and answer with a refusal payload");

    assert_eq!(payload["ok"], json!(false));
    assert_eq!(payload["refused"], json!("refresh_root_not_exact"));
    // The prefix predicate would have said yes. Pin that it was not consulted.
    assert!(
        state.covers_root(&canonical(&descendant)),
        "precondition: the descendant IS covered by the prefix predicate — that is \
         exactly why the exact predicate has to be a different function"
    );
    assert_eq!(node_count(&state), before, "a refusal mutates nothing");
}

// ---------------------------------------------------------------------------
// §5.3 — ingress canonicalization (SPEC-1b), never string matching
// ---------------------------------------------------------------------------

/// `<root>/../out` is textually "inside" nothing and resolves OUTSIDE the root.
/// `canonical_key` resolves it; the predicate then refuses on equality.
#[test]
fn spec1_5_3a_parent_traversal_caller_refuses() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);
    let out = temp.path().join("out");
    std::fs::create_dir_all(&out).expect("out dir");
    let traversal = repo.join("..").join("out").to_string_lossy().to_string();

    state.caller_root = Some(traversal.clone());
    let payload = admit_then_dispatch(&mut state, "ingest", &refresh_params(&traversal))
        .expect("admitted, then refused in the handler");

    assert_eq!(payload["refused"], json!("refresh_root_not_exact"));
}

/// A symlink INSIDE the root pointing OUT of it. The textual layer sees a child
/// of the declared root; `canonical_key` resolves the link and sees a stranger.
#[cfg(unix)]
#[test]
fn spec1_5_3b_symlink_out_of_root_refuses() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).expect("outside dir");
    let link = repo.join("escape");
    std::os::unix::fs::symlink(&outside, &link).expect("symlink out of the root");
    let via_link = link.to_string_lossy().to_string();

    state.caller_root = Some(via_link.clone());
    let payload = admit_then_dispatch(&mut state, "ingest", &refresh_params(&via_link))
        .expect("admitted, then refused in the handler");

    assert_eq!(payload["refused"], json!("refresh_root_not_exact"));
}

/// The `/tmp` → `/private/tmp` alias (spec R-J). Two spellings of ONE directory
/// must reach the SAME decision — a brain that declares one spelling must accept
/// the other, because they are the same root.
#[cfg(target_os = "macos")]
#[test]
fn spec1_5_3c_tmp_alias_reaches_the_same_decision() {
    let temp = tempfile::TempDir::with_prefix_in("spec1-alias-", "/tmp").expect("tempdir in /tmp");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);

    let unresolved = repo.to_string_lossy().to_string();
    let resolved = canonical(&repo);
    assert_ne!(
        unresolved, resolved,
        "precondition: this fixture must actually exercise the /tmp alias"
    );

    state.caller_root = Some(unresolved.clone());
    let via_alias = admit_then_dispatch(&mut state, "ingest", &refresh_params(&unresolved))
        .expect("the aliased spelling must be admitted");
    state.caller_root = Some(resolved.clone());
    let via_resolved = admit_then_dispatch(&mut state, "ingest", &refresh_params(&resolved))
        .expect("the resolved spelling must be admitted");

    assert_eq!(
        via_alias["ok"], via_resolved["ok"],
        "/tmp and /private/tmp must reach the same decision, not two"
    );
    assert_eq!(via_alias["ok"], json!(true));
}

/// Two textually equal NONEXISTENT paths must never match each other.
/// `canonical_key` falls back to the raw string when a path does not resolve
/// (spec §1.2 SPEC-1b), so the door has to refuse unresolvable paths ITSELF.
#[test]
fn spec1_5_3d_nonexistent_path_refuses_rather_than_string_matching() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);

    // A path that does not exist, declared verbatim as a root. Under a pure
    // string comparison this is an exact match and the refresh would be admitted.
    let ghost = temp
        .path()
        .join("ghost-root-that-never-existed")
        .to_string_lossy()
        .to_string();
    state.ingest_roots.push(ghost.clone());

    state.caller_root = Some(ghost.clone());
    let payload = admit_then_dispatch(&mut state, "ingest", &refresh_params(&ghost))
        .expect("admitted, then refused in the handler");

    assert_eq!(payload["refused"], json!("refresh_root_unresolvable"));
}

/// The door authenticates a ROOT RELATIONSHIP (spec §1.3). With no caller root
/// there is no relationship to authenticate, so it is fail-closed, not open.
#[test]
fn spec1_1b_unknown_caller_root_refuses_fail_closed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);
    assert!(
        state.caller_root.is_none(),
        "precondition: stdio sends no root"
    );

    let payload = admit_then_dispatch(&mut state, "ingest", &refresh_params(&canonical(&repo)))
        .expect("admitted, then refused in the handler");

    assert_eq!(payload["refused"], json!("refresh_caller_root_unknown"));
}

// ---------------------------------------------------------------------------
// §5.4 — the happy path
// ---------------------------------------------------------------------------

/// Exact-root refresh succeeds, absorbs a new commit, leaves the root set
/// unchanged (SPEC-1d) and leaves a journaled receipt (SPEC-1f).
#[test]
fn spec1_5_4_exact_root_refresh_absorbs_a_new_commit_and_keeps_the_root_set() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);
    let before = node_count(&state);
    let roots_before = state.ingest_roots.clone();
    let workspace_before = state.workspace_root.clone();

    // The "new commit": one more module the brain has never seen.
    write_repo(
        &repo,
        Some((
            "arrived.rs",
            "pub fn arrived() -> i64 { 99 }\npub struct Arrived { pub v: i64 }\n\
             pub fn arrived_two() -> i64 { 100 }\n",
        )),
    );

    state.caller_root = Some(canonical(&repo));
    let payload = admit_then_dispatch(&mut state, "ingest", &refresh_params(&canonical(&repo)))
        .expect("an exact-root refresh must succeed");

    assert_eq!(payload["ok"], json!(true), "payload was {payload}");
    assert_eq!(payload["mode"], json!("refresh"));
    assert!(
        node_count(&state) > before,
        "the refresh must absorb the new commit: {before} → {}",
        node_count(&state)
    );
    assert_eq!(
        state.ingest_roots, roots_before,
        "SPEC-1d: root set unchanged"
    );
    assert_eq!(
        state.workspace_root, workspace_before,
        "SPEC-1d: binding unchanged"
    );
    // SPEC-1f: a receipt, naming both node counts and the root it refreshed.
    assert_eq!(payload["refreshed_root"], json!(canonical(&repo)));
    assert_eq!(payload["node_count_before"], json!(before));
    assert_eq!(payload["node_count"], json!(node_count(&state)));
}

// ---------------------------------------------------------------------------
// §5.5 — the shrink floor, ratified at 60%
// ---------------------------------------------------------------------------

/// The R-D damage signature is a NARROW scan replacing a WIDE graph. The persist
/// layer's own guard is fail-open by written design (spec R-G: it backs up and
/// writes anyway), so this armor is the refresh's own: candidate first, and a
/// candidate under 60% of the live node count REFUSES, names BOTH counts, and
/// mutates nothing.
#[test]
fn spec1_5_5_narrow_scan_refuses_would_shrink_graph_with_the_graph_untouched() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    // Widen the repo well past the floor so deleting most of it is a real shrink.
    for index in 0..12 {
        std::fs::write(
            repo.join("src").join(format!("wide_{index}.rs")),
            format!(
                "pub fn wide_{index}() -> i64 {{ {index} }}\n\
                 pub struct Wide{index} {{ pub v: i64 }}\n\
                 pub fn wide_{index}_b() -> i64 {{ {index} }}\n"
            ),
        )
        .expect("wide module");
    }
    seed_declared_root(&mut state, &repo);
    let before = node_count(&state);
    let snapshot_before = std::fs::read(&state.graph_path).expect("snapshot exists after seeding");

    // The narrow scan: the repo loses almost everything between two calls.
    for index in 0..12 {
        std::fs::remove_file(repo.join("src").join(format!("wide_{index}.rs"))).expect("rm wide");
    }
    std::fs::remove_file(repo.join("src/helper.rs")).expect("rm helper");
    std::fs::write(repo.join("src/lib.rs"), "pub fn top() -> i64 { 1 }\n").expect("shrink lib.rs");

    state.caller_root = Some(canonical(&repo));
    let payload = admit_then_dispatch(&mut state, "ingest", &refresh_params(&canonical(&repo)))
        .expect("admitted, then refused in the handler");

    assert_eq!(payload["refused"], json!("refresh_would_shrink_graph"));
    // "names BOTH counts" — the spec's words, so both are asserted present.
    assert_eq!(payload["live_node_count"], json!(before));
    let candidate = payload["candidate_node_count"]
        .as_u64()
        .expect("the refusal must name the candidate's node count");
    assert!(
        candidate * 100 < u64::from(before) * 60,
        "precondition: the fixture must actually fall below the ratified 60% floor \
         ({candidate} vs {before})"
    );
    assert_eq!(payload["floor_percent"], json!(60));

    assert_eq!(node_count(&state), before, "the live graph is untouched");
    assert_eq!(
        std::fs::read(&state.graph_path).expect("snapshot still there"),
        snapshot_before,
        "not one byte of the durable snapshot may move on a refusal"
    );
}

// ---------------------------------------------------------------------------
// §5.6 — one admission seam, both transports (SPEC-1g)
// ---------------------------------------------------------------------------

/// An explicit `?brain=` selector NEVER satisfies the exact-root predicate, and
/// therefore buys NOTHING: a refresh under a selector refuses byte-identically
/// to the plain MCP non-exact case. Byte equality is the assertion because the
/// spec's guarantee is that the two transports share ONE seam — not two seams
/// that happen to agree today.
///
/// The REST selector precedent this closes is R-K: `caller_root` is already
/// overwritten under `?brain=` for the skeleton-write family. §5.6's sibling
/// assertion — that `ingest` never joins that family — is pinned below.
#[test]
fn spec1_5_6_rest_brain_selector_refuses_byte_identically_to_mcp() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);
    let stranger = temp.path().join("stranger");
    std::fs::create_dir_all(&stranger).expect("stranger dir");

    // (a) plain MCP, caller root that is not a declared root.
    state.caller_root = Some(canonical(&stranger));
    state.explicit_brain_selector = false;
    let mcp = admit_then_dispatch(&mut state, "ingest", &refresh_params(&canonical(&stranger)))
        .expect("admitted, then refused");

    // (b) REST under an explicit `?brain=` selector, caller root EXACTLY the
    //     declared root — the case a selector would otherwise "unlock".
    state.caller_root = Some(canonical(&repo));
    state.explicit_brain_selector = true;
    let rest = admit_then_dispatch(&mut state, "ingest", &refresh_params(&canonical(&repo)))
        .expect("admitted, then refused");
    state.explicit_brain_selector = false;

    assert_eq!(rest["refused"], json!("refresh_root_not_exact"));
    assert_eq!(
        serde_json::to_string(&mcp).unwrap(),
        serde_json::to_string(&rest).unwrap(),
        "a selector must refuse byte-identically, not merely similarly"
    );
}

/// `ingest`/refresh must never join the `skeleton_write_needs_root_gate`
/// caller-root overwrite list (spec R-K, verdict RC-7). If it did, the REST
/// selector would REWRITE `caller_root` to the selected brain's own workspace
/// root — and the exact-root predicate would then authenticate the door against
/// a value the door itself just invented.
#[test]
fn spec1_5_6b_refresh_never_joins_the_caller_root_overwrite_list() {
    for mode in ["refresh", "replace", "merge"] {
        assert!(
            !m1nd_mcp::server::skeleton_write_needs_root_gate(
                "ingest",
                &json!({ "path": "/x", "agent_id": AGENT, "mode": mode }),
            ),
            "ingest mode={mode} must never overwrite caller_root under ?brain="
        );
    }
}

// ---------------------------------------------------------------------------
// §5.8 (refresh half) — old-or-new, never mixed
// ---------------------------------------------------------------------------

/// The refresh's durable transition must be SINGLE and ATOMIC, so a crash lands
/// on the old graph or the new one and never on a blend.
///
/// WHAT THIS TEST EXECUTES: that every refusal path leaves the durable snapshot
/// byte-identical (nothing is written before the decision), and that the success
/// path leaves the durable snapshot holding exactly the NEW graph — i.e. there
/// is exactly ONE durable transition, and it is `snapshot::save_graph`'s
/// temp-file + `rename` (FM-PL-008). A `rename` on one filesystem is atomic, so
/// a `kill -9` at any instant lands on one side of it.
///
/// DECLARED BOUNDARY — the fault-injection half is NOT executed here, and is not
/// claimed. Killing a real process mid-refresh needs a live owner driven over a
/// transport that carries `M1nd-Caller-Root`; the existing subprocess harness
/// (`tests/persist_runtime_root.rs`) speaks stdio, which carries no caller root,
/// so SPEC-1 correctly refuses there and the verb cannot be driven to the point
/// of the kill. This is the SAME declared boundary that harness already carries
/// for its own crash cycle ("Cycle B — crash / `kill -9` with no checkpoint — is
/// deliberately NOT here. It is wave 2, and until it lands the fault-injection
/// half of this property stays NOT_RUN rather than silently claimed"). SPEC-1's
/// refresh joins that wave rather than growing a second, weaker harness.
#[test]
fn spec1_5_8_refresh_durable_transition_is_single_and_atomic() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo, None);
    seed_declared_root(&mut state, &repo);
    let snapshot_path = state.graph_path.clone();
    let before_bytes = std::fs::read(&snapshot_path).expect("seeded snapshot");
    let before_nodes = node_count(&state);

    // Every refusal reason, one after another: not one may move a durable byte.
    let stranger = temp.path().join("stranger");
    std::fs::create_dir_all(&stranger).expect("stranger dir");
    let refusals: Vec<(Option<String>, String)> = vec![
        (None, canonical(&repo)),
        (Some(canonical(&stranger)), canonical(&stranger)),
        (
            Some(temp.path().join("ghost").to_string_lossy().to_string()),
            temp.path().join("ghost").to_string_lossy().to_string(),
        ),
    ];
    for (caller, path) in refusals {
        state.caller_root = caller;
        let payload =
            admit_then_dispatch(&mut state, "ingest", &refresh_params(&path)).expect("refusal");
        assert_eq!(payload["ok"], json!(false));
        assert_eq!(
            std::fs::read(&snapshot_path).expect("snapshot"),
            before_bytes,
            "a refused refresh wrote to the durable snapshot"
        );
    }

    // The success path: the durable snapshot ends holding the NEW graph — not
    // the old one, and not something between the two.
    write_repo(
        &repo,
        Some((
            "arrived.rs",
            "pub fn arrived() -> i64 { 99 }\npub struct A { pub v: i64 }\n",
        )),
    );
    state.caller_root = Some(canonical(&repo));
    let payload = admit_then_dispatch(&mut state, "ingest", &refresh_params(&canonical(&repo)))
        .expect("the exact-root refresh must succeed");
    assert_eq!(payload["ok"], json!(true));

    let after_nodes = node_count(&state);
    assert!(after_nodes > before_nodes);
    let durable = std::fs::read_to_string(&snapshot_path).expect("snapshot after refresh");
    let durable: serde_json::Value = serde_json::from_str(&durable).expect("snapshot parses");
    assert_eq!(
        durable["nodes"].as_array().map(Vec::len),
        Some(after_nodes as usize),
        "the durable snapshot must hold exactly the NEW graph"
    );
    // No temp file survives the transition — a leftover `.tmp` is the footprint
    // of a write that was not a single rename.
    let leftovers: Vec<PathBuf> = std::fs::read_dir(snapshot_path.parent().unwrap())
        .expect("read runtime dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("tmp")
                && path.file_stem().and_then(|stem| stem.to_str()) == Some("graph_snapshot")
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "left a partial write behind: {leftovers:?}"
    );
}

// ---------------------------------------------------------------------------
// §5.9 — the regression pin (verdict RC-4)
// ---------------------------------------------------------------------------

/// The admission MUST be keyed BY ACTION, never by floor. `source.edit.commit`
/// and `graph.ingest.merge_existing` both sit at `SCOPED_GRANT_A2` — the exact
/// floor SPEC-1 is admitted at. If the allowlist were keyed by floor, opening
/// the refresh would open these two as well. Their refusal bytes are pinned
/// here, verbatim, so that mistake cannot be made silently.
#[test]
fn spec1_5_9_scoped_grant_a2_siblings_keep_todays_refusal_bytes() {
    let edit_commit = enforce_generic_action_policy(
        "edit_commit",
        &json!({ "agent_id": AGENT, "edit_id": "e-1" }),
    )
    .expect_err("source.edit.commit must stay refused")
    .to_string();
    assert_eq!(
        edit_commit,
        "invalid params for edit_commit: generic_action_authority_required: \
         semantic_action=source.edit.commit authority_floor=SCOPED_GRANT_A2 \
         cannot use generic REST/MCP dispatch; no exact typed G2/G3 lease \
         consumer is installed for this action"
    );

    let merge_existing = enforce_generic_action_policy(
        "external_mutation_service",
        &json!({ "agent_id": AGENT, "action": "graph_ingest_merge_existing" }),
    )
    .expect_err("graph.ingest.merge_existing must stay refused")
    .to_string();
    assert_eq!(
        merge_existing,
        "invalid params for external_mutation_service: generic_action_authority_required: \
         semantic_action=graph.ingest.merge_existing authority_floor=SCOPED_GRANT_A2 \
         cannot use generic REST/MCP dispatch; no exact typed G2/G3 lease \
         consumer is installed for this action"
    );

    // And the same floor reached through the `ingest` tool's own merge selector,
    // which additionally depends on a trusted fact that production never proves
    // (spec R-I). It must stay unresolved, not become admitted.
    let ingest_merge = enforce_generic_action_policy(
        "ingest",
        &json!({ "path": "/x", "agent_id": AGENT, "mode": "merge" }),
    )
    .expect_err("ingest mode=merge must stay refused")
    .to_string();
    assert!(
        ingest_merge.contains("generic_action_authority_required"),
        "got: {ingest_merge}"
    );
}

/// SPEC-1's own admission, stated positively: the refresh is admitted because
/// its ACTION is named, and the action it is named under is exactly the one the
/// owner ratified at `ScopedGrantA2`, A2-local.
#[test]
fn spec1_1a_refresh_is_admitted_by_action_at_the_ratified_floor() {
    let classified = m1nd_mcp::action_routes::classify_mcp_action(
        "ingest",
        &json!({ "path": "/x", "agent_id": AGENT, "mode": "refresh" }),
        m1nd_mcp::action_routes::TrustedMcpRouteFacts::default(),
    )
    .expect("mode=refresh must classify purely from (tool, params)");
    assert_eq!(
        classified.action.as_str(),
        "graph.ingest.refresh_declared_root"
    );
    assert_eq!(
        classified.authority_floor,
        m1nd_control::AuthorityFloor::ScopedGrantA2,
        "§6 item 2, ratified: ScopedGrantA2, A2-local"
    );
    assert!(
        enforce_generic_action_policy(
            "ingest",
            &json!({ "path": "/x", "agent_id": AGENT, "mode": "refresh" })
        )
        .is_ok(),
        "the action-keyed allowlist must admit the refresh at the dispatch seam"
    );
}
