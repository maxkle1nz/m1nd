//! End-to-end proof of the version-honesty strict gate.
//!
//! `M1ND_STRICT_VERSION=1` + an `M1ND_EXPECTED_VERSION` that does not match the
//! running binary must make the process REFUSE TO START — exit nonzero with a
//! one-line refusal on stderr, before any server loop, graph, or lease. This is
//! the hard guarantee for harnesses/experiments that must never run a stale
//! binary (the beta.8 incident). We spawn the real built binary because only a
//! spawned process can prove `std::process::exit` actually fired.

use std::process::Command;

/// Path to the compiled binary under test. Cargo sets `CARGO_BIN_EXE_<name>`
/// for integration tests automatically.
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

#[test]
fn strict_mode_version_mismatch_refuses_to_start() {
    let output = Command::new(BIN)
        .env("M1ND_STRICT_VERSION", "1")
        .env("M1ND_EXPECTED_VERSION", "0.0.0-beta.8")
        // Keep the refusal path cheap and hermetic — it exits before any of this
        // matters, but set them so a regression that slips past the gate can't
        // touch the developer's real runtime.
        .env("M1ND_READ_ONLY", "1")
        .env_remove("M1ND_EXPECTED_SHA")
        .output()
        .expect("spawn m1nd-mcp");

    assert!(
        !output.status.success(),
        "strict + version mismatch must exit nonzero, got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("STRICT VERSION REFUSAL"),
        "expected refusal line on stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("0.0.0-beta.8"),
        "refusal should name the mismatched expectation, got: {stderr}"
    );
}
