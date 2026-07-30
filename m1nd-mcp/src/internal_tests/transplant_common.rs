//! Shared setup for the transplant proof suites.
//!
//! `M1ND_PROOF_GATE` defaults ON (opt-out): every action carrying
//! `SOURCE_FILESYSTEM_WRITE` must consume an exact one-shot proof mark before it
//! may write. The transplant *logic* suites (battery, stress, harness, proptest,
//! selfhost, receipt_aging, node_identity, protected_zones, concurrency,
//! two_phase) exercise the VERB's behavior, not the gate, so they turn the gate
//! off here and keep their asserts on the move itself.
//!
//! The dedicated `transplant_proofgate` battery is the ONE suite that proves the
//! armed-gate contract, so it opts back IN by setting `M1ND_PROOF_GATE=1`.
//!
//! These suites live INSIDE the crate — they drive `dispatch_tool` and
//! `SessionState::initialize`, owner-internal seams the crate does not export —
//! which puts them in the same test process as the `surgical_handlers` armed-gate
//! probes. `M1ND_PROOF_GATE` is a process-global env var read live on every
//! dispatch, so a single arbiter below serializes every reader and writer of it:
//!
//!   * LOGIC suites take a SHARED lease ([`proof_gate_off_lease`]) — they all want
//!     the same value, so they still run in parallel with each other.
//!   * The armed-gate batteries (`transplant_proofgate` here, the
//!     `surgical_handlers` probes via [`arm_proof_gate_exclusively`]) take an
//!     EXCLUSIVE lease that restores the shared OFF baseline on drop, so no logic
//!     test can ever observe a transiently armed gate.

use std::sync::{Once, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// The process-global `M1ND_PROOF_GATE` arbiter.
fn arbiter() -> &'static RwLock<()> {
    static ARBITER: RwLock<()> = RwLock::new(());
    &ARBITER
}

/// Install the shared baseline (`M1ND_PROOF_GATE=false`) exactly once, under the
/// exclusive lease so the single `set_var` can never tear a concurrent live read.
fn install_off_baseline() {
    static PROOF_GATE_OFF: Once = Once::new();
    PROOF_GATE_OFF.call_once(|| {
        let _exclusive = arbiter().write().unwrap_or_else(|p| p.into_inner());
        std::env::set_var("M1ND_PROOF_GATE", "false");
    });
}

/// Take the SHARED lease that keeps the write proof gate disabled for a transplant
/// LOGIC test.
///
/// Hold the returned guard for the whole test body: it excludes the armed-gate
/// batteries (which arm the gate process-wide) while letting every other logic test
/// run concurrently. Bind it — `let _proof_gate = ...;`, never `let _ = ...;` — or
/// the lease is released on the spot and the exclusion is gone.
pub(crate) fn proof_gate_off_lease() -> RwLockReadGuard<'static, ()> {
    install_off_baseline();
    arbiter().read().unwrap_or_else(|p| p.into_inner())
}

/// Take the EXCLUSIVE lease for a battery that ARMS `M1ND_PROOF_GATE`.
///
/// Nothing else touching the gate runs while this guard lives, and dropping it
/// restores the shared OFF baseline the logic suites run under — so an armed
/// battery cannot leak its gate state into a later logic test. Recovers from
/// poison so one failing probe does not cascade-fail the others.
#[must_use]
pub(crate) fn arm_proof_gate_exclusively() -> ArmedProofGate {
    install_off_baseline();
    ArmedProofGate(arbiter().write().unwrap_or_else(|p| p.into_inner()))
}

/// Exclusive `M1ND_PROOF_GATE` lease; restores the shared OFF baseline on drop.
pub(crate) struct ArmedProofGate(#[allow(dead_code)] RwLockWriteGuard<'static, ()>);

impl Drop for ArmedProofGate {
    fn drop(&mut self) {
        std::env::set_var("M1ND_PROOF_GATE", "false");
    }
}
