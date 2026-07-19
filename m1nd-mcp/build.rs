#[path = "ui_bundle_support.rs"]
#[allow(dead_code)]
mod ui_bundle_support;

use std::process::Command;

use ui_bundle_support::ui_tree_identity;

fn release_ui_required() -> bool {
    std::env::var("M1ND_RELEASE_UI_REQUIRED").as_deref() == Ok("1")
}

fn expected_release_ui_digest() -> Option<String> {
    std::env::var("M1ND_EXPECTED_UI_BUNDLE_SHA256")
        .ok()
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

/// Best-effort exact source commit + dirty bit captured at build time. The full
/// commit (not a short display prefix) is a load-bearing G1 coherence binding:
/// a stale binary built from another commit must never look current merely
/// because both crates still advertise the same semantic version.
fn git_identity() -> (String, bool) {
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|sha| !sha.is_empty());

    let Some(commit) = commit else {
        return ("unknown".to_string(), true);
    };

    // `git status --porcelain` prints one line per dirty path; any output means
    // the working tree differs from HEAD, so tag the sha `-dirty`. A failure
    // here (e.g. no git) is treated as clean — the sha itself is what matters.
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| !out.stdout.is_empty())
        .unwrap_or(false);
    (commit, dirty)
}

fn main() {
    let manifest_dir = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must set CARGO_MANIFEST_DIR"),
    );
    let packaged_ui = manifest_dir.join("ui-dist");
    let uses_packaged_ui = packaged_ui.join("index.html").is_file();
    let ui_dist = if uses_packaged_ui {
        packaged_ui
    } else {
        manifest_dir.join("../m1nd-ui/dist")
    };
    let ui_package = if uses_packaged_ui {
        manifest_dir.join("ui-package.json")
    } else {
        manifest_dir.join("../m1nd-ui/package.json")
    };
    let require_release_ui = release_ui_required();
    println!("cargo:rustc-check-cfg=cfg(m1nd_packaged_ui)");
    if uses_packaged_ui {
        println!("cargo:rustc-cfg=m1nd_packaged_ui");
    }
    println!("cargo:rerun-if-env-changed=M1ND_RELEASE_UI_REQUIRED");
    println!("cargo:rerun-if-env-changed=M1ND_EXPECTED_UI_BUNDLE_SHA256");

    if !ui_dist.exists() {
        if require_release_ui {
            panic!(
                "M1ND release build refused: m1nd-ui/dist is absent; install the verified UI artifact before Cargo"
            );
        }
        println!("cargo:warning=m1nd-ui/dist/ not found. Run: cd m1nd-ui && npm run build");
        std::fs::create_dir_all(&ui_dist).ok();
        std::fs::write(
            ui_dist.join("index.html"),
            "<!DOCTYPE html><html><head><title>m1nd</title></head><body><h1>m1nd UI not built</h1><p>Run: cd m1nd-ui && npm run build</p></body></html>",
        ).ok();
    }

    println!("cargo:rerun-if-changed={}", ui_dist.display());
    println!("cargo:rerun-if-changed={}", ui_package.display());
    println!("cargo:rerun-if-changed=ui_bundle_support.rs");

    if require_release_ui && !ui_dist.join("index.html").is_file() {
        panic!("M1ND release build refused: verified UI artifact has no index.html");
    }

    let ui_identity = ui_tree_identity(&ui_dist);
    let (ui_digest, ui_placeholder) = match ui_identity {
        Ok(identity) => (identity.sha256, identity.placeholder),
        Err(err) => {
            if require_release_ui {
                panic!("M1ND release build refused: unable to hash verified UI artifact: {err}");
            }
            println!("cargo:warning=unable to hash m1nd-ui/dist: {err}");
            ("unknown".to_string(), true)
        }
    };
    if require_release_ui {
        if ui_placeholder {
            panic!("M1ND release build refused: placeholder UI artifact is forbidden");
        }
        let expected = expected_release_ui_digest().unwrap_or_else(|| {
            panic!(
                "M1ND release build refused: M1ND_EXPECTED_UI_BUNDLE_SHA256 is absent or invalid"
            )
        });
        if ui_digest != expected {
            panic!(
                "M1ND release build refused: UI digest mismatch (expected {expected}, observed {ui_digest})"
            );
        }
    }
    println!("cargo:rustc-env=M1ND_UI_BUNDLE_SHA256={ui_digest}");
    println!(
        "cargo:rustc-env=M1ND_UI_BUNDLE_PLACEHOLDER={}",
        if ui_placeholder { "1" } else { "0" }
    );
    println!(
        "cargo:rustc-env=M1ND_UI_BUNDLE_VERSION={}",
        env!("CARGO_PKG_VERSION")
    );

    // Embed the git sha so the running binary can always declare exactly what it
    // is (version + sha) and drift-warn against stale binaries. Best-effort
    // rerun triggers: HEAD move (commit/checkout) and index changes (staging).
    // These paths may not exist on a non-git build — that is fine.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
    let (build_source_commit, build_source_dirty) = git_identity();
    let display_sha = if build_source_dirty {
        format!("{build_source_commit}-dirty")
    } else {
        build_source_commit.clone()
    };
    println!("cargo:rustc-env=M1ND_GIT_SHA={display_sha}");
    println!("cargo:rustc-env=M1ND_BUILD_SOURCE_COMMIT={build_source_commit}");
    println!(
        "cargo:rustc-env=M1ND_BUILD_SOURCE_DIRTY={}",
        if build_source_dirty { "1" } else { "0" }
    );
}
