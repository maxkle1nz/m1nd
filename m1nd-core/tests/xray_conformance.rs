//! X-RAY conformance gate (Pilar 2 — the structural guardian, proven).
//!
//! Wired into the EXISTING `cargo test --workspace` CI gate (no CI-config
//! change): it reads the ratified `xray.manifest.json` and FAILS the build when
//! the as-built code drifts from the ratified North Star. Two robust checks:
//!
//!  1. require_exists — every load-bearing invariant the manifest declares MUST
//!     still exist in the source. Catches an invariant silently deleted/renamed
//!     (e.g. `mission_verify`, a commission engine). The compiler does NOT know
//!     your invariants; this is net-new enforcement.
//!  2. forbid — no production cross-crate dependency matches a `forbid` rule.
//!     (layer_order is kept as a backstop, but note: for a linearly-connected
//!     workspace every layer inversion is already a dependency CYCLE that cargo
//!     rejects, so the unique value here is the explicit `forbid` pairs.)
//!
//! When the manifest is NOT ratified, violations are reported, not fatal.

use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    ratified: bool,
    #[serde(default)]
    layer_order: Vec<String>,
    #[serde(default)]
    forbid: Vec<(String, String)>,
    #[serde(default)]
    require_exists: Vec<String>,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn load_manifest(root: &Path) -> Option<Manifest> {
    let raw = std::fs::read_to_string(root.join("xray.manifest.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

fn workspace_members(root: &Path) -> Vec<String> {
    let cargo = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    let mut out = Vec::new();
    if let Some(start) = cargo.find("members") {
        if let Some(open) = cargo[start..].find('[') {
            let from = start + open + 1;
            if let Some(close) = cargo[from..].find(']') {
                for tok in cargo[from..from + close].split(',') {
                    let name = tok.trim().trim_matches('"').trim();
                    if !name.is_empty() {
                        out.push(name.to_string());
                    }
                }
            }
        }
    }
    out
}

/// Internal (workspace-member) PRODUCTION dependency edges as (from, to) pairs,
/// resolved via `cargo metadata` so EVERY manifest form is covered: inline
/// `[dependencies]`, `[dependencies.name]` subtables, and
/// `[target.'cfg(...)'.dependencies]`. dev- and build-dependencies are excluded
/// (tests legitimately cross layers).
fn internal_prod_deps(root: &Path, members: &[String]) -> Vec<(String, String)> {
    let meta = cargo_metadata::MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()
        .expect("cargo metadata");
    let is_member = |n: &str| members.iter().any(|m| m == n);
    let mut deps = Vec::new();
    for pkg in &meta.packages {
        let pname = pkg.name.to_string();
        if !is_member(&pname) {
            continue;
        }
        for d in &pkg.dependencies {
            if d.kind == cargo_metadata::DependencyKind::Normal
                && d.name != pname
                && is_member(&d.name)
            {
                deps.push((pname.clone(), d.name.to_string()));
            }
        }
    }
    deps
}

/// Strip Rust line/block comments and double-quoted string literals so
/// `require_exists` matches only ACTUAL code: a comment or string that merely
/// mentions a symbol must NOT satisfy the invariant (a silent rename/deletion of
/// the real implementation has to be caught). Best-effort on raw strings —
/// sufficient because invariants appear in many code locations.
fn strip_rust(src: &str) -> String {
    let b = src.as_bytes();
    let n = b.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if b[i] == b'/' && i + 1 < n && b[i + 1] == b'/' {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(n);
            continue;
        }
        if b[i] == b'"' {
            i += 1;
            while i < n {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(b' ');
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Collect all `.rs` source under each member's `src/` (bounded recursion),
/// with comments and string literals stripped (see `strip_rust`).
fn source_blob(root: &Path, members: &[String]) -> String {
    fn walk(dir: &Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    out.push_str(&strip_rust(&s));
                    out.push('\n');
                }
            }
        }
    }
    let mut blob = String::new();
    for m in members {
        walk(&root.join(m).join("src"), &mut blob);
    }
    blob
}

fn layer_index(order: &[String], m: &str) -> Option<usize> {
    order.iter().position(|x| x == m)
}

/// Pure forbid/layer gate for one dependency edge.
fn dep_violation(manifest: &Manifest, from: &str, to: &str) -> Option<String> {
    if manifest.forbid.iter().any(|(a, b)| a == from && b == to) {
        return Some(format!("forbid: {from} must not depend on {to}"));
    }
    if let (Some(i), Some(j)) = (
        layer_index(&manifest.layer_order, from),
        layer_index(&manifest.layer_order, to),
    ) {
        if j > i {
            return Some(format!(
                "layer: {from} (L{i}) depends on higher {to} (L{j})"
            ));
        }
    }
    None
}

fn dep_violations(manifest: &Manifest, deps: &[(String, String)]) -> Vec<String> {
    deps.iter()
        .filter_map(|(f, t)| dep_violation(manifest, f, t).map(|r| format!("{f}->{t}: {r}")))
        .collect()
}

/// require_exists invariants missing from the source blob.
fn missing_invariants(manifest: &Manifest, source: &str) -> Vec<String> {
    manifest
        .require_exists
        .iter()
        .filter(|needle| !source.contains(needle.as_str()))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// THE LIVE GATE — runs inside `cargo test --workspace` (the CI gate)
// ---------------------------------------------------------------------------
#[test]
fn xray_conformance_gate() {
    let root = workspace_root();
    let manifest = load_manifest(&root).expect("xray.manifest.json present and parseable");
    let members = workspace_members(&root);
    assert!(
        members.len() >= 2,
        "expected workspace members, got {members:?}"
    );

    let deps = internal_prod_deps(&root, &members);
    let dep_viol = dep_violations(&manifest, &deps);
    let missing = missing_invariants(&manifest, &source_blob(&root, &members));

    let mut report = Vec::new();
    for v in &dep_viol {
        report.push(format!("forbidden dependency: {v}"));
    }
    for m in &missing {
        report.push(format!("required invariant MISSING from source: {m}"));
    }

    if manifest.ratified {
        assert!(
            report.is_empty(),
            "X-RAY conformance gate FAILED (ratified manifest violated):\n  {}",
            report.join("\n  ")
        );
    } else if !report.is_empty() {
        eprintln!(
            "[xray] provisional manifest — {} candidate violation(s), not blocking:\n  {}",
            report.len(),
            report.join("\n  ")
        );
    }
}

// ---------------------------------------------------------------------------
// Unit tests — prove the gate BITES (synthetic), independent of the live repo
// ---------------------------------------------------------------------------
fn order4() -> Vec<String> {
    ["m1nd-core", "m1nd-ingest", "m1nd-mcp", "m1nd-openclaw"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn gate_flags_explicit_forbid_pair() {
    // forbid a coupling the compiler would happily allow (no cycle implied).
    let m = Manifest {
        ratified: true,
        layer_order: order4(),
        forbid: vec![("m1nd-ingest".into(), "m1nd-openclaw".into())],
        require_exists: vec![],
    };
    let v = dep_violations(&m, &[("m1nd-ingest".into(), "m1nd-openclaw".into())]);
    assert_eq!(v.len(), 1, "explicit forbid must flag, got {v:?}");
}

#[test]
fn gate_allows_downward_dependency() {
    let m = Manifest {
        ratified: true,
        layer_order: order4(),
        forbid: vec![],
        require_exists: vec![],
    };
    let v = dep_violations(&m, &[("m1nd-mcp".into(), "m1nd-core".into())]);
    assert!(v.is_empty(), "downward dep must be allowed, got {v:?}");
}

#[test]
fn gate_flags_missing_invariant() {
    let m = Manifest {
        ratified: true,
        layer_order: vec![],
        forbid: vec![],
        require_exists: vec![
            "mission_verify".into(),
            "this_symbol_does_not_exist_xyz".into(),
        ],
    };
    let src = "fn mission_verify() {}\n";
    let missing = missing_invariants(&m, src);
    assert_eq!(missing, vec!["this_symbol_does_not_exist_xyz".to_string()]);
}

#[test]
fn require_exists_ignores_comments_and_strings() {
    // The symbol appears ONLY in a comment and a string literal -> after stripping
    // it must read as MISSING: a silent rename/deletion of the real implementation
    // cannot hide behind a leftover comment or tool-name string.
    let m = Manifest {
        ratified: true,
        layer_order: vec![],
        forbid: vec![],
        require_exists: vec!["mission_verify".into()],
    };
    let only_mentions = "// mission_verify is the proof gate\nlet tool = \"mission_verify\";\n";
    assert_eq!(
        missing_invariants(&m, &strip_rust(only_mentions)),
        vec!["mission_verify".to_string()],
        "comment/string mention must NOT satisfy the invariant"
    );
    // A real code definition still satisfies it.
    let real = strip_rust("pub fn mission_verify() {}\n");
    assert!(
        missing_invariants(&m, &real).is_empty(),
        "real symbol must satisfy the invariant"
    );
}
