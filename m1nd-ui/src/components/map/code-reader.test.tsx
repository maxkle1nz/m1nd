/*
 * CodeReader — the advanced reader, rendered against the REAL captured wire shape
 * (reader_snapshot.json) via renderToStaticMarkup (the F1 posture: no clicks, no
 * network — an injected snapshot drives the outline). The teeth: the outline is the
 * graph's symbols in line order; click-to-def degrades honestly (jump/candidates/
 * abstain) in the DOM; the per-symbol freshness dot is the block's state; and the
 * viewer-idle / language-pill honesty holds. (The rendered HIGHLIGHT + interactive
 * scroll/fold/navigate are proven in the browser e2e — Shiki paints client-side.)
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import CodeReader from './CodeReader';
import type { GraphSnapshot } from '../../lib/snapshot';
import type { BlockRollup } from '../../lib/buildMap';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const snap = JSON.parse(readFileSync(join(FIX, 'reader_snapshot.json'), 'utf8')) as GraphSnapshot;

const rollup: BlockRollup = {
  blockId: 'sb_repo_alpha_core',
  state: 'needs-evidence',
  requiredTypes: [],
  earnedTypes: [],
  receiptsEarned: 0,
  receiptsRequired: 0,
  wired: false,
  candidate: true,
  brokenReasons: [],
  boundaryStale: false,
};

const render = (path: string | null) =>
  renderToStaticMarkup(
    React.createElement(CodeReader, { path, brainRoot: null, rollup, snapshotOverride: snap }),
  );
const count = (s: string, re: RegExp) => (s.match(re) ?? []).length;

test('no file selected → the honest idle placeholder (no network in a static render)', () => {
  assert.match(render(null), /data-role="viewer-idle"/);
});

test('the OUTLINE renders the file’s graph symbols in line order, with a language pill', () => {
  const out = render('repo-alpha/src/store.rs');
  assert.match(out, /data-role="symbol-outline"/);
  assert.match(out, /data-role="reader-langpill"[^>]*>Rust</);
  // Store / open / save / close, each a selectable outline entry with its line.
  assert.equal(count(out, /data-role="outline-entry"/g), 4);
  assert.match(out, /data-symbol-id="file::repo-alpha\/src\/store\.rs::function::open"[^>]*data-symbol-line="14"/);
  assert.match(out, /data-symbol-id="file::repo-alpha\/src\/store\.rs::struct::Store"[^>]*data-symbol-line="5"/);
});

test('click-to-def degrades HONESTLY in the DOM: jump / candidates / abstain', () => {
  const out = render('repo-alpha/src/store.rs');
  // open → a single grounded jump into graph.rs.
  assert.match(out, /data-role="def-jump"[^>]*data-target-path="repo-alpha\/src\/graph\.rs"[^>]*data-target-line="18"/);
  // save → ambiguous, a candidate toggle (never a guessed jump).
  assert.match(out, /data-role="def-candidates-toggle"/);
  // close → external/dangling, an explicit abstain marker.
  assert.match(out, /data-role="def-abstain"/);
});

test('TS degradation in the DOM: render jumps by def; mount (call-only) abstains', () => {
  const out = render('repo-alpha/ui/panel.ts');
  assert.match(out, /data-role="reader-langpill"[^>]*>TypeScript</);
  // render → def edge → a jump into helpers.ts.
  assert.match(out, /data-role="def-jump"[^>]*data-target-path="repo-alpha\/ui\/helpers\.ts"/);
  // mount → call-only → abstain (calls-not-tracked); never a fabricated jump.
  assert.match(out, /data-role="def-abstain"/);
});

test('the per-symbol FRESHNESS dot carries the block’s state (an existing per-block fact)', () => {
  const out = render('repo-alpha/src/store.rs');
  // needs-evidence → the semantic state + the house ochre dot; never violet.
  assert.match(out, /data-role="freshness-dot"[^>]*data-state="needs-evidence"/);
  assert.match(out, /data-role="freshness-dot"[^>]*bg-verdict-reverify/);
  assert.doesNotMatch(out, /data-role="freshness-dot"[^>]*bg-iris/);
});

test('a file the graph carries no symbols for is an honest empty outline', () => {
  const out = render('repo-alpha/docs/guide.md');
  assert.match(out, /data-role="outline-empty"/);
  assert.match(out, /No symbols from the graph for this file/);
  // markdown IS a known grammar, so no "plain text" suffix here.
  assert.doesNotMatch(out, /shown as plain text/);
});

test('an unknown extension says plain text honestly in the empty outline', () => {
  const out = render('repo-alpha/data/blob.parquet');
  assert.match(out, /data-role="reader-langpill"[^>]*>plain text</);
  assert.match(out, /shown as plain text/);
});
