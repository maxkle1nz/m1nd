/*
 * The scan WAIT PANEL at the pixel boundary (scanMachine × BuildMapEmpty,
 * docs/uml/scan-loading.md). Rendered with react-dom/server, the house pattern.
 * The teeth:
 *  - NEVER-DEAD: an in-flight scanPhase renders the panel with a live elapsed
 *    clock, a named phase, and visible movement (the calm pulse) — the screen
 *    can no longer look frozen while the owner clusters;
 *  - the REAL node count rides the copy when known; absent stays generic;
 *  - `slow` adds the honest "usually takes a while" note and keeps the clock;
 *  - "Stop waiting" renders only when the abort gesture is wired, and the
 *    canceled toast wears the NEUTRAL tint (a user gesture, not a failure);
 *  - a settled phase (or no scanPhase at all — the legacy prop surface) renders
 *    NO panel: the old scanning/scanToast behavior is byte-compatible;
 *  - no percentage anywhere (no fabricated progress) + the copy law.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import BuildMap from './BuildMap';
import type { SystemBlocksSnapshot } from '../../lib/buildMap';
import type { ScanPhaseName } from '../../lib/scanMachine';

const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visible = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');
const noop = () => {};

const emptySnap: SystemBlocksSnapshot = {
  present: false,
  honest: 'no skeleton yet — import a seed or run a scan',
};

const phase = (p: ScanPhaseName, elapsedMs: number, nodeCount: number | null = 3210) => ({
  phase: p,
  elapsedMs,
  nodeCount,
});

const empty = (extra: Record<string, unknown> = {}) => (
  <BuildMap snapshot={emptySnap} rollup={null} onScan={noop} {...extra} />
);

// ── the wait panel, in flight ─────────────────────────────────────────────────

test('NEVER-DEAD: clustering renders the panel with phase + live elapsed + pulse', () => {
  const el = empty({ scanning: true, scanPhase: phase('clustering', 42_000) });
  const out = html(el);
  assert.match(out, /data-role="scan-wait"/);
  assert.match(out, /data-scan-phase="clustering"/);
  assert.match(out, /data-role="scan-phase-label"/);
  assert.match(out, /data-role="scan-elapsed"/);
  assert.match(out, /animate-pulse/, 'the calm heartbeat — visible movement, never a dead screen');
  const text = visible(el);
  assert.match(text, /clustering…/);
  assert.match(text, /0:42/, 'the elapsed clock is REAL time, mm:ss');
});

test('the wait copy carries the real node count when known', () => {
  const text = visible(empty({ scanning: true, scanPhase: phase('clustering', 5_000, 3210) }));
  assert.match(text, /clustering 3,210 nodes into a candidate map/);
});

test('an unknown node count keeps the copy generic (never an invented number)', () => {
  const text = visible(empty({ scanning: true, scanPhase: phase('clustering', 5_000, null) }));
  assert.match(text, /clustering this repo into a candidate map/);
  assert.doesNotMatch(text, /\d+ nodes/);
});

test('submitting names its own phase (the request is leaving)', () => {
  const el = empty({ scanning: true, scanPhase: phase('submitting', 0) });
  assert.match(html(el), /data-scan-phase="submitting"/);
  assert.match(visible(el), /sending the scan request…/);
});

test('slow adds the honest "usually takes a while" note and KEEPS the clock', () => {
  const el = empty({ scanning: true, scanPhase: phase('slow', 73_000) });
  const out = html(el);
  assert.match(out, /data-role="scan-slow-note"/);
  const text = visible(el);
  assert.match(text, /3,210 nodes usually takes a while/);
  assert.match(text, /a live naming runner can add ~2 minutes/);
  assert.match(text, /request stays open/i);
  assert.match(text, /1:13/, 'the clock still counts in slow — the wait never looks dead');
});

test('below the threshold there is NO slow note (the note is earned by real time)', () => {
  const out = html(empty({ scanning: true, scanPhase: phase('clustering', 3_000) }));
  assert.doesNotMatch(out, /data-role="scan-slow-note"/);
});

test('"Stop waiting" renders only when the abort gesture is wired, with honest title', () => {
  const without = html(empty({ scanning: true, scanPhase: phase('clustering', 2_000) }));
  assert.doesNotMatch(without, /data-role="scan-cancel"/);
  const withCancel = html(
    empty({ scanning: true, scanPhase: phase('clustering', 2_000), onCancelScan: noop }),
  );
  assert.match(withCancel, /data-role="scan-cancel"/);
  assert.match(withCancel, /owner may still finish/, 'the title says the owner is not stopped');
});

// ── settled phases + the legacy surface stay byte-compatible ──────────────────

test('a settled phase renders NO panel (idle / candidate_ready / error)', () => {
  for (const p of ['idle', 'candidate_ready', 'error'] as ScanPhaseName[]) {
    const out = html(empty({ scanPhase: phase(p, 9_000) }));
    assert.doesNotMatch(out, /data-role="scan-wait"/, p);
  }
});

test('no scanPhase at all (the legacy prop surface) renders no panel; the locked button stays', () => {
  const out = html(empty({ scanning: true }));
  assert.doesNotMatch(out, /data-role="scan-wait"/);
  assert.match(out, /data-role="scan-repo"[^>]*disabled=""/, 'the legacy lock is untouched');
});

test('the canceled toast wears the NEUTRAL tint — a user gesture, not a failure', () => {
  const el = empty({
    scanToast: { kind: 'canceled', text: 'stopped waiting — the owner may still finish this scan; reload to check' },
  });
  const out = html(el);
  assert.match(out, /data-role="scan-toast"/);
  assert.match(out, /data-toast-kind="canceled"/);
  assert.doesNotMatch(out, /data-toast-kind="canceled"[^>]*state-failure/, 'never the clay failure tint');
  assert.match(visible(el), /owner may still finish this scan/);
});

// ── honesty laws at the pixel boundary ────────────────────────────────────────

test('COPY LAW + no fabricated progress on every wait surface', () => {
  const surfaces = [
    html(empty({ scanning: true, scanPhase: phase('submitting', 0), onCancelScan: noop })),
    html(empty({ scanning: true, scanPhase: phase('clustering', 42_000), onCancelScan: noop })),
    html(empty({ scanning: true, scanPhase: phase('slow', 120_000), onCancelScan: noop })),
  ].map((s) => decode(s.replace(/<[^>]+>/g, ' ')));
  for (const text of surfaces) {
    assert.doesNotMatch(text, /\bproven\b/i);
    assert.doesNotMatch(text, /\bdone\b/i);
    assert.doesNotMatch(text, /\bcorrect\b/i);
    assert.doesNotMatch(text, /%|\bpercent/i, 'no percentage — the owner emits no progress fraction');
  }
});
