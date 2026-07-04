/*
 * Hall semantics — honesty tests at the data layer (HUMAN-LAYER-PRD §4A).
 * Fixtures are REAL captured envelopes (INV-01: never hand-written JSON) —
 * ../__fixtures__/instances.json + instance_self.json were captured live from the
 * served owner on :1338 (the bound m1nd brain + the hosted Cherry project brain).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  livenessBand,
  lastSeenPhrase,
  persistedPhrase,
  bornPhrase,
  repoBasename,
  entryBaseUrl,
  brainCounts,
  nameMatches,
  brainKindBadge,
  ownerLanding,
} from './hallSemantics';
import type { InstanceListResponse, InstanceSelfResponse } from '../types';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const load = <T>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));

// ── Fixtures: the real three-class list the live owner reports ─────────────────
const list = load<InstanceListResponse>('instances.json');
const self = load<InstanceSelfResponse>('instance_self.json');
const bound = list.instances.find((e) => e.brain_kind == null)!;
const project = list.instances.find((e) => e.brain_kind === 'project')!;

test('fixture precondition: the live list carries both a bound and a hosted project brain', () => {
  assert.ok(bound, 'a bound (brain_kind:null) entry exists');
  assert.ok(project, 'a hosted project brain (brain_kind:"project") exists');
});

// ── Liveness band: calm, matte, honest (§4A.3) ────────────────────────────────
test('livenessBand: live owner → live; stale heartbeat → stale; dormant when not running', () => {
  assert.equal(livenessBand(bound), 'live', 'live non-stale owner reads live');
  // The captured project brain is a live owner with a stale heartbeat.
  assert.equal(livenessBand(project), 'stale');
  // A dead pid (owner_live flips false) reads dormant, never a false live.
  assert.equal(livenessBand({ owner_live: false, stale: false, status: 'running' }), 'dormant');
  assert.equal(livenessBand({ owner_live: false, stale: true, status: 'stale' }), 'stale');
  assert.equal(livenessBand({ owner_live: false, status: 'failed' }), 'failure');
});

// ── Freshness: dormant-aware, never faked (INV-04 discipline) ─────────────────
test('lastSeenPhrase: real ages read honestly, never "now" for a gap', () => {
  assert.equal(lastSeenPhrase(null), 'last seen unknown');
  const now = 1_000_000_000_000;
  assert.equal(lastSeenPhrase(now, now), 'seen just now');
  assert.equal(lastSeenPhrase(now - 3 * 60 * 60 * 1000, now), 'last seen 3h ago');
  assert.equal(lastSeenPhrase(now - 5 * 24 * 60 * 60 * 1000, now), 'last seen 5d ago');
  // A real heartbeat from the fixture produces a phrase, never a crash.
  assert.match(lastSeenPhrase(bound.last_heartbeat_ms), /seen|last seen/);
});

test('persistedPhrase / bornPhrase: null-safe, absent renders absent', () => {
  assert.equal(persistedPhrase(null), null);
  assert.equal(persistedPhrase(120), 'persisted 2m ago');
  assert.equal(bornPhrase(null), null);
  const now = 1_000_000_000_000;
  assert.equal(bornPhrase(now - 3 * 24 * 60 * 60 * 1000, now), 'born 3 days ago');
  // self carries a real started_at — it yields some born phrase.
  assert.match(bornPhrase(self.instance.started_at_ms)!, /born/);
});

// ── Name + path + base url (§4A.3 / §4A.4) ─────────────────────────────────────
test('repoBasename + entryBaseUrl from the real entries', () => {
  assert.equal(repoBasename('/Users/kle1nz/.m1nd/runtimes/claude'), 'claude');
  assert.equal(repoBasename(bound.workspace_root), 'claude');
  // The bound entry serves its own UI on loopback.
  assert.equal(entryBaseUrl(bound), 'http://127.0.0.1:1338');
  // The hosted project brain has no bound port → no open-in-place URL (honest).
  assert.equal(entryBaseUrl(project), null);
});

// ── INV-10: count honesty — absent renders absent, never zero ─────────────────
test('INV-10: brainCounts returns numbers only when known, null (not 0) otherwise', () => {
  // Self graph_state carries real counts.
  const known = brainCounts({
    nodeCount: self.graph_state.node_count,
    edgeCount: self.graph_state.edge_count,
  });
  assert.equal(known.nodeCount, self.graph_state.node_count);
  assert.equal(known.edgeCount, self.graph_state.edge_count);
  assert.ok(known.nodeCount! > 0, 'fixture precondition: self has a real graph');
  // A dormant/hosted brain at rest reports nothing → null, NOT 0.
  const unknown = brainCounts({ nodeCount: undefined, edgeCount: null });
  assert.equal(unknown.nodeCount, null);
  assert.equal(unknown.edgeCount, null);
  assert.notEqual(unknown.nodeCount, 0, 'absent count must never collapse to zero (INV-10)');
});

// ── INV-09: the delete floor — exact-name match, empty never matches ──────────
test('INV-09: nameMatches requires an EXACT, non-empty match (the GitHub pattern)', () => {
  assert.equal(nameMatches('', 'claude'), false, 'empty never matches');
  assert.equal(nameMatches('  ', 'claude'), false, 'whitespace never matches');
  assert.equal(nameMatches('claud', 'claude'), false, 'partial never matches');
  assert.equal(nameMatches('Claude', 'claude'), false, 'case-sensitive');
  assert.equal(nameMatches('claude ', 'claude'), true, 'trimmed exact matches');
  assert.equal(nameMatches('claude', 'claude'), true);
});

// ── Kind badge (§4A.3): self=bound, project=project, else sibling ─────────────
test('brainKindBadge: self→bound, brain_kind:project→project, absent non-self→sibling (never guessed)', () => {
  assert.equal(brainKindBadge(bound, true), 'bound');
  assert.equal(brainKindBadge(project, false), 'project');
  assert.equal(brainKindBadge({ brain_kind: null }, false), 'sibling');
  assert.equal(brainKindBadge({ brain_kind: undefined }, false), 'sibling');
});

// ── INV-12: landing routing from owner state (§4A.1) ──────────────────────────
test('INV-12: ownerLanding — zero brains→threshold, history→tree, no history→hall', () => {
  assert.equal(ownerLanding({ brainCount: 0, hasLocalHistory: false }), 'threshold');
  assert.equal(ownerLanding({ brainCount: 0, hasLocalHistory: true }), 'threshold', 'no brains beats stale history');
  assert.equal(ownerLanding({ brainCount: 2, hasLocalHistory: true }), 'tree');
  assert.equal(ownerLanding({ brainCount: 2, hasLocalHistory: false }), 'hall');
});
