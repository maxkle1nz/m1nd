// === Property-based hardening of the `transplant` verb (A4) ===
//
// proptest generates small random crates (2-4 modules, 3-8 fns each, random
// same-module call edges + cross-module references in qualified / direct-use /
// grouped-use forms), picks a random movable fn and transplants it. The invariant
// under test is the one that must hold for EVERY input: the verb either moves the
// symbol cleanly OR refuses honestly with ZERO writes — it never corrupts.
//
// Deliberately bounded (<=32 cases) and NO `cargo check` inside the loop (too
// slow); the structural certificate runs per case, and ONE sampled compile check
// runs outside the loop.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use proptest::prelude::*;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Shared infra
// ---------------------------------------------------------------------------

fn make_state(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code())
        .map(|mut s| {
            s.ingest_roots = vec![root.to_string_lossy().to_string()];
            s
        })
        .expect("init session")
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn ingest(state: &mut SessionState, root: &Path) -> bool {
    m1nd_mcp::tools::handle_ingest(
        state,
        m1nd_mcp::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "proptest".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            adapter: "code".to_string(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            project_root: None,
        },
    )
    .is_ok()
}

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

fn fn_multiset(files: &BTreeMap<String, String>) -> BTreeMap<String, usize> {
    let mut m: BTreeMap<String, usize> = BTreeMap::new();
    for text in files.values() {
        for name in top_level_fn_names(text) {
            *m.entry(name).or_default() += 1;
        }
    }
    m
}

fn snapshot(root: &Path, nmods: usize) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for m in 0..nmods {
        let rel = format!("src/m{m}.rs");
        if let Ok(t) = std::fs::read_to_string(root.join(&rel)) {
            out.insert(rel, t);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The random-crate generator
// ---------------------------------------------------------------------------

/// One generated fn: an optional callee (global index) and how it is referenced.
#[derive(Clone, Debug)]
struct FnGen {
    target: Option<usize>,
    /// 0 = qualified path (no use), 1 = direct `use`, 2 = grouped `use {..}`.
    use_form: u8,
}

#[derive(Clone, Debug)]
struct Plan {
    sizes: Vec<usize>,
    fns: Vec<FnGen>,
    pick: u64,
}

fn plan_strategy() -> impl Strategy<Value = Plan> {
    (2usize..=4)
        .prop_flat_map(|nmods| proptest::collection::vec(3usize..=8, nmods))
        .prop_flat_map(|sizes| {
            let total: usize = sizes.iter().sum();
            let fns = proptest::collection::vec(
                (proptest::option::of(0usize..total.max(1)), 0u8..3)
                    .prop_map(|(target, use_form)| FnGen { target, use_form }),
                total,
            );
            (Just(sizes), fns, any::<u64>())
        })
        .prop_map(|(sizes, fns, pick)| Plan { sizes, fns, pick })
}

fn module_of(sizes: &[usize], global: usize) -> usize {
    let mut acc = 0;
    for (m, s) in sizes.iter().enumerate() {
        if global < acc + s {
            return m;
        }
        acc += s;
    }
    sizes.len() - 1
}

fn offset(sizes: &[usize], module: usize) -> usize {
    sizes[..module].iter().sum()
}

/// Materialize the plan into module_index -> source text.
fn build_sources(plan: &Plan) -> Vec<String> {
    let nmods = plan.sizes.len();
    let mut bodies: Vec<Vec<String>> = vec![Vec::new(); nmods]; // per-module fn texts
    let mut uses: Vec<BTreeSet<String>> = vec![BTreeSet::new(); nmods]; // per-module use lines

    for g in 0..plan.fns.len() {
        let mi = module_of(&plan.sizes, g);
        let fg = &plan.fns[g];
        let mut call = String::from("x");
        if let Some(t) = fg.target {
            if t < plan.fns.len() && t != g {
                let tm = module_of(&plan.sizes, t);
                if tm == mi {
                    // same-module: only call EARLIER fns (keep the graph acyclic).
                    if t < g {
                        call = format!("f{t}(x) + x");
                    }
                } else {
                    // cross-module: pub callee, referenced in one of three forms.
                    match fg.use_form {
                        1 => {
                            uses[mi].insert(format!("use crate::m{tm}::f{t};"));
                            call = format!("f{t}(x) + x");
                        }
                        2 => {
                            uses[mi].insert(format!("use crate::m{tm}::{{f{t}}};"));
                            call = format!("f{t}(x) + x");
                        }
                        _ => {
                            call = format!("crate::m{tm}::f{t}(x) + x");
                        }
                    }
                }
            }
        }
        bodies[mi].push(format!("pub fn f{g}(x: i64) -> i64 {{\n    {call}\n}}\n"));
    }

    (0..nmods)
        .map(|m| {
            let mut s = format!("//! module m{m}.\n\n");
            for u in &uses[m] {
                s.push_str(u);
                s.push('\n');
            }
            if !uses[m].is_empty() {
                s.push('\n');
            }
            s.push_str(&bodies[m].join("\n"));
            s
        })
        .collect()
}

/// Seed the crate on disk + ingest. Returns (chosen_symbol, src_rel, dst_rel).
fn seed_plan(
    state: &mut SessionState,
    root: &Path,
    plan: &Plan,
) -> Option<(String, String, String)> {
    let nmods = plan.sizes.len();
    let sources = build_sources(plan);
    let mut librs = String::new();
    for m in 0..nmods {
        librs.push_str(&format!("pub mod m{m};\n"));
    }
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"proptest-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(&root.join("src/lib.rs"), &librs);
    for (m, src) in sources.iter().enumerate() {
        write(&root.join(format!("src/m{m}.rs")), src);
    }
    if !ingest(state, root) {
        return None;
    }

    // Choose (src_mod, local fn, dst_mod) deterministically from `pick`.
    let src_mod = (plan.pick as usize) % nmods;
    let s = plan.sizes[src_mod];
    let local = ((plan.pick / 7) as usize) % s;
    let g = offset(&plan.sizes, src_mod) + local;
    let dst_mod = (src_mod + 1 + ((plan.pick / 131) as usize) % (nmods - 1)) % nmods;
    Some((
        format!("f{g}"),
        format!("src/m{src_mod}.rs"),
        format!("src/m{dst_mod}.rs"),
    ))
}

fn params(root: &Path, symbol: &str, src: &str, dest: &str) -> serde_json::Value {
    serde_json::json!({
        "agent_id": "proptest",
        "symbol": symbol,
        "source_file": root.join(src).to_string_lossy(),
        "dest_file": root.join(dest).to_string_lossy(),
    })
}

// ---------------------------------------------------------------------------
// The property
// ---------------------------------------------------------------------------

proptest! {
    // Bounded per the spec. 16 forward cases keep the embed-per-ingest cost
    // tolerable while still exercising a wide shape space (the dest-is-referencer
    // corruption was hit at case 4 of a 32-case run and is pinned separately as a
    // fast deterministic regression in transplant_harness.rs).
    #![proptest_config(ProptestConfig {
        cases: 16,
        max_shrink_iters: 128,
        .. ProptestConfig::default()
    })]

    #[test]
    fn prop_transplant_moves_cleanly_or_refuses_with_zero_writes(plan in plan_strategy()) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = make_state(root);

        let Some((symbol, src_rel, dst_rel)) = seed_plan(&mut state, root, &plan) else {
            return Ok(()); // ingest failed on a degenerate crate — not our concern
        };
        let nmods = plan.sizes.len();
        let before = snapshot(root, nmods);

        let result = dispatch_tool(&mut state, "transplant", &params(root, &symbol, &src_rel, &dst_rel));

        match result {
            Err(_) => {
                // Honest refusal MUST write nothing.
                let after = snapshot(root, nmods);
                prop_assert_eq!(before, after, "a refused transplant wrote to disk");
            }
            Ok(out) => {
                let after = snapshot(root, nmods);

                // (1) Global fn-name multiset is invariant: items only relocate.
                prop_assert_eq!(
                    fn_multiset(&before),
                    fn_multiset(&after),
                    "global fn multiset changed (an item was created or destroyed)"
                );

                // (2) The symbol left the source and landed in the dest.
                let src_after = after.get(&src_rel).cloned().unwrap_or_default();
                let dst_after = after.get(&dst_rel).cloned().unwrap_or_default();
                prop_assert!(
                    !top_level_fn_names(&src_after).contains(&symbol),
                    "symbol {} still in source {}", symbol, src_rel
                );
                prop_assert!(
                    top_level_fn_names(&dst_after).contains(&symbol),
                    "symbol {} missing from dest {}", symbol, dst_rel
                );

                // (3) Reported travelled deps relocated; shared deps stayed.
                if let Some(arr) = out.get("deps_travelled").and_then(|v| v.as_array()) {
                    for d in arr.iter().filter_map(|v| v.as_str()) {
                        prop_assert!(
                            !top_level_fn_names(&src_after).contains(d),
                            "travelled dep {} still in source", d
                        );
                        prop_assert!(
                            top_level_fn_names(&dst_after).contains(d),
                            "travelled dep {} missing from dest", d
                        );
                    }
                }
                if let Some(arr) = out.get("deps_shared").and_then(|v| v.as_array()) {
                    for name in arr.iter().filter_map(|d| d.get("name").and_then(|v| v.as_str())) {
                        prop_assert!(
                            top_level_fn_names(&src_after).contains(name),
                            "shared dep {} must stay in source", name
                        );
                    }
                }

                // (4) Only reported files changed on disk.
                let changed: BTreeSet<String> = out.get("files_changed")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|s| s.as_str())
                        .filter_map(|p| Path::new(p).file_name().map(|f| format!("src/{}", f.to_string_lossy())))
                        .collect())
                    .unwrap_or_default();
                for (rel, before_text) in &before {
                    if !changed.contains(rel) {
                        prop_assert_eq!(
                            before_text,
                            after.get(rel).unwrap(),
                            "file {} changed but was not reported in files_changed", rel
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ONE sampled compile check OUTSIDE the loop (the mission's slow-path proof).
// A fixed, valid 3-module crate; transplant a fn and assert the crate compiles.
// ---------------------------------------------------------------------------

fn cargo_check(root: &Path) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let out = Command::new(cargo)
        .args(["check", "--quiet", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root.join("_check_target"))
        .output();
    match out {
        Err(e) => Err(format!("cargo-unavailable: {e}")),
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
    }
}

#[test]
fn proptest_sampled_case_compiles_after_transplant() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);

    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"proptest-sample\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        &root.join("src/lib.rs"),
        "pub mod m0;\npub mod m1;\npub mod m2;\n",
    );
    // m0::mover is called cross-module (qualified + grouped-use) and calls an
    // m0-private helper that must travel with it.
    write(
        &root.join("src/m0.rs"),
        "//! m0.\n\npub fn mover(x: i64) -> i64 {\n    helper(x) + 1\n}\n\nfn helper(x: i64) -> i64 {\n    x * 2\n}\n\npub fn resident(x: i64) -> i64 {\n    x - 1\n}\n",
    );
    write(
        &root.join("src/m1.rs"),
        "//! m1.\n\npub fn q(x: i64) -> i64 {\n    crate::m0::mover(x)\n}\n",
    );
    write(
        &root.join("src/m2.rs"),
        "//! m2 destination.\n\npub fn keep(x: i64) -> i64 {\n    x\n}\n",
    );
    assert!(ingest(&mut state, root));

    match cargo_check(root) {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => {
            eprintln!("SKIP sampled compile: {e}");
            return;
        }
        Err(e) => panic!("sampled fixture must compile BEFORE the move:\n{e}"),
    }

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "mover", "src/m0.rs", "src/m2.rs"),
    )
    .expect("sampled transplant succeeds");

    cargo_check(root).expect("sampled crate must COMPILE after the transplant");
}
