/*
 * Card anatomy v2 — the GOLD/DEPTH field logic (HUMAN-LAYER-PRD §4A.3.1).
 * Fed by REAL captured envelopes: snapshot.compact.json (light nodes),
 * predict_calibration.json (a real calibration block), am_i_stale.json,
 * instance_self.json, north_warm.json.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import type { GraphSnapshot } from './snapshot';
import type { AmIStaleOutput } from '../api/toolTypes';
import {
  freshnessG1,
  calibrationG2,
  compoundingG3,
  alivenessG4,
  lastClaimD1,
  honestGapsD2,
  type CalibrationBlock,
} from './cardV2';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));

const snap = load<GraphSnapshot>('snapshot.compact.json');
const stale = load<AmIStaleOutput>('am_i_stale.json');
const predict = load<{ calibration: CalibrationBlock }>('predict_calibration.json');
const self = load<{ active_agent_sessions: number; queries_processed: number }>('instance_self.json');
const north = load<{ context?: { coverage?: { visited?: number; total?: number } }; honest_gaps?: string[] }>('north_warm.json');

// ── G1 — freshness vs git ─────────────────────────────────────────────────────
test('§4A.3.1 G1: am_i_stale with nothing changed → "everything I read is current"', () => {
  const g1 = freshnessG1(stale);
  assert.ok(g1);
  // The real fixture reported all-fresh (stale array empty).
  if ((stale.stale ?? []).length === 0) {
    assert.equal(g1!.allCurrent, true);
    assert.match(g1!.caption, /everything I read is current/);
  }
  // A synthetic changed case reads the count as a FLOOR-free plain fact.
  const changed = freshnessG1({ checked: 3, stale: [{ path: 'a.rs', reason: 'changed' }, { path: 'b.rs', reason: 'changed' }], fresh: [] });
  assert.equal(changed!.changed, 2);
  assert.match(changed!.caption, /2 files changed since I read them/);
  assert.equal(freshnessG1(null), null, 'not-yet-checked renders nothing (no fake)');
});

// ── G2 — calibration ──────────────────────────────────────────────────────────
test('§4A.3.1 G2: a calibrated block → "measured here ✓" + the exact receipt row', () => {
  const g2 = calibrationG2(predict.calibration);
  assert.ok(g2);
  assert.equal(g2!.measured, predict.calibration.calibrated);
  if (predict.calibration.calibrated) {
    assert.match(g2!.caption, /measured here ✓/);
    assert.ok(g2!.receipt, 'the receipt row is present when measured');
    assert.match(g2!.receipt!, /τ /, 'the receipt shows tau');
    assert.match(g2!.receipt!, new RegExp(`n ${predict.calibration.n?.toLocaleString()}`), 'exact n');
  }
});

test('§4A.3.1 G2: an uncalibrated block states the engine law VERBATIM (capped at reverify, act UNREACHABLE)', () => {
  const g2 = calibrationG2({ calibrated: false });
  assert.ok(g2);
  assert.equal(g2!.measured, false);
  assert.match(g2!.caption, /capped at reverify \(act UNREACHABLE\)/, 'the engine law is stated verbatim');
  assert.equal(g2!.receipt, null, 'no exact row when uncalibrated (nothing to show)');
});

// ── G3 — the compounding meter ────────────────────────────────────────────────
test('§4A.3.1 G3: the compounding meter counts DISTINCT memories + newest + aging from the snapshot', () => {
  const g3 = compoundingG3(snap);
  assert.ok(g3);
  assert.ok(g3!.count > 0, 'the compact fixture has memories');
  // The caption is one calm line (no chart, no gauge).
  assert.match(g3!.caption, /memor(y|ies)/);
  assert.doesNotMatch(g3!.caption, /%/, 'no percentage (anti-scope)');
  // Aging uses the 30-day rule: with now far in the future, everything ages.
  const farFuture = compoundingG3(snap, Date.now() + 400 * 24 * 60 * 60 * 1000);
  assert.ok(farFuture!.aging > 0, 'with now +400d, dated memories are aging');
});

// ── G4 — aliveness ────────────────────────────────────────────────────────────
test('§4A.3.1 G4: aliveness merges sessions + queries into ONE caption (not two rows)', () => {
  const g4 = alivenessG4(self);
  assert.ok(g4);
  assert.equal(g4!.sessions, self.active_agent_sessions);
  assert.equal(g4!.queries, self.queries_processed);
  assert.match(g4!.caption, new RegExp(`${self.active_agent_sessions} agent`));
  assert.match(g4!.caption, new RegExp(`${self.queries_processed} quer`));
  assert.equal(alivenessG4(null), null, 'absent → nothing (hosted brain honesty)');
});

// ── D1 — the last learned claim ───────────────────────────────────────────────
test('§4A.3.1 D1: the last claim is the newest L1GHT memory (label + agent + age, absent-honest)', () => {
  const d1 = lastClaimD1(snap);
  assert.ok(d1);
  assert.ok(d1!.claim.length > 0, 'a claim label is present');
  // Provenance is real or honestly absent (INV-04) — never faked.
  assert.ok(d1!.sourceAgent === null || typeof d1!.sourceAgent === 'string');
});

// ── D2 — honest gaps ──────────────────────────────────────────────────────────
test('§4A.3.1 D2: honest gaps render coverage + ghost edges + north gaps from real fields', () => {
  const d2 = honestGapsD2({
    coverage: north.context?.coverage,
    ghostEdges: 12,
    gaps: north.honest_gaps,
  });
  assert.ok(d2);
  assert.match(d2!.coverageLine, /files visited/);
  assert.equal(d2!.ghostEdges, 12);
  // Coverage numbers come straight from the real north envelope.
  if (north.context?.coverage?.total != null) {
    assert.match(d2!.coverageLine, new RegExp(north.context.coverage.total.toLocaleString()));
  }
  assert.equal(honestGapsD2({}), null, 'no coverage + no gaps → nothing (no fabricated zero)');
});
