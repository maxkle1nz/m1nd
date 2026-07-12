# askGOD verdict — the Gardener arc (self-moving organism) — 2026-07-12

> Seat: askGOD verdict (Fable), marcha medium, sources read and cited.
> Proposal judged: turn on auto_ingest + daemon alerts + auto-reconcile.
> Owner authorization: "autorizo tudo — isso vai elevar o m1nd a um organismo mais
> esperto e capaz de responder de verdade sempre que for visto."

VERDICT: CHANGE (direction right; one factual mis-aim, one artifact conflation, one
architecture hole — do NOT implement the 3 legs as written)
CONFIDENCE: alta on code facts · média on operational scoping (cost unmeasured)

## The three discoveries that changed the design

1. **auto_ingest is a DOCUMENT watcher, not a code watcher.** `detect_allowed_format`
   only accepts md/txt/rst/adoc/html/pdf/docx/pptx/xlsx/xml/json/bib — a code upsert
   becomes an "ignored: unsupported or code file" event (auto_ingest.rs:298-319, 780-787;
   universal_adapter.rs:68-88). **Code re-ingest belongs to the DAEMON**
   (daemon_handlers.rs:635-680: the tick calls handle_ingest per changed file AND emits
   alerts; daemon_start defaults watch_paths = ingest_roots).
2. **On the HTTP owner, watchers only advance BY TRAFFIC.** The free-running loop
   (WatchNotice→run_daemon_tick, idle pump) lives ONLY in the stdio loop
   (server.rs:6040-6160); http_server/mcp_http have none. The owner's phrase —
   "responder de verdade sempre que for visto" — is LITERALLY the existing
   traffic-tick mechanism, currently disarmed (server.rs:5475-5482, 4587).
3. **Reconcile ≠ candidate, and the lease protects nothing by law.**
   `system_blocks_reconcile` is an OCC write to the RATIFIED store; candidate freshness
   is a different cycle (skeleton_candidate re-scan). The candidate_lease is advisory
   BY RATIFIED LAW ("it never blocks an edit") — an auto-reconciler must YIELD
   voluntarily; the lease cannot shield the human.

## Additional traps found

- **Fail-open is violable today**: server.rs:4587 uses `?` — an auto-ingest tick error
  propagates into the agent's unrelated tool call. Fix FIRST, before arming anything.
- **LRU eviction silently kills watchers**: insert_with_eviction drops the SessionState
  (and its RecommendedWatcher) — "armed today, dead in a week, no alert". Re-arm must
  live in the ProjectBrainRegistry warm-boot/resolve path.
- **auto_ingest_start MUTATES workspace_root** (sets it to the first root) — reintroduces
  the #326 store-dir/code-root bug class if armed on a hosted brain.
- **OCC churn against the human**: every auto-reconcile bumps store_version and kills an
  in-flight human candidate_edit batch with Conflict. Only voluntary yielding protects.
- **rebuild_engines per tick + 200/75ms debounce**: a git checkout (thousands of events)
  needs burst coalescing measured before aggressive defaults; Linux inotify limits when
  this ever becomes a cross-platform factory default.
- **Status honesty**: with traffic-ticks, last_tick_ms ages without traffic — no surface
  may claim "continuously monitored"; watch_backend must not lie on HTTP.

## The v1 that survives (required changes, condensed)

1. Re-aim leg 1: code re-ingest = DAEMON per-brain (watch_paths = the brain's
   ingest_roots). auto_ingest stays OUT of v1 (or enters later, honestly named the
   documents lane).
2. Fail-open first: log-and-continue at server.rs:4587 (+ audit run_daemon_tick).
3. Resume on brain warm-boot (registry resolve path), lazy after the listener; the
   synchronous bootstrap scan never on the boot path. Survives eviction (test it).
4. Leg 2 honesty: v1 = freshness-by-traffic ("when seen", literal). Zero-traffic alerts
   = explicit v2 slice (per-brain tick task on HTTP with lock-contention measurement).
5. Leg 3 rewritten: auto-reconcile runs after a daemon tick that re-ingested files, with
   a quiet window (30-60s no new events), yields to an active candidate_lease
   (skip + reschedule), 1 OCC retry then alert — never a loop. Say plainly: reconcile
   refreshes the STORE; candidate freshness is another cycle, outside this arc.
6. Untouched: human ratify/import; north Budget Law (alerts ride the existing bell/pulse
   lanes, alert cap 500 already exists).
7. Validation accepted + amplified: eviction→rearm test; tick-error-never-fails-tool-call
   test; quiet-window and lease-yield tests; restart resume test.
8. Factory default OFF; opt-in per brain; v1 scope = the hot brains (m1nd + game),
   never all 8.

## Status
- Executor dispatched with this corrected design (2026-07-12).
- The organism-inside PRD/UML (Fable author) runs in parallel as the NEXT arc's draft.
