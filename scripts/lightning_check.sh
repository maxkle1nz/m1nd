#!/usr/bin/env bash
# The LIGHTNING CHECK — the fast day-to-day signal over the never-cut core.
#
# THIS IS NOT THE MERGE GATE. It exists so the loop between two edits costs
# minutes, not an hour — and it buys that speed by NOT running most of the
# suite. The merge gate remains the FULL suite on three OSes, without
# exception; "the lightning passed" is never "the tests passed". The lane's
# contract, selector and budget live in docs/TEST-PORTFOLIO.md §4.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(bash scripts/cargo_target_dir.sh)}"

start=$SECONDS
# The never-cut core (selector pinned in .config/nextest.toml).
cargo nextest run --profile lightning --workspace --all-targets
# The two proofs nextest cannot carry:
# the 13 compile_fail sentinels (the candidate boundary)…
cargo test --locked -p m1nd-mcp --doc
# …and the lean edge feature unification hides from every workspace build.
cargo check --locked -p m1nd-mcp --no-default-features
elapsed=$(( SECONDS - start ))

echo
echo "LIGHTNING CHECK PASSED in ${elapsed}s — a fast day-to-day signal, NOT the merge gate."
echo "Deliberately NOT proven here: retrieval over this repo's real history (retrobuilder),"
echo "the transplant compiler oracles, 10k-op stress, wide property runs, the full grammar"
echo "matrix, the browser suites, and every OS that is not this machine."
echo "The merge gate is the FULL suite on 3 OSes. Portfolio + the never-cut fifteen:"
echo "docs/TEST-PORTFOLIO.md"
