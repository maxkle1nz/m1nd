/*
 * Build Map — the degraded EMPTY screen (HUMAN-VIEW-V2-SCREENS §1.3; PRD F6).
 * When the brain has no skeleton (`present:false`), the map renders the honest
 * backend copy and an Import CTA that is DISABLED with a note pointing at the
 * write verb — F1 is read-only, so the map never fakes a mutation (INV-11).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import BuildMap from './BuildMap';
import type { SystemBlocksSnapshot } from '../../lib/buildMap';

const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const visibleText = (el: React.ReactElement) => html(el).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ');

// The exact honest shape the handler returns when no store exists.
const empty: SystemBlocksSnapshot = {
  present: false,
  honest: 'no skeleton yet — import a seed or run a scan',
};

test('present:false → the honest empty screen renders (no fabricated blocks)', () => {
  const out = html(<BuildMap snapshot={empty} rollup={null} />);
  assert.match(out, /data-role="build-map-empty"/);
  assert.doesNotMatch(out, /data-role="block-card"/, 'no cards when there is no skeleton');
});

test('the empty screen carries the backend\'s honest copy', () => {
  const text = visibleText(<BuildMap snapshot={empty} rollup={null} />);
  assert.match(text, /No skeleton yet for this repo/);
  assert.match(text, /import a seed or run a scan/, 'the backend honest string is shown verbatim');
});

test('the Import CTA is DISABLED and points at the write verb (F1 is read-only)', () => {
  const out = html(<BuildMap snapshot={empty} rollup={null} />);
  assert.match(out, /data-role="import-seed"[^>]*disabled/, 'import is disabled — never a fake mutation');
  const text = visibleText(<BuildMap snapshot={empty} rollup={null} />);
  assert.match(text, /system_blocks_seed_import verb/, 'the CTA names the verb to run from the CLI');
});

test('COPY LAW holds on the empty screen too', () => {
  const text = html(<BuildMap snapshot={empty} rollup={null} />).replace(/<[^>]+>/g, ' ');
  assert.doesNotMatch(text, /\bproven\b/i);
  assert.doesNotMatch(text, /\bdone\b/i);
  assert.doesNotMatch(text, /\bcorrect\b/i);
});
