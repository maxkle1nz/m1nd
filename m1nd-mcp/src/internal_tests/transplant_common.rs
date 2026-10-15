//! Shared arbitration for the surviving source-writer proof-gate tests.
//!
//! The legacy filename is retained so every test that touches the process-global
//! `M1ND_PROOF_GATE` uses the same arbiter. The exclusive lease restores the
//! shared OFF baseline on drop; individual probes explicitly exercise both the
//! default-on policy and its opt-out behavior.

use std::sync::{Once, RwLock, RwLockWriteGuard};

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

/// Take the EXCLUSIVE lease for a battery that ARMS `M1ND_PROOF_GATE`.
///
/// Nothing else touching the gate runs while this guard lives, and dropping it
/// restores the shared OFF baseline — so an armed battery cannot leak its gate
/// state into a later probe. Recovers from
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
