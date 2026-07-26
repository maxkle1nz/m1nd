//! Shared setup for the transplant integration suites.
//!
//! `M1ND_PROOF_GATE` defaults ON (opt-out): every action carrying
//! `SOURCE_FILESYSTEM_WRITE` must consume an exact one-shot proof mark before it
//! may write. The transplant *logic* suites (battery, stress, harness, proptest,
//! selfhost, receipt_aging, node_identity, protected_zones, concurrency,
//! two_phase) exercise the VERB's behavior, not the gate, so they turn the gate
//! off here and keep their asserts on the move itself.
//!
//! The dedicated `transplant_proofgate` battery is the ONE suite that proves the
//! armed-gate contract: it lives in its own single-test binary (a separate
//! process) and opts back IN by setting `M1ND_PROOF_GATE=1`, so nothing here
//! touches it.

use std::sync::Once;

/// Disable the write proof gate for a transplant LOGIC suite, once per process.
///
/// `M1ND_PROOF_GATE` is read live from the environment on every dispatch, and the
/// suites run their `#[test]`s on parallel threads. A process-global [`Once`]
/// performs the single `set_var` before any test thread reaches `dispatch_tool`,
/// so every later live read observes a stable `"false"` — no torn read, no race.
/// Call it as the first line of each suite's state constructor.
pub fn disable_proof_gate_for_logic_tests() {
    static PROOF_GATE_OFF: Once = Once::new();
    PROOF_GATE_OFF.call_once(|| std::env::set_var("M1ND_PROOF_GATE", "false"));
}
