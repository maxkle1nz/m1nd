# HUMAN VIEW v2 — F2.5 technical amendment: the mission tray, direct delivery, and the runner daemon

Status: **RATIFIED** — oracle-confronted (CHANGE → 8 mandatory objections applied), ratified by the owner 2026-07-09 including the single scope narrowing (§5f: the Routing Rules screen deferred; capability pins land day one). This amendment stitches three already-signed surfaces into one buildable phase: PRD §7 (pluggable runners, the three packet modes, the policy layer), F0-TECH §9 (packet hygiene + `m1nd-runnerd`), and the mission-letter contract proven live by the first external hand agent (2026-07-09: the full cycle — packet in, gate run, receipt imported with OCC — executed against an ephemeral owner by an independent agent, both refusal laws exercised on purpose). Where this amendment NARROWS a signed scope it says so explicitly (§5f) and puts the narrowing itself up for ratification.

---

## 1. The mission letter — the one shape every seat speaks

The write mode's unit of state is a **mission letter**: a JSON document describing one mission's live state, emitted by whoever runs the mission, consumed by the tray. Frozen as `m1nd-mission-letter-v0`:

```json
{
  "schema": "m1nd-mission-letter-v0",
  "mission_id": "msn_<12hex>",
  "mission_seq": 3,
  "prev_letter_id": "<sha12 of the previous letter for this mission, or null for seq 1>",
  "block_id": "sb_...",
  "brain_ref": "<the brain's registered display name / repo_id — never an absolute path>",
  "seat": "oracle | hand",
  "runner_id": "<the owner-side pinned runner id, or null for non-runner emitters>",
  "capability": "build-runner | naming-runner | loop-runner | hand-runner | review-runner",
  "phase": "judging | executing | gate | review | merge_wait | landed | failed",
  "verdict": { "decision": "APPROVE | CHANGE | REJECT", "gist": "<one line>" },
  "gate": { "command": "<verbatim>", "exit_status": 0, "artifact_hash": "sha256:..." },
  "receipt_candidate": { "block_id": "sb_...", "type": "test", "scope": {"boundary_version": 1, "contract_version": 1}, "evidence": {"artifact_hash": "sha256:...", "evidence_refs": ["..."]} },
  "receipt": { "imported": true, "store_version": 9 },
  "packet_ref": "sha256:<hash of the MissionPacket markdown that opened the mission>",
  "synthetic": false,
  "tokens_total": 0,
  "started_at": "<ISO-8601>",
  "updated_at": "<ISO-8601>"
}
```

Signed decisions:
- **(1a) `seat`/`capability` name roles, never vendors.** Voice/model/vendor is the operator's private runner configuration and NEVER appears in the letter or any public surface.
- **(1b) Per-phase field gating** — a letter in `executing` has no verdict; `merge_wait` requires `gate`; **`landed` requires `receipt.imported == true` with a real `store_version`** (see 1d). Schema-optional, semantically gated by phase, mirroring the receipt taxonomy's per-type rules.
- **(1c) The letter is state, not evidence.** A mission letter NEVER changes a block's color. Color changes only through `receipt_import` with a valid scope — the existing anti-poison law, unchanged.
- **(1d) The `landed` law (oracle objection 3):** `landed` is RESERVED for "the receipt is confirmed in the SystemBlockStore" — the emitter may only write it after a successful `receipt_import` (and the tray renders the store_version it names). A zero-exit gate WITHOUT an imported receipt is `merge_wait` ("gate green — receipt not landed"), never `landed`. No surface may render gate-zero as evidence.
- **(1e) Ordering is causal, not clock-based (oracle objection 1):** mission state is NOT "max updated_at". Each mission's letters form a hash chain: `mission_seq` increments by 1 and `prev_letter_id` names the prior letter's content id. The current state is the head of the longest valid chain; a letter whose `prev_letter_id` does not match the current head is REJECTED at post time (`stale_head`, CAS semantics — PRD §3.1 extended to the letter stream). Append-only storage is preserved; the CAS guards the head pointer, not the log.
- **(1g) A real letter names a real block (field-hardening, proposed by the first hand agent):** `mission_post` refuses a letter whose `block_id` exists in no block of the bound brain's skeleton (`unknown_block`). *Clarified 2026-07-10 (field bug, the F11-c curation dispatch):* a SKELETON-scoped mission — one about the whole candidate, like the curation mission — anchors its letter at the store's **skeleton id**, which is a real identity of this brain and validates like a block; any OTHER skeleton/block id still refuses (the guard recognizes the one true anchor, it is not loosened). A legitimately synthetic letter — a smoke test or warm-pool probe — sets `synthetic: true` (serde-default `false`, byte-compatible) and skips the guard: the escape hatch is explicit, never a silent pass. A brain with no store yet accepts (nothing to validate against).
- **(1f) No absolute paths in the contract (oracle objection 7):** `brain_ref` replaces `brain_root`; the `worktree` field is REMOVED from the public schema. Worktree paths and other host-local detail live in an owner-runtime-local side record keyed by `mission_id` (never versionable, never served raw off-loopback); the tray fetches them only for local display. *Clarified 2026-07-10 (field bug, seen twice in one dogfood):* the canonical `brain_ref` is the brain's **display name — the basename of its project root, case-sensitive** — exactly the identity the `mission_post` brain guard compares against the DISPATCHING brain (the hosted brain a `?brain=` selector or routed session resolves, never blindly the owner's bound graph). It is NOT the skeleton id's sanitized slug (`sk_repo_b1_candidate` → the letter says `Repo-B1`, not `repo_b1`). A letter naming a different brain's ref is refused (`brain_mismatch`) — the honest refusal that replaced the 2026-07-09 mis-route where a reconnect-collapsed session silently posted letters into the bound brain's box.

## 2. Transport — the mailbox, extended honestly

Ground: the mailbox is the engine's letter surface (append-only JSONL, sha256 content ids, fates, `GET /api/mailbox?brain=`). The hand agent independently chose it over a sidecar. **Correction (oracle objection 2): the current `Letter` struct does NOT carry mission fields — F2.5a extends it; "exactly as it exists" was overclaim.** Signed:

- **(2a)** `Letter` gains an optional `kind` (default `"note"`, back-compatible with every existing line) and an optional `mission: MissionLetter` payload validated against §1. Existing letters parse unchanged (serde defaults); the mailbox caps/fates apply to mission letters like any other.
- **(2b)** The tray reads `GET /api/mailbox?brain=<root>&kind=mission`; the read side computes per-mission heads (the §1e chain) and returns heads + an honest `superseded_count` per mission. Absent `kind` = today's behavior byte-for-byte.
- **(2c)** Posting: a new verb `mission_post` (WRITE, deny-listed on read-only owners). Input `{agent_id, letter}`; validation = §1 schema + phase gating (1b/1d) + **head CAS (1e)** — a stale head returns `stale_head` and nothing is appended. Content-hash dedup still applies to identical replays (idempotent).
- **(2d)** Retention: mission letters obey the mailbox's existing caps; the tray shows the head per mission and the honest superseded count.

## 3. The tray — the fixed surface (the owner's ask, screen-book §5 extended)

- **(3a) Placement:** a right-edge fixed tray, collapsible, visible on EVERY surface (map, tree, hall). Collapsed: a thin strip with per-phase counts. Expanded: mission cards.
- **(3b) A mission card shows:** block name (click → the block on the map), seat + capability + `runner_id`, phase (the seven-state enum verbatim), the verdict gist when present, the gate line (`command · exit N`) when present, elapsed time, and — on `landed` — the receipt anchor (`receipt ✓ store vN`, the store_version from 1d). Copy law applies throughout.
- **(3c) Failure is never folded away:** a `failed` mission stays pinned atop the tray until explicitly dismissed (presentation-local; letters and ledger untouched).
- **(3d) Empty state:** "no missions yet — point an agent at a block" (links to compose).
- **(3e) The tray is read-only over letters**; its only writes are the compose flow's (§4) and local dismissal state.
- **(3f) Provenance on click (the owner's question, answered):** expanding a `landed` card shows the chain: packet_ref → runner_id (pinned capability + workspace) → gate artifact hash → receipt store_version. One card answers "what extended what, run by whom, proven by which receipt".

## 4. The compose panel — un-disabling the radios (PacketCompose, F2 → F2.5)

- **(4a) `direct`** = compose the packet (unchanged pure compositor) + `mission_post` the seq-1 letter (phase `judging`, with `packet_ref`) + deliver the packet markdown as a mailbox letter addressed to the target agent's inbox. No runner involved. The panel declares: "direct: posted to the agent's inbox — delivery is not execution."
- **(4b) `spawn`** = compose the packet + hand it to `m1nd-runnerd` (§5) naming a pinned `runner_id`; runnerd emits letters from `executing` onward on the same mission chain. Disabled with an honest note when no runnerd is registered.
- **(4c)** Screenshot toggle stays OFF/disabled until spawn ships redaction (F0-TECH §9 unchanged).
- **(4d)** Mode availability is policy-gated: read-only owner → clipboard only; no runnerd → clipboard + direct. Disabled states always say why.

## 5. `m1nd-runnerd` — the MVP scope (F0-TECH §9 made concrete)

- **(5a) Identity & registration (oracle objection 5):** capabilities are **pinned owner-side** in local config (`runners.toml` under the runtime root: `runner_id`, allowed capability, workspace root allowlist). `POST /api/runnerd/announce` (loopback + shared local secret `0600` + a per-boot challenge echo) proves LIVENESS ONLY — it can never grant or widen a capability. An announce for an unpinned `runner_id` is refused and logged.
- **(5b) MVP capabilities: `build-runner` AND `naming-runner`** — both one-shot command templates from operator-local config (`command = ["<operator's agent CLI>", "{packet_file}"]`); naming-runner is the cheap/fast lane (skeleton naming, research), build-runner the code lane. The packet is written into an **isolated git worktree** created per mission (worktree-per-agent doctrine as product law); runnerd streams phase letters via `mission_post` on the mission chain.
- **(5c) The gate & the candidate (oracle objection 4):** config names a `gate_command`; runnerd runs it after the agent exits, hashes the full log, and on zero exit emits a `merge_wait` letter carrying a **complete `receipt_candidate`** (§1 — block, type, scope read from a fresh snapshot, evidence with the real artifact hash and captured execution window). `receipt_import` preserves that candidate byte-for-byte while mechanically refusing reversed/equal timestamps, future-dated windows, or windows longer than 24 hours. **Runnerd holds NO `receipt_import` permission in the MVP**; the import is a human act from the tray (§6, F2.5d) or an explicitly-authorized agent session outside runnerd. Only after a confirmed import may a `landed` letter (1d) be posted.
- **(5d) Hard laws:** the owner NEVER spawns (runnerd is the only spawner); runnerd refuses to run without an isolated worktree; worktree paths stay in the owner-runtime-local side record (1f); kill/timeout → `failed` letter with the reason, never silence. Same-UID local malware is DECLARED out of the MVP threat model (loopback + secret defends the network and other users; a hostile same-user process is out of scope, stated honestly).
- **(5e) Out of MVP, declared:** loop-runner/hand-runner/review-runner adapters, parallel mission scheduling, remote runners, automatic receipt import, the Routing Rules screen (per-mission manual runner choice in compose for now).
- **(5f) Scope note vs PRD §10 (oracle objection 6):** PRD §10 sketched F2.5 as "spawn cycle with build-runner + naming-runner + policy layer live". This amendment DELIVERS both runners and the policy pins (5a) but DEFERS the Routing Rules screen to the next phase. That single deferral is a narrowing of signed scope and is called out here for explicit owner ratification.

## 6. Slices (each = one PR, RED-first; the safety laws land FIRST — oracle objection 8)

- **F2.5a — the contract + the laws (backend):** `mission_letter.rs` (schema, per-phase gating incl. the 1d landed-law, the 1e head-CAS), the `Letter.kind/mission` extension with byte-compatible parsing, `mission_post` + deny-list + `stale_head`, the `kind=mission` head-computing read, the owner-runtime-local side record (1f), and the complete `receipt_candidate` shape. Fixtures + the anti-lie tests (gate-zero-cannot-land; stale-head-rejected; letter-cannot-color).
- **F2.5b — the tray + direct (UI):** the fixed tray (3a-3f incl. provenance), `direct` mode in compose (4a), policy-gated mode availability (4d).
- **F2.5c — runnerd (new crate):** identity/pinning/announce (5a), both MVP runners (5b), worktree-per-mission, the gate + `receipt_candidate` emission (5c), `spawn` mode wiring (4b).
- **F2.5d — the human landing:** the tray's `merge_wait` card offers "import this receipt" pre-filled from the candidate (one click, still explicit, still the human's gesture); on success the `landed` letter posts; PATHOS/doc gate for the whole F2.5 arc.

## 7. What this amendment does NOT decide

The operator's concrete runner configuration (which local agent fills which capability) is private operator config, out of the repo. First-class marketplace runners stay future (PRD §7). Multi-brain tray aggregation is deferred — the tray reads the viewed brain, like the map. The Routing Rules screen is deferred per 5f and awaits its own slice.

## 8. System-integrity hardening (ARC-1, post-ratification)

Two write-surface laws an oracle verdict surfaced after ratification: doctrine that lacked a mechanical mirror. Each lands as its own RED-first slice.

### § ratify — a mechanical mirror for "RATIFY IS HUMAN"

- **The gap.** `system_blocks_ratify` sits in the skeleton write-gate (`skeleton_write_needs_root_gate`), but the explicit `?brain=` REST selector deliberately sets `caller_root = workspace_root` for skeleton writes (http_server dispatch) so the owner's screen can write the viewed brain — which also means the gate cleared for ANY local process reaching that REST route. The `ratifier` is a free string and no origin/seat check existed. "Ratify is the human gesture" was doctrine (M1ND_INSTRUCTIONS §6) with no enforcement: any local process could ratify via REST.
- **The law.** Ratify passes only when the gesture is human. Mechanically: `handle_system_blocks_ratify` REQUIRES `ratified_via:"human-ui"` — the origin token the owner's Human View screen stamps (the `client.ts` api layer; the browser is the only composer of it). A runner/agent MCP client never composes it by contract, so a bare ratify is refused `human_gesture_required` and TOUCHES NOTHING, with the lesson *"ratify is the human gesture — the owner's screen sends it; agents never do."*
- **Honesty about the token.** On an unauthenticated loopback the field is forgeable, so the guard is deliberately DOUBLE: (a) the mandatory field with a teaching refusal, and (b) a RED test pinning that a seat/agent call WITHOUT the field is refused. It kills the CHEAP vector — an agent that simply calls ratify — not a malicious same-UID process, which is out of the threat model exactly as §5d declares for same-UID local malware. The refusal is a soft `{ok:false, refused, lesson}` envelope (the write-surface refusal shape), never a silent pass.

### § candidate_edit — o5 on the write verb, for the runner seat

- **The gap.** The o5 sanitizer (`naming_runner::sanitize_naming`) governed only the naming LANE — the scan path that applies a runner daemon's `/name` output before the seed is stored. The `candidate_edit` WRITE verb did NOT sanitize: its module comment declared the op stream "trusted MCP input", so a RUNNER-seat rename (a curation-mission hand editing via the verb) could write a hostile name/purpose — a URL, a control char, a path, a token-like blob — straight into the store.
- **The law.** When the seat is RUNNER, every name/purpose entering through `candidate_edit` passes the o5 sanitizer, REUSED verbatim (`naming_runner::sanitize_rename_fields` delegating to the same per-field gate — no second copy of the rules). A violation REFUSES the op, and the o1 preflight-on-a-clone makes that refusal atomic: a single hostile rename aborts the WHOLE batch with the honest reason naming the field + class (`name: path separator`, `purpose: token-like string`, …), byte-compatible with the naming-lane style. The OWNER seat stays UN-sanitized — the human at the screen types whatever they mean, including a name with `/`; o5 is for LLM input, not the owner's keystrokes.
- **Pinned RED→GREEN.** A runner batch with a hostile rename is refused naming the field; the SAME batch under the owner seat applies (stored verbatim); a legitimate runner rename — the single-line plain-text shape the naming lane emits — passes byte-identically (validation only, never a rewrite).
