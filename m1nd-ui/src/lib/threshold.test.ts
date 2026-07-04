/*
 * Threshold logic — INV-12/11/05 proof (HUMAN-LAYER-PRD §4A.2).
 * Pure, DOM-free. A tiny in-memory KV stands in for localStorage.
 * Fixtures: the real tools schema is captured live; north fixtures are the
 * shipped captured envelopes (INV-01).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  orientationDismissed,
  dismissOrientationForever,
  beatDismissed,
  dismissBeat,
  allBeatsDismissed,
  rememberLastBrain,
  lastBrain,
  ingestSupportsProjectRoot,
  bootstrapParams,
  mayOfferForeignIngest,
  progressCopy,
  orientationBeats,
  ORIENTATION_BEATS,
  type KV,
} from './threshold';
import type { ToolSchema } from '../types';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const load = <T>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));

function memKV(): KV {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => void m.set(k, v),
  };
}

// ── INV-12: onboarding never returns ──────────────────────────────────────────
test('INV-12: ESC-forever persists — orientation dismissed stays dismissed', () => {
  const kv = memKV();
  assert.equal(orientationDismissed(kv), false, 'fresh: not dismissed');
  dismissOrientationForever(kv);
  assert.equal(orientationDismissed(kv), true);
  // Every beat now reads dismissed too (the whole thing is gone).
  for (const b of ORIENTATION_BEATS) assert.equal(beatDismissed(kv, b), true);
  assert.equal(allBeatsDismissed(kv), true);
});

test('INV-12: each beat dismisses INDEPENDENTLY and persists', () => {
  const kv = memKV();
  dismissBeat(kv, 'map');
  assert.equal(beatDismissed(kv, 'map'), true);
  assert.equal(beatDismissed(kv, 'anchors'), false, 'other beats survive');
  assert.equal(allBeatsDismissed(kv), false);
  dismissBeat(kv, 'anchors');
  dismissBeat(kv, 'gaps');
  assert.equal(allBeatsDismissed(kv), true, 'all three individually dismissed → spent');
});

test('INV-12: last-visited brain is remembered (the tree-landing signal)', () => {
  const kv = memKV();
  assert.equal(lastBrain(kv), null);
  rememberLastBrain(kv, 'inst_abc');
  assert.equal(lastBrain(kv), 'inst_abc');
});

// ── INV-11: feature-detect project_root; clobber ban ──────────────────────────
test('INV-11: ingestSupportsProjectRoot reads the REAL tools schema', () => {
  const tools = load<{ tools: ToolSchema[] }>('tools.json').tools;
  // The live owner (post-#260) advertises project_root.
  assert.equal(ingestSupportsProjectRoot(tools), true, 'the captured schema advertises project_root');
  // Absent / empty / no-ingest all read false (honest, never assumed).
  assert.equal(ingestSupportsProjectRoot(null), false);
  assert.equal(ingestSupportsProjectRoot([]), false);
  const noProjectRoot: ToolSchema[] = [
    { name: 'ingest', description: '', inputSchema: { type: 'object', properties: { path: {} }, required: ['path'] } },
  ];
  assert.equal(ingestSupportsProjectRoot(noProjectRoot), false);
});

test('INV-11: bootstrapParams uses the one-call envelope only when advertised', () => {
  const withPr = bootstrapParams('/repo', true);
  assert.equal(withPr.path, '/repo');
  assert.equal(withPr.project_root, '/repo', 'the one-call bootstrap sets project_root=path');
  const plain = bootstrapParams('/repo', false);
  assert.equal(plain.path, '/repo');
  assert.equal('project_root' in plain, false, 'the fallback is a plain ingest (no project_root)');
});

test('INV-11 (clobber ban): a bare foreign ingest is offered ONLY on an empty owner or via the isolated bootstrap', () => {
  // Empty owner: nothing to clobber → allowed.
  assert.equal(mayOfferForeignIngest({ ownerHasGraph: false, supportsProjectRoot: false }), true);
  // Non-empty owner WITHOUT project_root: the clobber ban → forbidden.
  assert.equal(mayOfferForeignIngest({ ownerHasGraph: true, supportsProjectRoot: false }), false);
  // Non-empty owner WITH project_root: the bootstrap isolates → allowed.
  assert.equal(mayOfferForeignIngest({ ownerHasGraph: true, supportsProjectRoot: true }), true);
});

// ── INV-05: progress is words, never a fabricated percent ─────────────────────
test('INV-05: progress copy is words, never a percent or a bar', () => {
  const reading = progressCopy('reading');
  assert.match(reading, /about a minute/);
  assert.doesNotMatch(reading, /\d+\s*%/, 'no fabricated percent');
  assert.doesNotMatch(reading, /\d+\/\d+/, 'no fake fraction');
  assert.equal(progressCopy('idle'), '');
  assert.match(progressCopy('done'), /opening your map/);
});

// ── The 3-beat orientation from the real north packet (§4A.2) ─────────────────
test('§4A.2: orientation beats come from real north fields; absent data → no fabricated beat', () => {
  // Warm north fixture has a real fingerprint.
  const warm = load<{ binding: { fingerprint?: { node_count?: number; edge_count?: number } }; context?: { anchors?: Array<{ label?: string }> } }>('north_warm.json');
  const nc = warm.binding.fingerprint?.node_count ?? null;
  const ec = warm.binding.fingerprint?.edge_count ?? null;
  const anchors = (warm.context?.anchors ?? []).map((a) => a.label ?? '').filter(Boolean);

  const beats = orientationBeats({ nodeCount: nc, edgeCount: ec, anchorLabels: anchors, memoryCount: 3 });
  // The gaps beat is always the honest close.
  assert.ok(beats.some((b) => b.beat === 'gaps'));
  // When counts are known, the map beat states them (no fabricated number).
  if (nc != null && ec != null) {
    const map = beats.find((b) => b.beat === 'map');
    assert.ok(map, 'a map beat exists when counts are known');
    assert.match(map!.text, new RegExp(`${nc} files`));
  }
  // With NO map data, no map beat is fabricated.
  const noData = orientationBeats({ nodeCount: null, edgeCount: null, anchorLabels: [], memoryCount: null });
  assert.equal(noData.some((b) => b.beat === 'map'), false, 'no counts → no map beat');
  assert.equal(noData.some((b) => b.beat === 'anchors'), false, 'no anchors → no anchors beat');
  assert.equal(noData.length, 1, 'only the honest gaps close remains');
});

test('§4A.2: zero memories renders the honest "agents leave notes" line', () => {
  const beats = orientationBeats({ nodeCount: 10, edgeCount: 5, anchorLabels: [], memoryCount: 0 });
  const gaps = beats.find((b) => b.beat === 'gaps');
  assert.match(gaps!.text, /agents leave notes here as they work/);
});
