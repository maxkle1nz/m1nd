/*
 * Hall semantics — honesty tests at the data layer (HUMAN-LAYER-PRD §4A).
 * Fixtures are REAL captured envelopes (INV-01: never hand-written JSON) —
 * ../__fixtures__/instances.json + instance_self.json were captured live from the
 * served owner on :1338 (the bound m1nd brain + the hosted project-b project brain).
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
  brainImplClass,
  bindingClassLabel,
  ownerLanding,
  isProjectBrain,
  resolvedBrainCounts,
  visibleConflicts,
  brainFreshnessMs,
  restBrainSelectorSupported,
  canOpenBrainInPlace,
  groupBrainCards,
} from './hallSemantics';
import type { InstanceListResponse, InstanceSelfResponse, InstanceRegistryEntry } from '../types';

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
  // A project brain has NO process status — present in the owner reads live,
  // never a stale/failed instance band (even though the entry carries a stale
  // instance status field, which does not apply to an in-process brain).
  assert.equal(livenessBand(project), 'live', 'a project brain reads live, not its instance status');
  assert.equal(livenessBand({ brain_kind: 'project', status: 'stale', stale: true }), 'live');
  // A real owner instance still honors its process status.
  assert.equal(livenessBand({ owner_live: false, stale: false, status: 'running' }), 'dormant');
  assert.equal(livenessBand({ owner_live: false, stale: true, status: 'stale' }), 'stale');
  assert.equal(livenessBand({ owner_live: false, status: 'failed' }), 'failure');
});

// ── Project-brain semantics: not an instance (§4A.3; Max's live screenshot) ────
test('isProjectBrain: only brain_kind:"project" is a project brain', () => {
  assert.equal(isProjectBrain(project), true);
  assert.equal(isProjectBrain(bound), false);
  assert.equal(isProjectBrain({ brain_kind: null }), false);
  assert.equal(isProjectBrain({}), false);
});

test('resolvedBrainCounts: a project brain uses its OWN entry counts, others use known', () => {
  // The project entry carries recorded counts → used directly, ignoring known.
  const pc = resolvedBrainCounts(project, { nodeCount: null, edgeCount: null });
  assert.equal(pc.nodeCount, project.node_count);
  assert.equal(pc.edgeCount, project.edge_count);
  // A fresh project brain (no recorded counts) → absent, never 0.
  const fresh = resolvedBrainCounts({ brain_kind: 'project', node_count: null, edge_count: null }, { nodeCount: 5, edgeCount: 9 });
  assert.equal(fresh.nodeCount, null);
  assert.equal(fresh.edgeCount, null);
  // A non-project brain uses the caller-supplied known counts.
  const bc = resolvedBrainCounts(bound, { nodeCount: 42, edgeCount: 99 });
  assert.equal(bc.nodeCount, 42);
  assert.equal(bc.edgeCount, 99);
});

test('visibleConflicts: lock/runtime + the ambiguous duplicate_workspace are stripped', () => {
  // The fixture project entry has a stale_lock conflict → hidden for kind=project.
  assert.ok(project.conflicts.includes('stale_lock'));
  assert.deepEqual(visibleConflicts(project), []);
  // duplicate_workspace is NO LONGER read raw here: the backend set it on every
  // ephemeral-id duplicate (N identical cards). The Hall now DERIVES the genuine
  // badge when it groups cards (groupBrainCards), so the ambiguous raw flag is
  // stripped in BOTH classes — a project brain AND a real owner.
  assert.deepEqual(visibleConflicts({ brain_kind: 'project', conflicts: ['duplicate_workspace'] }), []);
  assert.deepEqual(visibleConflicts({ brain_kind: null, conflicts: ['duplicate_workspace'] }), []);
  // A real owner keeps its OTHER (non-duplicate) conflicts.
  assert.deepEqual(visibleConflicts({ brain_kind: null, conflicts: ['stale_lock', 'shared_runtime'] }), ['stale_lock', 'shared_runtime']);
});

// ── Card grouping: one card per workspace (defense in depth; §4A.3) ───────────
test('groupBrainCards: 2 distinct workspaces render 2 cards; identical strings collapse to 1', () => {
  // The real fixture has a bound + a hosted project brain — DISTINCT workspace
  // roots → two cards, neither a genuine duplicate.
  const two = groupBrainCards(list.instances);
  assert.equal(two.length, 2, 'two distinct workspaces → two cards');
  assert.ok(two.every((c) => c.duplicateWorkspace === false), 'distinct roots are not duplicates');

  // Three entries for the SAME workspace (the ephemeral-id field bug: identical
  // workspace_root, different instance_ids) collapse to ONE card, silently.
  const dup = (id: string): InstanceRegistryEntry => ({ ...project, instance_id: id });
  const three = groupBrainCards([dup('inst_a'), dup('inst_b'), dup('inst_c')]);
  assert.equal(three.length, 1, 'three identical-workspace entries → one card');
  assert.equal(three[0].duplicateWorkspace, false, 'identical strings are NOT a genuine duplicate');
});

test('groupBrainCards: the FIRST (freshest) entry per workspace wins, order preserved', () => {
  // Registry order IS recency (freshest-first) — the first entry per workspace is
  // kept, never re-sorted (R4/INV-10).
  const mk = (id: string, ws: string): InstanceRegistryEntry => ({ ...project, instance_id: id, workspace_root: ws });
  const cards = groupBrainCards([
    mk('inst_fresh', '/rt/a'),
    mk('inst_stale', '/rt/a'),
    mk('inst_other', '/rt/b'),
  ]);
  assert.equal(cards.length, 2, 'two workspaces → two cards');
  assert.equal(cards[0].entry.instance_id, 'inst_fresh', 'the freshest (first) entry wins its card');
  assert.deepEqual(cards.map((c) => c.entry.workspace_root), ['/rt/a', '/rt/b'], 'first-appearance order preserved');
});

test('groupBrainCards: the genuine duplicate badge fires only when DISTINCT roots collapse', () => {
  // Two textually DIFFERENT workspace_root strings that canonicalize to the same
  // place (here: a trailing slash) — the rare real "duplicate workspace" conflict.
  const a: InstanceRegistryEntry = { ...project, instance_id: 'inst_a', workspace_root: '/rt/repo' };
  const b: InstanceRegistryEntry = { ...project, instance_id: 'inst_b', workspace_root: '/rt/repo/' };
  const cards = groupBrainCards([a, b]);
  assert.equal(cards.length, 1, 'two roots for the same place → one card');
  assert.equal(cards[0].duplicateWorkspace, true, 'distinct root strings → the genuine duplicate badge');
});

test('brainFreshnessMs: a project brain uses last_activity_ms, others the heartbeat', () => {
  assert.equal(brainFreshnessMs(project), project.last_activity_ms);
  assert.equal(brainFreshnessMs(bound), bound.last_heartbeat_ms);
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
  assert.equal(repoBasename('/rt/runtimes/claude'), 'claude');
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

// ── Implementation class → RECEIPT line (§4A.8, INV-14): NOT a card face ──────
test('brainImplClass: self→bound, brain_kind:project→project, absent non-self→sibling (never guessed)', () => {
  assert.equal(brainImplClass(bound, true), 'bound');
  assert.equal(brainImplClass(project, false), 'project');
  assert.equal(brainImplClass({ brain_kind: null }, false), 'sibling');
  assert.equal(brainImplClass({ brain_kind: undefined }, false), 'sibling');
});

test('bindingClassLabel: the receipt wording (process-bound / owner-hosted / sibling owner)', () => {
  assert.equal(bindingClassLabel(bound, true), 'process-bound');
  assert.equal(bindingClassLabel(project, false), 'owner-hosted');
  assert.equal(bindingClassLabel({ brain_kind: null }, false), 'sibling owner (own port)');
});

// ── INV-12: landing routing from owner state (§4A.1) ──────────────────────────
test('INV-12: ownerLanding — zero brains→threshold, history→tree, no history→hall', () => {
  assert.equal(ownerLanding({ brainCount: 0, hasLocalHistory: false }), 'threshold');
  assert.equal(ownerLanding({ brainCount: 0, hasLocalHistory: true }), 'threshold', 'no brains beats stale history');
  assert.equal(ownerLanding({ brainCount: 2, hasLocalHistory: true }), 'tree');
  assert.equal(ownerLanding({ brainCount: 2, hasLocalHistory: false }), 'hall');
});

// ── §4A.9.5: the REST brain selector capability (Open feature-detect) ─────────
test('§4A.9.5: restBrainSelectorSupported is TRUE only when the tools stamp is present', () => {
  assert.equal(restBrainSelectorSupported({ tools: [], rest_brain_selector: true }), true);
  assert.equal(restBrainSelectorSupported({ tools: [], rest_brain_selector: false }), false);
  assert.equal(restBrainSelectorSupported({ tools: [] }), false, 'a pre-2H owner (no stamp) is unsupported');
  assert.equal(restBrainSelectorSupported(null), false);
  assert.equal(restBrainSelectorSupported(undefined), false);
});

test('§4A.9: canOpenBrainInPlace — bound always; hosted iff the stamp; sibling never', () => {
  // The bound/self brain always opens in the tree (it IS the default graph).
  assert.equal(canOpenBrainInPlace(bound, true, false), true);
  assert.equal(canOpenBrainInPlace(bound, true, true), true);
  // A hosted project brain opens in place ONLY with the selector stamp.
  assert.equal(canOpenBrainInPlace(project, false, false), false, 'no stamp → hosted stays disabled');
  assert.equal(canOpenBrainInPlace(project, false, true), true, 'stamp → hosted opens in the tree');
  // A sibling owner (no brain_kind) never opens in place (it uses its own port).
  assert.equal(canOpenBrainInPlace({ brain_kind: null }, false, true), false);
});
