# Jimi Guardian Context — 2026-05-05

This note starts the m1nd guardian lane from a fresh checkout of `origin/main`.
It is an internal operating note, not public product copy.

## Current Ground Truth

- Active work branch: `codex/readme-agent-continuity`
- Base observed from origin: `b2cb233 feat(mcp): add agent-native help guidance`
- First guardian commit on this branch: `27c549b docs(readme): show how m1nd preserves agent continuity`
- Public README change: added a concise continuity quote and a short agent testimonial.
- m1nd graph re-ingest on the active checkout succeeded.

## Verified Gates

- `cargo fmt --check`: pass
- `cargo check -p m1nd-mcp -p m1nd-ingest`: pass
- `cargo test -p m1nd-mcp help -- --nocapture`: pass
- Local stdio smoke:
  - server version: `0.8.0`
  - live tool count from `tools/list`: `92`
  - includes `ingest`, `seek`, and `help`
  - local `ingest -> seek` scanned the populated graph and returned results
- First repo-local smoke harness:
  - `python3 scripts/mcp_agent_smoke.py --repo . --json`
  - `python3 scripts/mcp_agent_smoke.py --repo . --transport http --json`
  - current continuation expands this to `initialize -> tools/list -> trust_selftest -> session_handshake -> recovery_playbook when needed -> ingest -> seek -> help -> doctor`
  - uses isolated runtime state, real Content-Length framed stdio, and the HTTP
    tool API

## First Real Friction

The host-provided m1nd MCP surface available to the agent behaved differently
from the local stdio binary.

Observed behavior:

- `ingest` returned a populated graph.
- immediate `seek` returned `proof_state=blocked`
- `seek` reported `total_candidates_scanned=0`

Cross-check:

- the local stdio binary on the same checkout exposed the full live tool
  surface and `ingest -> seek` scanned graph candidates correctly.

Interpretation:

- the core runtime appears healthy in this smoke
- the likely problem is host binding, graph/session continuity, or insufficient
  diagnostic output from retrieval calls
- this is high leverage because it directly affects whether agents trust m1nd
  before falling back to shell search

Captured in:

- `docs/AGENT-TASKNOTES.md`

## Guardian Priority Matrix

| Priority | Lane | Why It Matters | First Bounded Move |
|---|---|---|---|
| P0 | Transport/session continuity diagnostics | If `ingest -> seek` can silently lose graph state in a host binding, agents lose trust immediately. | Add a diagnostic/smoke path that distinguishes empty graph, wrong session, stale snapshot, restricted host surface, and query failure. |
| P0 | Agent-first smoke harness | The product must be validated like an agent uses it, not only through unit tests. | Stdio and HTTP harness paths added; document host-binding gaps next. |
| P1 | Tasknotes to issue-grade backlog | The repo already has a strong friction-capture protocol, but open notes need prioritization and conversion into shippable slices. | Turn `docs/AGENT-TASKNOTES.md` open notes into a ranked implementation matrix. |
| P1 | Public story parity | README is strong, but localized READMEs/wiki/demo should not drift from the new positioning. | Check current public surfaces for stale tool counts, old positioning, or weaker value framing. |
| P1 | `m1nd doctor` / health surface | Agents need fast answers to "is my graph active, stale, empty, or bound to another session?" | Add or strengthen a health-style call with graph counts, active roots, transport, runtime dir, and recovery hints. |
| P2 | Docs/wiki alignment | m1nd is powerful but broad; docs need to route users by task instead of making them memorize tools. | Audit wiki quickstart/tool-matrix against live `tools/list` and `help`. |
| P2 | Continuity UX | The highest user value is preserved orientation across long work. | Strengthen `trail_resume`, `boot_memory`, and perspective docs through real smokes. |
| P2 | Apply/edit trust boundary | Write tools are high value and high risk. | Verify `edit_preview`, `edit_commit`, and `apply_batch` docs/tests against current runtime behavior. |
| P3 | Website/demo conversion | m1nd needs to be understood emotionally and operationally. | Make the demo prove one journey: cold repo -> m1nd orientation -> safer edit. |

## Next Recommended Cut

Do not start with broad feature expansion.

Start with the smallest cut that increases agent trust:

1. create a repo-local smoke script for stdio `ingest -> seek -> help`
2. add a failing/diagnostic case for graph-state loss or empty candidate scans
3. make the recovery hint explicit when retrieval sees zero candidates after
   a successful ingest in the same session
4. keep `docs/AGENT-TASKNOTES.md` as the living capture surface

The north is simple: m1nd should make the first ten minutes of any serious
agent session feel grounded, resumable, and verifiable.
