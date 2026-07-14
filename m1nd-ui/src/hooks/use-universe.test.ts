/*
 * reduceUniversePoll — the honest universe classifier, proven DOM-free (the reducer is
 * extracted from the hook exactly so this file can exist). The teeth: a 404 degrades to
 * an empty-READY sky (pre-F30, byte-compatible); a real blip WITH a prior good read keeps
 * last-good + a discreet retry note; the FIRST real failure is an honest `error`, never a
 * silent empty sky; and a success clears the note.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  reduceUniversePoll,
  INITIAL_UNIVERSE_STATE,
  type UniversePollState,
} from './useUniverse';
import type { UniverseResponse } from '../lib/universe';

const world = (name: string) => ({
  key: name,
  root: `/repo/${name}`,
  name,
  awake: false,
  presences: [],
  pending: { stamps: 0, ratifies: 0 },
  letters: { merge_wait: 0, total: 0 },
});

const populated: UniverseResponse = {
  schema: 'm1nd-universe-v0',
  worlds: [world('alpha')],
  owner: { alerts_pending: 2 },
  totals: { worlds: 1, awake: 0, pending: 2 },
};

test('a successful poll → READY on the body, note cleared, hadGood set', () => {
  const next = reduceUniversePoll(INITIAL_UNIVERSE_STATE, { ok: true, data: populated });
  assert.equal(next.status, 'ready');
  assert.equal(next.universe.worlds.length, 1);
  assert.equal(next.note, null);
  assert.equal(next.hadGood, true);
});

test('a partial/legacy body is normalized, never an undefined field into the render', () => {
  const partial = { schema: 'm1nd-universe-v0' } as unknown as UniverseResponse;
  const next = reduceUniversePoll(INITIAL_UNIVERSE_STATE, { ok: true, data: partial });
  assert.deepEqual(next.universe.worlds, []);
  assert.deepEqual(next.universe.owner, { alerts_pending: 0 });
  assert.deepEqual(next.universe.totals, { worlds: 0, awake: 0, pending: 0 });
});

test('a 404 degrades to an empty-READY sky (pre-F30) — the entry rule falls through', () => {
  const next = reduceUniversePoll(INITIAL_UNIVERSE_STATE, { ok: false, is404: true });
  assert.equal(next.status, 'ready');
  assert.deepEqual(next.universe.worlds, []);
  assert.equal(next.note, null);
  assert.equal(next.hadGood, true, 'a 404 is a settled answer');
});

test('the FIRST real (non-404) failure is an honest error, never a silent empty sky', () => {
  const next = reduceUniversePoll(INITIAL_UNIVERSE_STATE, { ok: false, is404: false });
  assert.equal(next.status, 'error', 'error, not a fabricated ready-empty');
  assert.equal(next.note, null);
  assert.equal(next.hadGood, false);
});

test('a real blip AFTER a good read keeps LAST-GOOD + flies the discreet retry note', () => {
  const good = reduceUniversePoll(INITIAL_UNIVERSE_STATE, { ok: true, data: populated });
  const blip = reduceUniversePoll(good, { ok: false, is404: false });
  assert.equal(blip.status, 'ready', 'the populated sky stays lit');
  assert.equal(blip.universe.worlds.length, 1, 'last-good worlds are kept');
  assert.equal(blip.note, 'read failed — retrying');
});

test('a blip after a 404-degrade keeps the empty sky + the retry note (hadGood via 404)', () => {
  const degraded = reduceUniversePoll(INITIAL_UNIVERSE_STATE, { ok: false, is404: true });
  const blip = reduceUniversePoll(degraded, { ok: false, is404: false });
  assert.equal(blip.status, 'ready');
  assert.equal(blip.note, 'read failed — retrying');
});

test('recovery: a success after an error/blip clears the note and lands READY', () => {
  const errored: UniversePollState = { status: 'error', universe: INITIAL_UNIVERSE_STATE.universe, note: null, hadGood: false };
  const recovered = reduceUniversePoll(errored, { ok: true, data: populated });
  assert.equal(recovered.status, 'ready');
  assert.equal(recovered.note, null);
  assert.equal(recovered.universe.worlds.length, 1);
});
