/*
 * loadMissions — the tray read's routing + the honest degraded-owner branch, proven
 * DOM-free (the fetch heart extracted like loadBuildMap: a recorder `sinks` + a
 * mocked global.fetch, no React render). The teeth: a hosted brainRoot rides the
 * `?brain=` selector on `kind=mission`; a response WITH `missions` is `ready`; a
 * pre-F2.5a owner (a field-report body with NO `missions`) is `unsupported`, never a
 * fake-empty tray; a failed read is `error`.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadMissions, type MissionsSinks } from './useMissions';
import type { MissionHead, MissionsStatus } from './useMissions';
import type { MissionsResponse } from '../lib/missions';

const HEAD: MissionHead = {
  mission_id: 'msn_00000000a001',
  head_letter_id: 'aaaa11110002',
  head: {
    schema: 'm1nd-mission-letter-v0',
    mission_id: 'msn_00000000a001',
    mission_seq: 1,
    block_id: 'sb_m1nd_mailbox',
    brain_ref: 'm1nd',
    seat: 'hand',
    capability: 'build-runner',
    phase: 'executing',
    tokens_total: 0,
    started_at: '2026-07-09T10:00:00Z',
    updated_at: '2026-07-09T10:00:00Z',
  },
  superseded_count: 0,
};

function recorder(body: string, status = 200) {
  const calls: string[] = [];
  const statuses: MissionsStatus[] = [];
  let missions: MissionHead[] | null = null;
  const sinks: MissionsSinks = {
    isMounted: () => true,
    setMissions: (m) => {
      missions = m;
    },
    setStatus: (s) => statuses.push(s),
    setError: () => {},
  };
  const fetchMock = (async (input: RequestInfo | URL) => {
    calls.push(String(input));
    return new Response(body, { status, headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch;
  return { calls, statuses, sinks, fetchMock, getMissions: () => missions };
}

async function withFetch(fetchMock: typeof fetch, fn: () => Promise<void>) {
  const real = globalThis.fetch;
  globalThis.fetch = fetchMock;
  try {
    await fn();
  } finally {
    globalThis.fetch = real;
  }
}

test('a hosted brainRoot rides the ?brain= selector on kind=mission', async () => {
  const body: MissionsResponse = { served_brain: { display_name: 'm1nd' }, missions: [HEAD] };
  const { calls, statuses, sinks, fetchMock, getMissions } = recorder(JSON.stringify(body));
  await withFetch(fetchMock, () => loadMissions('/path/to/m1nd', sinks));
  assert.equal(calls.length, 1);
  assert.ok(calls[0].includes('/api/mailbox?kind=mission'), calls[0]);
  assert.ok(calls[0].includes(`brain=${encodeURIComponent('/path/to/m1nd')}`), `must carry ?brain= — ${calls[0]}`);
  assert.deepEqual(statuses, ['ready']);
  assert.equal(getMissions()?.length, 1);
});

test('a null brainRoot leaves the URL untouched — the bound box', async () => {
  const body: MissionsResponse = { missions: [] };
  const { calls, sinks, fetchMock } = recorder(JSON.stringify(body));
  await withFetch(fetchMock, () => loadMissions(null, sinks));
  assert.ok(!calls[0].includes('brain='), `bound read carries no selector — ${calls[0]}`);
});

test('a pre-F2.5a owner (no `missions` key) degrades to unsupported, never a fake-empty tray', async () => {
  // The old owner ignores kind=mission and returns the field-report caixinha.
  const legacy = JSON.stringify({ served_brain: { display_name: 'm1nd' }, letters: [], counts: {} });
  const { statuses, sinks, fetchMock, getMissions } = recorder(legacy);
  await withFetch(fetchMock, () => loadMissions(null, sinks));
  assert.deepEqual(statuses, ['unsupported']);
  assert.equal(getMissions(), null, 'no missions set — the tray shows the honest "needs an updated owner"');
});

test('a failed read is `error`, never a silent empty', async () => {
  const { statuses, sinks, fetchMock } = recorder(JSON.stringify({ error: 'not_found' }), 404);
  await withFetch(fetchMock, () => loadMissions(null, sinks));
  assert.deepEqual(statuses, ['error']);
});
