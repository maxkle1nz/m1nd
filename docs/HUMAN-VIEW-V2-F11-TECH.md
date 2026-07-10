# HUMAN VIEW v2 — F11 technical amendment: candidate editing with minimum human friction

Status: **RATIFIED** — oracle-confronted (CHANGE → 6 mandatory objections applied), ratified by the owner 2026-07-10. (The same oracle pass also delivered the promised post-hoc review of the DIR-FIRST granularity fix: **RATIFY-POST-HOC** — DIR-FIRST with FLOOR=3 stands, the top-level clamp stays, single-pass majority costura stays, the Capacitor mipmap noise is deferred to F11 review/curation.) The F11 screen (Edit Names & Boundaries) is ALREADY designed and ratified in the screen book (§3) — this amendment does not redesign it; it supplies the missing engine underneath and binds the whole state to one law the owner set in plain words: **"reduce human friction to the maximum."** The default path is zero-touch — the map arrives assembled and named; the human reads and stamps. Editing is the available exception, never the job.

## 0. The friction law (the owner's directive, made mechanical)

- **(0a)** After a scan with a live naming-runner, the intended experience is: *read the map ~30s → one "Ratify all" click*. Every design decision below is scored against that path.
- **(0b)** Runner-named blocks (`named_by:"runner"`) are ratifiable WITHOUT an individual touch (the F0c-b law already distinguishes them); only raw-heuristic labels gate. With full runner naming, "Ratify all" appears immediately.
- **(0c)** The heavy case (a large candidate like a 43-block production monorepo) has a one-gesture escape: **dispatch a curation mission to the hand** — the agent does the editing; the human reviews the result, not the blocks.

## 1. The verb — `candidate_edit` (WRITE, deny-listed, dual-hand by birth)

One verb, typed operations, one OCC transaction — never six loose verbs:

```json
{ "agent_id": "...", "expected_store_version": 7, "ops": [
  {"op":"rename",         "block_id":"sb_x", "name":"Auth", "purpose":"..."},
  {"op":"merge",          "into":"sb_x", "block_ids":["sb_y","sb_z"]},
  {"op":"split",          "block_id":"sb_x", "by":{"paths":[["a/**"],["b/**"]]}},
  {"op":"move_member",    "path":"src/hook.ts", "from":"sb_x", "to":"sb_y"},
  {"op":"resolve_seam",   "path":"src/shared.ts", "resolution":"primary:sb_y"},
  {"op":"assign_unmapped","path":"scripts/x.sh", "block_id":"sb_x"}
] }
```

Signed decisions:
- **(1a) Candidate-only.** Every op refuses on a `ratified` skeleton (`skeleton_not_candidate`) — editing a signed boundary is a different ceremony (the deferred revision-promotion flow). The whole batch is atomic: all ops apply under one OCC key and one `store_version` bump, or nothing does (first invalid op aborts the batch with its index named).
- **(1b) Dual-hand by birth.** The verb is agnostic of caller: the human's F11 screen and the hand's curation mission speak the same contract under the same OCC — concurrent editors collide into an honest `Conflict`, never a merge of intents.
- **(1c) Provenance per touch.** `rename` by the GUI stamps `named_by:"owner"` (the strongest label; clears `needs_owner_naming`); by an agent seat it stamps `named_by:"runner"`. `merge` recomputes the surviving block's `candidate_meta` and unions membership (dedup, shared entries preserved); `split` by explicit path groups (the UI supplies them from the boundary-diff view; community/directory presets are client-side conveniences that compile to path groups). `resolve_seam` rewrites the member's role on BOTH blocks in the same batch (primary on one, removed or demoted on the other — never a half-resolved seam). `assign_unmapped` moves a path from `unmapped_residue` into a block as an exact member.
- **(1d) Sockets follow ids.** Merge/split rewrite internal socket `to:` targets by block id (the F0c stable-id law pays off here); a dangling target after an edit is an abort, not a silent drop.
- **(1e) Ratify stays human.** `candidate_edit` never flips state; `system_blocks_ratify` remains the owner's gesture, exactly like receipt landing. The hand proposes; the human signs.

## 2. The naming-runner wiring (the zero-touch enabler — closes the F0c-a declared gap)

- **(2a)** `skeleton_candidate` with `naming:"auto"` now actually calls a pinned live naming-runner via the runnerd registry: one naming packet per block (member list + dominant kinds + top symbols, no file bodies), bounded per-block timeout, batch-parallel. Runner success → `named_by:"runner"`, `needs_owner_naming:false`. Runner absent/timeout/parse-fail → the existing heuristic fallback, honestly marked, per block (a partial run is fine — some runner-named, some heuristic).
- **(2b)** The same path is exposed as an in-screen action: **"Name with runner"** (selected block or all provisional blocks) — implemented as a thin client of the same engine, applying results through `candidate_edit` rename ops so provenance and OCC hold.
- **(2c)** The naming packet and the response schema are declared (name ≤ 40 chars, one-line purpose ≤ 120; a response violating the schema falls back honestly). No naming output is ever trusted into any field but `name`/`purpose`.

## 3. The curation mission (the heavy-case escape hatch)

- **(3a)** The candidate banner offers **"Send to an agent for curation"** (visible when a runnerd hand is live): composes a curation packet (the full candidate seed + the report + the owner's optional note), opens a mission (the existing mission-letter chain), and the hand edits via `candidate_edit` — merging thin blocks, naming, resolving seams, assigning orphans.
- **(3b)** The mission's `merge_wait`-equivalent is the edited candidate itself (the store's new version) + a summary letter; the human's review IS the F11 screen over the polished result, and the landing IS `ratify`. No new machinery: mission letters, the tray, and the verb already exist — this is wiring, not invention.
- **(3c)** The hand can never ratify (1e) and never edits a ratified skeleton (1a) — the same two laws bound both hands.

## 4. The F11 screen (build what is drawn — the screen book §3 is the spec)

- **(4a)** Two columns as drawn: the block list (inline-editable names, count, confidence, seam flags, expandable members with certainty dots) and the selected panel (editable purpose, boundary diff, seam resolution radio, Split/Merge/Reset actions). The unmapped tray with "assign to block ▾". Footer: "Ratify N blocks → v1" / "Ratify selected only" / "Later".
- **(4b)** Every action compiles to `candidate_edit` ops, batched per gesture (a rename is one op; a seam radio is one op; "Reset proposal" is client-side revert of unapplied edits — applied edits are history, undone by further edits, never by store rollback).
- **(4c)** The friction ordering: runner-named blocks render calm (no badge); provisional ones surface first with "— needs you"; the screen's header shows the zero-touch status line ("all N blocks runner-named — ready to ratify" when true).
- **(4d)** The drift sub-state (the drawn DRIFT ALERT) stays deferred to the drift slice — declared, not silently dropped.

## 4bis. Oracle objections applied (CHANGE → 6)

- **(o1) Atomicity is preflight-on-a-clone, not first-error-persist (a1).** The batch is validated against a working copy: every op AND every final invariant (no dangling socket, no empty block, no orphaned member, no unresolved seam left by the batch) is checked before ANY persistence. First error returns its op index; on full success one persist + one `store_version` bump. A partial apply under OCC is *less* safe than none — never done.
- **(o2) Merge canonicalization before any member op (a2).** Before applying, build a canonical `merged_id → survivor_id` map from the batch's merges; reject cycles, merge-into-victim, and ambiguity. Every `move_member`/`resolve_seam`/`assign_unmapped` `from`/`to`/`block_id` is canonicalized through it, so an op naming a block that another op absorbs resolves to the survivor (or aborts if the reference is dead). `resolve_seam` supports **3+ owners** (a member claimed by N blocks): the resolution names the primary and the batch removes the member from all others in one pass.
- **(o3) Presets compile to explicit path groups server-side before mutation (a3).** Community/directory split presets may render as a client preview, but the mutation payload is always explicit path groups; the server compiles/validates them against current membership (non-empty, disjoint, total-or-honest-residue) before splitting — the mutation is reproducible from the stored ops alone.
- **(o4) A soft, non-blocking lease — never a hard lock (a4).** A curation mission stamps an advisory `curating_by`/`curating_until` (the expiry — what the compare-and-set needs; F11-a landed the expiry form, not a `since` stamp) the F11 screen surfaces ("a hand is curating candidate vN") with a stale banner; it NEVER blocks the owner (a dead agent must not trap the candidate). A `Conflict` forces reload/rebase, never a silent semantic merge. **The advisory lease is a first-class owner verb** (`candidate_lease {acquire|release|refresh, agent_id, ttl_secs}`) — this closes the TOCTOU the hand agent flagged in its own file-lease takeover (the request that arrived the same day the oracle asked for the soft lease): the owner is the single serialization point, so acquire is atomic (compare-and-set on `curating_by`+expiry), refresh extends the owner-held TTL, and an expired lease is reclaimable by anyone (no dead-agent trap). It stays ADVISORY — `candidate_edit` never *requires* a held lease (that would let a dead agent block the owner); the lease only warns. Lands in F11-a beside the verb.
- **(o5) The naming runner is UNTRUSTED input (a5).** A name/purpose from a runner is sanitized as hostile: reject control chars, active HTML/Markdown, `/`, `\`, URLs, emails, absolute paths, token/base64/hash-like strings; enforce length; strip to plain text; on any violation fall back to the heuristic label, marked. A runner name is never trusted into any field but a sanitized `name`/`purpose`, never rendered unescaped.
- **(o6) Provenance is enforced at ratify, not just stamped (a6).** `NamedBy::Owner` is a real state; a human `candidate_edit rename` sets it and clears `needs_owner_naming`; and `system_blocks_ratify` REJECTS any block still `needs_owner_naming` (raw-heuristic, untouched) — so "Ratify all" over runner-named blocks is legal (the human signs the whole map) but a raw-heuristic block cannot be ratified without a touch. The friction law holds without weakening the Ratification law.

## 5. Slices (each PR-able, RED-first, our standard)

- **F11-a (backend):** `candidate_edit` + all six ops + preflight-on-a-clone atomic batch + merge canonicalization + the `candidate_lease` advisory verb + the ratify provenance gate (o6) + the anti-lie tests (ratified-refuses; OCC conflict intact; preflight-abort-persists-nothing; seam-rewrites-all-owners; merge-unions-and-rewrites-sockets; dangling-socket-aborts; provenance stamps; ratify-refuses-untouched-heuristic; lease-is-advisory-never-blocks).
- **F11-b (naming wiring):** the scan→runnerd naming call + per-block fallback + the schema gate + the in-screen action path (engine only; the button lands with c).
- **F11-c (the screen):** the drawn F11 over the existing walk (evolve `ReviewRatify` into the two-column screen), SSR-tested per the uiproof standard, plus the curation-mission dispatch button.
- Acceptance dogfood: the two LIVE candidates (a 14-block sparse repo and a 43-block production monorepo) — the sparse one must reach ratified in ≤3 human gestures with a live runner; the monorepo through one curation mission + review + one ratify.

## 6. Out of scope, declared

Editing ratified skeletons / promoting `candidate_revision` (the next ceremony), the drift-alert sub-state, undo-history beyond further-edits, and naming-runner fine-tuning.
