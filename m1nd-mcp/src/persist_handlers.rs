// === m1nd-mcp/src/persist_handlers.rs ===
// Persist/load handler with optional binary snapshots.

use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistInput {
    pub agent_id: String,
    pub action: String, // save | load | checkpoint | status
    #[serde(default)]
    pub format: Option<String>, // json | bin (default json)
}

/// The public persist verb may select an action and format, never a filesystem
/// location. Snapshot paths come only from the owner configuration. Refuse a
/// symlink at the managed file or its immediate directory before any load/write.
fn managed_snapshot_path(state: &SessionState, is_bin: bool) -> M1ndResult<PathBuf> {
    let path = if is_bin {
        state.graph_path.with_extension("bin")
    } else {
        state.graph_path.clone()
    };
    let parent = path.parent().ok_or_else(|| M1ndError::InvalidParams {
        tool: "persist".into(),
        detail: "owner-managed snapshot path has no parent directory".into(),
    })?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(M1ndError::Io)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(M1ndError::InvalidParams {
            tool: "persist".into(),
            detail: "owner-managed snapshot parent must be a real directory, not a symlink".into(),
        });
    }
    refuse_snapshot_symlink(&path)?;
    Ok(path)
}

fn refuse_snapshot_symlink(path: &Path) -> M1ndResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(M1ndError::InvalidParams {
            tool: "persist".into(),
            detail: "owner-managed snapshot is a symlink; refusing path escape".into(),
        }),
        Ok(metadata) if !metadata.is_file() => Err(M1ndError::InvalidParams {
            tool: "persist".into(),
            detail: "owner-managed snapshot exists but is not a regular file".into(),
        }),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(M1ndError::Io(error)),
    }
}

pub fn handle_persist(
    state: &mut SessionState,
    input: PersistInput,
) -> M1ndResult<serde_json::Value> {
    let fmt = input.format.as_deref().unwrap_or("json");
    let is_bin = fmt.eq_ignore_ascii_case("bin");

    // The sole path is owner-managed: graph_path (json) or its `.bin` sibling.
    let path = managed_snapshot_path(state, is_bin)?;

    match input.action.as_str() {
        "status" => {
            let g = state.graph.read();
            let result = serde_json::json!({
                "status": "ok",
                "nodes": g.num_nodes(),
                "edges": g.num_edges(),
                "queries_processed": state.queries_processed,
                "graph_path": state.graph_path.to_string_lossy(),
                "plasticity_path": state.plasticity_path.to_string_lossy(),
                "snapshot_path": path.to_string_lossy(),
                "format_supported": ["json", "bin"],
            });
            Ok(result)
        }
        "checkpoint" | "save" => {
            if is_bin {
                // Actor-aware: queue the derived BIN artifact until after
                // CURRENT, or write it directly for a legacy non-actor owner.
                let persisted_path = state.persist_binary_snapshot()?;
                debug_assert_eq!(persisted_path, path);
            } else {
                // Existing JSON path (graph + plasticity + antibodies)
                state.persist()?;
            }
            state.track_agent(&input.agent_id);
            Ok(serde_json::json!({
                "status": "saved",
                "graph_path": state.graph_path.to_string_lossy(),
                "plasticity_path": state.plasticity_path.to_string_lossy(),
                "bin_path": if is_bin { Some(path.to_string_lossy().to_string()) } else { None }
            }))
        }
        "load" => {
            // Load either JSON or BIN snapshot
            let graph = if is_bin {
                m1nd_core::snapshot_bin::load_graph(&path)?
            } else {
                m1nd_core::snapshot::load_graph(&path)?
            };

            // Swap graph and rebuild engines
            state.graph = Arc::new(parking_lot::RwLock::new(graph));
            state.rebuild_engines()?;
            state.bump_graph_generation();
            state.track_agent(&input.agent_id);

            let g = state.graph.read();
            Ok(serde_json::json!({
                "status": "loaded",
                "format": if is_bin { "bin" } else { "json" },
                "snapshot_path": path.to_string_lossy(),
                "nodes": g.num_nodes(),
                "edges": g.num_edges(),
            }))
        }
        other => Ok(serde_json::json!({
            "status": "error",
            "message": format!("Unknown action: {}", other)
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpConfig;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;

    fn state_in(root: &Path) -> SessionState {
        let runtime = root.join("runtime");
        std::fs::create_dir(&runtime).expect("runtime");
        let config = McpConfig {
            graph_source: runtime.join("graph.json"),
            plasticity_state: runtime.join("plasticity.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };
        SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("state")
    }

    #[test]
    fn public_path_override_is_rejected_for_load_and_save_without_io() {
        let temp = tempfile::tempdir().expect("tempdir");
        let sentinel = temp.path().join("outside-snapshot.json");
        std::fs::write(&sentinel, "sentinel\n").expect("sentinel");

        for action in ["load", "save", "checkpoint", "status"] {
            let request = serde_json::json!({
                "agent_id": "attacker",
                "action": action,
                "path": sentinel
            });
            let error = serde_json::from_value::<PersistInput>(request)
                .expect_err("legacy persist.path must fail closed");
            assert!(
                error.to_string().contains("unknown field `path`"),
                "unexpected {action} refusal: {error}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(&sentinel).expect("read sentinel"),
            "sentinel\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn managed_snapshot_symlink_is_refused_without_reading_or_writing_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = state_in(temp.path());
        let sentinel = temp.path().join("outside-snapshot.json");
        std::fs::write(&sentinel, "sentinel\n").expect("sentinel");
        symlink(&sentinel, &state.graph_path).expect("snapshot symlink");

        for action in ["load", "save"] {
            let error = handle_persist(
                &mut state,
                PersistInput {
                    agent_id: "owner".into(),
                    action: action.into(),
                    format: None,
                },
            )
            .expect_err("snapshot symlink must fail closed");
            assert!(error.to_string().contains("symlink"), "unexpected: {error}");
        }
        assert_eq!(
            std::fs::read_to_string(&sentinel).expect("read sentinel"),
            "sentinel\n"
        );
    }

    #[test]
    fn default_status_uses_only_owner_managed_snapshot_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = state_in(temp.path());
        let expected = state.graph_path.to_string_lossy().to_string();
        let result = handle_persist(
            &mut state,
            PersistInput {
                agent_id: "owner".into(),
                action: "status".into(),
                format: None,
            },
        )
        .expect("status");
        assert_eq!(result["snapshot_path"], expected);
    }
}
