//! One-time adoption of a pre-1.5 legacy graph snapshot into the runtime root.
//!
//! Before 1.5 the graph snapshot lived at `./graph_snapshot.json` (relative to
//! the working directory). With a dedicated runtime dir (`M1ND_RUNTIME_DIR`) the
//! snapshot is checkpoint-managed under the runtime root instead. On upgrade the
//! new runtime graph is born EMPTY while the populated legacy snapshot sits
//! untouched in the old location — stranding the whole graph and dropping every
//! upgrader into `needs_ingest` with its brain sitting right there.
//!
//! This module adopts the legacy snapshot ONCE at boot: it copies the exact
//! legacy bytes into the runtime root so the normal load path then finds them. A
//! typed journal (mirroring `boot_kv_migration`) makes the adoption idempotent
//! and auditable, and the adoption never overwrites a populated runtime graph.

use m1nd_core::error::M1ndResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Journal file written under the runtime root once adoption commits.
pub const ADOPTION_JOURNAL_FILE: &str = "legacy_graph_adoption_v1.json";
const JOURNAL_SCHEMA: &str = "m1nd-legacy-graph-adoption-v1";
const VERSION: u32 = 1;

/// Durable, one-time record of a legacy-snapshot adoption.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LegacyGraphAdoptionJournalV1 {
    pub schema: String,
    pub version: u32,
    pub status: String,
    pub legacy_source_path: String,
    pub source_digest: String,
    pub node_count: u64,
    pub adopted_at_ms: u64,
}

/// What the one boot-time adoption attempt decided. Every non-`Adopted` variant
/// is a deliberate, safe no-op.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyAdoptionOutcome {
    /// The legacy snapshot was copied into the runtime root.
    Adopted { node_count: u64 },
    /// The runtime graph already has nodes — never overwrite it.
    SkippedRuntimeAlreadyPopulated,
    /// No legacy snapshot exists (or it is empty), so there is nothing to adopt.
    SkippedNoLegacySnapshot,
    /// A prior boot already adopted (the journal is present) — one-time.
    SkippedAlreadyAdopted,
    /// The runtime graph path IS the legacy path — no separate location to adopt.
    SkippedSameLocation,
    /// The legacy snapshot exists but could not be read/parsed; left untouched.
    SkippedLegacyUnreadable(String),
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Adopt a pre-1.5 legacy graph snapshot into the runtime root exactly once.
///
/// Owner boot only (a read-only attach never writes). Returns a
/// [`LegacyAdoptionOutcome`]; the only writing path is `Adopted`, and it can
/// never fire over a populated runtime graph, a previously adopted runtime
/// (journal present), or a legacy file that fails to parse. The one honest
/// stderr line is emitted here so the behavior is identical wherever it runs.
///
/// When (and only when) the graph is adopted, the legacy plasticity sidecar is
/// adopted alongside it — best-effort, and ONLY if it parses cleanly. A
/// known-corrupt legacy-format plasticity file is skipped with a warning so the
/// graph still boots (Step 4's own "continuing without it" import guard stays
/// intact).
pub(crate) fn maybe_adopt_legacy_snapshot(
    runtime_graph_path: &Path,
    legacy_graph_path: &Path,
    runtime_plasticity_path: &Path,
    legacy_plasticity_path: &Path,
    runtime_root: &Path,
) -> LegacyAdoptionOutcome {
    let journal_path = runtime_root.join(ADOPTION_JOURNAL_FILE);

    // One-time: a committed journal means adoption already ran. Never re-adopt,
    // even if the runtime graph was later emptied.
    if journal_path.exists() {
        return LegacyAdoptionOutcome::SkippedAlreadyAdopted;
    }

    // No separate legacy location (no runtime dir, or an explicit --graph that
    // already points at the legacy file): there is nothing to adopt onto itself.
    if same_file(runtime_graph_path, legacy_graph_path) {
        return LegacyAdoptionOutcome::SkippedSameLocation;
    }

    // Cheap gate FIRST: on the overwhelmingly common boot (no legacy file in the
    // old location) return before touching the runtime graph at all, so this
    // repair adds no load cost to a normal warm boot.
    if !legacy_graph_path.exists() {
        return LegacyAdoptionOutcome::SkippedNoLegacySnapshot;
    }

    // A legacy candidate exists — only now is it worth the reads. It must parse
    // as a valid, non-empty snapshot.
    let legacy_bytes = match std::fs::read(legacy_graph_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "[m1nd] legacy graph snapshot at {} not adopted (unreadable): {error}",
                legacy_graph_path.display()
            );
            return LegacyAdoptionOutcome::SkippedLegacyUnreadable(error.to_string());
        }
    };
    let node_count = match m1nd_core::snapshot::decode_graph_json(&legacy_bytes) {
        Ok(graph) => graph.num_nodes() as u64,
        Err(error) => {
            eprintln!(
                "[m1nd] legacy graph snapshot at {} not adopted (invalid snapshot): {error}",
                legacy_graph_path.display()
            );
            return LegacyAdoptionOutcome::SkippedLegacyUnreadable(error.to_string());
        }
    };
    if node_count == 0 {
        return LegacyAdoptionOutcome::SkippedNoLegacySnapshot;
    }

    // Never adopt over a populated runtime graph — the adversarial invariant.
    if runtime_graph_is_populated(runtime_graph_path) {
        return LegacyAdoptionOutcome::SkippedRuntimeAlreadyPopulated;
    }

    // Adopt: copy the EXACT legacy bytes into the checkpoint-managed runtime
    // path (durable, cross-platform), then journal the one-time event.
    if let Err(error) =
        crate::boot_kv_migration::durable_atomic_write(runtime_graph_path, &legacy_bytes)
    {
        eprintln!(
            "[m1nd] legacy graph snapshot at {} not adopted (copy failed): {error}",
            legacy_graph_path.display()
        );
        return LegacyAdoptionOutcome::SkippedLegacyUnreadable(format!("copy failed: {error}"));
    }
    // The graph is adopted — bring its plasticity sidecar along, best-effort.
    adopt_legacy_plasticity(runtime_plasticity_path, legacy_plasticity_path);

    let journal = LegacyGraphAdoptionJournalV1 {
        schema: JOURNAL_SCHEMA.into(),
        version: VERSION,
        status: "adopted".into(),
        legacy_source_path: legacy_graph_path.display().to_string(),
        source_digest: sha256(&legacy_bytes),
        node_count,
        adopted_at_ms: now_ms(),
    };
    if let Err(error) = write_journal(&journal_path, &journal) {
        // The graph copy already succeeded; a journal-write failure must not
        // crash boot. Re-adoption is still prevented by the now non-empty
        // runtime graph on the next boot.
        eprintln!("[m1nd] legacy snapshot adopted but journal write failed: {error}");
    }
    eprintln!(
        "[m1nd] adopted legacy graph snapshot from {} ({node_count} nodes) into runtime root {}",
        legacy_graph_path.display(),
        runtime_root.display()
    );
    LegacyAdoptionOutcome::Adopted { node_count }
}

/// Best-effort adoption of the legacy plasticity sidecar, run only when the
/// graph was just adopted. Copies the exact legacy bytes into the runtime
/// plasticity path ONLY when the runtime plasticity is absent AND the legacy
/// file parses cleanly as plasticity state. Every failure is a graceful skip
/// with one honest warning — the graph is already adopted and boots regardless.
fn adopt_legacy_plasticity(runtime_plasticity_path: &Path, legacy_plasticity_path: &Path) {
    if runtime_plasticity_path.exists()
        || !legacy_plasticity_path.exists()
        || same_file(runtime_plasticity_path, legacy_plasticity_path)
    {
        return;
    }
    // Adopt ONLY if it parses cleanly. The field's legacy plasticity is a known
    // corrupt legacy format — skipped here rather than copied forward.
    if let Err(error) = m1nd_core::snapshot::load_plasticity_state(legacy_plasticity_path) {
        eprintln!(
            "[m1nd] legacy plasticity at {} not adopted (invalid legacy format): {error}; continuing without it",
            legacy_plasticity_path.display()
        );
        return;
    }
    let bytes = match std::fs::read(legacy_plasticity_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "[m1nd] legacy plasticity at {} not adopted (unreadable): {error}; continuing without it",
                legacy_plasticity_path.display()
            );
            return;
        }
    };
    match crate::boot_kv_migration::durable_atomic_write(runtime_plasticity_path, &bytes) {
        Ok(()) => eprintln!(
            "[m1nd] adopted legacy plasticity state from {}",
            legacy_plasticity_path.display()
        ),
        Err(error) => eprintln!(
            "[m1nd] legacy plasticity at {} not adopted (copy failed): {error}; continuing without it",
            legacy_plasticity_path.display()
        ),
    }
}

/// True when both paths resolve to the same existing file. A path that does not
/// exist canonicalizes to an error, so an absent runtime graph is never "same".
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// True only when the runtime snapshot exists AND loads AND has at least one
/// node. An absent, empty, or unparseable runtime snapshot counts as "not
/// populated" — boot would start fresh from it anyway, so adopting a valid
/// legacy snapshot over it is a recovery, never a loss of real graph data.
fn runtime_graph_is_populated(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    m1nd_core::snapshot::load_graph(path)
        .map(|graph| graph.num_nodes() > 0)
        .unwrap_or(false)
}

fn write_journal(path: &Path, journal: &LegacyGraphAdoptionJournalV1) -> M1ndResult<()> {
    crate::boot_kv_migration::durable_atomic_write(path, &serde_json::to_vec_pretty(journal)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::NodeType;
    use std::path::PathBuf;

    /// Build a snapshot file with `n` real code nodes at `path`.
    fn write_snapshot_with_nodes(path: &Path, n: usize) {
        let mut graph = Graph::new();
        for i in 0..n {
            graph
                .add_node(
                    &format!("file::src/mod{i}.rs"),
                    &format!("module {i}"),
                    NodeType::File,
                    &[],
                    0.0,
                    0.0,
                )
                .expect("add node");
        }
        m1nd_core::snapshot::save_graph(&graph, path).expect("save snapshot");
    }

    /// A runtime-dir layout mirroring the M1ND_RUNTIME_DIR split: the runtime
    /// graph lives under the runtime root; the legacy graph in the parent (cwd).
    struct Layout {
        _temp: tempfile::TempDir,
        runtime_root: PathBuf,
        runtime_graph: PathBuf,
        legacy_graph: PathBuf,
        runtime_plasticity: PathBuf,
        legacy_plasticity: PathBuf,
    }

    impl Layout {
        fn adopt(&self) -> LegacyAdoptionOutcome {
            maybe_adopt_legacy_snapshot(
                &self.runtime_graph,
                &self.legacy_graph,
                &self.runtime_plasticity,
                &self.legacy_plasticity,
                &self.runtime_root,
            )
        }
    }

    fn layout() -> Layout {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_root = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_root).expect("runtime root");
        Layout {
            runtime_graph: runtime_root.join("graph_snapshot.json"),
            legacy_graph: temp.path().join("graph_snapshot.json"),
            runtime_plasticity: runtime_root.join("plasticity_state.json"),
            legacy_plasticity: temp.path().join("plasticity_state.json"),
            runtime_root,
            _temp: temp,
        }
    }

    #[test]
    fn empty_runtime_with_valid_legacy_adopts_and_journals_and_loads() {
        let l = layout();
        write_snapshot_with_nodes(&l.legacy_graph, 5);

        assert_eq!(l.adopt(), LegacyAdoptionOutcome::Adopted { node_count: 5 });

        // Journal written.
        let journal_path = l.runtime_root.join(ADOPTION_JOURNAL_FILE);
        assert!(journal_path.exists(), "adoption journal must be written");
        let journal: LegacyGraphAdoptionJournalV1 =
            serde_json::from_slice(&std::fs::read(&journal_path).expect("read journal"))
                .expect("parse journal");
        assert_eq!(journal.status, "adopted");
        assert_eq!(journal.node_count, 5);
        assert_eq!(journal.schema, JOURNAL_SCHEMA);

        // Boot loads N nodes from the runtime path.
        let loaded =
            m1nd_core::snapshot::load_graph(&l.runtime_graph).expect("load adopted snapshot");
        assert_eq!(loaded.num_nodes(), 5);
    }

    #[test]
    fn second_boot_is_a_no_op() {
        let l = layout();
        write_snapshot_with_nodes(&l.legacy_graph, 3);

        assert_eq!(l.adopt(), LegacyAdoptionOutcome::Adopted { node_count: 3 });
        assert_eq!(l.adopt(), LegacyAdoptionOutcome::SkippedAlreadyAdopted);

        // Still exactly the adopted graph.
        let loaded = m1nd_core::snapshot::load_graph(&l.runtime_graph).expect("load");
        assert_eq!(loaded.num_nodes(), 3);
    }

    #[test]
    fn populated_runtime_is_left_untouched_even_with_legacy_present() {
        let l = layout();
        // Runtime already has its own graph; legacy is a different, larger one.
        write_snapshot_with_nodes(&l.runtime_graph, 2);
        write_snapshot_with_nodes(&l.legacy_graph, 9);
        let runtime_digest_before = sha256(&std::fs::read(&l.runtime_graph).expect("read runtime"));

        assert_eq!(
            l.adopt(),
            LegacyAdoptionOutcome::SkippedRuntimeAlreadyPopulated
        );

        // Runtime bytes are byte-for-byte unchanged, and no journal was written.
        let runtime_digest_after = sha256(&std::fs::read(&l.runtime_graph).expect("read runtime"));
        assert_eq!(runtime_digest_before, runtime_digest_after);
        assert!(!l.runtime_root.join(ADOPTION_JOURNAL_FILE).exists());
        let loaded = m1nd_core::snapshot::load_graph(&l.runtime_graph).expect("load");
        assert_eq!(loaded.num_nodes(), 2);
    }

    #[test]
    fn corrupt_legacy_is_skipped_with_no_adoption() {
        let l = layout();
        std::fs::write(&l.legacy_graph, b"this is not a graph snapshot").expect("write corrupt");

        let outcome = l.adopt();
        assert!(
            matches!(outcome, LegacyAdoptionOutcome::SkippedLegacyUnreadable(_)),
            "corrupt legacy must be skipped gracefully, got {outcome:?}"
        );

        // Nothing adopted: runtime graph absent, no journal, boot starts empty.
        assert!(!l.runtime_graph.exists(), "runtime graph must stay absent");
        assert!(!l.runtime_root.join(ADOPTION_JOURNAL_FILE).exists());
    }

    #[test]
    fn valid_legacy_plasticity_is_adopted_but_corrupt_is_skipped() {
        // Valid plasticity → copied alongside the graph.
        {
            let l = layout();
            write_snapshot_with_nodes(&l.legacy_graph, 4);
            m1nd_core::snapshot::save_plasticity_state(&[], &l.legacy_plasticity)
                .expect("save legacy plasticity");

            assert_eq!(l.adopt(), LegacyAdoptionOutcome::Adopted { node_count: 4 });
            assert!(
                l.runtime_plasticity.exists(),
                "valid legacy plasticity must be adopted alongside the graph"
            );
            m1nd_core::snapshot::load_plasticity_state(&l.runtime_plasticity)
                .expect("adopted plasticity still parses at the runtime path");
        }
        // Corrupt (known legacy-format) plasticity → graph adopted, plasticity skipped.
        {
            let l = layout();
            write_snapshot_with_nodes(&l.legacy_graph, 4);
            std::fs::write(&l.legacy_plasticity, b"not plasticity json")
                .expect("write corrupt plasticity");

            assert_eq!(l.adopt(), LegacyAdoptionOutcome::Adopted { node_count: 4 });
            assert!(
                !l.runtime_plasticity.exists(),
                "corrupt legacy plasticity must be skipped, never copied forward"
            );
            // The graph adoption is unaffected — it still boots.
            let loaded = m1nd_core::snapshot::load_graph(&l.runtime_graph).expect("graph loads");
            assert_eq!(loaded.num_nodes(), 4);
        }
    }
}
