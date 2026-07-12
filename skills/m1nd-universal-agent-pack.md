# m1nd Universal Agent Pack

Use this pack in any coding-agent host that can read project rules, custom
instructions, memory, or prompt files.

## Prime Directive

Use `m1nd` as the first investigative layer before raw grep, filesystem
globbing, or manual file reads when the task depends on repository structure,
docs, impact, continuity, or risky change preparation.

Skip the first pass only when the user gave exact file/line truth, when compiler
or runtime output is the only source of truth, or when the task is a trivial
local file action.

## Startup: north First

In a live MCP session the front door is one call: `north(task)`. It returns
binding trust, task context (focus nodes + PageRank anchors), prior
cross-session memory, a sufficiency signal, one `next_move`, and `honest_gaps`
in a single packet, before any query. If it returns `needs_ingest` (empty/unbound
graph), `ingest` the repo, then `north` again — that is a real answer, not a
failure. `north` composes `trust_selftest` + `orient` + `boot_memory` + `focus`.
Heed `reception`: `reception.match == "caller_root_mismatch"` means the bound
graph does NOT cover your current repo — do not trust retrieval for it; read
`reception.options[]`. ONE call sets you up: `ingest` with
`project_root=<your repo root>` creates a per-project brain inside the served
owner, ingests your repo, binds your session, and returns its north packet —
thereafter every call from your root routes to YOUR brain automatically.
Absent/null `reception` = your root matches the brain serving you. Reception
governs WRITES, not just reads: a read under mismatch is a warning, but a WRITE
under mismatch is PROHIBITED — see Write-Mode Laws below; bootstrap first, write
after.

Drop to the trust-only sub-checks when the binding looks degraded and you need
just the trust verdict:

0. If a tool call fails with `Transport closed`, stop treating it as graph
   staleness. The MCP transport died before m1nd could run. Verify the binary
   with a local smoke, restart/rebind the host MCP client or open a fresh
   thread, then continue.
1. Call `trust_selftest` if the host exposes it.
2. If unavailable, call `session_handshake`.
3. If only `health` is exposed, inspect `tool_surface_contract` and
   `host_binding_alignment`.
4. If trust is not full, call or follow `recovery_playbook`.
5. Only then rely on retrieval surfaces such as `search`, `seek`, or `activate`.

When the task names a concrete repo, worktree, or absolute path, pass it as
`scope` to `session_handshake`, `trust_selftest`, `recovery_playbook`, `doctor`,
or `validate_plan`. If the response reports
`context_guard.wrong_workspace_binding=true` or trust mode
`wrong_workspace_binding`, the host is bound to a different workspace than the
one you asked about. Rebind with `M1ND_WORKSPACE_ROOT`, intentionally ingest the
requested workspace on the same binding, or use explicit federation for real
cross-repo work. If the open host cannot be rebound now, use a fresh isolated
probe/runtime bound to the requested workspace before falling back to raw file
search. Record this as
`m1nd_usage_mode=isolated_probe_after_wrong_workspace_binding`, not as "m1nd
unavailable". Do not call this graph staleness.

`trust_selftest` and `recovery_playbook` are diagnostic. They do not ingest,
repair, refresh the host, or mutate the graph.

## The m1nd Voice — rendering `human_view`

The north packet carries `human_view` (`m1nd-human-view-v0`): the m1nd voice for
the HUMAN — a server-composed, already-mounted card (the `m1nd` wordmark + `│`
gutter fixed at column 6; ≤4 lines, ≤80 chars/line; states `clean | bell |
coherence | mismatch | needs_ingest`; a mechanical `state_sig`). Render it by
joining `lines[]` with newlines inside a fenced code block — never re-compose
it, never decorate it. Every line is a measured fact or a verbatim server
string (brand law G1: no uncalibrated adjectives, no benefit claims).

**Cadence — the default is NEGATIVE (verbatim law):** Do NOT render the card
unless m1nd contributed structurally to the mission AND the content is useful
to the human NOW; never in consecutive messages; never the same state_sig twice
in a session; on state change or first orient. When in doubt, stay silent —
silence is the honest card.

**Duties:** translate the card's CONTENT into the conversation's language
keeping the geometry (gutter at column 6, ≤80 cols) and ids/state tokens
(`merge_wait`, `needs_ingest`, `full_trust`) intact. The DEEP rung (R2) is the
agent's: when the human asks ("what's the bell?", "show me m1nd") render a
deeper card FROM the packet's structured fields (`landing_bell`, the mission
tray, blocks) in the SAME grammar — never a fact the packet does not carry.
When a surface cannot hold unicode, apply the 1:1 ASCII map `│`→`|`, `·`→`.`,
`—`→`-` (widths identical, geometry never moves).

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

Before using graph results as task truth, classify the binding:

- `full_repo_binding`: active workspace/ingest root is the repo under work, or
  the requested scope is inside that root. Use m1nd normally, then prove with
  source/tests.
- `wrong_workspace_binding`: active workspace is another repo. Rebind with
  `M1ND_WORKSPACE_ROOT`, intentionally ingest the target repo, use isolated
  probe, or federate only for true cross-repo work.
- `nested_workspace_binding`: active workspace/root is a subdirectory of the
  requested repo. Treat results as partial subtree truth only.
- `file_level_binding`: ingest roots are docs, PRDs, L1GHT files, or generated
  handoffs. Treat results as document context only, not implementation coverage.

Do not loop on retrieval against nested/file-level bindings for repo-wide tasks.
Upgrade the binding to repo root or record
`m1nd_usage_mode=partial_scope_orientation` and prove with direct files/tests.

## Default Trained Investigation Loop

For unfamiliar repo work, audits, bug hunts, reviews, and risky changes, use
this loop by default:

1. Orient with `north(task)`. One round-trip returns binding trust, task
   context, prior cross-session memory, a sufficiency signal, one `next_move`,
   and `honest_gaps`. `needs_ingest` → `ingest` → `north` again.
2. If trust is degraded or retrieval comes back `blocked`/empty unexpectedly,
   drop to `recovery_playbook` before interpreting absence.
   `wrong_workspace_binding` means rebind, intentional ingest, or real
   federation; it is not stale graph proof.
3. Follow the `next_move`, or route focused questions through `search`, `seek`,
   or `activate`. Obey the calibrated verdict — do not override it:
   `act`/`reverify`/`abstain` (abstain = STOP, not a weak yes; the prediction gate
   is armed by `calibrate_predict` and the seek trust envelope by
   `calibrate_envelope`, else each verdict caps at `reverify`); `why`
   carries a `closure` verdict (`blocked` = the path rests on an unresolved
   edge); `seek` carries a `trust_envelope` + a sufficiency signal (`sufficient`
   = stop gathering); `trust_band: insufficient_evidence` means NO evidence, not
   medium risk.
4. Read the runtime envelope on retrieval responses. Empty result arrays are
   not final truth until workspace binding, graph identity, and recovery state
   are coherent.
5. Verify with direct source, tests, compiler/runtime output, and focused
   probes. `m1nd` narrows and connects; execution truth still comes from the
   repo.
6. Before edits or reviews, run `impact`, `validate_plan`, and usually
   `surgical_context_v2`.
7. Close warmer than you found it: `memorize` every durable finding (with
   `confidence` and repo-relative `evidence` paths), then leave one
   field-telemetry signal — `learn(correct|wrong|partial)` on a retrieval, or one
   JSON line in `~/.m1nd/field-reports.jsonl` when m1nd itself misbehaves
   (local-only; never fix m1nd mid-mission — report). A memory-delivery fault is
   `class:"memory_misdelivery"` + a `kind`. Letters distribute LOCALLY into
   per-project boxes (`<repo>/.m1nd/inbox.jsonl`, git-travelling) + a medulla box
   for projectless letters; triage is `m1nd-mcp --inbox-sweep` / `GET /api/inbox_sweep`
   and a project's box reads via `GET /api/mailbox?brain=<root>` (CLI/REST, not MCP).
   Record the investigation path: m1nd calls, recovery path, files inspected, commands run.
   Every `memorize` is stamped with an `Origin-Brain` (its project root, or
   `medulla` for the owner's doctrine store), so recall names which brain a claim
   came from. A `memorize` from a root with NO project brain is REFUSED (never
   silently written into the shared medulla); the refusal hands you the one-call
   bootstrap `ingest project_root=<your repo>` — run it, then memory lands
   project-private.
8. Memory is pull, never push (the medulla law): your default recall beat carries
   exactly your own project brain + the shared `medulla` (promoted/doctrine) — no
   other repo's private claim ever reaches you unless it was promoted. Every recall
   row is labeled `tier` (`project` | `medulla`) + `origin_brain`. To inspect
   across projects, pass `tier` on `seek`/`north`/`boot_memory`: `project`,
   `medulla`, `project+medulla` (default), or `all-brains` (the explicit fan-out
   over every hosted brain, each hit origin-labeled). `all-brains` is one argument
   away and never ambient — use it only when you truly need another project's
   knowledge.
9. Promotion is the audited crossing (do it deliberately, rarely): a `memorize` is
   always project-private — a finding does NOT become shared doctrine by being
   written. When a VERIFIED claim is genuinely transversal, `promote {brain, claim,
   reason}` copies it UP into the medulla with the full readable chain (Origin-Brain,
   Origin-Claim, Promoted-By, Promotion-Reason); the project original stays in place
   stamped Promoted-To (elevate, never move). The verb gates: only `State: verified`
   (or a founder claim) may promote; a secret/conflict-marker is refused at the
   hygiene floor; evidence is origin-qualified so freshness delegates to the home
   brain, else the claim is marked `evidence_unverifiable` (never reads fresher than
   it can prove). It is an ORCHESTRATOR act — a maker proposes, the orchestrator
   executes; any id may call it but `Promoted-By` audits every promotion. Demote via
   `learn wrong` on the medulla copy — never touches the project witness.
10. Delegation is the grounded spawn (ORGANISM R6 + R7/M7): spawning a subagent? `delegate
    {agent_id, task}` composes the RETRIEVAL half of its spec in ONE read-only call —
    the mother's binding (the NAMED brain the child must land on), a LABELED memory
    slice (M7: each row carries `tier` `project`|`medulla` + `origin_brain` beside age +
    author — explicit cargo, the child inherits exactly what you chose AND tells doctrine
    from project fact; the DEFAULT beat = project claims + medulla doctrine, never
    `all-brains`), ranked anchors, a staleness header, dependents, and a "what m1nd could
    NOT determine → your duties" section — rendered as `prompt_markdown` you APPEND to
    your brief (memory lines: `- [tier] claim — origin · author, age`; appendix: your
    text wins on what-to-do, the file on what-is, the packet outranks assumption only).
    `mission.binding` is the SAME datum reception verifies
    (`M1nd-Caller-Root` ↔ `covers_root`), so the child VERIFIES it landed (silent on
    match), never chooses — the child law. Abstains honestly (`needs_ingest` /
    `unscopable` / `seeds_unresolvable`) with evidence + a `next_move`. When the child
    returns, `debrief {agent_id, delegation_id, outcome, touched_paths|diff, findings}`
    grades the diff against the packet and TEACHES the graph (the only mutation, via
    `memorize`/`learn`): findings memorize under the subagent, map-miss lessons under
    you, one `outcomes.jsonl` row per debrief (`outcome_unverified` without evidence).
    Conformance grades PATHS, never code quality — never merge-safe. Every debrief
    deposits memory the next `delegate` surfaces.

This is the trained-agent behavior to preserve across hosts: m1nd is graph plus
operating doctrine, not graph alone.

## Write-Mode Laws — reception governs writes

The served owner hosts many per-project brains and routes each request to the
brain that covers the caller's repo. Retrieval under a wrong binding is
recoverable; a WRITE under a wrong binding corrupts shared state. A real incident
set these laws — a foreign repo's skeleton was written into a bound brain because
the writer never checked which brain answered it.

1. **No write under a reception mismatch.** When a response carries
   `reception.match == "caller_root_mismatch"`, the brain serving you does NOT
   cover your repo. Reads are a warning; **writes are prohibited** — `memorize`,
   `skeleton_candidate`, `candidate_edit`, `system_blocks_seed_import` / `_ratify`
   / `_reconcile`, and `mission_post` would each land in the WRONG brain. The one
   correct gesture BEFORE any write: `ingest project_root=<your repo root>` — one
   call creates/resolves your brain, ingests it, binds the session, and returns
   north. Then write. (The `memorize` refusal — a write from a root with no
   project brain is refused, never silently dropped into the shared medulla, and
   hands you this bootstrap — is the one already-mechanical instance.)
2. **No twin brains.** Minting a brain for a root that is the PARENT, CHILD, or
   WORKTREE of an existing brain is refused with a teaching error
   (`overlap_parent` / `overlap_child` / `overlap_worktree`) naming the conflict
   and two ways forward: bind to the existing brain (`ingest
   project_root=<existing>`), or pass `allow_overlap:true` only when you know
   exactly why. It holds on both seams (MCP wire and REST `POST
   /api/tools/ingest`). A burst worktree does NOT earn its own brain — bind to the
   main repo's.

**The write surface, briefly.** The block map (skeleton) is edited by one atomic
verb, `candidate_edit` (six typed ops under `expected_store_version`; refuses on a
ratified skeleton — candidate-only); `candidate_lease` is advisory and never
blocks. **Ratify is EXCLUSIVELY human — no agent ratifies a skeleton, ever.** A
mission is a letter (`mission_post`): `brain_ref` is the brain's display name (the
basename of its root, never an absolute path; a wrong one is `brain_mismatch`),
`block_id` must name a real block (else `unknown_block`; a smoke/probe letter sets
`synthetic:true`), and a letter is STATE, never evidence — it never colors a
block, only `receipt_import` does.

## Mission Control (not the default loop)

Mission Control is NOT the default loop. The default is `north` → verbs →
`memorize` (the composable close is `Stop → cross_verify(evidence_freshness) →
memorize(claims, evidence)` directly; `memorize` needs no `mission_id`). Reserve
`mission_*` for `SubagentStop` and the rare turn where a mission is genuinely
open — it is never the default `Stop` path, and not how ordinary reviews or bug
hunts run.

When a mission IS open, the loop is small:

1. Call `mission_start` with `agent_id`, `repo`, `task`, `mode`, `budget`, and
   `risk`.
2. Record meaningful actions with `mission_event` when available; otherwise
   call `mission_next` with the last meaningful event.
3. Treat `do_not` as a guardrail. If you disagree, record a dissent event.
4. Before final output, call `mission_verify` for each material claim.
5. Use `mission_handoff` when another agent or later session may resume.
6. Close with `mission_close`, including gaps, event digest, and non-claims.

Evidence rule: direct proof must be referenced by the claim, for example
`event:evt_1`, `file_read:path:line`, `test_run:name`, or `runtime_probe:id`.
Do not let an unrelated direct event validate a graph-only claim.

Mission Control is not a replacement for source reads, tests,
compiler/runtime output, recovery tools, or host rebind. Its key value is
forcing the switch from graph orientation to direct proof when the graph has
done enough.

Bug-hunt calibration: if `mission_next` returns `direct_sweep`, do one
negative-space sweep over public contracts/docs, boundary values, error paths,
async/concurrency behavior, and helper/exported APIs. Record it as
`coverage_sweep`, `boundary_sweep`, or `edge_case_sweep` before closing.

## Short-Audit Route

For tiny repos, localized bug hunts, or narrow reviews, use m1nd as a bounded
orientation pass instead of a long graph investigation:

1. Orient with one `north(task)` call (trust + context in the same round-trip);
   `needs_ingest` → one bounded ingest → re-`north`. Drop to `trust_selftest` /
   scoped `session_handshake` only if the binding looks degraded.
2. Run at most one or two cheap follow-up calls: `search`, `seek`, or
   `activate` (or `audit` for the structural map).
3. When suspect files or behaviors are visible, switch to direct source reads,
   git diff, tests, compiler/runtime output, and focused probes.
4. Record `short_audit_orientation` if this helped, or `recovery_overhead` if
   m1nd state repair consumed meaningful time.

Prefer the host-neutral agent CLI for this route:

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

Use `m1nd agent first-minute` when the live MCP session is stale, bound to the
wrong repo, or has not loaded this pack yet — it is the host-neutral CLI escape
hatch for first contact, not the in-session front door (`north` is). It scopes,
trusts, ingests when needed, runs one bounded orientation pass, returns anchors,
and hands control back to direct proof.

`m1nd hosts plan`/`m1nd hosts apply` now emit SessionStart-family hook recipes
(`SessionStart`/`agentSpawn`/`TaskStart`) plus a per-host doctrine file for every
TIER-A and TIER-B host, all routed through the `m1nd-north-shim` command so the
orientation packet is injected as `additionalContext` at session start. `apply`
merges owned hook JSON without clobbering existing hooks and PRINTS the blocks
for host-managed configs (Claude/Cline/Kiro) instead of writing them.

`agent next` emits an `m1nd-agent-action-envelope-v0` with the first safe move.
Use it when choosing between scope, trust, orient, context, recover, or direct
proof. `agent orient` returns `schema=m1nd-agent-cli-v0`, records
`short_audit_orientation` or `recovery_overhead`, and always tells the agent to
switch to direct proof. Use `probe_m1nd.py short-audit` only as a compatibility
fallback when the npm CLI is unavailable, and raw `probe_m1nd.py run` only when
a custom multi-tool sequence is needed.

`agent context` is anchor-first. Use it with a concrete path, identifier, or
`--anchor <file>` after orientation. Do not use it as the first broad narrative
question unless `--allow-discovery` is intentionally chosen and the result is
treated as orientation only.

## Session Companions

If your host has a session-memory companion such as DEXT3R, use it for
continuity, not code truth. Good uses are:

- recovering the session north star
- recalling prior decisions, open loops, and handoff context
- attaching a scoped `m1nd flash` to the current session
- spotting that the current conversation has drifted across projects

Bad uses are:

- treating global memory search as repo truth
- replacing `m1nd agent next` or the m1nd MCP trust loop
- treating a host that exposes only the companion wrapper as proof that the
  m1nd graph is unhealthy
- claiming code behavior without direct source/test/runtime proof
- trusting a flash when the companion reports missing scope, wrong project, or
  unavailable m1nd context

If direct `m1nd` MCP tools are missing but a companion wrapper exists, record
`m1nd_usage_mode=missing_m1nd_host_tool_surface` and try the host-neutral CLI
before falling back to raw local search.

The universal route is:

```text
session companion -> continuity and prior decisions
m1nd agent next    -> first safe repo move
m1nd MCP tools     -> structural graph/docs/impact/mission context
direct proof       -> final truth
```

When companion scope is missing or global-only, record
`m1nd_usage_mode=companion_orientation_only`, then run:

```bash
m1nd agent next --repo /path/to/project --query "current task" --json
```

## Full-Spec Escalation

For broad audits, hard bug hunts, multi-repo systems, docs/L1GHT work,
long-running investigations, security/risk review, or when the user asks for
the full m1nd system, load the full operating layer:

`skills/m1nd-operator/references/full-spec-agent-os.md`

Treat it as a route table, not a checklist. The compact pack gets you moving;
the full-spec layer tells you which m1nd/L1GHT tool combination to use for each
situation.

## Tool Routing

- In-session front door -> `north(task)` (trust + task context + memory + sufficiency + next_move)
- Exact text -> `search`
- Path pattern -> `glob`
- Known purpose, unknown location -> `seek`
- Topic, subsystem, or neighborhood -> `activate`
- Unfamiliar repo -> `north` (or `audit` for the structural map alone)
- Stacktrace or runtime error text -> `trace`
- Risky change -> `impact`, `predict`, `validate_plan`, then usually
  `surgical_context_v2`
- Deep architecture/risk -> RETROBUILDER. Use `ghost_edges` for hidden
  historical co-change, `taint_trace` for trust-boundary or sensitive flow,
  `twins` for structural duplication, `refactor_plan` for extraction
  communities, and `runtime_overlay` for span/log heat on graph nodes. These
  tools produce anchors and hypotheses; direct source/test/runtime proof is
  still final truth.
- Docs/specs -> `ingest` with `adapter="universal"` or `adapter="light"`, then
  document binding/drift tools
- Mission loop -> `mission_start`, `mission_event`, `mission_next`,
  `mission_verify`, `mission_handoff`, `mission_close`

## Recovery Rules

`Transport closed` is a host transport failure, not a m1nd proof-state. Do not
call `doctor`, `recovery_playbook`, or `ingest` through that dead binding. A
fresh MCP transport must be launched first.

If a local helper/probe fails before initialization with
`runtime_root ... is already owned by instance`, treat it as a runtime sidecar
lock collision, not stale graph truth. Prefer `m1nd agent scope/trust/orient`,
which isolates runtime state by default. If the npm CLI is unavailable, use an
isolated `--runtime-dir`, use the current `probe_m1nd.py` helper, or group
dependent checks into one `probe_m1nd.py run` process before falling back to
files. In benchmark lanes or strict write scopes, prefer
`m1nd agent ... --repo /path/to/repo --json` so runtime metadata stays in the
isolated runtime dir while the requested repo becomes `M1ND_WORKSPACE_ROOT`.
The helper prefers `~/.m1nd/bin/m1nd-mcp` before a stale `m1nd-mcp` on `PATH`.

If the host appears to be launching an old or stale native runtime, and a local
`m1nd` CLI is available outside the MCP transport, run:

```bash
m1nd update check --channel beta
m1nd update plan --channel beta
m1nd update apply --channel beta --yes
```

Then restart or rebind the host MCP client. `m1nd update` is external repair:
it does not ingest, pick the workspace, repair graph contents, or refresh a
client's cached tool list inside an already-open conversation.
If host config points to an absolute current managed runtime, a stale
`m1nd-mcp` on `PATH` is a shadow warning only. If the host launches `PATH` or
config is unknown, stale `PATH` is actionable. Verify with `m1nd hosts status`
and `m1nd hosts plan`, then rebind or open a fresh host session before claiming
the updated runtime or tool surface is active.
Use `m1nd hosts apply` only as the local mutation step after `status` or
`plan`. Without `--yes` it stays a dry-run preview. With `--yes`, it can
install or refresh agent-pack files and write canonical MCP config snippets for
known hosts, but it does not prove rebind, refresh cached tool lists, repair
graph state, or automate generic-host config paths.

In live multi-agent sessions, use `--no-kill` to update the managed binary
without stopping every active `m1nd-mcp` host. `m1nd restart --source
/path/to/m1nd --yes` remains the lower-level source-checkout repair path for
development builds.

If `seek`, `search`, or `activate` returns `blocked`, zero candidates, or an
unexpectedly empty graph after ingest, treat it as possible stale binding or
session split-brain before blaming the repo.

Pass the returned `recovery.arguments` to `recovery_playbook` when present. If
the response has no payload, call `recovery_playbook` with:

```json
{"agent_id":"agent","observed_tool":"seek","observed_proof_state":"blocked","observed_candidates":0}
```

If m1nd is visible but required tools such as `ingest`, `trust_selftest`, or
`recovery_playbook` are missing, classify the session as
`degraded_host_tool_surface`. Use m1nd only for orientation and verify final
truth with local files until the host binding refreshes.

## Change Discipline

Before risky edits:

1. Use `seek` or `activate` to find the connected surface.
2. Use `impact` for blast radius.
3. Use `validate_plan` for missing work.
4. Use `surgical_context_v2` for compact edit context.
5. Run compiler/tests/runtime checks for execution truth.

`m1nd` complements the compiler, test runner, LSP, debugger, security scanner,
and local file truth. It does not replace them.

## Continuity

Keep `agent_id` stable within one investigation. Use trails, perspectives, and
coverage sessions when work spans agents, branches, or sessions.

The soul (ORGANISM R16): a repo's `docs/PATHOS.md` is its SOUL — the curated
handoff. The pathos skill AUTHORS it; m1nd is the ENGINE that verifies it.
`soul_check` parses the soul into anchored claims, verifies each per class
(path/line-hint/symbol/git/consistency/receipt/runtime/declared), and returns a
one-line FRESHNESS RECEIPT — "N fresh · M stale · K receipt-priced, checked
<date> @<sha>" — the line a cold context reads to know how much to trust the
handoff. THE TWO TISSUES: verifiable tissue is machine-checkable; DECLARED tissue
(doctrine, taste, why-we-work-this-way) is UNPROVABLE-but-curated and NEVER
fake-verified. `soul_read` pulls the body (whole or a section), never ambient. The
CURATOR is a near-PR/doc-gate workflow (agent judgment, deterministic substrate):
sweep with `soul_check` → verify against code/git/runtime → update durable claims
via `memorize {soul_source}` (the ONE write door) → prune stale never silently →
re-check → receipt in the PR. Who verifies the curator (§C8.4): its report passes
`soul_check {verify_curator_report}` run by a DIFFERENT agent — grader ≠ author.

For host setup, prefer exporting `M1ND_WORKSPACE_ROOT` to the actual repository
or project root. Claude Code, Antigravity, Gemini, Cursor, Windsurf, VS Code,
and generic shells can expose their own workspace hints too, but
`M1ND_WORKSPACE_ROOT` is the portable contract.

When the host binding looks wrong, run the read-only status/plan cockpit before
mutating anything. Apply is a separate opt-in mutation step:

```bash
m1nd hosts status --host all --project /path/to/project --json
m1nd hosts plan --host all --project /path/to/project --json
m1nd hosts apply --host all --project /path/to/project --yes --json
```

`hosts plan` emits the install, MCP-config, `M1ND_WORKSPACE_ROOT`, rebind, and
verification recipe for each supported packaged host. Prefer
`M1ND_WORKSPACE_ROOT` in host config so the binding does not fall back to
`OLDPWD` or another ambient workspace hint.

## L1GHT

Use `adapter="light"` for graph-native semantic markdown. Use
`adapter="universal"` or `adapter="auto"` for ordinary docs, wiki pages, PDFs,
and office documents.
