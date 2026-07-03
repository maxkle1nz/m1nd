/*
 * SOFT PROOF semantics — honesty-invariant tests at the data layer.
 * These do not need a DOM. Fixtures are REAL captured envelopes (INV-01 rule:
 * never hand-written JSON) in ../__fixtures__/.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  tierToBand,
  postItState,
  humanizeAge,
  authorLabel,
  blastCountPhrase,
  actionLanguageForGap,
  nextMoveButtonLabel,
  RUNG01_BANNED_VOCABULARY,
  STALE_AFTER_MS,
} from './softProof';
import type { ImpactOutput, NorthPacket, SeekOutput } from '../api/toolTypes';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const load = <T>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));

// ── INV-02: abstain/Unknown reads as insufficient_evidence (violet band) ──────
test('INV-02: Unknown tier maps to insufficient_evidence, never medium', () => {
  assert.equal(tierToBand('Unknown'), 'insufficient_evidence');
  assert.equal(tierToBand('LowRisk'), 'low');
  assert.equal(tierToBand('MediumRisk'), 'medium');
  assert.equal(tierToBand('HighRisk'), 'high');
  // Unrecognized / missing tier is the honest unknown, not a false "low".
  assert.equal(tierToBand('garbage'), 'insufficient_evidence');
});

// ── INV-04: provenance absent, not faked ──────────────────────────────────────
test('INV-04: absent provenance yields the unknown state and honest labels', () => {
  assert.equal(postItState({ ageMs: null, sourceAgent: null }), 'unknown');
  assert.equal(authorLabel(null), 'author unknown');
  assert.equal(humanizeAge(null), 'age unknown');
  // A real value is never rendered as "now".
  assert.notEqual(humanizeAge(1000), 'now');
});

test('post-it state machine: fresh / aging / stale', () => {
  assert.equal(postItState({ ageMs: 1000, sourceAgent: 'a' }), 'fresh');
  assert.equal(postItState({ ageMs: STALE_AFTER_MS + 1, sourceAgent: 'a' }), 'aging');
  assert.equal(postItState({ ageMs: 1000, sourceAgent: 'a', evidenceStale: true }), 'stale');
  // Unknown precedence: no provenance beats any freshness claim.
  assert.equal(postItState({ ageMs: null, sourceAgent: null, evidenceStale: true }), 'unknown');
});

// ── INV-08: blast counts are floors, not ceilings ─────────────────────────────
test('INV-08: truncated impact renders floor language; untruncated renders a plain count', () => {
  const big = load<ImpactOutput>('impact.json');
  const small = load<ImpactOutput>('impact_small.json');
  assert.equal(big.truncated, true, 'fixture precondition: big impact is truncated');
  assert.equal(small.truncated, false, 'fixture precondition: small impact is untruncated');

  const bigPhrase = blastCountPhrase(big.total_blast_nodes, big.truncated);
  assert.match(bigPhrase, /^≥ \d+ mapped files$/, 'truncated must use ≥ floor language');

  const smallPhrase = blastCountPhrase(small.total_blast_nodes, small.truncated);
  assert.doesNotMatch(smallPhrase, /≥/, 'untruncated must NOT use ≥');
  assert.match(smallPhrase, /^\d+ mapped files$/);
});

// ── INV-07: gaps carry a next action, not anxiety; vocabulary quarantined ─────
test('INV-07: every real honest_gap renders action language + exactly one step, no epistemic vocab', () => {
  const cold = load<NorthPacket>('north_cold.json');
  assert.ok(cold.honest_gaps.length >= 2, 'fixture precondition: cold north has gaps');
  for (const gap of cold.honest_gaps) {
    const copy = actionLanguageForGap(gap);
    assert.ok(copy.text.length > 0, 'gap must produce plain copy');
    // The rung-0/1 copy must not leak banned epistemic vocabulary.
    const lower = copy.text.toLowerCase();
    for (const banned of RUNG01_BANNED_VOCABULARY) {
      assert.ok(!lower.includes(banned), `banned vocab "${banned}" leaked into: "${copy.text}"`);
    }
  }
  // The needs_ingest gap routes to a concrete read action.
  const ingestGap = cold.honest_gaps.find((g) => /empty or unbound/i.test(g));
  assert.ok(ingestGap);
  assert.equal(actionLanguageForGap(ingestGap as string).action, 'Read the repo');
});

test('INV-07: an unmapped gap falls back to a double-check action, never a bare warning', () => {
  const copy = actionLanguageForGap('some entirely novel situation the engine reported');
  assert.equal(copy.action, 'Double-check');
  assert.ok(copy.text.length > 0);
});

test('next_move from real north maps to an action button label', () => {
  const warm = load<NorthPacket>('north_warm.json');
  const label = nextMoveButtonLabel(warm.next_move);
  assert.ok(label.length > 0);
  assert.doesNotMatch(label.toLowerCase(), /surgical_context|abstain|binding/);
});

// ── INV-04 against the real seek envelope: a code hit with no author stays unknown
test('INV-04: a real seek code-node hit without source_agent renders unknown state', () => {
  const seek = load<SeekOutput>('seek.json');
  // The graph code nodes in seek results carry no source_agent/authored_ms_ago.
  const codeHit = seek.results.find((r) => !r.source_agent && r.authored_ms_ago == null);
  assert.ok(codeHit, 'fixture precondition: at least one hit lacks provenance');
  assert.equal(
    postItState({ ageMs: codeHit!.authored_ms_ago ?? null, sourceAgent: codeHit!.source_agent ?? null }),
    'unknown',
  );
});
