/*
 * Threshold logic — INV-12/11/05 proof (HUMAN-LAYER-PRD §4A.2).
 * Pure, DOM-free. A tiny in-memory KV stands in for localStorage.
 * Fixtures: tools-current.json is the evolving live-schema capture; tools.json
 * remains the byte-frozen G0 baseline. North fixtures are the shipped captured
 * envelopes (INV-01).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
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

// ── INV-11: closed bootstrap surface + evolving registry fixture ─────────────
test('INV-11: current tools keep bound ingest but expose no bootstrap arguments', () => {
  const tools = load<{ tools: ToolSchema[] }>('tools-current.json').tools;
  const ingest = tools.find((tool) => tool.name === 'ingest');
  assert.ok(ingest, 'the compatibility ingest name remains in the current registry');
  assert.match(ingest.description, /POLICY-DISABLED/);
  const properties = ingest.inputSchema?.properties ?? {};
  assert.ok(Object.prototype.hasOwnProperty.call(properties, 'path'));
  assert.equal(Object.prototype.hasOwnProperty.call(properties, 'project_root'), false);
  assert.equal(Object.prototype.hasOwnProperty.call(properties, 'allow_overlap'), false);
  assert.doesNotMatch(JSON.stringify(tools), /brain\.bootstrap|ONE-CALL BOOTSTRAP/);
});

test('INV-11: the evolving fixture is the complete unique registry and preserves the frozen baseline', () => {
  const current = load<{ tools: ToolSchema[] }>('tools-current.json').tools;
  const frozen = load<{ tools: ToolSchema[] }>('tools.json').tools;
  const currentNames = current.map((tool) => tool.name);
  const frozenNames = frozen.map((tool) => tool.name);

  assert.equal(current.length, 138, 'all_tool_schemas registry count');
  assert.equal(new Set(currentNames).size, current.length, 'tool names are unique');
  for (const name of frozenNames) assert.ok(currentNames.includes(name), `current registry retains ${name}`);

  const preview = current.find((tool) => tool.name === 'graph_ingest_preview');
  assert.ok(preview, 'the governed read-only preview is advertised');
  assert.equal(
    preview.inputSchema.properties.schema &&
      (preview.inputSchema.properties.schema as { const?: string }).const,
    'm1nd-graph-ingest-preview-request-v1',
  );
  assert.equal((preview.inputSchema as { additionalProperties?: boolean }).additionalProperties, false);

  const frozenBytes = readFileSync(join(FIX, 'tools.json'));
  assert.equal(
    createHash('sha256').update(frozenBytes).digest('hex'),
    'd5cf5872edfdaf3745e6e5e8216e7fe6bb42f01f05b80d5939c4a538c3c215b6',
    'the G0 tools fixture remains byte-frozen',
  );
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
