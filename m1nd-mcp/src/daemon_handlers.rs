use crate::protocol::layers;
use crate::session::{DaemonAlert, SessionState};
use m1nd_core::error::{M1ndError, M1ndResult};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

pub fn handle_daemon_start(
    state: &mut SessionState,
    input: layers::DaemonStartInput,
) -> M1ndResult<serde_json::Value> {
    let started_at_ms = now_ms();
    state.daemon_state.active = true;
    state.daemon_state.started_at_ms = Some(started_at_ms);
    state.daemon_state.last_tick_ms = Some(started_at_ms);
    state.daemon_state.watch_paths = if input.watch_paths.is_empty() {
        state.ingest_roots.clone()
    } else {
        input.watch_paths
    };
    state.daemon_state.poll_interval_ms = input.poll_interval_ms;
    state.persist_daemon_state()?;
    Ok(json!({
        "status": "started",
        "active": true,
        "started_at_ms": started_at_ms,
        "watch_paths": state.daemon_state.watch_paths,
        "poll_interval_ms": state.daemon_state.poll_interval_ms,
    }))
}

pub fn handle_daemon_stop(
    state: &mut SessionState,
    _input: layers::DaemonStopInput,
) -> M1ndResult<serde_json::Value> {
    state.daemon_state.active = false;
    state.daemon_state.last_tick_ms = Some(now_ms());
    state.persist_daemon_state()?;
    Ok(json!({
        "status": "stopped",
        "active": false,
        "started_at_ms": state.daemon_state.started_at_ms,
        "last_tick_ms": state.daemon_state.last_tick_ms,
    }))
}

pub fn handle_daemon_status(
    state: &mut SessionState,
    _input: layers::DaemonStatusInput,
) -> M1ndResult<serde_json::Value> {
    Ok(json!({
        "active": state.daemon_state.active,
        "started_at_ms": state.daemon_state.started_at_ms,
        "last_tick_ms": state.daemon_state.last_tick_ms,
        "watch_paths": state.daemon_state.watch_paths,
        "poll_interval_ms": state.daemon_state.poll_interval_ms,
        "alert_count": state.daemon_alerts.len(),
        "runtime_root": state.runtime_root,
        "graph_generation": state.graph_generation,
        "cache_generation": state.cache_generation,
    }))
}

pub fn handle_alerts_list(
    state: &mut SessionState,
    input: layers::AlertsListInput,
) -> M1ndResult<serde_json::Value> {
    let mut alerts = state
        .daemon_alerts
        .iter()
        .filter(|alert| input.include_acked || !alert.acked)
        .cloned()
        .collect::<Vec<_>>();
    alerts.sort_by(|a, b| {
        b.created_at_ms
            .cmp(&a.created_at_ms)
            .then_with(|| a.alert_id.cmp(&b.alert_id))
    });
    alerts.truncate(input.limit);
    Ok(json!({
        "alerts": alerts,
        "total": alerts.len(),
        "active": state.daemon_state.active,
    }))
}

pub fn handle_alerts_ack(
    state: &mut SessionState,
    input: layers::AlertsAckInput,
) -> M1ndResult<serde_json::Value> {
    if input.alert_ids.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "alerts_ack".into(),
            detail: "Provide at least one alert_id.".into(),
        });
    }
    let acked_at_ms = now_ms();
    let mut acked = 0usize;
    for alert in &mut state.daemon_alerts {
        if input.alert_ids.iter().any(|id| id == &alert.alert_id) && !alert.acked {
            alert.acked = true;
            alert.acked_at_ms = Some(acked_at_ms);
            acked += 1;
        }
    }
    state.persist_daemon_alerts()?;
    Ok(json!({
        "acked": acked,
        "requested": input.alert_ids.len(),
        "acked_at_ms": acked_at_ms,
    }))
}

pub struct DaemonAlertSeed {
    pub severity: String,
    pub kind: String,
    pub message: String,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub suggested_tool: Option<String>,
    pub suggested_target: Option<String>,
    pub file_path: Option<String>,
    pub node_id: Option<String>,
}

pub fn make_daemon_alert(seed: DaemonAlertSeed) -> DaemonAlert {
    let created_at_ms = now_ms();
    DaemonAlert {
        alert_id: format!("alert-{}-{}", seed.kind, created_at_ms),
        severity: seed.severity,
        kind: seed.kind,
        message: seed.message,
        confidence: seed.confidence,
        evidence: seed.evidence,
        suggested_tool: seed.suggested_tool,
        suggested_target: seed.suggested_target,
        file_path: seed.file_path,
        node_id: seed.node_id,
        created_at_ms,
        acked: false,
        acked_at_ms: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpConfig;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;

    fn build_state() -> (tempfile::TempDir, SessionState) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        (temp, state)
    }

    #[test]
    fn daemon_lifecycle_and_alert_ack_roundtrip() {
        let (_temp, mut state) = build_state();

        let started = handle_daemon_start(
            &mut state,
            layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec!["/tmp/watch".into()],
                poll_interval_ms: 750,
            },
        )
        .expect("daemon start");
        assert_eq!(started["active"], true);
        assert_eq!(started["poll_interval_ms"], 750);

        let seeded = make_daemon_alert(DaemonAlertSeed {
            severity: "warning".into(),
            kind: "trust_drop".into(),
            message: "trust dropped".into(),
            confidence: 0.82,
            evidence: vec!["file::src/core.py".into()],
            suggested_tool: Some("trust".into()),
            suggested_target: Some("file::src/core.py".into()),
            file_path: Some("/tmp/watch/src/core.py".into()),
            node_id: Some("file::src/core.py".into()),
        });
        let seeded_id = seeded.alert_id.clone();
        state.record_daemon_alert(seeded);
        state
            .persist_daemon_alerts()
            .expect("persist daemon alerts");

        let listed = handle_alerts_list(
            &mut state,
            layers::AlertsListInput {
                agent_id: "test".into(),
                include_acked: false,
                limit: 10,
            },
        )
        .expect("alerts list");
        assert_eq!(listed["total"], 1);
        assert_eq!(listed["alerts"][0]["alert_id"], seeded_id);

        let acked = handle_alerts_ack(
            &mut state,
            layers::AlertsAckInput {
                agent_id: "test".into(),
                alert_ids: vec![seeded_id.clone()],
            },
        )
        .expect("alerts ack");
        assert_eq!(acked["acked"], 1);

        let hidden = handle_alerts_list(
            &mut state,
            layers::AlertsListInput {
                agent_id: "test".into(),
                include_acked: false,
                limit: 10,
            },
        )
        .expect("alerts list hidden");
        assert_eq!(hidden["total"], 0);

        let visible = handle_alerts_list(
            &mut state,
            layers::AlertsListInput {
                agent_id: "test".into(),
                include_acked: true,
                limit: 10,
            },
        )
        .expect("alerts list visible");
        assert_eq!(visible["total"], 1);
        assert_eq!(visible["alerts"][0]["acked"], true);

        let status = handle_daemon_status(
            &mut state,
            layers::DaemonStatusInput {
                agent_id: "test".into(),
            },
        )
        .expect("daemon status");
        assert_eq!(status["active"], true);
        assert_eq!(status["alert_count"], 1);

        let stopped = handle_daemon_stop(
            &mut state,
            layers::DaemonStopInput {
                agent_id: "test".into(),
            },
        )
        .expect("daemon stop");
        assert_eq!(stopped["active"], false);
    }
}
