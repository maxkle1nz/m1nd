//! Honesty regression: the cold-start `trust` lie must stay dead.
//!
//! On a node with NO defect-history evidence, m1nd used to emit a `trust_score`
//! of 0.5 with `TrustTier::Unknown`. That 0.5 reads to an agent like "50%
//! confidence" but is a meaningless neutral prior. Because `risk_multiplier`
//! equals 1.0 on that path it also made a cold-start node indistinguishable from
//! a verified-clean LowRisk node.
//!
//! These tests assert the NUMBER IS GONE on every agent-facing surface for the
//! no-evidence path (absent or null, never 0.5), that the action-routable band
//! `insufficient_evidence` is present instead, and that an evidenced node is now
//! genuinely distinguishable. They also lock the serde back-compat of the shared
//! `HeuristicSignals` wire struct (older JSON lacking the new fields still loads).
//!
//! Each `tests/*.rs` file is its own crate, so the `build_state`/`call` harness is
//! replicated here on purpose rather than imported.

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::protocol::layers::HeuristicSignals;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn build_state(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        runtime_dir: Some(root.to_path_buf()),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

fn call(state: &mut SessionState, tool: &str, params: Value) -> Value {
    dispatch_tool(state, tool, &params).expect("tool call")
}

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "m1nd_trustband_{tag}_{nanos}_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// THE regression test (asserts ABSENCE): a cold `trust` call must NOT surface a
/// numeric mean_trust, must carry `trust_band: "insufficient_evidence"`, and the
/// note must NOT contain the literal "0.5".
#[test]
fn handle_trust_cold_omits_the_number_and_carries_band() {
    let runtime = unique_dir("trust_cold");
    let mut state = build_state(&runtime);

    // Empty ledger → no-evidence path.
    let out = call(&mut state, "trust", json!({ "agent_id": "tester" }));

    // mean_trust is absent-or-null — the bare 0.5 is GONE.
    let mean_trust = out
        .get("summary")
        .and_then(|s| s.get("mean_trust"))
        .expect("summary.mean_trust key present");
    assert!(
        mean_trust.is_null(),
        "cold mean_trust must be null, got {mean_trust}"
    );

    // The action-routable band routes the agent instead of a number.
    assert_eq!(
        out.get("trust_band").and_then(Value::as_str),
        Some("insufficient_evidence"),
        "cold trust output must carry trust_band=insufficient_evidence"
    );

    // No numeric 0.5 anywhere in the serialized output, and not in the note.
    let serialized = serde_json::to_string(&out).unwrap();
    assert!(
        !serialized.contains("0.5"),
        "cold trust JSON must not contain the literal 0.5: {serialized}"
    );
    let note = out.get("note").and_then(Value::as_str).unwrap_or("");
    assert!(
        !note.is_empty(),
        "cold trust output must carry a routing note"
    );
    assert!(
        !note.contains("0.5"),
        "cold trust note must not contain the literal 0.5: {note}"
    );

    let _ = std::fs::remove_dir_all(&runtime);
}

/// Distinguishability + evidenced path: a never-seen node yields
/// `insufficient_evidence`; an evidenced node (defects recorded) yields a real
/// risk band with a numeric mean_trust. The two outputs must differ.
#[test]
fn evidenced_trust_is_distinguishable_from_cold() {
    let runtime = unique_dir("trust_evidenced");
    let mut state = build_state(&runtime);

    // Cold call first.
    let cold = call(&mut state, "trust", json!({ "agent_id": "tester" }));
    assert_eq!(
        cold.get("trust_band").and_then(Value::as_str),
        Some("insufficient_evidence")
    );
    assert!(cold
        .get("summary")
        .and_then(|s| s.get("mean_trust"))
        .unwrap()
        .is_null());

    // Seed real defect evidence (same public API existing tests use).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    for i in 0..3 {
        state
            .trust_ledger
            .record_defect("file::src/risky.rs", now - i as f64 * 60.0);
    }

    let evidenced = call(
        &mut state,
        "trust",
        json!({ "agent_id": "tester", "scope": "all", "min_history": 1 }),
    );

    // Now mean_trust is numeric and the top-level band is no longer cold.
    let mean = evidenced
        .get("summary")
        .and_then(|s| s.get("mean_trust"))
        .and_then(Value::as_f64);
    assert!(
        mean.is_some(),
        "evidenced mean_trust must be numeric, got {evidenced}"
    );
    assert_ne!(
        evidenced.get("trust_band").and_then(Value::as_str),
        Some("insufficient_evidence"),
        "evidenced output must not be insufficient_evidence"
    );

    // Per-node entry carries a risk band and a numeric trust_score (not absent).
    let nodes = evidenced
        .get("trust_scores")
        .and_then(Value::as_array)
        .expect("trust_scores array");
    let entry = nodes
        .iter()
        .find(|e| e.get("node_id").and_then(Value::as_str) == Some("file::src/risky.rs"))
        .expect("risky node present in report");
    let band = entry.get("trust_band").and_then(Value::as_str).unwrap();
    assert!(
        matches!(band, "high" | "medium" | "low"),
        "evidenced node band {band} must be a risk band"
    );
    assert!(
        entry.get("trust_score").and_then(Value::as_f64).is_some(),
        "evidenced node must keep a numeric trust_score"
    );

    // The two outputs genuinely differ (cold vs evidenced).
    assert_ne!(cold, evidenced);

    let _ = std::fs::remove_dir_all(&runtime);
}

/// HeuristicSignals (the shared wire struct) — the no-evidence surface OMITS the
/// numeric trust_score entirely (skip_serializing_if) and carries the band.
#[test]
fn heuristic_signals_cold_omits_score_field() {
    let cold = HeuristicSignals {
        heuristic_factor: 1.0,
        trust_score: None,
        trust_risk_multiplier: 1.0,
        trust_band: "insufficient_evidence".to_string(),
        trust_tier: "Unknown".to_string(),
        tremor_magnitude: None,
        tremor_observation_count: 0,
        tremor_risk_level: None,
        reason: "no evidence".to_string(),
    };
    let v = serde_json::to_value(&cold).unwrap();
    // The trust_score KEY must be absent on the cold path (not null, absent).
    assert!(
        v.get("trust_score").is_none(),
        "cold HeuristicSignals must omit the trust_score field entirely: {v}"
    );
    assert_eq!(
        v.get("trust_band").and_then(Value::as_str),
        Some("insufficient_evidence")
    );
    assert!(!serde_json::to_string(&v).unwrap().contains("0.5"));

    // Evidenced: the number is present as Some(_).
    let warm = HeuristicSignals {
        heuristic_factor: 1.0,
        trust_score: Some(0.82),
        trust_risk_multiplier: 1.2,
        trust_band: "low".to_string(),
        trust_tier: "LowRisk".to_string(),
        tremor_magnitude: None,
        tremor_observation_count: 0,
        tremor_risk_level: None,
        reason: "evidenced".to_string(),
    };
    let vw = serde_json::to_value(&warm).unwrap();
    assert!(vw.get("trust_score").and_then(Value::as_f64).is_some());
    assert_eq!(vw.get("trust_band").and_then(Value::as_str), Some("low"));
}

/// serde back-compat: a HeuristicSignals JSON string that LACKS the new fields
/// (`trust_score`, `trust_band`) must still deserialize (serde default).
#[test]
fn heuristic_signals_deserializes_legacy_json_without_new_fields() {
    // Old wire shape: no trust_score, no trust_band.
    let legacy = r#"{
        "heuristic_factor": 1.0,
        "trust_risk_multiplier": 1.0,
        "trust_tier": "Unknown",
        "tremor_observation_count": 0,
        "reason": "legacy"
    }"#;
    let parsed: HeuristicSignals =
        serde_json::from_str(legacy).expect("legacy HeuristicSignals must deserialize");
    assert!(parsed.trust_score.is_none());
    assert_eq!(parsed.trust_band, "");
    assert_eq!(parsed.trust_tier, "Unknown");
}
