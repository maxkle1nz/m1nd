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

/**
 * Earned-fresh (PRD §3.1/§5, MVP): a receipt counts for a block only when its
 * scope binds to the block's CURRENT (block_id, boundary_version, contract_version)
 * — never counted for a version it did not see — AND it is not expired
 * (`expires_on` in the future or null) AND it is not a failing receipt.
 */
export function isEarnedFresh(receipt: Receipt, block: SystemBlock, now: number): boolean {
  if (receipt.scope.block_id !== block.block_id) return false;
  if (receipt.scope.boundary_version !== block.boundary_version) return false;
  if (receipt.scope.contract_version !== block.contract_version) return false;
  if (receipt.validity.expires_on != null) {
    const exp = Date.parse(receipt.validity.expires_on);
    if (Number.isFinite(exp) && exp <= now) return false;
  }
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
  /** Aggregate unmapped residue across blocks (F7 — always visible, even at 0). */
  unmappedCount: number;
  /** The whole skeleton is a candidate (first-run, F6) — every card dashed + banner. */
  candidate: boolean;
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
  return { rollups, counts, unmappedCount, candidate };
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

/** The repo id embedded in a skeleton id (`sk_<repo>_seed_<yyyy>_<mm>`). */
export function repoIdFromSkeletonId(skeletonId: string): string | null {
  const m = skeletonId.match(/^sk_(.+?)_seed_/);
  return m ? m[1] : null;
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
