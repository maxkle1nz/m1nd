/*
 * The hash router's pure core — parse/serialize every route, the deep-link
 * precedence over the landing rule, and the brain-key fallback (evicted /
 * collision). DOM-free (node:test), the repo's `use-universe.test.ts` pattern.
 *
 * NO-LEAK: every fixture root is neutral (`/work/repo-alpha`, `/srv/project-b`) —
 * never a personal path. The whole point of the basename key is that a public spec
 * can carry `#/world/repo-alpha/map` without leaking where the repo lives on disk.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  basename,
  parseRoute,
  serializeRoute,
  resolveBrainKey,
  routeToIntent,
  resolveDeepLink,
  type ParsedRoute,
} from './router';
import { BOUND_VIEW, type ViewedBrain } from './viewedBrain';
import type { UniverseWorld } from './universe';
import type { InstanceRegistryEntry } from '../types';

// ── fixtures (neutral roots only) ────────────────────────────────────────────

function world(root: string, name: string, nodeCount?: number): UniverseWorld {
  return {
    key: root,
    root,
    name,
    node_count: nodeCount,
    awake: false,
    presences: [],
    pending: { stamps: 0, ratifies: 0 },
    letters: { merge_wait: 0, total: 0 },
  };
}

function brainEntry(root: string, name: string): InstanceRegistryEntry {
  return {
    instance_id: 'inst_test',
    workspace_root: root,
    runtime_root: '/work/.m1nd',
    graph_source: 'ingest',
    plasticity_state: 'stable',
    pid: 1,
    started_at_ms: 0,
    last_heartbeat_ms: 0,
    mode: 'serve',
    status: 'running',
    stale: false,
    conflicts: [],
    display_name: name,
    project_root: root,
    node_count: 42,
  };
}

// ── basename (the URL brain key) ──────────────────────────────────────────────

test('basename is separator-agnostic and trailing-slash tolerant', () => {
  assert.equal(basename('/work/repo-alpha'), 'repo-alpha');
  assert.equal(basename('/work/repo-alpha/'), 'repo-alpha');
  assert.equal(basename('C:\\work\\repo-beta'), 'repo-beta');
  assert.equal(basename('repo-solo'), 'repo-solo');
  assert.equal(basename('/work/repo-alpha///'), 'repo-alpha');
});

// ── parseRoute ────────────────────────────────────────────────────────────────

test('parseRoute reads every bound route', () => {
  assert.deepEqual(parseRoute('#/universe'), { surface: 'universe', brainKey: null, block: null });
  assert.deepEqual(parseRoute('#/hall'), { surface: 'hall', brainKey: null, block: null });
  assert.deepEqual(parseRoute('#/tree'), { surface: 'tree', brainKey: null, block: null });
  assert.deepEqual(parseRoute('#/map'), { surface: 'map', brainKey: null, block: null });
  assert.deepEqual(parseRoute('#/map?block=sb_x'), {
    surface: 'map',
    brainKey: null,
    block: 'sb_x',
  });
});

test('parseRoute reads every hosted-world route', () => {
  assert.deepEqual(parseRoute('#/world/repo-alpha/tree'), {
    surface: 'tree',
    brainKey: 'repo-alpha',
    block: null,
  });
  assert.deepEqual(parseRoute('#/world/repo-alpha/map'), {
    surface: 'map',
    brainKey: 'repo-alpha',
    block: null,
  });
  assert.deepEqual(parseRoute('#/world/repo-alpha/map?block=sb_core'), {
    surface: 'map',
    brainKey: 'repo-alpha',
    block: 'sb_core',
  });
});

test('parseRoute tolerates a missing leading # and decodes the key', () => {
  assert.deepEqual(parseRoute('/tree'), { surface: 'tree', brainKey: null, block: null });
  assert.equal(parseRoute('#/world/my%20repo/tree')?.brainKey, 'my repo');
});

test('parseRoute returns null for a non-route (→ the landing rule runs)', () => {
  for (const h of ['', '#', '#/', '#/threshold', '#/nope', '#/world/x', '#/world/x/hall', 'garbage']) {
    assert.equal(parseRoute(h), null, `expected null for ${JSON.stringify(h)}`);
  }
});

// ── serializeRoute + roundtrip ────────────────────────────────────────────────

test('serializeRoute writes the canonical hash for each surface', () => {
  assert.equal(serializeRoute('universe', BOUND_VIEW, null), '#/universe');
  assert.equal(serializeRoute('hall', BOUND_VIEW, null), '#/hall');
  assert.equal(serializeRoute('threshold', BOUND_VIEW, null), '#/');
  assert.equal(serializeRoute('tree', BOUND_VIEW, null), '#/tree');
  assert.equal(serializeRoute('map', BOUND_VIEW, null), '#/map');
  assert.equal(serializeRoute('map', BOUND_VIEW, 'sb_x'), '#/map?block=sb_x');
});

test('serializeRoute writes the world basename, never the absolute root (no-leak)', () => {
  const hosted: ViewedBrain = { root: '/work/repo-alpha', displayName: 'repo-alpha', nodeCount: 9 };
  assert.equal(serializeRoute('tree', hosted, null), '#/world/repo-alpha/tree');
  assert.equal(serializeRoute('map', hosted, 'sb_core'), '#/world/repo-alpha/map?block=sb_core');
  // the absolute root never appears in the hash
  assert.ok(!serializeRoute('map', hosted, 'sb_core').includes('/work/'));
});

test('parse ∘ serialize round-trips every addressable location', () => {
  const hosted: ViewedBrain = { root: '/srv/project-b', displayName: 'project-b', nodeCount: 3 };
  const cases: Array<[Parameters<typeof serializeRoute>, ParsedRoute]> = [
    [['universe', BOUND_VIEW, null], { surface: 'universe', brainKey: null, block: null }],
    [['hall', BOUND_VIEW, null], { surface: 'hall', brainKey: null, block: null }],
    [['tree', BOUND_VIEW, null], { surface: 'tree', brainKey: null, block: null }],
    [['map', BOUND_VIEW, 'sb_x'], { surface: 'map', brainKey: null, block: 'sb_x' }],
    [['tree', hosted, null], { surface: 'tree', brainKey: 'project-b', block: null }],
    [['map', hosted, 'sb_y'], { surface: 'map', brainKey: 'project-b', block: 'sb_y' }],
  ];
  for (const [args, expected] of cases) {
    assert.deepEqual(parseRoute(serializeRoute(...args)), expected);
  }
});

// ── resolveBrainKey (R3 both directions + fallback) ───────────────────────────

test('resolveBrainKey finds a world by basename', () => {
  const worlds = [world('/work/repo-alpha', 'repo-alpha', 100), world('/srv/repo-beta', 'repo-beta')];
  const v = resolveBrainKey('repo-alpha', worlds, null);
  assert.deepEqual(v, { root: '/work/repo-alpha', displayName: 'repo-alpha', nodeCount: 100 });
});

test('resolveBrainKey falls back to the Hall registry when the panorama is empty', () => {
  const v = resolveBrainKey('repo-gamma', [], [brainEntry('/work/repo-gamma', 'repo-gamma')]);
  assert.deepEqual(v, { root: '/work/repo-gamma', displayName: 'repo-gamma', nodeCount: 42 });
});

test('resolveBrainKey returns null for an evicted key (→ landing fallback)', () => {
  assert.equal(resolveBrainKey('ghost', [world('/work/repo-alpha', 'repo-alpha')], []), null);
});

test('resolveBrainKey returns null on a basename COLLISION (ambiguous, never guesses)', () => {
  const worlds = [world('/work/dup', 'dup'), world('/elsewhere/dup', 'dup')];
  assert.equal(resolveBrainKey('dup', worlds, null), null);
});

// ── routeToIntent ─────────────────────────────────────────────────────────────

test('routeToIntent carries block only on the map surface', () => {
  const view: ViewedBrain = { root: '/work/repo-alpha', displayName: 'repo-alpha', nodeCount: 1 };
  assert.deepEqual(routeToIntent({ surface: 'map', brainKey: 'repo-alpha', block: 'sb_x' }, view), {
    surface: 'map',
    view,
    block: 'sb_x',
  });
  // a tree route drops any stray block
  assert.deepEqual(
    routeToIntent({ surface: 'tree', brainKey: 'repo-alpha', block: 'sb_x' } as ParsedRoute, view),
    { surface: 'tree', view, block: null },
  );
});

// ── resolveDeepLink (deep-link BEATS landing; honest fallback) ─────────────────

test('resolveDeepLink gives up when there is no deep link (landing rule runs)', () => {
  assert.deepEqual(resolveDeepLink(null, [], null, false), { kind: 'give-up' });
});

test('resolveDeepLink applies a bound route immediately — even before any read settles', () => {
  // This is the precedence: `#/tree` seeds the shell at once, so `surface != null`
  // before the landing gate ever fires.
  const out = resolveDeepLink({ surface: 'tree', brainKey: null, block: null }, [], null, false);
  assert.equal(out.kind, 'apply');
  assert.deepEqual(out.kind === 'apply' && out.intent, {
    surface: 'tree',
    view: BOUND_VIEW,
    block: null,
  });
});

test('resolveDeepLink applies a world route once the key resolves', () => {
  const worlds = [world('/work/repo-alpha', 'repo-alpha', 7)];
  const out = resolveDeepLink(
    { surface: 'map', brainKey: 'repo-alpha', block: 'sb_x' },
    worlds,
    null,
    false,
  );
  assert.equal(out.kind, 'apply');
  assert.deepEqual(out.kind === 'apply' && out.intent, {
    surface: 'map',
    view: { root: '/work/repo-alpha', displayName: 'repo-alpha', nodeCount: 7 },
    block: 'sb_x',
  });
});

test('resolveDeepLink waits (pending) for a world key while the reads are unsettled', () => {
  const out = resolveDeepLink({ surface: 'map', brainKey: 'later', block: null }, [], null, false);
  assert.deepEqual(out, { kind: 'pending' });
});

test('resolveDeepLink gives up on an unresolvable world key once the reads settle', () => {
  // evicted brain / basename collision / pre-F30 owner → land normally, never strand
  const out = resolveDeepLink({ surface: 'map', brainKey: 'gone', block: null }, [], [], true);
  assert.deepEqual(out, { kind: 'give-up' });
});
