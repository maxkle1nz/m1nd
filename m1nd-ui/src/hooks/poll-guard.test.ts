/*
 * pollGuard — the anti-stacking guard the status pollers share, proven DOM-free.
 *
 * The teeth: while one poll is in flight, every further `begin()` returns null
 * (the tick is SKIPPED), so a stalled server can never make requests pile up
 * (the live incident: 19 pending health polls under SIGSTOP). `settle` frees the
 * slot only for the request that owned it; `abort` cancels the in-flight signal
 * and immediately re-arms the guard.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPollGuard } from './pollGuard';

test('a second begin() while one is in flight is skipped — polls never stack', () => {
  const guard = createPollGuard();

  const first = guard.begin();
  assert.ok(first, 'the first tick starts a poll');
  assert.equal(guard.inFlight, true);

  // Simulate a stalled server: the first poll never settles, and many more ticks
  // fire. Not one of them starts a request — the pending count stays at exactly 1.
  for (let i = 0; i < 19; i++) {
    assert.equal(guard.begin(), null, `tick ${i} must be skipped while a poll is in flight`);
  }
  assert.equal(first!.signal.aborted, false, 'skipping never touches the live request');
});

test('after the in-flight poll settles, the next tick starts a fresh poll', () => {
  const guard = createPollGuard();

  const first = guard.begin();
  assert.ok(first);
  guard.settle(first!);
  assert.equal(guard.inFlight, false, 'settling the current request frees the slot');

  const second = guard.begin();
  assert.ok(second, 'the next tick runs once the slot is free');
  assert.notEqual(second, first, 'a fresh controller, not the settled one');
});

test('abort() cancels the in-flight request and re-arms the guard', () => {
  const guard = createPollGuard();

  const first = guard.begin();
  assert.ok(first);
  guard.abort();
  assert.equal(first!.signal.aborted, true, 'teardown aborts the live request');
  assert.equal(guard.inFlight, false, 'the slot is freed for reuse');

  const second = guard.begin();
  assert.ok(second, 'the guard is immediately usable after abort');
});

test('a stale settle (from an aborted, superseded poll) never frees a newer poll', () => {
  const guard = createPollGuard();

  const first = guard.begin();
  assert.ok(first);
  guard.abort(); // first is superseded
  const second = guard.begin(); // a new poll takes the slot
  assert.ok(second);

  // The aborted first request's promise finally-settles late — it must NOT clear
  // the second poll's slot (identity check), or a stacked request could slip in.
  guard.settle(first!);
  assert.equal(guard.inFlight, true, 'the newer poll still owns the slot');
  assert.equal(guard.begin(), null, 'still no stacking after a stale settle');
});
