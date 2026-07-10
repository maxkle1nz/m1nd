/*
 * candidateEdit — the F11-c PURE policy tests (screen book §3; F11-TECH §4).
 * The testable seams behind Edit Names & Boundaries: every gesture compiles to
 * candidate_edit ops batched per gesture (§4b), the split's explicit path groups
 * (o3), the friction ordering + zero-touch line (§4c), the o4 curating banner,
 * the v2 ratify gate (0b: runner-named needs no manual accept), and the write
 * reducers over a mocked client (mirrors candidate.test.ts).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  acceptNameOp,
  assignOp,
  blockSeams,
  curatingBanner,
  mergeOp,
  provisionalFirstQueue,
  purposeOp,
  ratifyGateReasonV2,
  renameOp,
  runCandidateEdit,
  runCandidateNaming,
  seamOp,
  splitOpFromSelection,
  zeroTouchLine,
  type CandidateEditResult,
  type CandidateNamingResult,
} from './candidateEdit';
import type { CandidateMeta, SystemBlock, SystemBlockStore } from './buildMap';

// ── factories (the candidate.test.ts shapes) ─────────────────────────────────

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

function candBlock(id: string, name: string, m: CandidateMeta, paths?: string[]): SystemBlock {
  const memberPaths = paths ?? [`${id}/a.rs`, `${id}/b.rs`];
  return {
    block_id: id,
    name,
    purpose: `${name} purpose`,
    kind: 'scanned',
    state: 'candidate',
    boundary_version: 1,
    contract_version: 1,
    membership_source: 'proposed',
    membership: memberPaths.map((path) => ({ path, role: 'primary' as const })),
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
    store_version: 3,
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

// ── gesture → op compilers (§4b) ─────────────────────────────────────────────

test('renameOp compiles a committed rename to ONE op and skips no-ops', () => {
  const b = candBlock('sb_a', 'Old Name', meta());
  assert.deepEqual(renameOp(b, '  Auth  '), { op: 'rename', block_id: 'sb_a', name: 'Auth' });
  // Unchanged name → no op (no-op gestures post nothing).
  assert.equal(renameOp(b, 'Old Name'), null);
  // An empty draft never compiles — the server would refuse an empty name.
  assert.equal(renameOp(b, '   '), null);
  // Purpose-only change rides the same rename op shape.
  assert.deepEqual(purposeOp(b, 'Owns login.'), {
    op: 'rename',
    block_id: 'sb_a',
    purpose: 'Owns login.',
  });
});

test('acceptNameOp is a REAL owner touch — a rename op with the stored name', () => {
  const b = candBlock('sb_a', 'Heuristic Guess', meta({ needs_owner_naming: true }));
  assert.deepEqual(acceptNameOp(b), { op: 'rename', block_id: 'sb_a', name: 'Heuristic Guess' });
});

test('seamOp compiles both radio choices; assignOp and mergeOp are one op each', () => {
  assert.deepEqual(seamOp('src/shared.ts', 'both'), {
    op: 'resolve_seam',
    path: 'src/shared.ts',
    resolution: 'both',
  });
  assert.deepEqual(seamOp('src/shared.ts', { primary: 'sb_pay' }), {
    op: 'resolve_seam',
    path: 'src/shared.ts',
    resolution: 'primary:sb_pay',
  });
  assert.deepEqual(assignOp('scripts/x.sh', 'sb_a'), {
    op: 'assign_unmapped',
    path: 'scripts/x.sh',
    block_id: 'sb_a',
  });
  assert.deepEqual(mergeOp('sb_a', ['sb_b']), { op: 'merge', into: 'sb_a', block_ids: ['sb_b'] });
});

test('splitOpFromSelection builds EXPLICIT disjoint+total path groups from the selection (o3)', () => {
  const b = candBlock('sb_p', 'P', meta(), ['src/api/a.rs', 'src/api/b.rs', 'src/db/c.rs']);
  const out = splitOpFromSelection(b, new Set(['src/api/a.rs', 'src/api/b.rs']));
  assert.ok('op' in out, 'a partial selection splits');
  assert.deepEqual(out.op, {
    op: 'split',
    block_id: 'sb_p',
    by: { paths: [['src/api/a.rs', 'src/api/b.rs'], ['src/db/c.rs']] },
  });
  // Empty and total selections refuse with the honest reason (never a bad op).
  const none = splitOpFromSelection(b, new Set());
  assert.ok('reason' in none && /select the members/.test(none.reason));
  const all = splitOpFromSelection(b, new Set(['src/api/a.rs', 'src/api/b.rs', 'src/db/c.rs']));
  assert.ok('reason' in all && /BOTH sides/.test(all.reason));
});

// ── seams: many-to-many members + their owners ───────────────────────────────

test('blockSeams lists multi-owner members with every claiming block', () => {
  const a = candBlock('sb_a', 'A', meta(), ['shared.ts', 'a1.rs']);
  const b = candBlock('sb_b', 'B', meta(), ['shared.ts', 'b1.rs']);
  const c = candBlock('sb_c', 'C', meta(), ['shared.ts', 'c1.rs']);
  const store = candStore([a, b, c]);
  const seams = blockSeams(store, 'sb_a');
  assert.equal(seams.length, 1);
  assert.equal(seams[0].path, 'shared.ts');
  assert.deepEqual(seams[0].owners, ['sb_a', 'sb_b', 'sb_c'], '3+ owners supported');
  assert.deepEqual(blockSeams(store, 'sb_ghost'), [], 'unknown block → empty, never a throw');
});

// ── the friction law (§4c): ordering + zero-touch + the v2 gate ──────────────

const provisional = candBlock('sb_prov', 'guess?', meta({ needs_owner_naming: true, directory_support: 0.9, coverage_ratio: 0.9 }));
const runnerNamed = candBlock('sb_run', 'Auth', meta({ named_by: 'runner', directory_support: 0.5, coverage_ratio: 0.5 }));
const ownerNamed = candBlock('sb_own', 'Payments', meta({ named_by: 'owner', directory_support: 0.7, coverage_ratio: 0.7 }));

test('provisionalFirstQueue surfaces needs-you blocks FIRST, then lowest support', () => {
  const q = provisionalFirstQueue(candStore([runnerNamed, ownerNamed, provisional]));
  assert.deepEqual(
    q.map((b) => b.block_id),
    ['sb_prov', 'sb_run', 'sb_own'],
    'provisional first (despite its higher support), then by support ascending',
  );
});

test('zeroTouchLine carries the §4c phrasing when ALL blocks are runner-named', () => {
  const allRunner = candStore([
    candBlock('sb_1', 'A', meta({ named_by: 'runner' })),
    candBlock('sb_2', 'B', meta({ named_by: 'runner' })),
  ]);
  assert.equal(zeroTouchLine(allRunner), 'all 2 blocks runner-named — ready to ratify');
  // A mixed owner/runner map is still zero-touch-ready, phrased honestly.
  assert.equal(
    zeroTouchLine(candStore([runnerNamed, ownerNamed])),
    'all 2 blocks named — ready to ratify',
  );
  // Any block still needing the owner → no line.
  assert.equal(zeroTouchLine(candStore([runnerNamed, provisional])), null);
  assert.equal(zeroTouchLine(candStore([])), null);
});

test('ratifyGateReasonV2: provisional blocks gate; runner-named needs NO manual accept (0b)', () => {
  assert.match(
    ratifyGateReasonV2(candStore([provisional, runnerNamed])) ?? '',
    /1 block still needs a name/,
  );
  // All named (runner/owner) + no seams → the gate opens with no acceptance step.
  assert.equal(ratifyGateReasonV2(candStore([runnerNamed, ownerNamed])), null);
  // An unresolved seam still gates.
  const seam = candBlock('sb_seam', 'S', meta({ named_by: 'runner', shared_member_count: 1 }));
  assert.match(ratifyGateReasonV2(candStore([runnerNamed, seam])) ?? '', /1 unresolved seam/);
  assert.match(ratifyGateReasonV2(candStore([])) ?? '', /nothing to ratify/);
});

// ── the o4 curating banner ────────────────────────────────────────────────────

test('curatingBanner shows a LIVE advisory lease and hides an expired/absent one', () => {
  const live = candStore([runnerNamed], {
    curating_by: 'hand-7',
    curating_until: '2026-07-10T12:00:00Z',
  });
  assert.deepEqual(curatingBanner(live, '2026-07-10T11:00:00Z'), {
    by: 'hand-7',
    until: '2026-07-10T12:00:00Z',
  });
  assert.equal(curatingBanner(live, '2026-07-10T12:00:01Z'), null, 'expired → no banner');
  assert.equal(curatingBanner(candStore([runnerNamed]), '2026-07-10T11:00:00Z'), null, 'free → no banner');
});

// ── the write reducers (mocked client — the reconcile/ratify pattern) ─────────

const editOk: CandidateEditResult = {
  store_version: 4,
  block_count: 2,
  ops_applied: 2,
  store: candStore([runnerNamed]),
};

test('runCandidateEdit: success reloads with the honest count; conflict reloads; error informs', async () => {
  const ok = await runCandidateEdit(async () => editOk, 3);
  assert.equal(ok.toast.kind, 'ok');
  assert.match(ok.toast.text, /applied 2 edits → store v4/);
  assert.equal(ok.shouldReload, true);

  const conflict = await runCandidateEdit(async () => {
    throw new Error('store version conflict: expected 3, actual 5 — reload and retry');
  }, 3);
  assert.equal(conflict.toast.kind, 'conflict');
  assert.equal(conflict.shouldReload, true, 'a conflict reloads — never a silent merge');

  const err = await runCandidateEdit(async () => {
    throw new Error('candidate_edit rejected at op 1: unknown block');
  }, 3);
  assert.equal(err.toast.kind, 'error');
  assert.equal(err.shouldReload, false);
});

test('runCandidateNaming: partial success reloads; refusal and zero-named inform without reload', async () => {
  const partial: CandidateNamingResult = {
    store_version: 4,
    named: ['sb_a', 'sb_b'],
    fell_back: [['sb_c', 'naming runner timed out after 20s']],
  };
  const ok = await runCandidateNaming(async () => partial, 3);
  assert.equal(ok.toast.kind, 'ok');
  assert.match(ok.toast.text, /runner named 2 blocks, 1 fell back → store v4/);
  assert.equal(ok.shouldReload, true);

  const refused = await runCandidateNaming(
    async () => ({
      store_version: 3,
      named: [],
      fell_back: [],
      refusal: 'no_naming_runner: no runner daemon announced',
    }),
    3,
  );
  assert.equal(refused.toast.kind, 'error');
  assert.match(refused.toast.text, /no_naming_runner/);
  assert.equal(refused.shouldReload, false, 'nothing changed — no reload');

  const zero = await runCandidateNaming(
    async () => ({ store_version: 3, named: [], fell_back: [['sb_a', 'hostile output']] }),
    3,
  );
  assert.equal(zero.toast.kind, 'error');
  assert.equal(zero.shouldReload, false);

  const conflict = await runCandidateNaming(async () => {
    throw new Error('store version conflict: expected 3, actual 6');
  }, 3);
  assert.equal(conflict.toast.kind, 'conflict');
  assert.equal(conflict.shouldReload, true);
});
