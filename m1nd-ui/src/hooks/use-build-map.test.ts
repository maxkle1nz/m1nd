/*
 * loadBuildMap — the §4A.9 brain routing, proven DOM-free (the fetch heart is
 * extracted from the hook exactly so this file can exist: a recorder `sinks`
 * object + a mocked global.fetch, no React render needed — the repo's
 * client.test.ts pattern).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadBuildMap, nextReadStatus, type BuildMapSinks } from './useBuildMap';

const SNAPSHOT_BODY = JSON.stringify({
  result: {
    present: true,
    store_version: 1,
    block_count: 0,
    store: { schema: 'm1nd-system-block-store-v0', store_version: 1, skeleton: {}, blocks: [] },
  },
});
const GRAPH_BODY = JSON.stringify({ nodes: [], edges: [] });

function recorder() {
  const calls: string[] = [];
  const statuses: string[] = [];
  const sinks: BuildMapSinks = {
    isMounted: () => true,
    setSnapshot: () => {},
    setMemberStates: () => {},
    setStatus: (s) => statuses.push(s),
    setError: () => {},
  };
  const fetchMock = (async (input: RequestInfo | URL) => {
    const url = String(input);
    calls.push(url);
    const body = url.includes('/api/graph/snapshot') ? GRAPH_BODY : SNAPSHOT_BODY;
    return new Response(body, { status: 200, headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch;
  return { calls, statuses, sinks, fetchMock };
}

test('a hosted brainRoot rides BOTH reads as the ?brain= selector (§4A.9)', async () => {
  const { calls, statuses, sinks, fetchMock } = recorder();
  const real = globalThis.fetch;
  globalThis.fetch = fetchMock;
  try {
    await loadBuildMap('/path/to/project-b', sinks);
  } finally {
    globalThis.fetch = real;
  }
  assert.equal(calls.length, 2, 'snapshot + graph');
  const sel = encodeURIComponent('/path/to/project-b');
  assert.ok(calls[0].includes('/api/tools/system_blocks_snapshot'), calls[0]);
  assert.ok(calls[0].includes(`brain=${sel}`), `snapshot must carry ?brain= — got ${calls[0]}`);
  assert.ok(calls[1].includes('/api/graph/snapshot'), calls[1]);
  assert.ok(calls[1].includes(`brain=${sel}`), `graph must carry ?brain= — got ${calls[1]}`);
  assert.deepEqual(statuses, ['ready']);
});

test('a null brainRoot leaves both URLs untouched — the bound brain, byte-compatible with F1', async () => {
  const { calls, sinks, fetchMock } = recorder();
  const real = globalThis.fetch;
  globalThis.fetch = fetchMock;
  try {
    await loadBuildMap(null, sinks);
  } finally {
    globalThis.fetch = real;
  }
  assert.equal(calls.length, 2);
  for (const url of calls) {
    assert.ok(!url.includes('brain='), `bound read must carry no selector — got ${url}`);
  }
});

test('an unmounted sink stops the write-through after the fetch resolves', async () => {
  const { sinks, fetchMock } = recorder();
  let statusWrites = 0;
  sinks.isMounted = () => false;
  sinks.setStatus = () => {
    statusWrites += 1;
  };
  const real = globalThis.fetch;
  globalThis.fetch = fetchMock;
  try {
    await loadBuildMap(null, sinks);
  } finally {
    globalThis.fetch = real;
  }
  assert.equal(statusWrites, 0, 'nothing lands after unmount');
});

// ── stale-while-revalidate: a reload never blanks the map ─────────────────────

test('nextReadStatus: a same-brain reload with last-good stays MOUNTED (refreshing)', () => {
  assert.equal(nextReadStatus(true, false), 'refreshing', 'the human keeps their selection/scroll');
});

test('nextReadStatus: a cold start (no last-good) honestly loads', () => {
  assert.equal(nextReadStatus(false, false), 'loading');
});

test('nextReadStatus: a brain switch loads even with last-good (it was another brain)', () => {
  assert.equal(nextReadStatus(true, true), 'loading', 'the old brain map is not shown as the new one');
});

test('nextReadStatus: a cold start on a brain switch loads', () => {
  assert.equal(nextReadStatus(false, true), 'loading');
});
