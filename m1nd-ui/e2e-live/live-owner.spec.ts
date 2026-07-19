/*
 * Real-owner G7 LIVE proof. There are intentionally no route interceptors,
 * HARs, fixtures, web servers, mutations, or localhost fallbacks in this lane.
 */
import { strict as assert } from 'node:assert';
import { expect, type Request, type Response } from '@playwright/test';
import { parseOrganismManifestResponse } from '../src/api/client';
import type { OrganismManifestResponseV1 } from '../src/api/types';
import { test } from './live-test';

interface LiveMetadata {
  readonly gate: 'G7_LIVE_REAL_BROWSER';
  readonly ownerOrigin: string;
  readonly ownerPort: number;
  readonly expectedUiBundleSha256: string;
}

interface ProbeResult {
  readonly status: number;
  readonly contentType: string;
  readonly body: unknown;
}

interface BrowserProbeResults {
  readonly health: ProbeResult;
  readonly manifest: ProbeResult;
  readonly stats: ProbeResult;
}

const READ_ONLY_POST_PATHS = new Set([
  '/api/tools/system_blocks_snapshot',
  '/api/tools/trust',
  '/api/tools/tremor',
]);

function record(value: unknown, label: string): Record<string, unknown> {
  if (value == null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`G7 LIVE NOT_PROVEN: ${label} is not an object`);
  }
  return value as Record<string, unknown>;
}

function refuse(detail: string): never {
  throw new Error(`G7 LIVE NOT_PROVEN: ${detail}`);
}

function assertJson200(probe: ProbeResult, endpoint: string): void {
  if (probe.status !== 200) refuse(`${endpoint} returned HTTP ${probe.status}`);
  if (!probe.contentType.toLowerCase().includes('application/json')) {
    refuse(`${endpoint} did not return application/json`);
  }
}

function assertAvailableFresh(
  manifest: OrganismManifestResponseV1,
  authorityId: 'source' | 'runtime_binary' | 'ui_bundle',
): void {
  const authority = manifest.manifest.authorities[authorityId];
  if (authority.status !== 'AVAILABLE' || authority.freshness !== 'FRESH') {
    refuse(
      `${authorityId} authority is ${authority.status}/${authority.freshness}, expected AVAILABLE/FRESH`,
    );
  }
}

async function assertOwnerSocket(response: Response, ownerOrigin: string, ownerPort: number) {
  const url = new URL(response.url());
  assert.equal(url.origin, ownerOrigin, `${url.pathname} escaped the configured owner origin`);
  assert.equal(response.fromServiceWorker(), false, `${url.pathname} came from a service worker`);
  const address = await response.serverAddr();
  assert.ok(address, `${url.pathname} has no real server address`);
  const expectedAddress = new URL(ownerOrigin).hostname.replace(/^\[|\]$/g, '');
  const actualAddress = address.ipAddress.replace(/^\[|\]$/g, '');
  assert.equal(actualAddress, expectedAddress, `${url.pathname} reached the wrong server address`);
  assert.equal(address.port, ownerPort, `${url.pathname} reached the wrong server port`);
}

test('G7 LIVE: real owner shell, coherent manifest, exact UI attestation, and safe read', async ({
  page,
}) => {
  const metadata = test.info().config.metadata as unknown as LiveMetadata;
  assert.equal(metadata.gate, 'G7_LIVE_REAL_BROWSER');

  const observedRequests: Request[] = [];
  const observedStaticResponses: Response[] = [];
  page.on('request', (request) => {
    observedRequests.push(request);
  });
  page.on('response', (response) => {
    if (['document', 'script', 'stylesheet'].includes(response.request().resourceType())) {
      observedStaticResponses.push(response);
    }
  });

  const documentResponse = await page.goto('/', { waitUntil: 'domcontentloaded' });
  assert.ok(documentResponse, 'the owner returned no document response');
  assert.equal(documentResponse.status(), 200, 'the real owner shell did not return HTTP 200');
  assert.match(
    documentResponse.headers()['content-type'] ?? '',
    /text\/html/i,
    'the owner root did not serve an HTML shell',
  );
  await assertOwnerSocket(documentResponse, metadata.ownerOrigin, metadata.ownerPort);
  await page.waitForLoadState('load');

  const manifestStatus = page.locator('[data-role="manifest-status"]');
  await expect(manifestStatus).toBeVisible();
  await expect(manifestStatus).toHaveAttribute('data-manifest-state', 'ready');

  const staticResourceTypes = new Set(
    observedStaticResponses.map((response) => response.request().resourceType()),
  );
  for (const expectedType of ['document', 'script', 'stylesheet']) {
    assert.ok(staticResourceTypes.has(expectedType), `the shell loaded no ${expectedType} response`);
  }
  for (const response of observedStaticResponses) {
    assert.ok(response.status() < 400, `${response.url()} returned HTTP ${response.status()}`);
    await assertOwnerSocket(response, metadata.ownerOrigin, metadata.ownerPort);
  }
  assert.equal(page.context().serviceWorkers().length, 0, 'the context observed a service worker');
  const serviceWorkerState = await page.evaluate(async () => ({
    controlled: navigator.serviceWorker?.controller != null,
    registrations: navigator.serviceWorker
      ? (await navigator.serviceWorker.getRegistrations()).length
      : 0,
  }));
  assert.deepEqual(
    serviceWorkerState,
    { controlled: false, registrations: 0 },
    'the shell was controlled by or registered a service worker',
  );

  const probeId = `g7-${Date.now().toString(36)}`;
  const paths = ['/api/health', '/api/manifest', '/api/graph/stats'] as const;
  const responseWaits = paths.map((path) =>
    page.waitForResponse((response) => {
      const url = new URL(response.url());
      return url.pathname === path && url.searchParams.get('g7_live_probe') === probeId;
    }),
  );

  const browserProbe = await page.evaluate(async (id): Promise<BrowserProbeResults> => {
    const read = async (path: string): Promise<ProbeResult> => {
      const response = await fetch(`${path}?g7_live_probe=${encodeURIComponent(id)}`, {
        method: 'GET',
        cache: 'no-store',
        headers: { Accept: 'application/json' },
      });
      let body: unknown;
      try {
        body = await response.json();
      } catch {
        body = null;
      }
      return {
        status: response.status,
        contentType: response.headers.get('content-type') ?? '',
        body,
      };
    };
    return {
      health: await read('/api/health'),
      manifest: await read('/api/manifest'),
      stats: await read('/api/graph/stats'),
    };
  }, probeId);

  const probeResponses = await Promise.all(responseWaits);
  for (const response of probeResponses) {
    assert.equal(response.request().method(), 'GET', 'a G7 probe was not read-only');
    await assertOwnerSocket(response, metadata.ownerOrigin, metadata.ownerPort);
  }

  assertJson200(browserProbe.manifest, '/api/manifest');
  let manifest: OrganismManifestResponseV1;
  try {
    manifest = parseOrganismManifestResponse(browserProbe.manifest.body);
  } catch (error) {
    const detail = error instanceof Error ? error.message : 'unknown manifest parse failure';
    return refuse(detail);
  }

  const { source, runtime, ui, authorities } = manifest.manifest;
  const coherence = manifest.verification.coherence;
  if (coherence !== 'COHERENT') {
    const issueKinds = manifest.verification.issues
      .map((issue) => `${issue.kind}:${issue.authority_id ?? 'manifest'}`)
      .join(',');
    return refuse(`manifest coherence=${coherence}; issues=${issueKinds || 'none'}`);
  }

  if (source.version !== runtime.binary_version || source.version !== ui.bundle_version) {
    return refuse(
      `source/binary/bundle versions diverge (${source.version}/${runtime.binary_version}/${ui.bundle_version})`,
    );
  }
  assertAvailableFresh(manifest, 'source');
  assertAvailableFresh(manifest, 'runtime_binary');
  assertAvailableFresh(manifest, 'ui_bundle');
  if (authorities.source.revision !== source.version || authorities.source.digest !== source.commit) {
    return refuse('source projection differs from its authority fact');
  }
  if (
    authorities.runtime_binary.revision !== source.commit ||
    authorities.runtime_binary.digest !== runtime.binary_sha256
  ) {
    return refuse('runtime binary projection is not bound to the exact source commit and binary digest');
  }
  if (
    authorities.ui_bundle.revision !== ui.bundle_version ||
    authorities.ui_bundle.digest !== ui.bundle_sha256
  ) {
    return refuse('UI runtime projection differs from the UI bundle authority fact');
  }
  if (ui.bundle_sha256 !== metadata.expectedUiBundleSha256) {
    return refuse('the served UI digest differs from the explicitly expected promoted bundle digest');
  }

  await expect(manifestStatus.locator('[data-manifest-coherence="COHERENT"]')).toHaveText(
    'COHERENT',
  );
  await expect(manifestStatus.locator('[data-manifest-version-drift="false"]')).toContainText(
    `SRC/BIN/BND ${source.version}/${runtime.binary_version}/${ui.bundle_version} · ALIGNED`,
  );

  assertJson200(browserProbe.health, '/api/health');
  const health = record(browserProbe.health.body, '/api/health');
  if (health.status !== 'ok') return refuse(`health.status=${String(health.status)}`);

  assertJson200(browserProbe.stats, '/api/graph/stats');
  const stats = record(browserProbe.stats.body, '/api/graph/stats');
  for (const field of ['node_count', 'edge_count'] as const) {
    const value = stats[field];
    if (!Number.isSafeInteger(value) || (value as number) < 0) {
      return refuse(`/api/graph/stats.${field} is not a non-negative integer`);
    }
  }
  record(stats.served_brain, '/api/graph/stats.served_brain');

  const observedApiRequests = observedRequests.filter((request) =>
    new URL(request.url()).pathname.startsWith('/api/'),
  );
  assert.ok(observedApiRequests.length >= 3, 'the real shell made no observable owner API reads');
  const unsafeRequests: string[] = [];
  for (const request of observedRequests) {
    const url = new URL(request.url());
    assert.equal(url.origin, metadata.ownerOrigin, `${url.pathname} escaped the owner origin`);
    if (!url.pathname.startsWith('/api/')) continue;
    const method = request.method();
    if (!['GET', 'HEAD', 'OPTIONS'].includes(method) && !READ_ONLY_POST_PATHS.has(url.pathname)) {
      unsafeRequests.push(`${method} ${url.pathname}`);
    }
  }
  assert.deepEqual(
    unsafeRequests,
    [],
    `the shell emitted requests not declared read-only: ${unsafeRequests.join(', ')}`,
  );
});
