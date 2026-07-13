/*
 * MissionCard — the archive gesture rendered honestly (HUMAN-VIEW-V2 F2.5e). Rendered
 * with react-dom/server against the captured `kind=mission` fixture.
 *
 * The teeth: a merge_wait card WITH a candidate + BOTH archive handlers offers the
 * discreet "Archive — superseded by newer boundary" (data-role="archive-superseded");
 * WITHOUT onArchive it renders no archive affordance; the confirm (opened via the
 * initialArchiveView SSR seam) shows the live TWO-BOUNDARY comparison ("proved at
 * boundary vX — the block is at vY") so the gesture is explicit and honest; when the
 * receipt is still importable it says so aloud ("still importable — archive anyway?");
 * a head whose phase is the NEW `archived` renders WITHOUT crashing (PHASE_META /
 * PHASE_ACCENT carry the entry); and the copy law holds (no "proven/done/correct").
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import MissionCard from './MissionCard';
import type { ArchiveBoundaryView, MissionHead, MissionsResponse } from '../../lib/missions';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const missions = (JSON.parse(readFileSync(join(FIX, 'missions_heads.json'), 'utf8')) as MissionsResponse)
  .missions!;
const mergeWait = missions.find((h) => h.head.phase === 'merge_wait')!;

const noop = () => {};
const noopArchive = () => {};
const noopPreview = async (): Promise<ArchiveBoundaryView> => ({
  provedAt: 1,
  currentAt: 1,
  blockPresent: true,
  stillImportable: true,
});
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visible = (out: string) => decode(out.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');

const render = (props: Partial<React.ComponentProps<typeof MissionCard>>) =>
  renderToStaticMarkup(
    React.createElement(MissionCard, {
      head: mergeWait,
      onOpenBlock: noop,
      now: Date.parse('2026-07-13T12:00:00Z'),
      ...props,
    }),
  );

// ── the discreet trigger ────────────────────────────────────────────────────────

test('a merge_wait card WITH a candidate + both archive handlers offers "Archive — superseded by newer boundary"', () => {
  const out = render({ onArchive: noopArchive, onArchivePreview: noopPreview });
  assert.match(out, /data-role="archive-superseded"/);
  assert.match(visible(out), /Archive — superseded by newer boundary/);
});

test('WITHOUT onArchive the card offers no archive affordance (a pure read-only render)', () => {
  const out = render({});
  assert.doesNotMatch(out, /data-role="archive-superseded"/, 'no handler → no archive button');
  assert.doesNotMatch(out, /data-role="archive-confirm"/);
});

// ── the two-boundary confirm (the explicit gesture + the honesty) ───────────────

test('the archive confirm shows the live TWO-BOUNDARY comparison (proved-at vs the block’s current)', () => {
  const view: ArchiveBoundaryView = { provedAt: 1, currentAt: 3, blockPresent: true, stillImportable: false };
  const out = render({ onArchive: noopArchive, onArchivePreview: noopPreview, initialArchiveView: view });
  assert.match(out, /data-role="archive-confirm"/);
  const t = visible(out);
  assert.match(t, /proved at boundary v1 — the block is at v3/, 'both boundaries, live');
  assert.match(out, /data-role="archive-confirm-go"/, 'the explicit Archive button');
  assert.match(out, /data-role="archive-cancel"/, 'and a cancel');
  // A moved boundary is NOT still importable — no "archive anyway" line.
  assert.doesNotMatch(out, /data-role="archive-still-importable"/);
  // The confirm replaces the trigger — never both at once.
  assert.doesNotMatch(out, /data-role="archive-superseded"/);
});

test('the confirm says "still importable — archive anyway?" ONLY when the boundary has not moved', () => {
  const stillImportable: ArchiveBoundaryView = { provedAt: 2, currentAt: 2, blockPresent: true, stillImportable: true };
  const out = render({ onArchive: noopArchive, onArchivePreview: noopPreview, initialArchiveView: stillImportable });
  assert.match(out, /data-role="archive-still-importable"/);
  assert.match(visible(out), /still importable — archive anyway\?/);
  assert.match(visible(out), /proved at boundary v2 — the block is at v2/);
});

test('a gone block reads "v— (gone)", never a fabricated version', () => {
  const gone: ArchiveBoundaryView = { provedAt: 1, currentAt: null, blockPresent: false, stillImportable: false };
  const out = render({ onArchive: noopArchive, onArchivePreview: noopPreview, initialArchiveView: gone });
  assert.match(visible(out), /the block is at v— \(gone\)/);
  assert.doesNotMatch(out, /data-role="archive-still-importable"/);
});

// ── the NEW phase renders without crashing (PHASE_META / PHASE_ACCENT) ───────────

test('a head whose phase is the new `archived` renders WITHOUT crashing (glyph + label present)', () => {
  const archivedHead: MissionHead = {
    ...mergeWait,
    head: { ...mergeWait.head, phase: 'archived', receipt_candidate: undefined, receipt: undefined },
  };
  const out = renderToStaticMarkup(
    React.createElement(MissionCard, { head: archivedHead, onOpenBlock: noop, now: 0 }),
  );
  assert.match(out, /data-phase="archived"/, 'the card is tagged with the new phase');
  assert.match(visible(out), /archived/, 'the phase label renders verbatim (no PHASE_META crash)');
  // A terminal archived card offers neither import nor archive affordances.
  assert.doesNotMatch(out, /data-role="import-receipt"/);
  assert.doesNotMatch(out, /data-role="archive-superseded"/);
});

// ── copy law ────────────────────────────────────────────────────────────────────

test('COPY LAW: the archive affordance never says proven/done/correct', () => {
  const view: ArchiveBoundaryView = { provedAt: 1, currentAt: 3, blockPresent: true, stillImportable: false };
  const t = visible(render({ onArchive: noopArchive, onArchivePreview: noopPreview, initialArchiveView: view }));
  assert.doesNotMatch(t, /\bproven\b/i);
  assert.doesNotMatch(t, /\bdone\b/i);
  assert.doesNotMatch(t, /\bcorrect\b/i);
});
