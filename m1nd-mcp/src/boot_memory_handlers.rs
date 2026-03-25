use crate::session::{BootMemoryEntry, SessionState};
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

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
    match input.action.as_str() {
        "status" => Ok(status_payload(state)),
        "list" => {
            let mut entries: Vec<&BootMemoryEntry> = state.boot_memory.values().collect();
            entries.sort_by(|a, b| a.key.cmp(&b.key));
            Ok(json!({
                "status": "ok",
                "count": entries.len(),
                "path": state.boot_memory_path.to_string_lossy(),
                "entries": entries.into_iter().map(|entry| {
                    json!({
                        "key": entry.key,
                        "tags": entry.tags,
                        "source_refs": entry.source_refs,
                        "updated_at_ms": entry.updated_at_ms,
                        "updated_by_agent": entry.updated_by_agent,
                    })
                }).collect::<Vec<_>>(),
            }))
        }
        "get" => {
            let key = required_key(&input)?;
            match state.boot_memory.get(&key) {
                Some(entry) => Ok(json!({
                    "status": "ok",
                    "path": state.boot_memory_path.to_string_lossy(),
                    "entry": entry,
                })),
                None => Ok(json!({
                    "status": "missing",
                    "key": key,
                    "path": state.boot_memory_path.to_string_lossy(),
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

fn status_payload(state: &SessionState) -> Value {
    let mut keys: Vec<&String> = state.boot_memory.keys().collect();
    keys.sort();
    json!({
        "status": "ok",
        "count": state.boot_memory.len(),
        "path": state.boot_memory_path.to_string_lossy(),
        "keys": keys,
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use tempfile::tempdir;

    #[test]
    fn boot_memory_roundtrip_persists_to_disk() {
        let tmp = tempdir().unwrap();
        let graph_path = tmp.path().join("graph_snapshot.json");
        let plasticity_path = tmp.path().join("plasticity_state.json");
        let config = McpConfig {
            graph_source: graph_path,
            plasticity_state: plasticity_path,
            ..McpConfig::default()
        };

        let graph = Graph::new();
        let domain = DomainConfig::code();
        let mut state = SessionState::initialize(graph, &config, domain).unwrap();

        let input = BootMemoryInput {
            agent_id: "jimi".into(),
            action: "set".into(),
            key: Some("boot_doctrine".into()),
            value: Some(json!({"codex": "house"})),
            tags: vec!["boot".into()],
            source_refs: vec!["/Users/cosmophonix/SISTEMA/FIRESTARTER.md".into()],
        };
        handle_boot_memory(&mut state, input).unwrap();

        let raw = std::fs::read_to_string(tmp.path().join("boot_memory_state.json")).unwrap();
        assert!(raw.contains("boot_doctrine"));
        assert!(raw.contains("codex"));
    }
}
