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
