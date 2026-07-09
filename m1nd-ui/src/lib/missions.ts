/*
 * missions — the mission tray's pure heart (HUMAN-VIEW-V2-F2.5 §1–§4).
 *
 * The write mode's unit of state is a MISSION LETTER: a JSON document describing one
 * mission's live state, emitted by whoever runs the mission and read by the tray.
 * This module is the DOM-free logic the MissionTray renders and the tests prove — the
 * TypeScript mirror of `m1nd-mcp/src/mission_letter.rs`'s frozen `m1nd-mission-letter-v0`
 * shape (serde serialization is the wire contract these types are craved against):
 *   - `seat`/`phase` are snake_case, `capability` kebab-case, `verdict.decision` UPPERCASE;
 *   - optional fields are absent (never `null`) when the emitter had nothing (serde
 *     `skip_serializing_if = Option::is_none`), except `prev_letter_id` which is
 *     absent for seq-1;
 *   - `tokens_total`, `started_at`, `updated_at` are always present.
 *
 * The tray READS letters (§3e). Its only write is the compose flow's `direct` mode
 * (§4a): compose the seq-1 `judging` letter here, post it via `mission_post`.
 *
 * Copy law (PRD §13): scoped, auditable claims only — never "done/proven/correct".
 * The seven phase names are rendered VERBATIM; the §1d landed-law is encoded so the
 * UI can never render a green gate as a landing.
 */
import type { Receipt, SystemBlocksSnapshot } from './buildMap';

/** The frozen schema tag (mission_letter.rs `MISSION_LETTER_SCHEMA`). */
export const MISSION_LETTER_SCHEMA = 'm1nd-mission-letter-v0';

/** The mailbox `kind` a mission letter files under (§2a). */
export const KIND_MISSION = 'mission';

/** The seat naming a role, never a vendor (§1a). Snake_case on the wire. */
export type Seat = 'oracle' | 'hand';

/** The five MVP capabilities (§1) — role names, kebab-cased on the wire. */
export type Capability =
  | 'build-runner'
  | 'naming-runner'
  | 'loop-runner'
  | 'hand-runner'
  | 'review-runner';

/** The seven-state phase enum (§1), verbatim, snake_case on the wire. */
export type Phase =
  | 'judging'
  | 'executing'
  | 'gate'
  | 'review'
  | 'merge_wait'
  | 'landed'
  | 'failed';

/** The ordered phase enum — the strip renders counts in this causal order. */
export const PHASES: readonly Phase[] = [
  'judging',
  'executing',
  'gate',
  'review',
  'merge_wait',
  'landed',
  'failed',
] as const;

/** A verdict's decision (§1). UPPERCASE on the wire. */
export type VerdictDecision = 'APPROVE' | 'CHANGE' | 'REJECT';

/** The receipt taxonomy reused from the SystemBlock store (system_blocks.rs). */
export type ReceiptType = 'test' | 'structural' | 'runtime' | 'review' | 'handoff' | 'spec';

/** The oracle's verdict (§1) — a decision plus a one-line gist. */
export interface Verdict {
  decision: VerdictDecision;
  gist: string;
}

/** The gate evidence line (§1) — verbatim command, exit status, hash of the log. */
export interface GateEvidence {
  command: string;
  exit_status: number;
  artifact_hash: string;
}

/** A candidate receipt's scope (§1) — the two version anchors the emitter can see. */
export interface CandidateScope {
  boundary_version: number;
  contract_version: number;
}

/** Receipt evidence (anti-poison) — the universal anchor is mandatory, the
 *  execution identity optional. Mirrors system_blocks.rs `ReceiptEvidence`. */
export interface ReceiptEvidence {
  command?: string;
  cwd?: string;
  exit_status?: number;
  started_at?: string;
  ended_at?: string;
  artifact_hash: string;
  stdout_excerpt?: string;
  evidence_refs: string[];
}

/** A complete receipt candidate (§1, §5c) — the one-click import proposal. */
export interface ReceiptCandidate {
  block_id: string;
  type: ReceiptType;
  scope: CandidateScope;
  evidence: ReceiptEvidence;
}

/** The receipt anchor a `landed` letter carries (§1d) — `imported == true` with a
 *  real `store_version` is the ONLY thing that lands. */
export interface ReceiptAnchor {
  imported: boolean;
  store_version: number;
}

/** The mission letter — the one shape every seat speaks (§1). No absolute-path
 *  field exists (§1f: `brain_ref` is a reference string, never a path). */
export interface MissionLetter {
  schema: string;
  mission_id: string;
  mission_seq: number;
  /** The prior letter's content id (§1e), absent for seq-1. */
  prev_letter_id?: string | null;
  block_id: string;
  brain_ref: string;
  seat: Seat;
  runner_id?: string | null;
  capability: Capability;
  phase: Phase;
  verdict?: Verdict;
  gate?: GateEvidence;
  receipt_candidate?: ReceiptCandidate;
  receipt?: ReceiptAnchor;
  packet_ref?: string;
  tokens_total: number;
  started_at: string;
  updated_at: string;
}

/** The head of one mission's chain (mission_letter.rs `MissionHead`, §2b). */
export interface MissionHead {
  mission_id: string;
  head_letter_id: string;
  head: MissionLetter;
  /** How many letters for this mission are NOT the head (§2b honest count). */
  superseded_count: number;
}

/** The `GET /api/mailbox?kind=mission` payload (http_server.rs `handle_mailbox`,
 *  §2b): the `served_brain` echo + the per-mission heads. A pre-F2.5a owner ignores
 *  `kind` and returns the field-report shape instead (no `missions`), which the
 *  hook reads as "needs an updated owner". */
export interface MissionsResponse {
  served_brain?: { project_root?: string | null; display_name?: string | null } | null;
  missions?: MissionHead[];
}

// ---------------------------------------------------------------------------
// Copy — the seven phase names + the §1d landed-law, rendered honestly.
// ---------------------------------------------------------------------------

/**
 * CONCEPT → GLYPH (the collapsed strip's per-phase markers). One marker per phase,
 * centralized here so the strip cannot drift — the compact vertical strip reads as
 * single text glyphs by design (the lucide concept-icons in lib/icons/registry.tsx
 * are for the wider surfaces; a phase is a state, not a noun with an icon). The
 * label is the phase enum VERBATIM (copy law: never "done/proven") — `merge_wait`
 * spaces to "merge wait" for reading, the underscore is not a claim.
 */
export const PHASE_META: Record<Phase, { glyph: string; label: string }> = {
  judging: { glyph: '⚖', label: 'judging' },
  executing: { glyph: '▶', label: 'executing' },
  gate: { glyph: '⚙', label: 'gate' },
  review: { glyph: '⊙', label: 'review' },
  merge_wait: { glyph: '⏳', label: 'merge wait' },
  landed: { glyph: '✓', label: 'landed' },
  failed: { glyph: '✗', label: 'failed' },
};

/** The phase name, VERBATIM (copy law: none of these is "done/proven/correct"). */
export function phaseLabel(phase: Phase): string {
  return PHASE_META[phase]?.label ?? phase;
}

/**
 * A readable block name derived from the stable `block_id` (`sb_<repo>_<slug>`): the
 * tray reads LETTERS (§3e), which carry `block_id`, not the human name — so it
 * de-slugs the id (`sb_m1nd_core_graph_kernel` → "core graph kernel"), stripping the
 * repo prefix when it matches `brainRef`. Deterministic; falls back to the raw id.
 */
export function blockLabelFromId(blockId: string, brainRef?: string): string {
  let slug = blockId.replace(/^sb_/, '');
  const repo = brainRef?.trim().toLowerCase();
  if (repo && slug.toLowerCase().startsWith(`${repo}_`)) slug = slug.slice(repo.length + 1);
  const words = slug.split('_').filter(Boolean).join(' ');
  return words || blockId;
}

/**
 * The §1d landed-law, encoded for the tray: a `merge_wait` letter whose gate is
 * green (exit 0) but whose receipt is NOT imported is exactly "gate green — receipt
 * not landed", never a landing. Returns that honest line, or null when it does not
 * apply (so the card only flies it when the law is live).
 *
 * §6-F2.5d — candidate-aware: a green gate WITH a `receipt_candidate` still reads
 * "receipt not landed" (the law holds until the human lands it, and the card offers
 * the one-click import beside this line); a green gate WITHOUT a candidate has
 * nothing to import — the honest state is "gate green — no candidate attached", never
 * a button that would fabricate a receipt.
 */
export function mergeWaitStatusLine(letter: MissionLetter): string | null {
  if (letter.phase !== 'merge_wait') return null;
  const receiptLanded = letter.receipt?.imported === true && (letter.receipt?.store_version ?? 0) >= 1;
  if (receiptLanded) return null;
  if (letter.gate && letter.gate.exit_status === 0) {
    return letter.receipt_candidate
      ? 'gate green — receipt not landed'
      : 'gate green — no candidate attached';
  }
  return 'receipt not landed';
}

/** The gate line (§3b): `<command> · exit N`, or null when there is no gate. */
export function gateLine(letter: MissionLetter): string | null {
  if (!letter.gate) return null;
  return `${letter.gate.command} · exit ${letter.gate.exit_status}`;
}

/**
 * The receipt anchor a `landed` card shows (§3b, §1d): `receipt ✓ store vN`, the
 * `store_version` the receipt confirmed. Null unless the letter truly landed (an
 * imported receipt with a real store_version) — the UI cannot fake a landing.
 */
export function receiptAnchorLabel(letter: MissionLetter): string | null {
  const r = letter.receipt;
  if (!r || !r.imported || r.store_version < 1) return null;
  return `receipt ✓ store v${r.store_version}`;
}

// ---------------------------------------------------------------------------
// Ordering + elapsed (pure, testable).
// ---------------------------------------------------------------------------

/** Compare two ISO-8601 strings for a newest-first sort (lexicographic — ISO sorts
 *  correctly; a missing/blank ts sinks last). */
function byUpdatedDesc(a: MissionHead, b: MissionHead): number {
  const ta = a.head.updated_at ?? '';
  const tb = b.head.updated_at ?? '';
  if (ta === tb) return 0;
  return ta < tb ? 1 : -1;
}

/**
 * The tray order (§3c): `failed` missions are PINNED atop the tray (newest failure
 * first), then every other mission by `updated_at` descending. Failure is never
 * folded away — the pin is presentation, the letters and ledger are untouched.
 * Pure: returns a new array, never mutates the input.
 */
export function sortForTray(heads: MissionHead[]): MissionHead[] {
  const failed = heads.filter((h) => h.head.phase === 'failed').sort(byUpdatedDesc);
  const rest = heads.filter((h) => h.head.phase !== 'failed').sort(byUpdatedDesc);
  return [...failed, ...rest];
}

/** Per-phase counts across the heads (for the collapsed strip). */
export function phaseCounts(heads: MissionHead[]): Record<Phase, number> {
  const counts = Object.fromEntries(PHASES.map((p) => [p, 0])) as Record<Phase, number>;
  for (const h of heads) counts[h.head.phase] = (counts[h.head.phase] ?? 0) + 1;
  return counts;
}

/**
 * A compact elapsed label from a start ISO timestamp: `12s`, `5m`, `2h`, `3d`. Pure;
 * `now` is injectable so the render is deterministic under test. An unparseable
 * start returns '' (the card simply omits the elapsed segment — never a fake time).
 */
export function elapsedLabel(startedAt: string, now: number = Date.now()): string {
  const start = Date.parse(startedAt);
  if (Number.isNaN(start)) return '';
  const secs = Math.max(0, Math.floor((now - start) / 1000));
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

// ---------------------------------------------------------------------------
// Direct mode (§4a) — the seq-1 judging letter + the send orchestration.
// ---------------------------------------------------------------------------

/** 6 random bytes → a `msn_<12hex>` id (mission_letter.rs `valid_mission_id`:
 *  exactly 12 lowercase-hex characters). `getRandomValues` is injectable for a
 *  deterministic test. */
export function newMissionId(rand: (out: Uint8Array) => Uint8Array = defaultRandom): string {
  const bytes = rand(new Uint8Array(6));
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  return `msn_${hex}`;
}

function defaultRandom(out: Uint8Array): Uint8Array {
  globalThis.crypto.getRandomValues(out);
  return out;
}

/** The sha256 hex of a string (WebCrypto — available in the browser and Node ≥20).
 *  Used for the packet_ref: `sha256:<hex>` of the composed MissionPacket markdown. */
export async function sha256Hex(text: string): Promise<string> {
  const data = new TextEncoder().encode(text);
  const digest = await globalThis.crypto.subtle.digest('SHA-256', data);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}

/** Args for the seq-1 `judging` letter (§4a). */
export interface Seq1Args {
  blockId: string;
  /** The brain reference (§1f) — a display name / repo_id, NEVER an absolute path. */
  brainRef: string;
  /** `sha256:<hex>` of the composed packet markdown (§4a). */
  packetRef: string;
  /** The judging seat (§4a: direct posts the seq-1 `judging` letter). Default `oracle`. */
  seat?: Seat;
  /** The intended lane (§1). Default `build-runner` (the code lane, §5b). */
  capability?: Capability;
  /** ISO timestamp for started_at/updated_at. Default now. */
  now?: string;
  /** The mission id (default a fresh `msn_<12hex>`). Injectable for tests. */
  missionId?: string;
}

/**
 * Compose the seq-1 `judging` mission letter for `direct` mode (§4a). Seq 1 with a
 * NULL prev (the chain root — §1e), phase `judging` (the oracle proposing the
 * mission; per-phase gating lets a `judging` letter carry no verdict/gate/receipt),
 * `runner_id` absent (no runner is involved in direct — §4a). The `packet_ref`
 * anchors the letter to the exact packet that opened it. Pure.
 */
export function composeSeq1Letter(args: Seq1Args): MissionLetter {
  const nowIso = args.now ?? new Date().toISOString();
  return {
    schema: MISSION_LETTER_SCHEMA,
    mission_id: args.missionId ?? newMissionId(),
    mission_seq: 1,
    // prev_letter_id is ABSENT for seq-1 (§1e) — omitted, never null on the wire.
    block_id: args.blockId,
    brain_ref: args.brainRef,
    seat: args.seat ?? 'oracle',
    capability: args.capability ?? 'build-runner',
    phase: 'judging',
    packet_ref: args.packetRef,
    tokens_total: 0,
    started_at: nowIso,
    updated_at: nowIso,
  };
}

/** The `mission_post` outcome the owner returns (mission_letter.rs `PostOutcome`). */
export interface PostOutcome {
  letter_id: string;
  mission_id: string;
  mission_seq: number;
  deduped: boolean;
  phase?: Phase;
}

/** The seams `sendDirectPacket` writes through — injected so the flow is provable
 *  DOM-free (the repo's `loadBuildMap` pattern: a fetch/write heart extracted from
 *  the component). */
export interface DirectSendDeps {
  /** Post the seq-1 letter (default `api.missionPost`). May throw (`stale_head`,
   *  read-only deny, network) — the caller surfaces the honest error, never retries. */
  postMission: (letter: MissionLetter) => Promise<PostOutcome>;
  /** Copy the packet markdown for the human to paste into the agent (the honest MVP
   *  fallback — no common-letter post path exists in F2.5a). */
  writeClipboard?: (text: string) => Promise<void> | void;
  hash?: (text: string) => Promise<string>;
  now?: () => string;
  newId?: () => string;
}

/** What `direct` needs to build + send the letter. */
export interface DirectSendArgs {
  markdown: string;
  blockId: string;
  brainRef: string;
  seat?: Seat;
  capability?: Capability;
}

/** The result of a `direct` send. */
export interface DirectSendResult {
  letter: MissionLetter;
  outcome: PostOutcome;
  /** True when the packet markdown was copied to the clipboard (the paste half). */
  clipboardCopied: boolean;
}

/**
 * `direct` mode (§4a): compose the seq-1 `judging` letter (packet_ref = sha256 of
 * the composed markdown), `mission_post` it, and — because F2.5a exposes NO
 * common-letter post path (the mailbox `Letter` has no recipient field, and the
 * only box-writing verb is `mission_post`) — copy the packet markdown to the
 * clipboard for the human to paste into the agent. "Delivery is not execution": the
 * letter is state, the paste is the delivery. The post runs FIRST; a failed post
 * throws (the caller shows the honest error) and the clipboard step never masks it.
 */
export async function sendDirectPacket(
  args: DirectSendArgs,
  deps: DirectSendDeps,
): Promise<DirectSendResult> {
  const hex = await (deps.hash ?? sha256Hex)(args.markdown);
  const letter = composeSeq1Letter({
    blockId: args.blockId,
    brainRef: args.brainRef,
    packetRef: `sha256:${hex}`,
    seat: args.seat,
    capability: args.capability,
    now: deps.now?.(),
    missionId: deps.newId?.(),
  });
  const outcome = await deps.postMission(letter);
  let clipboardCopied = false;
  if (deps.writeClipboard) {
    try {
      await deps.writeClipboard(args.markdown);
      clipboardCopied = true;
    } catch {
      clipboardCopied = false;
    }
  }
  return { letter, outcome, clipboardCopied };
}

// ---------------------------------------------------------------------------
// Spawn mode (§4b, F2.5c) — the runner daemon liveness + the owner→runnerd proxy.
// ---------------------------------------------------------------------------

/** One live runner from `GET /api/runnerd/status` (runnerd_owner.rs `status_json`).
 *  Liveness only (§5a) — a port + last_seen, never a capability. */
export interface LiveRunner {
  runner_id: string;
  port: number;
  last_seen_ms: number;
}

/** The `GET /api/runnerd/status` payload (§5a read). Empty `runners` = no daemon has
 *  announced (the compose keeps spawn disabled with the amendment's honest note). */
export interface RunnerdStatus {
  runners: LiveRunner[];
  count: number;
}

/** The `mission_spawn` proxy input (§4b). The owner adds the secret + the workspace
 *  routing; the browser sends only these fields (it never holds the shared secret). */
export interface SpawnInput {
  runnerId: string;
  packetMarkdown: string;
  blockId: string;
  brainRef: string;
}

/** The `mission_spawn` proxy result (§4b) — the daemon's acceptance, relayed. */
export interface SpawnOutcome {
  mission_id: string;
  accepted: boolean;
  runner_id?: string | null;
}

/** Whether any runner is live (the spawn radio un-disables only then, §4b). */
export function runnerdAvailable(status: RunnerdStatus | null | undefined): boolean {
  return !!status && Array.isArray(status.runners) && status.runners.length > 0;
}

/** The seam `sendSpawnPacket` writes through — injected so the flow is provable
 *  without a network (the repo's `sendDirectPacket` pattern). */
export interface SpawnSendDeps {
  /** POST the owner's `mission_spawn` proxy (default `api.missionSpawn`). May throw
   *  the daemon's honest refusal (`unpinned_runner`, `workspace_not_allowed`, no
   *  runner) — the caller surfaces it verbatim, never a silent retry. */
  spawnMission: (input: SpawnInput) => Promise<SpawnOutcome>;
}

/** What `spawn` needs to launch a mission (§4b). */
export interface SpawnSendArgs {
  markdown: string;
  blockId: string;
  brainRef: string;
  runnerId: string;
}

/**
 * `spawn` mode (§4b): hand the composed packet to a pinned, live runner via the
 * owner's `mission_spawn` proxy. Unlike `direct`, there is NO client-composed letter
 * and NO clipboard — the runner daemon opens the mission chain (`judging → executing
 * → merge_wait|failed`) and NEVER lands (§1d). The result carries the `mission_id`
 * the toast shows ("mission started — watch the tray").
 */
export async function sendSpawnPacket(
  args: SpawnSendArgs,
  deps: SpawnSendDeps,
): Promise<SpawnOutcome> {
  return deps.spawnMission({
    runnerId: args.runnerId,
    packetMarkdown: args.markdown,
    blockId: args.blockId,
    brainRef: args.brainRef,
  });
}

// ---------------------------------------------------------------------------
// The human landing (§6-F2.5d) — the tray's one-click receipt import.
// ---------------------------------------------------------------------------

/** The `receipt_import` outcome the owner returns (system_blocks_handlers.rs
 *  `handle_receipt_import`) — the new `store_version` + the block's receipt count. */
export interface ReceiptImportOutcome {
  store_version: number;
  block_id: string;
  receipt_count: number;
}

/** The seams `landCandidate` writes through — injected so the whole human-landing
 *  flow is provable DOM-free (the repo's `runReconcile`/`sendDirectPacket` pattern). */
export interface LandDeps {
  /** A FRESH `system_blocks_snapshot` (default `api.systemBlocksSnapshot`) — read at
   *  land time so `expected_store_version` and the block's CURRENT resolution are
   *  never stale. */
  snapshot: () => Promise<SystemBlocksSnapshot>;
  /** The anti-poison `receipt_import` (default `api.receiptImport`). Throws the
   *  owner's honest refusal (`stale_scope`, `conflict`, read-only) verbatim — never a
   *  silent retry, never a scope rewrite. */
  importReceipt: (input: {
    expectedStoreVersion: number;
    blockId: string;
    receipt: Receipt;
  }) => Promise<ReceiptImportOutcome>;
  /** Post the `landed` letter (default `api.missionPost`) once the import confirmed. */
  postMission: (letter: MissionLetter) => Promise<PostOutcome>;
  /** ISO clock for `updated_at`, injectable for a deterministic test. */
  now?: () => string;
}

/** The honest toast a land attempt reduces to (the `ReconcileToast` sibling, §6-F2.5d). */
export type LandToastKind = 'ok' | 'stale' | 'conflict' | 'readonly' | 'error' | 'no_candidate';
export interface LandToast {
  kind: LandToastKind;
  text: string;
}

/** The outcome of a human-landing attempt. `landed` is true ONLY when the receipt
 *  imported AND the `landed` letter posted. `shouldReload` refreshes the tray (a
 *  success re-renders the head; a conflict — or a post-failure after the store already
 *  bumped — reloads on the fresh truth; a `stale_scope` leaves the `merge_wait` card
 *  as-is: nothing landed, the human must re-run the gate). */
export interface LandOutcome {
  landed: boolean;
  toast: LandToast;
  shouldReload: boolean;
  /** The `landed` letter that was (or would have been) posted — for assertion/debug. */
  letter?: MissionLetter;
}

/** Best-effort human string from an unknown thrown error — the `ApiError.detail` the
 *  owner emits, else `.message`, else the stringified error. Duck-typed so the heart
 *  stays dependency-free (mirrors buildMap's private `errorText`). */
function landErrorText(err: unknown): string {
  if (err && typeof err === 'object') {
    const o = err as { detail?: unknown; message?: unknown };
    if (typeof o.detail === 'string' && o.detail.length > 0) return o.detail;
    if (typeof o.message === 'string' && o.message.length > 0) return o.message;
  }
  return String(err);
}

/**
 * Classify a failed receipt import into an honest toast (§6-F2.5d), grounded in the
 * owner's REAL error strings (system_blocks.rs `SeedError`):
 *  - `stale_scope` — the boundary moved since the gate ran; the evidence is stale and
 *    must be RE-EARNED. This is the anti-poison law working — NEVER "update the scope
 *    to pass". No reload (nothing changed; the human must re-run the gate).
 *  - OCC `conflict` — the store moved between our snapshot read and the import; reload
 *    the tray so the next attempt keys off the fresh version (the F3b pattern).
 *  - read-only owner — land from a writable session.
 * Anything else surfaces the owner's message verbatim (never swallowed).
 */
export function landErrorToast(err: unknown): { toast: LandToast; shouldReload: boolean } {
  const s = landErrorText(err);
  if (/stale_scope/i.test(s)) {
    return {
      toast: { kind: 'stale', text: 'the boundary moved since the gate ran — re-run the gate' },
      shouldReload: false,
    };
  }
  if (/conflict/i.test(s)) {
    return {
      toast: { kind: 'conflict', text: 'the store moved — reloading, then re-run the import' },
      shouldReload: true,
    };
  }
  if (/read[-\s]?only/i.test(s)) {
    return {
      toast: { kind: 'readonly', text: 'this owner is read-only — land from a writable session' },
      shouldReload: false,
    };
  }
  return { toast: { kind: 'error', text: s }, shouldReload: false };
}

/**
 * The human landing (§6-F2.5d): turn a `merge_wait` head's `receipt_candidate` into a
 * confirmed `landed` letter in ONE explicit human gesture. Pure over its injected
 * seams (the `runReconcile` pattern), so the happy path and every honest refusal are
 * unit-proven with a mocked client — no DOM, no network.
 *
 * The flow (spec §6-F2.5d B):
 *  1. Read a FRESH snapshot → the `expected_store_version` + the block's CURRENT
 *     resolution (`membership_fingerprint`).
 *  2. Assemble the COMPLETE receipt from the candidate: the candidate's TYPE + EVIDENCE
 *     + the candidate's SCOPE VERSIONS (`boundary_version`/`contract_version`). The
 *     evidence was earned against THAT boundary, so the scope is NEVER re-dated to the
 *     fresh snapshot to force a pass — a moved boundary MUST re-earn (step 3). Only the
 *     `block_id` + `resolution_hash` bind at import time (mission_letter.rs §F2.5d); the
 *     emitter is the named human-tray gesture, the validity stales on boundary/member.
 *  3. `receipt_import` with the fresh `expected_store_version`. A `stale_scope` (the
 *     boundary moved since the gate) → honest toast, NOTHING landed (the law working). A
 *     `conflict` → reload. Never a silent retry, never a scope rewrite.
 *  4. On success, post the `landed` letter (§1d: `receipt.imported == true` with the
 *     real `store_version`): seq = head.seq + 1, prev = head_letter_id (§1e chain),
 *     inheriting the head's identity + gate (so the §3f provenance chain stays whole).
 */
export async function landCandidate(head: MissionHead, deps: LandDeps): Promise<LandOutcome> {
  const letter = head.head;
  const candidate = letter.receipt_candidate;
  if (!candidate) {
    // No candidate attached — the tray never offers the button here; a defensive guard
    // so a stale click can never fabricate a receipt (§1d honesty). No engine call.
    return {
      landed: false,
      toast: { kind: 'no_candidate', text: 'gate green — no candidate attached' },
      shouldReload: false,
    };
  }

  // 1. Fresh snapshot → expected_store_version + the block's current resolution.
  let snap: SystemBlocksSnapshot;
  try {
    snap = await deps.snapshot();
  } catch (err) {
    return { landed: false, ...landErrorToast(err) };
  }
  const storeVersion = snap.store?.store_version ?? snap.store_version;
  const block = snap.store?.blocks.find((b) => b.block_id === candidate.block_id);
  if (!snap.present || storeVersion == null || !block) {
    return {
      landed: false,
      toast: { kind: 'error', text: 'the block is not in the current store — re-run the gate' },
      shouldReload: false,
    };
  }

  // 2. Assemble the complete receipt. The scope's version anchors come from the
  //    CANDIDATE (never re-dated); only block_id + resolution_hash bind at import time.
  const receipt: Receipt = {
    type: candidate.type,
    emitter: { kind: 'verb', id: 'human-tray-landing' },
    scope: {
      block_id: candidate.block_id,
      boundary_version: candidate.scope.boundary_version,
      contract_version: candidate.scope.contract_version,
      resolution_hash: block.membership_fingerprint ?? '',
    },
    evidence: candidate.evidence,
    validity: { expires_on: null, stales_on: ['boundary_change', 'member_change'] },
  };

  // 3. Import — the anti-poison gate. A stale scope / conflict is the LAW working.
  let outcome: ReceiptImportOutcome;
  try {
    outcome = await deps.importReceipt({
      expectedStoreVersion: storeVersion,
      blockId: candidate.block_id,
      receipt,
    });
  } catch (err) {
    return { landed: false, ...landErrorToast(err) };
  }

  // 4. Post the landed letter (§1d/§1e). Inherit the head's identity + gate; the
  //    receipt anchor names the store_version the import produced.
  const nowIso = deps.now?.() ?? new Date().toISOString();
  const landedLetter: MissionLetter = {
    schema: MISSION_LETTER_SCHEMA,
    mission_id: letter.mission_id,
    mission_seq: letter.mission_seq + 1,
    prev_letter_id: head.head_letter_id,
    block_id: letter.block_id,
    brain_ref: letter.brain_ref,
    seat: letter.seat,
    ...(letter.runner_id ? { runner_id: letter.runner_id } : {}),
    capability: letter.capability,
    phase: 'landed',
    ...(letter.gate ? { gate: letter.gate } : {}),
    receipt: { imported: true, store_version: outcome.store_version },
    ...(letter.packet_ref ? { packet_ref: letter.packet_ref } : {}),
    tokens_total: letter.tokens_total,
    started_at: letter.started_at,
    updated_at: nowIso,
  };

  try {
    await deps.postMission(landedLetter);
  } catch (err) {
    // The import DID land (the store bumped) but the letter post failed — reload on the
    // fresh truth and surface the honest reason; never claim a landing we could not post.
    const { toast } = landErrorToast(err);
    return { landed: false, toast, shouldReload: true, letter: landedLetter };
  }

  return {
    landed: true,
    toast: { kind: 'ok', text: `receipt landed — store v${outcome.store_version}` },
    shouldReload: true,
    letter: landedLetter,
  };
}
