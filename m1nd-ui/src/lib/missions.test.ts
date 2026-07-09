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
  blockLabelFromId,
  composeSeq1Letter,
  elapsedLabel,
  gateLine,
  mergeWaitStatusLine,
  newMissionId,
  PHASES,
  phaseCounts,
  phaseLabel,
  receiptAnchorLabel,
  sha256Hex,
  sortForTray,
  type MissionsResponse,
} from './missions';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const heads = (JSON.parse(readFileSync(join(FIX, 'missions_heads.json'), 'utf8')) as MissionsResponse)
  .missions!;

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

// ── §1d — the landed-law, encoded ─────────────────────────────────────────────

test('mergeWaitStatusLine flies "gate green — receipt not landed" on a green gate with no receipt', () => {
  const mergeWait = heads.find((h) => h.head.phase === 'merge_wait')!;
  assert.equal(mergeWaitStatusLine(mergeWait.head), 'gate green — receipt not landed');
  // Not a merge_wait → no line (the law only speaks where it applies).
  const landed = heads.find((h) => h.head.phase === 'landed')!;
  assert.equal(mergeWaitStatusLine(landed.head), null);
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
