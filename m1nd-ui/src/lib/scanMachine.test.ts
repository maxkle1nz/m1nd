/*
 * scanMachine — the scan gesture's loading state machine, proven transition by
 * transition (docs/uml/scan-loading.md). The teeth:
 *  - every LEGAL transition lands the declared phase with the declared fields;
 *  - the reducer is TOTAL: an event that does not apply returns the SAME state
 *    reference (late RESOLVED after abort, stray TICK after settle, double SCAN);
 *  - NEVER-DEAD: in-flight states always carry a live elapsed clock, and TICK
 *    alone promotes submitting → clustering (liveness even if SENT is missed);
 *  - the slow threshold promotes clustering → slow and never demotes;
 *  - HONEST ABORT: canceling lands idle + the "owner may still finish" note;
 *  - the display copy holds the copy law (no proven/done/correct) and never
 *    contains a percentage (no fabricated progress).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  canceledScanToast,
  formatElapsed,
  isScanInFlight,
  scanIdle,
  scanPhaseLabel,
  scanReducer,
  scanSlowNote,
  scanWaitCopy,
  SCAN_SLOW_AFTER_MS,
  type ScanMachineEvent,
  type ScanMachineState,
  type ScanPhaseName,
} from './scanMachine';
import type { WriteToast } from './buildMap';

const T0 = 1_000_000; // an arbitrary wall-clock origin
const okToast: WriteToast = { kind: 'ok', text: 'proposed 12 blocks from 300 files · 4 unmapped · store v3' };
const errToast: WriteToast = { kind: 'error', text: 'clustering exploded' };
const roToast: WriteToast = { kind: 'readonly', text: 'this owner is read-only — scan from a writable session' };

/** Drive the machine through a list of events (default threshold). */
function run(events: ScanMachineEvent[], from: ScanMachineState = scanIdle()): ScanMachineState {
  return events.reduce((s, e) => scanReducer(s, e), from);
}

// ── the happy path ────────────────────────────────────────────────────────────

test('idle + SCAN → submitting, clock armed, previous toast cleared', () => {
  const dirty: ScanMachineState = { ...scanIdle(), toast: errToast };
  const s = scanReducer(dirty, { type: 'SCAN', at: T0 });
  assert.equal(s.phase, 'submitting');
  assert.equal(s.startedAt, T0);
  assert.equal(s.elapsedMs, 0);
  assert.equal(s.toast, null);
});

test('submitting + SENT → clustering (the POST left)', () => {
  const s = run([{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 + 5 }]);
  assert.equal(s.phase, 'clustering');
  assert.equal(s.elapsedMs, 5);
});

test('clustering + TICK below the threshold stays clustering, clock advances', () => {
  const s = run([
    { type: 'SCAN', at: T0 },
    { type: 'SENT', at: T0 },
    { type: 'TICK', at: T0 + 3_000 },
  ]);
  assert.equal(s.phase, 'clustering');
  assert.equal(s.elapsedMs, 3_000);
});

test('TICK at/past the threshold promotes to slow — and slow never demotes', () => {
  const atThreshold = run([
    { type: 'SCAN', at: T0 },
    { type: 'SENT', at: T0 },
    { type: 'TICK', at: T0 + SCAN_SLOW_AFTER_MS },
  ]);
  assert.equal(atThreshold.phase, 'slow');
  const later = scanReducer(atThreshold, { type: 'TICK', at: T0 + SCAN_SLOW_AFTER_MS + 1_000 });
  assert.equal(later.phase, 'slow');
  assert.equal(later.elapsedMs, SCAN_SLOW_AFTER_MS + 1_000);
});

test('the threshold is injectable (a test/e2e seam), default = SCAN_SLOW_AFTER_MS', () => {
  const s0 = run([{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 }]);
  const fast = scanReducer(s0, { type: 'TICK', at: T0 + 2_000 }, 1_500);
  assert.equal(fast.phase, 'slow');
  const def = scanReducer(s0, { type: 'TICK', at: T0 + 2_000 });
  assert.equal(def.phase, 'clustering');
});

test('RESOLVED with reloading (ok/conflict) → candidate_ready, toast kept, clock frozen', () => {
  const s = run([
    { type: 'SCAN', at: T0 },
    { type: 'SENT', at: T0 },
    { type: 'TICK', at: T0 + 4_000 },
    { type: 'RESOLVED', at: T0 + 4_800, toast: okToast, reloading: true },
  ]);
  assert.equal(s.phase, 'candidate_ready');
  assert.equal(s.toast, okToast);
  assert.equal(s.elapsedMs, 4_800);
});

test('RESOLVED without reloading (readonly/error) → error, retry stays open', () => {
  const s = run([
    { type: 'SCAN', at: T0 },
    { type: 'SENT', at: T0 },
    { type: 'RESOLVED', at: T0 + 900, toast: roToast, reloading: false },
  ]);
  assert.equal(s.phase, 'error');
  assert.equal(s.toast, roToast);
  // the retry IS the scan gesture — legal again from error:
  const retried = scanReducer(s, { type: 'SCAN', at: T0 + 2_000 });
  assert.equal(retried.phase, 'submitting');
  assert.equal(retried.toast, null);
});

test('a re-scan is legal from candidate_ready (the store can be re-proposed)', () => {
  const ready = run([
    { type: 'SCAN', at: T0 },
    { type: 'SENT', at: T0 },
    { type: 'RESOLVED', at: T0 + 100, toast: okToast, reloading: true },
  ]);
  const again = scanReducer(ready, { type: 'SCAN', at: T0 + 5_000 });
  assert.equal(again.phase, 'submitting');
});

// ── the abort (HONEST ABORT law) ──────────────────────────────────────────────

test('ABORTED from any in-flight phase → idle + the honest canceled note', () => {
  for (const pre of [
    [{ type: 'SCAN', at: T0 }] as ScanMachineEvent[],
    [{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 }] as ScanMachineEvent[],
    [
      { type: 'SCAN', at: T0 },
      { type: 'SENT', at: T0 },
      { type: 'TICK', at: T0 + SCAN_SLOW_AFTER_MS },
    ] as ScanMachineEvent[],
  ]) {
    const s = run([...pre, { type: 'ABORTED', at: T0 + 11_000 }]);
    assert.equal(s.phase, 'idle');
    assert.equal(s.toast?.kind, 'canceled');
    assert.match(s.toast?.text ?? '', /owner may still finish/, 'the abort copy is honest: the owner is not stopped');
  }
});

test('a LATE RESOLVED after the abort is a provable no-op (total reducer)', () => {
  const aborted = run([
    { type: 'SCAN', at: T0 },
    { type: 'SENT', at: T0 },
    { type: 'ABORTED', at: T0 + 2_000 },
  ]);
  const late = scanReducer(aborted, { type: 'RESOLVED', at: T0 + 9_000, toast: okToast, reloading: true });
  assert.equal(late, aborted, 'same reference — the settle after an abort changes nothing');
});

// ── settle gestures ───────────────────────────────────────────────────────────

test('DISMISS_TOAST clears the note and settles to idle from error/candidate_ready', () => {
  const err = run([
    { type: 'SCAN', at: T0 },
    { type: 'SENT', at: T0 },
    { type: 'RESOLVED', at: T0 + 100, toast: errToast, reloading: false },
  ]);
  const cleared = scanReducer(err, { type: 'DISMISS_TOAST' });
  assert.equal(cleared.phase, 'idle');
  assert.equal(cleared.toast, null);
});

test('RESET returns to pristine idle (the reload landed the candidate dress)', () => {
  const ready = run([
    { type: 'SCAN', at: T0 },
    { type: 'SENT', at: T0 },
    { type: 'RESOLVED', at: T0 + 100, toast: okToast, reloading: true },
  ]);
  assert.deepEqual(scanReducer(ready, { type: 'RESET' }), scanIdle());
});

// ── totality + liveness laws ──────────────────────────────────────────────────

test('TOTAL: every event in every phase never throws; inapplicable pairs are no-ops', () => {
  const phases: ScanMachineState[] = [
    scanIdle(),
    run([{ type: 'SCAN', at: T0 }]),
    run([{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 }]),
    run([{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 }, { type: 'TICK', at: T0 + SCAN_SLOW_AFTER_MS }]),
    run([{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 }, { type: 'RESOLVED', at: T0, toast: okToast, reloading: true }]),
    run([{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 }, { type: 'RESOLVED', at: T0, toast: errToast, reloading: false }]),
  ];
  const events: ScanMachineEvent[] = [
    { type: 'SCAN', at: T0 + 1 },
    { type: 'SENT', at: T0 + 1 },
    { type: 'TICK', at: T0 + 1 },
    { type: 'RESOLVED', at: T0 + 1, toast: okToast, reloading: true },
    { type: 'ABORTED', at: T0 + 1 },
    { type: 'DISMISS_TOAST' },
    { type: 'RESET' },
  ];
  for (const s of phases) {
    for (const e of events) {
      const out = scanReducer(s, e); // must not throw
      assert.ok(out != null);
    }
  }
  // the named no-ops return the SAME reference:
  const idle = scanIdle();
  assert.equal(scanReducer(idle, { type: 'SENT', at: T0 }), idle);
  assert.equal(scanReducer(idle, { type: 'TICK', at: T0 }), idle);
  assert.equal(scanReducer(idle, { type: 'ABORTED', at: T0 }), idle);
  const flight = run([{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 }]);
  assert.equal(scanReducer(flight, { type: 'SCAN', at: T0 + 1 }), flight, 'double SCAN mid-flight is refused');
});

test('NEVER-DEAD: TICK alone promotes submitting → clustering (liveness without SENT)', () => {
  const s = run([{ type: 'SCAN', at: T0 }, { type: 'TICK', at: T0 + 1_000 }]);
  assert.equal(s.phase, 'clustering');
  assert.equal(s.elapsedMs, 1_000);
});

test('a backwards clock floors elapsed at 0 (never a negative display)', () => {
  const s = run([{ type: 'SCAN', at: T0 }, { type: 'SENT', at: T0 }, { type: 'TICK', at: T0 - 5_000 }]);
  assert.equal(s.elapsedMs, 0);
});

test('isScanInFlight names exactly the three held phases', () => {
  const inFlight: ScanPhaseName[] = ['submitting', 'clustering', 'slow'];
  const settled: ScanPhaseName[] = ['idle', 'candidate_ready', 'error'];
  for (const p of inFlight) assert.equal(isScanInFlight(p), true, p);
  for (const p of settled) assert.equal(isScanInFlight(p), false, p);
});

// ── display derivations ───────────────────────────────────────────────────────

test('formatElapsed renders a calm mm:ss clock', () => {
  assert.equal(formatElapsed(0), '0:00');
  assert.equal(formatElapsed(7_400), '0:07');
  assert.equal(formatElapsed(112_000), '1:52');
  assert.equal(formatElapsed(723_999), '12:03');
  assert.equal(formatElapsed(-50), '0:00');
});

test('the wait copy carries the REAL node count when present, generic when absent', () => {
  assert.equal(scanWaitCopy(3210), 'clustering 3,210 nodes into a candidate map');
  assert.equal(scanWaitCopy(null), 'clustering this repo into a candidate map');
  assert.match(scanSlowNote(3210), /3,210 nodes usually takes a while/);
  assert.match(scanSlowNote(null), /a large graph usually takes a while/);
  assert.match(scanSlowNote(null), /request stays open/, 'the slow note says the wait is ALIVE');
});

test('COPY LAW + honesty: no proven/done/correct, and no percentage anywhere', () => {
  const copy = [
    scanPhaseLabel('submitting'),
    scanPhaseLabel('clustering'),
    scanPhaseLabel('slow'),
    scanWaitCopy(999),
    scanWaitCopy(null),
    scanSlowNote(999),
    scanSlowNote(null),
    canceledScanToast().text,
  ].join(' ');
  assert.doesNotMatch(copy, /\bproven\b/i);
  assert.doesNotMatch(copy, /\bdone\b/i);
  assert.doesNotMatch(copy, /\bcorrect\b/i);
  assert.doesNotMatch(copy, /%|\bpercent/i, 'no fabricated progress fraction — the owner emits none');
});
