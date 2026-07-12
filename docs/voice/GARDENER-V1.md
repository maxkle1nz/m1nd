# Gardener v1 — the organism that moves when seen

> Arc law: `docs/voice/ASKGOD-VERDICT-GARDENER.md` (CHANGE verdict, 2026-07-12).
> This doc records the design as BUILT, the registered numbers, the measured
> cost, and the v2 slices deliberately left out. Branch: `feat/gardener-v1`.

## The design in five lines

1. **Fail-open first:** a background vigil (auto-ingest tick, daemon tick) can
   NEVER fail an agent's tool call — the previously violable `?` in
   `dispatch_tool` is now `vigil_fail_open` (log + swallow), and every other
   vigil path was audited already-fail-open.
2. **The code leg is the DAEMON, per brain:** `daemon_start` routed to a brain
   arms it (watch set = the brain's own ingest roots), the opt-in persists in
   the brain's OWN store dir (factory default OFF), and the resume rides the
   registry warm-boot/resolve path — surviving restart AND LRU eviction. The
   resume is lazy: no scan before the listener; the first traffic tick does the
   inventory work.
3. **Honesty by traffic:** v1 freshness is "when seen", literally — on the HTTP
   owner ticks advance on non-recall verb traffic (recall verbs — seek / north /
   boot_memory / delegate — do not tick); `watch_backend` can no longer resume
   the `native_fs` claim of a notify watcher that died with its process, and no
   surface says "continuously monitored" (tool schema, help guidance and wiki
   rewritten honestly).
4. **Bursts coalesce, nothing is lost:** the stdio event window went 75 ms →
   500 ms silence with a 5 s cap, and the tick itself now detects ONCE per
   burst, pushes the whole changed set into a persisted FIFO backlog and drains
   `max_files` per tick — the old truncate-then-advance hole silently LOST every
   file beyond the budget on the git backend.
5. **Auto-reconcile with cedência:** a burst schedules a reconcile of the
   RATIFIED system-blocks store behind a 45 s quiet window (every activity tick
   pushes the deadline — one window per burst); a live `candidate_lease` makes
   the reconciler YIELD voluntarily (skip + reschedule; the lease is advisory by
   ratified law — it cannot block, so WE cede); the write keys on a FRESH
   `store_version`, gets exactly 1 OCC retry, then an `auto_reconcile_conflict`
   alert on the existing lane — never a loop. A candidate skeleton is skipped:
   candidate freshness is another cycle (the re-scan), outside this arc.

## Registered numbers (verdict: "número escolhido REGISTRADO e justificado")

| Constant | Value | Why |
|---|---|---|
| `BURST_COALESCE_WINDOW_MS` | 500 ms | git's index/lock/pack pauses run into the low hundreds of ms; 75 ms of silence fired mid-checkout. 500 ms closes one checkout into ONE detection while adding ≤ 0.5 s latency to a single-file save. |
| `BURST_COALESCE_CAP_MS` | 5 s | a sliding silence window alone can starve under continuous churn; the cap bounds one coalescing pass so the graph advances during a storm. |
| `AUTO_RECONCILE_QUIET_WINDOW_MS` | 45 s | inside the verdict's 30–60 s band: long enough that one logical burst (checkout + follow-up churn) collapses into one window; short enough the map refreshes within a minute of quiet. |
| drain budget (`max_files`) | 32 (existing default) | keeps one drain tick at ~1.6–1.9 s measured (below), an acceptable per-routed-call tax for an opt-in brain. |

## Measured cost (the bench the verdict demanded before aggressive defaults)

`bench_daemon_tick_burst` (`#[ignore]`, release, macOS dev machine, git
backend, drain budget 32; RSS via `/usr/bin/time -l`):

| N changed | detection tick | drain ticks | total | per file |
|---|---|---|---|---|
| 10 | 357 ms | 0 | 357 ms | 35.7 ms |
| 100 | 1 189 ms | 3 (2 477 ms) | 3 666 ms | 36.7 ms |
| 1000 | 8 673 ms | 31 (50 899 ms) | 59 573 ms | 59.6 ms |

Peak RSS of the whole bench process (three repos + graphs): ~3.7 GB — a coarse
upper-bound proxy, recorded honestly as such.

**The honest reading:** a drain tick costs ~1.6–1.9 s and the first post-burst
detection of a 1000-file burst ~8.7 s, paid inline by whatever routed call
triggers it. This is exactly why the factory default stays OFF and arming is a
per-brain opt-in on the hot brains only.

## v2 — registered, deliberately out of v1

- **Zero-traffic alerts** (a per-brain tick task on the HTTP owner, with
  lock-contention measured first) — the verdict's explicit v2 slice. Until
  then: no traffic, no advance, and every surface says so.
- **Detection-walk hashing:** `inventory_from_watch_paths` re-hashes every file
  every tick (the polling-era cost model) — the dominant term in the measured
  detection cost. The v2 lever is reusing git status to skip unchanged content.
- **Candidate freshness** (auto re-scan of a candidate skeleton) — another
  cycle by verdict law; the auto-reconciler skips candidate stores.

## Test map (the names the arc is judged by)

- fail-open: `erroring_auto_ingest_vigil_never_fails_the_agents_tool_call`
  (RED-proven against the old `?`), `broken_background_tick_never_fails_the_agents_tool_call`,
  `vigil_fail_open_swallows_a_failing_vigil`
- restart resume: `armed_daemon_resumes_across_restart_and_ticks_again`
  (RED-proven: the persisted mid-tick `tick_in_flight: true` wedged every resume)
- eviction→rearm: `evicted_brains_armed_daemon_rearms_on_the_next_resolve`
- HTTP status honesty: `resumed_status_never_claims_a_dead_notify_watcher` (RED-proven)
- burst coalescing: `burst_bigger_than_tick_budget_drains_completely_without_losing_the_tail`
  (RED-proven against the truncate-then-advance loss)
- quiet window: `quiet_window_coalesces_bursts_then_reconciles`
- lease yield: `auto_reconcile_yields_to_a_live_lease_and_reschedules`
- ratified-only: `auto_reconcile_skips_a_candidate_skeleton`
- OCC: `occ_conflict_gets_one_retry_then_an_alert_never_a_loop`
- upgrade safety: `pre_gardener_daemon_state_still_deserializes_and_stays_armed`
- auto-ingest guard: `auto_ingest_start_never_demotes_a_hosted_brains_code_root`
- cost: `bench_daemon_tick_burst` (ignored; run manually)

Honest residue and refusals: `docs/voice/GARDENER-DIVERGENCES.md`.
