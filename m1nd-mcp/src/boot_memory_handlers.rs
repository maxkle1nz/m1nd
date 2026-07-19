use crate::session::{BootMemoryEntry, SessionState};
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct BootMemoryInput {
    pub agent_id: String,
    pub action: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
}

pub fn handle_boot_memory(state: &mut SessionState, input: BootMemoryInput) -> M1ndResult<Value> {
    let migration = crate::boot_kv_migration::migration_status(&state.runtime_root)?;
    let migrated_entries = if migration.is_some() {
        crate::boot_kv_migration::compatibility_entries(&state.runtime_root)?.unwrap_or_default()
    } else {
        Vec::new()
    };
    if migration.is_some() && matches!(input.action.as_str(), "set" | "delete") {
        return Ok(json!({
            "status": "retired",
            "requested_action": input.action,
            "writes_allowed": false,
            "legacy_path": state.boot_memory_path.to_string_lossy(),
            "migration": migration.as_ref(),
            "route": {
                "semantic_memory": "memorize",
                "configuration": crate::boot_kv_migration::BOOT_CONFIG_FILE,
            },
            "note": "Boot KV is a read-only compatibility surface after conservation migration; no dual-write occurred.",
        }));
    }
    match input.action.as_str() {
        "status" => Ok(status_payload(state, migration, &migrated_entries)),
        "list" => {
            let entries = if migration.is_some() {
                migrated_entries
                    .iter()
                    .map(|row| {
                        json!({
                            "key": row.entry.key,
                            "tags": row.entry.tags,
                            "source_refs": row.entry.source_refs,
                            "updated_at_ms": row.entry.updated_at_ms,
                            "updated_by_agent": row.entry.updated_by_agent,
                            "storage": row.storage,
                            "migration_target": row.target,
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                let mut legacy: Vec<&BootMemoryEntry> = state.boot_memory.values().collect();
                legacy.sort_by(|a, b| a.key.cmp(&b.key));
                legacy
                    .into_iter()
                    .map(|entry| {
                        json!({
                            "key": entry.key,
                            "tags": entry.tags,
                            "source_refs": entry.source_refs,
                            "updated_at_ms": entry.updated_at_ms,
                            "updated_by_agent": entry.updated_by_agent,
                            "storage": "legacy",
                        })
                    })
                    .collect::<Vec<_>>()
            };
            Ok(json!({
                "status": "ok",
                "count": entries.len(),
                "path": state.boot_memory_path.to_string_lossy(),
                "entries": entries,
                "storage": if migration.is_some() { "retired" } else { "legacy" },
                "migration": migration.as_ref(),
            }))
        }
        "get" => {
            let key = required_key(&input)?;
            if migration.is_some() {
                return match migrated_entries.iter().find(|row| row.entry.key == key) {
                    Some(row) => Ok(json!({
                        "status": "migrated",
                        "path": state.boot_memory_path.to_string_lossy(),
                        "entry": row.entry,
                        "storage": row.storage,
                        "migration_target": row.target,
                        "migration": migration.as_ref(),
                    })),
                    None => Ok(json!({
                        "status": "missing",
                        "key": key,
                        "path": state.boot_memory_path.to_string_lossy(),
                        "storage": "retired",
                        "migration": migration.as_ref(),
                    })),
                };
            }
            match state.boot_memory.get(&key) {
                Some(entry) => Ok(json!({
                    "status": "ok",
                    "path": state.boot_memory_path.to_string_lossy(),
                    "entry": entry,
                    "storage": "legacy",
                })),
                None => Ok(json!({
                    "status": "missing",
                    "key": key,
                    "path": state.boot_memory_path.to_string_lossy(),
                    "storage": "legacy",
                })),
            }
        }
        "set" => {
            let key = required_key(&input)?;
            let value = input.value.ok_or_else(|| M1ndError::InvalidParams {
                tool: "boot_memory".into(),
                detail: "action=set requires value".into(),
            })?;
            let entry = BootMemoryEntry {
                key: key.clone(),
                value,
                tags: input.tags,
                source_refs: input.source_refs,
                updated_at_ms: now_ms(),
                updated_by_agent: input.agent_id.clone(),
            };
            state.boot_memory.insert(key.clone(), entry.clone());
            state.persist_boot_memory()?;
            state.track_agent(&input.agent_id);
            Ok(json!({
                "status": "saved",
                "path": state.boot_memory_path.to_string_lossy(),
                "entry": entry,
            }))
        }
        "delete" => {
            let key = required_key(&input)?;
            let existed = state.boot_memory.remove(&key).is_some();
            state.persist_boot_memory()?;
            state.track_agent(&input.agent_id);
            Ok(json!({
                "status": if existed { "deleted" } else { "missing" },
                "key": key,
                "path": state.boot_memory_path.to_string_lossy(),
            }))
        }
        other => Err(M1ndError::InvalidParams {
            tool: "boot_memory".into(),
            detail: format!("unknown action: {}", other),
        }),
    }
}

fn required_key(input: &BootMemoryInput) -> M1ndResult<String> {
    input.key.clone().ok_or_else(|| M1ndError::InvalidParams {
        tool: "boot_memory".into(),
        detail: format!("action={} requires key", input.action),
    })
}

fn status_payload(
    state: &SessionState,
    migration: Option<Value>,
    migrated_entries: &[crate::boot_kv_migration::MigratedCompatibilityEntry],
) -> Value {
    let keys = if migration.is_some() {
        migrated_entries
            .iter()
            .map(|row| row.entry.key.clone())
            .collect::<Vec<_>>()
    } else {
        let mut keys = state.boot_memory.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        keys
    };
    json!({
        "status": "ok",
        "count": keys.len(),
        "path": state.boot_memory_path.to_string_lossy(),
        "keys": keys,
        "storage": if migration.is_some() { "retired" } else { "legacy" },
        "writes_allowed": migration.is_none(),
        "migration": migration.as_ref(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpConfig;
    use crate::session::{BootMemoryState, SessionState};
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use tempfile::tempdir;

    #[test]
    fn boot_memory_migrates_then_refuses_hidden_dual_write() {
        let tmp = tempdir().unwrap();
        let graph_path = tmp.path().join("graph_snapshot.json");
        let plasticity_path = tmp.path().join("plasticity_state.json");
        let config = McpConfig {
            graph_source: graph_path,
            plasticity_state: plasticity_path,
            ..McpConfig::default()
        };

        let legacy = BootMemoryState {
            entries: [(
                "boot_doctrine".into(),
                BootMemoryEntry {
                    key: "boot_doctrine".into(),
                    value: json!({"codex": "house"}),
                    tags: vec!["boot".into()],
                    source_refs: vec!["~/notes.md".into()],
                    updated_at_ms: 1,
                    updated_by_agent: "agent-a".into(),
                },
            )]
            .into_iter()
            .collect(),
        };
        std::fs::write(
            tmp.path().join("boot_memory_state.json"),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let graph = Graph::new();
        let domain = DomainConfig::code();
        let mut state = SessionState::initialize(graph, &config, domain).unwrap();

        let input = BootMemoryInput {
            agent_id: "agent-a".into(),
            action: "set".into(),
            key: Some("boot_doctrine".into()),
            value: Some(json!({"codex": "house"})),
            tags: vec!["boot".into()],
            source_refs: vec!["~/notes.md".into()],
        };
        let response = handle_boot_memory(&mut state, input).unwrap();
        assert_eq!(response["status"], "retired");
        assert_eq!(response["writes_allowed"], false);

        let get = handle_boot_memory(
            &mut state,
            BootMemoryInput {
                agent_id: "agent-a".into(),
                action: "get".into(),
                key: Some("boot_doctrine".into()),
                value: None,
                tags: vec![],
                source_refs: vec![],
            },
        )
        .unwrap();
        assert_eq!(get["status"], "migrated");
        assert_eq!(get["entry"]["value"]["codex"], "house");
        assert_eq!(get["storage"], "migrated_light");
        assert!(get["migration_target"]
            .as_str()
            .unwrap()
            .ends_with(".light.md"));

        let status = handle_boot_memory(
            &mut state,
            BootMemoryInput {
                agent_id: "agent-a".into(),
                action: "status".into(),
                key: None,
                value: None,
                tags: vec![],
                source_refs: vec![],
            },
        )
        .unwrap();
        assert_eq!(status["count"], 1);
        assert_eq!(status["keys"][0], "boot_doctrine");
        assert_eq!(status["storage"], "retired");

        let raw = std::fs::read_to_string(tmp.path().join("boot_memory_state.json")).unwrap();
        assert!(!raw.contains("boot_doctrine"));
        assert!(!raw.contains("codex"));
        assert!(tmp.path().join("boot_kv_migration_v1.json").exists());
        let lights = std::fs::read_dir(tmp.path().join("agent-memory"))
            .unwrap()
            .filter_map(Result::ok)
            .count();
        assert_eq!(lights, 1);
    }
}
