/*
 * INV-13 (precision is mechanical) + icon-lint bite — HUMAN-LAYER-PRD §4A.7.
 *
 *  - The registry draws every icon at stroke 1.5 with currentColor and an
 *    accessible label (an icon never appears without a text label — aria minimum).
 *  - StatCell/StatValue render right-aligned tabular mono.
 *  - The icon-lint BITES: a fixture importing Sparkles + strokeWidth≠1.5 makes the
 *    linter exit non-zero and name all three rules. If the lint stops biting, the
 *    one-icon-per-concept guard is dead.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, copyFileSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { Icon, ICON, ICON_STROKE, type IconName } from './registry';
import { StatCell, StatValue } from '../../components/soft/StatCell';

const HERE = dirname(fileURLToPath(import.meta.url));
const UI_ROOT = join(HERE, '..', '..', '..');

test('INV-13: every registry icon renders at stroke 1.5, currentColor, with a label', () => {
  assert.equal(ICON_STROKE, 1.5, 'the universal stroke is 1.5');
  for (const name of Object.keys(ICON) as IconName[]) {
    const out = renderToStaticMarkup(React.createElement(Icon, { name, label: name }));
    assert.match(out, /stroke-width="1\.5"/, `${name} draws stroke 1.5`);
    assert.match(out, /stroke="currentColor"/, `${name} inherits currentColor (violet quarantine holds)`);
    assert.match(out, new RegExp(`aria-label="${name}"`), `${name} carries an aria-label`);
  }
});

test('INV-13: icon sizes are the two sanctioned values (14 inline, 16 header)', () => {
  const inline = renderToStaticMarkup(React.createElement(Icon, { name: 'graph', label: 'nodes' }));
  assert.match(inline, /width="14"/, 'default inline size is 14');
  const header = renderToStaticMarkup(React.createElement(Icon, { name: 'graph', label: 'nodes', size: 16 }));
  assert.match(header, /width="16"/, 'header size is 16');
});

test('INV-13: a decorative icon is aria-hidden (a visible sibling label names it)', () => {
  const out = renderToStaticMarkup(React.createElement(Icon, { name: 'graph', decorative: true }));
  assert.match(out, /aria-hidden="true"/);
  assert.doesNotMatch(out, /aria-label=/, 'no aria-label when decorative (sibling label carries it)');
});

test('INV-13: StatValue renders right-aligned tabular mono; StatCell pairs icon + visible label + value', () => {
  const val = renderToStaticMarkup(React.createElement(StatValue, null, '2,089'));
  assert.match(val, /font-mono/, 'value is monospaced (tabular by construction)');
  assert.match(val, /tabular-nums/, 'belt: tabular-nums');
  assert.match(val, /text-right/, 'value is right-aligned');

  const cell = renderToStaticMarkup(
    React.createElement(StatCell, { icon: 'graph', label: 'nodes', value: '2,089' }),
  );
  assert.match(cell, /data-icon="graph"/, 'the concept icon is present');
  assert.match(cell, /nodes/, 'the visible label is present (INV-13 label-on-first-use)');
  assert.match(cell, /data-role="stat-value"/, 'the value uses the tabular-mono primitive');
});

test('the CONCEPT→ICON registry has no duplicate glyph within a concept family it must distinguish', () => {
  // Verdict glyphs must all be distinct (act/reverify/abstain read differently).
  const verdicts = [ICON.verdictAct, ICON.verdictReverify, ICON.verdictAbstain];
  assert.equal(new Set(verdicts).size, 3, 'act/reverify/abstain are three distinct glyphs');
  // Lens glyphs distinct.
  const lenses = [ICON.groupDir, ICON.groupKind, ICON.layer];
  assert.equal(new Set(lenses).size, 3, 'directory/kind/layer are three distinct lens glyphs');
  // memory === kindMemory by design (same concept, same icon — the law holds across tables).
  assert.equal(ICON.memory, ICON.kindMemory, 'memory and its KIND glyph are the same icon');
});

test('icon-lint BITES: a Sparkles + strokeWidth≠1.5 fixture fails the linter (proves the guard)', () => {
  const lint = join(UI_ROOT, 'scripts', 'icon-lint.mjs');
  const fixture = join(HERE, '__lint_fixtures__', 'bad-sparkles.tsx.fixture');
  const probe = join(HERE, '__bite_probe__.tsx');
  copyFileSync(fixture, probe);
  let threw = false;
  let output = '';
  try {
    execFileSync('node', [lint], { cwd: UI_ROOT, encoding: 'utf8', stdio: 'pipe' });
  } catch (e: unknown) {
    threw = true;
    const err = e as { status?: number; stderr?: string; stdout?: string };
    assert.notEqual(err.status, 0, 'the linter exits non-zero on the red-case');
    output = (err.stderr ?? '') + (err.stdout ?? '');
  } finally {
    rmSync(probe, { force: true });
  }
  assert.ok(threw, 'the linter must FAIL (throw) on the Sparkles fixture');
  assert.match(output, /Sparkles/, 'names the banned Sparkles import');
  assert.match(output, /lucide-react.*outside the icon registry/, 'names the illegal direct import');
  assert.match(output, /strokeWidth 2 ≠ 1\.5/, 'names the stroke violation');
});

test('the registry vendored the lucide ISC license text alongside (as the fonts do)', () => {
  const license = readFileSync(join(HERE, 'LICENSE-lucide.txt'), 'utf8');
  assert.match(license, /ISC License/, 'ISC license text is vendored');
  assert.match(license, /Lucide Icons/, 'names Lucide');
});
