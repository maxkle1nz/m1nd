/*
 * Universe semantics tests (HUMAN-VIEW-V2 F30) — the pure functions behind the L0
 * panorama, at the honesty boundary. Neutral fixtures only (no-leak law): no real
 * project/agent name of the owner ever appears.
 *
 * Proven: size ∝ node_count on a LOG curve (small never vanishes, big caps, absent
 * = floor), light ∝ freshness (stale dims to a FLOOR, never dark; absent = a
 * neutral mid the caller labels "age unknown"), the DETERMINISTIC layout, the
 * serif headline FACTS (counts, plurals, honest zero states — never a vital), and
 * the Landing queue (per-world stamps/ratifies + owner alerts, order, scope).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  worldRadius,
  worldLight,
  worldAgeLabel,
  worldPending,
  layoutWorlds,
  universeHeadline,
  buildLandingItems,
  WORLD_R_MIN,
  WORLD_R_MAX,
  WORLD_LIGHT_FLOOR,
  type UniverseWorld,
  type UniverseResponse,
} from './universe';

const NOW = 1_800_000_000_000;

function world(over: Partial<UniverseWorld> = {}): UniverseWorld {
  return {
    key: '/w/one',
    root: '/w/one',
    name: 'world-one',
    node_count: 500,
    edge_count: 900,
    updated_ms: NOW,
    awake: false,
    presences: [],
    pending: { stamps: 0, ratifies: 0 },
    letters: { merge_wait: 0, total: 0 },
    ...over,
  };
}

// ── SCALE ──────────────────────────────────────────────────────────────────

test('worldRadius: absent/zero count → the floor (size unknown, never a fabricated disc)', () => {
  assert.equal(worldRadius(undefined), WORLD_R_MIN);
  assert.equal(worldRadius(0), WORLD_R_MIN);
});

test('worldRadius: log-scaled and monotonic, capped at the max', () => {
  const small = worldRadius(100);
  const mid = worldRadius(5_000);
  const big = worldRadius(1_000_000);
  assert.ok(small > WORLD_R_MIN, 'a small repo is still visible above the floor');
  assert.ok(mid > small, 'monotonic: more nodes → larger');
  assert.ok(big <= WORLD_R_MAX + 1e-9, 'a huge repo pins at the max, never overflows');
  // Log curve: the jump 100→5000 is bigger than 5000→1e6 (diminishing returns).
  assert.ok(mid - small > big - mid, 'the growth is logarithmic, not linear');
});

// ── LIGHT (freshness, shown honestly) ───────────────────────────────────────

test('worldLight: fresh → ~full, stale → the floor (never dark), absent → a neutral mid', () => {
  assert.ok(worldLight(NOW, NOW) > 0.98, 'just-updated is near full light');
  const old = worldLight(NOW - 60 * 24 * 60 * 60 * 1000, NOW); // 60 days
  assert.equal(old, WORLD_LIGHT_FLOOR, 'far past the horizon sits exactly at the floor');
  assert.ok(old >= WORLD_LIGHT_FLOOR, 'a stale world dims but never goes dark');
  assert.equal(worldLight(undefined, NOW), 0.55, 'absent freshness → a neutral mid');
});

test('worldAgeLabel: absent → "age unknown"; fresh → "now" (staleness is DATA)', () => {
  assert.equal(worldAgeLabel(undefined, NOW), 'age unknown');
  assert.equal(worldAgeLabel(NOW, NOW), 'now');
  assert.equal(worldAgeLabel(NOW - 3 * 60 * 60 * 1000, NOW), '3h');
});

test('worldPending: stamps + ratifies (alerts are owner-scope, never folded in)', () => {
  assert.equal(worldPending(world({ pending: { stamps: 2, ratifies: 3 } })), 5);
  assert.equal(worldPending(world({ pending: { stamps: 0, ratifies: 0 } })), 0);
});

// ── LAYOUT (deterministic) ──────────────────────────────────────────────────

test('layoutWorlds: deterministic, one placement per world, index 0 at the centre', () => {
  const worlds = [world({ key: 'a' }), world({ key: 'b' }), world({ key: 'c' })];
  const a = layoutWorlds(worlds, { width: 1000, height: 640, nowMs: NOW });
  const b = layoutWorlds(worlds, { width: 1000, height: 640, nowMs: NOW });
  assert.equal(a.length, 3, 'one placement per world');
  assert.deepEqual(
    a.map((p) => [p.cx, p.cy]),
    b.map((p) => [p.cx, p.cy]),
    'identical inputs → identical layout (no physics sim, no randomness)',
  );
  assert.equal(a[0].cx, 500, 'index 0 sits at the observatory origin (centre x)');
  assert.equal(a[0].cy, 320, 'index 0 sits at the observatory origin (centre y)');
  // Every placement stays inside the frame with room for its disc.
  for (const p of a) {
    assert.ok(p.cx - p.r >= 0 && p.cx + p.r <= 1000, 'x within the frame');
    assert.ok(p.cy - p.r >= 0 && p.cy + p.r <= 640, 'y within the frame');
  }
});

test('layoutWorlds: a lone world sits dead-centre', () => {
  const [only] = layoutWorlds([world()], { width: 800, height: 600, nowMs: NOW });
  assert.equal(only.cx, 400);
  assert.equal(only.cy, 300);
});

// ── HEADLINE (universe FACTS — counts, never a vital) ────────────────────────

test('universeHeadline: plurals + honest zero states, joined by the calm separator', () => {
  assert.equal(
    universeHeadline({ worlds: 3, awake: 1, pending: 8 }),
    '3 worlds · 1 awake · 8 await your hand',
  );
  assert.equal(
    universeHeadline({ worlds: 1, awake: 0, pending: 0 }),
    '1 world · none awake · nothing awaits your hand',
    'singular world, and honest none/nothing rather than a bare 0',
  );
});

// ── THE LANDING (the unified queue) ──────────────────────────────────────────

function universe(over: Partial<UniverseResponse> = {}): UniverseResponse {
  return {
    schema: 'm1nd-universe-v0',
    worlds: [],
    owner: { alerts_pending: 0 },
    totals: { worlds: 0, awake: 0, pending: 0 },
    ...over,
  };
}

test('buildLandingItems: one bucket per world gesture-type (only when > 0), owner alerts last', () => {
  const u = universe({
    worlds: [
      world({ key: 'a', root: '/w/a', name: 'alpha', pending: { stamps: 2, ratifies: 0 } }),
      world({ key: 'b', root: '/w/b', name: 'beta', pending: { stamps: 1, ratifies: 3 } }),
      world({ key: 'c', root: '/w/c', name: 'gamma', pending: { stamps: 0, ratifies: 0 } }),
    ],
    owner: { alerts_pending: 1 },
  });
  const items = buildLandingItems(u);
  // alpha:stamp, beta:stamp, beta:ratify, owner:alerts — gamma (all zero) omitted.
  // Item ids are keyed by the world KEY (canonical root), a stable list key.
  assert.deepEqual(
    items.map((i) => i.id),
    ['a:stamp', 'b:stamp', 'b:ratify', 'owner:alerts'],
    'stamps before ratifies within a world; a zero world contributes nothing; alerts last',
  );
  const alpha = items[0];
  assert.equal(alpha.kind, 'stamp');
  assert.equal(alpha.chip, 'alpha');
  assert.equal(alpha.scope, 'world');
  assert.equal(alpha.worldRoot, '/w/a');
  assert.match(alpha.line, /2 receipts await your stamp/);

  const alert = items[3];
  assert.equal(alert.kind, 'alert');
  assert.equal(alert.chip, 'owner', 'a daemon alert wears the owner chip, never a world');
  assert.equal(alert.scope, 'owner');
  assert.equal(alert.worldRoot, undefined, 'owner alerts have no world to open');
  assert.match(alert.line, /1 daemon alert to acknowledge/);
});

test('buildLandingItems: an empty universe yields an empty queue (honest, not fabricated)', () => {
  assert.deepEqual(buildLandingItems(universe()), []);
});

test('buildLandingItems: singular vs plural is honest in the human line', () => {
  const u = universe({
    worlds: [world({ key: 'a', root: '/w/a', name: 'alpha', pending: { stamps: 1, ratifies: 1 } })],
  });
  const items = buildLandingItems(u);
  assert.match(items[0].line, /1 receipt await your stamp/, 'one receipt, singular noun');
  assert.match(items[1].line, /1 block await ratification/, 'one block, singular noun');
});
