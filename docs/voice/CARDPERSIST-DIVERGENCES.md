# CARDPERSIST "does the letter reach disk?" — divergences and honest residue

> Companion to the field reports of `2026-07-13T03:20` / `03:30` (m1nd
> `field-reports.jsonl`) and to `docs/voice/P0-DIVERGENCES.md`. The task was
> ratified as a bug fix: *"cartas de mission-control aceitas via REST
> `?brain=<root>` NUNCA chegam a disco — vivem em memória de um runtime de
> instance hospedado e morrem no restart."* This file records the honest residue
> of the investigation, because the residue is the finding: **the bug as described
> does not reproduce on `main` (6b018a8).** Nothing below is sugar-coated — where
> the disk contradicts the report, the disk wins (physical reality > report).

## The claim vs the disk

The field report concluded the P0/P1 charters (`msn_…claudeguardianp0we`, `…fixt`,
`…umbr`, plus the P1 executors) had **zero files in any `mission-control/` on disk**
and had "died in memory in a hosted instance runtime."

The disk says otherwise. All six named charters are on disk in the BOUND
mission-control, `~/.m1nd/runtimes/claude/mission-control/`, with file mtimes at
their creation/close time (e.g. `…claudeguardianp0we.json`: id-ts `2026-07-12
23:59:15`, mtime `2026-07-13 00:24:35` — its own `updated_at_ms` on close). They
were on disk hours BEFORE the `03:30` "zero on disk" claim. Independently, an
UNRELATED hosted project brain (a different repo) had its own two `msn_*` charters
from the same window persisted normally under its
`project-brains/<fp>/mission-control/` — the exact hosted path the report said was
ephemeral.

## The exact mechanism (why it always persisted)

1. **A charter is on disk BEFORE the ack.** `handle_mission_start` calls
   `save_mission(state, &mission)?` (`m1nd-mcp/src/mission_handlers.rs:117`) BEFORE
   it builds its `Ok(json!( … ))` response; `handle_mission_event` (`:157`) and
   `handle_mission_close` (`:353`) do the same. `save_mission`
   (`mission_handlers.rs:537`) writes `<runtime_root>/mission-control/<msn>.json`.
   This has been true since the file's FIRST commit (`111e6b2 "runtime: add
   mission control loop"`) — there was never an in-memory-only window.
2. **A hosted brain's `runtime_root` IS its durable store.** `boot_store` sets
   `runtime_dir: Some(store.clone())` (`m1nd-mcp/src/project_brains.rs:217`), so a
   charter opened on a `?brain=` hosted brain lands at
   `<owner runtime>/project-brains/<fp>/mission-control/<msn>.json` and warm-boots
   back from there — not a temp dir.
3. **`?brain=/Users/kle1nz/m1nd` resolves to the BOUND graph, not a hosted
   instance.** `resolve_brain` checks `bound_matches` FIRST
   (`m1nd-mcp/src/http_server.rs:1634-1646`), comparing the request against
   `project_root_display()`, which returns the first non-sidecar ingest root
   (`m1nd-mcp/src/session.rs:1007-1013`) — `/Users/kle1nz/m1nd`. So the selector
   matches bound and persists to the bound mission-control. Live probe confirms it:
   `mission_start?brain=/Users/kle1nz/m1nd` for `cardpersist-executor` produced
   `msn_1783938868118_cardpersistexecuto.json` on disk the instant it returned.

## Root of the misdiagnosis (the honest telemetry)

- **mission-CONTROL vs mission-LETTER.** `P0-DIVERGENCES.md` item #2 already
  established that `msn_*` mission-control charters are ABSENT from
  `GET /api/mailbox?kind=mission` BY DESIGN (that board is the mission-LETTER rail,
  `m1nd-mission-letter-v0`). The `03:20` observation ("charters not on the board")
  was true and expected; the `03:30` leap from "not on the board" to "not on disk /
  died in memory" was the error.
- **`inst_f168ae0e3b608a2e` IS the bound owner, not a twin.** The report read it as
  "an instance for `/Users/kle1nz/m1nd` SEPARATE from the bound." The registry
  entry (`~/.m1nd/registry-claude/instances/inst_f168ae0e3b608a2e.json`, and
  `/api/instances`) says `brain_kind: "medulla"`, `project_root:
  /Users/kle1nz/m1nd`, `runtime_root: ~/.m1nd/runtimes/claude` — it is the served
  owner itself. There is exactly ONE runtime for that root; no project brain for
  `/Users/kle1nz/m1nd` exists on disk. "Two runtimes for one root" was a misread of
  the medulla owner (`inst_f168…`) beside a hosted brain for a DIFFERENT,
  unrelated repo (`inst_a24952…`).

## What was delivered (proof, not a fix)

No source change: there is no persistence defect to repair, and the verdict régua
("carta EM DISCO antes do ack; um root = um mission-control canônico") is ALREADY
satisfied by `save_mission`-before-ack + the bound-first resolution. Instead, the
invariant is now a standing regression guard:

- `m1nd-mcp/tests/per_brain_open.rs::charter_survives_owner_restart_on_the_hosted_path`
  — opens a charter on a HOSTED `?brain=` brain, asserts the card `is_file()` at its
  store's mission-control BEFORE the ack, drops the owner (the `launchctl kickstart`
  analog), boots a fresh owner on the same runtime, and reloads the charter through
  the real seam. GREEN today; proven to have teeth by a counterfactual that
  neutered `save_mission` (both the ack-time `is_file()` and the post-restart reload
  went RED, exactly the imagined "lives in memory" bug).

## One latent risk, noted and OUT of scope

If a project brain were ever MINTED for the bound root itself (e.g.
`ingest project_root=/Users/kle1nz/m1nd` — the overlap guard checks other project
brains, not the bound graph), `?brain=/Users/kle1nz/m1nd` would still resolve to
BOUND (bound-first, `http_server.rs:1643`) and SHADOW that project brain's
mission-control, orphaning any charter written to it. This did NOT happen in the
field (no such brain on disk) and is a DIFFERENT concern from the reported bug;
addressing it would mean changing mint/routing policy — explicitly excluded by the
task ("cirúrgico; não redesenhe o mission-control"). Recorded here so the next
wire-wearer sees it, not silently absorbed.
