/*
 * api client — the §4A.9 `?brain=` selector on every graph/tool door.
 *
 * The acceptance line: while a hosted brain is viewed, EVERY fetch URL carries its
 * selector; absent (the bound view) the URL is byte-compatible with pre-2H. We spy
 * on globalThis.fetch and read back the exact URLs the client builds (no network).
 */
import { test, mock, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { api } from './client';

const CHERRY = '/path/to/project-b';
const ENCODED = encodeURIComponent(CHERRY);

/** Install a fetch spy that records URLs and returns a given JSON body. */
function spyFetch(body: unknown) {
  const urls: string[] = [];
  const fake = mock.fn(async (url: string) => {
    urls.push(url);
    return {
      ok: true,
      status: 200,
      statusText: 'OK',
      json: async () => body,
    } as unknown as Response;
  });
  globalThis.fetch = fake as unknown as typeof fetch;
  return urls;
}

afterEach(() => {
  mock.restoreAll();
});

// ── The bound view: no selector, byte-compatible ──────────────────────────────

test('§4A.9: absent brain leaves graph/tool URLs untouched (byte-compatible)', async () => {
  const urls = spyFetch({ result: {}, node_count: 0, edge_count: 0, nodes: [], edges: [] });
  await api.graphSnapshot();
  await api.graphStats();
  await api.subgraph('q', 10, 2);
  await api.tool('trust', { scope: 'all' });
  for (const u of urls) {
    assert.doesNotMatch(u, /[?&]brain=/, `no selector on a bound call: ${u}`);
  }
});

// ── The hosted view: the selector rides EVERY door ────────────────────────────

test('§4A.9: graphSnapshot carries ?brain=<encoded root>', async () => {
  const urls = spyFetch({ nodes: [], edges: [], version: 1 });
  await api.graphSnapshot(CHERRY);
  assert.equal(urls.length, 1);
  assert.match(urls[0], new RegExp(`/api/graph/snapshot\\?brain=${ENCODED.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`));
});

test('§4A.9: graphStats carries ?brain=', async () => {
  const urls = spyFetch({ node_count: 1, edge_count: 1 });
  await api.graphStats(CHERRY);
  assert.match(urls[0], /\/api\/graph\/stats\?brain=/);
  assert.ok(urls[0].includes(ENCODED), 'the root is URL-encoded');
});

test('§4A.9: subgraph JOINS the selector with & (it already has a query string)', async () => {
  const urls = spyFetch({ nodes: [], edges: [], meta: {} });
  await api.subgraph('some query', 30, 2, CHERRY);
  // The existing ?query=…&top_k=…&depth=… must gain &brain=, never a second ?.
  assert.match(urls[0], /\?query=/);
  assert.match(urls[0], /&brain=/);
  assert.equal((urls[0].match(/\?/g) ?? []).length, 1, 'exactly one ? in the URL');
});

test('§4A.9: a bare tool call carries ?brain= on the POST route', async () => {
  const urls = spyFetch({ result: {} });
  await api.tool('seek', { query: 'x', top_k: 20 }, CHERRY);
  assert.match(urls[0], /\/api\/tools\/seek\?brain=/);
  assert.ok(urls[0].includes(ENCODED));
});

test('§4A.9: an empty/whitespace brain is treated as absent (bound)', async () => {
  const urls = spyFetch({ result: {} });
  await api.tool('trust', {}, '   ');
  assert.doesNotMatch(urls[0], /[?&]brain=/, 'a blank selector is the bound graph');
});

// ── F3b: the reconcile write verb ─────────────────────────────────────────────

/** Like spyFetch, but also records the POST bodies so we can assert the OCC key. */
function spyFetchBodies(body: unknown) {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fake = mock.fn(async (url: string, init?: RequestInit) => {
    calls.push({ url, init });
    return { ok: true, status: 200, statusText: 'OK', json: async () => body } as unknown as Response;
  });
  globalThis.fetch = fake as unknown as typeof fetch;
  return calls;
}

test('systemBlocksReconcile POSTs the bare tool route with agent_id + the OCC key, unwrapping {result}', async () => {
  const report = { dirty: true, blocks: [], unmapped_total: 5, unmapped_materialized: 5, store_version: 8, file_count: 100 };
  const calls = spyFetchBodies({ result: report });
  const got = await api.systemBlocksReconcile(7);
  assert.equal(calls.length, 1);
  assert.match(calls[0].url, /\/api\/tools\/system_blocks_reconcile$/, 'the bare tool route');
  assert.equal(calls[0].init?.method, 'POST');
  const sent = JSON.parse(String(calls[0].init?.body));
  assert.equal(sent.agent_id, 'gui', 'the GUI agent id');
  assert.equal(sent.expected_store_version, 7, 'the OCC key it read from the snapshot');
  assert.deepEqual(got, report, 'the {result} envelope is unwrapped to the report');
});

test('§4A.9: systemBlocksReconcile carries ?brain= when a hosted brain is viewed', async () => {
  const calls = spyFetchBodies({ result: {} });
  await api.systemBlocksReconcile(1, CHERRY);
  assert.match(calls[0].url, /\/api\/tools\/system_blocks_reconcile\?brain=/);
  assert.ok(calls[0].url.includes(ENCODED));
});
