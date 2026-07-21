# M1ND-10 G7 LIVE first real run and the gate-dependency finding — 2026-07-20

> Result: G7 LIVE `NOT_PROVEN`. Not a fail-closed precondition refusal and not a PASS — the first
> true end-to-end run of the gate (which the program always listed `NOT_RUN`) exposed one harness
> bug (now fixed) and then one structural truth: G7 completion depends on provenance that only G8
> (signed release) and G9 (installed production authority) can produce.

## What ran, on what

- Binary: `target/release/m1nd-mcp` built from clean `main` at `68b50e18`, version `1.4.0`,
  digest `d71a42d8449f40ebb3d7…` (build embeds the source commit, so the digest tracks HEAD).
- Orchestrator: `scripts/m1nd10_g7_live_orchestrator.py` — boots one isolated read-only owner on a
  kernel-selected non-1338 loopback port, verifies exact source/binary/UI/browser identities,
  runs the real `npm run test:e2e:live` lane, emits a token-free receipt, terminates every process
  group it creates. It never contacts the installed owner or port 1338.
- Environment: profile `cosmophonix` (ported from `kle1nz`); the served owner/1338 and the m1nd MCP
  belong to the `kle1nz` profile and were untouched (fail-open, as the port doctrine requires).

## The harness bug the first run exposed (fixed)

`execute()` passes a pre-created `TemporaryDirectory` to `stage_git_ui_tree`, whose
`destination.mkdir()` assumed a fresh path and raised `FileExistsError`. The 18 unit tests never
covered `execute()`'s pre-created destination (they staged into a fresh subdirectory), so the gate
had genuinely never run end-to-end. Fixed at `scripts/m1nd10_g7_live_orchestrator.py:498`
(`mkdir(..., exist_ok=True)`; the emptiness guard is on `destination/ui-harness`, the tree actually
materialized) with a regression proven RED before / GREEN after. Committed as `68b50e18`.

## How far the real gate reached (all green up to the manifest)

With absolute paths and a clean source, the gate accepted every identity and prepared every input:

- binary digest verified; source UI digest `7b7904c8…` verified;
- Chromium bundle attested byte-for-byte: revision 1228, version 149.0.7827.55, 338 files,
  359,442,012 bytes, digest `a9a6fd31…` matching the pinned lock; executable digest sealed;
- `npm ci --offline --ignore-scripts` exit 0 over 340 locked dependencies, no token in output;
- harness materialized from `git archive` (265 files); `node_modules` sealed.

## Where it stopped, and why it is structural

The isolated owner booted and served `/api/manifest`, and **the owner itself classified its own
manifest coherence as `DRIFT`**. The orchestrator requires `COHERENT`
(`m1nd10_g7_live_orchestrator.py:1120`) and refused with `manifest_not_coherent`. The owner's own
verification issues, read directly from a locally booted isolated owner (never 1338), are:

- `required digest 'architecture.skeleton_digest' is absent` — a fresh owner has no ratified skeleton;
- `required digest 'release_provenance.release_candidate_digest' is absent`;
- `release provenance signature is absent; G1 does not synthesize one`;
- `active_mode is not listed in supported_modes`;
- repeated `authority is unavailable or unknown` / `authority freshness is unknown`.

This is the organism refusing to fake a provenance chain it does not have. A freshly built binary
with no ratified skeleton, no signed release candidate, and no installed production authority is
**honestly** in `DRIFT`. The refusal is correct behavior, not a defect.

## The finding: the top gates are one provenance chain, not independent steps

The same production-authority requirement blocks the formal G6 blind run:
`scripts/benchmark/m1nd10_g6_blind_runner.py:2654` — *"formal run requires a pinned production
authority assembly"* (a `software_test` provider runs but permanently marks the run non-formal),
and the scorer consumes operator-only labels, which the implementer/reviewer role never opens.

Therefore, as measured on 2026-07-20:

- **G7 completion** requires a `COHERENT` manifest → requires a signed release candidate (G8) and
  installed production authority (G9).
- **G6 formal** requires a pinned production authority assembly (G9) and operator-held labels.
- **G8** is the signing/publishing ceremony that mints `release_candidate_digest`.
- **G9** is the installed production custody (hardware-protected signers, quorum, sentinel) that is
  documented `NOT_INSTALLED`.

The real remaining frontier is **G9 custody**, and the branch above it (G6-formal, G7-complete, G8)
converges on it. This is the owner's standing decision point (hardware custody vs. a ratified
hardened-software floor), exactly as the guardian method and PATHOS already anticipated.

## What is not blocked (and is already proven)

Retrieval quality — the "is the product good?" question — was already measured and won:
`docs/benchmarks/m1nd10-g6-current-report.json` (2026-07-18, corpus v1, 200 tasks) is
`claimable: true` with all six checks green (abstention recall 0.95, top-5 anchor recall passed,
`north` p95 1.086 ms, `seek` p95 237 ms, 102 paired improvements vs 5 regressions, sign test
p ≈ 1.4e-24 against the `rg`/Read baseline). The formal re-seal over the immutable candidate is
what waits on the authority chain — not the quality result itself.

## Repository state

`git status --porcelain` = clean; HEAD `68b50e18`; no 1338 contact; no orphaned `m1nd-mcp` process;
no leaked temporaries (`ephemeral_state_removed: true`, `owner_process_group_terminated: true`).
Receipts under the session scratchpad, not the repository.
