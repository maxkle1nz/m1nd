/*
 * UniverseView + TheLanding render tests (HUMAN-VIEW-V2 F30), at the pixel
 * boundary. Rendered with react-dom/server (no new deps), the repo's component-test
 * pattern. Neutral fixtures only (no-leak law): no real project/agent name.
 *
 * Proven: the L0 serif headline states universe FACTS (never a vital); every world
 * paints as a labelled disc with its honest freshness age; a live presence orbits
 * as a satellite; a pending world wears the amber dashed ring (the house
 * verdict-reverify pastel, NOT neon); the Landing lists per-world + owner buckets
 * with navigable scope; and the honest empty states read a sentence, not a blank.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import UniverseView from './UniverseView';
import TheLanding from './TheLanding';
import type { UniverseResponse, UniverseWorld } from '../../lib/universe';

const NOW = 1_800_000_000_000;
const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const noop = () => {};

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

function universe(over: Partial<UniverseResponse> = {}): UniverseResponse {
  return {
    schema: 'm1nd-universe-v0',
    worlds: [],
    owner: { alerts_pending: 0 },
    totals: { worlds: 0, awake: 0, pending: 0 },
    ...over,
  };
}

// ── UniverseView ─────────────────────────────────────────────────────────────

test('the L0 header states universe FACTS as a serif sentence', () => {
  const u = universe({ totals: { worlds: 2, awake: 1, pending: 3 } });
  const out = html(<UniverseView universe={u} onOpenWorld={noop} onOpenOwner={noop} nowMs={NOW} />);
  assert.match(out, /data-role="universe-headline"/);
  assert.match(out, /2 worlds · 1 awake · 3 await your hand/);
  assert.match(out, /font-serif/, 'the headline is serif (the client-composed voice)');
});

test('each world paints a labelled disc with its honest freshness age', () => {
  const u = universe({
    worlds: [
      world({ key: 'a', root: '/w/a', name: 'alpha', updated_ms: NOW - 2 * 60 * 60 * 1000 }),
      world({ key: 'b', root: '/w/b', name: 'beta', updated_ms: undefined }),
    ],
    totals: { worlds: 2, awake: 0, pending: 0 },
  });
  const out = html(<UniverseView universe={u} onOpenWorld={noop} onOpenOwner={noop} nowMs={NOW} />);
  assert.match(out, /data-role="universe-canvas"/);
  assert.match(out, /data-world-name="alpha"/);
  assert.match(out, /data-world-name="beta"/);
  assert.match(out, />alpha</, 'the world name is a mono label');
  assert.match(out, />2h</, 'a stale world shows its age as DATA (2h)');
  assert.match(out, />age unknown</, 'a world with no recorded freshness says so honestly');
});

test('a live presence orbits as a satellite; a pending world wears the amber dashed ring', () => {
  const u = universe({
    worlds: [
      world({
        key: 'a',
        root: '/w/a',
        name: 'alpha',
        awake: true,
        presences: [
          {
            agent_id: 'atlas',
            root: '/w/a',
            first_seen_ms: NOW - 60_000,
            last_seen_ms: NOW - 5_000,
            query_count: 4,
            mutation: { observed_at_ms: NOW - 2_000 },
          },
        ],
        pending: { stamps: 1, ratifies: 0 },
      }),
    ],
    totals: { worlds: 1, awake: 1, pending: 1 },
  });
  const out = html(<UniverseView universe={u} onOpenWorld={noop} onOpenOwner={noop} nowMs={NOW} />);
  assert.match(out, /data-role="universe-satellite"/, 'the live presence orbits the world');
  assert.match(out, /data-world-awake="true"/);
  assert.match(out, /stroke-dasharray="3 4"/, 'the pending ring is dashed');
  // The amber is the house verdict-reverify pastel, NEVER neon.
  assert.match(out, /--verdict-reverify/, 'the pending accent is the pastel token');
  assert.doesNotMatch(out, /#(39ff14|00ff|ff00ff|0ff)/i, 'no neon hue anywhere');
});

test('an empty universe reads an honest sentence, not a blank', () => {
  const out = html(<UniverseView universe={universe()} onOpenWorld={noop} onOpenOwner={noop} nowMs={NOW} />);
  assert.match(out, /data-role="universe-empty"/);
  assert.match(out, /An empty sky/);
  assert.doesNotMatch(out, /data-role="universe-canvas"/, 'no empty canvas when there are no worlds');
});

// ── TheLanding ───────────────────────────────────────────────────────────────

test('the Landing lists per-world stamps/ratifies + the owner alert, with the await-your-hand badge', () => {
  const u = universe({
    worlds: [
      world({ key: 'a', root: '/w/a', name: 'alpha', pending: { stamps: 2, ratifies: 1 } }),
    ],
    owner: { alerts_pending: 1 },
    totals: { worlds: 1, awake: 0, pending: 4 },
  });
  const out = html(<TheLanding universe={u} onOpenWorld={noop} onOpenOwner={noop} />);
  assert.match(out, /data-role="the-landing"/);
  assert.match(out, /data-role="landing-badge"[^>]*>[^<]*4 await your hand/);
  assert.doesNotMatch(out, /bell/i, 'the Landing is NEVER called a bell in copy');
  const stampItems = out.match(/data-landing-kind="stamp"/g) ?? [];
  const ratifyItems = out.match(/data-landing-kind="ratify"/g) ?? [];
  const alertItems = out.match(/data-landing-kind="alert"/g) ?? [];
  assert.equal(stampItems.length, 1, 'one stamp bucket');
  assert.equal(ratifyItems.length, 1, 'one ratify bucket');
  assert.equal(alertItems.length, 1, 'one owner-alert bucket');
  assert.match(out, /2 receipts await your stamp/);
  assert.match(out, /1 block await ratification/);
  assert.match(out, /data-landing-chip="owner"/, 'the alert wears the owner chip');
});

test('the Landing honest empty state reads a sentence', () => {
  const out = html(<TheLanding universe={universe()} onOpenWorld={noop} onOpenOwner={noop} />);
  assert.match(out, /data-role="landing-empty"/);
  assert.match(out, /Nothing awaits your hand/);
  assert.doesNotMatch(out, /data-role="landing-item"/, 'no items in an empty queue');
});
