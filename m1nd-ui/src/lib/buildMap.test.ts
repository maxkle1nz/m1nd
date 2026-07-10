/*
 * buildMap — the PRD §5 rollup policy, proven against the REAL seed fixture.
 *
 * The honest day-1 truth: the ratified m1nd skeleton has twelve blocks, every
 * receipt array empty, so EVERY block is "needs evidence" (0/N) — never a
 * fabricated green. The synthetic cases prove the other rungs: all-required
 * earned-fresh → evidence-backed; a failing receipt or a dangling socket →
 * broken; no contract → unknown (the engine abstains). Layout is deterministic.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  rollupBlock,
  rollupStore,
  gridLayout,
  canvasSize,
  domainTag,
  brainRefFor,
  repoIdFromSkeletonId,
  membershipByRole,
  COLS,
  type Receipt,
  type ReceiptTypeName,
  type SystemBlock,
  type SystemBlocksSnapshot,
} from './buildMap';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const snapshot = JSON.parse(
  readFileSync(join(FIX, 'system_blocks_snapshot.json'), 'utf8'),
) as SystemBlocksSnapshot;
const store = snapshot.store!;
const names = new Set(store.blocks.map((b) => b.name));

function earnedReceipt(type: ReceiptTypeName, block: SystemBlock, failing = false): Receipt {
  const exec =
    type === 'test'
      ? {
          command: 'cargo test -p m1nd-core',
          cwd: '.',
          exit_status: failing ? 1 : 0,
          started_at: '2026-07-09T00:00:00Z',
          ended_at: '2026-07-09T00:01:00Z',
        }
      : failing
        ? { exit_status: 1 }
        : {};
  return {
    type,
    emitter: { kind: 'ci', id: 'ci-x' },
    scope: {
      block_id: block.block_id,
      boundary_version: block.boundary_version,
      contract_version: block.contract_version,
      resolution_hash: 'sha256:res',
    },
    evidence: { artifact_hash: 'sha256:art', evidence_refs: ['artifacts/x.txt'], ...exec },
    validity: { expires_on: null, stales_on: [] },
  };
}

const clone = (b: SystemBlock): SystemBlock => JSON.parse(JSON.stringify(b));

// ── The real seed: the honest day-1 map ───────────────────────────────────────

test('the real seed rolls up to twelve blocks, ALL "needs evidence" (0/N earned)', () => {
  const rollup = rollupStore(store);
  assert.equal(store.blocks.length, 12, 'the ratified skeleton is twelve blocks');
  for (const b of store.blocks) {
    const r = rollup.rollups.get(b.block_id)!;
    assert.equal(r.state, 'needs-evidence', `${b.block_id} needs evidence (receipts empty)`);
    assert.equal(r.receiptsEarned, 0, `${b.block_id} has earned nothing yet`);
    assert.equal(
      r.receiptsRequired,
      b.receipt_contract.required.length,
      'the denominator is the block\'s own required contract',
    );
    assert.equal(r.candidate, false, 'a ratified block is not a candidate');
  }
});

test('the System Health counts derive from the rollup (needs=12, all others 0)', () => {
  const { counts, unmappedCount, candidate } = rollupStore(store);
  assert.equal(counts['needs-evidence'], 12);
  assert.equal(counts['evidence-backed'], 0);
  assert.equal(counts.broken, 0);
  assert.equal(counts.unknown, 0);
  assert.equal(counts.planned, 0);
  assert.equal(unmappedCount, 0, 'the seed leaves nothing unmapped');
  assert.equal(candidate, false, 'the seed skeleton is ratified — not a candidate map');
});

test('no green without a receipt: no block reads evidence-backed on the empty seed', () => {
  const rollup = rollupStore(store);
  for (const r of rollup.rollups.values()) {
    assert.notEqual(r.state, 'evidence-backed', 'PRD acceptance (d): no green without fresh receipts');
  }
});

// ── The other rungs (synthetic) ───────────────────────────────────────────────

test('all required receipts earned-fresh → evidence-backed', () => {
  const b = clone(store.blocks[0]); // required: spec, structural, test
  assert.deepEqual(
    b.receipt_contract.required.map((r) => r.type),
    ['spec', 'structural', 'test'],
  );
  b.receipts = b.receipt_contract.required.map((req) => earnedReceipt(req.type, b));
  const r = rollupBlock(b, names, new Map(), Date.now());
  assert.equal(r.state, 'evidence-backed');
  assert.equal(r.receiptsEarned, 3);
  assert.equal(r.receiptsRequired, 3);
});

test('a receipt whose scope is a STALE version is not counted (never for a version it did not see)', () => {
  const b = clone(store.blocks[0]);
  const stale = earnedReceipt('spec', b);
  stale.scope.contract_version = 2; // block is at contract 1
  b.receipts = [stale];
  const r = rollupBlock(b, names, new Map(), Date.now());
  assert.equal(r.receiptsEarned, 0, 'a stale-scope receipt earns nothing');
  assert.equal(r.state, 'needs-evidence');
});

test('an expired receipt is not fresh', () => {
  const b = clone(store.blocks[0]);
  b.receipt_contract.required = [{ type: 'spec' }];
  const expired = earnedReceipt('spec', b);
  expired.validity.expires_on = '2000-01-01T00:00:00Z';
  b.receipts = [expired];
  const r = rollupBlock(b, names, new Map(), Date.now());
  assert.equal(r.receiptsEarned, 0);
  assert.equal(r.state, 'needs-evidence');
});

test('a failing (non-zero exit) receipt → broken', () => {
  const b = clone(store.blocks[0]);
  b.receipts = [earnedReceipt('test', b, true)];
  const r = rollupBlock(b, names, new Map(), Date.now());
  assert.equal(r.state, 'broken');
  assert.ok(r.brokenReasons.some((x) => x.includes('failing')), 'the reason is named honestly');
});

test('a dangling output socket (target block absent) → broken', () => {
  const b = clone(store.blocks[0]);
  b.sockets.outputs = [{ to: 'No Such Block', type: 'api' }];
  const r = rollupBlock(b, names, new Map(), Date.now());
  assert.equal(r.state, 'broken');
  assert.ok(r.brokenReasons.some((x) => x.includes('broken socket')));
});

test('an erosion/broken member (from an xray tag) → broken; absence stays neutral', () => {
  const b = clone(store.blocks[0]);
  const path = b.membership[0].path;
  const broken = rollupBlock(b, names, new Map([[path, 'broken']]), Date.now());
  assert.equal(broken.state, 'broken');
  // Absence is NEUTRAL — an unpainted member does not downgrade the block.
  const neutral = rollupBlock(b, names, new Map([[path, 'ok']]), Date.now());
  assert.equal(neutral.state, 'needs-evidence');
});

test('a block with no declared contract → unknown (the engine abstains)', () => {
  const b = clone(store.blocks[0]);
  b.receipt_contract.required = [];
  const r = rollupBlock(b, names, new Map(), Date.now());
  assert.equal(r.state, 'unknown');
});

test('a candidate (unratified) block is flagged candidate', () => {
  const b = clone(store.blocks[0]);
  b.state = 'candidate';
  const r = rollupBlock(b, names, new Map(), Date.now());
  assert.equal(r.candidate, true);
});

// ── Whole-store candidate + planned counting ──────────────────────────────────

test('a candidate skeleton rolls up candidate=true (first-run map)', () => {
  const candStore = JSON.parse(JSON.stringify(store));
  candStore.skeleton.state = 'candidate';
  assert.equal(rollupStore(candStore).candidate, true);
});

test('planned blocks are counted apart, not mixed into the scanned-state buckets', () => {
  const s = JSON.parse(JSON.stringify(store));
  s.blocks[0].kind = 'planned';
  const { counts } = rollupStore(s);
  assert.equal(counts.planned, 1);
  assert.equal(counts['needs-evidence'], 11, 'the planned block left the needs bucket');
});

// ── Derivations ───────────────────────────────────────────────────────────────

test('repoIdFromSkeletonId + domainTag derive a stable, compact tag from block_id', () => {
  assert.equal(repoIdFromSkeletonId(store.skeleton.skeleton_id), 'm1nd');
  assert.equal(domainTag('sb_m1nd_core_graph_kernel', 'm1nd'), 'CORE GRAPH');
  assert.equal(domainTag('sb_m1nd_ingest', 'm1nd'), 'INGEST');
  assert.equal(domainTag('sb_m1nd_ui_api_state', 'm1nd'), 'UI API');
  // every real block yields a non-empty tag
  for (const b of store.blocks) {
    assert.ok(domainTag(b.block_id, 'm1nd').length > 0, `${b.block_id} → a tag`);
  }
});

test('repoIdFromSkeletonId also parses the scan candidate form (sk_<slug>_candidate)', () => {
  // The scan mints `sk_<slug>_candidate` (skeleton_scan.rs); before this the
  // regex only knew the seed form, so a scanned store derived repoId = null
  // (field bug 2026-07-10: the curation letter then said brain_ref "brain").
  assert.equal(repoIdFromSkeletonId('sk_repo_b1_candidate'), 'repo_b1');
  assert.equal(repoIdFromSkeletonId('sk_m1nd_seed_2026_06'), 'm1nd', 'the seed form is untouched');
  assert.equal(repoIdFromSkeletonId('not_a_skeleton'), null);
});

test('brainRefFor derives the guard identity: basename of the brain root, with honest fallbacks', () => {
  // Hosted map (the ?brain= root is known): the display name is the BASENAME of
  // the project root — case and hyphens intact, exactly what the owner's
  // mission_post brain guard compares (mission_letter_handlers.rs).
  assert.equal(brainRefFor('/private/tmp/ws/Repo-B1', 'repo_b1'), 'Repo-B1');
  assert.equal(brainRefFor('/private/tmp/ws/Repo-B1/', null), 'Repo-B1', 'trailing slash is trimmed');
  // Bound map (root = null): fall back to the skeleton slug, then the honest last resort.
  assert.equal(brainRefFor(null, 'm1nd'), 'm1nd');
  assert.equal(brainRefFor(undefined, null), 'brain');
  // Never an absolute path (§1f).
  assert.doesNotMatch(brainRefFor('/a/b/c', null), /\//);
});

test('membershipByRole aggregates the seed roles', () => {
  const roles = membershipByRole(store.blocks[0]);
  const primary = roles.find((r) => r.role === 'primary');
  assert.ok(primary && primary.count === store.blocks[0].membership.length, 'block 0 is all primary');
});

// ── Deterministic layout (F0-TECH §7) ─────────────────────────────────────────

test('gridLayout is deterministic and lays out in COLS columns; stable across calls', () => {
  const a = gridLayout(12);
  const b = gridLayout(12);
  assert.deepEqual(a, b, 'same count → same coordinates (stable position)');
  assert.equal(a.length, 12);
  // row 0 shares a y; the first COLS cards ascend in x.
  for (let i = 1; i < COLS; i += 1) {
    assert.equal(a[i].y, a[0].y, 'first row shares a y');
    assert.ok(a[i].x > a[i - 1].x, 'columns ascend in x');
  }
  assert.ok(a[COLS].y > a[0].y, 'the next row is lower');
  const size = canvasSize(12);
  assert.ok(size.width > 0 && size.height > 0);
});
