//! `--attach auto` — the SECOND discovery question.
//!
//! Born RED from a measured field defect (project mailbox letter
//! `opus5-guardian`, 2026-07-30, severity high, reproduced across three
//! independent sessions): **the product does not reach its own brain in its own
//! repo.** An agent working in `<repo>` gets a stdio owner over `<repo>/.m1nd`,
//! which is EMPTY, while the machine's served owner holds the full graph and
//! ALREADY declares `<repo>` among its ingest roots. `--attach auto` cannot
//! bridge them, because it only ever asked ONE question:
//!
//!   1. "is there a live serve owner for MY runtime_root?"
//!
//! and never the one that matters to an agent sitting in a repo:
//!
//!   2. "is there a live serve owner that has INGESTED my repo?"
//!
//! This file is that second question's acceptance battery, written before the
//! second pass existed. Against the pre-fix binary every case that exercises
//! question 2 FAILS with the letter's own message verbatim ("no live serve
//! ReadWrite owner for runtime_root <…>"); `runtime_root_match_still_wins…` is a
//! pin, not a demand — it is green before and after, and its job is to prove the
//! fallback never became a replacement.
//!
//! No assertion here was weakened after the implementation landed. One fixture
//! bug was fixed during the green run and is recorded rather than hidden: the
//! token fixture wrote a `0644` credential, which `read_existing_bearer_token`
//! correctly refuses (`InsecurePermissions`), so `write_token_file` now writes
//! `0600` exactly as the owner does. The assertions it feeds are unchanged.
//!
//! Fixtures are in-process: a tempdir registry with hand-written
//! `instances/*.json` entries (the exact records `list_instances` reads) and a
//! real `ingest_roots.json` beside each owner's graph — the same file
//! `SessionState::persist_ingest_roots` writes and `load_ingest_roots` reads. No
//! subprocess, no lease, no port bound. `pid` is this test process, so every
//! fixture owner is genuinely live by the registry's own liveness rule.

use crate::instance_registry::{discover_serve_owner, InstanceRegistryEntry, OwnerDiscovery};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

/// Canonical spelling, so a fixture path compares equal to what the owner would
/// have stored (macOS `/var` → `/private/var`, symlinks resolved).
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// A live serve ReadWrite owner in `registry`, whose bound graph declares
/// `declared_roots` (persisted exactly where the owner persists them: an
/// `ingest_roots.json` beside the graph snapshot).
///
/// Returns the owner's canonical runtime root.
fn write_serve_owner(
    registry: &Path,
    id: &str,
    runtime_root: &Path,
    workspace_root: &Path,
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
        workspace_root: canonical(workspace_root).to_string_lossy().into_owned(),
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
    write_entry(registry, &entry);
    runtime_root
}

fn write_entry(registry: &Path, entry: &InstanceRegistryEntry) {
    let dir = registry.join("instances");
    std::fs::create_dir_all(&dir).expect("instances dir");
    std::fs::write(
        dir.join(format!("{}.json", entry.instance_id)),
        serde_json::to_string_pretty(entry).expect("serialize entry"),
    )
    .expect("write instance entry");
}

fn read_entry(registry: &Path, id: &str) -> InstanceRegistryEntry {
    let raw = std::fs::read_to_string(registry.join("instances").join(format!("{id}.json")))
        .expect("read instance entry");
    serde_json::from_str(&raw).expect("parse instance entry")
}

/// A directory that exists on disk (coverage refuses to compare paths that do
/// not resolve, so every fixture root is real).
fn mkdir(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).expect("mkdir");
    canonical(path)
}

/// An owner bearer-token file exactly as the owner writes one: owner-only `0600`
/// on unix, since `read_existing_bearer_token` refuses a group/world-readable
/// credential (`HttpSecurityError::InsecurePermissions`).
#[cfg(feature = "serve")]
fn write_token_file(path: &Path, token: &str) {
    std::fs::write(path, token).expect("write token file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("chmod 0600 token file");
    }
}

// ---------------------------------------------------------------------------
// 1. The letter's exact reproduction.
// ---------------------------------------------------------------------------

/// The measured defect, minimized: an agent in `<repo>` whose own runtime root
/// (`<repo>/.m1nd`) has NO owner, and a served owner elsewhere whose declared
/// ingest roots include `<repo>`. Before the second question this refused with
/// "no live serve ReadWrite owner for runtime_root <repo>/.m1nd" while the owner
/// that holds the repo's graph was alive one port away.
#[test]
fn attach_auto_finds_the_live_owner_that_declared_this_repo() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let client_runtime = mkdir(&repo.join(".m1nd"));

    let owner_runtime = write_serve_owner(
        &registry,
        "inst_served_owner",
        &tmp.path().join("owner-runtime"),
        &tmp.path().join("owner-runtime"),
        1338,
        &[&repo],
    );

    let found = discover_serve_owner(&client_runtime, Some(&repo), Some(&registry))
        .expect("the owner that ingested this repo must be discoverable");

    assert_eq!(found.base_url, "http://127.0.0.1:1338");
    assert_eq!(found.runtime_root, owner_runtime);
    match &found.discovery {
        OwnerDiscovery::IngestCoverage {
            declared_root,
            caller_root,
        } => {
            assert_eq!(Path::new(declared_root), repo.as_path());
            assert_eq!(Path::new(caller_root), repo.as_path());
        }
        other => panic!("expected ingest-coverage discovery, got {other:?}"),
    }
}

/// The monorepo shape: the caller sits INSIDE a declared root. This is
/// `covers_root`'s question ("may this brain legitimately serve that caller?"),
/// and its answer is yes — the subpackage's files were ingested with the repo.
#[test]
fn a_caller_inside_a_declared_root_is_covered() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let package = mkdir(&repo.join("packages").join("ui"));
    let client_runtime = mkdir(&tmp.path().join("client-runtime"));

    write_serve_owner(
        &registry,
        "inst_owner",
        &tmp.path().join("owner-runtime"),
        &tmp.path().join("owner-runtime"),
        1337,
        &[&repo],
    );

    let found = discover_serve_owner(&client_runtime, Some(&package), Some(&registry))
        .expect("a caller under a declared root is covered by that root's owner");
    assert_eq!(found.base_url, "http://127.0.0.1:1337");
}

// ---------------------------------------------------------------------------
// 2. The fallback is a fallback.
// ---------------------------------------------------------------------------

/// The first question still wins when it can be answered. The second pass is an
/// added question, never a replacement: an owner that holds the client's OWN
/// runtime root is the client's owner even when another owner also covers the
/// caller's repo.
#[test]
fn runtime_root_match_still_wins_over_ingest_coverage() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let client_runtime = mkdir(&repo.join(".m1nd"));

    // The client's OWN runtime root has a live serve owner (port 1400)…
    write_serve_owner(
        &registry,
        "inst_own_runtime",
        &client_runtime,
        &repo,
        1400,
        &[],
    );
    // …and a second owner elsewhere also declares this repo (port 1338).
    write_serve_owner(
        &registry,
        "inst_covering",
        &tmp.path().join("owner-runtime"),
        &tmp.path().join("owner-runtime"),
        1338,
        &[&repo],
    );

    let found = discover_serve_owner(&client_runtime, Some(&repo), Some(&registry))
        .expect("the runtime-root owner answers");
    assert_eq!(found.base_url, "http://127.0.0.1:1400");
    assert_eq!(found.discovery, OwnerDiscovery::RuntimeRoot);
}

// ---------------------------------------------------------------------------
// 3. Ambiguity is the owner's to resolve.
// ---------------------------------------------------------------------------

/// Two live owners both covering the caller root: refuse, and NAME BOTH. The
/// registry's freshest-first order must not silently break the tie — a wrong
/// silent pick sends an agent's whole session to the wrong brain.
#[test]
fn two_owners_covering_the_caller_root_refuse_and_name_both() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("workspace").join("repo"));
    let workspace = canonical(&tmp.path().join("workspace"));
    let client_runtime = mkdir(&tmp.path().join("client-runtime"));

    write_serve_owner(
        &registry,
        "inst_exact",
        &tmp.path().join("owner-a"),
        &tmp.path().join("owner-a"),
        1338,
        &[&repo],
    );
    write_serve_owner(
        &registry,
        "inst_ancestor",
        &tmp.path().join("owner-b"),
        &tmp.path().join("owner-b"),
        1339,
        &[&workspace],
    );

    let error = discover_serve_owner(&client_runtime, Some(&repo), Some(&registry))
        .expect_err("two covering owners must refuse, never guess");
    assert!(
        error.contains("http://127.0.0.1:1338") && error.contains("http://127.0.0.1:1339"),
        "the refusal must name BOTH candidates so the owner can resolve it: {error}"
    );
}

// ---------------------------------------------------------------------------
// 4. The honest two-fact refusal.
// ---------------------------------------------------------------------------

/// When neither question can be answered, the refusal teaches BOTH facts — the
/// runtime root that has no owner AND the caller root no live owner ingests —
/// plus the real next step. A refusal that names only the first fact is what
/// sent three sessions looking for the wrong repair.
#[test]
fn neither_question_answered_refuses_with_both_facts() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let stranger = mkdir(&tmp.path().join("stranger"));
    let client_runtime = mkdir(&repo.join(".m1nd"));

    // A live serve owner exists, but it neither holds this runtime root nor
    // declares this repo.
    write_serve_owner(
        &registry,
        "inst_unrelated",
        &tmp.path().join("owner-runtime"),
        &tmp.path().join("owner-runtime"),
        1338,
        &[&stranger],
    );

    let error = discover_serve_owner(&client_runtime, Some(&repo), Some(&registry))
        .expect_err("no owner on either question");
    assert!(
        error.contains(&client_runtime.to_string_lossy().into_owned()),
        "fact 1 — the runtime root with no owner — must be named: {error}"
    );
    assert!(
        error.contains(&repo.to_string_lossy().into_owned()),
        "fact 2 — the caller root no live owner ingests — must be named: {error}"
    );
    // The caller root is a PREFIX of `<repo>/.m1nd`, so naming fact 1 alone
    // would satisfy the assert above by accident. The refusal must also say
    // WHICH question failed second — in words, not by substring luck.
    assert!(
        error.to_lowercase().contains("ingest"),
        "the refusal must name the second question (declared ingest roots): {error}"
    );
    assert!(
        error.contains("--serve"),
        "the refusal must teach the real next step: {error}"
    );
}

// ---------------------------------------------------------------------------
// 5. Token resolution for a FOREIGN-runtime owner.
// ---------------------------------------------------------------------------

/// The second pass attaches to an owner whose runtime root is NOT the client's,
/// so the client-runtime-root token fallback would read the WRONG file (or none
/// at all). Discovery therefore hands back THE OWNER's runtime root, and the
/// bearer token resolves from there. Nothing is derived from a hardcoded path.
#[cfg(feature = "serve")]
#[test]
fn the_discovered_owners_own_runtime_root_carries_the_bearer_token() {
    use crate::http_security::{read_existing_bearer_token, HTTP_AUTH_TOKEN_FILE_NAME};

    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let client_runtime = mkdir(&repo.join(".m1nd"));

    let owner_runtime = write_serve_owner(
        &registry,
        "inst_served_owner",
        &tmp.path().join("owner-runtime"),
        &tmp.path().join("owner-runtime"),
        1338,
        &[&repo],
    );

    // Two token files exist: the client's stale one and the owner's real one.
    // Only the owner's authenticates against the owner.
    let client_token = "c".repeat(64);
    let owner_token = "a1b2c3d4".repeat(8);
    write_token_file(
        &client_runtime.join(HTTP_AUTH_TOKEN_FILE_NAME),
        &client_token,
    );
    write_token_file(&owner_runtime.join(HTTP_AUTH_TOKEN_FILE_NAME), &owner_token);

    let found =
        discover_serve_owner(&client_runtime, Some(&repo), Some(&registry)).expect("discovery");
    let resolved = read_existing_bearer_token(&found.runtime_root.join(HTTP_AUTH_TOKEN_FILE_NAME))
        .expect("the discovered owner's token must be readable from its own runtime root");

    assert_eq!(resolved, owner_token);
    assert_ne!(
        resolved, client_token,
        "the client's own runtime-root token is the WRONG credential for a foreign owner"
    );
}

// ---------------------------------------------------------------------------
// 6. Path identity — never a string prefix; a worktree is its main repo.
// ---------------------------------------------------------------------------

/// Two pins in one, because they are the same mistake seen from both sides.
///
/// (a) A git WORKTREE of an ingested repo discovers that repo's owner. The house
///     already ruled a worktree belongs to its main repository — `detect_root_overlap`
///     refuses to mint it a second brain (`RootOverlap::Worktree`) — so discovery
///     resolves the caller through the same `worktree_main_repo` rule.
///
/// (b) A plain SIBLING directory whose name merely starts with the repo's name
///     (`<repo>-scratch` next to `<repo>`) is NOT covered. A raw string prefix
///     would match it; canonical path identity must not. This is the Windows
///     phase-2 jurisprudence in miniature.
#[test]
fn a_worktree_resolves_to_its_main_repo_and_a_name_prefixed_sibling_never_matches() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let client_runtime = mkdir(&tmp.path().join("client-runtime"));

    write_serve_owner(
        &registry,
        "inst_owner",
        &tmp.path().join("owner-runtime"),
        &tmp.path().join("owner-runtime"),
        1338,
        &[&repo],
    );

    // (a) A real worktree layout: `<repo>/.git/worktrees/<name>` + a `.git` FILE
    // in the worktree pointing at it (exactly what `git worktree add` writes).
    let worktree = mkdir(&tmp.path().join("repo-wt-feature"));
    let gitdir = repo.join(".git").join("worktrees").join("feature");
    std::fs::create_dir_all(&gitdir).expect("worktree gitdir");
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", gitdir.display()),
    )
    .expect("worktree .git file");

    let found = discover_serve_owner(&client_runtime, Some(&worktree), Some(&registry))
        .expect("a worktree of an ingested repo reaches that repo's owner");
    assert_eq!(found.base_url, "http://127.0.0.1:1338");

    // (b) A sibling that is NOT a worktree and only shares a name prefix.
    let sibling = mkdir(&tmp.path().join("repo-scratch"));
    let error = discover_serve_owner(&client_runtime, Some(&sibling), Some(&registry))
        .expect_err("a name-prefixed sibling is a different repo, never coverage");
    assert!(
        error.contains(&sibling.to_string_lossy().into_owned()),
        "the refusal names the caller root it could not place: {error}"
    );
}

// ---------------------------------------------------------------------------
// 7. The second pass inherits the first pass's gates.
// ---------------------------------------------------------------------------

/// A dead owner, a stale one, a read-only one, and a stdio-only one (no port)
/// are never second-pass candidates — the same four gates pass 1 applies. Each
/// declares the caller's repo, so only the gate can reject them.
///
/// A fifth, HEALTHY owner declares the same repo and must be the single answer.
/// Without that positive control this case would pass on a binary that never
/// asks the second question at all — an assert that cannot fail proves nothing.
#[test]
fn dead_stale_read_only_and_portless_owners_are_never_candidates() {
    let tmp = tempdir().expect("tempdir");
    let registry = tmp.path().join("registry");
    let repo = mkdir(&tmp.path().join("repo"));
    let client_runtime = mkdir(&tmp.path().join("client-runtime"));

    for (id, port) in [
        ("inst_dead", 1401u16),
        ("inst_stale", 1402),
        ("inst_read_only", 1403),
        ("inst_portless", 1404),
    ] {
        write_serve_owner(
            &registry,
            id,
            &tmp.path().join(format!("owner-{id}")),
            &tmp.path().join(format!("owner-{id}")),
            port,
            &[&repo],
        );
    }

    let mut dead = read_entry(&registry, "inst_dead");
    dead.pid = u32::MAX - 1; // no such process
    write_entry(&registry, &dead);

    let mut stale = read_entry(&registry, "inst_stale");
    stale.last_heartbeat_ms = 0; // heartbeat far outside the staleness window
    write_entry(&registry, &stale);

    let mut read_only = read_entry(&registry, "inst_read_only");
    read_only.mode = "read_only".to_string();
    write_entry(&registry, &read_only);

    let mut portless = read_entry(&registry, "inst_portless");
    portless.port = None; // stdio-only owner: unreachable by construction
    portless.bind = None;
    write_entry(&registry, &portless);

    // The positive control: one healthy owner declaring the same repo.
    write_serve_owner(
        &registry,
        "inst_healthy",
        &tmp.path().join("owner-healthy"),
        &tmp.path().join("owner-healthy"),
        1338,
        &[&repo],
    );

    let found = discover_serve_owner(&client_runtime, Some(&repo), Some(&registry))
        .expect("exactly one owner passes the gates, so this is not ambiguous — it is the answer");
    assert_eq!(
        found.base_url, "http://127.0.0.1:1338",
        "only the healthy owner may be reached"
    );
}
