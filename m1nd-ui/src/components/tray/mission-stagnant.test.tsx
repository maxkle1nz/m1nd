/*
 * MissionCard — the stagnant judging/executing dismiss, rendered honestly (§3c
 * extended). Rendered with react-dom/server against the captured `kind=mission`
 * fixture. The teeth: a judging/executing head unmoved > 24h WITH onDismiss offers the
 * ✕; a fresh one does not; a pure read (no onDismiss) offers nothing; and the confirm
 * (opened via the initialDismissConfirmOpen SSR seam) is HONEST that it only hides the
 * card — the letter stays on the mailbox, a contract transition is future work.
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
const executing = missions.find((h) => h.head.phase === 'executing')!; // updated 2026-07-09T10:03Z

const STAGNANT_NOW = Date.parse('2026-07-13T12:00:00Z'); // 4+ days later
const FRESH_NOW = Date.parse('2026-07-09T12:00:00Z'); // ~2h later

const noop = () => {};
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visible = (out: string) => decode(out.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');

const render = (props: Partial<React.ComponentProps<typeof MissionCard>>, head: MissionHead = executing) =>
  renderToStaticMarkup(React.createElement(MissionCard, { head, onOpenBlock: noop, now: STAGNANT_NOW, ...props }));

test('a stagnant executing head WITH onDismiss offers the ✕', () => {
  const out = render({ onDismiss: noop });
  assert.match(out, /data-role="mission-stagnant-dismiss"/);
});

test('a FRESH executing head offers no stagnant dismiss', () => {
  const out = render({ onDismiss: noop, now: FRESH_NOW });
  assert.doesNotMatch(out, /data-role="mission-stagnant-dismiss"/);
});

test('WITHOUT onDismiss a stagnant card offers nothing (a pure read-only render)', () => {
  const out = render({});
  assert.doesNotMatch(out, /data-role="mission-stagnant-dismiss"/);
  assert.doesNotMatch(out, /data-role="dismiss-confirm"/);
});

test('the confirm is honest — hides the card, the letter stays, a transition is future work', () => {
  const out = render({ onDismiss: noop, initialDismissConfirmOpen: true });
  assert.match(out, /data-role="dismiss-confirm"/);
  assert.match(out, /data-role="dismiss-confirm-go"/, 'the explicit Dismiss');
  assert.match(out, /data-role="dismiss-confirm-cancel"/, 'and a cancel');
  const t = visible(out);
  assert.match(t, /Hides it from the tray/i);
  assert.match(t, /the letter itself stays on the mailbox/i);
  assert.match(t, /a contract transition is future work/i);
});

test('a merge_wait head is never stagnant-dismissable (only judging/executing)', () => {
  const mergeWait = missions.find((h) => h.head.phase === 'merge_wait')!;
  const out = render({ onDismiss: noop }, mergeWait);
  assert.doesNotMatch(out, /data-role="mission-stagnant-dismiss"/);
});

test('COPY LAW: the stagnant dismiss confirm never says proven/done/correct', () => {
  const t = visible(render({ onDismiss: noop, initialDismissConfirmOpen: true }));
  assert.doesNotMatch(t, /\bproven\b/i);
  assert.doesNotMatch(t, /\bdone\b/i);
  assert.doesNotMatch(t, /\bcorrect\b/i);
});
