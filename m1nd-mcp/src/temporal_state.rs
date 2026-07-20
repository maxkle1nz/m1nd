//! Durable owner state for both runtime co-change matrices.
//!
//! `SessionState` owns two distinct matrices (the explicit temporal engine and
//! the query orchestrator fallback). Persisting only one silently changes
//! predictions after restart, so this sidecar commits both in one atomic,
//! graph-bound envelope.

use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::Graph;
use m1nd_core::temporal::{CoChangeMatrix, CoChangeMatrixStateV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub const TEMPORAL_STATE_FILE: &str = "temporal_state_v1.json";
const TEMPORAL_STATE_SCHEMA: &str = "m1nd-temporal-runtime-state-v1";
const TEMPORAL_STATE_VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct TemporalRuntimeStateV1 {
    schema: String,
    version: u32,
    primary: CoChangeMatrixStateV1,
    orchestrator: CoChangeMatrixStateV1,
    state_digest: String,
}

#[derive(Serialize)]
struct DigestView<'a> {
    schema: &'a str,
    version: u32,
    primary: &'a CoChangeMatrixStateV1,
    orchestrator: &'a CoChangeMatrixStateV1,
}

fn digest(state: &TemporalRuntimeStateV1) -> M1ndResult<String> {
    let bytes = serde_json::to_vec(&DigestView {
        schema: &state.schema,
        version: state.version,
        primary: &state.primary,
        orchestrator: &state.orchestrator,
    })?;
    let mut hasher = Sha256::new();
    hasher.update(b"m1nd/temporal-runtime-state/v1\0");
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn durable_atomic_write(path: &Path, bytes: &[u8], fail_before_rename: bool) -> M1ndResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            M1ndError::PersistenceFailed("temporal state path has no filename".into())
        })?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_path = parent.join(format!(
        ".{file_name}.tmp-{}-{sequence}",
        std::process::id()
    ));
    let result = (|| -> M1ndResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if fail_before_rename {
            return Err(M1ndError::PersistenceFailed(
                "injected temporal commit interruption".into(),
            ));
        }
        std::fs::rename(&temp_path, path)?;
        // Windows refuses fsync on directory handles; write-through covers renames.
        #[cfg(not(windows))]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Encode both graph-bound temporal matrices without touching the working
/// filesystem. This is the candidate-first checkpoint representation and is
/// byte-deterministic for the same graph and matrix states.
pub fn encode_temporal_state(
    graph: &Graph,
    primary: &CoChangeMatrix,
    orchestrator: &CoChangeMatrix,
) -> M1ndResult<Vec<u8>> {
    let mut state = TemporalRuntimeStateV1 {
        schema: TEMPORAL_STATE_SCHEMA.into(),
        version: TEMPORAL_STATE_VERSION,
        primary: primary.export_state(graph)?,
        orchestrator: orchestrator.export_state(graph)?,
        state_digest: String::new(),
    };
    state.state_digest = digest(&state)?;
    Ok(serde_json::to_vec_pretty(&state)?)
}

/// Decode and strictly validate an in-memory temporal checkpoint payload.
pub fn decode_temporal_state(
    bytes: &[u8],
    graph: &Graph,
) -> M1ndResult<(CoChangeMatrix, CoChangeMatrix)> {
    let state: TemporalRuntimeStateV1 = serde_json::from_slice(bytes)?;
    if state.schema != TEMPORAL_STATE_SCHEMA || state.version != TEMPORAL_STATE_VERSION {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "unsupported temporal runtime state: schema={}, version={}",
                state.schema, state.version
            ),
        });
    }
    let actual_digest = digest(&state)?;
    if state.state_digest != actual_digest {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "temporal runtime state digest mismatch: declared={}, actual={actual_digest}",
                state.state_digest
            ),
        });
    }
    let primary = CoChangeMatrix::from_state(graph, state.primary)?;
    let orchestrator = CoChangeMatrix::from_state(graph, state.orchestrator)?;
    Ok((primary, orchestrator))
}

/// Atomically commit both matrices. Failure is fatal to the owning checkpoint:
/// graph bytes must not be advertised as a complete save without their temporal
/// evidence.
pub fn save_temporal_state(
    path: &Path,
    graph: &Graph,
    primary: &CoChangeMatrix,
    orchestrator: &CoChangeMatrix,
) -> M1ndResult<()> {
    let bytes = encode_temporal_state(graph, primary, orchestrator)?;
    durable_atomic_write(path, &bytes, false)
}

/// Strictly restore both matrices, or `None` only when no state has ever been
/// persisted. A present but malformed, unknown, digest-invalid, or graph-stale
/// file is an error and never degrades to bootstrap state.
pub fn load_temporal_state(
    path: &Path,
    graph: &Graph,
) -> M1ndResult<Option<(CoChangeMatrix, CoChangeMatrix)>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    decode_temporal_state(&bytes, graph).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use m1nd_core::types::{NodeId, NodeType};

    fn test_graph(node_count: usize) -> Graph {
        let mut graph = Graph::new();
        for index in 0..node_count {
            graph
                .add_node(
                    &format!("node_{index}"),
                    &format!("node_{index}"),
                    NodeType::File,
                    &[],
                    0.0,
                    0.0,
                )
                .expect("add node");
        }
        graph.finalize().expect("finalize");
        graph
    }

    fn learned_pair(graph: &Graph, repeats: usize) -> CoChangeMatrix {
        let mut matrix = CoChangeMatrix::bootstrap(graph, 128).expect("bootstrap");
        for _ in 0..repeats {
            matrix.note_node_appearance(NodeId::new(0));
            matrix.note_node_appearance(NodeId::new(1));
            matrix
                .record_co_change(NodeId::new(0), NodeId::new(1), 0.0)
                .expect("learn");
        }
        matrix
    }

    #[test]
    fn restart_restores_both_matrices_exactly() {
        let graph = test_graph(3);
        let primary = learned_pair(&graph, 2);
        let orchestrator = learned_pair(&graph, 5);
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TEMPORAL_STATE_FILE);
        let encoded = encode_temporal_state(&graph, &primary, &orchestrator).expect("encode");
        assert_eq!(
            encoded,
            encode_temporal_state(&graph, &primary, &orchestrator).expect("deterministic encode")
        );
        save_temporal_state(&path, &graph, &primary, &orchestrator).expect("save");
        assert_eq!(std::fs::read(&path).expect("saved bytes"), encoded);
        let (restored_primary, restored_orchestrator) = load_temporal_state(&path, &graph)
            .expect("load")
            .expect("present");
        assert_eq!(
            restored_primary.predict(NodeId::new(0), 8),
            primary.predict(NodeId::new(0), 8)
        );
        assert_eq!(
            restored_orchestrator.predict(NodeId::new(0), 8),
            orchestrator.predict(NodeId::new(0), 8)
        );
    }

    #[test]
    fn corruption_unknown_version_and_graph_drift_fail_closed() {
        let graph = test_graph(2);
        let matrix = learned_pair(&graph, 1);
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TEMPORAL_STATE_FILE);
        save_temporal_state(&path, &graph, &matrix, &matrix).expect("save");
        let pristine = std::fs::read(&path).expect("read");

        let mut state: TemporalRuntimeStateV1 = serde_json::from_slice(&pristine).expect("decode");
        state.primary.total_entries += 1;
        std::fs::write(&path, serde_json::to_vec(&state).expect("encode")).expect("tamper");
        assert!(load_temporal_state(&path, &graph).is_err());

        let mut state: TemporalRuntimeStateV1 = serde_json::from_slice(&pristine).expect("decode");
        state.version += 1;
        std::fs::write(&path, serde_json::to_vec(&state).expect("encode")).expect("tamper");
        assert!(load_temporal_state(&path, &graph).is_err());

        std::fs::write(&path, &pristine[..pristine.len() / 2]).expect("truncate");
        assert!(load_temporal_state(&path, &graph).is_err());

        std::fs::write(&path, pristine).expect("restore");
        assert!(load_temporal_state(&path, &test_graph(3)).is_err());
    }

    #[test]
    fn interrupted_commit_preserves_previous_valid_generation() {
        let graph = test_graph(2);
        let old = learned_pair(&graph, 1);
        let new = learned_pair(&graph, 6);
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TEMPORAL_STATE_FILE);
        save_temporal_state(&path, &graph, &old, &old).expect("old save");

        let bytes = encode_temporal_state(&graph, &new, &new).expect("encode new");
        assert!(durable_atomic_write(&path, &bytes, true).is_err());
        let (primary, orchestrator) = load_temporal_state(&path, &graph)
            .expect("old remains readable")
            .expect("present");
        assert_eq!(
            primary.predict(NodeId::new(0), 8),
            old.predict(NodeId::new(0), 8)
        );
        assert_eq!(
            orchestrator.predict(NodeId::new(0), 8),
            old.predict(NodeId::new(0), 8)
        );
    }
}
