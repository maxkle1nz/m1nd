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

// ── filterBrains: fuzzy PROJECT name, recency order preserved (R4) ─────────────
test('§4A.5: filterBrains preserves registry recency order and fuzzy-matches PROJECT names', () => {
  // Empty query → all rows, unchanged order (recents-first is the registry's).
  const all = filterBrains(rows, '');
  assert.deepEqual(all.map((r) => r.entry.instance_id), rows.map((r) => r.entry.instance_id));
  // The fuzzy match is on the PROJECT name (display_name), not the plumbing path.
  assert.equal(filterBrains(rows, 'zzzznomatch').length, 0);
  // "m1nd" finds the bound brain (its repo), "cerry" the project brain.
  assert.equal(filterBrains(rows, 'm1nd').length, 1, '"m1nd" matches the bound brain by its project name');
  assert.equal(filterBrains(rows, 'm1nd')[0].entry.instance_id, bound.instance_id);
  assert.equal(filterBrains(rows, 'cerry').length, 1, '"cerry" matches the project brain');
  assert.equal(filterBrains(rows, 'cerry')[0].entry.instance_id, project.instance_id);
  // The plumbing token "claude" (the runtime dir) matches NOTHING — the Hall
  // never searches by plumbing (the Brain Chip law, carried into the palette).
  assert.equal(filterBrains(rows, 'claude').length, 0, '"claude" (plumbing) matches no project name');
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

// ── §4A.9 in the palette: hosted brain jump follows the selector stamp ────────
test('§4A.9: a hosted project brain jump is DISABLED without the stamp — retired residue gone', () => {
  const out = html(<BrainPalette isOpen instances={list.instances} selfId={bound.instance_id} onClose={noop} onOpenBound={noop} restSelector={false} />);
  // Without the stamp the hosted (no-port) row is a disabled option, but the
  // retired "REST brain routing" residue is deleted.
  const projTag = out.match(new RegExp(`<button[^>]*data-role="brain-jump"[^>]*data-openable="false"[^>]*>`))?.[0] ?? '';
  assert.ok(hasDisabledAttr(projTag), 'the hosted brain jump is disabled without the stamp');
  assert.doesNotMatch(out, /REST brain routing/, 'the retired residue text is gone');
  // The bound brain jump IS enabled.
  const selfTag = out.match(new RegExp(`<button[^>]*data-role="brain-jump"[^>]*data-openable="true"[^>]*>`))?.[0] ?? '';
  assert.ok(!hasDisabledAttr(selfTag), 'the bound brain jump is enabled');
});

test('§4A.9: a hosted project brain jump is ENABLED with the stamp + a tree-open handler', () => {
  const out = html(
    <BrainPalette isOpen instances={list.instances} selfId={bound.instance_id} onClose={noop} onOpenBound={noop} onOpenBrain={noop} restSelector />,
  );
  // With the stamp the hosted row becomes openable (data-openable="true", enabled).
  const projTag = out.match(new RegExp(`<button[^>]*data-role="brain-jump"[^>]*data-openable="true"[^>]*title="Open this brain in the tree"[^>]*>`))?.[0] ?? '';
  assert.ok(projTag.length > 0, 'the hosted brain jump names the real tree-open action');
  assert.ok(!hasDisabledAttr(projTag), 'the hosted brain jump is enabled with the stamp');
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
