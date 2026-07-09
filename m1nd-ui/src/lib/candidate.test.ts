/*
 * candidate — the F0c scan/review/ratify PURE policy (HUMAN-VIEW-V2 F0c §3/§5).
 * These are the testable seams behind the walk: the component-confidence summary
 * (never a fabricated blend), the low-support-first review queue, the §5 blanket
 * ratify gate (every candidate owner-accepted AND no unresolved seam), and the
 * scan/ratify runners reduced from a mocked client (mirrors reconcile.test.ts).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  candidateConfidence,
  blockSupport,
  reviewQueue,
  unresolvedSeamCount,
  ratifyAllGateReason,
  canRatifyAll,
  scanSummary,
  ratifySummary,
  writeErrorToast,
  runScan,
  runRatify,
  type CandidateMeta,
  type RatifyResult,
  type SkeletonCandidateResult,
  type SystemBlock,
  type SystemBlockStore,
} from './buildMap';

// ── factories (minimal valid shapes) ─────────────────────────────────────────

function meta(over: Partial<CandidateMeta> = {}): CandidateMeta {
  return {
    named_by: 'heuristic',
    needs_owner_naming: false,
    edge_sample_size: 40,
    directory_support: 0.9,
    coverage_ratio: 0.9,
    shared_member_count: 0,
    ...over,
  };
}

function candBlock(id: string, name: string, m: CandidateMeta, members = 3): SystemBlock {
  return {
    block_id: id,
    name,
    purpose: `${name} purpose`,
    kind: 'scanned',
    state: 'candidate',
    boundary_version: 1,
    contract_version: 1,
    membership_source: 'proposed',
    membership: Array.from({ length: members }, (_, i) => ({ path: `${id}/f${i}.rs`, role: 'primary' as const })),
    sockets: { inputs: [], outputs: [], external: [] },
    receipt_contract: { version: 1, required: [], optional: [], waived: [], declared_by: null, declared_at: null },
    receipts: [],
    layout: { x: null, y: null, locked: false, algorithm_seed: null, version: 1 },
    unmapped_residue: [],
    candidate_meta: m,
  };
}

function candStore(blocks: SystemBlock[], over: Partial<SystemBlockStore> = {}): SystemBlockStore {
  return {
    schema: 'm1nd-system-block-store-v0',
    store_version: 2,
    skeleton: {
      skeleton_id: 'sk_demo_seed_2026_07',
      version: 1,
      state: 'candidate',
      ratification: { method: 'verb', ratifier: '', ratified_at: '', commit: '' },
    },
    blocks,
    unmapped_policy: { visible: true, default_action: 'leave_unmapped_until_ratified' },
    ...over,
  };
}

// low, mid, high support (mean of components ×100 = 40, 70, 97)
const low = candBlock('sb_low', 'Low', meta({ directory_support: 0.4, coverage_ratio: 0.4, graph_cohesion: 0.4 }));
const mid = candBlock('sb_mid', 'Mid', meta({ directory_support: 0.7, coverage_ratio: 0.7, graph_cohesion: 0.7 }));
const high = candBlock('sb_high', 'High', meta({ directory_support: 0.97, coverage_ratio: 0.97, graph_cohesion: 0.97 }));

// ── candidateConfidence — components, not a vibe blend (§3b) ───────────────────

test('candidateConfidence keeps every component and summarizes the present ones', () => {
  const c = candidateConfidence(meta({ directory_support: 0.9, coverage_ratio: 0.8, graph_cohesion: 0.7 }));
  assert.equal(c.components.length, 3);
  assert.equal(c.components.find((x) => x.key === 'directory_support')?.pct, 90);
  assert.equal(c.components.find((x) => x.key === 'coverage_ratio')?.pct, 80);
  assert.equal(c.components.find((x) => x.key === 'graph_cohesion')?.pct, 70);
  assert.equal(c.summaryPct, 80); // mean of 90/80/70
});

test('a docs/no-edge block never fabricates cohesion — null stays null, summary skips it', () => {
  const c = candidateConfidence(meta({ graph_cohesion: undefined, directory_support: 1.0, coverage_ratio: 0.6 }));
  assert.equal(c.components.find((x) => x.key === 'graph_cohesion')?.pct, null);
  assert.equal(c.summaryPct, 80); // mean of the two PRESENT components (100, 60), cohesion excluded
});

// ── reviewQueue — lowest support first, honest total + limit (§3b/§7) ─────────

test('reviewQueue orders candidate blocks lowest-support first (block_id tie-break)', () => {
  const q = reviewQueue(candStore([high, low, mid]));
  assert.deepEqual(q.ordered.map((b) => b.block_id), ['sb_low', 'sb_mid', 'sb_high']);
  assert.equal(q.total, 3);
  assert.equal(q.limit, 16);
  assert.ok(blockSupport(low) < blockSupport(high));
});

test('reviewQueue includes ONLY candidate blocks — a ratified block is not queued', () => {
  const ratified = { ...candBlock('sb_done', 'Done', meta()), state: 'ratified' as const };
  const q = reviewQueue(candStore([low, ratified]));
  assert.deepEqual(q.ordered.map((b) => b.block_id), ['sb_low']);
});

test('review_limit bounds the queue page, never the emitted total', () => {
  const many = Array.from({ length: 20 }, (_, i) =>
    candBlock(`sb_${String(i).padStart(2, '0')}`, `B${i}`, meta({ directory_support: i / 20, coverage_ratio: i / 20 })),
  );
  const q = reviewQueue(candStore(many), 5);
  assert.equal(q.total, 20, 'the honest total is every block');
  assert.equal(q.limit, 5, 'the limit is only the page bound');
});

// ── the §5 blanket ratify gate ────────────────────────────────────────────────

test('ratify gate: an unaccepted name blocks Ratify all, with an honest reason', () => {
  const store = candStore([low, mid]);
  const reason = ratifyAllGateReason(store, new Set(['sb_low']));
  assert.match(reason ?? '', /accept 1 more name/);
  assert.equal(canRatifyAll(store, new Set(['sb_low'])), false);
});

test('ratify gate: all accepted + no seam → Ratify all is offered', () => {
  const store = candStore([low, mid]);
  const accepted = new Set(['sb_low', 'sb_mid']);
  assert.equal(ratifyAllGateReason(store, accepted), null);
  assert.equal(canRatifyAll(store, accepted), true);
});

test('ratify gate: an unresolved seam blocks the blanket gesture even when all accepted (§5)', () => {
  const seamBlock = candBlock('sb_seam', 'Seam', meta({ shared_member_count: 2 }));
  const store = candStore([low, seamBlock]);
  assert.equal(unresolvedSeamCount(store), 1);
  const accepted = new Set(['sb_low', 'sb_seam']);
  const reason = ratifyAllGateReason(store, accepted);
  assert.match(reason ?? '', /1 unresolved seam/);
  assert.match(reason ?? '', /Edit Names & Boundaries/);
  assert.equal(canRatifyAll(store, accepted), false);
});

test('ratify gate: an empty candidate store is honestly not ratifiable', () => {
  const store = candStore([]);
  assert.match(ratifyAllGateReason(store, new Set()) ?? '', /nothing to ratify/);
});

// ── summaries ─────────────────────────────────────────────────────────────────

test('scanSummary reads the report census honestly', () => {
  const res = {
    store_version: 3,
    report: { block_count: 7, repo_file_count: 342, unmapped_total: 23 },
  } as unknown as SkeletonCandidateResult;
  assert.equal(scanSummary(res), 'proposed 7 blocks from 342 files · 23 unmapped · store v3');
});

test('ratifySummary counts the ratified blocks + the new store version', () => {
  const res: RatifyResult = {
    store_version: 3,
    ratified_block_ids: ['a', 'b'],
    skeleton_state: 'ratified',
    ratifier: 'gui',
    ratified_at: '2026-07-09T12:00:00Z',
  };
  assert.equal(ratifySummary(res), 'ratified 2 blocks → store v3');
});

// ── writeErrorToast — the shared honest failure grammar ───────────────────────

test('writeErrorToast: an OCC conflict names both versions and reloads', () => {
  const t = writeErrorToast({ detail: 'store version conflict: expected 2, actual 5 …' }, 2, 'ratify');
  assert.equal(t.kind, 'conflict');
  assert.match(t.text, /expected v2, actual v5/);
});

test('writeErrorToast: a read-only owner informs with the action named', () => {
  const t = writeErrorToast({ detail: 'm1nd is attached read-only (--read-only); mutation tool is disabled' }, null, 'scan');
  assert.equal(t.kind, 'readonly');
  assert.match(t.text, /read-only/);
  assert.match(t.text, /scan from a writable session/);
});

test('writeErrorToast: anything else surfaces the owner message verbatim', () => {
  const t = writeErrorToast({ message: 'boom' }, 2, 'ratify');
  assert.equal(t.kind, 'error');
  assert.equal(t.text, 'boom');
});

// ── runScan / runRatify — mocked-client flows ─────────────────────────────────

test('runScan: success → an ok toast (summary) AND a reload (the map re-renders in candidate dress)', async () => {
  const res = { store_version: 1, report: { block_count: 3, repo_file_count: 10, unmapped_total: 1 } } as unknown as SkeletonCandidateResult;
  const { toast, shouldReload } = await runScan(async () => res, null);
  assert.equal(toast.kind, 'ok');
  assert.match(toast.text, /proposed 3 blocks/);
  assert.equal(shouldReload, true);
});

test('runScan: a read-only refusal informs WITHOUT a reload (nothing changed)', async () => {
  const { toast, shouldReload } = await runScan(async () => {
    throw { detail: 'm1nd is attached read-only (--read-only)' };
  }, null);
  assert.equal(toast.kind, 'readonly');
  assert.equal(shouldReload, false);
});

test('runRatify: success → an ok toast AND a reload (the map re-renders ratified)', async () => {
  const res: RatifyResult = { store_version: 3, ratified_block_ids: ['a'], skeleton_state: 'ratified', ratifier: 'gui', ratified_at: 'x' };
  const { toast, shouldReload } = await runRatify(async () => res, 2);
  assert.equal(toast.kind, 'ok');
  assert.match(toast.text, /ratified 1 block/);
  assert.equal(shouldReload, true);
});

test('runRatify: an OCC conflict renders the conflict toast + reloads, NEVER a silent retry', async () => {
  const { toast, shouldReload } = await runRatify(async () => {
    throw { detail: 'store version conflict: expected 2, actual 4' };
  }, 2);
  assert.equal(toast.kind, 'conflict');
  assert.equal(shouldReload, true);
});
