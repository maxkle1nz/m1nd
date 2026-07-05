//! ORGANISM ladder R16 · the SOUL (SOUL-PRD S0/S1) — behavioral battery.
//!
//! RED before R16: no soul surface existed at all — grep-proven on `main`. A
//! project's PATHOS handoff had NO verification state per claim, NO freshness
//! receipt, and the two tissues were interleaved unmarked. A cold session could
//! not KNOW how much to trust the handoff. This file is the GREEN that could not
//! exist before the verbs.
//!
//! THE LAWS UNDER TEST (SOUL-PRD, ORGANISM §C8.6):
//!   - `soul_check` parses PATHOS into anchored claims, classifies + verifies each,
//!     and emits the honesty report + one-line FRESHNESS RECEIPT (N fresh / M stale
//!     / K receipt-priced).
//!   - the TWO TISSUES hold: a declared-unprovable claim (doctrine/taste) is NEVER
//!     reported fresh and never machine-verified (SOUL-INV-5).
//!   - fake-fresh is impossible: a missing anchor is `evidence_stale`, never fresh
//!     (SOUL-INV-3); an UNANCHORED verifiable claim is a named finding (SOUL-INV-1).
//!   - the consistency pass catches an intra-soul contradiction (two disagreeing
//!     numbers) without touching the filesystem.
//!   - the §C8.4 seat check: a curator report must pass by a DIFFERENT agent than
//!     the one that curated it (grader ≠ author); a silent prune is refused.
//!
//! Neutral fixtures only (NO other-project names, NO personal paths).

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("m1nd_soul_{tag}_{nanos}_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn git(root: &Path, args: &[&str]) {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {:?}: {}", args, e));
}

/// Build a neutral git-tracked "project" with a real code file (so a symbol
/// anchor resolves), then ingest it so the graph is populated. Returns the repo
/// root. The soul document is written by the caller so each test controls it.
fn build_repo(tag: &str) -> (PathBuf, SessionState) {
    let repo = unique_dir(tag);
    write(
        &repo.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    );
    // A real symbol the `symbol` check class can resolve: `widget::render`.
    write(
        &repo.join("src/widget.rs"),
        "pub fn render() -> i64 {\n    42\n}\n",
    );
    write(&repo.join("scripts/gate.py"), "print('gate')\n");
    write(&repo.join(".keepfile"), "keep\n");

    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["add", "."]);
    git(
        &repo,
        &["commit", "-m", "chore: seed the fixture project (#101)"],
    );
    git(&repo, &["tag", "v1.0.0"]);

    let mut state = build_state(&unique_dir(&format!("{tag}_rt")));
    // Ingest the repo so the graph carries `widget::render` for symbol checks.
    call(
        &mut state,
        "ingest",
        json!({
            "agent_id": "tester",
            "path": repo.to_string_lossy().to_string(),
            "adapter": "code",
            "mode": "replace",
        }),
    );
    // Point the session's roots at the repo so soul discovery + git verification
    // resolve there (mirrors resolve_git_root_from_state's candidate order).
    state.ingest_roots = vec![repo.to_string_lossy().to_string()];
    state.workspace_root = Some(repo.to_string_lossy().to_string());
    (repo, state)
}

/// A soul with a KNOWN mix: fresh path/symbol/git anchors, a missing-file anchor,
/// an intra-soul contradiction, and declared (doctrine) tissue.
const SEED_SOUL: &str = r#"# PROJECT HANDOFF — the soul under test

> Read this first.

## North Star
The bar: genuinely BEAT the baseline, measured honestly. Never sugarcoat.

## Current State
- Battery harness: `scripts/gate.py` — TRACKED in-repo.
- The renderer lives at `widget::render`.
- Released as `v1.0.0`, landed in `#101`.
- The old probe `scripts/ghost_probe.py` runs the smoke check.
- Battery at 36 cases, green.

## Proof Standard
- Battery at 37 cases before merge.

## Operating Doctrine
Proof-grown: measure before claiming. Commit and push always.

## Do Not Do
- Don't bypass the gate.
"#;

/// PROBE (ignored by default — run with `--ignored`): soul_check against the REAL
/// `docs/PATHOS.md` at the current worktree HEAD, the SOUL-PRD's seed battery case.
/// Prints the honest cp10 numbers. Not a CI assertion (the live soul drifts by
/// design); it is the RED-maker turning today's drift into a measured number.
#[test]
#[ignore = "probe: run against the live PATHOS with --ignored"]
fn probe_real_pathos_reports_the_honest_numbers() {
    // Resolve the repo root from CARGO_MANIFEST_DIR/.. (the m1nd worktree).
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo = Path::new(manifest).parent().unwrap().to_path_buf();
    let soul = repo.join("docs/PATHOS.md");
    assert!(soul.exists(), "no soul at {:?}", soul);

    let mut state = build_state(&unique_dir("real_probe_rt"));
    state.ingest_roots = vec![repo.to_string_lossy().to_string()];
    state.workspace_root = Some(repo.to_string_lossy().to_string());

    let out = call(
        &mut state,
        "soul_check",
        json!({ "agent_id": "probe", "soul_path": "docs/PATHOS.md" }),
    );
    println!("\n===== SOUL_CHECK vs LIVE docs/PATHOS.md =====");
    println!("receipt: {}", out["receipt_line"].as_str().unwrap());
    println!("by_state: {}", out["by_state"]);
    println!("claims: {}", out["claims"]);
    println!("soul_lag: {}", out["soul_lag"]);
    println!("checks_skipped: {}", out["checks_skipped"]);
    println!("stale rows ({}):", out["stale"].as_array().unwrap().len());
    for row in out["stale"].as_array().unwrap() {
        println!(
            "  [{}] {} — {}",
            row["reason"].as_str().unwrap_or("?"),
            row["anchor"]
                .as_str()
                .unwrap_or(row["quantity"].as_str().unwrap_or("?")),
            row["claim"]
                .as_str()
                .unwrap_or(row["detail"].as_str().unwrap_or("")),
        );
    }
    println!("=============================================\n");
}

/// GREEN — soul_check parses, classifies, verifies, and emits the receipt with
/// the two-tissue split. Fresh anchors are fresh; the missing file is stale; the
/// declared tissue is counted, never verified.
#[test]
fn soul_check_reports_fresh_stale_and_declared_with_a_receipt() {
    let (repo, mut state) = build_repo("mix");
    write(&repo.join("docs/PATHOS.md"), SEED_SOUL);

    let out = call(
        &mut state,
        "soul_check",
        json!({ "agent_id": "grader", "soul_path": "docs/PATHOS.md" }),
    );

    assert_eq!(out["schema"], "m1nd-soul-check-v0", "schema stamp");
    let by = &out["by_state"];

    // Fresh anchors: the tracked script path, the resolved symbol, the real tag,
    // the merged PR ref → at least those hold fresh.
    let fresh = by["verified_fresh"].as_u64().unwrap();
    assert!(
        fresh >= 3,
        "expected the fresh path/symbol/git anchors to verify, got {fresh}; out={out}"
    );

    // The missing file `scripts/ghost_probe.py` MUST be stale (evidence_file_missing).
    let stale_rows = out["stale"].as_array().unwrap();
    assert!(
        stale_rows
            .iter()
            .any(|r| r["reason"] == "evidence_file_missing"
                && r["anchor"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ghost_probe.py")),
        "the missing probe file must be reported evidence_file_missing; stale={stale_rows:?}"
    );

    // Declared tissue (North Star / Doctrine / Do Not Do) is counted, never verified.
    assert!(
        by["declared"].as_u64().unwrap() >= 1,
        "declared tissue must be counted; out={out}"
    );

    // The freshness receipt line exists and carries counts + a date + @sha.
    let receipt = out["receipt_line"].as_str().unwrap();
    assert!(
        receipt.starts_with("soul: checked "),
        "receipt shape: {receipt}"
    );
    assert!(
        receipt.contains("fresh"),
        "receipt names fresh count: {receipt}"
    );
    assert!(receipt.contains(" @"), "receipt carries @sha: {receipt}");

    // soul_lag is present (the fields exist even if the count is null in a fixture).
    assert!(out["soul_lag"].is_object(), "soul_lag present: {out}");

    fs::remove_dir_all(&repo).ok();
}

/// SOUL-INV-3 / two tissues — a declared-tissue claim is NEVER reported fresh, and
/// a fake-fresh is impossible: doctoring the soul to reference a missing file
/// flips the claim to stale, never leaves it fresh.
#[test]
fn soul_check_never_fake_fresh_and_declared_is_never_verified() {
    let (repo, mut state) = build_repo("honest");
    // A soul whose only verifiable anchor is a MISSING file, plus pure doctrine.
    let doctored = r#"# HANDOFF

## Current State
- The engine lives at `src/missing_engine.rs`.

## Operating Doctrine
Never sugarcoat. Verify before asserting.
"#;
    write(&repo.join("docs/PATHOS.md"), doctored);

    let out = call(&mut state, "soul_check", json!({ "agent_id": "grader" }));

    let by = &out["by_state"];
    // No fresh claim can exist over a missing anchor + declared-only doctrine.
    assert_eq!(
        by["verified_fresh"].as_u64().unwrap(),
        0,
        "a missing anchor + declared doctrine must yield ZERO fresh; out={out}"
    );
    assert!(
        by["evidence_stale"].as_u64().unwrap() >= 1,
        "the missing engine file must be stale; out={out}"
    );
    // Declared doctrine counted, but the receipt says declared tissue intact —
    // never folded into fresh.
    assert!(
        by["declared"].as_u64().unwrap() >= 1,
        "doctrine is declared tissue; out={out}"
    );
    let receipt = out["receipt_line"].as_str().unwrap();
    assert!(
        receipt.contains("0 fresh"),
        "the receipt must not overclaim freshness: {receipt}"
    );

    fs::remove_dir_all(&repo).ok();
}

/// The consistency pass catches an intra-soul contradiction (battery 36 vs 37)
/// with no filesystem access — reason `contradicted`.
#[test]
fn soul_check_consistency_pass_catches_intra_soul_contradiction() {
    let (repo, mut state) = build_repo("consistency");
    write(&repo.join("docs/PATHOS.md"), SEED_SOUL); // carries "36" and "37"

    let out = call(&mut state, "soul_check", json!({ "agent_id": "grader" }));

    let findings = out["consistency_findings"].as_array().unwrap();
    assert!(
        findings.iter().any(|f| f["reason"] == "contradicted"
            && f["quantity"] == "battery"),
        "the soul asserts battery as both 36 and 37 — must be flagged contradicted; findings={findings:?}, out={out}"
    );

    fs::remove_dir_all(&repo).ok();
}

/// soul_read pulls the body (whole + section) and the authored headline; it never
/// fabricates a receipt (that is soul_check's job).
#[test]
fn soul_read_pulls_body_and_section_and_headline() {
    let (repo, mut state) = build_repo("read");
    write(&repo.join("docs/PATHOS.md"), SEED_SOUL);

    let whole = call(&mut state, "soul_read", json!({ "agent_id": "reader" }));
    assert_eq!(whole["schema"], "m1nd-soul-read-v0");
    assert!(
        whole["content"].as_str().unwrap().contains("North Star"),
        "whole read carries the document"
    );
    assert_eq!(
        whole["headline"], "PROJECT HANDOFF — the soul under test",
        "headline is the soul's first authored title"
    );

    let section = call(
        &mut state,
        "soul_read",
        json!({ "agent_id": "reader", "section": "current state" }),
    );
    let body = section["content"].as_str().unwrap();
    assert!(
        body.contains("widget::render"),
        "section read is scoped: {body}"
    );
    assert!(
        !body.contains("Do Not Do"),
        "section read stops at the next section: {body}"
    );

    fs::remove_dir_all(&repo).ok();
}

/// §C8.4 — who verifies the curator. A curator report must pass by a DIFFERENT
/// agent than the one that curated it (grader ≠ author), account every prune, and
/// carry the still_stale honesty valve. The seat is checked mechanically.
#[test]
fn curator_report_seatcheck_requires_grader_not_author_and_no_silent_prune() {
    let (repo, mut state) = build_repo("seat");
    write(&repo.join("docs/PATHOS.md"), SEED_SOUL);

    // A well-formed report curated by "curator-a", graded by a DIFFERENT agent.
    let good_report = json!({
        "curated_by": "host:sub:curator-a",
        "checked": 12,
        "updated": 3,
        "pruned": [
            {"what": "stale probe line", "why": "file gone", "where_it_went": "git history + archive tail"}
        ],
        "declined": [],
        "still_stale": [],
        "receipt_line": "soul: checked 2026-07-05 @abc123 — 10 fresh · 1 stale · 1 priced · declared intact"
    });

    let ok = call(
        &mut state,
        "soul_check",
        json!({
            "agent_id": "host:sub:curator-b",
            "verify_curator_report": good_report,
        }),
    );
    assert_eq!(ok["schema"], "m1nd-soul-curator-seatcheck-v0");
    assert_eq!(
        ok["passed"], true,
        "independent grader + clean report passes: {ok}"
    );
    assert_eq!(ok["seat_independent"], true);

    // The SAME agent grading its own curation → seat violation (the circularity).
    let self_graded = call(
        &mut state,
        "soul_check",
        json!({
            "agent_id": "host:sub:curator-a",
            "verify_curator_report": json!({
                "curated_by": "host:sub:curator-a",
                "pruned": [],
                "still_stale": []
            }),
        }),
    );
    assert_eq!(
        self_graded["passed"], false,
        "grader == author must fail: {self_graded}"
    );
    assert_eq!(self_graded["seat_independent"], false);

    // A SILENT prune (no `why`/`where_it_went`) → refused (SOUL-INV-2).
    let silent = call(
        &mut state,
        "soul_check",
        json!({
            "agent_id": "host:sub:curator-b",
            "verify_curator_report": json!({
                "curated_by": "host:sub:curator-a",
                "pruned": [{"what": "some line"}],
                "still_stale": []
            }),
        }),
    );
    assert_eq!(
        silent["passed"], false,
        "a silent prune must fail: {silent}"
    );
    let violations = silent["violations"].as_array().unwrap();
    assert!(
        violations
            .iter()
            .any(|v| v.as_str().unwrap().contains("silent")),
        "the silent-prune violation must be named: {violations:?}"
    );

    // A report MISSING the still_stale honesty valve → refused (SOUL-PRD §5.3).
    let no_valve = call(
        &mut state,
        "soul_check",
        json!({
            "agent_id": "host:sub:curator-b",
            "verify_curator_report": json!({
                "curated_by": "host:sub:curator-a",
                "pruned": []
            }),
        }),
    );
    assert_eq!(
        no_valve["passed"], false,
        "missing still_stale must fail: {no_valve}"
    );

    // Declared-tissue lock (SOUL-INV-5): a prune that removes declared tissue
    // without `proposed: true` → refused.
    let declared_prune = call(
        &mut state,
        "soul_check",
        json!({
            "agent_id": "host:sub:curator-b",
            "verify_curator_report": json!({
                "curated_by": "host:sub:curator-a",
                "pruned": [{
                    "what": "a doctrine line",
                    "why": "thought it obsolete",
                    "where_it_went": "deleted",
                    "tissue": "declared"
                }],
                "still_stale": []
            }),
        }),
    );
    assert_eq!(
        declared_prune["passed"], false,
        "removing declared tissue without proposed:true must fail (SOUL-INV-5): {declared_prune}"
    );

    fs::remove_dir_all(&repo).ok();
}

/// soul_update rides the ONE memorize sink with `Soul-Source` provenance
/// (SOUL-INV-8) — the write half. A claim registered with a soul_source lands in
/// the store carrying its `Soul-Source: <path>#<section>` frontmatter, subject to
/// the same gates as every memory.
#[test]
fn soul_update_writes_through_memorize_with_soul_source_provenance() {
    let (repo, mut state) = build_repo("update");

    let out = call(
        &mut state,
        "memorize",
        json!({
            "agent_id": "host:sub:curator-a",
            "node_label": "soul_current_state_renderer",
            "state": "verified",
            "claims": [{
                "label": "the renderer lives at widget::render and returns 42",
                "confidence": 0.9,
                "evidence": ["src/widget.rs"]
            }],
            "soul_source": "docs/PATHOS.md#Current State"
        }),
    );

    // The write succeeded and the sidecar carries the Soul-Source line.
    let path = out
        .get("path")
        .or_else(|| out.get("output_path"))
        .and_then(|v| v.as_str())
        .expect("memorize returns the written path");
    let written = fs::read_to_string(path).expect("read the sidecar");
    assert!(
        written.contains("Soul-Source: docs/PATHOS.md#Current State"),
        "the soul citizen must carry its Soul-Source provenance; wrote:\n{written}"
    );
    // It is a normal memory too — same door, same frontmatter contract.
    assert!(
        written.contains("Protocol: L1GHT"),
        "rides the ONE sink: {written}"
    );
    assert!(
        written.contains("Source-Agent: host:sub:curator-a"),
        "provenance stamped by the sink: {written}"
    );

    fs::remove_dir_all(&repo).ok();
}
