# GENESIS — Ingest Consumers Spec (v2, post-verdict)

**Status: DRAFT v2 — rewritten under an askGOD `CHANGE` verdict (alta, 2026-07-25); awaiting
the confirmation verdict, then owner ratification.** Provenance: the checkpoint-32 panel
(P1→P2→P3, the 12 requirements), the 2026-07-25 adoption lab, and the first verdict's own
workspace reading, whose refutations are receipts below. Nothing here authorizes code.

**Provenance honesty (verdict RC-8):** `graph.ingest.refresh_declared_root` (SPEC-1) is a
**NEW item, not part of the cp32-ratified P1→P2→P3 order**. P1 (#403, commit `476f73be`)
merged to main on 2026-07-24 (git is the tiebreaker — the first draft copied the verdict's
date instead of checking, the exact cp33 anti-pattern). Owner ratification for this document covers the **insertion of SPEC-1 into the
queue**, not merely its floor. SPEC-2 (birth) is the cp32 P2/P3 line itself.

## 0 · Receipts

From the adoption lab (measured, serve-mode, authenticated REST, 17,408-node runtime copy):

- **R-A — CORRECTED BY THE FIELD, 2026-07-27.** The lab measured that adoption works, and the
  lab was right about what it measured: a binary loads a populated runtime whole. **What the lab
  did not have was a stale checkpoint.** In the field, the brain actor's `start` restores
  checkpoint `CURRENT` and rebuilds its whole session from disk, and adoption ran *before any
  actor existed* — so on a runtime whose `CURRENT` predated the adoption, **the actor reverted
  it on the same boot**. The owner's own repo loaded 5540 nodes and served **0**, every boot,
  for five days, while the journal recorded `status: "adopted"`. Read together with the caveat
  below, that is the worst combination available: the rescue was **spent without ever having
  worked**, and its own one-time guard forbade retrying it.
  **Fixed:** adoption now runs *inside* the actor boundary and commits through the checkpoint,
  so `CURRENT` is never older than the files it describes; the journal is written only after the
  commit is acknowledged; and a journal beside an EMPTY brain — the exact footprint of a
  reverted adoption — is re-adoptable rather than spent. An affected installation recovers on
  its next boot with no operator step.
  The verdict's caveat still holds and is unchanged by this: **adoption is ONE-TIME and
  journaled** — it is a birth path, not a recovery net. A destructive write *after* a
  successful first boot still will not rebuild; recovery is the `.bak-<ts>` plus a human hand.
  What changed is only that an adoption which never landed no longer counts as one that did.
  **Method note worth more than the fix:** a lab can only measure the states it reproduces. This
  one was structurally unable to see the defect, and nothing in 1458 green tests could either —
  it needed a *second boot* on a runtime carrying prior state. See `docs/PATHOS.md`, the
  lifecycle-proof front.
- **R-B** — the daily loop (`north`/`seek`/`memorize`) answers with zero authority walls.
- **R-C (corrected per verdict RC-9)** — every ingest form is refused, but the mechanism is
  NOT a missing A2 consumer: `graph.ingest.merge_existing` **already has a typed A2 consumer
  declared** (`action_consumers.rs:391-408`). The measured wall is twofold: the generic
  dispatch gate is keyed **by floor** (`ScopedGrantA2 => false`, no exception list,
  `server.rs:5761-5771`) and the typed port **requires an authority lease**
  (`external_mutation_service.rs:1116-1135`) whose issuer is the HUMAN_GATED, frozen
  authority runtime. The graph is frozen because the lease plane is dormant.
- **R-D** — the hijack class is real: two bound-brain replacements in 24h via plain
  `ingest {path}` on the deployed 1.4.x owner.
- **R-E** — the current-main read-path perf regression is a separate front, in flight.

From the first verdict's own reading (its evidence, now this spec's constraints):

- **R-F** — `covers_root` is **prefix, not identity**: `path_starts_with_loosely` over
  `workspace_root`+`ingest_roots` (`session.rs:1287-1299`), whose first layer is raw
  `Path::starts_with` (no `..` resolution, no symlink resolution) and whose string fallback
  accepts nonexistent paths textually (`session.rs:1474-1500`).
- **R-G** — the persist shrink guard is **fail-open by written design**
  (`snapshot.rs:556-597`): it backs up and writes anyway. "Root set unchanged" does not
  imply "graph intact".
- **R-H** — `caller_root` is **client-resolved**: the attach bridge reads
  `M1ND_WORKSPACE_ROOT`/`PROJECT_ROOT`/`WORKSPACE_ROOT`/`REPO_ROOT`/cwd
  (`attach_client.rs:469-489`) and the owner stores the header raw (`mcp_http.rs:2603-2639`).
- **R-I** — the policy gate is **pure and pre-brain by design**
  (`enforce_generic_action_policy(tool, params)`, `server.rs:5785-5796`): no session, no
  `covers_root` at decision time. The one "trusted route fact" precedent
  (`ingest_changes_roots`) is dead plumbing — both production call sites pass
  `TrustedMcpRouteFacts::default()` (`server.rs:4205, 5816`).
- **R-J** — the binary holds **two competing canonical-root notions**:
  `ProjectBrainRegistry::canonical_key` (resolves symlinks and the `/tmp`→`/private/tmp`
  alias, `project_brains.rs:1141-1147`) vs the loose prefix logic of `covers_root`.
- **R-K** — the REST seam already contains a precedent of `caller_root` being overwritten
  under an explicit `?brain=` selector (`http_server.rs:4087-4091`).

## 1 · SPEC-1 — `graph.ingest.refresh_declared_root` (the freshness door, rebuilt)

### 1.1 Classification stays pure (verdict RC-4/RC-5)

Refresh is its **own action, classified purely from parameters**: a distinct `mode:"refresh"`
(or dedicated verb) so `classify_ingest` maps it from `(tool, params)` alone — the gate's
pure/pre-brain invariant (R-I) is untouched. This spec does **not** use trusted route facts,
because that plumbing is dead (R-I); any future need for one must first wire it and prove by
test that `default()` is no longer passed in production.

**The action's floor, named for ratification: `ScopedGrantA2`, admitted A2-LOCAL** (no
lease plane) through the action-keyed allowlist below. The lowering argument, restored from
v1 and now standing on the exact-root predicate instead of the refuted prefix one: an action
that structurally cannot cross brains does not lower the floor of any action that can —
and §6 asks the owner to ratify precisely this named floor, not an unnamed one.

The generic-dispatch admission is opened **keyed BY ACTION, never by floor**: an explicit
allowlist containing exactly `graph.ingest.refresh_declared_root`, with regression tests
proving `source.edit.commit` and `graph.ingest.merge_existing` keep today's refusal bytes
(verdict RC-4). Admission at dispatch only admits the *category*; every authority-relevant
fact is enforced **inside the typed handler, after brain resolution, fail-closed**.

### 1.2 The exact-root predicate (verdict RC-1 — `covers_root` is NOT reused)

A **new, authority-exclusive predicate** — `is_exact_declared_root(caller)` — admits a
refresh only when `canonical_key(caller)` is **EQUAL** to `canonical_key(r)` for some
declared root `r` (workspace root or a registered ingest root). Equality, never prefix;
`canonical_key` (R-J), never textual fallback. `covers_root` remains the *reception*
predicate (prefix, by design, for `session.rs:1197` and the skeleton-write `brainless_root`
gate at `server.rs:5940-5960`) and is explicitly not touched by this spec.

- SPEC-1a: a caller at `<root>/m1nd-ui` refuses `refresh_root_not_exact` — the verdict's
  own kill-shot case, now the first test.
- SPEC-1b: `caller_root` is canonicalized **at ingress** — every seam where a value
  becomes `session.caller_root` (`mcp_http.rs:2636`, and the same class at `:448-449` and
  `:614`; the REST session constructors) routes through canonicalization. **The primitive
  alone does not suffice:** `canonical_key` itself falls back to the raw string when a path
  does not resolve (`project_brains.rs:1146-1147`), so the ingress/predicate must REFUSE
  unresolvable paths explicitly (`refresh_root_unresolvable`) — two textually-equal
  nonexistent paths must never match. The external-mutation path already canonicalizes at
  its own seam (`mcp_http.rs:976`) — reuse that precedent. Tests, run against BOTH
  transports (MCP and REST): `<root>/../out` refuses; a symlink inside the root pointing out
  refuses; `/tmp` vs `/private/tmp` reach the same decision; a nonexistent path refuses
  rather than string-matching. **SPEC-1 cannot be implemented before this exists**
  (verdict RC-2).
- SPEC-1c: single-flight per canonical root — a second refresh in flight refuses
  `refresh_in_flight` (TOCTOU, cp32 req).
- SPEC-1d: refresh **never changes the root set**; a scan that would, aborts
  `refresh_would_change_roots` with nothing mutated.
- SPEC-1e: **`refresh_would_shrink_graph` — HARD, fail-closed** (verdict RC-6): the refresh
  is computed as a candidate first; if the candidate's node count falls below a declared
  floor fraction of the live graph (proposed default: 60%, ratified in §6), the refresh
  REFUSES, names both
  counts, and mutates nothing. This is deliberate armor the persist layer does not provide
  (R-G is fail-open and stays as-is at its layer). The R-D damage signature — a narrow scan
  replacing a wide graph — dies here even for legitimate callers.
- SPEC-1f: journaled receipt-or-refusal via the existing `graph_ingest_a2` machinery
  (payload schema, digest, journal, candidate artifact, recovery kind); crash mid-refresh
  recovers to old-or-new, never mixed.
- SPEC-1g: MCP and REST route through ONE admission seam; **an explicit `?brain=` selector
  NEVER satisfies the exact-root predicate**, and `ingest`/refresh never joins the
  `skeleton_write_needs_root_gate` caller-root overwrite list (R-K; verdict RC-7). Test:
  refresh under an explicit selector refuses byte-identically to MCP.

### 1.3 What this closes, honestly (verdict RC-3)

`caller_root` is resolved by the CLIENT and travels as a header (R-H). With ingress
canonicalization (SPEC-1b) the textual tricks die, but a same-UID process that sets
`PROJECT_ROOT` to a root it does not legitimately inhabit can still present as that root.
Following the codebase's own honest phrasing precedent (`system_blocks_handlers.rs:492-494`):
**SPEC-1 closes the REFLEX vector — an agent acting from habit or misconfiguration — and
does not defend against a malicious same-UID local process.** That defense is the lease
plane (P3-full), not this door. Stated here so the spec never claims a proof standard its
own transport cannot meet — the exact contradiction the first verdict caught.

## 2 · SPEC-2 — `brain.bootstrap.birth` (unchanged in substance, confirmed by the verdict)

`PositiveSovereign`, admission by **owner-stamped human origin**: a server-side CLOSED
allowlist (`human-ui`, `human-touchid`, new `human-cli` minted only by the P2 ceremony
`m1nd init --birth <root>`), stamped by the owner's own surface — a client-claimed origin
string grants nothing (the ratify counter-precedent, `system_blocks_handlers.rs:435`).
Guards as v1: empty-destination defined on disk; overlap classes refuse; **no
`allow_overlap` below sovereign**; journaled prepare→commit with whole-or-none crash
recovery; single-flight; the bound dev graph is never touched. `m1nd init` today is only
`installSkills` — the PRD's "built" claim is corrected in the PR that lands P2.

## 3 · Non-goals, including the accepted risk (verdict RC-10)

- **Accepted and declared:** a legitimate holder of a declared root can still degrade that
  root's brain for every other agent of the same root — refresh authenticates the *root
  relationship*, not intent. The blast is bounded by SPEC-1e (shrink refusal), SPEC-1d
  (root-set invariance), SPEC-1f (journal receipt naming the writer) — and full defense is
  the lease plane, out of scope here. A written accepted risk is not an unseen risk.
- The G2/G3 authority runtime stays DORMANT; no autonomy activation; no `learn` change; no
  perf work (R-E, separate front); no Windows path-canon work.
- Migration stays a boot-time fact with no verb (R-A, one-time caveat included).

## 4 · Prose sweep — same-PR surfaces (verdict RC-11)

The v1.5.0 seven-surface incident is precedent: withdrawing or adding a capability sweeps
the prose in the same PR. Surfaces to update when SPEC-1/SPEC-2 land: `AGENTS.md` (write
laws + version-skew note), local build notes, `README.md`, `docs/wiki/src/tutorials/
quickstart.md`, `docs/wiki/src/api-reference/lifecycle.md`, `docs/wiki/src/changelog.md`,
`skills/m1nd-operator/SKILL.md`, `skills/m1nd-universal-agent-pack.md`, and
`tests/test_agent_surface_bootstrap_honesty.py` (teach the new verbs only inside their
version-gated truth; the guard list grows with any newly withdrawn teaching).

## 5 · Acceptance (RED battery, born before code — updated for v2)

1. Foreign-root replace → today's refusal bytes (hijack stays dead).
2. **Descendant-root refresh (`<root>/m1nd-ui`) → `refresh_root_not_exact`** — the verdict's
   kill-shot, now the first RED case.
3. `<root>/../out`, symlink-out, `/tmp`-alias, nonexistent-path callers → refused at
   ingress canonicalization (SPEC-1b), never string-matched.
4. Exact-root refresh on current-main → succeeds, absorbs a new commit, root set unchanged,
   journal receipt present. (RED today: R-C refusal.)
5. Narrow-scan refresh (candidate < 60% of live nodes) → `refresh_would_shrink_graph`,
   graph untouched (RED today: nothing refuses this — R-G).
6. Refresh under REST `?brain=` selector → refused byte-identically to MCP (SPEC-1g).
7. Birth without owner-stamped origin → refused; with client-claimed origin → refused
   identically; via P2 ceremony on empty destination → brain exists, routes by caller root,
   dev graph untouched; concurrent second birth refuses.
8. Kill -9 mid-birth → completes-or-removes whole; mid-refresh → old-or-new, never mixed.
9. Regression: `source.edit.commit` and `graph.ingest.merge_existing` refusal bytes
   unchanged after the action-keyed dispatch allowlist lands (verdict RC-4).

## 6 · Gates

Verdict history: v1 → `CHANGE` (11 required changes) → v2 → `CHANGE` (10/11 satisfied, spine
confirmed, four one-line fixes) → v3 (this text) applies all four. Per the anti-ceremony
ceiling (1 review + 1 independent re-review per source delta; a third internal pass over the
same artifact is prohibited), **the next gate is the owner**, who ratifies four named things:

1. SPEC-1's **insertion into the queue** (it is new, outside cp32's P1→P2→P3);
2. SPEC-1's floor: **`ScopedGrantA2`, A2-local**, via the action-keyed allowlist;
3. the shrink-floor default (**60%**);
4. SPEC-2's **`human-cli`** allowlist entry (minted only by `m1nd init --birth`).

Then implementation, battery-first, in the proof-grown rite.
