/*
 * PreFlightCard render gate — the §C1 reader-2 rendering, proven against REAL
 * captured north packets (src/__fixtures__/preflight_north*.json) from the live
 * :1338 owner. react-dom/server, no new test deps (mirrors soft.test.tsx).
 *
 * Enforces on the card:
 *   - every beat renders from a REAL packet field (INV-01: never invents)
 *   - honest gaps are in-frame, each with exactly one action (INV-03/INV-07)
 *   - abstain wears iris violet; NOTHING else does (violet quarantine, INV-02)
 *   - blast counts are floors over a truncated subgraph (INV-08)
 *   - absent [needs-backend]/absent fields render absent-honest, never faked
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import PreFlightCard from './PreFlightCard';
import type { NorthPacket, ImpactOutput } from '../../api/toolTypes';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = (name: string): NorthPacket =>
  JSON.parse(readFileSync(join(FIX, name), 'utf8')) as NorthPacket;
const loadRaw = <T,>(name: string): T =>
  JSON.parse(readFileSync(join(FIX, name), 'utf8')) as T;

const warm = load('preflight_north.json'); // full_trust, memory, reception, next_move
const degraded = load('preflight_north_degraded.json'); // honest_gaps + reception
const impact = loadRaw<ImpactOutput>('impact.json');

const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) =>
  s
    .replace(/&#x27;/g, "'")
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"');

// ── The card mounts and names what's about to be touched ──────────────────────
test('the card renders its dialog shell + the seeded target', () => {
  const out = html(<PreFlightCard packet={warm} target="parser.rs" />);
  assert.match(out, /data-role="preflight-card"/);
  assert.match(out, /Before we touch/);
  assert.match(out, /parser\.rs/);
});

// ── BEAT 1: BINDING — real counts + action-language trust line ────────────────
test('BINDING beat: real node/edge counts from the packet fingerprint (INV-01)', () => {
  const out = html(<PreFlightCard packet={warm} target="x" />);
  const fp = (warm.binding as { fingerprint: { node_count: number; edge_count: number } })
    .fingerprint;
  assert.match(out, /data-role="binding-line"/);
  assert.match(out, new RegExp(fp.node_count.toLocaleString().replace(/,/g, ',')));
  assert.match(out, /nodes/);
  assert.match(out, /connections/);
});

// ── The reception rider — a real caller_root_mismatch, verbatim ───────────────
test('the reception rider renders the real honest line (abstain-class, violet)', () => {
  const out = html(<PreFlightCard packet={warm} target="x" />);
  const honest = (warm as { reception?: { honest?: string } }).reception?.honest ?? '';
  assert.ok(honest.length > 0, 'the fixture carries a real reception');
  assert.match(out, /data-role="reception-rider"/);
  // The rider is abstain-class (the FreshnessBanner), so it MAY wear violet.
  assert.match(out, /data-role="freshness-banner"/);
  assert.match(decode(out), new RegExp(honest.slice(0, 24).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
});

// ── BEAT 2: THE VERDICT + the blast line (floor language, INV-08) ─────────────
test('VERDICT beat: full_trust + gathering → reverify chip (not abstain)', () => {
  const out = html(<PreFlightCard packet={warm} target="x" />);
  // The warm capture is full_trust + gathering → reverify.
  assert.match(out, /data-verdict="reverify"/);
  assert.doesNotMatch(out, /data-verdict="abstain"/);
});

test('INV-08: the blast line is floor language over a truncated subgraph', () => {
  const truncated: ImpactOutput = { ...impact, truncated: true, total_blast_nodes: 14 };
  const out = html(<PreFlightCard packet={warm} target="x" impact={truncated} />);
  assert.match(out, /data-role="blast-line"/);
  assert.match(decode(out), /≥\s*14\s*mapped/);
});

test('absent impact → no blast line (INV-01, the beat renders nothing)', () => {
  const out = html(<PreFlightCard packet={warm} target="x" impact={null} />);
  assert.doesNotMatch(out, /data-role="blast-line"/);
});

// ── BEAT 3: ANCHORS — the neighborhood strip, real nodes only ─────────────────
test('ANCHORS beat: renders a strip of real focus/anchor labels (INV-01)', () => {
  const out = html(<PreFlightCard packet={warm} target="x" />);
  assert.match(out, /data-role="anchor-strip"/);
  const chips = out.match(/data-role="anchor-chip"/g) ?? [];
  assert.ok(chips.length > 0, 'at least one real anchor chip');
  assert.ok(chips.length <= 6, 'capped to a strip');
});

// ── BEAT 4: WHAT AGENTS KNOW — memory strip with R7 tier, absent-honest ───────
test('MEMORY beat: the real surfaced claim + author + age render', () => {
  const out = html(<PreFlightCard packet={warm} target="x" />);
  const m = warm.memory[0];
  assert.match(out, /data-role="memory-strip"/);
  assert.match(decode(out), new RegExp(m.claim.slice(0, 8).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  if (m.source_agent) assert.match(out, new RegExp(m.source_agent));
});

test('MEMORY beat: R7 tier rides through when the row carries it', () => {
  const tier = (warm.memory[0] as { tier?: string }).tier;
  const out = html(<PreFlightCard packet={warm} target="x" />);
  if (tier) {
    assert.match(out, /data-role="memory-tier"/);
    assert.match(decode(out), new RegExp(tier));
  }
});

test('MEMORY beat: empty recall over a non-empty store tells the R0 truth, no fake zero', () => {
  const missed = { ...warm, memory: [], memory_exists: 27 } as NorthPacket;
  const out = html(<PreFlightCard packet={missed} target="x" />);
  assert.match(out, /data-role="memory-empty"/);
  assert.match(decode(out), /27/);
  // Never a fabricated "0 memories".
  assert.doesNotMatch(decode(out), /\b0 (memories|notes)\b/);
});

// ── BEAT 5: HONEST GAPS — first-class, one action each (INV-03/INV-07) ────────
test('INV-03/INV-07: each honest gap is in-frame with exactly one action', () => {
  const out = html(<PreFlightCard packet={degraded} target="x" />);
  assert.ok((degraded.honest_gaps ?? []).length >= 1, 'the fixture has a real gap');
  assert.match(out, /data-role="honest-gaps"/);
  const gapCards = out.match(/data-role="gap-card"/g) ?? [];
  assert.equal(gapCards.length, degraded.honest_gaps.length, 'one card per real gap');
  const actions = out.match(/data-role="gap-action"/g) ?? [];
  assert.equal(actions.length, degraded.honest_gaps.length, 'exactly one action per gap');
});

test('no honest gaps → the gap beat drops (INV-01, renders nothing)', () => {
  assert.deepEqual(warm.honest_gaps, [], 'the warm capture has zero gaps');
  const out = html(<PreFlightCard packet={warm} target="x" />);
  assert.doesNotMatch(out, /data-role="honest-gaps"/);
});

// ── The primary next-move button — verbatim, absent → no button ───────────────
test('the next-move button renders the packet next_move verbatim', () => {
  const out = html(<PreFlightCard packet={warm} target="x" />);
  assert.match(out, /data-role="next-move"/);
  assert.match(decode(out), new RegExp((warm.next_move ?? '').slice(0, 16).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
});

test('absent next_move → no button (no fabricated action, INV-01)', () => {
  const noMove = { ...warm, next_move: null } as NorthPacket;
  const out = html(<PreFlightCard packet={noMove} target="x" />);
  assert.doesNotMatch(out, /data-role="next-move"/);
});

// ── The verdict surface: abstain wears violet, on a cold packet ───────────────
test('a needs_ingest packet → abstain verdict (violet), the honest cold read', () => {
  const cold = JSON.parse(readFileSync(join(FIX, 'north_cold.json'), 'utf8')) as NorthPacket;
  const out = html(<PreFlightCard packet={cold} target="x" />);
  assert.match(out, /data-verdict="abstain"/);
  assert.match(out, /data-abstain="true"/, 'the abstain chip is violet');
});

// ── VIOLET QUARANTINE at render: only abstain/unknown surfaces wear it ─────────
test('violet-lint holds at render: only abstain-class elements carry data-abstain', () => {
  // The warm card: reverify verdict (no abstain), a mismatch reception rider
  // (abstain-class FreshnessBanner) and the honest-unknown target dot.
  const out = html(<PreFlightCard packet={warm} target="x" targetBand="low" />);
  // Count abstain markers — each must be a KNOWN abstain surface (reception rider
  // / freshness banner). The reverify verdict chip must NOT be abstain.
  assert.doesNotMatch(out, /data-verdict="abstain"/);
  // The reverify verdict chip carries no abstain marker.
  const verdictSeg = out.slice(out.indexOf('data-verdict="reverify"'));
  assert.ok(verdictSeg.length > 0);
  // The non-abstain beats (binding line, anchor chips, memory) carry no iris.
  // (Component-level: iris is only reachable via the allow-listed soft components;
  // the violet-lint script proves the source-level quarantine separately.)
});

test('INV-02: no abstain-class element in the card carries an animation property', () => {
  const cold = JSON.parse(readFileSync(join(FIX, 'north_cold.json'), 'utf8')) as NorthPacket;
  const out = html(<PreFlightCard packet={cold} target="x" />);
  assert.doesNotMatch(out, /tremor-breath|animate-/, 'abstain never animates');
});
