# GENESIS — Ingest Consumers Spec (P2/P3)

**Status: DRAFT — awaiting askGOD verdict before any implementation.**
Provenance: the checkpoint-32 panel (P1→P2→P3 ratified order, the 12 P3 requirements) plus the
2026-07-25 adoption lab, whose receipts re-scoped the wave. Author seat: guardian. Nothing in
this document authorizes code; it exists to be confronted.

## 0 · The measured reality this spec stands on

Every claim here was measured, not assumed. The lab: a copy of the real runtime root
(17,408 nodes / 70,377 edges / 54 memory claims), the current-main binary, serve mode,
authenticated REST — the deployment shape.

- **R-A (adoption works).** The current-main binary booting on a populated runtime root loads
  the full brain. Migration of the machine's brain needs NO new verb — it is a boot-time fact
  (`legacy_snapshot_adoption.rs`). The 2026-07-24 "boots empty" lab tested an empty root — it
  measured *birth*, not *adoption*.
- **R-B (the daily loop is alive).** `north`, `seek`, `memorize` all answer on current-main
  from the covering root. Zero authority walls on the read/memory loop.
- **R-C (all ingest is walled).** Measured refusals, verbatim class:
  `generic_action_authority_required: semantic_action=graph.ingest.replace
  authority_floor=POSITIVE_SOVEREIGN … no exact typed G2/G3 lease consumer is installed`
  — and `merge` at `SCOPED_GRANT_A2` refuses identically. Consequence: on current-main the
  graph is FROZEN — it cannot absorb the repo's own daily merges.
- **R-D (the hijack class is real).** Twice in 24h a foreign-root session's plain
  `ingest {path}` on the deployed 1.4.0 owner wholesale-replaced the bound brain
  (29,005→9,293; then 17,407→2,724). Both restored; both prove the 1.4.x door must close.
- **R-E (perf is a separate front).** Current-main read-path regression (seek 0.12s → 5–62s)
  is being hunted independently. This spec deliberately does not touch it.

The tension this spec resolves: **R-C freezes freshness to close R-D — but freshness and
hijack-safety are not the same door.** The refusal treats "the owner refreshing the root it
already covers" and "a stranger replacing someone else's brain" as one action. They are not.

## 1 · The taxonomy split (the design)

Today `classify_ingest` maps every rootful ingest to `graph.ingest.replace|merge_existing`
(one sovereign/A2 wall) or `brain.bootstrap` (no consumer). The split:

### SPEC-1 — `graph.ingest.refresh_covered_root` (NEW action, the freshness door)

- **Definition:** an ingest whose resolved canonical caller root is EXACTLY a root the
  serving brain already covers (`covers_root` — workspace root or registered ingest root),
  and whose execution cannot add, remove, or change any root in the brain's root set.
- **Floor:** `ScopedGrantA2`, admitted by a LOCAL typed consumer (no G2/G3 crypto plane
  required), reusing the existing `graph_ingest_a2` machinery: payload schema + digest,
  external-mutation journal entry, candidate artifact, recovery kind. The module's header
  already declares this ownership split; the consumer supplies the admission it lacks.
- **Why the lower floor is honest (grok req: "distinct action if lower floor"):** this action
  is *structurally incapable* of the R-D incident — a foreign caller's root does not satisfy
  `covers_root`, so the action refuses before any mutation, with the SAME refusal a hijacker
  sees today. Lowering the floor of an action that cannot cross brains does not lower the
  floor of any action that can.
- **Guards (each one a test):**
  - SPEC-1a: caller root resolved server-side (the `M1nd-Caller-Root` seam), never from a
    client-supplied path argument alone; path argument must canonicalize to the same root or
    the call refuses `refresh_root_mismatch`.
  - SPEC-1b: refresh NEVER changes the root set. A refresh whose scan would alter roots
    aborts with `refresh_would_change_roots`, mutating nothing.
  - SPEC-1c: single-flight per canonical root (TOCTOU, grok req): a second refresh while one
    is in flight refuses `refresh_in_flight`, never queues silently, never interleaves.
  - SPEC-1d: MCP wire and REST `POST /api/tools/ingest` route through ONE admission seam
    (parity, grok req) — proven by the same refusal bytes on both doors.
  - SPEC-1e: journaled: refusal-or-receipt, never silence; a crash mid-refresh recovers via
    the existing `graph_ingest_a2` recovery kind, and recovery never half-applies.

### SPEC-2 — `brain.bootstrap.birth` (fresh birth, the sovereign door)

- **Definition:** minting a brain for a root NO existing brain covers.
- **Floor:** `PositiveSovereign` — unchanged. Admission requires an **owner-stamped human
  origin**, the `receipt_import` precedent exactly: a server-side CLOSED allowlist
  (`human-ui`, `human-touchid`, plus a new `human-cli` minted only by the P2 ceremony), where
  the stamp is composed by the owner's own surface. The codebase already states the law this
  follows: *"a client-supplied origin token (including 'human-ui') grants no authority"* —
  a generic MCP client claiming the origin string is refused identically to today.
- **P2 is the ceremony that mints the stamp:** `m1nd init --birth <root>` (today `init` is
  only `installSkills` — the PRD's "built" claim is stale and gets corrected in the same PR
  that lands this). The agent's role stays exactly what checkpoint 32 ratified: DETECT the
  brainless root and OFFER the exact command string; the human runs it once.
- **Guards (each one a test):**
  - SPEC-2a: "empty destination" is defined ON DISK (grok req): birth refuses
    `destination_not_empty` if the target store dir holds any manifest, snapshot, or
    checkpoint — no orphan adoption through the birth door.
  - SPEC-2b: overlap classes (`overlap_parent|child|worktree`) refuse exactly as the
    two-tier law states; `allow_overlap` DOES NOT EXIST on any path below sovereign
    (grok req: no escape hatch off the sovereign path).
  - SPEC-2c: partial birth is recoverable (grok req): birth is journaled
    prepare→commit; a crash between them leaves a store the next boot either completes or
    removes whole — never a half-brain that routing can bind.
  - SPEC-2d: single-flight per canonical root, same as SPEC-1c.
  - SPEC-2e: birth NEVER touches the bound dev graph — the "bootstrap never shadows" rule,
    kept bytewise (the owner's graph is not replaced; R-D stays impossible).

### SPEC-3 — migration/adoption stays a boot-time fact (NO verb)

The ~17k-node brain migration that item-zero named is ALREADY SOLVED by adoption (R-A).
No MCP/REST verb performs migration; proposing one is out of scope (grok req: separate
migration from fresh birth). The update rite documents the one real cost: a changed binary
recomputes the embedding cache once (~60–70s warm-up, `cache 0 reused / N new`).

### SPEC-4 — what stays frozen, on purpose

`graph.ingest.replace` from ANY root (the R-D weapon), cross-root writes, `learn`
(SCOPED_GRANT_A2, separate decision), and every `allow_overlap` path: all keep today's exact
refusals. This spec ADDS two narrow doors; it widens none.

## 2 · Acceptance (the wave's RED battery, before any code)

1. Foreign-root replace attempt → today's refusal bytes, unchanged (the hijack stays dead).
2. Covering-root refresh on current-main → succeeds, absorbs a new commit's symbols, root
   set unchanged, journal receipt present. (Born RED: today it refuses — R-C receipt.)
3. Birth without the owner-stamped origin → refused; with a client-claimed origin string →
   refused identically (the ratify precedent).
4. Birth via the P2 ceremony on an empty destination → brain exists, routes by caller root,
   bound dev graph untouched (SPEC-2e), and a second concurrent birth refuses (SPEC-2d).
5. Kill -9 mid-birth → next boot completes-or-removes whole (SPEC-2c); kill mid-refresh →
   recovery via the a2 recovery kind, graph either old or new, never mixed (SPEC-1e).
6. All of the above byte-identical over MCP and REST (SPEC-1d).

## 3 · Non-goals

The G2/G3 cryptographic authority runtime stays DORMANT — this spec installs local typed
consumers under the existing floors, it does not activate autonomy. No change to `learn`.
No perf work (separate front, in flight). No Windows path-canon work.

## 4 · Gates before implementation

askGOD verdict on this document → owner ratification of the two floors (SPEC-1's A2-local
and SPEC-2's human-origin allowlist entry) → then implementation, battery-first, in the
proof-grown rite. The verdict must specifically confront SPEC-1's floor-lowering argument —
if it does not survive adversarial reading, the freshness door does not open.
