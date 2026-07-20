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

// ── F2.5d: the human landing's receipt_import write verb ──────────────────────

const RECEIPT = {
  type: 'test' as const,
  emitter: { kind: 'verb' as const, id: 'human-ui-landing' },
  scope: { block_id: 'sb_m1nd_mailbox', boundary_version: 1, contract_version: 1, resolution_hash: 'sha256:fp' },
  evidence: { artifact_hash: 'sha256:log', evidence_refs: ['artifact://run/2'] },
  validity: { expires_on: null, stales_on: ['boundary_change', 'member_change'] },
};

test('receiptImport POSTs the bare receipt_import route with agent_id + OCC key + the receipt, unwrapping {result}', async () => {
  const outcome = { store_version: 10, block_id: 'sb_m1nd_mailbox', receipt_count: 1 };
  const calls = spyFetchBodies({ result: outcome });
  const got = await api.receiptImport({ expectedStoreVersion: 9, blockId: 'sb_m1nd_mailbox', receipt: RECEIPT });
  assert.equal(calls.length, 1);
  assert.match(calls[0].url, /\/api\/tools\/receipt_import$/, 'the bare tool route');
  assert.equal(calls[0].init?.method, 'POST');
  const sent = JSON.parse(String(calls[0].init?.body));
  assert.equal(sent.agent_id, 'gui', 'the GUI agent id');
  assert.equal(sent.expected_store_version, 9, 'the OCC key from the fresh snapshot');
  assert.equal(sent.block_id, 'sb_m1nd_mailbox');
  assert.deepEqual(sent.receipt, RECEIPT, 'the assembled receipt rides verbatim');
  assert.equal(
    sent.imported_via,
    'human-ui',
    'the owner screen stamps the human-origin token the backend requires; an agent never does',
  );
  assert.deepEqual(got, outcome, 'the {result} envelope is unwrapped to the outcome');
});

test('§4A.9: receiptImport carries ?brain= when a hosted brain is viewed', async () => {
  const calls = spyFetchBodies({ result: {} });
  await api.receiptImport({ expectedStoreVersion: 1, blockId: 'sb_x', receipt: RECEIPT }, CHERRY);
  assert.match(calls[0].url, /\/api\/tools\/receipt_import\?brain=/);
  assert.ok(calls[0].url.includes(ENCODED));
});

// ── F-01: the UI expresses intent but does not mint ratification authority ────

test('systemBlocksRatify sends intent and OCC data without a forgeable authority token', async () => {
  const result = {
    store_version: 6,
    ratified_block_ids: ['sb_x'],
    skeleton_state: 'ratified',
    ratifier: 'gui',
    ratified_at: 't',
  };
  const calls = spyFetchBodies({ result });
  const got = await api.systemBlocksRatify({ expectedStoreVersion: 5, ratifier: 'gui' });
  assert.equal(calls.length, 1);
  assert.match(calls[0].url, /\/api\/tools\/system_blocks_ratify$/, 'the bare tool route');
  assert.equal(calls[0].init?.method, 'POST');
  const sent = JSON.parse(String(calls[0].init?.body));
  assert.equal(sent.agent_id, 'gui', 'the GUI agent id');
  assert.equal(sent.expected_store_version, 5, 'the OCC key it read from the snapshot');
  assert.equal(sent.ratifier, 'gui');
  assert.equal('ratified_via' in sent, false, 'client-authored strings are not authority');
  assert.equal('block_ids' in sent, false, 'a blanket ratify omits block_ids');
  assert.deepEqual(got, result, 'the {result} envelope is unwrapped');
});

test('§4A.9: systemBlocksRatify carries block_ids without inventing authority', async () => {
  const calls = spyFetchBodies({ result: {} });
  await api.systemBlocksRatify({ expectedStoreVersion: 2, ratifier: 'gui', blockIds: ['sb_a'] }, CHERRY);
  assert.match(calls[0].url, /\/api\/tools\/system_blocks_ratify\?brain=/);
  assert.ok(calls[0].url.includes(ENCODED));
  const sent = JSON.parse(String(calls[0].init?.body));
  assert.deepEqual(sent.block_ids, ['sb_a']);
  assert.equal('ratified_via' in sent, false, 'per-block intent is not an authority credential');
});

// ── M1ND-10 G1: read-only organism manifest ──────────────────────────────────

const MANIFEST_RESPONSE = {
  schema: 'm1nd-organism-manifest-response-v1',
  manifest: {
    schema: 'm1nd-organism-manifest-v1',
    organism_id: 'm1nd',
    repo_id: 'm1nd',
    brain_id: 'brain:test',
    project_root_fingerprint: 'sha256:root',
    source: { commit: 'abc123', dirty: false, version: '1.4.0' },
    runtime: {
      owner_id: 'owner:test',
      binary_version: '1.4.0',
      binary_sha256: 'sha256:binary',
      started_at: 1_752_844_000_000,
    },
    graph: {
      generation: 7,
      snapshot_sha256: 'sha256:graph',
      node_count: 10,
      edge_count: 20,
    },
    architecture: {
      store_version: 3,
      skeleton_digest: 'sha256:skeleton',
      ratification_state: 'ratified',
    },
    ui: {
      bundle_version: '0.1.0',
      bundle_sha256: 'sha256:ui',
      mode: 'embedded',
    },
    capabilities: { policy_version: 'UNAVAILABLE', enabled_effects: [] },
    autonomy: {
      supported_modes: ['HUMAN_GATED'],
      mechanically_proven_modes: [],
      active_mode: 'UNKNOWN',
      activation_receipt_id: '',
      constitution_digest: '',
      constitution_epoch: 0,
      safety_kernel_digest: '',
      autonomy_epoch: 0,
      grants_digest: '',
      quorum_policy_digest: '',
      max_effective_tier_projection: 'NONE',
      issuance_frozen: true,
      sentinel_safety_state: 'UNKNOWN',
    },
    schemas: {
      mission: 'm1nd-mission-letter-v0',
      receipt: 'm1nd-system-block-receipt-v0',
      checkpoint: 'UNAVAILABLE',
      light: 'm1nd-light-claim-v0',
      system_blocks: 'm1nd-system-block-store-v0',
    },
    authorities: {
      source: {
        revision: '1.4.0',
        digest: 'abc123',
        observed_at: 1_752_844_100_000,
        freshness: 'FRESH',
        status: 'AVAILABLE',
      },
      runtime_binary: {
        revision: '1.4.0',
        digest: 'sha256:binary',
        observed_at: 1_752_844_100_000,
        freshness: 'FRESH',
        status: 'AVAILABLE',
      },
      graph: {
        revision: '7',
        digest: 'sha256:graph',
        observed_at: 1_752_844_100_000,
        freshness: 'FRESH',
        status: 'AVAILABLE',
      },
      architecture: {
        revision: '3',
        digest: 'sha256:skeleton',
        observed_at: 1_752_844_100_000,
        freshness: 'FRESH',
        status: 'AVAILABLE',
      },
      ui_bundle: {
        revision: '0.1.0',
        digest: 'sha256:ui',
        observed_at: 1_752_844_100_000,
        freshness: 'FRESH',
        status: 'AVAILABLE',
      },
      release_candidate: {
        revision: '',
        digest: '',
        observed_at: 1_752_844_100_000,
        freshness: 'UNKNOWN',
        status: 'UNAVAILABLE',
      },
    },
    release_provenance: { release_candidate_digest: '', signature: '' },
    generated_at: 1_752_844_100_000,
    manifest_sha256: 'sha256:manifest',
  },
  verification: {
    coherence: 'DRIFT',
    computed_manifest_sha256: 'sha256:manifest',
    issues: [
      {
        kind: 'DRIFT',
        authority_id: null,
        detail: 'source/binary/bundle versions diverge: source=1.4.0, binary=1.4.0, bundle=0.1.0',
      },
    ],
  },
} as const;

test('manifest GET preserves source/binary/bundle drift and absent authority facts verbatim', async () => {
  const calls = spyFetchBodies(MANIFEST_RESPONSE);
  const got = await api.manifest();

  assert.equal(calls.length, 1);
  assert.match(calls[0].url, /\/api\/manifest$/);
  assert.equal(calls[0].init?.method, undefined, 'manifest is a read-only GET');
  assert.equal(calls[0].init?.cache, 'no-store', 'periodic truth reads cannot reuse a cached manifest');
  assert.equal(got, MANIFEST_RESPONSE, 'validated response is not copied or rewritten');
  assert.equal(got.manifest.source.version, '1.4.0');
  assert.equal(got.manifest.runtime.binary_version, '1.4.0');
  assert.equal(got.manifest.ui.bundle_version, '0.1.0');
  assert.equal(got.manifest.autonomy.active_mode, 'UNKNOWN');
  assert.equal(got.manifest.release_provenance.release_candidate_digest, '');
  assert.equal(got.verification.coherence, 'DRIFT');
});

test('manifest GET routes the selected brain root through the shared selector', async () => {
  const calls = spyFetchBodies(MANIFEST_RESPONSE);
  const brain = '/workspace/repo beta';
  const got = await api.manifest(brain);

  assert.equal(got, MANIFEST_RESPONSE);
  assert.match(calls[0].url, /\/api\/manifest\?brain=%2Fworkspace%2Frepo%20beta$/);
});

test('manifest parsing requires every core authority fact', async () => {
  const malformed = structuredClone(MANIFEST_RESPONSE);
  delete (malformed.manifest.authorities as { graph?: unknown }).graph;
  spyFetch(malformed);

  await assert.rejects(
    () => api.manifest(),
    /invalid organism manifest at \$\.manifest\.authorities\.graph: expected object/,
  );
});

test('manifest parsing rejects a verification digest that does not seal the manifest', async () => {
  const malformed = structuredClone(MANIFEST_RESPONSE);
  (
    malformed.verification as { computed_manifest_sha256: string }
  ).computed_manifest_sha256 = 'sha256:different';
  spyFetch(malformed);

  await assert.rejects(
    () => api.manifest(),
    /computed_manifest_sha256: expected value equal to \$\.manifest\.manifest_sha256/,
  );
});

test('manifest parsing fails closed instead of inventing missing autonomy facts', async () => {
  const malformed = structuredClone(MANIFEST_RESPONSE);
  delete (malformed.manifest.autonomy as { active_mode?: string }).active_mode;
  spyFetch(malformed);

  await assert.rejects(
    () => api.manifest(),
    /invalid organism manifest at \$\.manifest\.autonomy\.active_mode: expected string/,
  );
});

test('manifest parsing rejects a persuasive-looking response with the wrong schema', async () => {
  spyFetch({ ...MANIFEST_RESPONSE, schema: 'm1nd-organism-manifest-response-v0' });
  await assert.rejects(() => api.manifest(), /m1nd-organism-manifest-response-v1/);
});
