# Benchmarks and Release Ceremonies

This directory holds the public benchmark contracts, formal ceremony runbooks,
and public G6 corpus definitions for `m1nd`.

## Contents

- `m1nd10-g6-metric-spec-v1.json`: G6 retrieval and abstention metric contract.
- `m1nd10-g6-held-out-v2/`: public G6 held-out v2 corpus queries, source manifest,
  digests, and schemas (`operator-only/` labels and `runner-results/` stay
  operator-private and gitignored).
- `m1nd10-g6-generalization-v2/`: public G6 generalization v2 corpus queries,
  source manifest, and digests.
- `M1ND10_G6_RETRIEVAL_PROTOCOL.md`: G6 retrieval and abstention gate protocol.
- `G6-FORMAL-CEREMONY.md`: owner-run G6 formal blind ceremony runbook.
- `G7-LIVE-CEREMONY.md`: G7 live UI and accessibility ceremony specification.
- `G9-CUSTODY-CEREMONY.md`: G9 Secure Enclave custody ceremony specification.
- `G9-PLATFORM-DECISION.md`: G9 macOS signing and distribution decision record.
- `M1ND-10-OWNER-SITTING.md`: owner-sitting ceremony checklist.

## Runner and Preflight Scripts

Under `scripts/benchmark/`:

- `g6_formal_preflight.sh`: verifies public G6 artifacts, frozen pins, and
  corpus snapshot integrity before an owner formal run.
- `g6_formal_run.sh`: executes the formal G6 blind evaluation ceremony (supports
  `--dry-run` for non-claimable local verification).
- `m1nd10_g6_blind_runner.py`: sealed corpus runner and snapshot verifier.
- `m1nd10_g6_retrieval.py`: G6 retrieval and abstention scorer.
- `m1nd10_g6_generalization_score.py`: G6 generalization scorer.
