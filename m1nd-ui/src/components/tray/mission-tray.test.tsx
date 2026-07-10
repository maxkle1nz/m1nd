/*
 * MissionTray render — honesty at the pixel boundary (HUMAN-VIEW-V2 F2.5 §3).
 * Rendered with react-dom/server against a captured fixture faithful to the
 * `GET /api/mailbox?kind=mission` shape (missions_heads.json): 1 executing, 1
 * merge_wait with a green gate, 1 landed with an imported receipt, 1 failed.
 *
 * The acceptance teeth: the `failed` mission is PINNED atop the tray (§3c) even
 * though it is the oldest; the `landed` card shows the receipt anchor `receipt ✓
 * store v9` (§3b) and its provenance chain expands (§3f); the `merge_wait` card
 * flies the §1d landed-law "gate green — receipt not landed" and NEVER a receipt;
 * the empty state invites pointing an agent at a block (§3d); a pre-F2.5a owner
 * degrades honestly (§3 degraded); the collapsed strip carries per-phase counts
 * (§3a); and the copy law holds (no "proven/done/correct").
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import MissionTray from './MissionTray';
import type { MissionsResponse } from '../../lib/missions';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const missions = (JSON.parse(readFileSync(join(FIX, 'missions_heads.json'), 'utf8')) as MissionsResponse)
  .missions!;
const LANDED = 'msn_00000000c003';

const noop = () => {};
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visible = (out: string) => decode(out.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');

const tray = (props: Partial<React.ComponentProps<typeof MissionTray>> = {}) =>
  renderToStaticMarkup(
    React.createElement(MissionTray, {
      missions,
      status: 'ready',
      expanded: true,
      onToggleExpanded: noop,
      onToggleProvenance: noop,
      onOpenBlock: noop,
      now: Date.parse('2026-07-09T12:00:00Z'),
      ...props,
    }),
  );

// ── §3c — failure is never folded away ────────────────────────────────────────

test('the failed mission is PINNED atop the tray, above the newer live missions', () => {
  const out = tray();
  const phases = [...out.matchAll(/data-role="mission-card"[^>]*data-phase="([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(phases, ['failed', 'executing', 'merge_wait', 'landed'], 'failed leads, then newest-first');
});

test('a failed card carries the dismiss ✕ (un-pin), a live card does not', () => {
  const out = tray({ onDismiss: noop });
  // Exactly one dismiss control — on the single failed card.
  assert.equal((out.match(/data-role="mission-dismiss"/g) ?? []).length, 1);
});

test('a dismissed failure is un-pinned from the list (data untouched — presentation only)', () => {
  const out = tray({ onDismiss: noop, dismissedIds: new Set(['msn_00000000d004']) });
  const phases = [...out.matchAll(/data-role="mission-card"[^>]*data-phase="([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(phases, ['executing', 'merge_wait', 'landed'], 'the failed card is gone from the list');
});

// ── §3b / §1d — the landed anchor + the landed-law ────────────────────────────

test('the landed card shows the receipt anchor `receipt ✓ store v9` (§3b/§1d)', () => {
  const t = visible(tray());
  assert.match(t, /receipt ✓ store v9/);
});

test('the merge_wait card flies the §1d landed-law and shows NO receipt anchor', () => {
  const out = tray();
  const t = visible(out);
  assert.match(t, /gate green — receipt not landed/, 'the landed-law on the UI');
  assert.match(out, /data-role="mission-landed-law"/);
  // The merge_wait card carries a gate line but never a receipt anchor (only landed does).
  assert.match(t, /cargo test -p m1nd-mcp · exit 0/);
  assert.equal((out.match(/data-role="mission-receipt"/g) ?? []).length, 1, 'only the landed card shows a receipt');
});

test('a card shows seat · capability · runner_id and the block name (§3b)', () => {
  const t = visible(tray());
  assert.match(t, /hand · build-runner · runner-build-1/);
  assert.match(t, /core graph kernel/, 'the executing block name, de-slugged from its id');
  assert.match(t, /system blocks/, 'the landed block name');
});

// ── §3f — provenance on a landed card ─────────────────────────────────────────

test('the landed card expands to its provenance chain (packet → runner → gate → receipt)', () => {
  const out = tray({ provenanceIds: new Set([LANDED]) });
  assert.match(out, /data-role="mission-provenance"/);
  const t = visible(out);
  assert.match(t, /sha256:landedpacket/, 'the packet_ref');
  assert.match(t, /runner-build-1/, 'the runner id');
  assert.match(t, /sha256:gatelogc/, 'the gate artifact hash');
  assert.match(t, /store v9/, 'the receipt store_version');
});

test('provenance is closed by default (only the toggle shows)', () => {
  const out = tray();
  assert.doesNotMatch(out, /data-role="mission-provenance"/);
  assert.match(out, /data-role="mission-provenance-toggle"/, 'the landed card offers the toggle');
});

// ── §3a — the collapsed strip ─────────────────────────────────────────────────

test('the collapsed strip carries per-phase counts + the expand toggle (§3a)', () => {
  const out = tray({ expanded: false });
  assert.match(out, /data-role="mission-tray"[^>]*data-expanded="false"/);
  assert.match(out, /data-role="mission-tray-toggle"/);
  const strip = [...out.matchAll(/data-role="mission-strip-count"[^>]*data-phase="([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(strip.sort(), ['executing', 'failed', 'landed', 'merge_wait'], 'a count per present phase');
});

// ── §3d — empty + §3 degraded ─────────────────────────────────────────────────

test('an empty ready tray invites pointing an agent at a block (§3d)', () => {
  const out = tray({ missions: [] });
  assert.match(out, /data-role="mission-tray-empty"/);
  assert.match(visible(out), /no missions yet — point an agent at a block/);
});

test('a pre-F2.5a owner degrades honestly — "mission letters need an updated owner"', () => {
  const out = tray({ missions: [], status: 'unsupported' });
  assert.match(out, /data-role="mission-tray-unsupported"/);
  assert.match(visible(out), /mission letters need an updated owner/);
  // The shell never breaks — the tray frame is still there.
  assert.match(out, /data-role="mission-tray"/);
});

test('a read error is shown honestly, never a silent empty tray', () => {
  const out = tray({ missions: [], status: 'error', error: 'network' });
  assert.match(out, /data-role="mission-tray-error"/);
  assert.match(visible(out), /couldn’t read mission letters — network/);
});

// ── Copy law ──────────────────────────────────────────────────────────────────

test('COPY LAW: the tray never says proven/done/correct', () => {
  const t = visible(tray({ provenanceIds: new Set([LANDED]), onDismiss: noop }));
  assert.doesNotMatch(t, /\bproven\b/i);
  assert.doesNotMatch(t, /\bdone\b/i);
  assert.doesNotMatch(t, /\bcorrect\b/i);
});

// ── Layout law (field bug 2026-07-10) ─────────────────────────────────────────
// The tray is an IN-FLOW flex column of the shell row, never a fixed overlay: a
// fixed tray painted over (and click-blocked) the map's block panel and the
// tree's drawer, and its `/97` alpha compiled to NO background (transparent
// text soup). Pinned here at the class boundary; the geometry is browser-proven.

function rootClasses(out: string): string {
  const m = out.match(/data-role="mission-tray"[^>]*class="([^"]*)"/);
  assert.ok(m, 'the tray root carries a class list');
  return m![1];
}

test('LAYOUT LAW: the expanded tray is in-flow (no fixed/z overlay) on solid ground', () => {
  const cls = rootClasses(tray());
  assert.doesNotMatch(cls, /\bfixed\b/, 'never position:fixed — it must make room, not paint over');
  assert.doesNotMatch(cls, /\bz-\d/, 'no z-index war with the surface beside it');
  assert.match(cls, /\bshrink-0\b/, 'a rigid flex column');
  assert.match(cls, /\bw-80\b/, 'the ~320px card column');
  assert.match(cls, /\bbg-porcelain\b/, 'solid ground');
  assert.doesNotMatch(cls, /bg-porcelain\/\d+/, 'no alpha ground — /97 compiled to no background at all');
});

test('LAYOUT LAW: the collapsed strip is in-flow (no fixed/z overlay) on solid ground', () => {
  const cls = rootClasses(tray({ expanded: false }));
  assert.doesNotMatch(cls, /\bfixed\b/);
  assert.doesNotMatch(cls, /\bz-\d/);
  assert.match(cls, /\bshrink-0\b/);
  assert.match(cls, /\bw-9\b/, 'the thin strip');
  assert.match(cls, /\bbg-porcelain\b/);
  assert.doesNotMatch(cls, /bg-porcelain\/\d+/);
});
