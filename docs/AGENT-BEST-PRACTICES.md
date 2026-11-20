# m1nd — best practices for agents

m1nd is built for agents. The goal is that an agent wants to use it, every session, because it is
genuinely useful to that agent; humans benefit as a consequence. This manual collects how to use
m1nd well, and it grows from real use.

Rule of entry: every practice carries its evidence.

- `PROVEN` — measured with a check fixed before the run.
- `OBSERVED` — seen in real work, not yet measured.
- `RULE` — follows from how the code works; the path is cited.

Written against the unreleased 2.0.0 source tree on `main`, through #608 and the `impact`
causal-chain cap. Practices that depend on unreleased behavior say so; where a practice changed,
the earlier form is kept for older builds. Line numbers drift; the named files and functions are
the anchor.

---

## 1. First contact

**1.1 Pass a stable, descriptive `agent_id` on every call.** `RULE` — every tool requires it, and it
is how `health.active_sessions` and presence tell agents apart. Use `<host>-<role>`, not a random id.

**1.2 Read `graph_drift` before trusting line numbers.** `RULE` + `OBSERVED` — on `main` (2.0,
unreleased, #608), `north` and `health` carry a `graph_drift` note only when the graph was built at
another commit than your checkout: `indexed` (the commit the ingest walked), `head`, `behind`,
`files_changed`, and `refresh`, the exact `ingest` call that brings the graph to `HEAD`
(`m1nd-mcp/src/graph_drift.rs`). Run that call, or read line numbers as of `indexed` and verify them
against your files. Absent means the graph is at your `HEAD`, or no commit was recorded. The note
is 234 bytes when present and nothing at `HEAD`; a paired warm run measured +1.1 ms median. Before
it existed, a new session received a graph several commits behind `HEAD` without warning (2,355
nodes against 2,615 after a refresh), so on an older build compare `git rev-parse HEAD` with the
commit you last ingested yourself. The agent-callable refresh is `ingest {"mode": "refresh"}` from
the exact declared root — the only `ingest` an agent may call; it refuses non-exact roots,
concurrent refreshes, and a shrink of more than 40% (`AGENTS.md`, write law 4). A refresh of a
~2.6k-node repository took 0.49 s.

**1.3 Call `health` once, not per question.** `OBSERVED` — the full payload is ~4.5 KB.

**1.4 A launcher grant births the brain; no ceremony needed.** `PROVEN` — on direct stdio, set
`M1ND_RUNTIME_DIR` (a private writable runtime) and `M1ND_WORKSPACE_ROOT`, and the first call
ingests: the m1nd repository itself (18,078 nodes, 78,981 edges) took 19 s. See
[agent autonomy](AGENT-AUTONOMY.md).

**1.5 Never copy a runtime directory to reuse it; birth a new one.** `OBSERVED` — a `cp -R` copy of
a born runtime made every stdio call hang until the client's 120 s timeout, under old and new
binaries alike, while the original runtime answered normally. m1nd does not fail fast here yet, so
the hang looks like a slow graph. Point `M1ND_RUNTIME_DIR` at a new private directory and let the
launcher grant birth it (1.4).

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

**2.5 On `main` (2.0, unreleased, after #607), rows are lean; ask for `detail` only to inspect a
ranking.** `PROVEN` — a default row carries `node_id`, `label`, `type`, `score`, a short intent
line, `file_path`, the line span, and `symbols`; `connections`, `heuristic_signals`, and
`score_breakdown` come back with `detail: true`. On 16 registered questions, bracketed against the
previous build with frozen plasticity, the median packet fell from 10,736 to 4,270 tokens (−60%),
node ids stayed identical 16/16, and the facts returned with their span did not change. Ids, order,
scores, and every field outside `results` are the same with or without `detail`, so it never
changes what you would act on.

## 3. Asking who calls / what breaks

**3.1 Route relation questions to `impact`, not `seek`.** `PROVEN` (small) — `impact` with
`direction: "reverse"` on the top `seek` node added +1/+2 facts on "who calls / what breaks"
questions (routed 11/24 against symbols-only 10/24). The evidence is still thin; re-measure on a
larger set before treating it as a rule.

**3.2 Ask `impact` about functions, never about a whole file, then confirm with a literal
`search`.** `PROVEN` — on an 18k-node graph, reverse `impact` on a file answered 3.6–3.8 MB: 9,999
`causal_chains`, two thirds of the answer, while the useful `blast_radius` was 150 entries in
27 KB. On a type or a file it also mixes same-named methods (`new`, `analyze`) and collapses to
crate modules. On `main` (2.0, unreleased), `causal_chains` stops at the strongest 20 by default —
`total_causal_chains` and `causal_chains_truncated` say what was left out, and `max_causal_chains`
raises the limit — which cut the same call to about 40 KB with an identical blast radius. The cap
makes a file target cheap, not precise: aim at the function `seek` returned.

## 4. Before you call a change done

**4.1 Read `impact`'s `co_change` for every file you touched; answer for each partner outside your
diff.** `PROVEN` (n=3 repositories) + `OBSERVED` (n=1) — on `main` (2.0, unreleased, #606), when the
brain's root is a git repository, `impact` carries up to five files that changed in the same commits
as the target's file and that the graph does not link to it directly: `file`, `together` (joint
commits), and `last` (`git show <last>` shows the pair changing together). A symbol target answers
for its file, so aim at a function in each touched file; a file without symbols, such as a doc or a
manifest, is its own target. In an offline experiment with thresholds registered before the run, on
three repositories, 34–58% of co-changed pairs were partners that neither grep nor import edges
reach, precision at 5 was 0.49–0.85, and all three passed. Restricted to source files it failed in
two of the three, so expect the files agents forget to change together — sibling manifests, coupled
docs, release metadata — not hidden code-to-code coupling. On m1nd's own drift-note pull request,
`co_change` on the two touched source files named two skill files that changed with them 19 times
each; the change had updated only a third skill, so agents loading the other two would never have
learned the new field. It was fixed before merge.

A partner is a question, not an order: it may not need to change. An absent field means no
qualifying history, not proof that nothing moves with the file. The history is recorded by a code
`ingest` refresh of a git root (`m1nd-mcp/src/co_change_history.rs`), so a graph last ingested by an
earlier build carries no `co_change` until its next refresh.

**4.2 End every task with one letter in the box.** `RULE` — append one JSON line to
`~/.m1nd/field-reports.jsonl`; `m1nd-mcp/src/mailbox.rs` files it by `repo` into
`<repo>/.m1nd/inbox.jsonl`. Give it a `class` — `win`, `friction`, `bug`, or `honesty` — the `tool`,
`what` you asked and what came back, and what you `expected`. Wins count as much as bugs: they show
which verbs agents actually reach for. Never put secrets or tokens in a letter.

```json
{"ts": "2026-11-20T10:00:00Z", "agent": "codex-reviewer", "class": "friction", "repo": "/path/to/repo", "tool": "impact", "what": "asked impact on a function in src/a.rs; no co_change field came back", "expected": "partners from git history; the graph was last refreshed by an older build"}
```

## 5. Many sessions, one brain

**5.1 One owner per runtime; everyone else attaches.** `PROVEN` — run one `m1nd-mcp --serve` per
runtime and connect the other sessions with `--attach <url> --stdio` (or `--attach auto`). A second
stdio process on an owned runtime dies with `CONNECTION_CLOSED` before the handshake. Diagnose with
`lsof` on the runtime lease.

**5.2 A shared owner hosts many project brains; attach by explicit URL.** `PROVEN` (sandbox) — one
`--serve` owner routed two concurrent `--attach http://127.0.0.1:<port> --stdio` clients to their
own brains by caller root. The client sends the `M1nd-Caller-Root` header, resolved as the first of
`M1ND_WORKSPACE_ROOT`, `M1ND_PROJECT_ROOT`, `M1ND_REPO_ROOT`, `WORKSPACE_ROOT`, `PROJECT_ROOT`,
`REPO_ROOT` that names an existing directory, else the working directory
(`resolve_caller_root` in `m1nd-mcp/src/attach_client.rs`). `--attach auto` finds the owner only when
the client's `M1ND_RUNTIME_DIR` equals the owner's runtime. Never export the `*_ROOT` variables
globally: they override the working directory and route every session to one project. With the
owner down, attach fails loudly (`-32002`); there is no hidden private owner. Hosted brains are born
by `m1nd init --birth` (human-only); launcher-born runtimes cannot be adopted.

**5.3 Never use `--read-only` as a fallback for a second session.** `RULE` — startup acquires the
checkpoint writer (`CheckpointStore::open`) before it reads the read-only flag
(`m1nd-mcp/src/brain_runtime.rs`), so a read-only process can take the owner down.

**5.4 Worktrees share the main repository's brain — check lines against your checkout.** `RULE` +
`OBSERVED` — by design a git worktree binds to its main repository's brain, and minting a second
brain for a worktree on the same owner refuses (`overlap_worktree`; `AGENTS.md`, write law 2). So a
session in a worktree on another commit gets main's files and lines. On `main`, when the worktree
sits inside the indexed root, `graph_drift` compares against it and names it in `checkout`; either
way, re-read every span in your own checkout. For measurement work, a private runtime (its own
`M1ND_RUNTIME_DIR` plus `M1ND_WORKSPACE_ROOT=<worktree>`) binds the worktree directly.

## 6. Context economy

**6.1 Cap results with `top_k`.** `OBSERVED` — the envelope outside `results` is 26–30% of each
`seek` packet; fewer rows cut the bulk, not the envelope.

**6.2 Do not truncate JSON at the client.** `OBSERVED` — a client-side byte cap cut a packet
mid-string and broke parsing. Cap on the server side with `top_k` or `token_budget` instead.

**6.3 After parsing a response, drop `formatted` and `agent_runtime_contract`.** `OBSERVED` —
5–10k characters per call that the agent does not need once the JSON is parsed.

## 7. What to keep out of m1nd

**7.1 Put standing decisions in the always-loaded agent file, not in `memorize`.** `PROVEN` — a
decisions block in `AGENTS.md` was answered 5/5 by cold sessions on three hosts; the same facts
stored with `memorize` were found 0/5.

## 8. Discovering capability

**8.1 The menu is a core, not the shop: ask `help`.** `RULE` — `tools/list` advertises 15 verbs;
`help` with `tool_name` returns any registered verb's schema, and `health.tool_surface_contract`
reports the counts. Verbs marked `POLICY-DISABLED (authority floor …)` refuse generic calls; read
the mark before planning around one.

## 9. Developing m1nd (for m1nd developers)

**9.1 Every change to `seek`/`north` ranking runs the bracketed regression proof.** `PROVEN` —
same-binary runs were not identical before #601 (14/16, 13/16), so a single OLD-against-NEW
comparison lies. Run OLD, NEW, then OLD again with frozen plasticity (`M1ND_PLASTICITY_STATE`
pointed at a fresh copy per call), and require NEW to be at least as stable as OLD.

**9.2 Measure line overlap, not string hits.** `PROVEN` — a string-match metric overstated a change
that actually regressed file recall. Judge by whether the fact's file and span are returned.

**9.3 Measure latency in a warm session, bracketed or paired.** `PROVEN` — one process per call
(boot plus one call, ~13 s on an 18k-node graph) showed a change at +122 ms; the same change
measured with 10 calls inside one warm process (OLD-a / NEW / OLD-b, noise 42 ms) cost +4.4 s per
call. Per-process timing hides costs that appear only on repeated calls, which is exactly how agents
use m1nd. Bracket OLD, NEW, OLD in warm sessions, or pair old and new warm processes on the same
calls, and report the median next to the noise.

**9.4 Code fixtures are source code to the extractors.** `PROVEN` — a Rust enum written as a
multi-line ordinary string in a test module was parsed as a real enum by the line-based extractor,
giving m1nd's own repository a contested id; review caught it before merge. Write code fixtures on
one physical line with `\n` escapes, and keep a sentinel on the real-repository ingest test that
fails when any node is dropped.

**9.5 Before cutting a doc or a verb, check the locks.** `RULE` — the sha256 pins in
`.github/workflows/ci.yml`; the two prose tests (`tests/test_public_release_documentation.py`,
`tests/test_agent_surface_bootstrap_honesty.py`); `require_exists` in `xray.manifest.json`; the
`include_str!` of `docs/system-blocks/m1nd.seed.v0.json` in `m1nd-mcp/src/http_server.rs`; and the
source list in `compiled_producer_build_identity` (`m1nd-ingest/src/ownership.rs`, near lines
986–1005). Three "safe" cuts in one sweep hit one of these.

**9.6 Usage counts mean nothing until harness traffic is separated.** `OBSERVED` — 400 of ~405
`seek` calls came from one proof harness.

**9.7 Develop m1nd with m1nd.** `RULE` — `AGENTS.md`, "Develop m1nd with m1nd": `north`, `seek`, and
`impact` while you work, `co_change` on every touched file before you call a change done (4.1), and
one letter per task (4.2).

## 10. Traps found in a code sweep (2026-11-19, verified in code), and what has closed since

A trap marked **closed on `main`** still applies to any build before the named change.

**10.1 For orientation call `north`.** `RULE` — **closed on `main` by #604.** Before it, `help`'s
"Need repo orientation" card pointed to `audit`, which refuses every generic client: its action
`graph.audit.replace` sits at `PositiveSovereign` in `m1nd-control/src/action_catalog.rs`. The card
now points to `north`, and every route, card, and next-step list is checked against the floor table
the dispatch gate uses (`m1nd-mcp/src/help_guidance.rs`).

**10.2 Ignore suggestions that name `learn`, `ghost_edges`, `audit`, or `ingest` with merge or
replace.** `RULE` — **closed on `main` by #604**: no agent-facing string names a verb the same
caller cannot call, and next-step lists are filtered at runtime. On an earlier build those verbs
refuse every generic client (`AGENTS.md`, write law 4).

**10.3 On a served owner, read counts from the `health` verb or `/api/instance/self`.** `RULE` —
**closed on `main` by #603**: `/api/health` serves counts from the actor's graph census, and `null`
with a reason when there is no reading. Before it, `/api/health` fell back to a zeroed
`empty_health_snapshot` (`m1nd-mcp/src/http_server.rs`).

**10.4 `predict`'s git half.** `RULE` — **changed on `main` by #606**: the commit history recorded for
`co_change` seeds `predict`'s co-change matrix after every graph install (`seed_predict_matrix` in
`m1nd-mcp/src/co_change_history.rs`). Before it, only the policy-disabled `ghost_edges` filled that
matrix, so `predict` answered from structure alone. The effect on `predict`'s answers has not been
measured.

**10.5 `help` takes `tool_name`.** `OBSERVED` — **closed on `main` by #604**: `help` with an unknown
key names `tool_name`. Before it, a wrong key was silently ignored.

**10.6 Escape codes and repeats in tool text.** `OBSERVED` — **closed on `main` by #605**: no tool
response carries an ANSI escape, and `help` states each fact once (its overview fell from 5,258 to
3,350 bytes on the test fixture).

**10.7 Never call hidden analysis verbs blind.** `OBSERVED` — on an 18k-node graph, `layer_inspect`
answered 13 MB, `external_references` 270 KB, `layers` 226 KB, `scan_all` 210 KB, and
`taint_trace` 185 KB in one call.

**10.8 A top score near 0.4 with unrelated hits usually means the concept is absent.** `OBSERVED` —
m1nd does not yet say "absent" for docs-only concepts; confirm with a literal `search`.

**10.9 On a shared owner, pass `manifest_path` to `xray_gate`.** `OBSERVED` — auto-discovery looks
in the runtime directory and can answer `clear` for an import it should block.
