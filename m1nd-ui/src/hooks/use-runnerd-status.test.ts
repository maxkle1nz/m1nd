/*
 * loadRunnerdStatus — the compose panel's runner-daemon liveness read (F2.5c §5a),
 * proven DOM-free (the fetch heart extracted like loadMissions). The teeth: a well-
 * formed `{runners, count}` is `ready` (even when empty — the honest "no runner
 * connected"); a pre-F2.5c owner (NO `runners` key) is `unsupported`, never a fake
 * "no runners"; a failed read is `error`. The status read carries NO `?brain=` (the
 * registry is owner-process-global liveness, not per-brain).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadRunnerdStatus, type RunnerdStatusSinks, type RunnerdStatusState } from './useRunnerdStatus';
import type { LiveRunner } from '../lib/missions';

function recorder(body: string, status = 200) {
  const calls: string[] = [];
  const states: RunnerdStatusState[] = [];
  let runners: LiveRunner[] | null = null;
  const sinks: RunnerdStatusSinks = {
    isMounted: () => true,
    setRunners: (r) => {
      runners = r;
    },
    setState: (s) => states.push(s),
  };
  const fetchMock = (async (input: RequestInfo | URL) => {
    calls.push(String(input));
    return new Response(body, { status, headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch;
  return { calls, states, sinks, fetchMock, getRunners: () => runners };
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

test('a live status is ready and lists the runners; the read carries no ?brain=', async () => {
  const body = JSON.stringify({
    runners: [{ runner_id: 'build-1', port: 61999, last_seen_ms: 1 }],
    count: 1,
  });
  const { calls, states, sinks, fetchMock, getRunners } = recorder(body);
  await withFetch(fetchMock, () => loadRunnerdStatus(sinks));
  assert.equal(calls.length, 1);
  assert.ok(calls[0].includes('/api/runnerd/status'), calls[0]);
  assert.ok(!calls[0].includes('brain='), `liveness is owner-global, not per-brain — ${calls[0]}`);
  assert.deepEqual(states, ['ready']);
  assert.equal(getRunners()?.length, 1);
  assert.equal(getRunners()?.[0].runner_id, 'build-1');
});

test('an empty runners list is ready — the honest "no runner connected", not an error', async () => {
  const { states, sinks, fetchMock, getRunners } = recorder(JSON.stringify({ runners: [], count: 0 }));
  await withFetch(fetchMock, () => loadRunnerdStatus(sinks));
  assert.deepEqual(states, ['ready']);
  assert.equal(getRunners()?.length, 0);
});

test('a pre-F2.5c owner (no `runners` key) degrades to unsupported, never a fake-empty', async () => {
  const { states, sinks, fetchMock, getRunners } = recorder(JSON.stringify({ ok: true }));
  await withFetch(fetchMock, () => loadRunnerdStatus(sinks));
  assert.deepEqual(states, ['unsupported']);
  assert.equal(getRunners(), null, 'no runners set — spawn stays disabled with the honest note');
});

test('a failed read is `error`, never a silent empty', async () => {
  const { states, sinks, fetchMock } = recorder(JSON.stringify({ error: 'boom' }), 500);
  await withFetch(fetchMock, () => loadRunnerdStatus(sinks));
  assert.deepEqual(states, ['error']);
});
