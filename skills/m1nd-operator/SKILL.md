---
name: m1nd-operator
description: Use when the user mentions m1nd or when repo investigation, search, review, docs/spec work, or risky change prep should go through m1nd first before grep, glob, or manual file reads. Covers m1nd-first routing, L1GHT and universal document ingestion, risky edit preparation, document-to-code binding, multi-agent coordination, trails/continuity, daemon alerts, and refreshing the live m1nd tool surface from the local m1nd-mcp binary.
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
  If the host reports `attention`, run `m1nd hosts plan --host all --project
  /path/to/project --json` for the exact install, config, workspace env,
  rebind, and verification recipe.
- If the live MCP surface exposes `trust_selftest`, call it first and route by
  `verdict` before relying on retrieval. `full_trust` means proceed with
  m1nd-first; `needs_ingest` means ingest the intended repo; `orientation_only`
  or `degraded_host_tool_surface` means use m1nd only for orientation and
  verify final truth with local files until the binding is refreshed;
  `wrong_workspace_binding` means the active graph is healthy but bound to the
  wrong repo for the requested scope; `stale_binding_suspected` means compare
  binding fingerprints and follow the recovery playbook before trusting
  retrieval.
- If `trust_selftest` is not exposed but `session_handshake` is, call the
  handshake and route by `trust_mode` as the cheaper sub-check. When the task
  names a target repo or absolute path, pass it as `scope` so Context Guard can
  detect cross-repo binding mistakes before retrieval.
- If the selftest verdict or handshake trust mode is not `full_trust`, or
  retrieval returns `blocked`/zero candidates unexpectedly, call
  `recovery_playbook` before inventing the next step. Use its ordered steps and
  `binding_fingerprint` to compare host, stdio, HTTP, runtime root, graph paths,
  generation counters, and ingest roots.
- If a response includes `context_guard.wrong_workspace_binding=true`, stop the
  normal stale-graph path. Rebind the MCP host with `M1ND_WORKSPACE_ROOT` set to
  `requested_workspace_hint`, intentionally ingest that workspace on the same
  binding, or use `federate_auto`/`federate` only when the investigation truly
  spans repos. Do not treat this as proof that m1nd retrieval is broken.
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

- Unfamiliar repo or need a one-call orientation: use `audit`, then `batch_view`, `coverage_session`, or `cross_verify` as needed.
- Need a subsystem map: use `activate`.
- Need code by intent: use `seek`.
- Need why A connects to B: use `why`.
- Smells like missing validation, abstraction, cleanup, or lock: use `missing`.
- Have a stacktrace or runtime error text: use `trace`.
- Need blast radius before editing: use `impact`.
- Need co-change follow-through after editing: use `predict`.
- Need plan completeness and missing tests before implementation or review: use `validate_plan`.
- Need graph-native specs, design notes, or KB docs authored in `L1GHT`: ingest with `adapter: "light"` and usually `mode: "merge"`.
- Need regular spec/wiki/PDF/doc alignment with code: ingest with `adapter: "universal"` or `auto`, then use `document_resolve`, `document_bindings`, and `document_drift`.
- Need stateful navigation instead of stateless retrieval: use `perspective_*`.
- Need session continuity or handoff: use `trail_save`, `trail_list`, `trail_resume`, `trail_merge`, and sometimes `boot_memory`.
- Need background structural monitoring: use `daemon_start`, `daemon_status`, `daemon_tick`, `alerts_list`, and `alerts_ack`.

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

Use the bundled probe script from this skill directory whenever the live runtime matters more than remembered docs.

```bash
python3 scripts/probe_m1nd.py tools
python3 scripts/probe_m1nd.py call health '{"agent_id":"codex-m1nd"}'
python3 scripts/probe_m1nd.py call trust_selftest '{"agent_id":"codex-m1nd"}'
python3 scripts/probe_m1nd.py call session_handshake '{"agent_id":"codex-m1nd","scope":"/path/to/intended/repo"}'
python3 scripts/probe_m1nd.py call recovery_playbook '{"agent_id":"codex-m1nd","observed_tool":"seek","observed_proof_state":"blocked","observed_candidates":0}'
python3 scripts/probe_m1nd.py call help '{"agent_id":"codex-m1nd","tool_name":"validate_plan"}'
python3 scripts/probe_m1nd.py run '[{"name":"ingest","arguments":{"agent_id":"codex-m1nd","path":"/path/to/repo"}},{"name":"seek","arguments":{"agent_id":"codex-m1nd","query":"where retry backoff is decided","top_k":5}}]'
```

`probe_m1nd.py` uses an isolated temporary `--runtime-dir` by default so
parallel agent probes do not fight over the same runtime owner lock. If a
helper or older skill reports `runtime_root ... is already owned by instance`,
do not classify that as graph staleness or retrieval failure. Rerun with the
current helper, pass an explicit unique `--runtime-dir`, or combine dependent
calls with `probe_m1nd.py run` so they share one process intentionally. Use
`--shared-runtime` only when debugging shared runtime state.

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
