/*
 * missions — the mission tray's pure heart, proven against the F2.5a wire shape.
 * The teeth: `failed` is PINNED atop the tray even when it is the OLDEST mission
 * (the §3c pin overrides the time sort); the seven phase labels obey the copy law
 * (never "done/proven/correct"); `composeSeq1Letter` builds a valid seq-1 `judging`
 * letter (seq 1, null prev, `msn_<12hex>` id, the packet_ref anchored); the §1d
 * landed-law is encoded (a green gate is not a landing); and elapsed/id/hash are
 * deterministic under injected clocks/rng.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  archiveBoundaryView,
  archiveHead,
  blockLabelFromId,
  composeSeq1Letter,
  elapsedLabel,
  gateLine,
  isStagnantHead,
  landCandidate,
  landErrorToast,
  mergeWaitStatusLine,
  newMissionId,
  PHASES,
  phaseCounts,
  phaseLabel,
  receiptAnchorLabel,
  sha256Hex,
  sortForTray,
  type LandDeps,
  type MissionHead,
  type MissionLetter,
  type MissionsResponse,
  type PostOutcome,
} from './missions';
import type { Receipt, SystemBlocksSnapshot } from './buildMap';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const heads = (JSON.parse(readFileSync(join(FIX, 'missions_heads.json'), 'utf8')) as MissionsResponse)
  .missions!;

// A minimal fresh snapshot for the landing flow: only the fields `landCandidate` reads
// (present, store_version, the block's versions + resolution). Cast for focus — the
// full `SystemBlockStore` shape is proven by buildMap's own suite.
function snapshotWith(
  block: { boundary_version: number; contract_version: number; membership_fingerprint?: string },
  storeVersion = 9,
): SystemBlocksSnapshot {
  return {
    present: true,
    store_version: storeVersion,
    store: {
      store_version: storeVersion,
      blocks: [{ block_id: 'sb_m1nd_mailbox', ...block }],
    },
  } as unknown as SystemBlocksSnapshot;
}

const okPost = (letter: MissionLetter): Promise<PostOutcome> =>
  Promise.resolve({
    letter_id: 'newletter12',
    mission_id: letter.mission_id,
    mission_seq: letter.mission_seq,
    deduped: false,
    phase: letter.phase,
  });

// ── §3c — failure is never folded away ────────────────────────────────────────

test('sortForTray PINS failed atop the tray even when it is the OLDEST mission', () => {
  const ordered = sortForTray(heads);
  // The failed mission (updated 07:00 — the OLDEST) must lead, above the newest
  // executing (10:03): the pin overrides the time sort.
  assert.equal(ordered[0].head.phase, 'failed', 'failed leads');
  assert.equal(ordered[0].mission_id, 'msn_00000000d004');
  // The rest follow newest-first: executing (10:03) · merge_wait (09:58) · landed (09:00).
  assert.deepEqual(
    ordered.slice(1).map((h) => h.head.phase),
    ['executing', 'merge_wait', 'landed'],
  );
});

test('sortForTray is pure — the input array is not mutated', () => {
  const before = heads.map((h) => h.mission_id);
  sortForTray(heads);
  assert.deepEqual(
    heads.map((h) => h.mission_id),
    before,
  );
});

// ── Copy law — the seven phase names ──────────────────────────────────────────

test('phaseLabel obeys the copy law: no "done/proven/correct" in any of the 7 names', () => {
  for (const p of PHASES) {
    const label = phaseLabel(p);
    assert.doesNotMatch(label, /\bdone\b/i, `${p} label must not say done`);
    assert.doesNotMatch(label, /\bproven\b/i, `${p} label must not say proven`);
    assert.doesNotMatch(label, /\bcorrect\b/i, `${p} label must not say correct`);
  }
  // The names are the enum verbatim (merge_wait spaces for reading, not a claim).
  assert.equal(phaseLabel('merge_wait'), 'merge wait');
  assert.equal(phaseLabel('landed'), 'landed');
  assert.equal(phaseLabel('failed'), 'failed');
});

test('phaseCounts tallies every phase present in the heads', () => {
  const counts = phaseCounts(heads);
  assert.equal(counts.executing, 1);
  assert.equal(counts.merge_wait, 1);
  assert.equal(counts.landed, 1);
  assert.equal(counts.failed, 1);
  assert.equal(counts.judging, 0);
});

// ── §3c (extended) — the stagnant-head test ───────────────────────────────────

const executingHead = (): MissionHead => heads.find((h) => h.head.phase === 'executing')!; // updated 2026-07-09T10:03Z

test('isStagnantHead: a judging/executing head unmoved > 24h is stagnant', () => {
  const head = executingHead();
  // 4+ days after its updated_at.
  assert.equal(isStagnantHead(head, Date.parse('2026-07-13T12:00:00Z')), true);
});

test('isStagnantHead: the SAME head < 24h old is NOT stagnant (a fabricated staleness is refused)', () => {
  const head = executingHead();
  // ~2h after its updated_at.
  assert.equal(isStagnantHead(head, Date.parse('2026-07-09T12:00:00Z')), false);
});

test('isStagnantHead: only judging/executing can be stagnant — never merge_wait/landed/failed', () => {
  const way_later = Date.parse('2026-08-01T00:00:00Z');
  for (const phase of ['merge_wait', 'landed', 'failed'] as const) {
    const head = heads.find((h) => h.head.phase === phase)!;
    assert.equal(isStagnantHead(head, way_later), false, `${phase} is never stagnant`);
  }
});

test('isStagnantHead: an unparseable updated_at is never stagnant (no fabricated age)', () => {
  const head = executingHead();
  const broken: MissionHead = { ...head, head: { ...head.head, updated_at: 'not-a-date' } };
  assert.equal(isStagnantHead(broken, Date.parse('2027-01-01T00:00:00Z')), false);
});

// ── §1d — the landed-law, encoded ─────────────────────────────────────────────

test('mergeWaitStatusLine flies "gate green — receipt not landed" on a green gate with no receipt', () => {
  const mergeWait = heads.find((h) => h.head.phase === 'merge_wait')!;
  assert.equal(mergeWaitStatusLine(mergeWait.head), 'gate green — receipt not landed');
  // Not a merge_wait → no line (the law only speaks where it applies).
  const landed = heads.find((h) => h.head.phase === 'landed')!;
  assert.equal(mergeWaitStatusLine(landed.head), null);
});

test('mergeWaitStatusLine: a green gate with NO candidate is "gate green — no candidate attached" (§6-F2.5d)', () => {
  const mergeWait = heads.find((h) => h.head.phase === 'merge_wait')!;
  // Strip the candidate → the honest no-candidate state (there is nothing to import,
  // so the card never offers a button that would fabricate a receipt).
  const noCand: MissionLetter = { ...mergeWait.head, receipt_candidate: undefined };
  assert.equal(mergeWaitStatusLine(noCand), 'gate green — no candidate attached');
  // The candidate present → the law line holds; the import affordance rides beside it.
  assert.equal(mergeWaitStatusLine(mergeWait.head), 'gate green — receipt not landed');
});

test('receiptAnchorLabel only lands on an imported receipt with a real store_version', () => {
  const landed = heads.find((h) => h.head.phase === 'landed')!;
  assert.equal(receiptAnchorLabel(landed.head), 'receipt ✓ store v9');
  // A merge_wait (green gate, no imported receipt) never shows a receipt anchor.
  const mergeWait = heads.find((h) => h.head.phase === 'merge_wait')!;
  assert.equal(receiptAnchorLabel(mergeWait.head), null);
});

test('gateLine renders `command · exit N`, or null with no gate', () => {
  const mergeWait = heads.find((h) => h.head.phase === 'merge_wait')!;
  assert.equal(gateLine(mergeWait.head), 'cargo test -p m1nd-mcp · exit 0');
  const executing = heads.find((h) => h.head.phase === 'executing')!;
  assert.equal(gateLine(executing.head), null);
});

// ── §4a — the seq-1 judging letter ────────────────────────────────────────────

test('composeSeq1Letter builds a valid seq-1 judging letter (seq 1, null prev, msn_ id)', () => {
  const letter = composeSeq1Letter({
    blockId: 'sb_m1nd_mailbox',
    brainRef: 'm1nd',
    packetRef: 'sha256:deadbeef',
    now: '2026-07-09T12:00:00Z',
  });
  assert.equal(letter.schema, 'm1nd-mission-letter-v0');
  assert.equal(letter.mission_seq, 1, 'a chain starts at seq 1');
  assert.equal(letter.prev_letter_id, undefined, 'seq-1 carries a NULL prev (§1e) — absent on the wire');
  assert.match(letter.mission_id, /^msn_[0-9a-f]{12}$/, 'msn_<12hex> id shape');
  assert.equal(letter.phase, 'judging');
  assert.equal(letter.seat, 'oracle', 'direct posts the oracle judging seat');
  assert.equal(letter.capability, 'build-runner', 'the default code lane (§5b)');
  assert.equal(letter.block_id, 'sb_m1nd_mailbox');
  assert.equal(letter.brain_ref, 'm1nd');
  assert.equal(letter.packet_ref, 'sha256:deadbeef');
  assert.equal(letter.runner_id, undefined, 'no runner is involved in direct (§4a)');
  assert.equal(letter.started_at, '2026-07-09T12:00:00Z');
});

test('composeSeq1Letter honors an injected mission id + capability', () => {
  const letter = composeSeq1Letter({
    blockId: 'sb_m1nd_ingest',
    brainRef: 'm1nd',
    packetRef: 'sha256:abc',
    missionId: 'msn_0123456789ab',
    capability: 'naming-runner',
  });
  assert.equal(letter.mission_id, 'msn_0123456789ab');
  assert.equal(letter.capability, 'naming-runner');
});

test('newMissionId is msn_ + 12 lowercase hex (deterministic under injected rng)', () => {
  const id = newMissionId((out) => {
    out.set([0xab, 0xcd, 0xef, 0x01, 0x23, 0x45]);
    return out;
  });
  assert.equal(id, 'msn_abcdef012345');
  // The real generator still obeys the shape.
  assert.match(newMissionId(), /^msn_[0-9a-f]{12}$/);
});

test('sha256Hex is a real 64-hex sha256 (WebCrypto)', async () => {
  const hex = await sha256Hex('x');
  assert.match(hex, /^[0-9a-f]{64}$/);
  assert.equal(hex.slice(0, 8), '2d711642', 'sha256("x") prefix');
});

// ── elapsed + block name derivation ───────────────────────────────────────────

test('elapsedLabel is compact + deterministic under an injected clock', () => {
  const start = '2026-07-09T10:00:00Z';
  const at = (iso: string) => Date.parse(iso);
  assert.equal(elapsedLabel(start, at('2026-07-09T10:00:12Z')), '12s');
  assert.equal(elapsedLabel(start, at('2026-07-09T10:05:00Z')), '5m');
  assert.equal(elapsedLabel(start, at('2026-07-09T12:00:00Z')), '2h');
  assert.equal(elapsedLabel(start, at('2026-07-12T10:00:00Z')), '3d');
  assert.equal(elapsedLabel('not-a-date'), '', 'an unparseable start is never a fake time');
});

test('blockLabelFromId de-slugs the block_id, stripping the repo prefix', () => {
  assert.equal(blockLabelFromId('sb_m1nd_core_graph_kernel', 'm1nd'), 'core graph kernel');
  assert.equal(blockLabelFromId('sb_mailbox'), 'mailbox');
});

// ── §6-F2.5d — the human landing (landCandidate + landErrorToast) ──────────────

const mergeWaitHead = (): MissionHead => heads.find((h) => h.head.phase === 'merge_wait')!;

test('landCandidate: happy path — imports the candidate’s receipt AND posts the landed letter (both calls asserted)', async () => {
  const head = mergeWaitHead();
  const snap = snapshotWith({ boundary_version: 1, contract_version: 1, membership_fingerprint: 'sha256:fp_mailbox_b1' }, 9);
  const importCalls: Array<{ expectedStoreVersion: number; blockId: string; receipt: Receipt }> = [];
  const postCalls: MissionLetter[] = [];
  const result = await landCandidate(head, {
    snapshot: async () => snap,
    importReceipt: async (input) => {
      importCalls.push(input);
      return { store_version: 10, block_id: input.blockId, receipt_count: 1 };
    },
    postMission: async (letter) => {
      postCalls.push(letter);
      return okPost(letter);
    },
    now: () => '2026-07-09T12:00:00Z',
  });

  // The import: OCC-keyed on the FRESH store_version; the receipt assembled from the candidate.
  assert.equal(importCalls.length, 1);
  const imp = importCalls[0];
  assert.equal(imp.expectedStoreVersion, 9, 'the fresh store_version keys the OCC');
  assert.equal(imp.blockId, 'sb_m1nd_mailbox');
  assert.equal(imp.receipt.type, 'test', 'the candidate’s type');
  assert.deepEqual(imp.receipt.emitter, { kind: 'verb', id: 'human-ui-landing' }, 'the named human gesture');
  assert.equal(imp.receipt.scope.block_id, 'sb_m1nd_mailbox');
  assert.equal(imp.receipt.scope.boundary_version, 1, 'the CANDIDATE’s boundary (never re-dated)');
  assert.equal(imp.receipt.scope.contract_version, 1, 'the CANDIDATE’s contract');
  assert.equal(imp.receipt.scope.resolution_hash, 'sha256:fp_mailbox_b1', 'the block’s CURRENT resolution, bound at import');
  assert.deepEqual(imp.receipt.validity, { expires_on: null, stales_on: ['boundary_change', 'member_change'] });
  assert.equal(imp.receipt.evidence.artifact_hash, 'sha256:candloglb', 'the candidate’s evidence, verbatim');
  assert.deepEqual(imp.receipt.evidence.evidence_refs, ['artifact://run/2']);

  // The landed letter: seq+1, prev=head_letter_id, phase landed, the §1d anchor, inherited identity + gate.
  assert.equal(postCalls.length, 1);
  const landed = postCalls[0];
  assert.equal(landed.phase, 'landed');
  assert.equal(landed.mission_seq, 4, 'head.seq + 1');
  assert.equal(landed.prev_letter_id, 'bbbb22220003', 'the §1e chain: prev = head_letter_id');
  assert.deepEqual(landed.receipt, { imported: true, store_version: 10 }, 'the §1d anchor names the import’s store_version');
  assert.equal(landed.block_id, 'sb_m1nd_mailbox');
  assert.equal(landed.brain_ref, 'm1nd');
  assert.equal(landed.seat, 'hand');
  assert.equal(landed.capability, 'build-runner');
  assert.equal(landed.runner_id, 'runner-build-1', 'runner_id inherited');
  assert.equal(landed.packet_ref, 'sha256:mergepacket', 'packet_ref inherited (§3f provenance)');
  assert.deepEqual(landed.gate, head.head.gate, 'the gate inherited (the §3f provenance chain stays whole)');
  assert.equal(landed.mission_id, head.mission_id);
  assert.equal(landed.updated_at, '2026-07-09T12:00:00Z', 'the injected clock');

  // The outcome: landed, the copy-law toast, a reload.
  assert.equal(result.landed, true);
  assert.equal(result.toast.kind, 'ok');
  assert.equal(result.toast.text, 'receipt landed — store v10', 'copy law verbatim');
  assert.equal(result.shouldReload, true);
});

test('landCandidate: NEVER re-dates the scope — a moved fresh block still carries the CANDIDATE’s boundary', async () => {
  const head = mergeWaitHead(); // candidate scope: boundary 1 / contract 1
  // The fresh block MOVED (boundary 5, contract 2) since the gate ran. The receipt must
  // STILL carry the candidate's boundary 1 — re-dating it to 5 to "pass" is exactly the
  // poison the stale_scope law forbids. The owner rejects; the UI never rewrites.
  const snap = snapshotWith({ boundary_version: 5, contract_version: 2, membership_fingerprint: 'sha256:fp_mailbox_b5' }, 12);
  let sent: Receipt | null = null;
  await landCandidate(head, {
    snapshot: async () => snap,
    importReceipt: async (input) => {
      sent = input.receipt;
      return { store_version: 13, block_id: input.blockId, receipt_count: 1 };
    },
    postMission: okPost,
  });
  assert.ok(sent, 'the import was attempted');
  const scope = (sent as Receipt).scope;
  assert.equal(scope.boundary_version, 1, 'the candidate’s boundary, NOT the fresh block’s 5');
  assert.equal(scope.contract_version, 1, 'the candidate’s contract, NOT the fresh block’s 2');
  assert.equal(scope.resolution_hash, 'sha256:fp_mailbox_b5', 'only block_id + resolution_hash bind at import time');
});

test('landCandidate: stale_scope → the honest "re-run the gate" toast, and NO landed letter posted', async () => {
  const head = mergeWaitHead();
  let posted = 0;
  const result = await landCandidate(head, {
    snapshot: async () => snapshotWith({ boundary_version: 1, contract_version: 1 }, 9),
    importReceipt: async () => {
      throw {
        detail:
          'stale_scope: block sb_m1nd_mailbox is at boundary 2/contract 1, but the receipt was earned against boundary 1/contract 1 — never counted for a version it did not see (nothing was applied)',
      };
    },
    postMission: async (letter) => {
      posted++;
      return okPost(letter);
    },
  });
  assert.equal(result.landed, false);
  assert.equal(result.toast.kind, 'stale');
  assert.equal(result.toast.text, 'the boundary moved since the gate ran — re-run the gate', 'copy law verbatim');
  assert.equal(result.shouldReload, false, 'nothing changed — reloading won’t un-stale it');
  assert.equal(posted, 0, 'the law working: no landed letter when the evidence is stale');
});

test('landCandidate: an OCC conflict → reload + honest toast, never a silent retry, no landed letter', async () => {
  const head = mergeWaitHead();
  let posted = 0;
  const result = await landCandidate(head, {
    snapshot: async () => snapshotWith({ boundary_version: 1, contract_version: 1 }, 9),
    importReceipt: async () => {
      throw { detail: 'store version conflict: expected 9, actual 11 — reload and retry (nothing was applied)' };
    },
    postMission: async (letter) => {
      posted++;
      return okPost(letter);
    },
  });
  assert.equal(result.landed, false);
  assert.equal(result.toast.kind, 'conflict');
  assert.match(result.toast.text, /the store moved/);
  assert.equal(result.shouldReload, true, 'reload so the next attempt keys off the fresh version');
  assert.equal(posted, 0);
});

test('landCandidate: a head with NO candidate never touches the engine (the tray offers no button)', async () => {
  const head = heads.find((h) => h.head.phase === 'executing')!; // no receipt_candidate
  let snapshotCalls = 0;
  let importCalls = 0;
  let postCalls = 0;
  const deps: LandDeps = {
    snapshot: async () => {
      snapshotCalls++;
      return snapshotWith({ boundary_version: 1, contract_version: 1 });
    },
    importReceipt: async (input) => {
      importCalls++;
      return { store_version: 1, block_id: input.blockId, receipt_count: 1 };
    },
    postMission: async (letter) => {
      postCalls++;
      return okPost(letter);
    },
  };
  const result = await landCandidate(head, deps);
  assert.equal(result.landed, false);
  assert.equal(result.toast.kind, 'no_candidate');
  assert.equal(snapshotCalls, 0, 'no snapshot read');
  assert.equal(importCalls, 0, 'no import');
  assert.equal(postCalls, 0, 'no landed letter');
});

test('landCandidate: the block missing from the fresh store → honest error, no landing', async () => {
  const head = mergeWaitHead();
  let posted = 0;
  const result = await landCandidate(head, {
    // A snapshot whose block is a DIFFERENT id — the candidate's block is gone.
    snapshot: async () =>
      ({ present: true, store_version: 9, store: { store_version: 9, blocks: [{ block_id: 'sb_m1nd_other' }] } }) as unknown as SystemBlocksSnapshot,
    importReceipt: async (input) => ({ store_version: 10, block_id: input.blockId, receipt_count: 1 }),
    postMission: async (letter) => {
      posted++;
      return okPost(letter);
    },
  });
  assert.equal(result.landed, false);
  assert.equal(result.toast.kind, 'error');
  assert.match(result.toast.text, /re-run the gate/);
  assert.equal(posted, 0);
});

test('landErrorToast: the owner’s real strings → honest copy (stale_scope · conflict · read-only · verbatim)', () => {
  assert.deepEqual(landErrorToast({ detail: 'stale_scope: block sb_x is at boundary 2 ... nothing was applied' }), {
    toast: { kind: 'stale', text: 'the boundary moved since the gate ran — re-run the gate' },
    shouldReload: false,
  });
  const conflict = landErrorToast({ detail: 'store version conflict: expected 9, actual 11 — reload and retry' });
  assert.equal(conflict.toast.kind, 'conflict');
  assert.equal(conflict.shouldReload, true);
  const ro = landErrorToast({
    detail: "m1nd is attached read-only (--read-only); mutation tool 'receipt_import' is disabled.",
  });
  assert.equal(ro.toast.kind, 'readonly');
  assert.equal(ro.shouldReload, false);
  const other = landErrorToast({ detail: 'no workspace root is bound to this brain' });
  assert.equal(other.toast.kind, 'error');
  assert.match(other.toast.text, /no workspace root is bound/, 'the owner message, verbatim');
});

// ── F2.5e — the archive gesture (archiveHead + archiveBoundaryView + the phase) ─

test('PHASES includes the terminal archived phase (F2.5e); phaseLabel renders it verbatim', () => {
  assert.ok(PHASES.includes('archived'), 'archived is in the ordered phase enum');
  assert.equal(phaseLabel('archived'), 'archived');
  assert.doesNotMatch(phaseLabel('archived'), /done|proven|correct/i, 'copy law holds');
});

test('landErrorToast: stale_head → reload + "state moved" (the archive×import race, binding change 6)', () => {
  const { toast, shouldReload } = landErrorToast({
    detail: 'stale_head: the head is seq 3 (abc123); the next letter must be seq 4 with prev_letter_id=abc123',
  });
  assert.equal(shouldReload, true, 'a moved head reloads on the fresh truth');
  assert.match(toast.text, /state moved/i);
});

test('archiveBoundaryView: a MOVED boundary → provedAt≠currentAt, NOT still importable', () => {
  const head = mergeWaitHead(); // candidate scope: boundary 1
  const view = archiveBoundaryView(head, snapshotWith({ boundary_version: 3, contract_version: 1 }));
  assert.equal(view.provedAt, 1, 'proved at the candidate’s boundary');
  assert.equal(view.currentAt, 3, 'the block’s live boundary');
  assert.equal(view.blockPresent, true);
  assert.equal(view.stillImportable, false, 'a moved boundary stales the receipt');
});

test('archiveBoundaryView: an UNMOVED boundary → still importable (say it aloud in the confirm)', () => {
  const head = mergeWaitHead();
  const view = archiveBoundaryView(head, snapshotWith({ boundary_version: 1, contract_version: 1 }));
  assert.equal(view.currentAt, 1);
  assert.equal(view.stillImportable, true, 'the boundary has not moved — the receipt would still land');
});

test('archiveBoundaryView: the block is gone → currentAt null, not present, not importable', () => {
  const head = mergeWaitHead();
  const gone = { present: true, store_version: 9, store: { store_version: 9, blocks: [] } } as unknown as SystemBlocksSnapshot;
  const view = archiveBoundaryView(head, gone);
  assert.equal(view.currentAt, null);
  assert.equal(view.blockPresent, false);
  assert.equal(view.stillImportable, false);
});

test('archiveHead: posts a terminal archived letter (seq+1, inherits identity+gate, NO receipt) + reload', async () => {
  const head = mergeWaitHead();
  let posted: MissionLetter | undefined;
  const result = await archiveHead(head, {
    postMission: async (letter) => {
      posted = letter;
      return okPost(letter);
    },
    now: () => '2026-07-13T12:00:00Z',
  });
  assert.equal(result.archived, true);
  assert.equal(result.shouldReload, true);
  assert.match(result.toast.text, /archived — superseded by newer boundary/);
  assert.ok(posted, 'a letter was posted');
  assert.equal(posted!.phase, 'archived');
  assert.equal(posted!.mission_seq, head.head.mission_seq + 1, 'seq+1 extends the head');
  assert.equal(posted!.prev_letter_id, head.head_letter_id, '§1e chain: prev = the head letter id');
  assert.equal(posted!.mission_id, head.head.mission_id);
  assert.equal(posted!.brain_ref, head.head.brain_ref, 'inherits identity');
  assert.equal(posted!.receipt, undefined, 'NEVER landed-in-disguise: no receipt anchor (§1g)');
  assert.equal(posted!.receipt_candidate, undefined, 'the superseded candidate stays in history, not on the tip');
  assert.deepEqual(posted!.gate, head.head.gate, 'inherits the gate, like the landed letter does');
});

test('archiveHead: a stale_head race → reload + "state moved", nothing claimed archived', async () => {
  const head = mergeWaitHead();
  let posted = false;
  const result = await archiveHead(head, {
    postMission: async () => {
      posted = true;
      throw { detail: 'stale_head: the head is seq 3 (abc); the next letter must be seq 4' };
    },
  });
  assert.equal(posted, true, 'the post was attempted');
  assert.equal(result.archived, false, 'a stale head never claims an archive');
  assert.equal(result.shouldReload, true);
  assert.match(result.toast.text, /state moved/i);
});

test('archiveHead: refuses a non-merge_wait head (defensive) — never touches the engine', async () => {
  const landedHead = heads.find((h) => h.head.phase === 'landed')!;
  let called = false;
  const result = await archiveHead(landedHead, {
    postMission: async (l) => {
      called = true;
      return okPost(l);
    },
  });
  assert.equal(result.archived, false);
  assert.equal(called, false, 'no post for a non-merge_wait head');
  assert.match(result.toast.text, /only a merge_wait/i);
});
