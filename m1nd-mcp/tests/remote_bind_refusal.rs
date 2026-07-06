//! End-to-end proof of the network-exposure bind gate (SECURITY #1).
//!
//! `--serve --bind <non-loopback>` WITHOUT `--allow-remote` must make the process
//! REFUSE TO START — exit nonzero with a one-line refusal on stderr, BEFORE the
//! HTTP listener is created, before the graph loads, before any lease is taken.
//! An unguarded remote bind would expose graph mutation to the LAN with no auth,
//! so it must be a deliberate opt-in, never a mere warning.
//!
//! We spawn the real built binary because only a spawned process can prove
//! `std::process::exit` actually fired. The refusal path binds NO port (it exits
//! first), so this test never opens a network listener — and it uses the loopback
//! default for everything else, never the maintainer's real runtime or port 1338.

use std::process::Command;

/// Path to the compiled binary under test. Cargo sets `CARGO_BIN_EXE_<name>`
/// for integration tests automatically.
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

#[test]
fn serve_wildcard_bind_without_allow_remote_refuses_to_start() {
    let output = Command::new(BIN)
        .arg("--serve")
        .arg("--bind")
        .arg("0.0.0.0")
        // Keep the refusal path cheap and hermetic — it exits before any of this
        // matters, but set read-only so a regression that slips past the gate
        // cannot touch the developer's real runtime.
        .env("M1ND_READ_ONLY", "1")
        .output()
        .expect("spawn m1nd-mcp");

    assert!(
        !output.status.success(),
        "non-loopback bind without --allow-remote must exit nonzero, got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("REFUSING"),
        "expected refusal line on stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("--allow-remote"),
        "refusal should name the opt-in flag, got: {stderr}"
    );
}
