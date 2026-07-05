/*
 * preflight (view model) — proven against REAL captured north packets from the
 * live :1338 owner (src/__fixtures__/preflight_north*.json), never hand-written
 * JSON, so the reader can't drift from the wire (PRD §7 fixture discipline).
 *
 *   preflight_north.json          — warm, full_trust, memory surfaced (1 claim,
 *                                    R7 tier), reception mismatch rider, next_move.
 *   preflight_north_degraded.json — mismatch, honest_gaps non-empty, reception.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { NorthPacket } from '../api/toolTypes';
import {
  bindingView,
  receptionView,
  anchorChips,
  memoryRows,
  emptyMemoryLine,
  honestGaps,
  preflightVerdict,
  nextMove,
  seedBand,
} from './preflight';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const load = (name: string): NorthPacket =>
  JSON.parse(readFileSync(join(FIX, name), 'utf8')) as NorthPacket;

const warm = load('preflight_north.json');
const cold = JSON.parse(readFileSync(join(FIX, 'north_cold.json'), 'utf8')) as NorthPacket;

// ── BINDING beat ──────────────────────────────────────────────────────────────
test('bindingView reads the real fingerprint counts + trust mode (never invented)', () => {
  const b = bindingView(warm);
  const fp = (warm.binding as { fingerprint: { node_count: number; edge_count: number } })
    .fingerprint;
  assert.equal(b.nodeCount, fp.node_count, 'node count is the packet field, verbatim');
  assert.equal(b.edgeCount, fp.edge_count);
  assert.equal(b.trustMode, 'full_trust');
  assert.equal(b.degraded, false);
  assert.match(b.trustLine, /grounded/i);
});

test('bindingView: a cold (needs_ingest) packet reads degraded, action-language line', () => {
  const b = bindingView(cold);
  assert.equal(b.degraded, true);
  assert.notEqual(b.trustMode, 'full_trust');
  // No epistemic vocabulary at rung 0–1 (the anxiety principle, §2).
  assert.doesNotMatch(b.trustLine.toLowerCase(), /binding|calibration|abstain|epistemic/);
});

// ── The reception rider (§C1.4 / JOINT-I) ─────────────────────────────────────
test('receptionView surfaces a real caller_root_mismatch verbatim, never reworded', () => {
  const r = receptionView(warm);
  assert.equal(r.mismatch, true, 'the captured packet carries a real mismatch');
  const raw = (warm as { reception?: { honest?: string } }).reception?.honest ?? '';
  assert.equal(r.honest, raw, 'the honest line is the engine string, verbatim');
  assert.ok(raw.length > 0);
});

test('receptionView: absent reception → no rider (INV-01, renders nothing)', () => {
  const noReception = { ...warm, reception: undefined } as NorthPacket;
  const r = receptionView(noReception);
  assert.equal(r.mismatch, false);
  assert.equal(r.honest, null);
});

// ── ANCHORS beat ──────────────────────────────────────────────────────────────
test('anchorChips draws only nodes the packet returned, deduped, capped', () => {
  const chips = anchorChips(warm, 6);
  assert.ok(chips.length > 0, 'the warm packet has focus + anchors');
  assert.ok(chips.length <= 6, 'capped to a strip');
  // Every chip traces to a real node_id in the packet (no invention, INV-01).
  const ctx = warm.context as { focus_nodes?: Array<{ node_id?: string }>; anchors?: Array<{ node_id?: string }> };
  const known = new Set(
    [...(ctx.focus_nodes ?? []), ...(ctx.anchors ?? [])].map((n) => n.node_id),
  );
  for (const c of chips) assert.ok(known.has(c.nodeId), `${c.nodeId} is a real packet node`);
  // Deduped.
  assert.equal(new Set(chips.map((c) => c.nodeId)).size, chips.length);
});

test('anchorChips: a context-less (cold) packet → empty strip (the beat drops)', () => {
  assert.deepEqual(anchorChips(cold), []);
});

// ── WHAT AGENTS KNOW beat ─────────────────────────────────────────────────────
test('memoryRows carries the real claim + author + age + R7 tier from the packet', () => {
  const rows = memoryRows(warm);
  assert.ok(rows.length >= 1, 'the warm packet surfaced ≥1 memory');
  const raw = warm.memory[0];
  const row = rows[0];
  assert.equal(row.claim, raw.claim, 'claim verbatim');
  assert.equal(row.sourceAgent, raw.source_agent ?? null);
  assert.equal(row.ageMs, raw.age_ms ?? null);
  // R7: tier rides through when the row carries it.
  const rawTier = (raw as { tier?: string }).tier ?? null;
  assert.equal(row.tier, rawTier);
});

test('memoryRows: absent age/author stay null (INV-04, never faked)', () => {
  const packet = {
    ...warm,
    memory: [{ kind: 'light', claim: 'legacy', node_id: 'n1' }],
  } as NorthPacket;
  const [row] = memoryRows(packet);
  assert.equal(row.ageMs, null);
  assert.equal(row.sourceAgent, null);
});

test('emptyMemoryLine tells the truth with R0 memory_exists (recall miss ≠ empty store)', () => {
  // Non-empty store, nothing surfaced → says the store is non-empty.
  const missed = { ...warm, memory: [], memory_exists: 27 } as NorthPacket;
  assert.match(emptyMemoryLine(missed), /27/);
  assert.match(emptyMemoryLine(missed).toLowerCase(), /none matched|surfaced/);
  // Truly empty store → says so, no fabricated count.
  const empty = { ...warm, memory: [], memory_exists: 0 } as NorthPacket;
  assert.doesNotMatch(emptyMemoryLine(empty), /27/);
  assert.match(emptyMemoryLine(empty).toLowerCase(), /no notes here yet|leave notes/);
});

// ── HONEST GAPS beat ──────────────────────────────────────────────────────────
test('honestGaps passes the real gap strings straight through', () => {
  const degraded = load('preflight_north_degraded.json');
  const gaps = honestGaps(degraded);
  assert.ok(gaps.length >= 1, 'the degraded capture carries a real honest gap');
  assert.deepEqual(gaps, degraded.honest_gaps);
});

test('honestGaps: an empty honest_gaps → no gaps (the beat drops)', () => {
  assert.deepEqual(honestGaps(warm), warm.honest_gaps);
});

// ── VERDICT surface ───────────────────────────────────────────────────────────
test('preflightVerdict: needs_ingest / degraded → abstain (violet, "I won\'t guess")', () => {
  assert.equal(preflightVerdict(cold), 'abstain');
});

test('preflightVerdict: full_trust + gathering sufficiency → reverify (worth a second look)', () => {
  // The warm capture is full_trust with sufficiency "gathering".
  assert.equal(warm.sufficiency?.state, 'gathering');
  assert.equal(preflightVerdict(warm), 'reverify');
});

test('preflightVerdict: full_trust + sufficient → act (good to go)', () => {
  const sufficient = {
    ...warm,
    sufficiency: { ...(warm.sufficiency ?? {}), state: 'sufficient' },
  } as NorthPacket;
  assert.equal(preflightVerdict(sufficient), 'act');
});

// ── The next-move button + seed band ──────────────────────────────────────────
test('nextMove is the packet next_move verbatim; absent → null (no fabricated action)', () => {
  assert.equal(nextMove(warm), warm.next_move);
  const noMove = { ...warm, next_move: null } as NorthPacket;
  assert.equal(nextMove(noMove), null);
});

test('seedBand falls back to the honest unknown, never a confident default', () => {
  assert.equal(seedBand(null), 'insufficient_evidence');
  assert.equal(seedBand('low'), 'low');
});
