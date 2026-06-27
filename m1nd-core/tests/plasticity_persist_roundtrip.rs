//! Round-trip + invariant coverage for plasticity-state persistence.
//!
//! `snapshot::{save_plasticity_state, load_plasticity_state}` is the production
//! autosave path for learned synaptic weights (m1nd-mcp session.rs +
//! persist_handlers.rs). It had ZERO test coverage — a silent regression here
//! would make the engine quietly forget everything it learned across restarts.
//!
//! This locks two contracts the implementation explicitly claims:
//!   * the full `SynapticState` survives save -> load unchanged;
//!   * FM-PL-001 — a non-finite `current_weight` is sanitized to
//!     `original_weight` at the save boundary (so a poisoned weight can never be
//!     written to disk and re-loaded as NaN).
//!
//! (Surfaced by the X-RAY proof-coverage pass.)

use m1nd_core::plasticity::SynapticState;
use m1nd_core::snapshot::{load_plasticity_state, save_plasticity_state};

fn state(source: &str, target: &str, current: f32) -> SynapticState {
    SynapticState {
        source_label: source.to_string(),
        target_label: target.to_string(),
        relation: "calls".to_string(),
        original_weight: 0.5,
        current_weight: current,
        strengthen_count: 3,
        weaken_count: 1,
        ltp_applied: true,
        ltd_applied: false,
    }
}

#[test]
fn plasticity_state_round_trips_all_fields() {
    let dir = std::env::temp_dir().join("m1nd_plasticity_rt");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plasticity.json");

    let original = vec![state("alpha", "beta", 0.81), state("beta", "gamma", 0.42)];
    save_plasticity_state(&original, &path).expect("save plasticity state");

    let loaded = load_plasticity_state(&path).expect("load plasticity state");
    assert_eq!(loaded.len(), original.len(), "state count must survive");

    for (a, b) in original.iter().zip(loaded.iter()) {
        assert_eq!(a.source_label, b.source_label);
        assert_eq!(a.target_label, b.target_label);
        assert_eq!(a.relation, b.relation);
        assert_eq!(a.original_weight, b.original_weight);
        assert_eq!(
            a.current_weight, b.current_weight,
            "learned weight must survive"
        );
        assert_eq!(a.strengthen_count, b.strengthen_count);
        assert_eq!(a.weaken_count, b.weaken_count);
        assert_eq!(a.ltp_applied, b.ltp_applied);
        assert_eq!(a.ltd_applied, b.ltd_applied);
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_sanitizes_non_finite_current_weight_to_original() {
    // FM-PL-001: a poisoned (NaN) current_weight must never reach disk; the save
    // boundary rewrites it to original_weight so load returns a finite value.
    let dir = std::env::temp_dir().join("m1nd_plasticity_nan_firewall");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plasticity.json");

    let poisoned = vec![state("alpha", "beta", f32::NAN)];
    save_plasticity_state(&poisoned, &path).expect("save must not fail on NaN");

    let loaded = load_plasticity_state(&path).expect("load sanitized state");
    assert_eq!(loaded.len(), 1);
    assert!(
        loaded[0].current_weight.is_finite(),
        "NaN current_weight must be sanitized at the save boundary"
    );
    assert_eq!(
        loaded[0].current_weight, loaded[0].original_weight,
        "sanitized weight must fall back to original_weight"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_missing_plasticity_file_is_error_not_panic() {
    let missing = std::env::temp_dir().join("m1nd_plasticity_missing_xyz.json");
    let _ = std::fs::remove_file(&missing);
    assert!(
        load_plasticity_state(&missing).is_err(),
        "loading a missing plasticity file must return Err, not panic"
    );
}
