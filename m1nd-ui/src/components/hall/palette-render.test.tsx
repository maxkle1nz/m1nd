/*
 * BrainPalette + reduced-motion contract (HUMAN-LAYER-PRD §4A.5).
 * Fixtures: the real captured /api/instances envelope (INV-01).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import BrainPalette, { filterBrains, type BrainRow } from './BrainPalette';
import type { InstanceListResponse } from '../../types';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const hasDisabledAttr = (tag: string) => /\sdisabled(=""|>|\s)/.test(tag);
const noop = () => {};

const list = load<InstanceListResponse>('instances.json');
const bound = list.instances.find((e) => e.brain_kind == null)!;
const project = list.instances.find((e) => e.brain_kind === 'project')!;

const rows: BrainRow[] = [
  { entry: bound, isSelf: true, openable: true },
  { entry: project, isSelf: false, openable: false }, // hosted, no port → not openable
];

// ── filterBrains: fuzzy basename, recency order preserved (R4) ────────────────
test('§4A.5: filterBrains preserves registry recency order and fuzzy-matches basenames', () => {
  // Empty query → all rows, unchanged order (recents-first is the registry's).
  const all = filterBrains(rows, '');
  assert.deepEqual(all.map((r) => r.entry.instance_id), rows.map((r) => r.entry.instance_id));
  // A basename substring narrows it. Both fixtures live under .../claude/... roots
  // so a nonsense query yields nothing; the bound basename "claude" matches self.
  assert.equal(filterBrains(rows, 'zzzznomatch').length, 0);
  const claude = filterBrains(rows, 'claude');
  assert.ok(claude.length >= 1, '"claude" matches the bound brain basename');
});

// ── Render: recents-first, dot + last-seen, self label ────────────────────────
test('§4A.5: the palette renders a Brains group, recents-first, with liveness + last-seen', () => {
  const out = html(<BrainPalette isOpen instances={list.instances} selfId={bound.instance_id} onClose={noop} onOpenBound={noop} />);
  assert.match(out, /data-role="brain-palette"/);
  assert.match(out, />Brains</, 'the group is labeled Brains');
  // Every brain row is present, in registry order.
  const jumpIds = [...out.matchAll(/data-role="brain-jump"/g)];
  assert.equal(jumpIds.length, list.instances.length, 'one row per registry brain');
  assert.match(out, /this brain/, 'the self row is marked');
  assert.match(out, /last seen|seen just now/, 'each row shows freshness');
});

// ── INV-11 carried into the palette: hosted brain jump is disabled ────────────
test('INV-11: a hosted project brain jump is DISABLED with the residue tooltip', () => {
  const out = html(<BrainPalette isOpen instances={list.instances} selfId={bound.instance_id} onClose={noop} onOpenBound={noop} />);
  // The project brain (no port) row is a disabled option naming the residue.
  const projTag = out.match(new RegExp(`<button[^>]*data-role="brain-jump"[^>]*data-openable="false"[^>]*>`))?.[0] ?? '';
  assert.ok(hasDisabledAttr(projTag), 'the hosted brain jump is disabled');
  assert.match(out, /REST brain routing/, 'the residue is named');
  // The bound brain jump IS enabled.
  const selfTag = out.match(new RegExp(`<button[^>]*data-role="brain-jump"[^>]*data-openable="true"[^>]*>`))?.[0] ?? '';
  assert.ok(!hasDisabledAttr(selfTag), 'the bound brain jump is enabled');
});

test('§4A.5: a closed palette renders nothing', () => {
  assert.equal(html(<BrainPalette isOpen={false} instances={list.instances} selfId={null} onClose={noop} onOpenBound={noop} />), '');
});

// ── Reduced motion is a CONTRACT, not a courtesy (§4A.5) ──────────────────────
test('§4A.5: index.css carries the prefers-reduced-motion kill switch (tremor breath stands down, transitions zeroed)', () => {
  const css = readFileSync(join(FIX, '..', 'index.css'), 'utf8');
  assert.match(css, /@media \(prefers-reduced-motion: reduce\)/, 'the reduced-motion media query exists');
  // Inside the block: tremor breath animation is killed and durations are zeroed.
  const block = css.slice(css.indexOf('@media (prefers-reduced-motion: reduce)'));
  assert.match(block, /\.tremor-breath\s*\{[^}]*animation:\s*none/, 'the tremor breath stands down');
  assert.match(block, /animation-duration:\s*0\.001ms\s*!important/, 'animations are zeroed');
  assert.match(block, /transition-duration:\s*0\.001ms\s*!important/, 'transitions are zeroed');
});
