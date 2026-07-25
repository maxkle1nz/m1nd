use std::path::Path;

pub const NOISE_DIR_NAMES: &[&str] = &[
    ".cache",
    ".git",
    ".hg",
    ".m1nd-benchmark-fixtures",
    ".m1nd-field-workspaces",
    ".m1nd-real-audit-runtime",
    ".m1nd-self-audit-runtime",
    ".mypy_cache",
    ".next",
    ".pytest_cache",
    ".ruff_cache",
    ".svn",
    ".venv",
    ".vault",
    "__pycache__",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "vendor",
    "venv",
    "wiki-build",
];

pub const RUNTIME_ARTIFACT_FILE_NAMES: &[&str] = &[
    "alerts.json",
    "antibodies.json",
    "auto_ingest_events.jsonl",
    "auto_ingest_state.json",
    "global_savings.json",
    "graph_snapshot.json",
    "ingest_roots.json",
    "plasticity_state.json",
    "trust_state.json",
    "tremor_state.json",
];

/// Tag stamped on every node extracted from a file whose CONTENT looks like
/// build output rather than authored source. Ranking treats any `noise:` tag as
/// a demote signal (`m1nd_core::seed::is_noise_tag`), so a deliberately
/// committed bundle stays retrievable — it just stops out-ranking real code.
///
/// The probe measures SHAPE, so the tag also lands on dense machine-written
/// data that was never minified — this repo's own `docs/benchmarks/**/*.jsonl`
/// event streams, for one. That is the intended outcome, not a false positive:
/// a recorded event log should rank below the code a query is actually looking
/// for, and demoting is all the tag does.
pub const MINIFIED_NOISE_TAG: &str = "noise:minified";

/// Web-asset extensions a sourcemap can sit behind (`app.js.map`, `main.css.map`).
const SOURCEMAP_ASSET_STEMS: &[&str] = &[".js", ".mjs", ".cjs", ".jsx", ".ts", ".tsx", ".css"];

/// Content probe window. Minification is visible in the first few KB; reading
/// more buys nothing and costs on multi-MB bundles.
const MINIFIED_PROBE_BYTES: usize = 64 * 1024;
/// Below this the "average line" statistic is not yet meaningful.
const MINIFIED_MIN_PROBE_BYTES: usize = 2048;
/// Authored source wraps; minified output does not. Real code averages ~30-40
/// bytes per line.
const MINIFIED_MEAN_LINE_BYTES: f32 = 200.0;
/// Minifiers strip whitespace. Prose and authored code sit around 15-18%; a
/// bundle sits well under 10%. This is what separates a minified bundle from a
/// long-paragraph Markdown file, which also has few, long lines.
const MINIFIED_MAX_WHITESPACE_RATIO: f32 = 0.12;

pub fn default_skip_dirs() -> Vec<String> {
    NOISE_DIR_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

pub fn is_noise_dir_name(name: &str) -> bool {
    NOISE_DIR_NAMES.contains(&name)
}

pub fn is_runtime_artifact_file_name(name: &str) -> bool {
    RUNTIME_ARTIFACT_FILE_NAMES.contains(&name)
}

pub fn is_editor_temp_file_name(name: &str) -> bool {
    name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".tmp")
        || name == ".DS_Store"
        || name.starts_with(".#")
        || name.starts_with("4913")
}

/// Named build output: a minified bundle or its sourcemap.
///
/// These are DERIVED files — the authored source they came from is normally in
/// the same repo, so ingesting them duplicates the corpus with symbols the
/// minifier renamed to one or two letters. Those renamed helpers collect every
/// call site in the bundle and win PageRank outright, which is the measured
/// ranking pollution this rule removes (askGOD F5 verdict, 2026-07-24).
///
/// The rule is deliberately NAME-EXACT, never "looks like a bundle": only the
/// `*.min.<ext>` / `*-min.<ext>` convention and true sourcemaps
/// (`<name>.<web-ext>.map`) qualify. A bare `*.map` is NOT matched — `world.map`
/// and `keyboard.map` are real files. Content that merely looks generated is
/// tagged, not skipped (see [`looks_minified_source`]).
pub fn is_minified_asset_file_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    for ext in ["js", "mjs", "cjs", "css"] {
        if name.ends_with(&format!(".min.{ext}")) || name.ends_with(&format!("-min.{ext}")) {
            return true;
        }
    }
    if let Some(stem) = name.strip_suffix(".map") {
        return SOURCEMAP_ASSET_STEMS.iter().any(|s| stem.ends_with(s));
    }
    false
}

/// Cheap, bounded probe: does this file's SHAPE say "machine-generated" rather
/// than "written by a person"?
///
/// Two signals, both required, both O(probe window):
///   * lines far longer than authored code ever runs, and
///   * whitespace stripped out (what a minifier does, and what prose never does).
///
/// A true means TAG (`MINIFIED_NOISE_TAG`) — never skip. A vendored bundle can
/// be committed on purpose, and a false positive must cost a demote, not the
/// file's existence in the graph.
pub fn looks_minified_source(content: &[u8]) -> bool {
    let probe = &content[..content.len().min(MINIFIED_PROBE_BYTES)];
    if probe.len() < MINIFIED_MIN_PROBE_BYTES {
        return false;
    }
    let newlines = probe.iter().filter(|byte| **byte == b'\n').count();
    let whitespace = probe
        .iter()
        .filter(|byte| byte.is_ascii_whitespace())
        .count();
    let mean_line = probe.len() as f32 / (newlines + 1) as f32;
    let whitespace_ratio = whitespace as f32 / probe.len() as f32;
    mean_line >= MINIFIED_MEAN_LINE_BYTES && whitespace_ratio < MINIFIED_MAX_WHITESPACE_RATIO
}

pub fn is_noise_dir_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_noise_dir_name)
}

pub fn is_noise_path(path: &Path) -> bool {
    if path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(is_noise_dir_name)
    }) {
        return true;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            is_editor_temp_file_name(name)
                || is_runtime_artifact_file_name(name)
                || is_minified_asset_file_name(name)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_noise_directories_anywhere_in_path() {
        assert!(is_noise_path(Path::new("/repo/.venv/lib/site.py")));
        assert!(is_noise_path(Path::new(
            "/repo/.m1nd-field-workspaces/round/repo/file.rs"
        )));
        assert!(is_noise_dir_path(Path::new("/repo/node_modules")));
        assert!(!is_noise_path(Path::new("/repo/src/lib.rs")));
    }

    #[test]
    fn skips_named_minified_bundles_and_sourcemaps() {
        assert!(is_noise_path(Path::new("/repo/assets/vendor.min.js")));
        assert!(is_noise_path(Path::new("/repo/assets/theme.min.css")));
        assert!(is_noise_path(Path::new("/repo/assets/lib-min.js")));
        assert!(is_noise_path(Path::new("/repo/assets/app.js.map")));
        assert!(is_noise_path(Path::new("/repo/assets/main.css.map")));
        // Authored sources that merely mention "min", and non-sourcemap `.map`
        // data files, stay in the corpus.
        assert!(!is_noise_path(Path::new("/repo/src/minify.js")));
        assert!(!is_noise_path(Path::new("/repo/src/admin.css")));
        assert!(!is_noise_path(Path::new("/repo/data/world.map")));
        assert!(!is_noise_path(Path::new("/repo/src/bundle.js")));
    }

    #[test]
    fn minified_shape_is_detected_without_eating_authored_source() {
        let bundle = "function a(e,t){return e+t}".repeat(200);
        assert!(looks_minified_source(bundle.as_bytes()));

        // Long-line prose (the Markdown false-positive risk): few newlines, but
        // ordinary whitespace density.
        let prose = "The walker discovers every authored source file under the \
                     managed root and refuses anything that is not bijectively \
                     addressable by its relative path. "
            .repeat(40);
        assert!(!looks_minified_source(prose.as_bytes()));

        // Ordinary wrapped source.
        let source = "pub fn handle_function_call(request: &Request) -> Response {\n    \
                      dispatch(request)\n}\n"
            .repeat(60);
        assert!(!looks_minified_source(source.as_bytes()));

        // Too small to judge: abstain rather than guess.
        assert!(!looks_minified_source(b"function a(e,t){return e+t}"));
    }

    #[test]
    fn detects_runtime_artifacts_and_editor_temp_files() {
        assert!(is_noise_path(Path::new("/repo/graph_snapshot.json")));
        assert!(is_noise_path(Path::new("/repo/plasticity_state.json")));
        assert!(is_noise_path(Path::new("/repo/file.md.swp")));
        assert!(is_noise_path(Path::new("/repo/.DS_Store")));
        assert!(!is_noise_path(Path::new("/repo/docs/notes.md")));
    }
}
