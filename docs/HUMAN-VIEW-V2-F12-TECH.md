# HUMAN VIEW v2 — F12: the curation lane (propose-apply) — TECH AMENDMENT

> Status: **RATIFIED by the owner, 2026-07-11** (oracle-confronted the same day: the askGOD verdict
> that shaped this design returned CHANGE against the direct-write draft and demanded exactly
> this shape; its seven required changes are folded in below as signed decisions).
> Amends: F2.5 §5e (the hand-runner refusal), F11 §3a (the curation mission).
> Law that never bends: **the hand edits the candidate; only the human ratifies.**

## 0. The one-line design

The runnerd gains a **`/curate` lane in the exact image of `/name`**: the owner sends the
curation letter + packet to the daemon; the pinned hand-runner **PROPOSES a batch of
`candidate_edit` ops as data**; the **OWNER validates, sanitizes (o5) and applies** them
itself, seat `runner`, under OCC. The agent never holds a write surface — not REST, not
MCP, not a file. Propose-apply is already this codebase's trust posture ("the trust
boundary is the LLM, not the loopback" — naming_runner.rs); F12 generalizes it from one
name to a whole curation.

## 1. Why not direct write (the oracle's findings, signed into law)

The rejected draft gave the runner `candidate_edit` over REST (`?brain=` consent). The
oracle proved three holes the draft missed, each now a signed decision:

- **(1a) Ratify gets a mechanical guard BEFORE any autonomy** — `system_blocks_ratify`
  sat in the same `?brain=`-bypassable gate list with a free-string `ratifier` and no seat
  check: "no agent ratifies, ever" was paper. The guard ships as an ARC-1 prerequisite
  (the `ratified_via: "human-ui"` origin field + RED test; same-UID local malice stays
  outside the threat model, exactly as F2.5 §5d declares for worktrees).
- **(1b) o5 follows the data, not the lane** — `candidate_edit` rename/purpose with seat
  `runner` passes `sanitize_naming` server-side (violation = the batch refuses, naming the
  field and class). Ships as an ARC-1 prerequisite. Owner-seat edits stay unsanitized —
  the o5 law exists for LLM output, not for the owner's own fingers.
- **(1c) Propose-apply over direct-write** — the runner returning ops-as-data makes the
  blast radius (memorize, receipt_import, ratify, anything else on the REST surface)
  impossible MECHANICALLY, not doctrinally. No containment story needed for verbs the
  agent can never reach.

## 2. The `/curate` contract (daemon side — mirror of `/name`)

- `POST /curate` (loopback + the runnerd secret): `{ letter, packet, store_version,
  skeleton_id, blocks: [BlockCurationView…] }` — the owner composes the view the runner
  needs (block ids, names, purposes, members, confidence components, seams, unmapped);
  the runner never reads the store itself.
- The daemon resolves its PINNED `hand-runner` (from `runners.toml`, never from announce),
  runs the pinned command once with the packet on stdin, and expects on stdout **one JSON
  document**: `{ "schema": "m1nd-curation-proposal-v0", "ops": [CandidateEditOp…],
  "report": "<one honest paragraph: what it merged/named/resolved and what it left>" }`.
- Daemon-side hygiene (mirror of `/name`): parse + shape-validate HERE before the wire;
  a malformed proposal is an honest per-mission failure, never a partial apply.
- §5e relaxations, declared (the naming-runner precedent, config.rs): a hand-runner needs
  **no worktree** (curation never touches repo files) and **no gate_command** (its
  deterministic gate is the owner-side preflight + the human review that follows);
  `curation_timeout_secs` (default 300) bounds the run; one curation per brain at a time
  (the advisory lease is the signal).

## 3. The apply (owner side — where every law already lives)

On receiving the proposal the OWNER, in one motion: schema-validates the ops → sanitizes
every rename/purpose (o5, seat runner — 1b) → acquires the advisory `candidate_lease` as
the curating hand → applies the whole batch via the existing `candidate_edit` engine
(preflight-on-a-clone: one invalid op persists NOTHING) under the letter's
`expected_store_version` (OCC) → releases the lease → posts the **summary letter**
(seq+1, phase `judging`, the runner's `report` verbatim) into the mission chain. The
screen shows the curated candidate + the report; **the human reviews the RESULT and
ratifies** — the F11 §3a promise, now with a lane.

## 4. Dispatch and surfaces

- `mission_spawn` accepts capability `hand-runner` (the §5e refusal lifts ONLY for this
  shape); the F11-c "Send to an agent for curation" button upgrades from the DIRECT
  clipboard path to a real spawn when a hand-runner is pinned and announced — the DIRECT
  path remains as the no-runner fallback (honest banner, as today).
- Agent-facing surfaces updated in the same PR (era-coherence law): M1ND_INSTRUCTIONS §6,
  the three skills, AGENTS.md — "curation is propose-apply; the hand never holds a pen."

## 5. Dogfood (the arc's gate — target corrected per the verdict)

A FRESH candidate on one of the owner's unbaptized repos (scan → candidate → one curation
mission consumed end-to-end → the human ratifies the curated map). The pre-o6 sparse repo
stays OUT of scope — unlocking it is the `candidate_revision` promotion ceremony, its own
queue item. Success = the curated candidate reaches ratified with the owner touching only
the review screen and the ratify button.

## 6. Out of scope (named, not hidden)

Concurrent multi-runner curation; the revision-promotion ceremony; daemon-side process
sandboxing beyond the contract shape (same-UID malice stays outside the threat model,
§5d's line); curation of RATIFIED skeletons (1a law: candidate-only, unchanged).
