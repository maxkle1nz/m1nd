//! The first-minute path must ask the SAME two discovery questions `--attach
//! auto` asks.
//!
//! Born RED from a measured field defect (project mailbox letter, 2026-07-31,
//! README demo capture): `m1nd agent context --repo <repo>` and `m1nd agent
//! first-minute` boot an ISOLATED runtime under the OS temp dir with
//! `node_count 0` and answer `ok=false, status=needs_authority` — "connect an
//! authenticated typed governed owner provider" — while the served owner on the
//! same machine, in the same minute, holds the real graph (18,084 nodes, 73,332
//! edges, 129 roots) and already declares that repo among its ingest roots.
//!
//! `--attach auto` was cured of exactly this defect class (#480): discovery's
//! SECOND question finds any live serve ReadWrite owner whose declared ingest
//! roots COVER the caller's repo. The cure lives in
//! `instance_registry::discover_serve_owner`. The first-minute path never
//! adopted it, because it had no way to ASK: the npm agent CLI is JavaScript and
//! the discovery is Rust, so nothing outside the `--attach` code path could put
//! the question.
//!
//! This battery is the acceptance battery for that missing seam — a one-shot,
//! read-only projection of the SAME discovery (`probe_serve_owner`), which the
//! npm CLI drives via `m1nd-mcp --discover-owner` before it decides whether to
//! boot a blind sidecar. It must never become a second discovery: every case
//! here is an answer `discover_serve_owner` already gives, restated in the
//! wire shape a non-Rust caller can read.
//!
//! Fixtures mirror `attach_auto_ingest_coverage.rs` exactly: an in-process
//! tempdir registry with hand-written `instances/*.json` entries and a real
//! `ingest_roots.json` beside each owner's graph. No subprocess, no lease, no
//! port bound. `pid` is this test process, so every fixture owner is genuinely
//! live by the registry's own liveness rule.

use crate::instance_registry::{probe_serve_owner, InstanceRegistryEntry};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn mkdir(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).expect("mkdir");
    canonical(path)
}

/// A live serve ReadWrite owner in `registry` whose bound graph declares
/// `declared_roots`, persisted exactly where the owner persists them.
fn write_serve_owner(
    registry: &Path,
    id: &str,
    runtime_root: &Path,
    port: u16,
    declared_roots: &[&Path],
) -> PathBuf {
    std::fs::create_dir_all(runtime_root).expect("owner runtime dir");
    let runtime_root = canonical(runtime_root);
    let roots: Vec<String> = declared_roots
        .iter()
        .map(|root| canonical(root).to_string_lossy().into_owned())
        .collect();
    std::fs::write(
        runtime_root.join("ingest_roots.json"),
        serde_json::to_string_pretty(&roots).expect("serialize ingest roots"),
    )
    .expect("write ingest_roots.json");

    let entry = InstanceRegistryEntry {
        instance_id: id.to_string(),
        workspace_root: runtime_root.to_string_lossy().into_owned(),
        runtime_root: runtime_root.to_string_lossy().into_owned(),
        graph_source: runtime_root
            .join("graph_snapshot.json")
            .to_string_lossy()
            .into_owned(),
        plasticity_state: runtime_root
            .join("plasticity_state.json")
            .to_string_lossy()
            .into_owned(),
        pid: std::process::id(),
        bind: Some("127.0.0.1".to_string()),
        port: Some(port),
        started_at_ms: crate::util::now_ms(),
        last_heartbeat_ms: crate::util::now_ms(),
        mode: "read_write".to_string(),
        status: "running".to_string(),
        owner_live: Some(true),
        stale: false,
        conflicts: Vec::new(),
        brain_kind: None,
    };
    let dir = registry.join("instances");
    std::fs::create_dir_all(&dir).expect("instances dir");
    std::fs::write(
        dir.join(format!("{}.json", entry.instance_id)),
        serde_json::to_string_pretty(&entry).expect("serialize entry"),
    )
    .expect("write instance entry");
    runtime_root
}

// ---------------------------------------------------------------------------
// 1. The letter's exact reproduction.
// ---------------------------------------------------------------------------

/// The measured defect, minimized: the first-minute client's own runtime root
/// (`<repo>/.m1nd`) has no owner and an empty graph, and the served owner
/// elsewhere on the machine declares `<repo>` among its ingest roots. The probe
/// must answer FOUND — that answer is the whole difference between attaching to
/// the real graph and booting a blind sidecar that reports `needs_authority`.
#[test]
fn the_probe_finds_the_served_owner_that_declared_this_repo() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let client_runtime = mkdir(&repo.join(".m1nd"));

    let owner_runtime = write_serve_owner(
        &registry,
        "inst_served_owner",
        &tmp.path().join("owner-runtime"),
        1338,
        &[&repo],
    );

    let probe = probe_serve_owner(&client_runtime, Some(&repo), Some(&registry));

    assert!(
        probe.found,
        "a live serve owner declaring this repo must be found: {:?}",
        probe.reason
    );
    assert_eq!(probe.base_url.as_deref(), Some("http://127.0.0.1:1338"));
    assert_eq!(probe.discovery.as_deref(), Some("ingest_coverage"));
    assert_eq!(
        probe.owner_runtime_root.as_deref(),
        Some(owner_runtime.to_string_lossy().as_ref()),
        "the owner's OWN runtime root is where its bearer token lives"
    );
    assert_eq!(
        probe.declared_root.as_deref(),
        Some(repo.to_string_lossy().as_ref())
    );
    assert_eq!(probe.reason, None, "a found owner carries no refusal");
}

/// The probe is a projection, never a second discovery: the FIRST question still
/// wins when it can be answered, and it is labelled as itself.
#[test]
fn an_owner_of_the_clients_own_runtime_root_is_reported_as_the_first_question() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));

    let shared_runtime = write_serve_owner(
        &registry,
        "inst_same_runtime",
        &tmp.path().join("runtime"),
        1337,
        &[&repo],
    );

    let probe = probe_serve_owner(&shared_runtime, Some(&repo), Some(&registry));

    assert!(probe.found, "reason: {:?}", probe.reason);
    assert_eq!(probe.discovery.as_deref(), Some("runtime_root"));
    assert_eq!(probe.base_url.as_deref(), Some("http://127.0.0.1:1337"));
    assert_eq!(
        probe.declared_root, None,
        "the runtime-root question has no covering root to report"
    );
}

// ---------------------------------------------------------------------------
// 2. The honest refusal — the isolated path must be able to say WHY.
// ---------------------------------------------------------------------------

/// No owner on either question: the probe answers NOT found and hands back the
/// discovery's own two-fact refusal verbatim, so the isolated boot the CLI then
/// performs can state why it is isolated instead of telling an agent on a
/// perfectly healthy machine to "connect an authenticated owner".
#[test]
fn no_covering_owner_answers_not_found_with_the_two_fact_reason() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let stranger = mkdir(&tmp.path().join("stranger"));
    let client_runtime = mkdir(&repo.join(".m1nd"));

    write_serve_owner(
        &registry,
        "inst_unrelated",
        &tmp.path().join("owner-runtime"),
        1338,
        &[&stranger],
    );

    let probe = probe_serve_owner(&client_runtime, Some(&repo), Some(&registry));

    assert!(!probe.found);
    assert_eq!(probe.base_url, None);
    assert_eq!(probe.discovery, None);
    let reason = probe.reason.expect("a refusal must carry its reason");
    assert!(
        reason.contains(&client_runtime.to_string_lossy().into_owned()),
        "fact 1 — the runtime root with no owner — must be named: {reason}"
    );
    assert!(
        reason.contains(&repo.to_string_lossy().into_owned()),
        "fact 2 — the caller root no live owner ingests — must be named: {reason}"
    );
    assert!(
        reason.to_lowercase().contains("ingest"),
        "the refusal must name the second question in words: {reason}"
    );
}

/// Ambiguity fails CLOSED, and the refusal names every candidate. Two live
/// owners covering one repo is a configuration question only the owner can
/// settle; a silent pick would send a whole first-minute session to the wrong
/// brain. The probe must not soften that into a guess.
#[test]
fn two_covering_owners_refuse_and_name_both_candidates() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("workspace").join("repo"));
    let workspace = canonical(&tmp.path().join("workspace"));
    let client_runtime = mkdir(&tmp.path().join("client-runtime"));

    write_serve_owner(
        &registry,
        "inst_exact",
        &tmp.path().join("owner-a"),
        1338,
        &[&repo],
    );
    write_serve_owner(
        &registry,
        "inst_ancestor",
        &tmp.path().join("owner-b"),
        1339,
        &[&workspace],
    );

    let probe = probe_serve_owner(&client_runtime, Some(&repo), Some(&registry));

    assert!(!probe.found, "ambiguity is never an answer");
    assert_eq!(probe.base_url, None);
    let reason = probe.reason.expect("a refusal must carry its reason");
    assert!(
        reason.contains("http://127.0.0.1:1338") && reason.contains("http://127.0.0.1:1339"),
        "the refusal must name BOTH candidates so the owner can resolve it: {reason}"
    );
}

// ---------------------------------------------------------------------------
// 3. The wire shape a non-Rust caller reads.
// ---------------------------------------------------------------------------

/// The npm agent CLI parses this payload to decide how to boot, so its JSON
/// shape is a contract: a stable schema, a boolean the caller can branch on, and
/// the caller root the answer is about. A payload that omits `found` or renames
/// the schema silently returns the CLI to the blind sidecar.
#[test]
fn the_probe_payload_is_json_a_non_rust_caller_can_branch_on() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let client_runtime = mkdir(&repo.join(".m1nd"));

    write_serve_owner(
        &registry,
        "inst_served_owner",
        &tmp.path().join("owner-runtime"),
        1338,
        &[&repo],
    );

    let probe = probe_serve_owner(&client_runtime, Some(&repo), Some(&registry));
    let value = serde_json::to_value(&probe).expect("the probe payload must serialize");

    assert_eq!(
        value.get("schema").and_then(|v| v.as_str()),
        Some("m1nd-owner-discovery-v0")
    );
    assert_eq!(value.get("found").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        value.get("base_url").and_then(|v| v.as_str()),
        Some("http://127.0.0.1:1338")
    );
    assert_eq!(
        value.get("caller_root").and_then(|v| v.as_str()),
        Some(repo.to_string_lossy().as_ref()),
        "the caller root the answer is about must be readable back"
    );
}
