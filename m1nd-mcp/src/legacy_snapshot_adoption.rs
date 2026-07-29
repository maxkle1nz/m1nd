//! One-time adoption of a pre-1.5 legacy graph snapshot into the runtime root.
//!
//! Before 1.5 the graph snapshot lived at `./graph_snapshot.json` (relative to
//! the working directory). With a dedicated runtime dir (`M1ND_RUNTIME_DIR`) the
//! snapshot is checkpoint-managed under the runtime root instead. On upgrade the
//! new runtime graph is born EMPTY while the populated legacy snapshot sits
//! untouched in the old location — stranding the whole graph and dropping every
//! upgrader into `needs_ingest` with its brain sitting right there.
//!
//! This module adopts the legacy snapshot ONCE, **through the brain actor**: the
//! legacy graph is installed into the live session and the SAME actor turn
//! commits it through the checkpoint. A typed journal (mirroring
//! `boot_kv_migration`) makes the adoption idempotent and auditable, and the
//! adoption never overwrites a populated runtime graph.
//!
//! The actor detour is the whole point. The adoption used to run pre-actor, in
//! `McpServer::new`, copying the legacy bytes straight into the runtime root.
//! `BrainActorHandle::start` then reconciled the checkpoint by itself —
//! `restore_checkpoint` + `reload_authoritative_from_disk` — and rebuilt the
//! session from a CURRENT that was captured while the runtime graph was still
//! empty. The adopted graph was therefore reverted on the same boot, before the
//! first tool call, while the journal still recorded `status: "adopted"`: the
//! one-time rescue was spent without ever having worked. CURRENT stays the
//! single source of truth; a boot-time migration must commit through it instead
//! of writing behind it.

use crate::brain_runtime::BrainActorHandle;
use crate::runtime_jobs::RuntimeJobFailure;
use crate::session::SessionState;
use m1nd_core::error::M1ndResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;

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
    /// The legacy snapshot was installed into the session and checkpointed.
    Adopted { node_count: u64 },
    /// The runtime graph already has nodes — never overwrite it.
    SkippedRuntimeAlreadyPopulated,
    /// No legacy snapshot exists (or it is empty), so there is nothing to adopt.
    SkippedNoLegacySnapshot,
    /// A prior boot already adopted AND that adoption stuck — one-time.
    SkippedAlreadyAdopted,
    /// The runtime graph path IS the legacy path — no separate location to adopt.
    SkippedSameLocation,
    /// The legacy snapshot exists but could not be read/parsed/committed; the
    /// runtime is left exactly as the checkpoint restored it.
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
/// Owner boot only (a read-only attach never writes), and only AFTER the brain
/// actor has started: the actor's own checkpoint restore has already run, so the
/// runtime graph read here is exactly what CURRENT holds. Returns a
/// [`LegacyAdoptionOutcome`]; the only writing path is `Adopted`, and it can
/// never fire over a populated runtime graph, an adoption that already stuck, or
/// a legacy file that fails to parse. The one honest stderr line is emitted here
/// so the behavior is identical wherever it runs.
///
/// When (and only when) the graph is adopted, the legacy plasticity sidecar is
/// imported alongside it — best-effort, and ONLY if it parses cleanly. A
/// known-corrupt legacy-format plasticity file is skipped with a warning so the
/// graph still boots (the same "continuing without it" guard the boot import
/// already applies).
pub(crate) fn maybe_adopt_legacy_snapshot(
    actor: &BrainActorHandle,
    runtime_graph_path: &Path,
    legacy_graph_path: &Path,
    legacy_plasticity_path: &Path,
    runtime_root: &Path,
) -> LegacyAdoptionOutcome {
    let journal_path = runtime_root.join(ADOPTION_JOURNAL_FILE);

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

    // One-time — but only for an adoption that actually STUCK. A journal beside
    // an empty runtime graph is the exact footprint of the reverted adoption
    // this module used to produce: it recorded `status: "adopted"` for a graph
    // the actor threw away on the same boot. Treating that record as spent would
    // leave the brain permanently empty with its rescue already burned, so a
    // recorded-but-absent adoption is re-adoptable.
    if journal_path.exists() && runtime_graph_is_populated(runtime_graph_path) {
        return LegacyAdoptionOutcome::SkippedAlreadyAdopted;
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

    // Adopt INSIDE the actor boundary: the legacy graph is installed into the
    // live session and this same turn publishes CURRENT from it. The canonical
    // working files are written by the checkpoint projection, so CURRENT can
    // never be older than the adopted runtime graph.
    let legacy_display = legacy_graph_path.display().to_string();
    let graph_source = legacy_graph_path.to_path_buf();
    let plasticity_source = legacy_plasticity_path.to_path_buf();
    if let Err(error) = actor.try_execute_with_checkpoint_ack(move |state| {
        install_legacy_graph(state, &graph_source, &plasticity_source)
    }) {
        eprintln!(
            "[m1nd] legacy graph snapshot at {legacy_display} not adopted (checkpoint refused): {error}"
        );
        return LegacyAdoptionOutcome::SkippedLegacyUnreadable(format!(
            "checkpoint refused: {error}"
        ));
    }

    // The journal is written ONLY after that ACK. An adoption is recorded when
    // it is durable, never before — a crash between the two simply leaves the
    // committed graph un-journaled, and the next boot skips it as an already
    // populated runtime instead of claiming an adoption that never landed.
    let journal = LegacyGraphAdoptionJournalV1 {
        schema: JOURNAL_SCHEMA.into(),
        version: VERSION,
        status: "adopted".into(),
        legacy_source_path: legacy_display.clone(),
        source_digest: sha256(&legacy_bytes),
        node_count,
        adopted_at_ms: now_ms(),
    };
    if let Err(error) = write_journal(&journal_path, &journal) {
        // CURRENT already holds the adopted graph; a journal-write failure must
        // not crash boot. Re-adoption is still prevented by the now non-empty
        // runtime graph on the next boot.
        eprintln!("[m1nd] legacy snapshot adopted but journal write failed: {error}");
    }
    eprintln!(
        "[m1nd] adopted legacy graph snapshot from {legacy_display} ({node_count} nodes) into runtime root {}",
        runtime_root.display()
    );
    LegacyAdoptionOutcome::Adopted { node_count }
}

/// Install the legacy graph into the live session, exactly the way the `persist`
/// load action swaps a snapshot in: replace the graph, rebuild the engines that
/// are bound to it, and bump the generation. Nothing is written here — the
/// actor's checkpoint serializes the new session and projects the canonical
/// working files after CURRENT.
fn install_legacy_graph(
    state: &mut SessionState,
    legacy_graph_path: &Path,
    legacy_plasticity_path: &Path,
) -> Result<u32, RuntimeJobFailure> {
    let mut graph = m1nd_core::snapshot::load_graph(legacy_graph_path)
        .map_err(|error| RuntimeJobFailure::new("legacy_graph_unreadable", error.to_string()))?;
    if !graph.finalized && graph.num_nodes() > 0 {
        graph
            .finalize()
            .map_err(|error| RuntimeJobFailure::new("legacy_graph_finalize", error.to_string()))?;
    }
    let node_count = graph.num_nodes();
    state.graph = Arc::new(parking_lot::RwLock::new(graph));
    state
        .rebuild_engines()
        .map_err(|error| RuntimeJobFailure::new("legacy_graph_rebuild", error.to_string()))?;
    state.bump_graph_generation();
    import_legacy_plasticity(state, legacy_plasticity_path);
    Ok(node_count)
}

/// Best-effort import of the legacy plasticity sidecar, run only when the graph
/// was just installed (`rebuild_engines` has reset BOTH plasticity engines to
/// fresh ones bound to the adopted graph). Every failure is a graceful skip with
/// one honest warning — the graph is already adopted and checkpoints regardless.
fn import_legacy_plasticity(state: &mut SessionState, legacy_plasticity_path: &Path) {
    if !legacy_plasticity_path.exists() {
        return;
    }
    // Import ONLY if it parses cleanly. The field's legacy plasticity is a known
    // corrupt legacy format — skipped here rather than carried forward.
    let states = match m1nd_core::snapshot::load_plasticity_state(legacy_plasticity_path) {
        Ok(states) => states,
        Err(error) => {
            eprintln!(
                "[m1nd] legacy plasticity at {} not adopted (invalid legacy format): {error}; continuing without it",
                legacy_plasticity_path.display()
            );
            return;
        }
    };
    let mut graph = state.graph.write();
    // BOTH engines restore from the sidecar, exactly as strict recovery does
    // (`SessionState::recover_from_checkpoint`) and as the friendly boot import
    // does. `state.orchestrator.plasticity` is the engine `activate`/`query`
    // actually update (query.rs `query()` step 8), and it stamps its own
    // `query_count` into `last_used_query`. Left at zero by the `rebuild_engines`
    // above while the adopted graph carries the restored counts, the first
    // strengthen marks a just-used edge 1 — i.e. OLDER than every edge the
    // sidecar restored — and the adopting checkpoint publishes that skew right
    // back out. Re-applying the same validated plan to the same topology is
    // idempotent and cannot fail where the first import succeeded, so the two
    // share one report below.
    let imported = state
        .plasticity
        .import_state(&mut graph, &states)
        .and_then(|_| {
            state
                .orchestrator
                .plasticity
                .import_state(&mut graph, &states)
        });
    match imported {
        Ok(_) => eprintln!(
            "[m1nd] adopted legacy plasticity state from {}",
            legacy_plasticity_path.display()
        ),
        Err(error) => {
            // A refused import can stop halfway. The graph is the rescue and the
            // sidecar is a nicety, so drop the half-imported engines for clean
            // ones bound to the adopted graph rather than let partial synaptic
            // state reach the checkpoint that is about to publish it. BOTH are
            // dropped: keeping one restored beside one fresh is the very
            // divergence this import closes, only inverted.
            state.plasticity = m1nd_core::plasticity::PlasticityEngine::new(
                &graph,
                m1nd_core::plasticity::PlasticityConfig::default(),
            );
            state.orchestrator.plasticity = m1nd_core::plasticity::PlasticityEngine::new(
                &graph,
                m1nd_core::plasticity::PlasticityConfig::default(),
            );
            eprintln!(
                "[m1nd] legacy plasticity at {} not adopted ({error}); continuing without it",
                legacy_plasticity_path.display()
            );
        }
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
///
/// Read after actor start, so this file is the checkpoint's own projection of
/// CURRENT, not a stale pre-actor byte image.
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
    use crate::brain_runtime::{
        BrainSessionCell, UnboundBrainCheckpointAuthority, BRAIN_CHECKPOINT_DIRECTORY,
    };
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
        /// Boot the owner session and start its actor exactly as the registry
        /// does, then run the one boot-time adoption attempt against it.
        fn adopt(&self) -> LegacyAdoptionOutcome {
            self.boot(|_| {})
        }

        /// Same, with a hook that runs on the started actor BEFORE the adoption
        /// attempt (used to publish a CURRENT the adoption must reconcile with).
        fn boot(&self, before: impl FnOnce(&BrainActorHandle)) -> LegacyAdoptionOutcome {
            let session = Arc::new(BrainSessionCell::new(owner_session(&self.runtime_root)));
            let actor = BrainActorHandle::start(
                "legacy-adoption-owner".to_string(),
                Arc::clone(&session),
                self.runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
                Arc::new(UnboundBrainCheckpointAuthority),
                2,
                None,
            )
            .expect("start the bound actor");
            before(&actor);
            let outcome = maybe_adopt_legacy_snapshot(
                &actor,
                &self.runtime_graph,
                &self.legacy_graph,
                &self.legacy_plasticity,
                &self.runtime_root,
            );
            actor.stop().expect("stop the bound actor");
            drop(actor);
            drop(session);
            outcome
        }

        fn served_nodes(&self) -> u32 {
            let session = Arc::new(BrainSessionCell::new(owner_session(&self.runtime_root)));
            let actor = BrainActorHandle::start(
                "legacy-adoption-owner".to_string(),
                Arc::clone(&session),
                self.runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
                Arc::new(UnboundBrainCheckpointAuthority),
                2,
                None,
            )
            .expect("restart the bound actor");
            let served = actor
                .try_read_snapshot(|state| {
                    Ok::<u32, RuntimeJobFailure>(state.graph.read().num_nodes())
                })
                .expect("read the served node count")
                .value;
            actor.stop().expect("stop the bound actor");
            drop(actor);
            drop(session);
            served
        }
    }

    /// The owner boot the adoption runs behind: `McpServer::new` over the
    /// runtime root, exactly as `--serve`/stdio boot it.
    fn owner_session(runtime_root: &Path) -> crate::session::SessionState {
        crate::server::McpServer::new(crate::server::McpConfig {
            graph_source: runtime_root.join("graph_snapshot.json"),
            plasticity_state: runtime_root.join("plasticity_state.json"),
            runtime_dir: Some(runtime_root.to_path_buf()),
            registry_dir: Some(runtime_root.join("registry")),
            ..Default::default()
        })
        .expect("boot owner session")
        .into_session_state()
    }

    /// Write a two-node legacy snapshot plus a legacy plasticity sidecar aged
    /// through a real engine, and return the `last_used_query` that sidecar
    /// carries — well above the 0/1/2 a freshly rebuilt engine would stamp.
    fn write_legacy_pair_with_warm_plasticity(l: &Layout) -> u32 {
        use m1nd_core::plasticity::{PlasticityConfig, PlasticityEngine};
        use m1nd_core::types::{EdgeDirection, FiniteF32};

        /// Warm queries the previous boot is pretended to have run.
        const WARM_QUERIES: u32 = 41;

        let mut graph = Graph::new();
        let lib = graph
            .add_node("file::src/lib.rs", "lib.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add lib node");
        let core = graph
            .add_node(
                "file::src/core.rs",
                "core.rs",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add core node");
        graph
            .add_edge(
                lib,
                core,
                "imports",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.8),
            )
            .expect("add edge");
        graph.finalize().expect("finalize graph");
        // Saved BEFORE warming: the snapshot is pristine, the sidecar is the
        // only thing carrying the previous boot's counters.
        m1nd_core::snapshot::save_graph(&graph, &l.legacy_graph).expect("save legacy snapshot");

        let mut warm = PlasticityEngine::new(&graph, PlasticityConfig::default());
        let activated = vec![(lib, FiniteF32::new(0.9)), (core, FiniteF32::new(0.8))];
        for _ in 0..WARM_QUERIES {
            warm.update(&mut graph, &activated, &activated, "warm")
                .expect("warm plasticity cycle");
        }
        let states = warm.export_state(&graph).expect("export warm state");
        let restored_max = states
            .iter()
            .map(|state| state.last_used_query)
            .max()
            .expect("sidecar carries at least one synapse");
        assert_eq!(
            restored_max, WARM_QUERIES,
            "fixture must carry a non-zero restored query counter"
        );
        m1nd_core::snapshot::save_plasticity_state(&states, &l.legacy_plasticity)
            .expect("save legacy plasticity sidecar");
        restored_max
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

        // Boot loads N nodes from the runtime path, and the NEXT actor start —
        // the one that used to revert the adoption — still serves them.
        let loaded =
            m1nd_core::snapshot::load_graph(&l.runtime_graph).expect("load adopted snapshot");
        assert_eq!(loaded.num_nodes(), 5);
        assert_eq!(l.served_nodes(), 5);
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

    /// The owner's exact machine: the journal says `adopted` but the graph the
    /// adoption produced is gone (the pre-actor copy was reverted by the actor's
    /// CURRENT restore). A record of an adoption that did not stick must not
    /// spend the one-time rescue.
    #[test]
    fn a_recorded_adoption_that_did_not_stick_is_re_adoptable() {
        let l = layout();
        write_snapshot_with_nodes(&l.legacy_graph, 7);
        write_journal(
            &l.runtime_root.join(ADOPTION_JOURNAL_FILE),
            &LegacyGraphAdoptionJournalV1 {
                schema: JOURNAL_SCHEMA.into(),
                version: VERSION,
                status: "adopted".into(),
                legacy_source_path: l.legacy_graph.display().to_string(),
                source_digest: "0".repeat(64),
                node_count: 7,
                adopted_at_ms: 1,
            },
        )
        .expect("seed a spent-looking journal");

        assert_eq!(l.adopt(), LegacyAdoptionOutcome::Adopted { node_count: 7 });
        assert_eq!(l.served_nodes(), 7);
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

        // Nothing adopted: no journal, and the served brain stays empty.
        assert!(!l.runtime_root.join(ADOPTION_JOURNAL_FILE).exists());
        assert_eq!(l.served_nodes(), 0);
    }

    #[test]
    fn valid_legacy_plasticity_is_adopted_but_corrupt_is_skipped() {
        // Valid plasticity → imported alongside the graph.
        {
            let l = layout();
            write_snapshot_with_nodes(&l.legacy_graph, 4);
            m1nd_core::snapshot::save_plasticity_state(&[], &l.legacy_plasticity)
                .expect("save legacy plasticity");

            assert_eq!(l.adopt(), LegacyAdoptionOutcome::Adopted { node_count: 4 });
            assert!(
                l.runtime_plasticity.exists(),
                "the adopting checkpoint must project the runtime plasticity sidecar"
            );
            m1nd_core::snapshot::load_plasticity_state(&l.runtime_plasticity)
                .expect("adopted plasticity still parses at the runtime path");
        }
        // Corrupt (known legacy-format) plasticity → graph adopted, import skipped.
        {
            let l = layout();
            write_snapshot_with_nodes(&l.legacy_graph, 4);
            std::fs::write(&l.legacy_plasticity, b"not plasticity json")
                .expect("write corrupt plasticity");

            assert_eq!(l.adopt(), LegacyAdoptionOutcome::Adopted { node_count: 4 });
            // The graph adoption is unaffected — it still boots.
            let loaded = m1nd_core::snapshot::load_graph(&l.runtime_graph).expect("graph loads");
            assert_eq!(loaded.num_nodes(), 4);
            assert_eq!(l.served_nodes(), 4);
        }
    }

    /// The field's plasticity sidecar PARSES and is then refused mid-import.
    /// The rescue is the graph: a refused import must leave a clean engine
    /// behind, never half-imported synaptic state, because the very next thing
    /// that happens is the checkpoint publishing that engine.
    #[test]
    fn legacy_plasticity_refused_mid_import_still_adopts_a_clean_sidecar() {
        let l = layout();
        write_snapshot_with_nodes(&l.legacy_graph, 4);
        // Two rows with the identical full synaptic key: parses fine, refused by
        // `import_state` as a duplicate.
        let row = m1nd_core::plasticity::SynapticState {
            source_label: "file::src/mod0.rs".into(),
            target_label: "file::src/mod1.rs".into(),
            relation: "calls".into(),
            direction: Some(0),
            inhibitory: Some(false),
            original_weight: 1.0,
            current_weight: 1.0,
            strengthen_count: 0,
            weaken_count: 0,
            ltp_applied: false,
            ltd_applied: false,
            last_used_query: 0,
        };
        m1nd_core::snapshot::save_plasticity_state(&[row.clone(), row], &l.legacy_plasticity)
            .expect("save duplicate-key legacy plasticity");

        assert_eq!(l.adopt(), LegacyAdoptionOutcome::Adopted { node_count: 4 });
        assert_eq!(l.served_nodes(), 4);
        assert!(
            m1nd_core::snapshot::load_plasticity_state(&l.runtime_plasticity)
                .expect("the projected sidecar parses")
                .is_empty(),
            "a refused import must not leave partial synaptic state in the checkpoint"
        );
    }

    /// Regression: the adoption must restore the plasticity query counter into
    /// the ORCHESTRATOR's engine too, not only `state.plasticity`.
    ///
    /// `install_legacy_graph` calls `rebuild_engines`, which zeroes BOTH
    /// engines, and `activate`/`query` then strengthen through
    /// `state.orchestrator.plasticity` (query.rs `query()` step 8), which stamps
    /// its own `query_count` into `graph.edge_plasticity.last_used_query`.
    /// Importing the legacy sidecar into `state.plasticity` alone left that
    /// counter at zero while the adopted graph carried the restored counts, so
    /// the first strengthen after an adoption stamped a just-used edge with 1 —
    /// making it look OLDER than every edge the sidecar restored, and the
    /// adopting checkpoint publishes the skew straight back out. Sibling of the
    /// friendly boot fix in `server.rs`
    /// (`friendly_boot_restores_plasticity_counter_into_orchestrator_engine`);
    /// strict recovery (`SessionState::recover_from_checkpoint`) already imports
    /// into both.
    ///
    /// Driven through `install_legacy_graph` — the entire body of the actor turn
    /// `maybe_adopt_legacy_snapshot` runs, and its only caller — because the
    /// engine under test lives in memory on the session the adoption produced,
    /// which the actor owns and the tests above stop before returning. The
    /// end-to-end boot is what those tests already prove.
    #[test]
    fn adoption_restores_plasticity_counter_into_orchestrator_engine() {
        let l = layout();
        let restored_max = write_legacy_pair_with_warm_plasticity(&l);

        let mut state = owner_session(&l.runtime_root);
        assert_eq!(
            install_legacy_graph(&mut state, &l.legacy_graph, &l.legacy_plasticity)
                .expect("install the legacy graph"),
            2
        );
        state.ingest_roots = vec![l.runtime_root.to_string_lossy().to_string()];
        state.workspace_root = Some(l.runtime_root.to_string_lossy().to_string());

        let before: Vec<u32> = state.graph.read().edge_plasticity.last_used_query.clone();
        assert!(
            before.contains(&restored_max),
            "the adoption must import the legacy sidecar into the adopted graph, got {before:?}"
        );

        // One orchestrator-driven query (the production `activate` path).
        let output = crate::tools::handle_activate(
            &mut state,
            crate::protocol::core::ActivateInput {
                query: "lib core".into(),
                agent_id: "legacy-adoption-plasticity-parity".into(),
                top_k: 5,
                dimensions: vec!["structural".into(), "semantic".into()],
                xlr: false,
                include_ghost_edges: false,
                include_structural_holes: false,
                token_budget: None,
            },
        )
        .expect("activate");
        assert!(
            output.plasticity.edges_strengthened >= 1,
            "the query must strengthen at least one edge for this test to mean anything"
        );

        let after: Vec<u32> = state.graph.read().edge_plasticity.last_used_query.clone();
        let touched: Vec<(usize, u32)> = after
            .iter()
            .enumerate()
            .filter(|(slot, value)| before.get(*slot) != Some(*value))
            .map(|(slot, value)| (slot, *value))
            .collect();
        assert!(
            !touched.is_empty(),
            "a strengthened edge must restamp last_used_query"
        );
        for (slot, value) in touched {
            assert!(
                value >= restored_max,
                "just-used edge at CSR slot {slot} was stamped {value}, older than the restored \
                 maximum {restored_max}: the adoption left the orchestrator's plasticity engine \
                 with a zeroed query counter"
            );
        }
    }

    /// The refusal arm pins the other half of the same parity: a refused import
    /// must drop BOTH engines for clean ones. Leaving one restored and one fresh
    /// is the identical divergence, only inverted, and the adopting checkpoint
    /// publishes whichever engine it finds.
    #[test]
    fn refused_legacy_plasticity_leaves_both_engines_reset() {
        use m1nd_core::types::FiniteF32;

        let l = layout();
        write_legacy_pair_with_warm_plasticity(&l);
        // Overwrite the good sidecar with two rows sharing one full synaptic
        // key: parses fine, refused by `import_state` as a duplicate.
        let row = m1nd_core::plasticity::SynapticState {
            source_label: "file::src/lib.rs".into(),
            target_label: "file::src/core.rs".into(),
            relation: "imports".into(),
            direction: Some(0),
            inhibitory: Some(false),
            original_weight: 1.0,
            current_weight: 1.0,
            strengthen_count: 0,
            weaken_count: 0,
            ltp_applied: false,
            ltd_applied: false,
            last_used_query: 77,
        };
        m1nd_core::snapshot::save_plasticity_state(&[row.clone(), row], &l.legacy_plasticity)
            .expect("save duplicate-key legacy plasticity");

        let mut state = owner_session(&l.runtime_root);
        install_legacy_graph(&mut state, &l.legacy_graph, &l.legacy_plasticity)
            .expect("install the legacy graph");

        // A clean engine's first update stamps 1. Probe the two engines in turn
        // over the same co-activated pair: a counter that survived the refusal
        // in either one stamps far above that.
        let mut graph = state.graph.write();
        let lib = graph.resolve_id("file::src/lib.rs").expect("lib node");
        let core = graph.resolve_id("file::src/core.rs").expect("core node");
        let activated = vec![(lib, FiniteF32::new(0.9)), (core, FiniteF32::new(0.8))];

        state
            .plasticity
            .update(&mut graph, &activated, &activated, "probe-session-engine")
            .expect("probe the session engine");
        assert_eq!(
            graph.edge_plasticity.last_used_query.iter().max().copied(),
            Some(1),
            "a refused import must leave `state.plasticity` clean"
        );

        state
            .orchestrator
            .plasticity
            .update(
                &mut graph,
                &activated,
                &activated,
                "probe-orchestrator-engine",
            )
            .expect("probe the orchestrator engine");
        assert_eq!(
            graph.edge_plasticity.last_used_query.iter().max().copied(),
            Some(1),
            "a refused import must leave the orchestrator's engine clean too — a half-reset is \
             the same divergence as a half-import"
        );
    }
}
