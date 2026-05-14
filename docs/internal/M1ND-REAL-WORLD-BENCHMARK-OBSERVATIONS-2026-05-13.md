# m1nd Real-World Benchmark Observations

Date: 2026-05-13
Round: `real-world-20260513T005733Z`
Status: internal product learning, not public benchmark copy

## Source Artifacts

- `docs/benchmarks/real-world-rounds/real-world-20260513T005733Z/round.json`
- `docs/benchmarks/real-world-rounds/real-world-20260513T005733Z/report.json`
- `docs/benchmarks/real-world-rounds/real-world-20260513T005733Z/ROUND-NOTES.md`
- `docs/benchmarks/real-world-rounds/real-world-20260513T005733Z/lane-results/*.json`
- `docs/benchmarks/real-world-rounds/real-world-v2-20260513T231822Z/round.json`
- `docs/benchmarks/real-world-rounds/real-world-v2-20260513T231822Z/report.json`
- `docs/benchmarks/real-world-rounds/real-world-v2-20260513T231822Z/ROUND-NOTES.md`
- `docs/benchmarks/real-world-rounds/real-world-v2-20260513T231822Z/operator-only/judge-input.json`
- `docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/round.json`
- `docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/report.json`
- `docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/ROUND-NOTES.md`
- `docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/operator-only/answer-key.json`
- `docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-tempo-20260514T145029Z/report.json`
- `docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-tempo-20260514T145029Z/ROUND-NOTES.md`
- `docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-tempo-20260514T145029Z/operator-only/answer-key.json`

## Bottom Line

Update from the deterministic v2 round: the benchmark harness now produced a
complete adjudicated result across all six primary lanes and all sixty primary
tasks. That is a methodological unlock, not a public performance claim.

Update from the first accepted bug-hunt round: a seeded-defect audit on
`humanize` produced a stronger directional signal than the earlier broad task
rounds. The useful lesson is precise: m1nd is most valuable when agents receive
the operating doctrine, not merely when a graph exists somewhere in the
environment.

Bug-hunt seeded recall:

- `m1nd-trained`: `16/20` seeded bugs found, `80.0%` recall, per-lane counts
  `[3, 5, 4, 4]`
- `m1nd-basic`: `8/15` seeded bugs found, `53.3%` recall, per-lane counts
  `[2, 3, 3]`
- `direct`: `8/15` seeded bugs found, `53.3%` recall, per-lane counts
  `[3, 2, 3]`

Tempo comparison round on `p-limit`:

- `m1nd-temponizer-compact`: `10/10` seeded bugs found, `100.0%`
  recall, per-lane counts `[5, 5]`
- `m1nd-temponizer-full`: `6/10` seeded bugs found, `60.0%` recall,
  per-lane counts `[4, 2]`
- `m1nd-temponizer`: `9/10` seeded bugs found, `90.0%` recall, per-lane counts
  `[5, 4]`
- `m1nd-trained`: `9/10` seeded bugs found, `90.0%` recall, per-lane counts
  `[4, 5]`
- `direct`: `7/10` seeded bugs found, `70.0%` recall, per-lane counts `[3, 4]`

Read this honestly: this is still one fixture repo and not public benchmark
copy. It is strong internal product evidence that m1nd's universal agent pack,
first-minute doctrine, workspace checks, and recovery flow are not optional
polish. They are part of the product.

Deterministic v2 adjudicated rollup:

- `m1nd_available`: `29/30` success, median adjudicated score `23`, average
  adjudicated score `22.8/24`
- `no_m1nd`: `30/30` success, median adjudicated score `23`, average
  adjudicated score `22.6/24`
- all `60` task adjudications were comparable
- the only partial was `m1nd-2 / repo_architecture_audit`, which missed
  `src/click/parser.py`

Read this honestly: the deterministic v2 round did not produce a clean public
win for either arm. It did prove that the harness can now run deterministic
tasks, preserve lane artifacts, collect event streams, generate judge input, and
produce a complete adjudicated report.

The first real-world round is useful internal evidence, but not a clean
comparative proof.

The m1nd lanes performed well:

- `m1nd_available`: success rate `0.9667`
- `m1nd_available`: median run score `218`
- `m1nd_available`: median files opened `34`
- `m1nd_available`: median search iterations `29`
- `m1nd_available`: median tests/commands `26`

The control lanes were also strong:

- `no_m1nd`: success rate `0.9333`
- `no_m1nd`: median run score `229`
- `no_m1nd`: median files opened `38`
- `no_m1nd`: median search iterations `14`
- `no_m1nd`: median tests/commands `18`

Read this honestly: m1nd did not produce a decisive public win in this round.
It did produce strong signs of value, especially around structured orientation,
patch completion, proof discipline, and connected-context workflows. It also
showed friction: host binding, local probe ergonomics, benchmark byproducts, and
some parent-repo retrieval noise.

## What m1nd Helped With

### Structured First Contact

The m1nd lanes consistently used audit/seek/activate style entrypoints before
settling into direct files. This made their result JSONs read more like
connected investigation trails than raw grep transcripts.

Observed value:

- better architectural language around module boundaries
- clearer distinction between public API, implementation, tests, and docs
- more explicit proof and non-claim notes
- fewer median files opened than controls (`34` vs `38`)

### Patch Completion

All m1nd patch tasks completed:

- code change completion rate: `1.0`
- patch task successes: `6/6`

This is not yet proof that m1nd is better at patching, because patch targets
were not deterministic across lanes. It does show that m1nd did not slow agents
into non-completion, even with host friction.

### Proof Discipline

m1nd lanes tended to record host/tool limitations instead of hiding them. That
is useful for agent-first systems: a tool that teaches an agent when not to
trust its own context is valuable, even if that dimension should not dominate
every benchmark.

Examples:

- m1nd lanes recorded when host-side MCP was unavailable or wrong-workspace
- m1nd lanes switched to repo-local probes and verified final truth with tests
- m1nd lanes preserved missing supplied diff / seeded bug proof in some tasks

## Where m1nd Hurt Or Added Friction

### Host MCP Was Still Not Smooth Enough

Multiple m1nd lanes reported that the direct host-side MCP surface was missing,
wrong-workspace, or not trustworthy. They recovered by using repo-local smoke or
`probe_m1nd.py`, but that is extra ceremony.

Product implication:

Agents should not need to reason about three different m1nd access paths during
ordinary repo work. The ideal is one obvious command or tool surface that says:

- active workspace
- graph status
- tool surface status
- recommended next action
- exact fallback command if host MCP is unusable

### Repo-Local Probe Byproducts

At least one m1nd lane reported generated `ingest_roots.json` byproducts inside
isolated fixture repos. They were inside ignored benchmark fixtures, but this is
still agent-hostile behavior during benchmarks and patch tasks.

Product implication:

Probe/ingest flows need a cleaner `--no-worktree-artifacts` or
`--state-dir <path>` default for benchmark/agent contexts. Runtime metadata
should go to a controlled m1nd state directory, not into the target repo unless
explicitly requested.

### Retrieval Noise Across Contexts

One m1nd lane noted that a local activation pass surfaced stray nodes from the
parent m1nd repo before direct file verification corrected the answer.

Product implication:

Context Guard should be more visible in normal retrieval output, not only in
recovery cases. The result envelope should make it hard to miss when a result
comes from a different ingest root, graph namespace, or parent repo.

### More Search Iterations

m1nd lanes had higher median search iterations (`29`) than controls (`14`).
This may be partly because m1nd lanes logged tool/probe steps more carefully,
but it still matters.

Product implication:

If m1nd adds structural steps, those steps need to collapse later work. Better
benchmark telemetry should separate:

- m1nd graph calls
- shell searches
- file reads
- test/proof commands
- repeated reads
- failed/rerouted retrievals

## What Controls Proved

The control lanes were very strong. A good LLM with `rg`, direct files, and test
commands can perform real work in mature repos. That means m1nd's bar is not
"better than blind grep"; it is "better than a strong agent already using repo
tools well."

Controls were especially strong at:

- direct code localization
- self-contained patching
- fast source inspection
- keeping search iteration counts low

This is good news. It forces m1nd to earn its place as an agent-first
coordination and reasoning layer, not a decorative wrapper around search.

## Bug-Hunt Humanize Round

The first accepted bug-hunt round used `humanize` at commit
`877e1fda9829073fe086625915d1197cd07412ad`, with five seeded defects across
number, list, and time formatting behavior. Primary lanes were not told the bug
count or comparison arm.

Seeded bugs:

- `intcomma-negative-numbers-not-grouped`
- `fractional-negative-proper-fraction-loses-sign`
- `clamp-equal-boundary-marked-out-of-range`
- `natural-list-empty-list-renders-none`
- `naturaltime-numeric-future-flag-ignored`

Result:

- one `m1nd-trained` lane found all five seeded bugs
- every `m1nd-trained` lane found at least three seeded bugs
- `m1nd-basic` matched direct search/control performance in this round
- both `m1nd-basic` and direct lanes systematically missed the
  `naturaltime(..., future=True)` numeric-tense bug
- extra findings were preserved as unadjudicated, not converted into false
  positives

Interpretation:

The graph is not the whole product. The product is the graph plus the agent's
operating ritual: trust check, ingest or scope confirmation, cheap retrieval,
connected activation, direct-file proof, and explicit fallback when m1nd is
blocked. The `m1nd-trained` result says the doctrine should ship with m1nd as a
first-class artifact, not as a side note.

Status:

- promoted the measured loop into `skills/m1nd-first/SKILL.md`
- promoted the measured loop into `skills/m1nd-operator/SKILL.md`
- promoted the measured loop into `skills/m1nd-universal-agent-pack.md`
- documented `m1nd-trained`, `m1nd-basic`, and `direct` as separate bug-hunt
  instruction modes in `docs/benchmarks/README.md`
- added `scripts/benchmark/bug_hunt_round.py init` so future bug-hunt rounds
  can create lane prompts, result templates, event streams, and answer-key
  scaffolding without hand assembly
- added `docs/benchmarks/BUG_HUNT_BENCHMARK_PROTOCOL_2026-05-14.md`

Important non-claims:

- not a public performance claim
- not proof of universal bug-finding superiority
- not a precision score, because extra findings were not judged
- not evidence that m1nd helps equally without correct agent training
- not a replacement for tests, compiler output, direct file truth, or human
  review

## Bug-Hunt p-limit Tempo Round

The `p-limit` round tested whether Tempo/TEMPONIZER adds value on top of the
trained m1nd loop. It started with six lanes and was extended with two
additional full-spec Temponizer lanes:

- two `m1nd-temponizer-full`
- two `m1nd-temponizer`
- two `m1nd-trained`
- two `direct`

Result:

- `m1nd-temponizer-full`: `6/10`, `60.0%`
- `m1nd-temponizer`: `9/10`, `90.0%`
- `m1nd-trained`: `9/10`, `90.0%`
- `direct`: `7/10`, `70.0%`

Interpretation:

The lighter Tempo prompt did not improve seeded recall over `m1nd-trained` in
this small fixture, but it did improve operational telemetry: the lanes recorded
phase/time notes, made explicit probe-vs-full-test decisions, and avoided
treating long human-duration intuition as proof of rigor.

The full-spec Temponizer prompt underperformed. This is valuable negative
evidence about prompt integration, not a rejection of the temporal model. The
full prompt carried the correct formula and constraints, but it also added
bookkeeping pressure and caution. One lane avoided live m1nd because the helper
path could write sidecar state outside the narrow artifact allowlist; the other
lane used m1nd but excluded lower-confidence defects it had partially noticed.

Product lesson: Temponizer should become compact agent operating physics inside
m1nd, not a verbose checklist. Preserve the core model:

- classify phase `phi` as `GEN`, `IO`, `DBG`, or `PAR`
- compute `Tc = alpha(phi) * Tp`
- act on agent constraints, not inherited human-duration intuition
- record `Te` only around meaningful decision points

Do not make every move pay a reporting tax. Measure Tempo on efficiency,
abort/iterate quality, and handoff telemetry, not only raw bug recall.

Follow-up implemented in the benchmark harness: `m1nd-temponizer-compact` is now
a distinct instruction mode for the next round. It keeps the full formula but
limits `Te` recording to meaningful branch decisions.

Follow-up compact round:

- `m1nd-temponizer-compact`: `10/10`, `100.0%`, per-lane counts `[5, 5]`
- `m1nd-trained`: `8/10`, `80.0%`, per-lane counts `[4, 4]`
- `direct`: `7/10`, `70.0%`, per-lane counts `[3, 4]`

Interpretation:

The compact form is the first Tempo condition in this fixture that improved
seeded recall over both `m1nd-trained` and direct controls. This is still not a
public claim, but it is a useful product signal: the formula helps when it is
small enough to change decisions without becoming a paperwork layer.

Observed m1nd friction repeated:

- m1nd lanes often started at `needs_ingest`, then reached `full_trust`
- local probe/ingest still materialized graph sidecar files in benchmark
  workspaces during this round; this exposed the native `ingest_roots.json`
  leak fixed below
- final truth still came from direct source reads plus focused runtime probes

Next measurement improvement:

- scorer now records timestamp-derived wall-clock and first-finding fields when
  event streams provide parseable timestamps; stricter finding-event conventions
  are still needed
- m1nd should offer or default to a no-worktree-artifacts state dir for
  benchmark and sidecar-agent contexts

## Benchmark Design Gaps

The adjudicator found only three tasks cleanly comparable:

- `repo_architecture_audit`
- `flow_explanation`
- `bounded_refactor_plan`

The other tasks were useful, but underanchored:

- feature target not fixed
- bug symptom not fixed
- change request not fixed
- seeded bug not actually seeded
- review diff not supplied
- docs claim not fixed

This inflated primary lane self-scores. The next round must generate exact
payloads before the agents start.

## Product Patch Queue

### P0: Deterministic Real-World Benchmark V2

Build a fixture generator that creates a pinned, repeatable task bundle:

- pinned fixture repo commits
- exact feature target per task
- exact bug symptom and expected root cause
- deterministic seeded bug patch per lane
- supplied review diff with answer key
- fixed docs claim to check
- raw event transcript capture
- answer key file with expected files/functions/tests

Acceptance:

- all six primary lanes receive identical task payloads
- no lane chooses its own bug, diff, feature, or docs claim
- report separates self-score from adjudicated score

Status:

- implemented first deterministic payload scaffold in
  `scripts/benchmark/real_world_agent_round.py`
- implemented per-task repo binding, public payloads, answer key generation,
  supplied review diff, and seeded Click regression test injection
- implemented event stream scaffolding and report-level event capture guards
- implemented optional judge `adjudications[]` rollup so self-score and
  adjudicated score can be separated
- implemented fixture lock capture plus lane checkout HEAD verification
- implemented `judge-input` packet generation for adjudicators
- completed one deterministic v2 adjudication pass for
  `real-world-v2-20260513T231822Z`
- remaining: repeat deterministic rounds across more repo families before any
  public claim

### P0: Agent Event Capture

Add a lightweight transcript/evidence capture format for benchmark lanes:

- command/tool event stream
- file open sequence
- m1nd calls vs shell calls
- test commands and return codes
- patch diff summary
- raw elapsed times when available

Acceptance:

- each lane result points to an immutable event artifact
- adjudication does not rely only on compact self-reported JSON

Status:

- event streams now live under `event-streams/<lane-id>.jsonl`
- scorer reports event presence, agent-authored event counts, invalid event
  counts, and blocks claim-worthy output when primary lanes lack agent events
- remaining: host-side automation or lane discipline to capture real events
  consistently without manual copying

### P0: Clean m1nd State Placement

Prevent benchmark/probe runs from writing metadata into target repos unless
requested.

Candidate surfaces:

- `probe_m1nd.py --state-dir <path>`
- `probe_m1nd.py --no-worktree-artifacts`
- `ingest` option for external state root
- benchmark harness default state root under `.m1nd-benchmark-fixtures/state`

Acceptance:

- ingest/probe against fixture repos leaves `git status --short` clean except
  intentional patch files
- generated state paths are reported in the lane result

Status:

- added `probe_m1nd.py --no-worktree-artifacts`
- helper smoke from inside a `p-limit` benchmark workspace initially left no
  `graph_snapshot.json` or `plasticity_state.json`, but the compact follow-up
  round revealed `ingest_roots.json` still leaked into m1nd lane workspaces
- the helper now sets graph/plasticity paths under the isolated runtime dir and
  preserves the caller repo as `M1ND_WORKSPACE_ROOT`
- fixed native `persist_ingest_roots()` so ingest roots persist next to the
  graph snapshot, matching `load_ingest_roots()`
- added Rust regression test
  `ingest_roots_persist_next_to_graph_not_workspace_hint`
- debug-runtime smoke proved graph, plasticity, and ingest-root state land in
  the isolated runtime dir, not the audited repo
- rebuilt/restarted the managed runtime at `~/.m1nd/bin/m1nd-mcp`
- updated `probe_m1nd.py` to prefer the managed runtime before stale `PATH`
  binaries such as `/usr/local/bin/m1nd-mcp`
- `--no-worktree-artifacts` now sets the caller directory as
  `M1ND_WORKSPACE_ROOT` instead of preserving a stale inherited environment
- the helper also accepts `--workspace-root /path/to/repo` for multi-repo
  director sessions where the process is launched outside the audited checkout
- replaced the root-owned `/usr/local/bin/m1nd-mcp` with the same fixed build;
  SHA-256 now matches `~/.m1nd/bin/m1nd-mcp` and `target/release/m1nd-mcp`
- smoke using `/usr/local/bin/m1nd-mcp` plus `--no-worktree-artifacts` now
  leaves the audited repo clean and writes graph/plasticity/ingest-root state
  only under the isolated runtime dir

### P1: One-Command Agent Workspace Doctor

Create a single repo-local command for agents:

```bash
m1nd agent doctor --repo /path/to/repo --json
```

It should report:

- runtime version
- host MCP availability if detectable
- workspace binding
- graph node/edge counts
- ingest roots
- state directory
- suggested first m1nd call
- exact fallback when MCP is missing

Acceptance:

- agents do not need to choose between host MCP, smoke harness, and probe helper
  blindly
- output includes non-claims and recovery route

### P1: Retrieval Scope Provenance

Every retrieval result should expose compact source provenance:

- active workspace
- ingest root
- graph namespace
- result file path
- whether the result is in requested scope
- warning when cross-root or parent-repo results appear

Acceptance:

- parent-repo noise is visible immediately
- agent can filter or re-query without manual suspicion

### P1: Adjudicated Score Layer

Extend `real_world_agent_round.py` so reports include:

- primary lane self-score
- judge score
- task comparability class
- adjudicated arm score
- task-level exclusion reason

Acceptance:

- public-claim logic can depend on adjudicated results, not lane self-report
- underanchored tasks can remain in the corpus without polluting headline metrics

Status:

- judge lanes can now emit `adjudications[]`
- report now rolls adjudicated scores by primary arm and blocks public-claim
  readiness when primary lanes are not completely adjudicated
- generated a complete judge packet and adjudicated all sixty primary task
  results in `real-world-v2-20260513T231822Z`
- remaining: richer task-level exclusion UX and a scorer-compatible judge result
  template so judges do not accidentally write a custom schema

### P2: Task-Specific m1nd Recipes

Create agent recipes for the ten real-world tasks:

- audit: `audit -> activate -> view`
- localize: `seek -> why -> tests`
- flow explanation: `activate -> trace -> view`
- bug triage: `trace -> impact -> focused test`
- safe change plan: `impact -> predict -> validate_plan`
- patch: `surgical_context_v2 -> apply/edit -> tests`
- review: `impact/differential -> changed files -> tests`
- docs drift: `document_resolve -> document_drift -> code truth`

Acceptance:

- m1nd lanes spend fewer exploratory calls
- recipes are available in `m1nd-operator`

## Method Updates

Do not interrupt benchmark lanes mid-run unless the run is explicitly being
aborted. If an intervention happens, record it as an operator intervention and
keep the round internal-only.

Do not let agents choose the target for "seeded bug", "review diff", or "docs
drift" in a comparative round. That turns one task into six different tasks.

Do not publish any metric until:

- fixture commits are pinned
- tasks are deterministic
- event logs are preserved
- adjudicated score agrees with self-score direction
- at least two repo families repeat the same directional signal

## Next Recommended Move

Run at least two more deterministic v2 rounds with real agent-authored event
streams and complete adjudication before comparing arms publicly.

This is the highest-leverage path: the current m1nd product may already be
useful, but we need repeated comparable evidence before claiming how much better
it is.
