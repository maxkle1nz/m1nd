/*
 * SeekPanel render — the meaning-search panel renders sufficiency + VerdictChip
 * from a REAL captured SeekOutput (HUMAN-LAYER-PRD §4A.10). INV-16: a foreign hit
 * is dropped, never rendered. Rendered with react-dom/server (no new deps).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import SeekPanel from './SeekPanel';
import type { SeekOutput } from '../../api/toolTypes';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&quot;/g, '"');
const visible = (el: React.ReactElement) => decode(renderToStaticMarkup(el).replace(/<[^>]+>/g, ' '));

const seek = load<SeekOutput>('seek_meaning.json');
// The seek envelope was captured from the m1nd brain (the full graph). The
// viewed brain's "known paths" are therefore that brain's own file_paths — model
// them from the seek hits themselves (they belong to the brain that answered).
// A FOREIGN path (injected in the INV-16 test) is never in this set → dropped.
const knownPaths = new Set(
  seek.results.map((r) => r.file_path).filter((p): p is string => !!p),
);
const noop = () => {};

test('§4A.10: the panel renders sufficiency.state + why VERBATIM from the real envelope', () => {
  const out = renderToStaticMarkup(
    <SeekPanel query={seek.query} result={seek} loading={false} error={null} knownPaths={knownPaths} onOpenHit={noop} onClose={noop} />,
  );
  assert.match(out, /data-role="sufficiency"/);
  assert.match(out, /data-role="sufficiency-state"/);
  const text = visible(
    <SeekPanel query={seek.query} result={seek} loading={false} error={null} knownPaths={knownPaths} onOpenHit={noop} onClose={noop} />,
  );
  // The state word and the WHY line appear verbatim (the engine's own strings).
  assert.match(text, new RegExp(seek.sufficiency.state), 'sufficiency state word rendered');
  // The verbatim why line (compare on a stable substring to survive whitespace).
  const whyFragment = seek.sufficiency.why.slice(0, 40);
  assert.ok(text.includes(whyFragment), 'the sufficiency why line is rendered verbatim');
});

test('§4A.10: the panel renders the trust_envelope verdict as a VerdictChip', () => {
  const out = renderToStaticMarkup(
    <SeekPanel query={seek.query} result={seek} loading={false} error={null} knownPaths={knownPaths} onOpenHit={noop} onClose={noop} />,
  );
  assert.match(out, new RegExp(`data-verdict="${seek.trust_envelope.verdict}"`), 'the real verdict drives the chip');
  // The uncalibrated cap notice (calibrated:false → capped at reverify).
  if (seek.trust_envelope.calibrated === false) {
    const text = visible(
      <SeekPanel query={seek.query} result={seek} loading={false} error={null} knownPaths={knownPaths} onOpenHit={noop} onClose={noop} />,
    );
    assert.match(text, /not measured on this repo yet/, 'the uncalibrated cap is stated');
  }
});

test('§4A.10: NO sparkle icon anywhere in the panel (the honesty markers are the sufficiency + verdict)', () => {
  const out = renderToStaticMarkup(
    <SeekPanel query={seek.query} result={seek} loading={false} error={null} knownPaths={knownPaths} onOpenHit={noop} onClose={noop} />,
  );
  assert.doesNotMatch(out, /sparkle/i, 'no AI glitter — the seek mode is a labeled text toggle');
  // The score is the engine's, verbatim (a mono value), not invented stars.
  assert.doesNotMatch(out, /★|⭐/, 'no invented star glyphs');
});

test('§4A.10: each hit shows file:line + intent_summary + the engine score verbatim', () => {
  const withFile = seek.results.find((r) => r.file_path);
  assert.ok(withFile, 'a hit with a file_path exists in the real envelope');
  const text = visible(
    <SeekPanel query={seek.query} result={seek} loading={false} error={null} knownPaths={knownPaths} onOpenHit={noop} onClose={noop} />,
  );
  assert.ok(text.includes(withFile!.file_path!), 'the file path is shown');
  if (withFile!.score != null) {
    assert.ok(text.includes(withFile!.score.toFixed(3)), 'the engine score is rendered verbatim (3dp)');
  }
});

test('INV-16: a foreign-brain hit is DROPPED and the drop is surfaced, never rendered into the tree', () => {
  const foreign: SeekOutput = {
    ...seek,
    results: [
      ...seek.results,
      { node_id: 'foreign::x', label: 'ForeignSymbol', file_path: 'other-brain/src/foreign.rs', line_start: 1, score: 0.99 },
    ],
  };
  const out = renderToStaticMarkup(
    <SeekPanel query={seek.query} result={foreign} loading={false} error={null} knownPaths={knownPaths} onOpenHit={noop} onClose={noop} />,
  );
  // The foreign row is NOT rendered.
  assert.doesNotMatch(out, /ForeignSymbol/, 'the foreign-brain hit is not rendered');
  assert.doesNotMatch(out, /other-brain\/src\/foreign\.rs/, 'the foreign path is not shown');
  // The drop is surfaced honestly.
  assert.match(out, /data-role="seek-dropped"/, 'the drop is stated — the world is never silently smaller');
});

test('§4A.10: truncation is honest — "showing N of M that cleared relevance"', () => {
  const text = visible(
    <SeekPanel query={seek.query} result={seek} loading={false} error={null} knownPaths={knownPaths} onOpenHit={noop} onClose={noop} />,
  );
  // The real envelope has relevance_clearing_total >> shown → the truncation line.
  if ((seek.relevance_clearing_total ?? 0) > seek.results.length) {
    assert.match(text, /showing \d+ of \d+ that cleared relevance/);
  }
});
