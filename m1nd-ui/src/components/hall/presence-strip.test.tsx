/*
 * PresenceStrip render tests — the honesty invariants of the Hall's team band
 * (ORGANISM-INSIDE-PRD P1 · askGOD-verdict-P1), at the pixel boundary. Rendered
 * with react-dom/server (no new deps). Neutral fixtures only (no-leak law): no
 * real project/agent name of the owner ever appears in a mock.
 *
 * States proven: empty (honest sentence + the limitation always on the surface),
 * alive (who/where/on-what/age + the two mutation levels), expiring (fading with
 * its age still shown — never a binary online), and collision (calm amber, never
 * a modal). Plus the design law: the amber is the house `verdict-reverify`
 * pastel, never neon.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import PresenceStrip from './PresenceStrip';
import type { PresenceEntry, PresenceCollision } from '../../types';

const NOW = 1_800_000_000_000;
const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visibleText = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' '));

function presence(over: Partial<PresenceEntry> = {}): PresenceEntry {
  return {
    agent_id: 'atlas',
    root: '/work/repo-alpha',
    first_seen_ms: NOW - 10 * 60 * 1000,
    last_seen_ms: NOW - 30 * 1000,
    query_count: 4,
    mutation: {},
    ...over,
  };
}

// ── EMPTY: the honest sentence + the limitation, always on the surface ────────
test('empty roster renders the honest sentence and the limitation caption', () => {
  const out = html(<PresenceStrip presences={[]} collisions={[]} nowMs={NOW} />);
  assert.match(out, /data-role="presence-strip"/);
  assert.match(out, /data-role="presence-empty"/);
  assert.match(visibleText(<PresenceStrip presences={[]} collisions={[]} nowMs={NOW} />), /No agents visible to m1nd right now/);
  // The limitation is written on the surface even when empty (verdict risk).
  assert.match(out, /data-role="presence-limitation"/);
  assert.match(
    visibleText(<PresenceStrip presences={[]} collisions={[]} nowMs={NOW} />),
    /presence = activity visible to m1nd/,
  );
  // No collision, no error, no roster rows.
  assert.doesNotMatch(out, /data-role="presence-collision"/);
  assert.doesNotMatch(out, /data-role="presence-row"/);
});

// ── ALIVE: who / where / on-what / age + the two mutation levels ──────────────
test('a live roster renders agent, where, task, age, and an OBSERVED mutation dot', () => {
  const el = (
    <PresenceStrip
      presences={[
        presence({
          agent_id: 'atlas',
          root: '/work/repo-alpha',
          caller_root: '/work/repo-alpha/.wt/lane-1',
          last_seen_ms: NOW - 30 * 1000,
          task_ref: 'wire the presence beat',
          mutation: { observed_at_ms: NOW - 4000 },
        }),
      ]}
      collisions={[]}
      nowMs={NOW}
    />
  );
  const out = html(el);
  const text = visibleText(el);
  assert.match(out, /data-presence-agent="atlas"/);
  assert.match(out, /data-liveness="active"/);
  assert.match(out, /data-mutation="observed"/);
  assert.match(out, /data-mutation-dot="observed"/);
  assert.match(text, /atlas/);
  assert.match(text, /repo-alpha · lane-1/, 'where shows brain root · worktree');
  assert.match(text, /wire the presence beat/, 'the charter task is shown');
  assert.match(text, /30s/, 'the human age is rendered');
  assert.equal(html(<PresenceStrip presences={[]} collisions={[]} nowMs={NOW} />).match(/data-role="presence-count"/g)?.length, 1);
});

test('a DECLARED-intent agent renders the ring dot; a read-only agent has no dot', () => {
  const out = html(
    <PresenceStrip
      presences={[
        presence({ agent_id: 'beacon', mutation: { declared_intent: 'refactor the reader' } }),
        presence({ agent_id: 'cirrus', mutation: {} }),
      ]}
      collisions={[]}
      nowMs={NOW}
    />,
  );
  // Declared cloth: the ring dot, distinct from observed.
  const beacon = out.match(/data-presence-agent="beacon"[\s\S]*?<\/div>\s*<\/div>\s*<\/div>/)?.[0] ?? out;
  assert.match(out, /data-mutation="declared"/);
  assert.match(beacon, /data-mutation-dot="declared"/);
  // The read-only agent carries NO mutation dot.
  assert.match(out, /data-presence-agent="cirrus"[^>]*data-mutation="none"/);
  const cirrus = out.split('data-presence-agent="cirrus"')[1]?.split('data-presence-agent=')[0] ?? '';
  assert.doesNotMatch(cirrus, /data-mutation-dot=/, 'a read-only presence shows no mutation dot');
});

// ── EXPIRING: a fading agent keeps its age (never a binary "online") ──────────
test('an old-but-present agent reads fading and STILL shows its age', () => {
  const el = (
    <PresenceStrip
      presences={[presence({ agent_id: 'delta', last_seen_ms: NOW - 12 * 60 * 1000 })]}
      collisions={[]}
      nowMs={NOW}
    />
  );
  const out = html(el);
  assert.match(out, /data-liveness="fading"/);
  assert.match(visibleText(el), /12m/, 'the age is still rendered while fading');
  // Never a binary online/offline word.
  assert.doesNotMatch(visibleText(el).toLowerCase(), /\bonline\b|\boffline\b/);
});

// ── COLLISION: calm amber, house pastel, never a modal ────────────────────────
test('a collision renders a calm amber notice (verdict-reverify), never a modal/block', () => {
  const collisions: PresenceCollision[] = [
    {
      brain_root: '/work/repo-alpha',
      caller_root: '/work/repo-alpha/.wt/lane-1',
      agent_ids: ['atlas', 'beacon'],
      reason: 'same_worktree',
    },
  ];
  const el = (
    <PresenceStrip
      presences={[
        presence({ agent_id: 'atlas', caller_root: '/work/repo-alpha/.wt/lane-1', mutation: { observed_at_ms: NOW } }),
        presence({ agent_id: 'beacon', caller_root: '/work/repo-alpha/.wt/lane-1', mutation: { observed_at_ms: NOW } }),
      ]}
      collisions={collisions}
      nowMs={NOW}
    />
  );
  const out = html(el);
  assert.match(out, /data-role="presence-collision"/);
  assert.match(out, /data-collision-reason="same_worktree"/);
  assert.match(visibleText(el), /2 agents writing in the same worktree — atlas, beacon · lane-1/);
  // The house amber pastel, NOT neon: verdict-reverify, never a cyberpunk hue.
  assert.match(out, /verdict-reverify/);
  // No modal/overlay classes — it is an inline notice.
  assert.doesNotMatch(out, /fixed inset-0|z-50|role="dialog"/);
});

test('a declared_overlap collision reads in plain language', () => {
  const out = visibleText(
    <PresenceStrip
      presences={[presence()]}
      collisions={[{ brain_root: '/work/repo-beta', agent_ids: ['atlas', 'beacon'], reason: 'declared_overlap' }]}
      nowMs={NOW}
    />,
  );
  assert.match(out, /2 agents have overlapping declared work — atlas, beacon · repo-beta/);
});

// ── DESIGN LAW: nothing neon anywhere in the strip's markup ────────────────────
test('the strip carries no neon token (design law: paper/ink, calm)', () => {
  const out = html(
    <PresenceStrip
      presences={[
        presence({ agent_id: 'atlas', mutation: { observed_at_ms: NOW }, last_seen_ms: NOW - 5000 }),
        presence({ agent_id: 'beacon', mutation: { declared_intent: 'x' }, last_seen_ms: NOW - 8 * 60_000 }),
      ]}
      collisions={[
        { brain_root: '/work/repo-alpha', caller_root: '/work/repo-alpha/.wt/lane-1', agent_ids: ['atlas', 'beacon'], reason: 'same_worktree' },
      ]}
      nowMs={NOW}
    />,
  );
  // The cyberpunk residue that the SOFT PROOF Hall retired — none of it here.
  for (const neon of [/#00ff88/i, /#00f5ff/i, /bg-cyan/, /text-cyan/, /bg-emerald-\d/, /amber-\d{3}/, /slate-\d/, /neon/i]) {
    assert.doesNotMatch(out, neon, `neon token ${neon} must not appear in the strip`);
  }
});
