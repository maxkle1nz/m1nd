// === Hardening harness for the `transplant` verb ===
//
// The canonical contract lives in tests/transplant_battery.rs and the adversarial
// smoke cases in tests/transplant_stress.rs. THIS file is the hardening harness
// the PRD asked for: every KNOWN GAP from the proof-of-possibility wave is born
// here as a test that is RED against the pre-hardening verb and turns GREEN as the
// gap is closed. It also adds two reusable structural guarantees — the
// "nothing-else-changed" certificate (A2) and the round-trip property (A3) — plus
// the adversarial fixture pack (A1) and atomicity-under-failure (A6).
//
// Structural, NOT fragile-string-diff: the certificate compares per-file sets of
// top-level item signatures + use lines + untouched-item block bytes, exactly as
// the spec demands. Compiler oracle (cargo check) is used where the ultimate proof
// is "valid Rust", skipped honestly when cargo cannot spawn in the sandbox.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Shared infra (house style: mirrors transplant_battery.rs / transplant_stress.rs)
// ---------------------------------------------------------------------------

mod common;

fn make_state(root: &Path) -> SessionState {
    common::disable_proof_gate_for_logic_tests();
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

fn ingest(state: &mut SessionState, root: &Path) {
    let out = m1nd_mcp::tools::handle_ingest(
        state,
        m1nd_mcp::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "harness".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            adapter: "code".to_string(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            project_root: None,
        },
    )
    .expect("ingest");
    let nodes = out.get("node_count").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(nodes >= 3, "fixture graph must populate, got {nodes}");
}

fn cargo_toml() -> &'static str {
    "[package]\nname = \"fixture-transplant\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
}

fn params(root: &Path, symbol: &str, src: &str, dest: &str) -> serde_json::Value {
    serde_json::json!({
        "agent_id": "harness",
        "symbol": symbol,
        "source_file": root.join(src).to_string_lossy(),
        "dest_file": root.join(dest).to_string_lossy(),
    })
}

/// Run `cargo check` on the fixture crate with a target dir INSIDE the tempdir so it
/// never contends with the outer test's target. Ok(()) clean, Err(stderr) on a real
/// compile failure, Err("cargo-unavailable: …") when cargo cannot spawn (skip).
fn cargo_check(root: &Path) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest = root.join("Cargo.toml");
    let target = root.join("_check_target");
    let out = Command::new(cargo)
        .args(["check", "--quiet", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &target)
        .output();
    match out {
        Err(e) => Err(format!("cargo-unavailable: {e}")),
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
    }
}

/// Assert the crate compiles, or skip honestly when cargo is unavailable.
fn assert_compiles(root: &Path, ctx: &str) {
    match cargo_check(root) {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => eprintln!("SKIP {ctx} compile: {e}"),
        Err(e) => panic!("{ctx} crate must compile after the transplant:\n{e}"),
    }
}

// ---------------------------------------------------------------------------
// Structural extractors (top-level item signatures / use lines / item blocks).
// Column-0 discipline: real top-level Rust items begin at column 0 and their
// closing brace sits at column 0, so these extractors are immune to the very
// brace-in-string bug the verb is being hardened against (that `}` is mid-line).
// ---------------------------------------------------------------------------

fn item_re() -> Regex {
    Regex::new(
        r#"(?x)                       # verbose
        ^(?:pub(?:\([^)]*\))?\s+)?    # optional visibility
        (?:(?:async|unsafe|const|default|extern(?:\s+"[^"]*")?)\s+)*  # qualifiers
        (fn|struct|enum|trait|type|mod|static|union)\b               # item keyword
        "#,
    )
    .unwrap()
}

/// Set of top-level `fn` NAMES (the moved-symbol relocation is name-addressed).
fn top_level_fn_names(text: &str) -> BTreeSet<String> {
    let re = Regex::new(
        r#"^(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|unsafe|const|default)\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)"#,
    )
    .unwrap();
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        if let Some(c) = re.captures(line) {
            out.insert(c[1].to_string());
        }
    }
    out
}

/// Set of top-level `use`/`pub use` statements (trimmed, no trailing `;`).
fn use_lines(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let t = line.trim();
        if t.starts_with("use ") || t.starts_with("pub use ") {
            out.insert(t.trim_end_matches(';').trim().to_string());
        }
    }
    out
}

/// Map each top-level braced item signature -> its full block text (decl line down
/// to the column-0 closing brace). Used to prove UNTOUCHED items are byte-identical.
fn item_blocks(text: &str) -> BTreeMap<String, String> {
    let re = item_re();
    let lines: Vec<&str> = text.lines().collect();
    let mut map = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let col0 = !(line.starts_with(' ') || line.starts_with('\t') || line.is_empty());
        // A braced item: matches the item regex and is not a one-line `;` decl.
        if col0 && re.is_match(line) && !line.trim_end().ends_with(';') {
            let sig = line.trim_end().trim_end_matches('{').trim_end().to_string();
            // Walk to the column-0 closing brace.
            let mut j = i;
            let mut end = i;
            while j < lines.len() {
                if lines[j] == "}" {
                    end = j;
                    break;
                }
                j += 1;
            }
            let block = lines[i..=end].join("\n");
            map.insert(sig, block);
            i = end + 1;
            continue;
        }
        i += 1;
    }
    map
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap()
}

/// Snapshot every `.rs` file under `src/` as (relpath -> text).
fn snapshot_src(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let src = root.join("src");
    for entry in walk_rs(&src) {
        // These keys are matched against expectations that spell paths with
        // '/' (`src_rel: "src/alpha.rs"`), so they must arrive in that one
        // identity domain. On Windows `strip_prefix` hands back '\', no key
        // ever matches, and the certificate silently grades the source file of
        // a move as "unrelated" — asserting it must not change, when changing
        // is precisely its job.
        let rel = entry
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        out.insert(rel, std::fs::read_to_string(&entry).unwrap());
    }
    out
}

fn walk_rs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_rs(&p));
            } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// The "nothing-else-changed" certificate (A2).
// ---------------------------------------------------------------------------

/// Description of the ONLY structural changes a transplant is allowed to make.
struct Expected {
    symbol: String,
    /// Private deps that TRAVEL with the symbol (source loses them, dest gains).
    travelled: Vec<String>,
    /// Shared deps that STAY (allowed a visibility bump in the source).
    bumped: Vec<String>,
    src_rel: String,
    dst_rel: String,
    /// Referencer files whose `use`/qualified paths may be rewritten.
    referencers: Vec<String>,
}

/// Assert the diff between `before` and `after` consists ONLY of the expected
/// regions — implemented structurally over item-name sets, item blocks and use
/// lines (never a fragile string diff).
fn assert_nothing_else_changed(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
    exp: &Expected,
) {
    // (1) GLOBAL invariant: the multiset of top-level fn names across the whole
    // crate is unchanged — items only RELOCATE, none is created or destroyed.
    let names_before: BTreeMap<String, usize> = fn_name_multiset(before);
    let names_after: BTreeMap<String, usize> = fn_name_multiset(after);
    assert_eq!(
        names_before, names_after,
        "global top-level fn multiset must be invariant (items only relocate)"
    );

    let moved: BTreeSet<&str> = std::iter::once(exp.symbol.as_str())
        .chain(exp.travelled.iter().map(String::as_str))
        .collect();

    for (rel, before_text) in before {
        let after_text = after.get(rel).unwrap_or_else(|| {
            panic!("file {rel} vanished after transplant");
        });
        let fns_before = top_level_fn_names(before_text);
        let fns_after = top_level_fn_names(after_text);

        if *rel == exp.src_rel {
            // Source: loses exactly the moved set; every other fn name stays.
            for m in &moved {
                assert!(
                    fns_before.contains(*m) && !fns_after.contains(*m),
                    "source {rel} must LOSE `{m}`"
                );
            }
            let survivors: BTreeSet<String> = fns_before
                .difference(&moved.iter().map(|s| s.to_string()).collect())
                .cloned()
                .collect();
            assert_eq!(
                fns_after, survivors,
                "source {rel} must keep EXACTLY the non-moved fns"
            );
        } else if *rel == exp.dst_rel {
            // Dest: gains exactly the moved set; keeps all its residents.
            for m in &moved {
                assert!(
                    !fns_before.contains(*m) && fns_after.contains(*m),
                    "dest {rel} must GAIN `{m}`"
                );
            }
            let expected: BTreeSet<String> = fns_before
                .union(&moved.iter().map(|s| s.to_string()).collect())
                .cloned()
                .collect();
            assert_eq!(
                fns_after, expected,
                "dest {rel} item set must be residents+moved"
            );
        } else {
            // Every OTHER file keeps its exact top-level fn set.
            assert_eq!(
                fns_before, fns_after,
                "unrelated/referencer file {rel} must not gain or lose items"
            );
        }

        // (2) UNTOUCHED items are byte-identical (no accidental body corruption).
        // An item is "untouched" iff its signature survives verbatim AND it is not
        // one that was bumped (bumped items legitimately change their decl line).
        let blocks_before = item_blocks(before_text);
        let blocks_after = item_blocks(after_text);
        let bumped: BTreeSet<&str> = exp.bumped.iter().map(String::as_str).collect();
        for (sig, block) in &blocks_before {
            let is_moved = moved.iter().any(|m| sig_names_fn(sig, m));
            let is_bumped = bumped.iter().any(|b| sig_names_fn(sig, b));
            if is_moved || is_bumped {
                continue;
            }
            if let Some(after_block) = blocks_after.get(sig) {
                assert_eq!(
                    block, after_block,
                    "untouched item `{sig}` in {rel} must be byte-identical"
                );
            }
        }

        // (3) use-line deltas are confined to the allowed files.
        let is_touched =
            *rel == exp.src_rel || *rel == exp.dst_rel || exp.referencers.contains(rel);
        if !is_touched {
            assert_eq!(
                use_lines(before_text),
                use_lines(after_text),
                "unrelated file {rel} must not change its use lines"
            );
        }
    }
}

fn fn_name_multiset(files: &BTreeMap<String, String>) -> BTreeMap<String, usize> {
    let mut m: BTreeMap<String, usize> = BTreeMap::new();
    for text in files.values() {
        for name in top_level_fn_names(text) {
            *m.entry(name).or_default() += 1;
        }
    }
    m
}

/// True when a signature line declares `fn <name>`.
fn sig_names_fn(sig: &str, name: &str) -> bool {
    top_level_fn_names(&format!("{sig} {{")).contains(name)
}

// ===========================================================================
// A2 — the certificate on the canonical scenario
// ===========================================================================

const ALPHA: &str = r#"//! Alpha: the transplant SOURCE file.

/// Doc comment that must TRAVEL with the item (trivia-ownership law).
pub fn move_me(x: u32) -> u32 {
    let base = private_helper(x);
    shared_helper(base) + 1
}

// Used ONLY by move_me -> must travel with it (trichotomy: private).
fn private_helper(x: u32) -> u32 {
    x * 2
}

// Used by move_me AND stay_here -> must STAY, gain pub(crate), be back-imported.
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

fn seed_canonical(state: &mut SessionState, root: &Path) {
    write(&root.join("Cargo.toml"), cargo_toml());
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod gamma;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(&root.join("src/gamma.rs"), GAMMA);
    ingest(state, root);
}

#[test]
fn harness_certificate_canonical_move_changes_only_expected_regions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let before = snapshot_src(root);
    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("canonical transplant succeeds");
    let after = snapshot_src(root);

    assert_nothing_else_changed(
        &before,
        &after,
        &Expected {
            symbol: "move_me".into(),
            travelled: vec!["private_helper".into()],
            bumped: vec!["shared_helper".into()],
            src_rel: "src/alpha.rs".into(),
            dst_rel: "src/beta.rs".into(),
            referencers: vec!["src/gamma.rs".into()],
        },
    );
}

// ===========================================================================
// A3 — the round-trip property: A->B then B->A restores every file's item set
// ===========================================================================

#[test]
fn harness_round_trip_restores_every_file_item_set() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let names_before = fn_name_multiset(&snapshot_src(root));
    let per_file_before: BTreeMap<String, BTreeSet<String>> = snapshot_src(root)
        .iter()
        .map(|(k, v)| (k.clone(), top_level_fn_names(v)))
        .collect();

    // A -> B
    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("A->B transplant succeeds");

    // Re-ingest happens inside apply_batch; the graph now sees move_me in beta.
    // B -> A
    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/beta.rs", "src/alpha.rs"),
    )
    .expect("B->A transplant succeeds");

    let names_after = fn_name_multiset(&snapshot_src(root));
    assert_eq!(
        names_before, names_after,
        "round trip must preserve the global fn multiset"
    );
    let per_file_after: BTreeMap<String, BTreeSet<String>> = snapshot_src(root)
        .iter()
        .map(|(k, v)| (k.clone(), top_level_fn_names(v)))
        .collect();
    assert_eq!(
        per_file_before, per_file_after,
        "round trip: every file's top-level fn set must return to its origin"
    );

    assert_compiles(root, "round-trip");
}

// ===========================================================================
// GAP 1 — item extent is brace-counting, not parsing.
// A `}` inside a string literal must NOT truncate the moved region.
// RED before B1 (tree-sitter extent): the fn is cut at the string's brace,
// leaving its tail orphaned in the source.
// ===========================================================================

const G1_ALPHA: &str = r#"//! g1: a fn whose body has a brace inside a string + a macro.

pub fn move_me(x: u32) -> String {
    let s = "has a } brace inside a string literal";
    let t = format!("value={}", x);
    let marker_tail = s.len() as u32 + x;
    format!("{s}{t}{marker_tail}")
}

pub fn stay_here() -> u32 {
    0
}
"#;

#[test]
fn gap1_brace_in_string_does_not_truncate_moved_region() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    write(&root.join("src/alpha.rs"), G1_ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("g1 transplant succeeds");

    let alpha = read(root, "src/alpha.rs");
    let beta = read(root, "src/beta.rs");

    // The WHOLE body must have travelled — including the line AFTER the string brace.
    assert!(
        beta.contains("marker_tail") && beta.contains("format!(\"{s}{t}{marker_tail}\")"),
        "the full body (past the in-string brace) must travel to dest:\n{beta}"
    );
    // And NOTHING of the body may be orphaned in the source.
    assert!(
        !alpha.contains("marker_tail") && !alpha.contains("has a } brace"),
        "no fragment of the moved body may be left behind in source:\n{alpha}"
    );
    assert_compiles(root, "gap1");
}

// ===========================================================================
// GAP 2 — grouped `use ...::{a, b}` in a referencer must be split, not dropped.
// RED before B2: the group has no `alpha::move_me` substring, so the verb leaves
// it in refs_unresolved and the crate no longer compiles.
// ===========================================================================

#[test]
fn gap2_grouped_use_referencer_is_split_and_repointed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod refc;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(
        &root.join("src/refc.rs"),
        "//! refc: grouped import of two alpha items.\n\nuse crate::alpha::{move_me, stay_here};\n\npub fn use_both() -> u32 {\n    move_me(1) + stay_here(2)\n}\n",
    );
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("g2 transplant succeeds");

    let refc = read(root, "src/refc.rs");
    assert!(
        refc.contains("beta::move_me"),
        "grouped import: move_me must be re-pointed to beta:\n{refc}"
    );
    assert!(
        refc.contains("stay_here"),
        "grouped import: the OTHER member (stay_here) must survive:\n{refc}"
    );
    assert!(
        !refc.contains("alpha::{move_me") && !refc.contains("alpha::move_me"),
        "the old grouped path to move_me must be gone:\n{refc}"
    );
    assert_compiles(root, "gap2");
}

// ===========================================================================
// GAP 3 — glob `use ...::*` referencer is the one DISHONEST hazard: silently
// missed. At minimum it must be DETECTED and reported; resolving it is a bonus.
// RED before B2: the glob file is neither rewritten nor reported, and the crate
// stops compiling (move_me no longer in scope).
// ===========================================================================

#[test]
fn gap3_glob_use_referencer_is_detected_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod refg;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(
        &root.join("src/refg.rs"),
        "//! refg: glob import of the whole alpha module.\n\nuse crate::alpha::*;\n\npub fn use_glob() -> u32 {\n    move_me(2)\n}\n",
    );
    ingest(&mut state, root);

    let out = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("g3 transplant succeeds");

    // REQUIRED floor: the glob referencer is DETECTED and surfaced honestly.
    let unresolved: Vec<String> = out
        .get("refs_unresolved")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let reported = unresolved.iter().any(|u| u.contains("refg"));
    let files: Vec<String> = out
        .get("referencing_files")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let handled = files.iter().any(|f| f.contains("refg"));
    assert!(
        reported || handled,
        "the glob referencer must be DETECTED (reported in refs_unresolved or handled), not silently missed. unresolved={unresolved:?} files={files:?}"
    );

    // BONUS: resolving the glob keeps the crate compiling.
    assert_compiles(root, "gap3");
}

// The SHARPEST form of the dishonest hazard: a glob referencer that uses the
// symbol as a VALUE, not a call — so no `calls` edge forms in the graph and the
// only honest defence is a textual glob-scan over the source module. It must NOT
// be silently missed: either detected/reported, or resolved (crate compiles).
#[test]
fn gap3_glob_value_reference_is_not_silently_missed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod refv;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    // `move_me` referenced as a fn VALUE via a glob import (no call site).
    write(
        &root.join("src/refv.rs"),
        "//! refv: glob import, symbol used as a value (no call edge forms).\n\nuse crate::alpha::*;\n\npub fn as_value() -> fn(u32) -> u32 {\n    move_me\n}\n",
    );
    ingest(&mut state, root);

    let out = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("g3-value transplant succeeds");

    let unresolved: Vec<String> = out
        .get("refs_unresolved")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let files: Vec<String> = out
        .get("referencing_files")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let surfaced =
        unresolved.iter().any(|u| u.contains("refv")) || files.iter().any(|f| f.contains("refv"));
    assert!(
        surfaced,
        "a glob VALUE reference must not be silently missed. unresolved={unresolved:?} files={files:?}"
    );
    assert_compiles(root, "gap3-value");
}

// ===========================================================================
// GAP 4 — the moved fn's OWN top-of-file `use` needs must be carried to dest.
// RED before B3: move_me relies on `use std::collections::HashMap;` in alpha;
// dest (beta) lacks it, so the moved fn does not compile.
// ===========================================================================

const G4_ALPHA: &str = r#"//! g4: source whose moved fn relies on a file-level import.
use std::collections::HashMap;

pub fn move_me() -> usize {
    let mut m: HashMap<u32, u32> = HashMap::new();
    m.insert(1, 2);
    m.insert(3, 4);
    m.len()
}

pub fn stay_here() -> usize {
    42
}
"#;

#[test]
fn gap4_moved_fn_carries_its_own_file_level_imports() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    write(&root.join("src/alpha.rs"), G4_ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("g4 transplant succeeds");

    let beta = read(root, "src/beta.rs");
    assert!(
        beta.contains("use std::collections::HashMap"),
        "dest must carry the import the moved fn depends on:\n{beta}"
    );
    assert_compiles(root, "gap4");
}

// ===========================================================================
// GAP 5 — aliased grouped import must keep its alias when re-pointed.
// RED before B2: `use crate::alpha::{move_me as mm, stay_here};` is dropped
// into refs_unresolved and the crate stops compiling (mm unresolved).
// ===========================================================================

#[test]
fn gap5_aliased_grouped_use_preserves_the_alias() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod refa;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(
        &root.join("src/refa.rs"),
        "//! refa: aliased + grouped import.\n\nuse crate::alpha::{move_me as mm, stay_here};\n\npub fn use_alias() -> u32 {\n    mm(1) + stay_here(2)\n}\n",
    );
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("g5 transplant succeeds");

    let refa = read(root, "src/refa.rs");
    assert!(
        refa.contains("beta::move_me as mm") || refa.contains("move_me as mm"),
        "the alias `move_me as mm` must be preserved and re-pointed to beta:\n{refa}"
    );
    assert!(
        refa.contains("stay_here"),
        "the other grouped member must survive:\n{refa}"
    );
    assert_compiles(root, "gap5");
}

// ===========================================================================
// A1 — adversarial fixtures that must NOT corrupt (honesty regression guards).
// ===========================================================================

#[test]
fn adversarial_crlf_line_endings_do_not_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    // Windows line endings on the SOURCE file (m1nd CI runs on 3 OSes).
    let crlf = ALPHA.replace('\n', "\r\n");
    write(&root.join("src/alpha.rs"), &crlf);
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("CRLF transplant succeeds");

    let beta = read(root, "src/beta.rs");
    let alpha = read(root, "src/alpha.rs");
    assert!(
        beta.contains("fn move_me"),
        "item moved under CRLF:\n{beta}"
    );
    assert!(
        !alpha.contains("fn move_me"),
        "item removed from source under CRLF:\n{alpha}"
    );
    assert_compiles(root, "crlf");
}

#[test]
fn adversarial_unicode_identifiers_and_comments() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    let alpha = "//! α source with unicode: café, naïve, 日本語.\n\n/// Δ doc — computes π-ish things.\npub fn move_me(δ: u32) -> u32 {\n    // comentário: soma μ + δ\n    let μ = 3;\n    δ + μ\n}\n\npub fn stay_here() -> u32 {\n    0\n}\n";
    write(&root.join("src/alpha.rs"), alpha);
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("unicode transplant succeeds");

    let beta = read(root, "src/beta.rs");
    assert!(
        beta.contains("fn move_me") && beta.contains("δ + μ"),
        "unicode body travels intact:\n{beta}"
    );
    assert!(
        beta.contains("Δ doc") || beta.contains("π-ish"),
        "unicode doc comment travels:\n{beta}"
    );
    assert_compiles(root, "unicode");
}

#[test]
fn adversarial_generics_and_where_clause_span_lines() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    let alpha = "//! generics source.\n\npub fn move_me<T>(x: T) -> T\nwhere\n    T: Clone + std::fmt::Debug,\n{\n    let y = x.clone();\n    let _ = format!(\"{y:?}\");\n    y\n}\n\npub fn stay_here() -> u32 {\n    0\n}\n";
    write(&root.join("src/alpha.rs"), alpha);
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("generics transplant succeeds");

    let beta = read(root, "src/beta.rs");
    let alpha_after = read(root, "src/alpha.rs");
    assert!(
        beta.contains("fn move_me") && beta.contains("where") && beta.contains("T: Clone"),
        "the generic signature + where-clause travel together:\n{beta}"
    );
    assert!(
        !alpha_after.contains("T: Clone"),
        "the where-clause must not be orphaned in source:\n{alpha_after}"
    );
    assert_compiles(root, "generics");
}

#[test]
fn adversarial_nested_modules_do_not_confuse_extent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    // move_me is INDEPENDENT of the nested module: what is under test is that the
    // presence of `mod inner { fn nested_helper }` does not corrupt the EXTENT of
    // the moved top-level fn (nor drag the nested item out with it).
    let alpha = "//! nested-module source.\n\npub fn move_me() -> u32 {\n    123\n}\n\npub mod inner {\n    pub fn nested_helper() -> u32 {\n        7\n    }\n}\n\npub fn stay_here() -> u32 {\n    inner::nested_helper()\n}\n";
    write(&root.join("src/alpha.rs"), alpha);
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("nested-module transplant succeeds");

    let alpha_after = read(root, "src/alpha.rs");
    // The nested module and its helper must stay EXACTLY where they were.
    assert!(
        alpha_after.contains("pub mod inner") && alpha_after.contains("fn nested_helper"),
        "the nested module must not be dragged out with the moved fn:\n{alpha_after}"
    );
    assert!(
        !alpha_after.contains("fn move_me"),
        "only the top-level fn moves:\n{alpha_after}"
    );
}

// ===========================================================================
// A6 — atomicity under failure: a write failure AFTER preflight must leave every
// file untouched (rollback held). Plus exhaustive refusal-path atomicity.
// ===========================================================================

// A genuine mid-write injection: make the src dir read-only AFTER preflight has
// read the files, so apply_batch's temp-file write fails and NOTHING is renamed.
// POSIX-only: on Windows the read-only *directory* attribute does not block file
// creation, so there is no clean cross-platform injection point here (documented);
// the refusal-path test below runs everywhere.
#[cfg(unix)]
#[test]
fn atomicity_write_failure_after_preflight_touches_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let before = snapshot_src(root);

    // Freeze the src directory: reads still work (preflight), new temp files cannot
    // be created (the apply_batch write fails after all preflight checks passed).
    let src = root.join("src");
    let mut perms = std::fs::metadata(&src).unwrap().permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(&src, perms).unwrap();

    let result = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    );

    // Restore write permission BEFORE any assertion so tempdir cleanup never panics.
    let mut perms = std::fs::metadata(&src).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&src, perms).unwrap();

    assert!(
        result.is_err(),
        "a write failure after preflight must surface as an error, not a partial success"
    );
    let after = snapshot_src(root);
    assert_eq!(
        before, after,
        "atomicity: a failed transplant write must leave EVERY file byte-identical"
    );
}

// Cross-platform atomicity: every refusal class leaves all files byte-identical.
#[test]
fn atomicity_all_refusal_paths_write_nothing() {
    let scenarios: &[(&str, &str, &str)] = &[
        // (symbol, src, dest) each of which must be a clean refusal.
        ("ghost_symbol", "src/alpha.rs", "src/beta.rs"), // not found
        ("existing_resident", "src/alpha.rs", "src/beta.rs"), // lives elsewhere
        ("move_me", "src/alpha.rs", "src/alpha.rs"),     // same file
        ("move_me", "src/alpha.rs", "src/nope.rs"),      // missing dest
    ];
    for (sym, src, dest) in scenarios {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = make_state(root);
        seed_canonical(&mut state, root);
        let before = snapshot_src(root);

        let r = dispatch_tool(&mut state, "transplant", &params(root, sym, src, dest));
        assert!(r.is_err(), "scenario {sym}/{src}->{dest} must refuse");

        let after = snapshot_src(root);
        assert_eq!(
            before, after,
            "refusal scenario {sym}/{src}->{dest} must leave every file untouched"
        );
        assert!(
            !root.join("src/nope.rs").exists(),
            "a refused move must never create the missing dest file"
        );
    }
}

// ===========================================================================
// Regression (proptest-found): when the DESTINATION file is ALSO a referencer of
// the moved symbol, the referencer rewrite must NOT clobber the dest insertion.
// Minimal shrink of a 32-case proptest failure: moving f0 from m0 into m1, where
// m1 already references `crate::m0::f0`, DESTROYED f0 entirely (two edits to the
// same file, the second computed from the pre-insert text, overwrote the first).
// ===========================================================================

#[test]
fn regression_dest_is_also_a_referencer_preserves_the_item() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod m0;\npub mod m1;\n");
    write(
        &root.join("src/m0.rs"),
        "//! m0 (source).\n\npub fn f0(x: i64) -> i64 {\n    x\n}\n\npub fn other(x: i64) -> i64 {\n    x + 1\n}\n",
    );
    // m1 is BOTH the destination AND a referencer (`crate::m0::f0`).
    write(
        &root.join("src/m1.rs"),
        "//! m1 (destination that also references the moved symbol).\n\npub fn f8(x: i64) -> i64 {\n    crate::m0::f0(x) + x\n}\n",
    );
    ingest(&mut state, root);

    let before = snapshot_src(root);
    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "f0", "src/m0.rs", "src/m1.rs"),
    )
    .expect("dest-is-referencer transplant succeeds");
    let after = snapshot_src(root);

    // The item must NOT be destroyed — the global fn multiset is invariant.
    assert_eq!(
        fn_name_multiset(&before),
        fn_name_multiset(&after),
        "moving into a file that references the symbol must not destroy the item"
    );
    let m1 = read(root, "src/m1.rs");
    assert!(
        top_level_fn_names(&m1).contains("f0") && top_level_fn_names(&m1).contains("f8"),
        "dest must gain f0 AND keep its resident f8:\n{m1}"
    );
    assert!(
        !m1.contains("crate::m0::f0"),
        "the dest's own reference must be re-pointed off the old module:\n{m1}"
    );
    assert_compiles(root, "dest-is-referencer");
}
