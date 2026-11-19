# m1nd — best practices for agents

m1nd is built for agents. The goal is that an agent wants to use it, every session, because it is
genuinely useful to that agent; humans benefit as a consequence. This manual collects how to use
m1nd well, and it grows from real use.

Rule of entry: every practice carries its evidence.

- `PROVEN` — measured with a check fixed before the run.
- `OBSERVED` — seen in real work, not yet measured.
- `RULE` — follows from how the code works; the path is cited.

Written against the unreleased 2.0.0 source tree on `main`, after #601 (stable ranking) and #602
(symbol spans on `seek`). Practices that depend on unreleased behavior say so. Line numbers drift;
the named files and functions are the anchor.

---

## 1. First contact

**1.1 Pass a stable, descriptive `agent_id` on every call.** `RULE` — every tool requires it, and it
is how `health.active_sessions` and presence tell agents apart. Use `<host>-<role>`, not a random id.

**1.2 Check freshness before trusting line numbers.** `OBSERVED` — a new session received a graph
several commits behind `HEAD` without warning (2,355 nodes against 2,615 after a refresh); `health`
shows the git head but does not compare it with the indexed commit. Until m1nd reports drift
itself, compare `git rev-parse HEAD` with the commit you last ingested and refresh when they
differ. The agent-callable refresh is `ingest {"mode": "refresh"}` from the exact declared root —
the only `ingest` an agent may call; it refuses non-exact roots, concurrent refreshes, and a
shrink of more than 40% (`AGENTS.md`, write law 4). A refresh of a ~2.6k-node repository took
0.49 s.

**1.3 Call `health` once, not per question.** `OBSERVED` — the full payload is ~4.5 KB.

**1.4 A launcher grant births the brain; no ceremony needed.** `PROVEN` — on direct stdio, set
`M1ND_RUNTIME_DIR` (a private writable runtime) and `M1ND_WORKSPACE_ROOT`, and the first call
ingests: the m1nd repository itself (18,078 nodes, 78,981 edges) took 19 s. See
[agent autonomy](AGENT-AUTONOMY.md).

## 2. Asking where and how

**2.1 Use `seek` for "where is X / how does Y work".** `PROVEN` — it ranks files well for
natural-language questions.

**2.2 On published 1.6.5 (before #602), ask for symbols explicitly.** `PROVEN` — narrative
questions return `result_granularity: file_group`: whole files, lines 1..N.
`node_types: ["function", "struct", "class", "module"]` found 14/24 facts against 10/24, with ~35%
fewer tokens, on a fixed question set. It also lowers file recall (13/26 → 9/26 files in the top
5), so make it a second call, not a replacement for the default.

**2.3 On `main` (2.0, unreleased, after #602), the default `seek` already carries the span.**
`PROVEN` — each of the first 10 file-group rows has up to 3 `symbols` with
`line_start`/`line_end`. Facts whose span is returned rose 10 → 17/24 and 5 → 11/24, packets got
~8% smaller, and file ranking stayed identical 16/16. Read the symbol spans before reading whole
files.

**2.4 m1nd locates; your file read confirms.** `OBSERVED` — a stale line number made a grader
reject a true citation. Always open the span before quoting it.

## 3. Asking who calls / what breaks

**3.1 Route relation questions to `impact`, not `seek`.** `PROVEN` (small) — `impact` with
`direction: "reverse"` on the top `seek` node added +1/+2 facts on "who calls / what breaks"
questions (routed 11/24 against symbols-only 10/24). The evidence is still thin; re-measure on a
larger set before treating it as a rule.

## 4. Many sessions, one brain

**4.1 One owner per runtime; everyone else attaches.** `PROVEN` — run one `m1nd-mcp --serve` per
runtime and connect the other sessions with `--attach <url> --stdio` (or `--attach auto`). A second
stdio process on an owned runtime dies with `CONNECTION_CLOSED` before the handshake. Diagnose with
`lsof` on the runtime lease.

**4.1b A shared owner hosts many project brains; attach by explicit URL.** `PROVEN` (sandbox) — one
`--serve` owner routed two concurrent `--attach http://127.0.0.1:<port> --stdio` clients to their
own brains by caller root. The client sends the `M1nd-Caller-Root` header, resolved as the first of
`M1ND_WORKSPACE_ROOT`, `M1ND_PROJECT_ROOT`, `M1ND_REPO_ROOT`, `WORKSPACE_ROOT`, `PROJECT_ROOT`,
`REPO_ROOT` that names an existing directory, else the working directory
(`resolve_caller_root` in `m1nd-mcp/src/attach_client.rs`). `--attach auto` finds the owner only when
the client's `M1ND_RUNTIME_DIR` equals the owner's runtime. Never export the `*_ROOT` variables
globally: they override the working directory and route every session to one project. With the
owner down, attach fails loudly (`-32002`); there is no hidden private owner. Hosted brains are born
by `m1nd init --birth` (human-only); launcher-born runtimes cannot be adopted.

**4.2 Never use `--read-only` as a fallback for a second session.** `RULE` — startup acquires the
checkpoint writer (`CheckpointStore::open`) before it reads the read-only flag
(`m1nd-mcp/src/brain_runtime.rs`), so a read-only process can take the owner down.

**4.3 Worktrees share the main repository's brain — check lines against your checkout.** `RULE` +
`OBSERVED` — by design a git worktree binds to its main repository's brain, and minting a second
brain for a worktree on the same owner refuses (`overlap_worktree`; `AGENTS.md`, write law 2). So a
session in a worktree on another commit gets main's files and lines. Until m1nd notes the drift
itself, re-read every span in your own checkout. For measurement work, a private runtime (its own
`M1ND_RUNTIME_DIR` plus `M1ND_WORKSPACE_ROOT=<worktree>`) binds the worktree directly.

## 5. Context economy

**5.1 Cap results with `top_k`.** `OBSERVED` — the envelope outside `results` is 26–30% of each
`seek` packet; fewer rows cut the bulk, not the envelope.

**5.2 Do not truncate JSON at the client.** `OBSERVED` — a client-side byte cap cut a packet
mid-string and broke parsing. Cap on the server side with `top_k` or `token_budget` instead.

## 6. What to keep out of m1nd

**6.1 Put standing decisions in the always-loaded agent file, not in `memorize`.** `PROVEN` — a
decisions block in `AGENTS.md` was answered 5/5 by cold sessions on three hosts; the same facts
stored with `memorize` were found 0/5.

## 7. Discovering capability

**7.1 The menu is a core, not the shop: ask `help`.** `RULE` — `tools/list` advertises 15 verbs;
`help` with `tool_name` returns any registered verb's schema, and `health.tool_surface_contract`
reports the counts. Verbs marked `POLICY-DISABLED (authority floor …)` refuse generic calls; read
the mark before planning around one.

## 8. Changing m1nd's search (for m1nd developers)

**8.1 Every change to `seek`/`north` ranking runs the bracketed regression proof.** `PROVEN` —
same-binary runs were not identical before #601 (14/16, 13/16), so a single OLD-against-NEW
comparison lies. Run OLD, NEW, then OLD again with frozen plasticity (`M1ND_PLASTICITY_STATE`
pointed at a fresh copy per call), and require NEW to be at least as stable as OLD.

**8.2 Measure line overlap, not string hits.** `PROVEN` — a string-match metric overstated a change
that actually regressed file recall. Judge by whether the fact's file and span are returned.

## 9. Traps found in a code sweep (2026-11-19, verified in code)

**9.1 For orientation call `north`; never follow `help`'s "repo orientation" card.** `RULE` — the
card points to `audit`, which refuses every generic client: its action `graph.audit.replace` sits
at `PositiveSovereign` in `m1nd-control/src/action_catalog.rs`. The card lives in
`m1nd-mcp/src/help_guidance.rs` (the `HelpStage::Orient` guidance near line 1000 and the "Need repo
orientation" overview card near line 1830). Until that card changes, start with `north`.

**9.2 Ignore suggestions that name `learn`, `ghost_edges`, `audit`, or `ingest` with merge or
replace.** `RULE` — they refuse for every generic client. The suggestions come from the next-step
lists in `m1nd-mcp/src/personality.rs` (lines 80–168, and the `next` list near line 853) and the
`ingest` merge card in `m1nd-mcp/src/help_guidance.rs` (near line 1856); the refusal is
`AGENTS.md`, write law 4.

**9.3 On a served owner, read counts from the `health` verb or `/api/instance/self`, not
`/api/health`.** `RULE` — `/api/health` serves the cached snapshot, which falls back to a zeroed
`empty_health_snapshot` (`m1nd-mcp/src/http_server.rs`, near line 1801).

**9.4 `predict` answers from structure only today.** `RULE` — its git half is empty, because only
the policy-disabled `ghost_edges` fills the co-change matrix (the co-change fallback in
`m1nd-mcp/src/tools.rs` near lines 2834–2870; `handle_ghost_edges` in
`m1nd-mcp/src/layer_handlers.rs`).

**9.5 `help` takes `tool_name`; a wrong key is silently ignored.** `OBSERVED`.

**9.6 Never call hidden analysis verbs blind.** `OBSERVED` — on an 18k-node graph, `layer_inspect`
answered 13 MB, `external_references` 270 KB, `layers` 226 KB, `scan_all` 210 KB, and
`taint_trace` 185 KB in one call.

**9.7 Ask `impact` about functions, then confirm with a literal `search`.** `OBSERVED` — on a type
or a file it mixes same-named methods (`new`, `analyze`) and collapses to crate modules.

**9.8 A top score near 0.4 with unrelated hits usually means the concept is absent.** `OBSERVED` —
m1nd does not yet say "absent" for docs-only concepts; confirm with a literal `search`.

**9.9 After parsing a response, drop `formatted` and `agent_runtime_contract`.** `OBSERVED` —
5–10k characters per call that the agent does not need once the JSON is parsed.

**9.10 On a shared owner, pass `manifest_path` to `xray_gate`.** `OBSERVED` — auto-discovery looks
in the runtime directory and can answer `clear` for an import it should block.

**9.11 (developers) Before cutting a doc or a verb, check the locks.** `RULE` — the sha256 pins in
`.github/workflows/ci.yml`; the two prose tests (`tests/test_public_release_documentation.py`,
`tests/test_agent_surface_bootstrap_honesty.py`); `require_exists` in `xray.manifest.json`; the
`include_str!` of `docs/system-blocks/m1nd.seed.v0.json` in `m1nd-mcp/src/http_server.rs`; and the
source list in `compiled_producer_build_identity` (`m1nd-ingest/src/ownership.rs`, near lines
986–1005). Three "safe" cuts in the sweep hit one of these.

**9.12 (developers) Usage counts mean nothing until harness traffic is separated.** `OBSERVED` —
400 of ~405 `seek` calls came from one proof harness.
