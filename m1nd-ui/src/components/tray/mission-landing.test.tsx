/*
 * MissionCard — the human-landing affordance rendered honestly (HUMAN-VIEW-V2 F2.5d §6).
 * Rendered with react-dom/server against the captured `kind=mission` fixture: the
 * merge_wait head (a green gate + a complete receipt_candidate) and the landed head.
 *
 * The teeth: a merge_wait card WITH a candidate + a handler offers "Import this
 * receipt" (data-role="import-receipt"); WITHOUT the handler it renders read-only (no
 * button); WITHOUT a candidate it flies the honest "gate green — no candidate
 * attached" and never a button; the confirm shows EXACTLY what will be imported (block,
 * type, the truncated artifact hash, the evidence refs) so the gesture is explicit; a
 * long hash is truncated for display only; the landed card shows the receipt anchor and
 * offers no import; and the copy law holds (no "proven/done/correct").
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import MissionCard from './MissionCard';
import type { MissionHead, MissionsResponse } from '../../lib/missions';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const missions = (JSON.parse(readFileSync(join(FIX, 'missions_heads.json'), 'utf8')) as MissionsResponse)
  .missions!;
const mergeWait = missions.find((h) => h.head.phase === 'merge_wait')!;
const landed = missions.find((h) => h.head.phase === 'landed')!;

const noop = () => {};
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visible = (out: string) => decode(out.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');

const render = (props: Partial<React.ComponentProps<typeof MissionCard>>) =>
  renderToStaticMarkup(
    React.createElement(MissionCard, {
      head: mergeWait,
      onOpenBlock: noop,
      now: Date.parse('2026-07-09T12:00:00Z'),
      ...props,
    }),
  );

// ── the button ────────────────────────────────────────────────────────────────

test('a merge_wait card WITH a candidate + a handler offers "Import this receipt" (§6-F2.5d)', () => {
  const out = render({ onImportReceipt: noop });
  assert.match(out, /data-role="import-receipt"/);
  assert.match(visible(out), /Import this receipt/);
});

test('WITHOUT the handler the card offers no import button (a pure read-only render)', () => {
  const out = render({});
  assert.doesNotMatch(out, /data-role="import-receipt"/, 'no handler → no button');
});

test('a merge_wait card WITHOUT a candidate flies the honest "no candidate attached", no button', () => {
  const noCand: MissionHead = { ...mergeWait, head: { ...mergeWait.head, receipt_candidate: undefined } };
  const out = renderToStaticMarkup(
    React.createElement(MissionCard, { head: noCand, onOpenBlock: noop, onImportReceipt: noop, now: 0 }),
  );
  assert.doesNotMatch(out, /data-role="import-receipt"/);
  assert.match(visible(out), /gate green — no candidate attached/);
});

// ── the confirm (the explicit gesture) ─────────────────────────────────────────

test('the confirm shows EXACTLY what will be imported — block, type, hash, evidence refs', () => {
  const out = render({ onImportReceipt: noop, initialConfirmOpen: true });
  assert.match(out, /data-role="import-confirm"/);
  const t = visible(out);
  assert.match(t, /mailbox/, 'the block name');
  assert.match(out, /data-role="import-type"[^>]*>test</, 'the receipt type');
  assert.match(t, /sha256:candloglb/, 'the artifact hash (short — shown verbatim)');
  assert.match(t, /artifact:\/\/run\/2/, 'the evidence ref');
  assert.match(out, /data-role="import-confirm-go"/, 'the explicit Import button');
  assert.match(out, /data-role="import-cancel"/, 'and a cancel');
  // The button is replaced by the confirm — never both at once.
  assert.doesNotMatch(out, /data-role="import-receipt"/);
});

test('a long artifact hash is truncated in the confirm (display-only, never mangled on the wire)', () => {
  const longHash = 'sha256:' + 'a'.repeat(64);
  const head: MissionHead = {
    ...mergeWait,
    head: {
      ...mergeWait.head,
      receipt_candidate: {
        ...mergeWait.head.receipt_candidate!,
        evidence: { ...mergeWait.head.receipt_candidate!.evidence, artifact_hash: longHash },
      },
    },
  };
  const out = renderToStaticMarkup(
    React.createElement(MissionCard, { head, onOpenBlock: noop, onImportReceipt: noop, initialConfirmOpen: true, now: 0 }),
  );
  const t = visible(out);
  assert.match(t, /sha256:aaaaaaaaa…aaaaaa/, 'the truncated middle (head…tail)');
  assert.doesNotMatch(t, /a{30}/, 'the full 64-hex is NOT shown');
});

// ── post-success + copy law ────────────────────────────────────────────────────

test('the landed fixture renders the receipt anchor (post-success the card shows store vN), no import', () => {
  const out = renderToStaticMarkup(
    React.createElement(MissionCard, { head: landed, onOpenBlock: noop, onImportReceipt: noop, now: 0 }),
  );
  assert.match(visible(out), /receipt ✓ store v9/);
  assert.doesNotMatch(out, /data-role="import-receipt"/, 'a landed card has no import button');
});

test('COPY LAW: the landing affordance never says proven/done/correct', () => {
  const t = visible(render({ onImportReceipt: noop, initialConfirmOpen: true }));
  assert.doesNotMatch(t, /\bproven\b/i);
  assert.doesNotMatch(t, /\bdone\b/i);
  assert.doesNotMatch(t, /\bcorrect\b/i);
});
