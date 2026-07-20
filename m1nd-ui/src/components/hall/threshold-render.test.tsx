/*
 * Threshold + orientation render tests (HUMAN-LAYER-PRD §4A.2, INV-12/05).
 * Rendered with react-dom/server. A tiny in-memory KV stands in for localStorage.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import ThresholdCard from './ThresholdCard';
import OrientationBeats from './OrientationBeats';
import { dismissOrientationForever, dismissBeat, type KV } from '../../lib/threshold';

const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) => s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&#x2019;/g, '’');
const visibleText = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' '));
const noop = () => {};

function memKV(): KV {
  const m = new Map<string, string>();
  return { getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v) };
}

// ── The Threshold: one calm sentence, one honest closed state ──────────────
test('§4A.2: the Threshold exposes no unreachable bootstrap action', () => {
  const out = html(<ThresholdCard />);
  const text = visibleText(<ThresholdCard />);
  assert.match(text, /living map of your code/, 'the one calm sentence');
  assert.match(out, /data-role="bootstrap-unavailable"/, 'the closed state is explicit');
  assert.match(text, /brain_bootstrap_consumer_not_installed/);
  assert.doesNotMatch(out, /data-role="read-first-repo"|data-role="threshold-path"/);
  assert.doesNotMatch(text, /project_root|one-call/i);
  // No wizard vocabulary.
  assert.doesNotMatch(text.toLowerCase(), /step 1|step 2|next →|checklist|tour|get started/);
});

// ── INV-05: the Threshold never shows a fabricated percent ────────────────────
test('INV-05: the idle Threshold shows no progress bar and no percent', () => {
  const out = html(<ThresholdCard />);
  assert.doesNotMatch(out, /\d+\s*%/, 'no percent');
  assert.doesNotMatch(out, /role="progressbar"/, 'no determinate bar');
  // The progress copy only appears while reading (not at idle).
  assert.doesNotMatch(out, /data-role="threshold-progress"/);
});

// ── INV-12: orientation renders beats, and never returns once dismissed ───────
test('§4A.2: orientation renders the map + anchors beats + the honest gaps close', () => {
  const out = html(
    <OrientationBeats
      kv={memKV()}
      nodeCount={6569}
      edgeCount={20955}
      anchorLabels={['Graph', 'NodeId', 'SessionState']}
      memoryCount={2}
      gaps={['No durable memory for foo yet']}
      onSpent={noop}
    />,
  );
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.match(text, /6569 files/, 'the map beat states real counts');
  assert.match(text, /carry the most weight/, 'the anchors beat');
  assert.match(out, /data-beat="gaps"/, 'the gaps beat is present');
  assert.match(out, /data-abstain="true"/, 'the gaps beat renders the violet gap card');
  // Each beat has exactly one dismiss.
  const dismisses = (out.match(/data-role="dismiss-beat"/g) ?? []).length;
  assert.equal(dismisses, 3, 'three beats, three dismisses');
});

test('INV-12: once dismissed forever, orientation renders NOTHING', () => {
  const kv = memKV();
  dismissOrientationForever(kv);
  const out = html(
    <OrientationBeats kv={kv} nodeCount={10} edgeCount={5} anchorLabels={['A']} memoryCount={1} gaps={['g']} onSpent={noop} />,
  );
  assert.equal(out, '', 'a dismissed-forever orientation is empty');
});

test('INV-12: an individually-dismissed beat does not render; others survive', () => {
  const kv = memKV();
  dismissBeat(kv, 'map');
  const out = html(
    <OrientationBeats kv={kv} nodeCount={10} edgeCount={5} anchorLabels={['A']} memoryCount={1} gaps={['g']} onSpent={noop} />,
  );
  assert.doesNotMatch(out, /data-beat="map"/, 'the dismissed map beat is gone');
  assert.match(out, /data-beat="anchors"/, 'the anchors beat survives');
  assert.match(out, /data-beat="gaps"/, 'the gaps beat survives');
});

// ── The gaps beat degrades honestly to "agents leave notes" at zero memories ──
test('§4A.2: zero memories → the gaps beat says "agents leave notes here as they work"', () => {
  const out = html(
    <OrientationBeats kv={memKV()} nodeCount={10} edgeCount={5} anchorLabels={[]} memoryCount={0} gaps={[]} onSpent={noop} />,
  );
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.match(text, /agents leave notes here as they work/);
});
