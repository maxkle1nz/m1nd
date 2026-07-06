/*
 * mailbox pure-logic — the caixinha's fate/chapter/link derivation (§4A.11).
 * Fixtures are REAL captured `/api/mailbox` shapes (neutral repo names), never
 * hand-written inline — so the tests can't drift from the wire (INV-01).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  type MailboxResponse,
  classChip,
  CHIP_TONE_CLASS,
  fateLine,
  isOpenFate,
  dayChapters,
  dayKey,
  boxIdSet,
  headerLine,
  mailboxEchoMatches,
} from './mailbox';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));

const boxA = load<MailboxResponse>('mailbox_box_a.json');
const boxB = load<MailboxResponse>('mailbox_box_b.json');
const medulla = load<MailboxResponse>('mailbox_medulla.json');
const dangling = load<MailboxResponse>('mailbox_dangling.json');
const wrongEcho = load<MailboxResponse>('mailbox_wrong_echo.json');

// ── The matte class-chip palette (§4A.11 table) — five families, NO violet ──────
test('class chips map to the five matte tone families (never iris)', () => {
  assert.equal(classChip('win').tone, 'sage');
  assert.equal(classChip('bug').tone, 'brick');
  assert.equal(classChip('honesty').tone, 'amber');
  assert.equal(classChip('friction').tone, 'stone');
  assert.equal(classChip('triage').tone, 'slate');
  assert.equal(classChip('recibo').tone, 'slate');
  // Any unknown class degrades to calm stone — never an alarm hue.
  assert.equal(classChip('memory_misdelivery').tone, 'stone');
  // No chip tone references the violet/iris family.
  for (const cls of Object.values(CHIP_TONE_CLASS)) {
    assert.doesNotMatch(cls, /iris|veil|wisteria|#7c3aed|#a78bfa|#ede9fe/i);
  }
});

// ── Fate lines — the visible loop (● / ◍ / ↳ / ◌) ───────────────────────────────
test('fate lines render the right glyph per derived state', () => {
  const ids = boxIdSet(boxA.letters);
  const byTool = (t: string) => boxA.letters.find((l) => l.tool === t)!;

  // wet_ink bug → ● open
  assert.equal(fateLine(byTool('impact'), ids).glyph, '●');
  assert.match(fateLine(byTool('impact'), ids).text, /open/);

  // in_flight friction → ◍ in flight
  assert.equal(fateLine(byTool('trust'), ids).glyph, '◍');
  assert.match(fateLine(byTool('trust'), ids).text, /flight/);

  // external friction → ◌ external, stone tone (never violet)
  const ext = fateLine(byTool('context7'), ids);
  assert.equal(ext.glyph, '◌');
  assert.equal(ext.tone, 'stone');
  assert.match(ext.text, /external/);

  // fired_clay bug (answered) → ↳ answered by letter N, linking IN-BOX
  const fired = fateLine(byTool('seek'), ids);
  assert.equal(fired.glyph, '↳');
  assert.equal(fired.broken, false);
  assert.ok(fired.linkIds.length === 1);
  assert.ok(ids.has(fired.linkIds[0]), 'the receipt link resolves in this box');
});

// ── INV-18: receipts are always linked (down-pointing), breakage is honest ──────
test('INV-18: a receipt points DOWN at what it answers, resolved in-box', () => {
  const ids = boxIdSet(boxA.letters);
  const receipt = boxA.letters.find((l) => l.class === 'triage')!;
  const fl = fateLine(receipt, ids);
  assert.equal(fl.glyph, '↓');
  assert.equal(fl.broken, false);
  assert.match(fl.text, /answers letter/);
  for (const id of fl.linkIds) assert.ok(ids.has(id), 'the answered letter is in this box');
});

test('INV-18: a dangling receipt link renders the honest breakage, never a bare chip', () => {
  const ids = boxIdSet(dangling.letters);
  const bug = dangling.letters[0]; // fired_clay whose answered_by id is NOT in-box
  const fl = fateLine(bug, ids);
  assert.equal(fl.broken, true);
  assert.match(fl.text, /receipt not found/);
  assert.equal(fl.linkIds.length, 0, 'no fake link is offered when it cannot resolve');
});

test('every ↳/↓ link a fixture renders resolves to an in-box letter id (INV-18 sweep)', () => {
  for (const box of [boxA, boxB, medulla]) {
    const ids = boxIdSet(box.letters);
    for (const l of box.letters) {
      const fl = fateLine(l, ids);
      if (!fl.broken) for (const id of fl.linkIds) assert.ok(ids.has(id), `link ${id} in-box`);
    }
  }
});

// ── Day chapters — chronological, newest first, timezone-honest ─────────────────
test('letters group into day-chapters, newest chapter and letter first', () => {
  const chapters = dayChapters(boxA.letters);
  assert.deepEqual(
    chapters.map((c) => c.key),
    ['2026-07-05', '2026-07-04'],
    'newest day first',
  );
  // Within 2026-07-05, newest ts first.
  const day05 = chapters[0].letters.map((l) => l.ts);
  const sorted = [...day05].sort((a, b) => (a < b ? 1 : -1));
  assert.deepEqual(day05, sorted, 'newest letter first within a chapter');
  // dayKey is the ts head, verbatim (never client re-zoned).
  assert.equal(dayKey('2026-07-04T09:12:00Z'), '2026-07-04');
  assert.equal(dayKey('garbage'), 'no date');
});

// ── Counts stay honest — external visible, never in "abertas" ───────────────────
test('the header line states the whole truth; open excludes external', () => {
  const line = headerLine(boxA.counts);
  assert.match(line, /6 letters/); // 3 wet + 1 flight + 1 fired + 1 external
  assert.match(line, /4 open/); // wet_ink(3) + in_flight(1)
  assert.match(line, /1 external/);
  assert.equal(boxA.counts.open, boxA.counts.wet_ink + boxA.counts.in_flight);
  assert.equal(isOpenFate('external'), false, 'external is never open');
  assert.equal(isOpenFate('fired_clay'), false, 'answered is never open');
  assert.equal(isOpenFate('wet_ink'), true);
  assert.equal(isOpenFate('in_flight'), true);
});

// ── INV-17: the box scope — echo mismatch is dropped ────────────────────────────
test('INV-17: box A and box B share NO letter ids (no cross-box fold)', () => {
  const idsA = boxIdSet(boxA.letters);
  const idsB = boxIdSet(boxB.letters);
  for (const id of idsA) assert.ok(!idsB.has(id), `box A id ${id} absent from box B`);
});

test('INV-17: a matching echo renders; a wrong echo is dropped', () => {
  assert.equal(mailboxEchoMatches('/path/to/repo-alpha', boxA), true, 'right echo → render');
  // The wrong-echo fixture answered for repo-other while we asked repo-alpha.
  assert.equal(mailboxEchoMatches('/path/to/repo-alpha', wrongEcho), false, 'wrong echo → drop');
  // The medulla box trusts the medulla echo.
  assert.equal(mailboxEchoMatches('medulla', medulla), true);
  assert.equal(mailboxEchoMatches('medulla', boxA), false, 'a project echo is not the medulla');
});

// ── The medulla box — projectless only, external present + uncounted ────────────
test('the medulla box carries an external (Context7-shaped) letter, uncounted', () => {
  const ids = boxIdSet(medulla.letters);
  const ext = medulla.letters.find((l) => l.state === 'external')!;
  assert.ok(ext, 'the medulla holds an external letter');
  assert.equal(ext.tool, 'context7');
  const fl = fateLine(ext, ids);
  assert.equal(fl.glyph, '◌');
  assert.equal(fl.tone, 'stone', 'external wears stone, not iris');
  // It is NOT in the abertas count.
  assert.equal(medulla.counts.open, medulla.counts.wet_ink + medulla.counts.in_flight);
  assert.equal(medulla.counts.external, 1);
  // No repo-bearing letter in the medulla box (MED-INV-10 fixture discipline).
  for (const l of medulla.letters) {
    const named = l.repo.trim() !== '' && l.repo !== 'all';
    assert.ok(!named, `medulla letter names no project (repo="${l.repo}")`);
  }
});
