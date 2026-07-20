//! End-to-end proof of the network-exposure bind gate (SECURITY #1).
//!
//! `--serve --bind <non-loopback>` must make the process REFUSE TO START with or
//! without the legacy `--allow-remote` flag — exit nonzero with a one-line refusal
//! on stderr, BEFORE the HTTP listener is created, before the graph loads, before
//! any lease is taken. Authenticated TLS remote transport is not implemented, so
//! no flag may downgrade this gate to a warning.
//!
//! We spawn the real built binary because only a spawned process can prove
//! `std::process::exit` actually fired. The refusal path binds NO port (it exits
//! first), so this test never opens a network listener — and it uses the loopback
//! default for everything else, never the maintainer's real runtime or port 1338.

use std::process::Command;

/// Path to the compiled binary under test. Cargo sets `CARGO_BIN_EXE_<name>`
/// for integration tests automatically.
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

fn run_wildcard_bind(allow_remote: bool) -> std::process::Output {
    let mut command = Command::new(BIN);
    command.arg("--serve").arg("--bind").arg("0.0.0.0");
    if allow_remote {
        command.arg("--allow-remote");
    }
    command
        // Keep the refusal path cheap and hermetic — it exits before any of this
        // matters, but set read-only so a regression that slips past the gate
        // cannot touch the developer's real runtime.
        .env("M1ND_READ_ONLY", "1")
        .output()
        .expect("spawn m1nd-mcp")
}

fn assert_authenticated_remote_refusal(output: &std::process::Output) {
    assert!(
        !output.status.success(),
        "non-loopback bind must exit nonzero, got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("REFUSING"),
        "expected refusal line on stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("authenticated remote transport"),
        "refusal must name the missing authenticated transport, got: {stderr}"
    );
    assert!(
        stderr.contains("--allow-remote cannot override"),
        "refusal must state that the legacy flag cannot bypass the gate, got: {stderr}"
    );
}

#[test]
fn serve_wildcard_bind_without_allow_remote_refuses_to_start() {
    assert_authenticated_remote_refusal(&run_wildcard_bind(false));
}

#[test]
fn serve_wildcard_bind_with_allow_remote_still_refuses_to_start() {
    assert_authenticated_remote_refusal(&run_wildcard_bind(true));
}
