# Gardener v1 — divergences and honest residue

> Companion to `docs/voice/GARDENER-V1.md`. Everything the arc did NOT do as
> literally specified, and why — recorded instead of silently absorbed.

1. **OCC conflict proven at policy level, not end-to-end.** The verdict's
   "Conflict → 1 retry → alert" is pinned by
   `occ_conflict_gets_one_retry_then_an_alert_never_a_loop` with an INJECTED
   `SeedError::Conflict` (attempt-count asserted: exactly 2) plus the settle arm
   recording the real alert on the real lane. A deterministic END-TO-END
   conflict is impossible in a unit test: each attempt reads its own fresh
   `expected_store_version`, so a conflict requires a concurrent writer landing
   in the microseconds between the fresh read and the reconcile — a race a test
   cannot arrange without instrumenting production code with test-only seams.
   The wiring (exhausted → alert) is three lines, reviewed and settle-tested.

2. **The stdio coalesce window is pinned indirectly.** The sliding-silence loop
   lives inline in `serve()` (a blocking mpsc loop a unit test cannot drive
   without a full stdio harness). What is pinned: the registered constants
   (`BURST_COALESCE_WINDOW_MS`/`_CAP_MS`), `daemon_start` persisting the window
   (`coalesce_window_ms` asserted in the lifecycle test), and the cap guard in
   the loop (code-reviewed; the loop already broke on Request/StdinClosed).
   The BEHAVIORAL burst law — thousands of events → one detection, no lost
   tail — is pinned end-to-end at the tick level instead
   (`burst_bigger_than_tick_budget_drains_completely_without_losing_the_tail`),
   which is where the correctness actually lives.

3. **The bound owner's `workspace_root` mutation stays.** The verdict's cheap
   guard shipped for HOSTED brains (manifest-bound roots can no longer be
   demoted by `auto_ingest_start` — the #326 class). The BOUND owner keeps the
   historical mutation: its `workspace_root` is not manifest-anchored, several
   existing flows (workspace inference, fingerprint display) lean on the
   mutation, and re-designing bound-owner root semantics is beyond a "se for
   barato" guard. Residue: an owner-session `auto_ingest_start` over a docs dir
   still re-points the bound session's workspace root.

4. **Recall verbs do not advance freshness on the HTTP owner.** `seek`,
   `north`, `boot_memory` and `delegate` route through the tier-compose path,
   which deliberately does not autotick (read-only recall stays cheap). A
   session that ONLY calls recall verbs sees `last_tick_ms` age; any other verb
   advances it. Recorded as part of the honesty doctrine ("fresh when seen"
   means seen by a non-recall verb), not changed in v1.

5. **Peak-RSS is a coarse proxy.** The bench's allocation number is the whole
   test process (three sequential repos + graphs + harness) via
   `/usr/bin/time -l`, not a per-tick allocation profile. Honest as an upper
   bound; a per-tick allocator profile is v2 work if the number ever matters.
