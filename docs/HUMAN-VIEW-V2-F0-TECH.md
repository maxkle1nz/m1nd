# Human View v2 — F0 technical amendment

**The design decisions that must be signed before the first Rust struct.**

> Status: RATIFIED for F0a · 2026-07-07 · distilled from the pre-F0 oracle verdict (CHANGE → resolutions adopted in full)
> Sisters: `HUMAN-VIEW-V2-PRD.md` (product contract) · `HUMAN-VIEW-V2-SCREENS.md` · `HUMAN-VIEW-V2-UML.md`.
> This amendment BINDS the F0a implementation: anything here overrides looser wording in the PRD. Without it, F0a would be making architecture decisions inside code instead of implementing a ratified contract.

---

## 1. Store locus

- **Live state**: a **sidecar store per project brain** (alongside the brain's other runtime artifacts) — never inside the graph snapshot, never in the medulla.
- **Ratification/import/export**: a **versioned seed file in the repo** at `docs/system-blocks/<repo>.seed.v0.json`. The seed is the reviewable, PR-mergeable form; the sidecar is the living form. Import = seed → sidecar; export = sidecar → seed.
- The seed path lives under `docs/system-blocks/` and is **operator/internal surface** — never quoted as public product copy.

## 2. Membership is PATH-FIRST (the NodeId proof)

Verified in the engine: `NodeId` is an internal sequential `u32` **reassigned on every snapshot load** (`m1nd-core/src/snapshot.rs` load path calls `add_node` again). File `external_id`s (`file::<relpath>`) are stable while the path exists; symbol external_ids shift on rename/move/homonym reordering. Therefore:

- The **ratified boundary is paths/globs, repo-relative, with a role** — never node ids of any kind.
- `external_id`s are a **resolution cache**, recomputed per scan/render, stamped with a `resolution_hash`.
- **Rename/move = drift** (reopens scoped ratification) — never a silent remap.
- Public schemas say `external_id` when naming graph identity; the internal `NodeId` never leaves the engine.

```json
"membership": [
  { "path": "m1nd-core/src/graph.rs",      "role": "primary" },
  { "path": "m1nd-core/src/snapshot*.rs",  "role": "primary" },
  { "path": "m1nd-core/src/**/*_test.rs",  "role": "test", "optional": true }
],
"resolved": { "external_ids": [], "resolution_hash": null, "resolved_at_scan": null }
```

**Membership roles** (required on every entry): `primary | shared | generated | test | docs | external_socket`. Roles disambiguate receipts, residue and seams; a file may be `primary` in one block and `shared` in another.

## 3. Receipt evidence schema (anti-poison)

A receipt without hard evidence is not a receipt. `receipt_import` REJECTS payloads missing any required field:

```json
{
  "type": "test | structural | runtime | review | handoff | spec",
  "emitter": { "kind": "ci | runnerd | verb | owner", "id": "<who>" },
  "scope": { "block_id": "…", "boundary_version": 1, "contract_version": 1, "resolution_hash": "…" },
  "evidence": {
    "command": "cargo test -p m1nd-core",
    "cwd": "<repo-relative>",
    "exit_status": 0,
    "started_at": "<iso8601>", "ended_at": "<iso8601>",
    "artifact_hash": "<sha256 of the raw artifact>",
    "stdout_excerpt": "<bounded>",
    "evidence_refs": ["<artifact path or CI run url>"]
  },
  "validity": { "expires_on": null, "stales_on": ["member_change"] }
}
```

Execution timestamps are captured evidence, not caller-authored decoration.
At import, `started_at` MUST be earlier than `ended_at`; neither timestamp may
be later than the import instant; and the captured window MUST NOT exceed 24
hours. A refusal names the incoherent field and leaves the store untouched.
Receipt landing preserves the runnerd-composed timestamp bytes exactly — it
never rewrites or re-dates a candidate to make it pass.

Receipts bind to `(block_id, boundary_version, contract_version, resolution_hash)` — never counted for a version they did not see (PRD §3.1).

**MVP emitters (honest map):**
| Type | Emitter day-1 | Note |
|---|---|---|
| `structural` | `xray_paint` commit/ledger | real today |
| `handoff` | `soul_check` | real today |
| `spec` | ratifying the contract | born in F0a |
| `test` | **CI artifact or runnerd** | NO emitter today — the schema above is the import path |
| `review` | `debrief` **with evidence attached** | debrief without evidence stays `outcome_unverified`, never a receipt |
| `runtime` | **optional-neutral until a read API exists** | see §6 |

## 4. The receipt contract is SIGNED at ratification

The ratification screen becomes **"Names, Boundaries & Receipt Contract"** — three signatures in one session. Defaults come from block templates (`rust_core`, `ui`, `docs`, `installer`, `runtime_service`) and only pre-fill; the owner accepts/edits/waives. Editing required/optional/waived bumps `contract_version` and stales prior receipts. Scanned (brownfield) and Planned blocks share the same `ReceiptContract` shape:

```json
"receipt_contract": {
  "version": 1,
  "required": [
    { "type": "spec",       "stales_on": ["contract_change"] },
    { "type": "structural", "stales_on": ["member_change", "repaint"] },
    { "type": "test",       "stales_on": ["member_change"] }
  ],
  "optional": [ { "type": "runtime" }, { "type": "review" }, { "type": "handoff" } ],
  "waived": [],
  "declared_by": "owner", "declared_at": "<iso8601>"
}
```

## 5. Spawn executes in a COMPANION, never the owner

Decision grounded in the owner's own security posture (it refuses non-loopback bind without auth precisely because it exposes mutation; adding a process executor would widen that to unauthenticated local execution):

- **`m1nd-runnerd`** — a separate local companion, loopback-only, local secret/socket, registered with the owner by capability.
- Flow: UI → owner (policy gate, packet, mission id) → runnerd → **isolated worktree** → executes the configured agent → events back (`mission_started`, `heartbeat`, `output_landed`, `debrief_requested`, `receipt_import`).
- Owner absent a runnerd: spawn controls render disabled; clipboard/direct remain.
- **F0a models only the types** (`RunnerRef`, `RunnerCapability`, `RunnerWorkspaceTruth`, `MissionPolicy`, `Pin`, `MissionPacket`) — no process execution before F2.5.

## 6. Runtime signal: optional-neutral until a read API exists

The engine's `runtime_overlay` **ingests spans, applies activation boosts and persists** — it is a mutating verb, not a render source. Until a separate persisted read artifact/API exists (`runtime_overlay_snapshot` or equivalent), the blue axis renders **"no runtime signal"** everywhere and the `runtime` receipt type is optional-neutral. The render path NEVER calls the mutating verb.

## 7. Layout persistence

Stable position is a product law (same block → same place). The sidecar stores per block:

```json
"layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 }
```

First render seeds positions deterministically (`algorithm_seed`); the owner may pin (`locked`); layout versions with the skeleton.

## 8. F0a API surface (named now, implemented then)

`system_blocks_snapshot` (read) · `system_blocks_seed_import` · `system_blocks_ratify` · `skeleton_candidate` (F0c) · `receipt_import` · `receipt_recompute` (expiry/staleness pass). A present snapshot includes `skeleton_coherence`: `{ "status": "ok" }` when the serving project-root slug matches the `sk_<slug>_*` skeleton and every `sb_<slug>_*` block, or `{ "status": "mismatch", "expected_slug": "…", "found_slug": "…" }` when they diverge. This is a vital sign only: it never rejects a read or mutation, and an absent store emits no signal. All mutations are OCC-checked per PRD §3.1; archive/delete exists in the store **from F0a** (UML §9.11) — the UI may not ship a delete button before the store can refuse an unsafe one.

**Live, not a photograph.** The served Build Map subscribes to the `graph_changed` SSE class (`useLiveRefresh`) and re-reads `system_blocks_snapshot` whenever a store / skeleton / X-RAY write lands — from an agent, the CLI or another viewer — debounced ~500 ms and **scoped to the brain in view** (§4A.9.6: a mutation on another `brain_root` refetches nothing). The classifier that decides which writes emit `graph_changed` is `mcp_http::GRAPH_MUTATION_TOOLS` — a curated subset of the read-only deny-list, extended to cover the SystemBlock / skeleton / X-RAY verbs above (`system_blocks_ratify` / `_reconcile` / `_seed_import` / `_archive` / `_delete`, `skeleton_candidate`, `receipt_import`, `xray_paint` / `xray_retag`). The re-read is stale-while-revalidate — it opens as `refreshing` and never unmounts the map — so the human's selection, scroll and open block panel survive a live update. Only the map surface subscribes (never the App-level front-door read), so a write triggers exactly one refetch.

## 9. Packet hygiene

- Screenshot-in-packet defaults **OFF**; enabling it warns and applies redaction before any spawn.
- Clipboard mode must be truly read-only: it uses a **read-only compositor** (no delegate registry write) or the UI declares the write explicitly. (The current `delegate` verb writes a registry record — that is spawn/direct behavior, not clipboard.)
- External sockets in seeds/docs use **classes/aliases** (`payments_provider`, `mail_provider`) — never real hostnames, env values or credentials.

## 10. The seed schema (`m1nd-system-block-seed-v0`)

```json
{
  "schema": "m1nd-system-block-seed-v0",
  "repo": { "repo_id": "<name>", "root": ".", "source_commit": "<git-sha>" },
  "skeleton": {
    "skeleton_id": "sk_<repo>_seed_<yyyy_mm>", "version": 1, "state": "ratified",
    "ratification": { "method": "pr_merge", "ratifier": "owner", "ratified_at": "<iso8601>", "commit": "<merge-sha>" }
  },
  "blocks": [ { "block_id": "sb_…", "name": "…", "purpose": "…", "kind": "scanned|planned",
    "state": "candidate|ratified", "boundary_version": 1, "contract_version": 1,
    "membership_source": "ratified|proposed|manual", "membership": [ … ],
    "sockets": { "inputs": [], "outputs": [], "external": [] },
    "receipt_contract": { … }, "receipts": [], "layout": { … }, "unmapped_residue": [] } ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}
```

F0a implements loader + validator + roundtrip (import→export is byte-stable modulo resolution cache); F0b adds the real seed plus fixtures proving parse, stable block ids, and a no-leak check (no absolute paths, no private labels).

## 11. The candidate seed for the m1nd repo itself (F0b input — awaiting owner ratification)

Twelve blocks, proposed from the real repo structure (many-to-many with roles expected — e.g. `layer_handlers.rs` primary in Retrieval, shared with Evidence):

1. **Core Graph Kernel** — `m1nd-core` graph/types/query/activation/topology/seed/builder/resonance
2. **Core Evidence & Risk Engines** — calibration/trust/tremor/temporal/git_history/runtime_overlay/flow/taint/twins/refactor/layer/counterfactual/epidemic/snapshot/semantic/embed
3. **Ingest & Document Canonicalization** — `m1nd-ingest/**`
4. **Served Owner, Transport & Brain Routing** — main/cli/server/http_server/mcp_http/attach_client/session/instance_registry/project_brains/protocol
5. **Retrieval, Search & Analysis Verbs** — tools/search/layer_handlers/audit/report/help/result_shaping/trust_envelope/perspective
6. **Memory, Medulla, Soul, Mailbox & Delegation** — boot_memory/light_author/promote/soul/delegation/mailbox/mission/daemon/medulla_migration
7. **Editing, Safety, X-RAY & Locks** — surgical/xray/lock_handlers/auto_ingest/engine_ops/universal_docs
8. **Human UI Shell, Hall & Living Tree** — App/components/tree+hall libs/index.css
9. **UI API, State, Fixtures & Tests** — api/hooks/stores/fixtures/tests
10. **Distribution, Host Packs & Installer** — npm/skills/package.json/server.json/i18n
11. **Docs & Product Contracts** — docs/README/CONTRIBUTING/EXAMPLES/SECURITY/AGENTS
12. **CI, Scripts, Demo & OpenClaw** — .github/scripts/tests/m1nd-demo/m1nd-openclaw/root configs

## 12. Code pre-work (lands with this amendment's burst, gate-verified)

1. **`m1nd-ui/src/api/toolTypes.ts`**: `TrustEnvelope.verdict` gains `'unprovable'` (a `TrustVerdict` union type) + its consumers (`verdictLabel`, `VerdictChip` meta, icon registry) handle the fourth state + a regression test pins the union against the Rust protocol.
2. **Read-only deny list**: `runtime_overlay` added to `READ_ONLY_DENIED_TOOLS` — it mutates activation state and persists; a read-only attach must not reach it.
3. **Port wording**: docs say the served owner lives at `:1338` while the CLI default is `1337` — standardize the wording (the doc states the operator's configured port explicitly) before any runnerd work hard-codes either.
