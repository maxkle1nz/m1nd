# Context Economy — the graph as the agent's context service

> **Status: BRAINSTORM NOTE, owner-ratified direction (2026-07-25). NOT a frozen PRD.**
> The build order below was ratified by the owner; the PRD, when this graduates, goes through
> the house PRD rite. Nothing here authorizes implementation beyond the Layer-2 spike.

## The thesis

Agents do not need to *read code better*. They need to **stop re-reading what did not change,
stop receiving what they did not ask for, and stop writing whole files to express small
intents.** Every gain below is one of those three, served from the graph m1nd already keeps.

This note exists because the same idea arrived twice in one day from two independent
directions — a sibling agent-first language project designing enumerable, content-addressed
surfaces *into a new language*, and this repo's own live measurements of context waste — and
the two converge on one architecture. m1nd's role: retrofit those guarantees onto the
languages that already exist.

## Measured evidence (one day, this repo, live)

| measurement | value | where |
|---|---|---|
| binding fingerprint served in EVERY `north` | 380 roots ≈ 25,907 B ≈ **6,500 tokens** | fixed by head-truncation (#414) |
| one `surgical_context` at radius 2 | **2.09M chars** | bounded by #419 (draft) |
| whole-file write vs graph write (`transplant`, 714-line move) | 12,235 → 48 output tokens (**256×**) | #401 |
| verbs never called in 6 weeks of real use | **104 of 133** | traffic study 2026-07-24 |
| served brain replaced by a foreign session's plain ingest | **3× in one day** | 2026-07-24 incidents |
| independent 2026 result: curated repo context for agents | −28.6% runtime, −16.6% output tokens | Probe-and-Refine (arXiv 2606.20512) |

The last row is the external confirmation: context curation is not a vibe, it measures.

## The build order (ratified): floor → beam → room

**Floor — isolation (genesis P2/P3, already the named open front).** One project, one brain,
no foreign replace. Selective retrieval that can silently serve another repo's graph is worse
than grep — it is *efficient lying*. The 3×-in-one-day replace incident is the floor's proof
of necessity, not a separate problem.

**Beam — stable node identity.** Today a node id embeds its file path
(`file::<path>::<kind>::<name>`), so a re-ingest after a file move mints a NEW identity and
orphans everything anchored to the old one (the transplant wave's declared follow-up: paint
tags orphan on move). Everything interesting — receipts, caches, deltas, aging memory —
anchors to identity; unstable identity makes them sandcastles.

Design sketch (SCIP-informed):

- **Identity = logical descriptor, not file path.** `crate::module::item` hierarchy (for
  Rust: the item's canonical path; analogous per language), SCIP-style
  (`<package> <descriptor>` where the descriptor encodes the nesting). File path becomes an
  *attribute* of the node, never its name. A file move then preserves identity by
  construction — the exact case transplant needs.
- **Content hash = the version facet.** The node carries `sha256(content)` (real since #410)
  beside the identity. Identity answers "same thing?"; hash answers "same bytes?". The pair
  is the complete cache key.
- **Locals stay local.** Closures, statics, generated items without a stable logical name get
  scoped local ids (SCIP's `local N` pattern) and are never promised stable across ingests —
  declared, not silent.
- **Prior art, verified 2026-07-25:** SCIP (scip-code/scip, Apache-2.0, alive) — the
  descriptor grammar donor. github/stack-graphs (Apache-2.0) — **archived upstream 2025-09**;
  its lesson (name binding computed per-file, incrementally) survives as reading, not as a
  dependency. salsa (salsa-rs, Apache-2.0, alive) — the incremental-recomputation donor if
  Layer-3 deltas ever need memoization. Unison — concept donor for content addressing
  (license NOASSERTION: concepts only, no code).

**Room — selective retrieval.** Evolve `batch_view` (reuse-first; the verb exists and already
fetches N nodes in one call) with three primitives and NOT more:

1. `select`: node identities and relational selectors (`callers_of(X)`, `deps_of(X, depth)`)
2. `lod` per class: `full_body` | `interface` | `callsite` (level-of-detail masks)
3. `budget`: token ceiling, **default-on**, filled by rank, truncation always declared
   (`omitted: N, by: budget` — the #414/#419 honesty pattern, now law for every packet)

The failed default is the enemy: #419 measured 2.09M chars precisely because bounding was
opt-in. In this design unbounded is the opt-in.

## Beyond the room — what the first user actually needs

Ranked by economic leverage, each anchored to machinery that exists today:

1. **Context receipts + `diff_since(generation)`.** Every slice ships
   `(identity, sha256, graph_generation)`. Next turn, the agent revalidates its cached slice
   for ~1 token (`am_i_stale` exists) or asks *only for what changed since generation N*.
   Turn cost falls from O(context) to O(delta). In long sessions ~90% of re-reads are
   unchanged; this is the compounding win, and it is exactly the cache-efficiency thesis of
   TokenPilot (arXiv 2606.17016) applied to a code graph.
2. **The write-verb family.** `transplant` proved the graph writes (256×). Extend the family:
   `rename_symbol`, `change_signature` (rewrites every callsite), `extract_fn`, `inline_fn` —
   each with transplant's honesty receipt (`refs_unresolved`, `state_left_behind`). The agent
   stops emitting diffs for mechanical refactors entirely.
3. **Session working set + trail resurrection.** `north` already tracks `coverage.visited`;
   `trail_save`/`trail_resume` already exist (and sit in the 104 never-called verbs). Auto-save
   the working set (identities + hashes, not bodies) as a trail; a restarted session resumes
   warm in one call. Lived need: the host process died three times in one day and every
   rebirth was cold.
4. **Closure-scoped verification.** `impact` knows the reverse closure → `verify_closure(X)`
   runs only the tests that touch it. Cheaper turns-to-green for any language — the same
   metric the sibling language project bakes into its kill gate.
5. **Enumerable absence.** A slice states what does NOT exist as a dated claim
   ("callers: exactly these 3, generation N, enumeration complete"). Kills hallucinated
   surface — the retrofit of an enumerable-API guarantee onto languages that will never have
   one natively.

## Kill gates (falsifiable, pr00f-style)

- **Beam spike oracle:** ingest → move a file → re-ingest → a claim/tag anchored to a moved
  item MUST still resolve (RED today, by construction). Plus idempotence: two ingests of an
  unchanged tree yield byte-identical identity sets.
- **Room gate:** on a fixed task battery, bounded selective retrieval must beat today's
  m1nd-trained loop on tokens-to-green and turns-to-green. If it does not measure, the DSL
  layer dies (the sibling project's kill-gate discipline, applied to ourselves).
- **No silent truncation, ever:** any packet that omits carries `omitted` + count + pointer.
  Inherited as law from #414.

## Non-goals

- No new query language. Three primitives; `select` strings stay enumerable.
- No replacement of files for humans — projections remain.
- Nothing here touches the sovereign write plane (M1ND-10 authority floors unchanged).

## Next rite

1. Layer-2 (beam) spike in an isolated lab with the oracle above born RED — before any PRD.
2. PRD distilled from the spike through the house PRD rite, then `sp3c` for executors.
3. Layers land floor-first; the room only after the beam's oracle is green.
