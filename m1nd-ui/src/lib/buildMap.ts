/*
 * buildMap — the Build Map's wire types + the PURE rollup/layout policy
 * (HUMAN-VIEW-V2). This is F1: read-only. NOTHING here mutates; the render is a
 * projection of the ratified skeleton the `system_blocks_snapshot` verb serves.
 *
 * The types are transcribed from the Rust `SystemBlockStore` serialization
 * (m1nd-mcp/src/system_blocks.rs) as the `system_blocks_snapshot` handler returns
 * it: `{ present, store_version, block_count, store }`. The captured fixture in
 * src/__fixtures__/system_blocks_snapshot.json is the ground truth these match.
 *
 * The rollup implements PRD §5 to the letter: block color is a WRITTEN POLICY,
 * never a color average. Required receipts earned-fresh vs the block's own
 * declared contract decide evidence vs needs; a failing receipt or a broken
 * socket is broken; a block with no contract abstains (unknown). Absence is
 * NEUTRAL — an unpainted member or a missing receipt is "not scanned yet"/"needs
 * evidence", never a fabricated green and never an alarm.
 */

// ---------------------------------------------------------------------------
// Wire types — the `system_blocks_snapshot` shape (Rust `SystemBlockStore`).
// ---------------------------------------------------------------------------

export type ReceiptTypeName = 'test' | 'structural' | 'runtime' | 'review' | 'handoff' | 'spec';
export type MembershipRole = 'primary' | 'shared' | 'generated' | 'test' | 'docs' | 'external_socket';
export type SystemBlockKind = 'scanned' | 'planned';
export type SystemBlockStateName =
  | 'candidate'
  | 'planned'
  | 'building'
  | 'scanned'
  | 'ratified'
  | 'drifted'
  | 'archived'
  | 'restored';
export type MembershipSource = 'ratified' | 'proposed' | 'manual';
export type SkeletonStateName = 'candidate' | 'ratified';
/** Who named a candidate block (F0c §3a / F11 o6). The naming-runner is opt-in;
 *  the heuristic always works offline and marks the label provisional; `owner` is
 *  the strongest label — a human touch through Edit Names & Boundaries. */
export type NamedBy = 'owner' | 'runner' | 'heuristic';

export interface MembershipEntry {
  path: string;
  role: MembershipRole;
  optional?: boolean;
}

export interface ReceiptRequirement {
  type: ReceiptTypeName;
  stales_on?: string[];
}

export interface Socket {
  to?: string;
  type?: string;
  alias?: string;
  class?: string;
}

export interface Sockets {
  inputs: Socket[];
  outputs: Socket[];
  external: Socket[];
}

export interface ReceiptContract {
  version: number;
  required: ReceiptRequirement[];
  optional: ReceiptRequirement[];
  waived: ReceiptRequirement[];
  declared_by: string | null;
  declared_at: string | null;
}

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

export interface ReceiptScope {
  block_id: string;
  boundary_version: number;
  contract_version: number;
  resolution_hash: string;
}

export interface ReceiptEmitter {
  kind: 'ci' | 'runnerd' | 'verb' | 'owner';
  id: string;
}

export interface ReceiptValidity {
  expires_on: string | null;
  stales_on: string[];
}

export interface Receipt {
  type: ReceiptTypeName;
  emitter: ReceiptEmitter;
  scope: ReceiptScope;
  evidence: ReceiptEvidence;
  validity: ReceiptValidity;
}

export interface Layout {
  x: number | null;
  y: number | null;
  locked: boolean;
  algorithm_seed: unknown;
  version: number;
}

/** F0c candidate confidence — COMPONENTS, not a single vibe score (objection 8).
 *  Transcribed 1:1 from the Rust `CandidateMeta` (m1nd-mcp/src/system_blocks.rs) as
 *  the `skeleton_candidate` scan serializes it onto each proposed block. Present ONLY
 *  on a candidate block; `undefined` on every ratified/hand-authored block. Every
 *  field is honest: `graph_cohesion` is `undefined` (never faked) when the block saw
 *  fewer than the declared edge floor (a docs/no-edge block). */
export interface CandidateMeta {
  named_by: NamedBy;
  /** True while the label is a provisional heuristic — the card renders it muted
   *  ("unnamed — needs you") and it cannot be ratified without an owner touch (§5). */
  needs_owner_naming: boolean;
  /** Fraction of the block's edges that stay INSIDE it. `undefined` when
   *  `edge_sample_size` is below the floor — a docs block does not fabricate it. */
  graph_cohesion?: number;
  /** How many edges touched the block — the honest denominator behind cohesion. */
  edge_sample_size: number;
  /** How directory-aligned the block is: members-under-its-dir / repo-files-under-its-dir. */
  directory_support: number;
  /** Members backed by a real graph node / total members. */
  coverage_ratio: number;
  /** How many members carry `role:"shared"` (a multi-owner seam surfaced, §2a). */
  shared_member_count: number;
}

export interface SystemBlock {
  block_id: string;
  name: string;
  purpose: string;
  kind: SystemBlockKind;
  state: SystemBlockStateName;
  boundary_version: number;
  contract_version: number;
  membership_source: MembershipSource;
  membership: MembershipEntry[];
  sockets: Sockets;
  receipt_contract: ReceiptContract;
  receipts: Receipt[];
  layout: Layout;
  unmapped_residue: string[];
  /** Slice 3 reconcile baseline — the sha256 of the block's effective resolved
   *  membership. `undefined` until the first reconcile writes the honest baseline;
   *  its PRESENCE is how the UI knows a block has ever been reconciled. Optional so
   *  a pre-Slice-3 store (which omits it) still parses (retrocompat honesta). */
  membership_fingerprint?: string;
  /** Slice 3 reconcile cache — the ordered effective membership the fingerprint was
   *  taken over. Optional + omitted-when-empty, mirroring the Rust serde. */
  resolved_members?: string[];
  /** F0c candidate scores — present ONLY on a block a `skeleton_candidate` scan
   *  proposed; `undefined` on every ratified/hand-authored block (the Rust serde
   *  skips it when `None`), so a pre-F0c store parses byte-clean. */
  candidate_meta?: CandidateMeta;
}

export interface Skeleton {
  skeleton_id: string;
  version: number;
  state: SkeletonStateName;
  ratification: {
    method: string;
    ratifier: string;
    ratified_at: string;
    commit: string;
  };
}

export interface UnmappedPolicy {
  visible: boolean;
  default_action: string;
}

export interface SystemBlockStore {
  schema: string;
  store_version: number;
  skeleton: Skeleton;
  blocks: SystemBlock[];
  unmapped_policy: UnmappedPolicy;
  /** Slice 3 reconcile output — the REAL unmapped: repo files claimed by NO block
   *  (F7). Materialized capped (UNMAPPED_FILES_CAP=500 on the owner); the honest
   *  full count is `unmapped_total`. Optional + omitted-when-empty, mirroring the
   *  Rust serde — a pre-Slice-3 store parses byte-clean. */
  unmapped_files?: string[];
  /** The honest TOTAL of unmapped files, even when `unmapped_files` was capped.
   *  Omitted (undefined) while zero on the wire — a reconciled store with zero
   *  unmapped is told apart from a never-reconciled one by block fingerprints, not
   *  by this field (see `rollupStore`). */
  unmapped_total?: number;
  /** F11-a advisory curation lease (o4): the agent currently curating this
   *  candidate. NEVER blocks the owner — the screen only surfaces a banner.
   *  Omitted on the wire while free (serde skip), like the Rust store. */
  curating_by?: string;
  /** When the advisory lease EXPIRES (RFC3339 UTC). An expired lease renders no
   *  banner — it is reclaimable, never a dead-agent trap. */
  curating_until?: string;
}

/** The `system_blocks_snapshot` envelope. `present:false` carries `honest`. */
export interface SystemBlocksSnapshot {
  present: boolean;
  store_version?: number;
  block_count?: number;
  store?: SystemBlockStore;
  honest?: string;
}

// ---------------------------------------------------------------------------
// The `system_blocks_reconcile` report (Slice 3). Transcribed 1:1 from the Rust
// `ReconcileReport` + `BlockReconcile` (m1nd-mcp/src/system_blocks.rs) as the
// `handle_system_blocks_reconcile` handler serializes it — the handler MERGES
// `store_version` + `file_count` onto the report before it goes over the wire.
// Fields the Rust side skips-when-empty (`added`/`removed`/`missing`/
// `bumped_block_ids`) or skips-when-none (`note`) are optional here.
// ---------------------------------------------------------------------------

export type ReconcileOutcome = 'baseline' | 'bumped' | 'unchanged';

export interface BlockReconcile {
  block_id: string;
  outcome: ReconcileOutcome;
  /** The block's `boundary_version` AFTER this pass. */
  boundary_version: number;
  /** How many real files the block now resolves to. */
  resolved_count: number;
  /** Files that entered the block's resolved set (only on `bumped`). */
  added?: string[];
  /** Files that left the block's resolved set (only on `bumped`). */
  removed?: string[];
  /** Declared EXACT members absent from the file list ("declared but gone"). */
  missing?: string[];
}

export interface ReconcileReport {
  /** True iff this reconcile changed persisted state (baseline write, boundary
   *  bump, or a change in the unmapped set). A no-op reconcile is `false`. */
  dirty: boolean;
  blocks: BlockReconcile[];
  /** Ids of blocks whose `boundary_version` bumped this pass (skip-empty). */
  bumped_block_ids?: string[];
  /** The honest TOTAL count of files claimed by no block (never capped). */
  unmapped_total: number;
  /** How many unmapped paths were materialized into the store (≤ the cap). */
  unmapped_materialized: number;
  /** The honest staleness note — present iff at least one boundary bumped. */
  note?: string;
  /** Merged onto the report by the handler: the store version AFTER the pass. */
  store_version: number;
  /** Merged onto the report by the handler: how many files were reconciled. */
  file_count: number;
}

// ---------------------------------------------------------------------------
// The state grammar (PRD §5). Copy law: no "proven/done/correct" as a state.
// ---------------------------------------------------------------------------

export type BlockState = 'evidence-backed' | 'needs-evidence' | 'broken' | 'unknown';

/** The operator-language label for a state (PRD §13 copy law). Absence is neutral
 *  — "not scanned yet", never "amber/warning". */
export const STATE_LABEL: Record<BlockState, string> = {
  'evidence-backed': 'evidence-backed',
  'needs-evidence': 'needs evidence',
  broken: 'broken',
  unknown: 'not scanned yet',
};

// ---------------------------------------------------------------------------
// The rollup (PRD §5) — a written policy, never a color average.
// ---------------------------------------------------------------------------

/** A test/CI receipt whose recorded exit_status is non-zero is a FAILING receipt
 *  (a broken signal), not fresh evidence. */
export function isFailingReceipt(receipt: Receipt): boolean {
  return typeof receipt.evidence.exit_status === 'number' && receipt.evidence.exit_status !== 0;
}

/** The reason a receipt is stale, in the backend's evaluation order. Mirrors the
 *  Rust `receipt_stale_reason` (system_blocks.rs) 1:1: `block` | `boundary` |
 *  `contract` | `expired`. */
export type StaleReason = 'block' | 'boundary' | 'contract' | 'expired';
export type ReceiptFreshness = { fresh: true } | { fresh: false; reason: StaleReason };

/**
 * receiptFreshness (Slice 3) — per-receipt fresh/stale{reason} for DISPLAY, pure
 * and testable. A receipt is FRESH iff its scope still binds to the block's CURRENT
 * `(block_id, boundary_version, contract_version)` AND it has not expired — exactly
 * the `receipt_recompute` truth the owner computes (m1nd-mcp/src/system_blocks.rs).
 * This is the freshness AXIS only; a failing receipt is a separate BROKEN axis (see
 * `isFailingReceipt`), never conflated with staleness.
 */
export function receiptFreshness(
  receipt: Receipt,
  block: SystemBlock,
  now: number = Date.now(),
): ReceiptFreshness {
  if (receipt.scope.block_id !== block.block_id) return { fresh: false, reason: 'block' };
  if (receipt.scope.boundary_version !== block.boundary_version) return { fresh: false, reason: 'boundary' };
  if (receipt.scope.contract_version !== block.contract_version) return { fresh: false, reason: 'contract' };
  if (receipt.validity.expires_on != null) {
    const exp = Date.parse(receipt.validity.expires_on);
    if (Number.isFinite(exp) && exp <= now) return { fresh: false, reason: 'expired' };
  }
  return { fresh: true };
}

/**
 * Earned-fresh (PRD §3.1/§5, MVP): a receipt counts for a block only when it is
 * fresh by scope + expiry ([`receiptFreshness`]) AND it is not a failing receipt.
 * The freshness half is shared with the display path so the two can never drift.
 */
export function isEarnedFresh(receipt: Receipt, block: SystemBlock, now: number): boolean {
  if (!receiptFreshness(receipt, block, now).fresh) return false;
  if (isFailingReceipt(receipt)) return false;
  return true;
}

export interface BlockRollup {
  blockId: string;
  state: BlockState;
  /** The required receipt TYPES the block's contract declares (the denominator). */
  requiredTypes: ReceiptTypeName[];
  /** The required types covered by an earned-fresh receipt (the numerator). */
  earnedTypes: ReceiptTypeName[];
  /** M — distinct required types earned-fresh. */
  receiptsEarned: number;
  /** N — required types declared by the contract. The auditable denominator. */
  receiptsRequired: number;
  /** Has at least one declared socket (in/out/external). Border truth, separate
   *  from fill — a block with holes is NEVER hidden. */
  wired: boolean;
  /** The block is not ratified yet — rendered dashed, never mistakable for ratified. */
  candidate: boolean;
  /** Honest reasons the block is broken (failing receipt, broken socket). */
  brokenReasons: string[];
  /** Slice 3: the block has been reconciled (a `membership_fingerprint` baseline
   *  exists) AND carries at least one receipt earned against an OLDER boundary
   *  (`scope.boundary_version < block.boundary_version`) — its evidence predates
   *  the current membership. Drives the card's `⚠ boundary vN` badge. */
  boundaryStale: boolean;
}

/**
 * Roll a single block up to its color (PRD §5). Order is a written policy:
 *  1. a failing receipt or a broken (dangling) socket → broken (clay);
 *  2. no required contract at all → unknown (the engine abstains, grey);
 *  3. every required type earned-fresh → evidence-backed (sage);
 *  4. otherwise → needs evidence (ochre) — the honest day-1 state.
 * `knownBlockNames` are the block NAMES sockets may target; a `to` outside it is a
 * broken socket. `memberStates` (from the graph snapshot's xray tags) is honored
 * when present — an erosion-candidate/broken member pushes to broken — but ABSENCE
 * is neutral ("not scanned"), never a downgrade (today every member is unpainted).
 */
export function rollupBlock(
  block: SystemBlock,
  knownBlockNames: Set<string>,
  memberStates: Map<string, 'broken' | 'erosion' | 'ok'> = new Map(),
  now: number = Date.now(),
): BlockRollup {
  const requiredTypes = block.receipt_contract.required.map((r) => r.type);
  const N = requiredTypes.length;
  const earnedTypes = requiredTypes.filter((t) =>
    block.receipts.some((r) => r.type === t && isEarnedFresh(r, block, now)),
  );
  const M = earnedTypes.length;
  const wired =
    block.sockets.inputs.length + block.sockets.outputs.length + block.sockets.external.length > 0;
  const candidate = block.state !== 'ratified';

  const brokenReasons: string[] = [];
  for (const r of block.receipts) {
    if (isFailingReceipt(r)) brokenReasons.push(`failing ${r.type} receipt`);
  }
  for (const s of block.sockets.outputs) {
    if (s.to != null && !knownBlockNames.has(s.to)) brokenReasons.push(`broken socket → ${s.to}`);
  }
  for (const entry of block.membership) {
    const st = memberStates.get(entry.path);
    if (st === 'broken' || st === 'erosion') brokenReasons.push(`${st} member ${entry.path}`);
  }

  let state: BlockState;
  if (brokenReasons.length > 0) state = 'broken';
  else if (N === 0) state = 'unknown';
  else if (M === N) state = 'evidence-backed';
  else state = 'needs-evidence';

  // Boundary-moved evidence (Slice 3): only for a reconciled block (fingerprint
  // present), and only when a receipt was earned against an OLDER boundary — the
  // exact `stale_scope` the reconcile bump creates. A never-reconciled block never
  // shows the badge (absence is neutral).
  const boundaryStale =
    block.membership_fingerprint != null &&
    block.receipts.some(
      (r) => r.scope.block_id === block.block_id && r.scope.boundary_version < block.boundary_version,
    );

  return {
    blockId: block.block_id,
    state,
    requiredTypes,
    earnedTypes,
    receiptsEarned: M,
    receiptsRequired: N,
    wired,
    candidate,
    brokenReasons,
    boundaryStale,
  };
}

export interface StateCounts {
  'evidence-backed': number;
  'needs-evidence': number;
  broken: number;
  unknown: number;
  /** Planned blocks are counted apart (a contract, not code) — never mixed into
   *  the four scanned-state buckets. */
  planned: number;
}

export interface MapRollup {
  rollups: Map<string, BlockRollup>;
  counts: StateCounts;
  /** Aggregate declared unmapped residue across blocks (the seed's own field —
   *  kept for continuity; the tray now shows the reconcile truth below). */
  unmappedCount: number;
  /** The whole skeleton is a candidate (first-run, F6) — every card dashed + banner. */
  candidate: boolean;
  /** Slice 3: has this store EVER been reconciled? True iff any block carries a
   *  `membership_fingerprint` baseline. This is what tells "reconciled with zero
   *  unmapped" (an honest `0 files`) apart from "never reconciled" (neutral
   *  absence) — `unmapped_total` is omitted-when-zero on the wire, so it cannot. */
  reconciled: boolean;
  /** The REAL unmapped total from the reconcile (`store.unmapped_total`), 0 when
   *  absent. Meaningful only when `reconciled`. */
  unmappedTotal: number;
  /** The materialized unmapped sample (`store.unmapped_files`, capped on the owner)
   *  — the honest list the tray expands. Its length ≤ `unmappedTotal`. */
  unmappedFiles: string[];
}

/** Roll the whole store up: per-block rollups + System Health counts + the
 *  unmapped total + the first-run candidate flag. `memberStates` (from the graph
 *  snapshot's persisted `xray:state:*` tags, keyed by repo-relative path) is
 *  honored when present; absence is neutral ("not scanned"), the day-1 truth. */
export function rollupStore(
  store: SystemBlockStore,
  memberStates: Map<string, 'broken' | 'erosion' | 'ok'> = new Map(),
  now: number = Date.now(),
): MapRollup {
  const names = new Set(store.blocks.map((b) => b.name));
  const rollups = new Map<string, BlockRollup>();
  const counts: StateCounts = {
    'evidence-backed': 0,
    'needs-evidence': 0,
    broken: 0,
    unknown: 0,
    planned: 0,
  };
  for (const b of store.blocks) {
    const r = rollupBlock(b, names, memberStates, now);
    rollups.set(b.block_id, r);
    if (b.kind === 'planned') counts.planned += 1;
    else counts[r.state] += 1;
  }
  const unmappedCount = store.blocks.reduce((n, b) => n + b.unmapped_residue.length, 0);
  const candidate = store.skeleton.state !== 'ratified';
  // The reconcile truth (Slice 3): a store is reconciled once any block has a
  // fingerprint baseline. The unmapped total/files come straight from the store's
  // reconcile output (undefined on a pre-Slice-3 store → 0/[], the neutral day-1).
  const reconciled = store.blocks.some((b) => b.membership_fingerprint != null);
  const unmappedTotal = store.unmapped_total ?? 0;
  const unmappedFiles = store.unmapped_files ?? [];
  return { rollups, counts, unmappedCount, candidate, reconciled, unmappedTotal, unmappedFiles };
}

// ---------------------------------------------------------------------------
// Deterministic layout (F0-TECH §7) — same block → same place across renders.
// ---------------------------------------------------------------------------

export interface Point {
  x: number;
  y: number;
}

export const CARD_W = 264;
export const CARD_H = 138;
export const GAP_X = 60;
export const GAP_Y = 52;
export const PAD = 32;
export const COLS = 3;

/** First-render deterministic positions: a stable grid in the seed's block order
 *  (3 columns). Same order → same coordinates, every render (F0-TECH §7). */
export function gridLayout(count: number, cols: number = COLS): Point[] {
  const pts: Point[] = [];
  for (let i = 0; i < count; i += 1) {
    const col = i % cols;
    const row = Math.floor(i / cols);
    pts.push({ x: PAD + col * (CARD_W + GAP_X), y: PAD + row * (CARD_H + GAP_Y) });
  }
  return pts;
}

/** The canvas extent for `count` blocks in `cols` columns (drives scroll/pan). */
export function canvasSize(count: number, cols: number = COLS): { width: number; height: number } {
  const rows = Math.max(1, Math.ceil(count / cols));
  const usedCols = Math.max(1, Math.min(count, cols));
  return {
    width: PAD * 2 + usedCols * CARD_W + (usedCols - 1) * GAP_X,
    height: PAD * 2 + rows * CARD_H + (rows - 1) * GAP_Y,
  };
}

/** name → grid index, for resolving a socket's `to` (a block NAME) to a card. */
export function blockIndexByName(store: SystemBlockStore): Map<string, number> {
  const m = new Map<string, number>();
  store.blocks.forEach((b, i) => m.set(b.name, i));
  return m;
}

// ---------------------------------------------------------------------------
// Small derivations (pure, testable).
// ---------------------------------------------------------------------------

/** The repo id embedded in a skeleton id — the seed-import form
 *  (`sk_<repo>_seed_<yyyy>_<mm>`) AND the scan's candidate form
 *  (`sk_<repo>_candidate`, skeleton_scan.rs). NOTE: this is the SANITIZED slug
 *  (lowercase, `_`-separated), a display token — never the identity a mission
 *  letter's `brain_ref` must carry (that is [`brainRefFor`]). */
export function repoIdFromSkeletonId(skeletonId: string): string | null {
  const m = skeletonId.match(/^sk_(.+?)_seed_/) ?? skeletonId.match(/^sk_(.+?)_candidate$/);
  return m ? m[1] : null;
}

/** The §1f `brain_ref` a mission letter must carry: the brain's display name —
 *  the BASENAME of its project root, exactly the identity the owner's
 *  `mission_post` brain guard compares against (mission_letter_handlers.rs;
 *  case-sensitive, hyphens intact). A hosted map knows its root (the `?brain=`
 *  selector); the bound map (root = null) falls back to the skeleton's embedded
 *  repo id — right whenever the repo basename IS the slug (e.g. "m1nd"). Never
 *  an absolute path (§1f). Field bug 2026-07-10: deriving the ref from the
 *  skeleton id sent a sanitized slug (or "brain" on candidate-form ids) and the
 *  hosted brain refused the curation letter with brain_mismatch. */
export function brainRefFor(brainRoot: string | null | undefined, repoId: string | null): string {
  const base = brainRoot
    ?.trim()
    .replace(/[\\/]+$/, '')
    .split(/[\\/]/)
    .pop();
  return base || repoId || 'brain';
}

/**
 * The block's compact domain tag, derived from its stable `block_id`
 * (`sb_<repo>_<slug>`): strip the `sb_` and repo prefix, take the first two slug
 * tokens, upper-case. `sb_m1nd_core_graph_kernel` → `CORE GRAPH`. Deterministic
 * and stable across renders.
 */
export function domainTag(blockId: string, repoId: string | null): string {
  let slug = blockId.replace(/^sb_/, '');
  if (repoId && slug.startsWith(`${repoId}_`)) slug = slug.slice(repoId.length + 1);
  const tokens = slug.split('_').filter(Boolean);
  return tokens.slice(0, 2).join(' ').toUpperCase();
}

/** Membership counts grouped by role (for the block panel), in first-seen order. */
export function membershipByRole(block: SystemBlock): Array<{ role: MembershipRole; count: number }> {
  const order: MembershipRole[] = [];
  const counts = new Map<MembershipRole, number>();
  for (const e of block.membership) {
    if (!counts.has(e.role)) order.push(e.role);
    counts.set(e.role, (counts.get(e.role) ?? 0) + 1);
  }
  return order.map((role) => ({ role, count: counts.get(role) ?? 0 }));
}

// ---------------------------------------------------------------------------
// The reconcile gesture's view-model (Slice 3, F3b) — pure + testable, so the
// BuildMapView wire stays thin and the honest copy has a unit-tested home.
// ---------------------------------------------------------------------------

/** The one-line human summary of a reconcile report (the toast body):
 *  "2 boundaries moved · 5 unmapped · store v7". */
export function reconcileSummary(report: ReconcileReport): string {
  const moved = report.bumped_block_ids?.length ?? 0;
  const boundaries = `${moved} ${moved === 1 ? 'boundary' : 'boundaries'} moved`;
  return `${boundaries} · ${report.unmapped_total} unmapped · store v${report.store_version}`;
}

export type ReconcileToastKind = 'ok' | 'conflict' | 'readonly' | 'error';
export interface ReconcileToast {
  kind: ReconcileToastKind;
  text: string;
}

/** Best-effort human string from an unknown error — the `ApiError.detail` the
 *  owner emits, else `.message`, else the stringified error. No `ApiError` import:
 *  duck-typed so this stays dependency-free and testable. */
function errorText(err: unknown): string {
  if (err && typeof err === 'object') {
    const o = err as { detail?: unknown; message?: unknown };
    if (typeof o.detail === 'string' && o.detail.length > 0) return o.detail;
    if (typeof o.message === 'string' && o.message.length > 0) return o.message;
  }
  return String(err);
}

/**
 * Classify a failed reconcile into an honest toast (F3b §D). The two named cases
 * are grounded in the owner's real error strings:
 *  - OCC conflict — "store version conflict: expected N, actual M …" (system_blocks.rs
 *    `SeedError::Conflict`) → reload, never a silent retry;
 *  - read-only owner — "m1nd is attached read-only (--read-only); mutation tool … is
 *    disabled …" (server.rs) → informative, the button stays.
 * Anything else surfaces the owner's message verbatim (never swallowed).
 */
export function reconcileErrorToast(err: unknown, expectedVersion: number): ReconcileToast {
  const s = errorText(err);
  if (/conflict/i.test(s)) {
    const m = s.match(/actual\s+(\d+)/i);
    const actual = m ? m[1] : '?';
    return {
      kind: 'conflict',
      text: `the store moved (expected v${expectedVersion}, actual v${actual}) — reloading`,
    };
  }
  if (/read[-\s]?only/i.test(s)) {
    return { kind: 'readonly', text: 'this owner is read-only — reconcile from a writable session' };
  }
  return { kind: 'error', text: s };
}

/**
 * Run one reconcile and reduce it to a toast + a reload decision (F3b §D). Pure
 * over its injected `reconcileFn`, so the conflict/read-only/success flows are
 * unit-testable with a mocked client (no DOM, no network). A conflict reloads (the
 * store moved); success reloads (the map re-renders on the new truth); a read-only
 * or error refusal does NOT reload — nothing changed.
 */
export async function runReconcile(
  reconcileFn: (expectedVersion: number) => Promise<ReconcileReport>,
  expectedVersion: number,
): Promise<{ toast: ReconcileToast; shouldReload: boolean }> {
  try {
    const report = await reconcileFn(expectedVersion);
    return { toast: { kind: 'ok', text: reconcileSummary(report) }, shouldReload: true };
  } catch (err) {
    const toast = reconcileErrorToast(err, expectedVersion);
    return { toast, shouldReload: toast.kind === 'conflict' };
  }
}

// ---------------------------------------------------------------------------
// F0c — the scan (`skeleton_candidate`) + the Review-&-ratify walk. Types are
// transcribed 1:1 from the Rust `handle_skeleton_candidate` result + the
// `skeleton_scan::SkeletonScanReport` (m1nd-mcp/src/skeleton_scan.rs) and the
// `system_blocks_ratify` result (m1nd-mcp/src/system_blocks_handlers.rs). The
// gesture is REVIEW, not blanket ratify (amendment §5, objection 10): the owner
// accepts each provisional name; "Ratify all" unlocks ONLY when every candidate
// block is owner-accepted AND no shared-member seam is unresolved.
// ---------------------------------------------------------------------------

/** A member claimed by more than one block (the PRD §6 seam surface). */
export interface MultiOwnerSeam {
  path: string;
  communities: number[];
  block_ids: string[];
}

/** Per-block line of the scan report (mirrors the Rust `CandidateBlockReport`). */
export interface CandidateBlockReport {
  block_id: string;
  name: string;
  member_count: number;
  membership_count: number;
  dominant_directory: string;
  candidate_meta: CandidateMeta;
}

/** The naming stage's honest note (runner vs offline heuristic). */
export interface NamingReport {
  requested: string;
  applied: string;
  runner_available: boolean;
  note: string;
}

/** The `skeleton_candidate` scan report — the honest census behind the candidate
 *  map. `multi_owner_seams`/`unmapped` are omitted-when-empty on the wire. */
export interface SkeletonScanReport {
  algorithm: string;
  repo_file_count: number;
  graph_node_count: number;
  graph_edge_count: number;
  block_count: number;
  claimed_file_count: number;
  coverage_ratio: number;
  graph_cohesion_edge_floor: number;
  multi_owner_seams?: MultiOwnerSeam[];
  unmapped_total: number;
  unmapped?: string[];
  blocks: CandidateBlockReport[];
  naming: NamingReport;
}

/** Which transaction the scan applied (amendment §1). */
export type SkeletonCandidateTransactionState =
  | 'created_candidate_store'
  | 'replaced_candidate_store'
  | 'wrote_candidate_revision';

/** The `skeleton_candidate` result envelope. `candidate_seed` is the full written
 *  seed; the map re-reads the truth via `system_blocks_snapshot`, so it is not
 *  rendered directly (typed `unknown` — the report + version drive the toast). */
export interface SkeletonCandidateResult {
  present: boolean;
  store_version: number;
  transaction_state: SkeletonCandidateTransactionState;
  candidate_revision_written: boolean;
  block_count: number;
  review_limit: number;
  candidate_seed: unknown;
  report: SkeletonScanReport;
}

/** The `system_blocks_ratify` result envelope. */
export interface RatifyResult {
  store_version: number;
  ratified_block_ids: string[];
  skeleton_state: string;
  ratifier: string;
  ratified_at: string;
}

/** The shared honest write toast (scan / ratify reuse the reconcile toast shape —
 *  a tinted one-line message, same four kinds). */
export type WriteToastKind = ReconcileToastKind;
export type WriteToast = ReconcileToast;

/** A candidate block's confidence, summarized for the chip AND broken into its
 *  honest components for the tooltip (§3b/§5 — the store keeps components; the UI
 *  may summarize, but never fabricates cohesion). `graph_cohesion` stays `null`
 *  when the backend saw fewer than the edge floor. `summaryPct` is a plain mean of
 *  the PRESENT components — a UI ordering/label aid, never a persisted score. */
export interface CandidateConfidence {
  summaryPct: number;
  components: Array<{ key: 'graph_cohesion' | 'directory_support' | 'coverage_ratio'; label: string; pct: number | null }>;
  sharedMemberCount: number;
  edgeSampleSize: number;
  namedBy: NamedBy;
  needsOwnerNaming: boolean;
}

const PCT = (fraction: number) => Math.round(fraction * 100);

/** Summarize a block's `CandidateMeta` for display. The summary is the mean of the
 *  PRESENT components only (a docs block with no cohesion is not dragged to 0). */
export function candidateConfidence(meta: CandidateMeta): CandidateConfidence {
  const components: CandidateConfidence['components'] = [
    { key: 'graph_cohesion', label: 'cohesion', pct: meta.graph_cohesion == null ? null : PCT(meta.graph_cohesion) },
    { key: 'directory_support', label: 'directory', pct: PCT(meta.directory_support) },
    { key: 'coverage_ratio', label: 'coverage', pct: PCT(meta.coverage_ratio) },
  ];
  const present = components.map((c) => c.pct).filter((p): p is number => p != null);
  const summaryPct = present.length > 0 ? Math.round(present.reduce((a, b) => a + b, 0) / present.length) : 0;
  return {
    summaryPct,
    components,
    sharedMemberCount: meta.shared_member_count,
    edgeSampleSize: meta.edge_sample_size,
    namedBy: meta.named_by,
    needsOwnerNaming: meta.needs_owner_naming,
  };
}

/** A block's support score in [0,1] for the review queue's low-support-first sort
 *  (§3b). The mean of its present confidence components; a block with no
 *  `candidate_meta` (never in a candidate store) sorts last. */
export function blockSupport(block: SystemBlock): number {
  const meta = block.candidate_meta;
  if (!meta) return Number.POSITIVE_INFINITY;
  return candidateConfidence(meta).summaryPct / 100;
}

/** A candidate block whose name is still a provisional heuristic — it cannot be
 *  ratified without an owner touch (§5). */
export function blockNeedsNaming(block: SystemBlock): boolean {
  return block.candidate_meta?.needs_owner_naming === true;
}

/** A candidate block carrying an UNRESOLVED multi-owner seam (a `role:"shared"`
 *  member). F0c-b ships no seam-resolution gesture (the full Edit-Names-&-Boundaries
 *  editor is a later slice), so any shared member is unresolved and blocks the
 *  blanket ratify. */
export function blockHasUnresolvedSeam(block: SystemBlock): boolean {
  return (block.candidate_meta?.shared_member_count ?? 0) > 0;
}

/** How many candidate blocks still carry an unresolved seam (the §5 blanket gate). */
export function unresolvedSeamCount(store: SystemBlockStore): number {
  return store.blocks.filter((b) => b.state === 'candidate' && blockHasUnresolvedSeam(b)).length;
}

export interface ReviewQueue {
  /** Every candidate (un-ratified) block, lowest-support first (block_id tie-break). */
  ordered: SystemBlock[];
  /** The honest total (never capped). */
  total: number;
  /** The review-queue page bound (default 16) — bounds the UI queue, NEVER the seed. */
  limit: number;
}

/** Build the review queue (§5/§7): the candidate blocks ordered lowest-support
 *  first, with the honest total and the `review_limit` page bound. `review_limit`
 *  bounds only the UI queue — the scan emitted every block; the queue pages beyond
 *  the limit with the true count. Pure + deterministic (stable block_id tie-break). */
export function reviewQueue(store: SystemBlockStore, reviewLimit = 16): ReviewQueue {
  const candidates = store.blocks.filter((b) => b.state === 'candidate');
  const ordered = [...candidates].sort((a, b) => {
    const sa = blockSupport(a);
    const sb = blockSupport(b);
    if (sa !== sb) return sa - sb;
    return a.block_id.localeCompare(b.block_id);
  });
  return { ordered, total: ordered.length, limit: Math.max(1, reviewLimit) };
}

/** The honest reason the blanket "Ratify all" is not offered yet, or `null` when it
 *  is ready (§5): every candidate block owner-accepted AND no unresolved seam. */
export function ratifyAllGateReason(
  store: SystemBlockStore,
  acceptedIds: ReadonlySet<string>,
): string | null {
  const candidates = store.blocks.filter((b) => b.state === 'candidate');
  if (candidates.length === 0) return 'nothing to ratify — no candidate blocks';
  const unaccepted = candidates.filter((b) => !acceptedIds.has(b.block_id)).length;
  if (unaccepted > 0) {
    return `accept ${unaccepted} more name${unaccepted === 1 ? '' : 's'} before ratifying all`;
  }
  const seams = unresolvedSeamCount(store);
  if (seams > 0) {
    return `${seams} unresolved seam${seams === 1 ? '' : 's'} — resolve in Edit Names & Boundaries (a later slice)`;
  }
  return null;
}

/** True iff the blanket "Ratify all → v1" gesture may be offered (§5). */
export function canRatifyAll(store: SystemBlockStore, acceptedIds: ReadonlySet<string>): boolean {
  return ratifyAllGateReason(store, acceptedIds) == null;
}

/** The scan's one-line honest summary (the ok toast body). */
export function scanSummary(res: SkeletonCandidateResult): string {
  const r = res.report;
  return `proposed ${r.block_count} block${r.block_count === 1 ? '' : 's'} from ${r.repo_file_count} files · ${r.unmapped_total} unmapped · store v${res.store_version}`;
}

/** The ratify's one-line honest summary (the ok toast body). */
export function ratifySummary(res: RatifyResult): string {
  const n = res.ratified_block_ids.length;
  return `ratified ${n} block${n === 1 ? '' : 's'} → store v${res.store_version}`;
}

/** Classify a failed scan/ratify into an honest toast, sharing the reconcile
 *  error grammar: an OCC `conflict` (the store moved) reloads; a read-only owner
 *  informs (the gesture stays); anything else surfaces the owner's message verbatim.
 *  `action` names the gesture in the read-only copy ("scan"/"ratify"). */
export function writeErrorToast(err: unknown, expectedVersion: number | null, action: string): WriteToast {
  const s = errorText(err);
  if (/conflict/i.test(s)) {
    const m = s.match(/actual\s+(\d+)/i);
    const actual = m ? m[1] : '?';
    const expected = expectedVersion == null ? '?' : String(expectedVersion);
    return { kind: 'conflict', text: `the store moved (expected v${expected}, actual v${actual}) — reloading` };
  }
  if (/read[-\s]?only/i.test(s)) {
    return { kind: 'readonly', text: `this owner is read-only — ${action} from a writable session` };
  }
  return { kind: 'error', text: s };
}

/**
 * Run one scan and reduce it to a toast + a reload decision — pure over its injected
 * `scanFn`, so the success/conflict/read-only flows are unit-testable with a mocked
 * client (mirrors `runReconcile`). Success reloads (the map re-renders in candidate
 * dress); a conflict reloads (the store moved); a read-only/error refusal does NOT.
 */
export async function runScan(
  scanFn: () => Promise<SkeletonCandidateResult>,
  expectedVersion: number | null,
): Promise<{ toast: WriteToast; shouldReload: boolean }> {
  try {
    const res = await scanFn();
    return { toast: { kind: 'ok', text: scanSummary(res) }, shouldReload: true };
  } catch (err) {
    const toast = writeErrorToast(err, expectedVersion, 'scan');
    return { toast, shouldReload: toast.kind === 'conflict' };
  }
}

/**
 * Run the blanket ratify and reduce it to a toast + a reload decision — pure over
 * its injected `ratifyFn`. Success reloads (the map re-renders ratified); a conflict
 * reloads; a read-only/error refusal informs without a reload.
 */
export async function runRatify(
  ratifyFn: () => Promise<RatifyResult>,
  expectedVersion: number,
): Promise<{ toast: WriteToast; shouldReload: boolean }> {
  try {
    const res = await ratifyFn();
    return { toast: { kind: 'ok', text: ratifySummary(res) }, shouldReload: true };
  } catch (err) {
    const toast = writeErrorToast(err, expectedVersion, 'ratify');
    return { toast, shouldReload: toast.kind === 'conflict' };
  }
}
