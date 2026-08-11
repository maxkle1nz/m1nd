---
name: m1nd-operator
description: Use when the user mentions m1nd or when repo investigation, search, review, docs/spec work, or risky change prep should go through m1nd first before grep, glob, or manual file reads. Covers m1nd-first routing, L1GHT and universal document ingestion, risky edit preparation, document-to-code binding, the served owner + --attach bridge transport, the delegation layer (delegate/debrief packets), medulla tiers and audited promotion, soul freshness receipts, trails/continuity, and daemon alerts.
---

# m1nd Operator

This is the deep execution manual that complements the short `m1nd-first` doctrine.

Use this skill when `m1nd` should be the first layer of truth for the task.

`m1nd` is strongest when structure, connected context, blast radius, continuity, docs/code bindings, or multi-agent coordination are the bottleneck. It is not the replacement for `rg`, the compiler, runtime logs, or the test runner.

## Default Stance

Default to `m1nd` before grep, glob, or manual file reads.

The first question is not "which shell command should I run?" It is "can `m1nd` answer or narrow this directly from graph structure, connected context, docs, or cross-domain bindings?"

Only skip the `m1nd` first pass when:

- the user already gave the exact file and exact lines
- the question is pure compiler/test/runtime truth
- the task is a trivial local file action with no search or structural uncertainty

Trust-first is calibrated-first, not blind-first. Default to `m1nd`, AND hold its honest ruler in
the same hand: it narrows and connects, never replaces the compiler/tests/runtime; and it still
has live stumbles measured in the field — federated `seek`/`federate` can hang past 60s
(bound-timeout it or fall back), a freshly-`memorize`d claim can lag its own semantic recall for a
tick (the write is durable — re-`seek` or widen, don't conclude absence), and the runner naming
lane can fall back to heuristics under load. In each case WORK AROUND + one field-report line;
never blind insistence. The reason to still go first: the calibrated absence
(`abstain`/`caller_root_mismatch`/`gathering`) is itself the answer that most often changes the
decision — the counterfactual test for attribution.

## Trained Agent Loop

For unfamiliar repo work, audits, bug hunts, reviews, and risky changes, the
measured high-signal pattern starts by never starting cold:

1. Call `north(task)` FIRST, before reading or editing anything. It is the
   in-session front door. One round-trip returns binding trust (`trust_mode`;
   repair travels with it when degraded), task context (focus nodes + PageRank
   anchors), prior cross-session memory (each claim with real age + author —
   absent, never faked, when unknown), a sufficiency signal, one `next_move`,
   and `honest_gaps` (what m1nd does NOT know) — plus `code`, the real SOURCE of
   the top focus nodes (the symbol's own lines, or the file head when a node
   names no symbol) for up to 3 files inside a ~2,000-char budget, each slice
   declaring `total_lines` vs `lines_returned` and `truncated`. Read that payload
   instead of grepping or re-opening what north just named; widen the first call
   with `code_budget_chars` rather than paying a second round-trip, and pass
   `code: false` when you want orientation only. `north` composes
   `trust_selftest` + `orient` + `boot_memory` + `focus` +
   `surgical_context`/`batch_view`; reach for the pieces directly only when you
   need just one. Heed `reception`:
   `reception.match == "caller_root_mismatch"` means the bound graph does NOT
   cover your current repo — do not trust retrieval for it; read
   `reception.options[]`. Generic cross-root `ingest` remains withdrawn
   (`project_root` absent from the published schema), and over the wire
   `brain.bootstrap.birth` answers every client with
   `human_gesture_required`. The way forward for a brainless repo is the
   human's one-time ceremony `m1nd init --birth <repo>`: offer that command
   and stop. Until the human runs it, continue only against the bound graph
   with the mismatch warning intact, or reconnect to an owner that already
   hosts the intended repo. Absent/null = your root matches the brain
   serving you. Reception governs WRITES too, not only reads: a read under
   mismatch is a warning, a WRITE under mismatch is prohibited (see Write-Mode
   Laws below) — and no public bootstrap lifts it. VERSION SKEW: this skill
   describes the CURRENT product; check `binding.fingerprint.binary_version`
   (and its `binary_drift` warning) in the same packet before trusting either
   era's rule. A legacy 1.4.x served owner still HAS the one-call bootstrap,
   and there `ingest {project_root: <your repo root>}` is the correct
   foreign-repo gesture — it mints a SEPARATE per-project brain, never
   replacing the bound one. A plain `ingest {path}` from a foreign root is
   wrong in EVERY era: on a legacy owner it wholesale-REPLACES the bound brain
   (2026-07-24 incident — a 29k-node brain swapped for a foreign graph,
   restored the same day).
2. If `north` returns `needs_ingest` (empty/unbound graph), re-scan and call
   `north` again. `needs_ingest` is a REAL answer, not a failure. On a brain that
   already DECLARES your root, the repair is yours to run and needs no human:
   `ingest {mode:"refresh", path: <your root>}` from exactly that root (Law 3
   below). On a root no brain declares, it is not a refresh — no agent mints a
   brain on any transport — and the honest move is to OFFER the human their
   one-time ceremony, `m1nd init --birth <repo>`, and stop (or reconnect to an
   owner that already hosts the repo). A plain `ingest {path}` (mode `replace`)
   remains refused, and its refusal now names that same command.
3. Act on verdicts, do not override them (see below).
4. Prove final truth with direct source reads, tests, compiler/runtime output,
   and focused probes.
5. Before edits/reviews, run `impact`, `validate_plan`, and usually
   `surgical_context_v2`.
6. Record the investigation path: m1nd calls, recovery decisions, files
   inspected, commands run, and fallback reason.

Degraded path only: `trust_selftest` / `session_handshake` / `recovery_playbook`
are the DEGRADED/RECOVERY front door, not the default — reach for them when
trust looks off, retrieval is `blocked`/empty unexpectedly, a
`wrong_workspace_binding` is reported, or the transport is closed. Classify
`wrong_workspace_binding` as rebind/intentional-ingest/federation, never as
graph staleness.

Verdict semantics — trust the calibration:

- Retrieval and prediction return **`act` / `reverify` / `abstain`**. `abstain`
  = uncalibrated OR insufficient evidence: a STOP, not a weak yes — do not guess
  past it. The prediction gate is armed per-repo by `calibrate_predict` and the
  seek trust envelope by `calibrate_envelope` (from the ledger's learn outcomes);
  until each is armed its verdict caps at `reverify`, never `act`.
- `why` carries a `closure` verdict — `blocked` means the path rests on an
  unresolved (guessed/dropped) edge: verify that edge before relying on the path.
- `seek` carries a `trust_envelope` + a sufficiency stop-signal — `sufficient` =
  stop gathering; `gathering`/`saturated` = widen or refine.
- `trust_band: insufficient_evidence` = NO evidence, not medium risk — the
  honest cold-start answer, distinct from low/medium/high risk.

Internal bug-hunt rounds call this `m1nd-trained`: graph plus operating
doctrine. A visible MCP surface without this loop is only `m1nd-basic`.

## The menu is a core, not the surface

`tools/list` advertises about **fifteen** verbs by default — the owner-ratified
core plus the host-binding floor — while 140+ are registered:

| | verbs |
|---|---|
| the core (12) | `north` `memorize` `ingest` `seek` `search` `health` `trust_selftest` `view` `impact` `session_handshake` `boot_memory` `surgical_context` |
| the binding floor (3) | `help` `doctor` `recovery_playbook` |

That is the shop window, not the shop. **Nothing was removed**: every other verb
keeps its handler, its route and its authority floor, and naming one works
exactly as if it were listed. The cut is at advertisement only, and it was made
against measurement — six weeks of real traffic advertised 141 verbs and saw 13
called; across every prefix family (`perspective_*`, `mission_*`, `trail_*`,
`daemon_*`, `document_*`, …) exactly two calls were ever made. The verbs were
never bad. A 141-item menu is not a menu.

**`help` is the door.** It catalogs the FULL registry at every tier, so it can
name and explain a verb you cannot see:

- `help(agent_id, intent: "...")` or `help(agent_id, stage: "...")` — route to
  the right verb for what you are actually doing.
- `help(agent_id, tool_name: "...")` — any verb's full schema and minimal call,
  listed or not.
- A mistyped name comes back with a did-you-mean drawn from the whole registry.

`health.tool_surface_contract` reports the live `advertised_tool_count`,
`hidden_tool_count` and `full_registry_tool_count`. An operator who wants every
verb in the menu sets `M1ND_TOOL_TIER=full` on the server.

**The rule this replaces a reflex with:** never conclude m1nd cannot do
something because it is not in the tool list. Ask `help` first. The measurement
that motivated this change also caught the opposite failure — an independent
evaluator on a foreign 107k-LOC codebase ranked `surgical_context` in its top
three while agents here called it once in six weeks. Invisible is not absent,
and a long list made everything equally invisible.

## Advertised ≠ callable — the authority floors

`tools/list` advertises the full verb surface, but generic MCP/REST dispatch
admits only actions whose M1ND-10 authority floor is `ORDINARY`. A verb above it
(`SCOPED_GRANT_A2` / `POSITIVE_SOVEREIGN` / `SERVICE_IDENTITY`) refuses with
`generic_action_authority_required`; no payload shape, capability claim, or retry
lifts it — only an exact typed G2/G3 consumer (an authority lease), and none is
installed for those actions yet. 40 of the registry's verbs are affected
today, including `learn`, `debrief`, `promote`, `calibrate_predict` /
`calibrate_envelope`, `ghost_edges`, `runtime_overlay`, `apply` / `apply_batch` /
`edit_commit`, `daemon_start` / `_stop` / `_tick`, `auto_ingest_start` / `_stop`
/ `_tick` (their `_status` reads stay open), the `xray_*` commit branch,
`boot_memory` set/delete, `mission_close write_light_memory:true`, and every
system-blocks writer.

The schema is the live source of truth: each affected description is prefixed
`POLICY-DISABLED (authority floor …)`. Read it before planning a step around a
verb, and never spend turns retrying a floor refusal. Reads, `memorize`,
`delegate`, `trail_save`, the perspective family, and plain `mission_start` /
`_event` / `_verify` / `_handoff` / `_close` stay `ORDINARY` and work.

## Write-Mode Laws — reception governs writes, one repo means one brain

The served owner hosts many per-project brains and routes each request to the
brain that covers the caller's repo (`M1nd-Caller-Root` ↔ `covers_root`).
Retrieval under a wrong binding is a recoverable annoyance; a WRITE under a wrong
binding is corruption of shared state. A real incident set this law: a foreign
repo's skeleton was written into a bound brain because the writer never checked
which brain answered it. Three laws:

- **Law 1 — no write under a reception mismatch.** When a response carries
  `reception.match == "caller_root_mismatch"`, the brain serving you does NOT
  cover your repo. Reads are a warning; **writes are prohibited by doctrine** —
  `memorize`, `skeleton_candidate`, `candidate_edit`, `system_blocks_seed_import`
  / `_ratify` / `_reconcile`, and `mission_post` would each land in the WRONG
  brain. No public gesture lifts this: the brain-bootstrap consumer is NOT
  installed, so `ingest` does not accept `project_root` and cross-root bootstrap
  fails closed with `brain_bootstrap_consumer_not_installed`. Do not write from a
  mismatched session — reconnect to an owner that already hosts the intended
  repo, or stay read-only with the warning intact. The `memorize` refusal (a
  write from a root with no project brain is refused, never dropped into the
  shared medulla, and names the absent consumer instead of handing a repair that
  would fail closed) is the one already mechanical instance; the doctrine generalizes it to every write verb, and the
  broader mechanical refusal is landing. If you ever catch a miswrite, that is
  `class:"memory_misdelivery"` / `kind:"wrong_store_write"` in the field spool.
- **Law 2 — no twin brains (the overlap guard, both seams; built and tested,
  but NOT publicly reachable while the bootstrap consumer is absent).** Before
  minting a NEW project brain, the bootstrap classifies the root against every
  existing brain (warm map ∪ on-disk roster) into one of three overlap classes
  and refuses with a teaching error naming the conflict and two ways forward:
  - `overlap_child` — the new root is INSIDE an existing brain's root.
  - `overlap_parent` — an existing brain's root is INSIDE the new root (the
    mother-folder trap that would re-ingest the child repo from above).
  - `overlap_worktree` — the new root is a git worktree (`.git` is a gitdir file
    under `<main>/.git/worktrees/`) whose main repo already has a brain.
  The two ways forward it names: (a) bind to the existing brain, or (b) mint a
  separate brain anyway — only when you know exactly why. Neither is reachable
  from the public MCP surface today: `project_root` and `allow_overlap` are
  absent from the published `ingest` schema. The exact-same root is warm-reuse
  (never an overlap). This holds on BOTH doors: the MCP wire (`ingest` with a
  non-empty `project_root` is a bootstrap directive → `run_bootstrap`) and the
  REST `POST /api/tools/ingest` route both call one seam-shared guarded core, so a
  REST caller gets the same refusal as an HTTP 400. A burst worktree does NOT earn
  its own brain — bind to the main repo's. (Separately, bootstrap never SHADOWS
  the bound dev graph: a `project_root` the bound graph already covers is refused.)

- **Law 3 — the freshness door is the ONE ingest a plain client can run, and only
  from its own root.** `ingest {mode:"refresh"}` re-scans a root the brain has
  ALREADY declared. It is admitted at `SCOPED_GRANT_A2`, A2-locally, with no
  lease, and admitted BY ACTION rather than by floor — the two siblings sitting at
  that same floor (`source.edit.commit`, `graph.ingest.merge_existing`) stay
  refused, and their refusal bytes are pinned by test. `replace` and `merge` are
  unchanged: refused. Every refusal mutates nothing and names itself:
  `refresh_root_not_exact` (your caller root is not EXACTLY a declared root — a
  subdirectory is not the root, and an explicit REST `?brain=` selector never
  satisfies the predicate), `refresh_root_unresolvable` (the path does not resolve
  on disk — an unresolvable path is refused, never string-matched),
  `refresh_in_flight`, `refresh_would_change_roots`, and
  `refresh_would_shrink_graph` — which refuses when the fresh scan holds under
  **60%** of the live graph's nodes, and names both counts. That floor is armor
  the persist layer does not give you: its own shrink guard is fail-open by
  written design (it backs up and writes anyway), so "root set unchanged" never
  meant "graph intact". Reach for `refresh` when a declared root's graph is stale;
  never as a repair for a reception mismatch, which Law 1 still governs. Honest
  limit, from the spec itself: it closes the reflex vector, not a malicious
  same-UID local process. (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §1.)

Not yet built (do not rely on it): `skeleton_coherence`, a vital sign that makes
a brain flag loudly when it is wearing a skeleton from a foreign repo. Until it
lands, Law 1 is the doctrine that prevents the miswrite.

## The Candidate Map and Mission Letters (F11 + F2.5 write mode)

The block map (the skeleton) and the mission layer are the two write surfaces
beyond `memorize`. Both are candidate/human-gated by design.

**The candidate map (F11).** `skeleton_candidate` scans a repo into a CANDIDATE
block map; with `naming:"auto"` and a live naming-runner the map is born NAMED
(the zero-touch default — read the map, stamp it). One verb edits it:

```json
candidate_edit {"agent_id":"...","expected_store_version":7,"ops":[
  {"op":"rename","block_id":"sb_x","name":"Auth","purpose":"..."},
  {"op":"merge","into":"sb_x","block_ids":["sb_y","sb_z"]},
  {"op":"split","block_id":"sb_x","by":{"paths":[["a/**"],["b/**"]]}},
  {"op":"move_member","path":"src/hook.ts","from":"sb_x","to":"sb_y"},
  {"op":"resolve_seam","path":"src/shared.ts","resolution":"primary:sb_y"},
  {"op":"assign_unmapped","path":"scripts/x.sh","block_id":"sb_x"}
]}
```

Signed behavior: one ATOMIC OCC batch under `expected_store_version` validated
preflight-on-a-clone (one invalid op → the whole batch persists NOTHING, its
index named); it REFUSES on a ratified skeleton (`skeleton_not_candidate` —
candidate-only). Provenance per touch: `named_by` is `runner` | `owner` |
`heuristic`. `candidate_lease {acquire|release|refresh, agent_id, ttl_secs}` is
ADVISORY — TTL-bounded, expired-is-reclaimable, and it NEVER blocks the owner (a
dead agent must not trap the candidate). **Ratify is EXCLUSIVELY human.** No agent
ratifies a skeleton, ever — and `system_blocks_ratify` REJECTS any block still on
a raw untouched heuristic label (`needs_owner_naming`). The heavy case (a large
monorepo candidate) has a one-gesture escape: a CURATION MISSION where a hand
edits the whole candidate through the SAME `candidate_edit` verb — the human
reviews the polished result and ratifies. The hand proposes; the human signs.
`candidate_naming` is the screen's HTTP-only route, not an MCP loop verb.
**Curation is PROPOSE-APPLY (F12):** the `curation_spawn` HTTP-only verb sends the
candidate to a pinned live hand-runner (via the announced daemon's `/curate`); the
hand PROPOSES a batch of `candidate_edit` ops as DATA, and the OWNER sanitizes (o5,
seat `runner`) and applies them under OCC, then posts the summary letter. The hand
never holds a write surface — not REST, not MCP, not a file — and can NEVER ratify.

**The mission letter (F2.5).** A mission's live state is one
`m1nd-mission-letter-v0`, posted with `mission_post` (WRITE, deny-listed on
read-only owners):

- `brain_ref` is the brain's DISPLAY NAME — the basename of its project root,
  case-sensitive — never an absolute path, and never the skeleton id's slug. A
  letter naming a different brain is refused (`brain_mismatch`); this killed the
  reconnect-collapsed mis-route that silently posted into the bound brain's box.
- `block_id` must name a real block in the DISPATCHING brain's skeleton, else
  `unknown_block`. A whole-skeleton mission (e.g. the F11 curation dispatch)
  anchors at the store's skeleton id, which validates like a block. A legitimately
  synthetic letter (a smoke test or warm-pool probe) sets `synthetic:true` (the
  declared escape) and skips the guard — never a silent pass.
- A letter is STATE, not evidence: it NEVER changes a block's color; only
  `receipt_import` does (the anti-poison law). `landed` is RESERVED for a
  confirmed imported receipt — a green gate without an imported receipt is
  `merge_wait` ("gate green — receipt not landed"), never `landed`.
- Ordering is causal: each mission's letters form a hash chain (`mission_seq` +1,
  `prev_letter_id`); a stale head is refused (`stale_head`, CAS on the head
  pointer). Runnerd emits the `executing`→`merge_wait` letters but holds NO
  `receipt_import` permission — the human lands from the tray.

## Scope Binding Taxonomy

Before treating a m1nd result as stale, broken, or authoritative, classify the
relationship between the requested scope and the active binding:

- `full_repo_binding`: the active workspace/ingest root is the repo being
  investigated, or the requested scope is contained by that root. Proceed with
  normal m1nd-first, then prove final truth directly.
- `wrong_workspace_binding`: the active workspace is a different repo. Rebind
  the host with `M1ND_WORKSPACE_ROOT=/target/repo`, intentionally ingest the
  target repo, or use federation only for genuine cross-repo work.
- `nested_workspace_binding`: the active workspace/root is a subdirectory of the
  requested repo. Retrieval is valid only for that subtree. Rebind or ingest the
  repo root before repo-wide claims.
- `file_level_binding`: ingest roots are docs, PRDs, L1GHT files, or generated
  handoffs inside the repo. Use them as document truth only; they do not prove
  implementation coverage.

Operational rule: do not keep trying `seek`, `search`, or `activate` against a
nested/file-level binding when the task is repo-wide. Upgrade the binding once,
run an isolated `--workspace-root /target/repo` probe, or switch to direct
source/test proof and record `m1nd_usage_mode=partial_scope_orientation`.

## Mission Control (not the default loop)

Mission Control is NOT the default operating loop and NOT how ordinary reviews,
bug hunts, or refactors run. The default is `north` → verbs → `memorize`; the
composable close is `Stop → cross_verify(evidence_freshness) → memorize(claims,
evidence)` DIRECTLY — `memorize` takes free-form structured claims with evidence
paths and needs NO `mission_id`. Reserve `mission_*` for `SubagentStop` and for
the rare turn where a mission is genuinely open (a subagent whose entire job was
one scoped mission); it is never the default `Stop` path.

When a mission IS open, the loop is deliberately small:

1. `mission_start` creates the repo-scoped mission, route, budget envelope,
   starter moves, and non-claims.
2. `mission_event` records observed actions when available; otherwise
   `mission_next.last_event` can carry the latest action.
3. `mission_next` appends the last event and returns one recommended move plus
   `do_not` guardrails.
4. `mission_verify` treats a conclusion as a candidate claim. Graph-only or
   inferred evidence is not enough; source reads, tests, compiler/runtime
   output, or focused probes are required.
5. `mission_handoff` serializes verified claims, open hypotheses, dead paths,
   graph anchors, and the next required move for another agent or future
   session.
6. `mission_close` emits the proof packet: verified claims, rejected claims,
   tools observed, event digest, budget consumed, gaps, and non-claims.

Evidence rule: a direct mission event only proves a claim when the claim's
`evidence_refs` names the event id or direct source/test/runtime proof. Do not
let one direct read bless unrelated graph-only claims. When `mission_next` says
to switch to direct proof, stop spending graph budget unless you record a
dissent event. Mission Control is not a host repair tool, graph correctness
proof, or autonomous multi-agent orchestrator.

**Work runs INSIDE — the burst wears the wire (the immune-arc P0 doctrine).**
The confession the ORGANISM-INSIDE arc answers: a guardian can run an eight-PR
day with six executors and the organism's mission board sees NONE of it. So when
you ORCHESTRATE a burst — dispatching ≥2 executors, or landing a BIG change —
open ONE mission card so the work is visible ON the organism, not off-book:
`mission_start {agent_id, repo, mode, budget, risk, task}` at the start (over the
wire, or the REST loopback `POST /api/tools/mission_start` against the served
owner), `mission_event` at each milestone, `mission_close` with the honest
outcome at the end. A mission-control card is SINGLE-AGENT (`mission_event` /
`mission_close` enforce the card's own `agent_id`), so the burst posts under the
orchestrator's id — executors report back and the orchestrator posts; they do NOT
each open a card (anti-spam: ONE card per burst THEME, never one per executor).
NEGATIVE DEFAULT, like the voice: a card is for a REAL burst (≥2 executors or a
BIG change), never for a trivial one-file touch. The card is a TRAIL, never a
GATE — it records what happened; the deterministic gate still proves the work,
and no card auto-lands (the map colors only by a human `receipt_import` on the
mission-letter board, the always-law — a mission-control `mission_close` closes a
trail, it never colors a block).

## Short-Audit Route

Use `m1nd-short-audit` when the task is a small or localized bug hunt, a narrow
review, or a tiny repo where source/runtime proof is likely cheaper than deep
graph navigation.

The route is:

1. Call `north(task)` for the one-round-trip orient (trust + context + memory +
   sufficiency + `next_move`); `needs_ingest` -> the playbook's named repair (refresh if this brain declares your root; otherwise OFFER the human `m1nd init --birth <repo>`) -> `north` again. Drop
   to `trust_selftest`/scoped `session_handshake` + `recovery_playbook` only if
   trust looks off or retrieval is blocked.
2. If needed, perform one bounded recovery/ingest pass.
3. Run one or two cheap orientation calls when `north`'s anchors are not enough:
   `search`, `seek`, or `activate` (or `audit` for a wider sweep).
4. Stop graph exploration once suspect files and behaviors are visible, obeying
   the verdicts (`abstain` = stop, not weak-yes).
5. Prove with direct source reads, git diff, tests, compiler/runtime output, and
   focused probes.
6. Record `m1nd_usage_mode=short_audit_orientation` when it helped, or
   `recovery_overhead` when state repair consumed meaningful time.

For local helper use, prefer the first-class agent CLI:

```bash
m1nd agent first-minute \
  --repo /path/to/repo \
  --query "understand this system" \
  --json

m1nd agent next \
  --repo /path/to/repo \
  --query "focused subsystem or bug surface" \
  --json

m1nd agent orient \
  --repo /path/to/repo \
  --query "focused subsystem or bug surface" \
  --mode short \
  --json
```

`agent first-minute` is the HOST-NEUTRAL CLI escape hatch for stale, unbound, or
not-yet-loaded sessions — an out-of-session entry, not the in-session front door
(`north` is). Reach for it for first contact from a stale MCP client, broad
architecture/audit requests outside a live binding, or when an agent has not yet
loaded the m1nd operating doctrine. It scopes the repo, establishes trust, runs
one bounded orientation pass, returns anchors, and emits `do_not` guardrails plus
a direct-proof handoff.

It is no longer isolated by default: before booting anything it asks the runtime
`--attach auto`'s two questions (`m1nd-mcp --discover-owner`, read-only, no
lease) and BRIDGES to a live serve owner whose declared ingest roots cover
`--repo`, so on a machine with a served owner the first minute reads the real
graph instead of an empty sidecar. `runtime.boot` is `attached_serve_owner` or
`isolated_runtime`, and `runtime.owner_discovery` carries the owner or the
refusal. `--no-attach` forces the isolated runtime.

`agent next` emits an `m1nd-agent-action-envelope-v0` with the first safe move,
so use it when you are choosing between scope, trust, orient, context, recover,
or direct proof. `agent orient` returns `schema=m1nd-agent-cli-v0`, records
whether the lane was `short_audit_orientation` or `recovery_overhead`, and
always tells the agent to switch to direct proof. Use `probe_m1nd.py
short-audit` only as a compatibility fallback when the npm CLI is unavailable,
and raw `probe_m1nd.py run` only when you need a custom sequence of multiple
tools.

`agent context` is anchor-first. Use it after `first-minute`, `next`, `orient`,
or a direct source read identifies a concrete file. For broad narrative queries,
let it refuse and route back to orientation rather than accepting a plausible but
wrong capsule.

For broad audits, hard bug hunts, multi-repo systems, docs/L1GHT work,
long-running investigations, security/risk review, or explicit full-system
requests, escalate to `references/full-spec-agent-os.md`. It is the route table
for the whole m1nd/L1GHT tool surface; treat it as a router, not a checklist.

## Session Companion Routing

Some hosts expose an adjacent session-memory companion, for example COMPANION. Use
that layer when the bottleneck is conversation continuity: north star, prior
decisions, open loops, handoff context, workstyle/method friction, or a scoped
`m1nd flash` summary stored with the session.

Do not use that companion as code truth or as a replacement for m1nd's repo
binding. The safe routing split is:

- Companion memory: why the work exists, what was already decided, and what open
  loops remain.
- `m1nd agent next`: the first safe repo move when choosing between scope,
  trust, orient, context, recover, or direct proof.
- m1nd MCP tools: graph, docs/L1GHT, impact, validation, mission control, and
  connected structural context.
- Direct proof: source files, tests, compiler/runtime output, logs, browser
  smoke, and focused probes.

If the host exposes only the companion wrapper and no direct `m1nd` MCP tools,
classify the session as `missing_m1nd_host_tool_surface`, not as graph failure.
Try the host-neutral CLI before abandoning m1nd for raw local search:

```bash
m1nd agent next --repo /path/to/repo --query "current task" --json
```

Before trusting companion output, confirm the companion session is bound to the
same repo/project root as the task. If it reports missing scope, wrong project,
global-only candidates, unavailable flash, or stale memory, classify it as
`companion_orientation_only` and resume with the host-neutral CLI:

```bash
m1nd agent next --repo /path/to/repo --query "current task" --json
```

Global companion search is candidate discovery only. It can point at useful
prior sessions, but it must not override the current repo's m1nd trust loop,
source reads, tests, runtime probes, or CI evidence.

## Core Rules

- Prefer the live MCP surface over stale prose. If tool names, counts, or parameters matter, run the bundled helper from this skill directory: `python3 scripts/probe_m1nd.py tools`.
- Keep `agent_id` stable within one investigation. Change it only when intentionally starting another role or another concurrent investigation.
- Ingest first. Re-ingest after code changes, or use incremental ingest for code repos when appropriate.
- If a m1nd tool call fails with `Transport closed`, treat it as a host MCP
  transport death, not as a graph, retrieval, or proof-state failure. Recovery
  tools cannot run through a closed transport. Verify the local binary with the
  repo smoke harness, kill stale `m1nd-mcp --stdio` processes if you own that
  host, then restart/rebind the MCP client or open a fresh thread. After the
  host relaunches the transport, run `trust_selftest` or `session_handshake`
  before relying on retrieval.
- If the host is launching an old native runtime, use the external self-update
  helper first: `m1nd update check --channel beta`, `m1nd update plan --channel
  beta`, then `m1nd update apply --channel beta --yes`. In live multi-agent
  sessions, add `--no-kill` and rebind only the selected host. This helper does
  not ingest, choose a workspace, repair graph contents, or refresh an
  already-open client's cached MCP tool list. `m1nd restart --source
  /path/to/m1nd --yes` remains the lower-level source-checkout repair path for
  development builds.
- If the uncertainty is host-specific, run `m1nd hosts status --host all
  --project /path/to/project --json` from the CLI before mutating anything. It
  is read-only and reports agent-pack presence, likely MCP config wiring,
  runtime/PATH alignment, workspace hints, and `host_rebind_proven=false`.
  If host config selects an absolute current managed runtime, a stale
  `m1nd-mcp` on `PATH` is a shadow warning only. If the host launches `PATH` or
  config is unknown, stale `PATH` is actionable. In both cases, confirm with
  `hosts status`/`hosts plan`, then rebind or open a fresh host session; do not
  claim an already-open client's cached tool list refreshed itself.
  If the host reports `attention`, run `m1nd hosts plan --host all --project
  /path/to/project --json` for the exact install, config, workspace env,
  rebind, and verification recipe.
  Use `m1nd hosts apply --host all --project /path/to/project --yes --json`
  only when you want the local mutation step after status/plan. Without `--yes`
  it is still a dry-run preview. With `--yes`, it can install or refresh
  agent-pack files and write canonical MCP config snippets for known hosts, but
  it does not prove rebind, refresh cached host tool lists, repair graph state,
  or remove the manual config step for generic hosts. `plan`/`apply` also emit
  each host's SessionStart-family hook (routed through `m1nd-north-shim`) and a
  per-host doctrine file for the TIER-A and TIER-B hosts.
- The default front door is `north(task)` (it composes `trust_selftest` +
  orient + boot_memory + focus in one round-trip). Call `trust_selftest`
  directly as a RECOVERY route — when trust looks off, `north` degrades, or you
  need just the binding sub-check — and route by `verdict` before relying on
  retrieval. `full_trust` means proceed with m1nd-first; `needs_ingest` means
  ingest the intended repo; `orientation_only` or `degraded_host_tool_surface`
  means use m1nd only for orientation and verify final truth with local files
  until the binding is refreshed; `wrong_workspace_binding` means the active
  graph is healthy but bound to the wrong repo for the requested scope;
  `stale_binding_suspected` means compare binding fingerprints and follow the
  recovery playbook before trusting retrieval.
- If `trust_selftest` is not exposed but `session_handshake` is, call the
  handshake and route by `trust_mode` as the cheaper sub-check. When the task
  names a target repo or absolute path, pass it as `scope` so Context Guard can
  detect cross-repo binding mistakes before retrieval.
- If the selftest verdict or handshake trust mode is not `full_trust`, or
  retrieval returns `blocked`/zero candidates unexpectedly, call
  `recovery_playbook` before inventing the next step. Use its ordered steps and
  `binding_fingerprint` to compare host, stdio, HTTP, runtime root, graph paths,
  generation counters, and ingest roots. The fingerprint is budget-capped: it
  carries `ingest_root_count` (always the real total) plus the first 10 roots,
  and declares any omission in `ingest_roots_truncated` /
  `ingest_roots_omitted`. When you need the whole array, read `doctor` under
  `runtime_state.ingest_roots` — the surface `ingest_roots_full_surface` names.
- If a retrieval/orientation response includes `agent_runtime_contract`, treat
  it as the authoritative agent-facing envelope for that call. Read
  `trust_mode`, `session_identity`, `workspace_binding`, `graph_identity`, and
  `recovery.arguments` before interpreting `results: []` or `modules: []`.
  `wrong_workspace_binding` means rebind or intentionally ingest/federate the
  requested workspace; `needs_ingest` means cold graph; and
  `retrieval_needs_recovery` means pass the embedded payload to
  `recovery_playbook` before falling back to shell search.
- If a response includes `context_guard.wrong_workspace_binding=true`, stop the
  normal stale-graph path. Rebind the MCP host with `M1ND_WORKSPACE_ROOT` set to
  `requested_workspace_hint`, intentionally ingest that workspace on the same
  binding, or use `federate_auto`/`federate` only when the investigation truly
  spans repos. If the live host cannot be rebound in the current turn, run an
  isolated local probe with `--workspace-root requested_workspace_hint` and use
  that as bounded m1nd orientation before direct source/test proof. Do not treat
  this as proof that m1nd retrieval is broken.
- If `wrong_workspace_binding` is reported but the active root or ingest roots
  are nested inside the requested repo, classify it as `nested_workspace_binding`
  or `file_level_binding`. The graph may be useful for that sub-scope, but it is
  partial truth; rebind/ingest the repo root before repo-wide claims.
- If `trust_selftest` or `session_handshake` reports `needs_ingest`, or the
  mini `graph_state.node_count` is `0` while `ingest` is available, treat the
  session as a recoverable cold graph. Do not jump straight to shell fallback.
  Call `ingest` on the same MCP binding with the absolute intended
  repo/workspace path, never a managed runtime/session path such as
  `~/.codex/m1nd-runtimes/...`, `~/.claude/m1nd-runtimes/...`, an Antigravity
  agent runtime, or a generic `mcp-runtimes`/`agent-runtimes` folder. Host
  integrations should prefer `M1ND_WORKSPACE_ROOT`; m1nd also recognizes common
  workspace hints from Claude Code, Antigravity, Gemini, Cursor, Windsurf, VS
  Code, and shell/package-manager env vars. Then rerun `session_handshake` and
  one cheap retrieval. Fall back only if ingest is unavailable, ingest fails,
  or post-ingest retrieval is still blocked and `recovery_playbook`/`doctor`
  confirms stale binding or degraded host surface.
- If the host exposes `health` but not `trust_selftest`, `session_handshake`, or
  `recovery_playbook`, read `health.tool_surface_contract` and
  `health.host_binding_alignment`. Treat missing required host-visible tools as
  `degraded_host_tool_surface`, then verify with repo-local smokes or direct
  files until the host refreshes its binding.
- After ingest, sanity-check that retrieval is seeing the same active graph. If
  `seek`, `search`, or `activate` returns `blocked`, zero candidates, or an
  unexpectedly empty graph immediately after a successful ingest, suspect
  host-binding/session split-brain before blaming the repo or the m1nd core.
  If the response includes `recovery.arguments`, pass those arguments directly
  to `recovery_playbook`. Otherwise, call `recovery_playbook` with
  `observed_tool`, `observed_proof_state`, and `observed_candidates` from the
  suspicious response before falling back. Let the playbook decide when to call
  `doctor`.
- If the host tool surface exposes m1nd but is missing recovery tools such as
  `ingest`, classify the session as `degraded_host_tool_surface`. If `doctor`
  is available, call it with `observed_tool="tools/list"`,
  `observed_tool_count`, `available_tools`, and `missing_tools`. Until the MCP
  binding is refreshed, use m1nd only as orientation and verify final truth with
  direct repo files.
- Make `m1nd` the first investigative step before shell search:
  - exact text need -> try `search` before `rg`
  - path pattern need -> try `glob` before filesystem globbing
  - implementation-by-purpose need -> try `seek`
  - subsystem/topic/connected neighborhood need -> try `activate`
- Treat `proof_state`, `next_suggested_tool`, `next_suggested_target`, and `next_step_hint` as workflow control signals, not decorative fields.
- Use the cheapest surface that preserves structural truth:
  - exact text -> `search`
  - path pattern -> `glob`
  - known file -> `view`
  - known purpose, unknown location -> `seek`
  - topic/subsystem/neighborhood -> `activate`
- For docs/specs/knowledge, decide the lane early:
  - authored as graph-native semantic markdown -> `ingest` with `adapter: "light"`
  - ordinary markdown/wiki/HTML/PDF/office docs -> `ingest` with `adapter: "universal"` or `adapter: "auto"`
- Before risky edits, route through `impact`, `validate_plan`, and usually `surgical_context_v2`.
- In Codex, prefer `m1nd` for analysis, planning, and context. If the task requires local file edits under Codex's editing rules, use `apply_patch` for the final file mutation unless there is a specific reason to use `m1nd`'s write surfaces.

## Fast Routing

- Unfamiliar repo or need a one-call orientation: start with `north(task)`
  (`needs_ingest` -> the playbook's named repair, never a bare `ingest` -> `north`), then `audit` for a wider sweep and
  `batch_view`, `coverage_session`, or `cross_verify` as needed.
- Need a subsystem map: use `activate`.
- Need code by intent: use `seek` — obey its `trust_envelope` + sufficiency
  signal (`sufficient` = stop; `gathering`/`saturated` = widen/refine).
- Need why A connects to B: use `why` — a `closure: blocked` verdict means the
  path rests on an unresolved edge; verify that edge before relying on it.
- Smells like missing validation, abstraction, cleanup, or lock: use `missing`.
- Have a stacktrace or runtime error text: use `trace`.
- Need blast radius before editing: use `impact`.
- Need co-change follow-through after editing: use `predict` — obey
  `act`/`reverify`/`abstain` (`abstain` = stop; verdicts cap at `reverify` until
  `calibrate_predict` has armed the gate once).
- Need plan completeness and missing tests before implementation or review: use `validate_plan`.
- Need graph-native specs, design notes, or KB docs authored in `L1GHT`: ingest with `adapter: "light"` and usually `mode: "merge"`.
- Need regular spec/wiki/PDF/doc alignment with code: ingest with `adapter: "universal"` or `auto`, then use `document_resolve`, `document_bindings`, and `document_drift`.
- Need hidden coupling, deep architecture quality, taint/security paths,
  duplication/refactor seams, or runtime heat: use the RETROBUILDER family.
  `ghost_edges` finds historical co-change, `taint_trace` follows trust
  boundary/sensitive flow, `twins` finds structural duplicates,
  `refactor_plan` proposes extraction communities, and `runtime_overlay`
  overlays span/log heat onto graph nodes. Treat these as hypotheses and
  anchors until direct source/test/runtime proof confirms them.
- Need stateful navigation instead of stateless retrieval: use `perspective_*`.
- Need session continuity or handoff: use `trail_save`, `trail_list`, `trail_resume`, `trail_merge`, and sometimes `boot_memory`.
- Need standing structural freshness: use `daemon_start` (per-brain opt-in; advances when the brain is seen — verb traffic — never a free-running monitor), `daemon_status`, `daemon_tick`, `alerts_list`, and `alerts_ack`.
- Need to persist a durable finding across sessions: use `memorize` with `evidence` paths pointing at the relevant code; ingest that code first so evidence anchors. Use `mission_close(write_light_memory:true)` for one-step mission + memory commit.
- Need to check whether memorized claims still cite current code: use `cross_verify(check:["evidence_freshness"])`; returns `stale_evidence[]` + count.
- Need to know how much to trust the project's PATHOS handoff (near a PR / doc-gate): use `soul_check` — it parses `docs/PATHOS.md` into anchored claims and returns the freshness receipt (N fresh · M stale · K priced @sha), with declared tissue counted but never fake-verified. `soul_read` pulls the body. The curator is a workflow: sweep → verify → `memorize {soul_source}` → prune-never-silently → re-check; its report is seat-verified by a DIFFERENT agent via `soul_check {verify_curator_report}` (§C8.4).

## Compounding Memory

When you conclude something durable — a verified finding, a design decision, why code is shaped a certain way — persist it with `memorize` rather than leaving it only in the conversation. The workflow:

1. Ingest the relevant code first (`ingest` with the target path), so evidence paths resolve to real code nodes.
2. Call `memorize` with structured claims: `label`, `text`, `confidence` (low/medium/high/certain or 0.0–1.0), `ambiguity` (optional), and `evidence` (repo-relative paths).
3. m1nd writes a `.light.md` under `<runtime_root>/agent-memory/`, ingests it, and creates `grounded_in` edges linking claim nodes to the actual code nodes.
4. On the next session start, m1nd auto-loads all agent-memory files (reported in `session_handshake.agent_memory`). Past findings are in the graph immediately.
5. After code changes, check freshness: `cross_verify(check:["evidence_freshness"])` names which memorized claims cite code that has since changed. The ingest result itself also includes `memory_freshness` after a merge re-ingest.
6. For mission-driven work, `mission_close(write_light_memory:true)` persists verified claims as L1GHT memory in one step.

`memorize` is the POST-CAPTURE close: before ending, memorize every durable finding with `confidence` + repo-relative `evidence` paths so it lives in the same activation space as code and self-flags stale via `cross_verify(check:["evidence_freshness"])`.

Then one field-telemetry signal, keep working (report, never detour): when a retrieval was right/wrong, `learn(correct|wrong|partial)`; when m1nd ITSELF misbehaves (bug, friction, or honesty miss — claimed fresh/closed/act and was wrong), append ONE JSON line to `~/.m1nd/field-reports.jsonl` (`{ts,agent,repo,tool,class:"bug|honesty|friction|win",what,expected,snippet}`) — LOCAL-ONLY, never phones home. A memory-delivery fault (a claim leaked across brains, a false absence, a write to the wrong store) is `class:"memory_misdelivery"` + `kind:leak|false_absence|wrong_store_write|misattribution|vanished`. NEVER fix m1nd mid-mission; work around, note the workaround, report.

The mailbox (MEDULLA §9.2, slice M7b): the spool is the ONE write slot; letters distribute LOCALLY into per-project boxes (`<repo>/.m1nd/inbox.jsonl` — born local behind a consent-deferred `.gitignore`; the repo's own `m1nd init` is the ONE consent moment that flips it to committed and lets it travel with git, so do NOT assume a box is shared) + the medulla box for projectless letters — a letter naming a project NEVER lands in the medulla. Triage with `m1nd-mcp --inbox-sweep` (CLI) or `GET /api/inbox_sweep` (union of spool + every box, each letter once); read one project's box via `GET /api/mailbox?brain=<root>` (the §4A.9 selector, `served_brain` echo). Fates are derived from `answers[]` (`wet_ink`/`in_flight`/`fired_clay`/`external`); "abertas" = `wet_ink + in_flight`. These are CLI/REST surfaces — NOT MCP tools (never in the loop).

Caveat: `ingest mode:replace` wipes light memory nodes. Prefer `mode:merge` when re-ingesting code to preserve agent memory.

## Serve/attach era — transport truth

The runtime is a served OWNER plus thin bridges, not one process per host. A
single served owner (default port `1338`) holds the live graph; hosts and probes
ATTACH to it — there is no exclusive lease in attach mode, so many bridges share
one graph:

- Host bridge: `m1nd-mcp --attach http://127.0.0.1:1338` speaks stdio MCP to the
  host and forwards every frame to the owner's `POST /mcp`, relaying the owner's
  server→client push notifications back. It loads NO graph and takes NO lease.
- `--attach auto` instead of a pinned URL: it asks the registry TWO questions —
  an owner for this runtime root, then (failing that) a live owner whose declared
  ingest roots COVER this repo. The second question is what lets an agent working
  inside a repo the served owner already ingested reach the real graph instead of
  an empty repo-local `.m1nd`. It resolves a worktree to its main repo, REFUSES
  naming both when two owners cover one repo, and reads the token from the
  RESOLVED owner's runtime root. Corollary for honesty: an empty brain in a repo a
  served owner covers is a WIRING fault, not calibrated absence — check what
  `--attach auto` resolved before reporting the emptiness as truth.
- Direct probe: `POST /mcp` with header `M1nd-Caller-Root: /abs/repo/root` — the
  header is the caller identity reception verifies (`M1nd-Caller-Root` ↔
  `covers_root`); a mismatch returns the degraded reception, not a fabricated
  orientation.
- Runner daemon: `m1nd-runnerd` (its own port, `1339`, launchd) is the ONLY
  spawner — the owner never spawns. It runs a mission packet in an isolated
  worktree-per-mission and posts letters from `executing` onward, but holds NO
  `receipt_import` permission (the human lands from the tray). Capabilities are
  pinned owner-side (`runners.toml`); the announce proves liveness only and can
  never grant or widen a capability.

A freshly-landed verb appears ONLY after the owner is rebuilt and kickstarted —
a live verb needs the served owner restarted (`m1nd restart --source
/path/to/m1nd --yes` for a dev checkout, then reload the managed service). Bridges
opened AFTER an owner restart re-recover on their own; a long-lived bridge from
BEFORE the restart must reconnect (restart the host session). A persistent
"failed to connect" means the owner is down — bring it back, do not thrash. If
the tools drop mid-session, the owner was restarted underneath you: reconnect,
do not conclude m1nd is broken.

## Delegation Layer

Spawning a subagent? Compose the RETRIEVAL half of its spec in ONE read-only
call instead of hand-writing context.

`delegate {agent_id, task}` returns a packet whose core is `prompt_markdown` you
APPEND verbatim to your brief. It carries:

- `mission.binding` — the NAMED brain the child must land on. This is the SAME
  datum reception verifies (`M1nd-Caller-Root` ↔ `covers_root`), so the child
  VERIFIES it landed (silent on a match) rather than choosing — the child law.
- a LABELED memory slice — each row is `- [tier] claim — origin · author, age`,
  `tier` ∈ `project | medulla`, so the child inherits doctrine-vs-project-fact,
  not a flat blob. The slice is your DEFAULT beat (project task-relevant claims +
  the medulla doctrine the domain touches) — never `all-brains`, never another
  project's private claim.
- ranked anchors, a staleness header, known static dependents, and an explicit
  "what m1nd could NOT determine → your duties" section. Read the appendix law:
  your text wins on WHAT-TO-DO, the packet wins on WHAT-IS, the packet outranks
  assumption only.

`delegate` abstains HONESTLY — `needs_ingest` (empty graph), `unscopable` (the
task activates no coherent subgraph), `seeds_unresolvable` (every seed fails) —
always with evidence + a `next_move`, never a bare no. It is PROJECT-TIER: no
`all-brains` fan-out, and no predict/trust/tremor/xray enrichment yet; each
omission is stated in `non_claims`, never hidden.

When the subagent returns, `debrief {agent_id, delegation_id, outcome,
diff|touched_paths, findings}` grades its real diff against the packet and TEACHES
the graph — the ONLY mutation, through `memorize`/`learn`. It classifies each
touched path (`in_scope | expected_change | dependent_contact | unpredicted`)
with a worst-of verdict that carries fence existence ("stayed — no ratified
boundaries existed"), memorizes the subagent's findings under the SUBAGENT's id
and any map-miss lessons under YOURS (a clean run memorizes nothing), and appends
one `outcomes.jsonl` row stamped `outcome_unverified` unless you attach
`evidence`. Conformance grades PATHS, never code quality — it never says
merge-safe. Every debrief deposits memory the next `delegate` surfaces, so
skipping it wastes knowledge.

Subagent report protocol (what the child returns to you): a one-line header
`[m1nd dlg_<id>] landed <brain> · <N> anchors · <M> memory` (or the abstain +
its `next_move`), a `DEVIATIONS:` block naming every touched path outside the
packet's predicted set with a one-line why, and a `FINDINGS:` block of durable
claims (each with an `evidence` path) — the exact input your `debrief` grades.

## Promotion — the audited crossing

`memorize` is ALWAYS project-private; a finding does NOT become shared doctrine
by being written. The public crossing syntax is:

```json
promote {"brain": "<project_root>", "claim": "<slug>", "reason": "<one line>"}
```

It is currently fail-closed at `POSITIVE_SOVEREIGN` until an exact typed G2
authority consumer is installed. `State: verified`, founder/source labels,
caller identity, and arbitrary lease strings are evidence or metadata — never
authority. The intended operation copies a transversal claim into the medulla
with the full readable provenance chain while retaining the project witness, but
only an owner-resolved internal path may perform that crossing until the public
authority contract is mechanically proved. A maker may record “candidate for
promotion”; that is a proposal, never permission. The verb is served at the
routed HTTP door; a fresh live verb needs
the served owner rebuilt/kickstarted.

## Memory Honesty and the Soul Receipt

An empty memory beat is NOT proof of an empty store. `north` (and the recall
surfaces) stamp `memory_exists: N` — the on-disk L1GHT claim count — plus an
honest note when recall found no task-relevant match over a non-empty store.
Never conclude "no memory": if `memory_exists > 0` while the beat is empty, the
store simply held no task-relevant match — widen the task text, raise
`top_k`/`token_budget`, or `seek` the store directly.

Near a PR or doc-gate, price the handoff before trusting it. `soul_check` parses
a repo's soul (`docs/PATHOS.md`) into anchored CLAIMS, classifies each
(`path | line-hint | symbol | git | consistency | receipt | runtime | declared`),
verifies per class, and returns the honesty report plus a one-line FRESHNESS
RECEIPT:

```text
N fresh · M stale · K receipt-priced, checked <date> @<sha>
```

That line is what a cold context reads to know how much to trust the handoff.
THE TWO TISSUES hold: verifiable tissue (Current State / Access Map / Known
Problems) is machine-checkable; DECLARED tissue (North Star / Doctrine / taste /
why-we-work-this-way) is UNPROVABLE-but-curated and NEVER fake-verified — the
system knowing what it cannot verify IS the honesty. `soul_read` pulls the body
(whole or one section) — the explicit pull, never ambient. THE CURATOR is a
near-PR WORKFLOW (agent judgment on a deterministic substrate): sweep with
`soul_check` → verify against code/git/runtime → update durable claims via
`memorize {soul_source: "<path>#<section>"}` (the ONE write door) → prune stale
NEVER silently (every removal named + where it went; git keeps the text) →
re-check → carry the receipt in the PR body. Who verifies the curator (§C8.4):
its report must pass `soul_check {verify_curator_report: <report>}` run by a
DIFFERENT agent — grader ≠ author.

## The m1nd Voice — rendering `human_view`

The north packet carries `human_view` (`m1nd-human-view-v0`): the m1nd voice for
the HUMAN — a server-composed, already-mounted card (the `m1nd` wordmark + the
PULSE row + `│` gutter fixed at column 6; ≤4 lines, ≤80 chars/line; states
`clean | bell |
coherence | mismatch | needs_ingest`; a mechanical `state_sig`). Render it by
joining `lines[]` with newlines inside a fenced code block — never re-compose
it, never decorate it. Every line is a measured fact or a verbatim server
string (brand law G1: no uncalibrated adjectives, no benefit claims).

**Cadence — the default is NEGATIVE (verbatim law):** Do NOT render the card
unless m1nd contributed structurally to the mission AND the content is useful
to the human NOW; never in consecutive messages; never the same state_sig twice
in a session; on state change or first orient. When in doubt, stay silent —
silence is the honest card.

**The PULSE — the official signature (owner's stamp):** line 1 hangs `m1nd `
then FIVE cells — `trust · graph · focus · bell · coherence` — each calm `╷` or
raised `│` (e.g. `m1nd ╷╷╷│╷` = only the bell is calling). Read it as an
EXPRESSION (all low = calm; one stem standing up = look), never cell-by-cell;
the cell order is FIXED — never reorder or add one. Under a repo mismatch the
pulse is DROPPED and the plain `m1nd │ ` returns. Line 1 may also carry a
`map <N> blocks` segment (the served brain's ratified SystemBlock count). In the
DEEP rung you MAY render the legend `pulse ≔ trust ╷ · graph ╷ · focus ╷ ·
bell │ · coherence ╷`, and the two proof glyphs `⊢` (evidence ⊢ receipt; ASCII
`>`) and `∎` (a landed mission; ASCII `#`) — ONLY where the packet proves them,
never as decoration.

**The cockpit (`cockpit`) — ON REQUEST only:** when the human asks to look around
("?", "show me m1nd", "what can I check?"), call `cockpit` for the read-only
navigable menu; NEVER auto-serve it, and NEVER at a landing (there the card
speaks and the door is the tray). Its entries are argument-less reads and pointer
doors — a pointer (the tray) carries no verb, because the stamp is a human
gesture, never a cockpit click. Carry its `menu_sig` back verbatim when you
navigate (`select <slot>`).

**Duties:** translate the card's CONTENT into the conversation's language
keeping the geometry (gutter at column 6, ≤80 cols) and ids/state tokens
(`merge_wait`, `needs_ingest`, `full_trust`) intact. The DEEP rung (R2) is the
agent's: when the human asks ("what's the bell?", "show me m1nd") render a
deeper card FROM the packet's structured fields (`landing_bell`, the mission
tray, blocks) in the SAME grammar — never a fact the packet does not carry.
When a surface cannot hold unicode, apply the 1:1 ASCII map `╷`→`.`, `│`→`|`, `·`→`.`,
`—`→`-`, `⊢`→`>`, `∎`→`#` (widths identical, geometry never moves).

**Attribution — the second half of the voice:** narrate where m1nd helped ONLY
past the counterfactual test ("without it, would I have decided differently or
worse?") — it changed a decision, avoided a rediscovery, opened a front, proved
or refuted something. Consulting without effect = silence; never "used m1nd" as
merit; facts, never estimated savings (G1).

**The treacherous lexicon (translate these wrong and the human is damaged):**

| term | what it really means |
|---|---|
| `abstain` | a calibrated answer ("not enough evidence to act") — never a failure |
| `insufficient_evidence` | ZERO evidence, the honest cold start — never "medium risk" |
| `merge_wait` | waiting for the HUMAN stamp — never stuck or blocked |
| stale (receipt) | aged since it was earned — never invalid |
| `landed` | reserved for a receipt imported by the human gesture — never "gate green" |
| ratify | a human-only gesture; no agent ratifies, ever |
| reception mismatch | the serving brain does not cover YOUR repo — never "m1nd is broken" |
| fresh / stale | freshness is priced per boundary (the block's scope), never vibes |
| `full_trust` | the binding verdict (the graph covers and answers) — not a code-quality grade |
| the bell | a call to the human (missions await landing) — never an error |
| `needs_ingest` | "I don't know this repo yet" + who repairs it — never a crash. A repo with no brain needs the HUMAN's one-time `m1nd init --birth <repo>`; a stale one you refresh yourself |
| the tray | the human's door to land receipts — agents never land |

**The verb families (translate by family, not tool-by-tool):**

| family | the human metaphor | verbs |
|---|---|---|
| orient | the compass | `north`, `orient`, `audit` |
| retrieve | the flashlight | `seek`, `search`, `glob`, `activate`, `focus` |
| causality | the X-ray | `impact`, `why`, `trace`, `taint_trace` |
| simulate | the crystal ball | `predict`, `counterfactual`, `hypothesize`, `epidemic` |
| memory | the notebook | `memorize`, `boot_memory`, `promote`, `learn` |
| missions | the order board | `mission_post`, `mission_spawn`, `mission_*` |
| skeleton | the wall map | `skeleton_candidate`, `candidate_edit`, `system_blocks_*` |
| proof | the receipt ledger | `receipt_import`, `cross_verify`, `soul_check` |
| health | the doctor | `doctor`, `trust_selftest`, `health`, `recovery_playbook` |
| ingestion | the construction site | `ingest`, `auto_ingest_*`, `federate` |

## Read These References

- `references/routing-playbooks.md`
  - Use for end-to-end workflows by task type: onboarding, bug triage, risky change prep, spec-to-code work, multi-agent sessions, and long-lived monitoring.
- `references/tool-families.md`
  - Use for the complete capability map grouped by family, including the less obvious tools (`antibody_*`, `runtime_overlay`, `ghost_edges`, `flow_simulate`, `layers`, `refactor_plan`, etc.).
- `references/runtime-and-refresh.md`
  - Use for local installation facts, current live-surface notes, the docs-vs-runtime count discrepancy, refresh procedure, and the helper script usage.
- `references/l1ght-and-docs.md`
  - Use for the `L1GHT` mental model, marker vocabulary, header fields, `light` vs `universal`, and mixed code+docs graph workflows.

## Local Helper

Use `m1nd agent ...` whenever the live runtime matters more than remembered docs
and you need a host-neutral probe outside a stale MCP client.

```bash
m1nd agent scope --repo /path/to/repo --json
m1nd agent trust --repo /path/to/repo --ensure-ingest --json
m1nd agent orient --repo /path/to/repo --query "focused subsystem or bug surface" --mode short --json
m1nd agent recover --repo /path/to/repo --from wrong_workspace_binding --json
m1nd agent doctor --repo /path/to/repo --json
```

These reach the served owner themselves now: each asks `m1nd-mcp
--discover-owner` first (read-only, no lease) and bridges to the live owner
whose declared ingest roots cover `--repo`, reporting it under
`runtime.owner_discovery`. To ask only which owner a directory would reach:

```bash
cd /path/to/repo && m1nd-mcp --discover-owner   # exit 0 = found, 1 = none
```

The cheapest live probe of the graph itself is still a direct HTTP call to the
served owner — no runtime spawned at all, and it exercises the SAME graph the
hosts see:

```bash
# Probe the served owner directly (identity via the caller-root header).
curl -s http://127.0.0.1:1338/mcp \
  -H 'Content-Type: application/json' \
  -H 'M1nd-Caller-Root: /path/to/repo' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call",
       "params":{"name":"north","arguments":{"agent_id":"probe","task":"orient"}}}'
```

The bundled probe script remains a valid FALLBACK from this skill directory for
low-level MCP calls, older binaries, and no-owner situations (it launches its own
isolated runtime):

```bash
python3 scripts/probe_m1nd.py tools
python3 scripts/probe_m1nd.py call health '{"agent_id":"codex-m1nd"}'
python3 scripts/probe_m1nd.py call trust_selftest '{"agent_id":"codex-m1nd"}'
python3 scripts/probe_m1nd.py call session_handshake '{"agent_id":"codex-m1nd","scope":"/path/to/intended/repo"}'
python3 scripts/probe_m1nd.py call recovery_playbook '{"agent_id":"codex-m1nd","observed_tool":"seek","observed_proof_state":"blocked","observed_candidates":0}'
python3 scripts/probe_m1nd.py call help '{"agent_id":"codex-m1nd","tool_name":"validate_plan"}'
python3 scripts/probe_m1nd.py run '[{"name":"ingest","arguments":{"agent_id":"codex-m1nd","path":"/path/to/repo"}},{"name":"seek","arguments":{"agent_id":"codex-m1nd","query":"where retry backoff is decided","top_k":5}}]'
```

Use `m1nd agent orient --repo /path/to/repo --mode short --json` when the host MCP
session reports `wrong_workspace_binding` and you cannot restart/rebind that
host immediately. This is the preferred fallback before raw shell search: it
keeps m1nd useful while avoiding contamination of the open host's active graph.
Record the mode as `isolated_probe_after_wrong_workspace_binding`.

`probe_m1nd.py` uses an isolated temporary `--runtime-dir` by default so
parallel agent probes do not fight over the same runtime owner lock. If a
helper or older skill reports `runtime_root ... is already owned by instance`,
do not classify that as graph staleness or retrieval failure. Rerun with the
agent CLI, pass an explicit unique `--runtime-dir` to the probe helper, or
combine custom dependent calls with `probe_m1nd.py run` so they share one process intentionally. Use
`--shared-runtime` only when debugging shared runtime state.
The helper prefers `~/.m1nd/bin/m1nd-mcp` over a stale `m1nd-mcp` earlier on
`PATH`; override with `M1ND_MCP_BINARY`, `M1ND_MCP_BIN`, or `--binary` only when
you intentionally want another runtime.
For benchmark lanes or tight worktrees, add `--no-worktree-artifacts`; it
launches the runtime from the isolated runtime directory and sets the caller
directory as `M1ND_WORKSPACE_ROOT`, so probe metadata does not appear in
the target repo unless the runtime is explicitly configured to write there. If
you invoke the helper from a director repo while inspecting another checkout,
pass `--workspace-root /path/to/that/checkout` explicitly.

For the m1nd repo itself, prefer the repo-local agent smoke harness when you
need to distinguish a real runtime problem from a host-provided MCP binding
problem:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --json
python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --handshake-probe --json
python3 scripts/mcp_agent_smoke.py --repo . --json
python3 scripts/mcp_agent_smoke.py --repo . --transport http --json
```

Use `trust_selftest` as the cheap default when exposed. The current binary also
exposes the sub-check as `session_handshake`; the harness calls both when
available and falls back for older binaries. The default path must stay
diagnostic-only: no ingest, no repair, and no retrieval probe by default.
`recovery_playbook` is the in-band next-step surface when the selftest,
handshake, or retrieval looks suspicious. Add `--handshake-probe` only when the
task depends on retrieval trust.

That harness proves the minimum trust loop over real Content-Length framed
stdio and the HTTP tool API:

```text
initialize -> tools/list -> trust_selftest -> session_handshake -> recovery_playbook when needed -> ingest -> seek -> help -> doctor
```

What the helper is for:

- confirming the local binary still responds
- listing the live tool surface
- detecting `degraded_host_tool_surface` when required tools such as `ingest`,
  `seek`, `help`, `recovery_playbook`, or `doctor` are missing
- checking a tool's real response shape without relying on stale wiki prose
- catching graph/session continuity failures before falling back to broad shell
  search

## Working Posture

- Use `m1nd` when the question is about relationships, not just strings.
- Use `m1nd` first even when the answer might be textual, because `search`, `seek`, and `activate` can often narrow the surface before any shell reads.
- Use `m1nd` before big changes when hidden neighbors or missing tests could bite later.
- Use `m1nd` for continuity when the same investigation spans agents or sessions.
- Fall back to `rg`, direct file reads, compiler output, and runtime logs when execution truth is the real question.
