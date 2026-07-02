use std::process::Command;

/// Best-effort git short SHA (+ `-dirty` suffix) embedded at build time as
/// `M1ND_GIT_SHA`. Uses ONLY std — no build dependency. MUST NOT fail when
/// `.git` is absent (crates.io / vendored builds); falls back to "unknown" so
/// the binary can always answer "what am I?" honestly.
fn git_sha() -> String {
    let short = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|sha| !sha.is_empty());

    let Some(mut sha) = short else {
        return "unknown".to_string();
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
    if dirty {
        sha.push_str("-dirty");
    }
    sha
}

fn main() {
    let ui_dist = std::path::Path::new("../m1nd-ui/dist");

    if !ui_dist.exists() {
        println!("cargo:warning=m1nd-ui/dist/ not found. Run: cd m1nd-ui && npm run build");
        std::fs::create_dir_all(ui_dist).ok();
        std::fs::write(
            ui_dist.join("index.html"),
            "<!DOCTYPE html><html><head><title>m1nd</title></head><body><h1>m1nd UI not built</h1><p>Run: cd m1nd-ui && npm run build</p></body></html>",
        ).ok();
    }

    println!("cargo:rerun-if-changed=../m1nd-ui/dist/index.html");

    // Embed the git sha so the running binary can always declare exactly what it
    // is (version + sha) and drift-warn against stale binaries. Best-effort
    // rerun triggers: HEAD move (commit/checkout) and index changes (staging).
    // These paths may not exist on a non-git build — that is fine.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
    println!("cargo:rustc-env=M1ND_GIT_SHA={}", git_sha());
}
