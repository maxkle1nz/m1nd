---
name: m1nd-first
description: Use when investigating a repository, searching for implementation, reviewing changes, working from specs/docs, or preparing a risky code change in an environment where m1nd is available. This doctrine makes m1nd the first investigative layer before grep, glob, or manual file reads, except when the task is pure compiler/runtime truth or the exact file and lines are already known.
---

# m1nd-first

This is a doctrine, not a manual.

## Rules

- Start with `m1nd`. In an MCP session, the front door is one call: `north(task)` — it
  composes trust, task context, prior cross-session memory, a sufficiency signal, one
  `next_move`, and `honest_gaps` into a single packet before you read or edit anything.
  If it returns `needs_ingest` (empty/unbound graph), do not call generic `ingest`:
  that mutation surface is policy-disabled. Use the isolated `m1nd agent ... --repo`
  CLI path for investigation, or the exact authority flow plus
  `external_mutation_service` for a governed existing-brain mutation.
- Before `rg`, shell globbing, or manual file reads, `m1nd` answers or narrows FIRST — that is the default, not a question you ask only when convenient. The one exception is the **Skip Conditions** below (exact file+lines already known, or pure compiler/runtime truth); everything else earns a `north`/`seek`/`impact` pass first.
- Prefer the cheapest `m1nd` surface that preserves truth:
  - exact text -> `search`
  - path pattern -> `glob`
  - known purpose, unknown location -> `seek`
  - topic, subsystem, or connected neighborhood -> `activate`
  - unfamiliar repo orientation -> `north` (or `audit` when you only want the structural map)
  - stacktrace or runtime error text -> `trace`
- For docs/specs/knowledge, do not advertise generic ingest adapters while the
  mutation route is policy-disabled. Read the supplied artifacts directly or use
  an owner-internal indexing path; label graph ingestion `NOT_AVAILABLE` unless an
  exact typed consumer and authority receipt are present.
- Before risky edits or change reviews, pass through `impact`, `validate_plan`, and usually `surgical_context_v2`.
- Keep `agent_id` stable across one investigation unless intentionally splitting roles.

## Trust-First Stance

Trust `m1nd` as the first layer because reading structure first catches what surface reads miss —
proven repeatedly: graph-grounded confrontations reversed designs before a line was written (a
watcher mis-aimed at documents, a "sacred" law that was 80% doctrine, a spec verb that did not
exist), each with the exact `file:line`. The trust is CALIBRATED, not blind, and that is WHY it
comes first: a calibrated absence — `caller_root_mismatch`, `abstain`, `gathering`,
`insufficient_evidence` — is the high-value answer that stops you editing the wrong repo, not a
failure to route around. The counterfactual attribution law (below, in **The m1nd Voice**) is the
same discipline pointed at credit: name m1nd's help only when, without it, you would have decided
worse. And the ruler that keeps trust honest: m1nd narrows and connects — the compiler, tests, and
runtime still decide truth; when a verb hangs (federated `seek`/`federate` can exceed 60s), when
freshly-`memorize`d claims lag semantic recall, or when a runner-named scan falls back to
heuristics, WORK AROUND and file one field-report line — never insist blindly, never conclude
"m1nd is broken".

## Trained Agent Loop

For unfamiliar repo work, audits, bug hunts, reviews, and risky changes, use
this loop by default:

1. Orient in one round-trip: `north(task)`. It composes binding trust, task
   context (focus nodes + PageRank anchors), prior cross-session memory (each
   claim with its real age + author), a sufficiency signal, one `next_move`, and
   `honest_gaps` (what m1nd does NOT yet know). If it returns `needs_ingest`,
   do not call generic `ingest`; use the isolated CLI investigation path or the
   exact typed authority consumer for an existing brain. `needs_ingest` is a real
   answer, not a failure. `north` composes `trust_selftest` + `orient` + `boot_memory` +
   `focus`; reach for the pieces directly only when you need just one. Heed
   `reception`: `reception.match == "caller_root_mismatch"` means the bound graph
   does NOT cover your current repo — do not trust retrieval for it; read
   `reception.options[]`. The public cross-root bootstrap consumer is not
   installed: never add `project_root` or `allow_overlap` to `ingest`, and never
   treat the internal owner bootstrap as an executable repair. The honest code is
   `brain_bootstrap_consumer_not_installed`; connect to an owner that already
   hosts the intended brain or continue only with explicit mismatch caveats.
   Reception governs WRITES, not just reads: a read under mismatch is a warning,
   but a WRITE under mismatch is PROHIBITED by doctrine — any write verb
   (`memorize`, `skeleton_candidate`, `candidate_edit`, `system_blocks_*`,
   `mission_post`) would land in the WRONG brain. That is exactly how a foreign
   skeleton once overwrote a bound brain. Do not write from that session.
   Absent/null `reception` = your root matches the brain serving you.
   An empty memory beat is NOT proof of an empty store: the packet carries
   `memory_exists: N` (the on-disk claim count) plus an honest note when recall
   found no task-relevant match over a non-empty store — never conclude "no
   memory" without checking the stamp; widen the task text or `seek` the store
   directly instead.
2. If trust is degraded or retrieval comes back `blocked`/empty unexpectedly,
   drop to the recovery path: follow `recovery_playbook` before interpreting
   absence. `wrong_workspace_binding` means rebind, intentional ingest, or real
   federation; it is not stale graph proof.
3. Follow the `next_move`, or route focused questions through
   `search`/`seek`/`activate`. Obey the calibrated verdict — do not override it:
   - retrieval/prediction return **`act` / `reverify` / `abstain`**; `abstain`
     means uncalibrated or insufficient evidence — a STOP, not a weak yes. The
     prediction gate is armed by `calibrate_predict` and the seek trust envelope
     by `calibrate_envelope`; until each is armed its verdict caps at `reverify`.
   - `why` carries a **`closure`** verdict — `blocked` means the path rests on an
     unresolved (guessed/dropped) edge; verify that edge before relying on it.
   - `seek` carries a **`trust_envelope`** + a sufficiency stop-signal —
     `sufficient` means stop gathering; `gathering`/`saturated` mean widen.
   - **`trust_band: insufficient_evidence` means NO evidence**, not medium risk —
     the honest cold-start answer.
4. Read the compact runtime envelope on retrieval responses. Empty results are
   not final truth until workspace, graph identity, and recovery state are
   coherent.
5. Verify with direct source, tests, compiler/runtime output, and focused
   probes. m1nd narrows and connects; execution truth still comes from the
   repo.
6. Before edits or reviews, run `impact`, `validate_plan`, and usually
   `surgical_context_v2`.
7. Close warmer than you found it: `memorize` every durable finding (with
   `confidence` and repo-relative `evidence` paths) so the next session starts
   ahead, then leave one field-telemetry signal — `learn(correct|wrong|partial)`
   on a retrieval, or one JSON line in `~/.m1nd/field-reports.jsonl` when m1nd
   itself misbehaves (local-only; never fix m1nd mid-mission — report). A
   memory-delivery fault is `class:"memory_misdelivery"`. Letters distribute
   LOCALLY into per-project boxes (`<repo>/.m1nd/inbox.jsonl`) + a medulla box;
   triage is `m1nd-mcp --inbox-sweep` / `GET /api/inbox_sweep` (CLI/REST, not MCP).
8. Near a PR or merge, price the handoff: `soul_check` verifies a repo's soul
   (`docs/PATHOS.md`) claim-by-claim and returns a one-line freshness receipt
   ("N fresh · M stale · declared intact, checked <date> @<sha>") — the line a
   cold context reads to know how much to trust the handoff; `soul_read` pulls
   the verified body. Details under Compounding Memory below.

This is the `m1nd-trained` behavior measured in internal bug-hunt rounds: graph
plus operating doctrine, not graph alone.

## Advertised ≠ callable — the authority floors

`tools/list` advertises the full verb surface, but generic MCP/REST dispatch
admits only actions whose M1ND-10 authority floor is `ORDINARY`. A verb above it
(`SCOPED_GRANT_A2` / `POSITIVE_SOVEREIGN` / `SERVICE_IDENTITY`) refuses with
`generic_action_authority_required`; no payload shape, capability claim, or retry
lifts it — only an exact typed G2/G3 consumer (an authority lease), and none is
installed for those actions yet. 40 of the 141 advertised verbs are affected
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

## The Write Surface — closed routing, the map, and missions

Retrieval is forgiving; writing is not. Four things to know before any write:

1. **Write only into the brain that covers you** (the reception law, above). A
   write under `caller_root_mismatch` corrupts the wrong store. Public bootstrap
   and warm rebind are unavailable until an exact typed G2/G3 consumer exists;
   stop rather than inventing a repair call.
2. **No twin brains.** The internal owner-only mint path still refuses a root that
   is the PARENT, CHILD, or WORKTREE of an existing brain
   (`overlap_parent` / `overlap_child` / `overlap_worktree`), but this is an
   implementation invariant, not a public capability.
3. **The candidate map is edited by one verb.** `skeleton_candidate` scans a repo
   into a candidate block map (with `naming:"auto"` and a live naming-runner it is
   born NAMED — the zero-touch default). `candidate_edit` is the single write verb
   over it: six typed ops (`rename` / `merge` / `split` / `move_member` /
   `resolve_seam` / `assign_unmapped`), one atomic OCC batch under
   `expected_store_version` (one bad op persists NOTHING). It refuses on a ratified
   skeleton (candidate-only); `candidate_lease` is advisory (TTL, reclaimable) and
   NEVER blocks. **Ratify is EXCLUSIVELY human** — no agent ratifies a skeleton,
   ever, and an untouched raw-heuristic block cannot be ratified. The hand
   proposes; the human signs.
4. **A mission is a letter.** `mission_post` records one mission's state as an
   `m1nd-mission-letter-v0`; `brain_ref` is the brain's DISPLAY NAME (the basename
   of its root, never an absolute path) and a letter naming another brain is
   refused (`brain_mismatch`); `block_id` must name a real block in the bound
   skeleton (or the skeleton id itself for a whole-skeleton mission), else
   `unknown_block` — a genuinely synthetic smoke/probe letter sets
   `synthetic:true` as the declared escape. A letter is STATE, never evidence: it
   never colors a block; only `receipt_import` does, and `landed` is reserved for a
   confirmed imported receipt.

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
| `needs_ingest` | "I don't know this repo yet" + its one-call repair — never a crash |
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

## Scope Binding Taxonomy

When trust or retrieval looks wrong, classify the scope relationship before
calling more search tools:

- `full_repo_binding`: active workspace/ingest root equals the repo being
  investigated, or the requested scope is inside that root. Use m1nd normally,
  then prove final truth with source/tests.
- `wrong_workspace_binding`: active workspace is an unrelated repo. Rebind with
  `M1ND_WORKSPACE_ROOT=/target/repo` only through an isolated CLI probe; public
  rebind/bootstrap is unavailable. Use federation only for true cross-repo tasks.
- `nested_workspace_binding`: active workspace/root is a subdirectory of the
  requested repo. Treat m1nd as partial truth for that subtree only. Rebind or
  ingest the repo root before making repo-wide claims.
- `file_level_binding`: ingest roots are individual docs, PRDs, or L1GHT files
  inside the repo. Use them as document context only; they do not prove codebase
  coverage or implementation truth.

Do not loop on `seek`/`activate` when the binding is nested or file-level. Use an
isolated CLI binding when available, or explicitly switch to direct files/tests
and record `m1nd_usage_mode=partial_scope_orientation`.

## Mission Control (not the default loop)

The default loop is `north` → verbs → `memorize`, NOT a mission. Reserve
`mission_*` for `SubagentStop` and for the rare turn where a mission is
genuinely open (a subagent whose whole job was one scoped mission). It is NOT
the default `Stop` path, and NOT how ordinary reviews or bug hunts run — for
those, orient with `north`, prove directly, and close with `memorize`
(`Stop → cross_verify(evidence_freshness) → memorize(claims, evidence)`
directly; `memorize` needs no `mission_id`).

When a mission IS open: `mission_start` (repo, task, mode, budget, risk) →
record actions with `mission_event` (or feed `last_event` to `mission_next` for
exactly one next move) → obey `do_not` unless you log a dissent event →
`mission_verify` before final output → `mission_handoff` for a resumable
session → `mission_close` for the proof packet. A direct event does not prove
every later claim: a claim references its direct proof explicitly
(`event:evt_1`, `file_read:path:line`, `test_run:name`, `runtime_probe:id`).
Its most important behavior is `switch_to_direct_proof` — after graph
orientation it can tell you to stop calling `seek`/`activate` and prove the
claim directly.

**WORK RUNS INSIDE (the burst wears the wire).** When you ORCHESTRATE a burst —
dispatching ≥2 executors, or landing a BIG change — open ONE mission card so the
organism SEES the work instead of it happening off-book: `mission_start` at the
start, `mission_event` at each milestone, `mission_close` with the honest outcome
at the end (over the wire, or the REST loopback `POST /api/tools/mission_start`
with `{agent_id, repo, mode, budget, risk, task}`). A mission-control card is
SINGLE-AGENT — `mission_event`/`mission_close` require the card's own `agent_id` —
so the burst posts under the orchestrator's id: executors report back and the
orchestrator posts, they do NOT open cards of their own. ONE card per burst THEME,
never one per executor (anti-spam). NEGATIVE DEFAULT, like the voice: a card is for
a REAL burst, never a trivial one-file touch. The card is a TRAIL, never a GATE —
it records what happened; the gate still proves it, and no card auto-lands (the map
colors only by a human `receipt_import`, the always-law).

## Short-Audit Route

Use the short-audit route when the repo or suspected surface is small,
localized, and likely to be proven faster by direct source reads or runtime
probes than by extended graph navigation.

1. Inside a live MCP session, orient with `north(task)` — it establishes trust
   and task context in the same round-trip. Drop to `trust_selftest` /
   `session_handshake` (scoped to the intended repo) only when the binding looks
   degraded and you need the trust sub-check alone.
2. When the host MCP session is stale, bound to the wrong repo, or not loaded
   yet, use the host-neutral CLI escape hatch instead — it launches an isolated
   runtime bound to the repo and returns one machine-readable envelope:

   ```bash
   m1nd agent first-minute \
     --repo /path/to/repo \
     --query "understand this system" \
     --json

   m1nd agent next \
     --repo /path/to/repo \
     --query "focused subsystem or bug surface" \
     --json
   ```

   `m1nd agent first-minute` is the out-of-session first contact CLI for a
   brand-new repo or a broad "understand/audit/map this" request when no live
   `north` is available. It returns anchors, `do_not` guardrails, and the
   direct-proof handoff without requiring the agent to read this skill first. In a
   healthy session, `north` is the front door; this CLI is the recovery entry.

   It returns an `m1nd-agent-action-envelope-v0`; follow the emitted command or
   recovery path instead of spending calls guessing the tool family.
3. If trust is not full, do one bounded recovery/ingest pass. Prefer the
   host-neutral agent CLI so trust, ingest when needed, one cheap orientation
   query, and the direct-proof handoff happen in one isolated MCP process:

   ```bash
   m1nd agent orient \
     --repo /path/to/repo \
     --query "focused subsystem or bug surface" \
     --mode short \
     --json
   ```
4. Run at most one or two orientation calls such as `audit`, `search`, `seek`,
   or `activate`.
5. Switch to direct files, `rg`, git diff, tests, compiler/runtime output, and
   focused probes.
6. Record whether this was `short_audit_orientation` or `recovery_overhead`.

The short-audit route is still m1nd-first. It just caps graph/recovery spend so
tiny localized tasks do not turn into tool-operation exercises.

`m1nd agent context` is anchor-first. Do not call it on broad narrative queries
such as "trace chat flow" or "understand this repo" unless you already have a
concrete `--anchor <file>` or intentionally pass `--allow-discovery`. Context
capsules support proof; they are not the first orientation move.

## RETROBUILDER Escalation

When a task asks for deep architecture quality, hidden coupling, taint/security
paths, duplication, refactor seams, or runtime heat, check whether the agent CLI
returned `capability_suggestions.family_id=retrobuilder`. If it did, use only
the matching tools, then stop and prove directly:

- hidden co-change or files that "move together" -> `ghost_edges`, then
  `timeline` or `impact`
- untrusted input, sensitive data, auth, privacy, or trust-boundary review ->
  `taint_trace`, then `trust`, `type_trace`, or `validate_plan`
- duplicate structures, near-equivalent modules, cleanup, extraction, or
  spaghetti -> `twins`, then `refactor_plan`
- runtime heat, OpenTelemetry spans, logs, latency, production failures, or hot
  paths -> `runtime_overlay`, then `trace` or `impact`
- broad architecture/risk audit -> `layers` plus the relevant RETROBUILDER
  tools: `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`,
  `runtime_overlay`

RETROBUILDER output is graph orientation. It can identify strong hypotheses,
hidden neighbors, and proof targets, but it does not prove bugs or runtime
behavior without direct source reads, tests, compiler/runtime output, logs, or
focused probes.

## Session Companion Bridge

If the host also exposes a session-memory companion such as COMPANION, use it only
for continuity: north star, prior decisions, open loops, handoff context, and a
scoped `m1nd flash` summary when available.

If the host exposes only the companion wrapper and no direct `m1nd` MCP tools,
classify the situation as `missing_m1nd_host_tool_surface`, not as unhealthy
graph truth. First try the host-neutral CLI path below; fall back to raw
`rg`/file reads only if the CLI path is unavailable or its recovery output says
to use direct local truth.

Do not use a companion digest or global memory search as code truth. Before
using companion output, verify that the session is bound to the intended repo or
project root. If the companion reports missing scope, wrong project, unavailable
flash, or global candidate search, treat that output as orientation only and
return to:

```bash
m1nd agent next --repo /path/to/repo --query "current task" --json
```

Then prove final claims with direct source reads, tests, compiler/runtime
output, logs, or focused probes. In short: session companions preserve why the
work matters; `m1nd agent` chooses the next repo move; direct proof decides what
is true.

## Compounding Memory

When you conclude something durable — a verified finding, a design decision, why code is the way it is — use `memorize` to persist it rather than leaving it in the conversation only.

Quick pattern:

1. Ingest the relevant code first so evidence paths resolve.
2. Call `memorize` with `node_label`, `claims` (each with `label`, `text`, `confidence`, optional `evidence` as repo-relative paths).
3. The result is a `.light.md` in `<runtime_root>/agent-memory/` that is ingested immediately and auto-loaded at every future session start (reported in `session_handshake.agent_memory`). Evidence paths become `grounded_in` edges to real code nodes.
4. After code changes, `cross_verify(check:["evidence_freshness"])` tells you which memorized claims now cite stale code. A merge re-ingest also returns `memory_freshness` inline.
5. `mission_close(write_light_memory:true)` combines closing a mission and persisting its verified claims in one step.

Provenance and scope (MEDULLA M5a): every `memorize` is stamped with an `Origin-Brain` — the project root it was born in, or `medulla` for the owner's own doctrine store — so recall can always name WHICH brain a claim came from. If your session's root has no project brain, a `memorize` is REFUSED rather than written into the shared medulla store; the refusal reports `brain_bootstrap_consumer_not_installed` and carries no executable repair call.

Pull, never push — tier-scoped recall (MEDULLA M5b): your default memory beat carries exactly TWO feeds — your own project brain's memory + the shared `medulla` (promoted/doctrine claims). Another repo's private claim NEVER surfaces in your beat; it can only reach you if it was promoted to the medulla. Every recall row is labeled with its `tier` (`project` | `medulla`) and `origin_brain`. To inspect across projects, pass `tier` on `seek`/`north`/`boot_memory`: `project` (your store only), `medulla` (doctrine only), `project+medulla` (the default — zero change for existing callers), or `all-brains` (the explicit fan-out over every hosted brain, each hit labeled by `origin_brain`, warm-boots routed through the eviction cap). `all-brains` is one argument away and never ambient — reach for it only when you genuinely need another project's knowledge.

Promotion — the audited crossing (MEDULLA M6): a `memorize` is ALWAYS project-private; a finding does not become shared doctrine by being written. The public `promote` route is currently fail-closed at `POSITIVE_SOVEREIGN` until an exact typed G2 authority consumer is installed. `State: verified`, founder/source labels, caller identity, and arbitrary lease strings are evidence or metadata — never authority. The intended crossing still copies a genuinely transversal project claim into the medulla with its readable provenance while retaining the project witness, but only an owner-resolved internal path may perform it until the public authority contract is mechanically proved. Treat “candidate for promotion” as a proposal, not permission.

Delegation — hand a grounded packet down, debrief the return (ORGANISM R6 + R7/M7): spawning a subagent? `delegate {agent_id, task}` composes the RETRIEVAL half of its spec in ONE read-only call — the mother's binding (the NAMED brain the child must land on), a LABELED memory slice (M7): each row carries `tier` (`project` | `medulla`) + `origin_brain` beside age + author, so the child inherits exactly what you chose AND can tell doctrine from project fact (the slice is your DEFAULT beat — project task-relevant claims + the medulla doctrine the domain touches, never `all-brains`, never another project's private claim). Plus ranked anchors, a staleness header, known dependents, and an explicit "what m1nd could NOT determine → your duties" section — rendered as `prompt_markdown` you APPEND to your brief (memory lines read `- [tier] claim — origin · author, age`; appendix: your text wins on what-to-do, the file on what-is, the packet outranks assumption only). The packet's `mission.binding` is the SAME datum reception verifies (`M1nd-Caller-Root` ↔ `covers_root`), so the child VERIFIES it landed (silent on match) rather than choosing — the child law. `delegate` abstains honestly (`needs_ingest` / `unscopable` / `seeds_unresolvable`) with evidence + a `next_move`, never a bare no; no predict/trust/tremor/xray enrichment yet, each omission stated in `non_claims`. When the subagent returns, `debrief {agent_id, delegation_id, outcome, touched_paths|diff, findings}` grades its diff against the packet and TEACHES the graph (the only mutation, via `memorize`/`learn`): it classifies touched paths, memorizes findings under the subagent and map-miss lessons under you, and appends one `outcomes.jsonl` row (stamped `outcome_unverified` unless you attach evidence). Conformance grades PATHS, never code quality — never merge-safe. Every debrief deposits memory the next `delegate` surfaces, so skipping it wastes knowledge.

The soul — trust the handoff by a receipt, not by faith (ORGANISM R16): a repo's `docs/PATHOS.md` is its SOUL — the curated handoff (north, state, doctrine, access, known problems, next moves). The pathos skill AUTHORS it; m1nd is the ENGINE that verifies it where a brain exists. `soul_check` parses the soul into anchored CLAIMS, classifies each (path/line-hint/symbol/git/consistency/receipt/runtime/declared), verifies per class, and returns a one-line FRESHNESS RECEIPT — "N fresh · M stale · K receipt-priced, checked <date> @<sha>" — the line a cold context reads to know how much to trust the handoff. THE TWO TISSUES: verifiable tissue (Current State / Access Map / Known Problems) is machine-checkable; DECLARED tissue (North Star / Doctrine / taste / why-we-work-this-way) is UNPROVABLE-but-curated and NEVER fake-verified — the system knowing what it cannot verify IS the honesty. `soul_read` pulls the body (whole or a section) — the explicit pull, never ambient. THE CURATOR is a near-PR/doc-gate WORKFLOW (agent judgment; deterministic substrate): sweep with `soul_check` → verify against code/git/runtime → update durable claims via `memorize {soul_source: "<path>#<section>"}` (the ONE write door) → prune stale NEVER silently (each removal named + where-it-went) → re-check → carry the receipt in the PR. Who verifies the curator (§C8.4): its report passes `soul_check {verify_curator_report: <report>}` run by a DIFFERENT agent — grader ≠ author.

Caveat: `ingest mode:replace` wipes agent memory nodes. Use `mode:merge` when re-ingesting code to keep agent memory intact.

## Skip Conditions

Skip the `m1nd` first pass only when:

- the user already gave the exact file and exact lines
- the question is compiler, test, or runtime truth rather than structure
- the task is a trivial local file action with no structural uncertainty

## Fallback

If `m1nd` does not answer enough, then fall back to shell search, direct file reads, compiler output, tests, logs, and debugger data.

If a local helper/probe fails before MCP initialization with
`runtime_root ... is already owned by instance`, classify it as a sidecar
runtime lock collision, not stale graph truth. Prefer `m1nd agent trust` or
`m1nd agent orient`, which isolate runtime state by default. If the npm CLI is
not installed, use the current `probe_m1nd.py`, pass a unique explicit
`--runtime-dir`, or collapse dependent checks into one `probe_m1nd.py run`
call. Do not tell the user that m1nd retrieval is broken until a fresh isolated
probe or the repo-local smoke harness also fails.
In benchmark lanes or repos with narrow write scopes, add
`m1nd agent ... --repo /path/to/repo --json` so graph/runtime metadata stays in
the isolated runtime directory instead of the target worktree. The CLI sets
`M1ND_WORKSPACE_ROOT` for the requested repo.

If a tool call fails with `Transport closed`, classify it as a host MCP binding
failure before m1nd can run. Do not call `doctor`, `recovery_playbook`, or
`ingest` through that dead binding. Verify the binary with a local smoke, kill
stale `m1nd-mcp --stdio` processes if you own the host, and restart/rebind the
agent host or open a fresh thread so the MCP client launches a new transport.
After rebind, run `trust_selftest` or `session_handshake` before trusting
retrieval.

If the host appears to be launching an old native runtime, use the external CLI
self-update path:

```bash
m1nd update check --channel beta
m1nd update status --channel beta
m1nd update plan --channel beta
m1nd update apply --channel beta --yes
m1nd hosts status --host all --project /path/to/project --json
m1nd hosts plan --host all --project /path/to/project --json
m1nd hosts apply --host all --project /path/to/project --yes --json
```

Use `--no-kill` in live multi-agent sessions when you only want to update the
managed binary and rebind one selected host. `m1nd update` does not ingest,
repair graph contents, choose a workspace, or refresh an already-open client's
cached MCP tool list. `m1nd restart --source /path/to/m1nd --yes` remains the
lower-level source-checkout repair path for development builds.

When the question is host-specific, use `m1nd hosts status` before mutating
anything. It is read-only and reports agent-pack files, likely MCP config
wiring, runtime/PATH alignment, workspace hints, and `host_rebind_proven=false`
per supported host.
If host config points to an absolute current managed runtime, a stale
`m1nd-mcp` on `PATH` is only a shadow warning, not proof that the host is
stale. If the host launches `PATH` or config is unknown, treat a stale `PATH`
runtime as actionable. Then use `m1nd hosts plan` and rebind or open a fresh
host session; do not claim the client's cached tool list refreshed by itself.
If it reports `attention`, call `m1nd hosts plan` for the exact per-host
install, MCP-config, `M1ND_WORKSPACE_ROOT`, rebind, and verification recipe.
Use `m1nd hosts apply` only for the local mutation step after `status` or
`plan`. Without `--yes` it is still a dry-run preview. With `--yes`, it can
install or refresh agent-pack files and write canonical MCP config snippets for
known hosts, but it does not prove rebind, refresh cached tool lists, repair
graph state, or automate generic-host config paths.
`plan`/`apply` also emit each host's SessionStart-family hook (routed through the
`m1nd-north-shim` command) and a per-host doctrine file.

For local m1nd repo work, prefer the cheap trust selftest path before a full smoke:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --json
```

When the live MCP surface exposes `trust_selftest`, call that tool first:

```json
{"agent_id":"codex-m1nd"}
```

Treat its `verdict` as the session routing decision before relying on
retrieval. If the verdict is not `full_trust`, follow the embedded
`recovery_playbook` or call `recovery_playbook` with the same evidence before
guessing the next move. The selftest is diagnostic-only: no ingest, repair,
host refresh, graph mutation, or retrieval probe happens automatically.

When `seek`, `search`, `activate`, or `panoramic` includes
`agent_runtime_contract`, read that envelope before interpreting results. It is
the agent-facing runtime identity contract: `trust_mode` tells whether this is
`full_trust`, `needs_ingest`, `wrong_workspace_binding`, or
`retrieval_needs_recovery`; `workspace_binding` shows whether the requested
scope belongs to the active workspace; `graph_identity` shows the exact graph
generation and counts; and `recovery.arguments` is the payload to pass directly
to `recovery_playbook`. Empty result arrays are not final truth until the
contract says the runtime/workspace/graph identity is coherent.

If the verdict is `needs_ingest`, or `graph_state.node_count` is `0` while
`ingest` is available, treat it as a recoverable cold graph, not as a reason to
abandon m1nd. Call `ingest` on the same MCP binding with the absolute path of
the intended repo/workspace, never a managed runtime/session path such as
`~/.codex/m1nd-runtimes/...`, `~/.claude/m1nd-runtimes/...`, an Antigravity
agent runtime, or a generic `mcp-runtimes`/`agent-runtimes` folder. Host
integrations should prefer `M1ND_WORKSPACE_ROOT`; m1nd also recognizes common
workspace hints from Claude Code, Antigravity, Gemini, Cursor, Windsurf, VS
Code, and shell/package-manager env vars. Then rerun `session_handshake` and
one cheap retrieval. Fall back to direct files only when ingest is unavailable,
ingest fails, or a post-ingest retrieval still reports `blocked` and
`recovery_playbook`/`doctor` confirms stale binding or degraded host surface.

If `trust_selftest`, `session_handshake`, `recovery_playbook`, `doctor`,
`validate_plan`, or a retrieval response includes
`context_guard.wrong_workspace_binding=true`, do not classify the repo graph as
stale. The active binding is pointed at one workspace while the call asked about
another. Follow the embedded `recovery_playbook` payload, rebind the host with
`M1ND_WORKSPACE_ROOT` set to `requested_workspace_hint`, or explicitly ingest
that workspace on the same binding if the switch is intentional. Use
`federate_auto`/`federate` only when the task is genuinely cross-repo.
If the open host cannot be rebound during the current turn, do not abandon m1nd
as a layer. Use an isolated local probe bound to the requested workspace, then
switch to direct source/test proof:

```bash
m1nd agent orient \
  --repo /path/to/requested/repo \
  --query "focused subsystem or bug surface" \
  --mode short \
  --json
```

Record this as `m1nd_usage_mode=isolated_probe_after_wrong_workspace_binding`,
not as "m1nd unavailable". Fall back to raw `rg`/manual reads only if the probe
cannot ingest/retrieve or the recovery playbook says to use local truth.
If `wrong_workspace_binding` is reported but the active root or ingest roots
are actually inside the requested repo, treat this as `nested_workspace_binding`
or `file_level_binding`, not as a dead m1nd core. The graph may be useful for
that sub-scope, but it cannot support repo-wide claims until the binding is
upgraded.

If `trust_selftest` is not exposed but `session_handshake` is, call the cheaper
sub-check:

```json
{"agent_id":"codex-m1nd"}
```

When the task names a target repo or absolute path, include it as `scope`:

```json
{"agent_id":"codex-m1nd","scope":"/path/to/intended/repo"}
```

Treat its `trust_mode` as the session routing decision before relying on
retrieval. If the mode is not `full_trust`, call `recovery_playbook` before
guessing the next move. The playbook returns ordered recovery steps and a
binding fingerprint without ingesting, repairing, or probing automatically.

Use `--handshake-probe` only when retrieval trust itself matters. The plain
selftest/handshake path should stay cheap: no ingest, no repair, and no
retrieval probe by default. The repo-local smoke harness calls `trust_selftest`
and `session_handshake` when available and falls back to its built-in handshake
for older binaries.

If the host exposes `health` but not `trust_selftest`, `session_handshake`, or
`recovery_playbook`, read `health.tool_surface_contract` and
`health.host_binding_alignment`.
That is enough to classify the binding as partial/degraded and switch to local
smokes or direct file truth until the host refreshes its tool surface.

If `m1nd` is visible but the host tool surface is missing recovery tools such as
`ingest`, treat it as `degraded_host_tool_surface`, not as a normal graph
failure. Use whatever m1nd can still provide for orientation, but verify final
truth against local files until the MCP binding is refreshed. If
`recovery_playbook` is available, call it with the tool surface:

```json
{"agent_id":"codex-m1nd","observed_tool":"tools/list","observed_proof_state":"blocked","observed_tool_count":3,"available_tools":["seek","audit","doctor"],"missing_tools":["ingest"]}
```

If an `ingest` call appears to succeed but a follow-up retrieval call such as
`seek`, `search`, or `activate` returns `blocked`, zero candidates, or an empty
graph unexpectedly, do not assume the codebase is unindexed. Treat it as a
possible host-binding/session-continuity problem. If `recovery_playbook` is
available, use `recovery.arguments` from the retrieval response when present.
If the response does not include a recovery payload, call `recovery_playbook`
with the suspicious output first. Let the playbook decide when to call
`doctor`:

```json
{"agent_id":"codex-m1nd","observed_tool":"seek","observed_proof_state":"blocked","observed_candidates":0}
```

If the repo has a local m1nd checkout, verify the real runtime with its stdio
and HTTP smoke harness before deciding:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --json
python3 scripts/mcp_agent_smoke.py --repo . --transport http --json
```

## Deep Manual

If the task needs detailed routing, `L1GHT` semantics, document-lane choice, multi-agent coordination, or refresh procedures, consult:

- the companion `m1nd-operator` skill installed with this pack, usually at
  `m1nd-operator/SKILL.md` in the same skills root.
