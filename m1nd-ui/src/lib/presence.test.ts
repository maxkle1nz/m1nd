/*
 * Presence semantics — the pure gate (ORGANISM-INSIDE-PRD P1 · askGOD-verdict-P1).
 * Age honesty, the two-level mutation signal, and the collision predicate
 * (binding change 2) — the one place the "same worktree, both writing" law is
 * greppable and proven. Neutral names only (no-leak law).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { PresenceEntry } from '../types';
import {
  compactAge,
  presenceLiveness,
  mutationSignal,
  hasMutationSignal,
  deriveCollisions,
  resolveCollisions,
  PRESENCE_FADE_AFTER_MS,
} from './presence';

const NOW = 1_800_000_000_000;

/** A minimal, type-faithful presence — neutral codename + repo. */
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

// ── compactAge: honest, compact, never a faked age ────────────────────────────
test('compactAge renders now / s / m / h / d and an honest "?" for absent', () => {
  assert.equal(compactAge(NOW - 2_000, NOW), 'now'); // < 5s
  assert.equal(compactAge(NOW - 42_000, NOW), '42s');
  assert.equal(compactAge(NOW - 12 * 60_000, NOW), '12m');
  assert.equal(compactAge(NOW - 3 * 3_600_000, NOW), '3h');
  assert.equal(compactAge(NOW - 2 * 86_400_000, NOW), '2d');
  assert.equal(compactAge(null, NOW), '?');
  assert.equal(compactAge(undefined, NOW), '?');
  // A future/negative delta never fabricates — clamps to "now".
  assert.equal(compactAge(NOW + 9_999, NOW), 'now');
});

// ── liveness: age always known; fading is presentational, not expiry ──────────
test('presenceLiveness flips to fading past the display threshold, active within it', () => {
  assert.equal(presenceLiveness({ last_seen_ms: NOW - 30_000 }, NOW), 'active');
  assert.equal(presenceLiveness({ last_seen_ms: NOW - (PRESENCE_FADE_AFTER_MS - 1) }, NOW), 'active');
  assert.equal(presenceLiveness({ last_seen_ms: NOW - PRESENCE_FADE_AFTER_MS }, NOW), 'fading');
  assert.equal(presenceLiveness({ last_seen_ms: NOW - 20 * 60_000 }, NOW), 'fading');
});

// ── the two honest mutation levels (verdict 1c) ───────────────────────────────
test('mutationSignal: observed outranks declared outranks none', () => {
  assert.equal(mutationSignal(presence({ mutation: {} })), 'none');
  assert.equal(mutationSignal(presence({ mutation: { declared_intent: 'refactor the reader' } })), 'declared');
  assert.equal(mutationSignal(presence({ mutation: { declared_intent: '   ' } })), 'none'); // blank ≠ declared
  assert.equal(mutationSignal(presence({ mutation: { observed_at_ms: NOW - 5_000 } })), 'observed');
  // observed wins even when an intent is also declared.
  assert.equal(
    mutationSignal(presence({ mutation: { observed_at_ms: NOW, declared_intent: 'x' } })),
    'observed',
  );
  assert.equal(hasMutationSignal(presence({ mutation: {} })), false);
  assert.equal(hasMutationSignal(presence({ mutation: { observed_at_ms: NOW } })), true);
});

// ── the collision predicate (binding change 2) ────────────────────────────────
test('two mutating hands in the SAME worktree collide (the 2026-07-06 incident shape)', () => {
  const wt = '/work/repo-alpha/.wt/lane-1';
  const cs = deriveCollisions([
    presence({ agent_id: 'atlas', caller_root: wt, mutation: { observed_at_ms: NOW } }),
    presence({ agent_id: 'beacon', caller_root: wt, mutation: { observed_at_ms: NOW } }),
  ]);
  assert.equal(cs.length, 1);
  assert.equal(cs[0].brain_root, '/work/repo-alpha');
  assert.equal(cs[0].caller_root, wt);
  assert.deepEqual([...cs[0].agent_ids].sort(), ['atlas', 'beacon']);
  assert.equal(cs[0].reason, 'same_worktree');
});

test('same brain, DIFFERENT worktrees NEVER warns — the normal burst shape', () => {
  const cs = deriveCollisions([
    presence({ agent_id: 'atlas', caller_root: '/work/repo-alpha/.wt/lane-1', mutation: { observed_at_ms: NOW } }),
    presence({ agent_id: 'beacon', caller_root: '/work/repo-alpha/.wt/lane-2', mutation: { observed_at_ms: NOW } }),
    presence({ agent_id: 'cirrus', caller_root: '/work/repo-alpha/.wt/lane-3', mutation: { observed_at_ms: NOW } }),
  ]);
  assert.equal(cs.length, 0, 'three isolated worktrees on one brain is normal, not a collision');
});

test('a mutating hand + a NON-mutating hand in one worktree does not warn', () => {
  const wt = '/work/repo-beta/.wt/lane-1';
  const cs = deriveCollisions([
    presence({ agent_id: 'atlas', root: '/work/repo-beta', caller_root: wt, mutation: { observed_at_ms: NOW } }),
    presence({ agent_id: 'beacon', root: '/work/repo-beta', caller_root: wt, mutation: {} }), // read-only
  ]);
  assert.equal(cs.length, 0, 'a collision needs BOTH hands mutating');
});

test('two mutating hands on the BARE root (no worktree) collide, caller_root null', () => {
  const cs = deriveCollisions([
    presence({ agent_id: 'atlas', caller_root: null, mutation: { observed_at_ms: NOW } }),
    presence({ agent_id: 'beacon', caller_root: undefined, mutation: { declared_intent: 'writing' } }),
  ]);
  assert.equal(cs.length, 1);
  assert.equal(cs[0].caller_root, null);
});

test('a lone mutating hand is not a collision; three hands report all three ids', () => {
  assert.equal(deriveCollisions([presence({ mutation: { observed_at_ms: NOW } })]).length, 0);
  const wt = '/work/repo-alpha/.wt/lane-1';
  const cs = deriveCollisions([
    presence({ agent_id: 'atlas', caller_root: wt, mutation: { observed_at_ms: NOW } }),
    presence({ agent_id: 'beacon', caller_root: wt, mutation: { observed_at_ms: NOW } }),
    presence({ agent_id: 'cirrus', caller_root: wt, mutation: { declared_intent: 'x' } }),
  ]);
  assert.equal(cs.length, 1);
  assert.equal(cs[0].agent_ids.length, 3);
});

// ── resolveCollisions: the server is authoritative; derive only when absent ────
test('resolveCollisions prefers the server field (even empty) and derives only when absent', () => {
  const wt = '/work/repo-alpha/.wt/lane-1';
  const roster = [
    presence({ agent_id: 'atlas', caller_root: wt, mutation: { observed_at_ms: NOW } }),
    presence({ agent_id: 'beacon', caller_root: wt, mutation: { observed_at_ms: NOW } }),
  ];
  // Field ABSENT → derive (the pre-P1 owner fallback).
  assert.equal(resolveCollisions({ presences: roster }).length, 1);
  // Field PRESENT but empty → trust the server, do NOT re-derive.
  assert.equal(resolveCollisions({ presences: roster, collisions: [] }).length, 0);
  // Field present with a declared_overlap the UI would never derive → rendered.
  const served = resolveCollisions({
    presences: roster,
    collisions: [{ brain_root: '/work/repo-alpha', agent_ids: ['atlas', 'beacon'], reason: 'declared_overlap' }],
  });
  assert.equal(served.length, 1);
  assert.equal(served[0].reason, 'declared_overlap');
});
