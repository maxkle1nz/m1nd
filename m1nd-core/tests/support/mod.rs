// Shared test-only helpers for the RETROBUILDER stress/real integration
// suites (`retrobuilder_stress.rs`, `retrobuilder_real.rs`). Not part of any
// crate's public API — this file is pulled in only via `mod support;` from
// `tests/`.

use m1nd_core::graph::Graph;
use m1nd_ingest::{IngestConfig, Ingestor};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Copies `source` into a fresh private temp directory (skipping `target`,
/// `node_modules`, and every dot-directory — `.git`, `.m1nd`, checkpoint
/// lease dirs, etc.). `Ingestor::ingest()` walks the whole tree once to
/// extract and once more at COMPLETE-time to revalidate that no source
/// changed mid-ingest (`m1nd_ingest::CodeIngestBundleV1::revalidate_sources`)
/// — for a ~1000-file workspace that second walk lands minutes after the
/// first. Ingesting the LIVE working tree in place means any edit landing
/// anywhere in that multi-minute window (a human, another agent, a sibling
/// test) is real drift and correctly trips `FullReindexRequired` — the fence
/// is doing its job, but a long-running stress test has no business being
/// hostage to everything else touching the repo while it runs. Ingesting a
/// private copy instead makes the source set provably immutable for the
/// whole run, without touching the guard itself: genuine drift inside the
/// copy (a real bug in the ingest pipeline) still fails it exactly the same
/// way.
///
/// Dot-directories are skipped on top of `target`/`node_modules` because the
/// ingest walker never visits them either: it walks with
/// `.hidden(!include_dotfiles)` and `include_dotfiles` defaults to `false`
/// (`m1nd-ingest/src/walker.rs`), so a copied `.git` is bytes no extractor
/// will ever read. On this repo `.git` alone is ~28MB out of ~78MB total —
/// over a third of the copy payload — and the one test that actually parses
/// git history (`rb01_git_history_real`) reads it from the REAL root
/// directly, never from the snapshot.
pub fn snapshot_repo_root(source: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("snapshot tempdir");
    copy_tree(source, dir.path());
    dir
}

fn copy_tree(src: &Path, dst: &Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let file_type = entry.file_type().expect("file_type");
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if file_type.is_dir()
            && (name_str == "target" || name_str == "node_modules" || name_str.starts_with('.'))
        {
            continue;
        }
        let dst_path = dst.join(&name);
        if file_type.is_dir() {
            std::fs::create_dir(&dst_path).expect("create_dir");
            copy_tree(&entry.path(), &dst_path);
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dst_path).expect("copy file");
        }
        // Symlinks are skipped: the walker never follows them either
        // (`follow_links(false)`), so they carry nothing ingestable.
    }
}

/// A cheap content fingerprint of `root`'s ingestable tree: (relative path,
/// byte length, mtime) for every file the walker would visit, hashed —
/// stat-only, no file bodies read, no directory copied. Mirrors `copy_tree`'s
/// own skip list so the fingerprint and the thing it stands in for never
/// disagree about what's part of the tree.
fn fingerprint_tree(root: &Path) -> String {
    let mut entries = Vec::new();
    fingerprint_walk(root, Path::new(""), &mut entries);
    entries.sort();
    let mut hasher = DefaultHasher::new();
    for entry in &entries {
        entry.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn fingerprint_walk(dir: &Path, relative: &Path, out: &mut Vec<String>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if file_type.is_dir() {
            if name_str == "target" || name_str == "node_modules" || name_str.starts_with('.') {
                continue;
            }
            fingerprint_walk(&entry.path(), &relative.join(&name), out);
        } else if file_type.is_file() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            out.push(format!(
                "{}:{}:{mtime}",
                relative.join(&name).display(),
                metadata.len()
            ));
        }
    }
}

fn cache_file_path(root: &Path, fingerprint: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    let root_id = hasher.finish();
    std::env::temp_dir()
        .join("m1nd-retrobuilder-ingest-cache")
        .join(format!("{root_id:016x}-{fingerprint}.bin"))
}

/// Ingest the m1nd workspace root, sharing the resulting graph across every
/// test process this run spawns via an on-disk cache keyed by
/// `fingerprint_tree`. `nextest` runs each `#[test]` as its OWN process (the
/// one-process-per-test isolation `AGENTS.md` calls out as the reason nextest
/// is the canonical local runner), so a process-local `OnceLock` cannot share
/// this across the ~16 calls to `ingest_m1nd()` spread over
/// `retrobuilder_stress.rs` and `retrobuilder_real.rs` — every one of those
/// calls otherwise re-copies and re-parses the identical, immutable snapshot
/// from scratch. This cache does not touch the guarded ingest pipeline
/// (`m1nd-ingest`) at all and does not change what any test observes: the
/// FIRST caller (per tree state, across the whole machine) still runs the
/// real copy + the real guarded `Ingestor::ingest()`
/// (`FullReindexRequired`/`revalidate_sources` fully engaged, exactly as
/// before); every later caller with the SAME fingerprint reads back that
/// same graph instead of recomputing it. A cache miss for any reason —
/// first run, source changed, corrupt/stale payload, cross-version mismatch
/// — falls back to the full path and repopulates the cache, so a stale or
/// missing cache can only cost time, never correctness or a false green.
///
/// `label` is only for the `eprintln!` breadcrumbs below.
pub fn cached_ingest_m1nd(label: &str) -> Graph {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    let fp_start = Instant::now();
    let fingerprint = fingerprint_tree(&root);
    let cache_file = cache_file_path(&root, &fingerprint);
    let fp_elapsed = fp_start.elapsed();

    if let Ok(graph) = m1nd_core::snapshot_bin::load_graph(&cache_file) {
        eprintln!(
            "[{label}] ingest cache HIT (fingerprint {:.1}ms): {} nodes, {} edges <- {}",
            fp_elapsed.as_secs_f64() * 1000.0,
            graph.num_nodes(),
            graph.num_edges(),
            cache_file.display()
        );
        return graph;
    }

    let copy_start = Instant::now();
    let snapshot = snapshot_repo_root(&root);
    let copy_elapsed = copy_start.elapsed();

    let config = IngestConfig {
        root: snapshot.path().to_path_buf(),
        ..Default::default()
    };
    let ingest_start = Instant::now();
    let (graph, stats) = Ingestor::new(config).ingest().unwrap();
    let ingest_elapsed = ingest_start.elapsed();

    eprintln!(
        "[{label}] ingest cache MISS (fingerprint {:.1}ms): copy={:.1}ms ingest={:.1}ms \
         files_parsed={} nodes={} edges={}",
        fp_elapsed.as_secs_f64() * 1000.0,
        copy_elapsed.as_secs_f64() * 1000.0,
        ingest_elapsed.as_secs_f64() * 1000.0,
        stats.files_parsed,
        graph.num_nodes(),
        graph.num_edges()
    );

    if let Some(parent) = cache_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Process-unique temp path: `snapshot_bin::save_graph` already writes via
    // its own temp+rename (`path.with_extension("tmp")`), but that internal
    // temp name is DERIVED from the path we pass it, so two processes racing
    // to populate the SAME cache key would collide on that internal temp
    // file if we handed both the final `cache_file` path directly. Putting
    // the pid in the file STEM (not the extension `with_extension` replaces)
    // means save_graph's own `.with_extension("tmp")` still yields a
    // per-process-unique path, so both writers' internal temp files — and
    // the rename we do ourselves right after — stay collision-free.
    // Whichever process's rename lands last wins; both wrote a valid,
    // independently-ingested graph for the same fingerprint.
    let stem = cache_file.file_stem().unwrap_or_default().to_string_lossy();
    let unique_tmp = cache_file.with_file_name(format!("{stem}.tmp-{}.bin", std::process::id()));
    if m1nd_core::snapshot_bin::save_graph(&graph, &unique_tmp).is_ok() {
        let _ = std::fs::rename(&unique_tmp, &cache_file);
    } else {
        let _ = std::fs::remove_file(&unique_tmp);
    }

    graph
}
