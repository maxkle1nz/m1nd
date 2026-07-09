# HUMAN VIEW v2 — F0c technical amendment: `skeleton_candidate` — any repo gets a map

Status: **RATIFIED** — oracle-confronted (CHANGE → 11 mandatory objections applied), ratified by the owner 2026-07-09. This is the slice that turns the product from "a map where we hand-wrote the seed" into "point the engine at ANY repo and it proposes the map; the human ratifies". The pipeline is signed (PRD §6 brownfield; the Ratification law: auto-clustering only ever produces a CANDIDATE). This amendment stitches that pipeline onto the organs that exist: Louvain communities (m1nd-core/topology.rs), the seed/store contract (F0a), reconcile fingerprints (F0a-3), the candidate UI dress (F1), and the optional naming-runner (F2.5c).

## 1. The verb — `skeleton_candidate` (WRITE, deny-listed on read-only)

Input: `{agent_id, expected_store_version | null, review_limit?, naming?: "auto" | "heuristic"}`.
- Output: a full candidate seed (`skeleton.state="candidate"`, every block `state="candidate"`, `membership_source="proposed"`) **written through its OWN transaction** (objection 2 — never the seed-import path, whose `force` resets `store_version` and whose semantics are operator-import, not scan): store absent + `expected: null` → creates the candidate store at v1; store whose skeleton is still `candidate` + OCC key → replaces the candidate wholesale (§4b); store whose skeleton is `ratified` + OCC key → writes ONLY `store.candidate_revision` (§4a), never touching live blocks. Any other combination refuses honestly.
- **Contract fit (objection 1):** the store/blocks gain versioned OPTIONAL fields — `SystemBlock.candidate_meta: Option<CandidateMeta>` (the §3 scores + `named_by`) and `SystemBlockStore.candidate_revision: Option<SeedFile>` — both `serde(default, skip_serializing_if)` so every existing seed/store parses byte-stable and `deny_unknown_fields` stays intact.

## 2. The clustering — the collapse is specified, not assumed

- **(2a) From node assignments to file claims (objection 4):** Louvain returns `assignments: Vec<CommunityId>` per NodeId. The collapse ladder: a FILE node's own assignment wins for its path; symbol/child nodes aggregate to their file by weighted majority; a file whose symbols tie between communities is claimed by neither — it enters the candidate as `role:"shared"` on both blocks AND is listed under `multi_owner_seams` in the report (the PRD §6 seam surface). Files with no graph node attach by directory only.
- **(2b) Communities × directories:** a community spanning many directories splits by directory; sibling directories inside one community merge. Membership emits directory globs at ≥90% claim, exact paths otherwise.
- **(2c) Degeneration fallbacks are declared per repo class (objection 6):** single-directory code repo (everything under `src/`) → community-first, then sub-directory/file clusters within it; docs/no-edge repo → directory+extension clustering with `graph_cohesion` honestly absent (see §3); tiny repo (< ~30 files) → one block per top-level directory, no Louvain (noise floor declared).
- **(2d) Sockets bind to STABLE ids (objection 11):** internal candidate sockets carry `to: <candidate block_id>` (never display names — names change during ratification; the UI resolves labels). External sockets are NOT guessed; where an obvious class exists from manifest deps it may propose `class` aliases ONLY (F0-TECH §9), else empty.

## 3. Naming & confidence — provisional labels never masquerade as final

- **(3a) `naming:"auto"` is the default (objection 9 — PRD §6 says naming is runner work):** try a pinned naming-runner (F2.5c registry); each named block gets `named_by:"runner"`. Runner absent/timeout → heuristic labels marked `named_by:"heuristic"` AND `needs_owner_naming:true` — the UI renders these as provisional (muted, "unnamed — needs you") and they CANNOT be ratified without touching (see §5). `naming:"heuristic"` skips the runner explicitly (offline mode), same marking.
- **(3b) Confidence is COMPONENTS, not a single vibe score (objection 8):** `CandidateMeta` carries `graph_cohesion` (internal-edge fraction; `null` when `edge_sample_size` is below a declared floor — a docs repo does not fake cohesion), `edge_sample_size`, `directory_support` (how directory-aligned the block is), `coverage_ratio` (members with graph nodes / members), and `shared_member_count`. The UI may summarize; the store keeps the components. Low-support blocks sort first in the review queue.

## 4. Revision & inheritance safety — a candidate inherits NOTHING it did not earn

- **(4a)** On a `ratified` skeleton: the scan writes ONLY `candidate_revision` (side-by-side; the Edit-Names-&-Boundaries flow diffs it later — promotion is out of F0c). The live blocks, their receipts, fingerprints and reconcile state are untouched, mechanically.
- **(4b)** On a `candidate` skeleton: wholesale replace under OCC — and the replacement **carries NO inheritance (objection 3)**: no receipts, no `membership_fingerprint`, no `resolved_members`, no `pre_archive_state`, and the store-level `unmapped_files`/`unmapped_total` reconcile cache is CLEARED (the next reconcile recomputes against the new boundaries). `candidate_revision` is also cleared (a fresh candidate supersedes a pending revision).
- **(4c)** Receipts NEVER appear in any candidate output (they bind to ratified boundaries by scope).

## 5. The UI (thin — the surfaces exist; the gesture is REVIEW, not blanket ratify)

- The F1 empty state's second button wires up: **"Scan this repo"** → the verb (`naming:"auto"`) → the map renders in candidate dress (dashed + banner) with per-block provisional-name treatment and the component scores in the chip tooltip.
- **(objection 10)** The candidate banner's button is **"Review & ratify"**: it walks the blocks (lowest support first); a block with `needs_owner_naming` must be named (or its runner name touched/accepted) before it becomes ratifiable. **"Ratify all" appears ONLY when every block has an owner-accepted name and no `shared`-seam is unresolved** — a blanket gesture over provisional labels is not offered. (The full Edit-Names-&-Boundaries editor stays a later slice; F0c ships the minimal walk: rename + accept + ratify.)
- **(objection 7)** `review_limit` (default 16) bounds the REVIEW QUEUE UI, never the emitted seed: the scan emits every block it found; the queue pages beyond the limit with the honest total. No `misc` dump-block exists — files that cluster nowhere stay in `unmapped_residue`/the tray with the true count.

## 6. The dogfood gate (objection 5 — quality vs the hand-written truth)

The slice's acceptance test runs the scan against THIS repo's own graph fixture and asserts **category recovery against the ratified seed**, not "≥N blocks": the candidate must (a) separate core-graph from evidence/risk within m1nd-core OR surface them as one block with the seam listed; (b) keep ingest, the served owner surface, and the UI as distinct blocks; (c) claim ≥80% of the files the hand-written seed claims (coverage), with the residue honest; (d) produce zero blocks whose members span unrelated top-level trees. Failing any of these fails the slice — the first map a stranger sees is the product's handshake.

## 7. Slices & out of scope

- **F0c-a (backend):** the collapse ladder + clustering, `CandidateMeta`/`candidate_revision` contract fields, the verb + its own transaction + inheritance-clearing, degeneration fallbacks, the category-recovery dogfood test.
- **F0c-b (UI):** Scan button, provisional-name dress, the Review-&-ratify walk, review_limit paging. (One PR with F0c-a per burst discipline if the sizes allow.)
- **Out (declared):** the full Edit-Names-&-Boundaries editor, promoting `candidate_revision` over a ratified skeleton, multi-scale/semantic clustering, socket type inference beyond edge relations.
