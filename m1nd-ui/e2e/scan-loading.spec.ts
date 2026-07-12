/*
 * SCAN loading UX — the browser proof (docs/uml/scan-loading.md; the uiproof
 * standard: deterministic `npx playwright test`, zero agents in the loop).
 *
 * What is proven, in a REAL browser against the real app bundle:
 *  1. clicking "Scan this repo" on a skeleton-less brain shows the WAIT PANEL —
 *     named phase + a live mm:ss elapsed clock that visibly counts (the screen
 *     can never look dead while the owner clusters);
 *  2. past the slow threshold the panel SAYS the wait is long and keeps counting;
 *  3. the resolve lands the candidate dress (banner) after the reload;
 *  4. a failed scan surfaces the owner's message verbatim + the retry works;
 *  5. "Stop waiting" aborts the browser wait with the honest canceled note.
 *
 * Every /api/* call is mocked in-page (page.route) — NOTHING reaches any live
 * owner. The skeleton_candidate mock delays its fulfillment to reproduce the
 * real synchronous wait (the owner's naming budget alone can hold ~110s).
 */
import { expect, test, type Page, type Route } from '@playwright/test';

// ── neutral fixtures (no-leak law: example names only) ────────────────────────

const NODE_COUNT = 3210;

const instance = {
  instance_id: 'inst_e2e_0001',
  workspace_root: '/work/repo-alpha',
  runtime_root: '/work/repo-alpha/.m1nd',
  graph_source: 'ingest',
  plasticity_state: 'stable',
  pid: 4242,
  bind: null,
  port: null,
  started_at_ms: 1_700_000_000_000,
  last_heartbeat_ms: Date.now(),
  mode: 'serve',
  status: 'running',
  owner_live: true,
  stale: false,
  conflicts: [],
};

const selfEnvelope = {
  instance,
  graph_state: {
    node_count: NODE_COUNT,
    edge_count: 9_000,
    finalized: true,
    graph_generation: 1,
    plasticity_generation: 1,
    cache_generation: 1,
    ingest_root_count: 1,
    ingest_roots: ['/work/repo-alpha'],
    workspace_root: '/work/repo-alpha',
    runtime_root: '/work/repo-alpha/.m1nd',
  },
  active_agent_sessions: 0,
  queries_processed: 0,
  last_persist_secs_ago: 10,
  display_name: 'repo-alpha',
  project_root: '/work/repo-alpha',
};

const health = {
  status: 'ok',
  uptime_secs: 60,
  node_count: NODE_COUNT,
  edge_count: 9_000,
  queries_processed: 0,
  agent_sessions: [],
  domain: 'code',
  graph_generation: 1,
  plasticity_generation: 1,
};

/** A minimal, type-faithful candidate block (mirrors the unit-test factories). */
function candBlock(id: string, name: string) {
  return {
    block_id: id,
    name,
    purpose: `${name} purpose`,
    kind: 'scanned',
    state: 'candidate',
    boundary_version: 1,
    contract_version: 1,
    membership_source: 'proposed',
    membership: [
      { path: `${id}/a.rs`, role: 'primary' },
      { path: `${id}/b.rs`, role: 'primary' },
    ],
    sockets: { inputs: [], outputs: [], external: [] },
    receipt_contract: {
      version: 1,
      required: [],
      optional: [],
      waived: [],
      declared_by: null,
      declared_at: null,
    },
    receipts: [],
    layout: { x: null, y: null, locked: false, algorithm_seed: null, version: 1 },
    unmapped_residue: [],
    candidate_meta: {
      named_by: 'heuristic',
      needs_owner_naming: false,
      edge_sample_size: 40,
      directory_support: 0.9,
      coverage_ratio: 0.9,
      shared_member_count: 0,
    },
  };
}

const candidateStore = {
  schema: 'm1nd-system-block-store-v0',
  store_version: 2,
  skeleton: {
    skeleton_id: 'sk_repo_alpha_candidate',
    version: 1,
    state: 'candidate',
    ratification: { method: 'verb', ratifier: '', ratified_at: '', commit: '' },
  },
  blocks: [candBlock('sb_repo_alpha_core', 'Core'), candBlock('sb_repo_alpha_ui', 'UI')],
  unmapped_policy: { visible: true, default_action: 'leave_unmapped_until_ratified' },
};

const scanResult = {
  present: true,
  store_version: 2,
  transaction_state: 'created_candidate_store',
  candidate_revision_written: false,
  block_count: 2,
  review_limit: 16,
  candidate_seed: {},
  report: {
    algorithm: 'louvain+directory',
    repo_file_count: 321,
    graph_node_count: NODE_COUNT,
    graph_edge_count: 9_000,
    block_count: 2,
    claimed_file_count: 300,
    coverage_ratio: 0.93,
    graph_cohesion_edge_floor: 5,
    unmapped_total: 21,
    blocks: [],
    naming: {
      requested: 'auto',
      applied: 'heuristic',
      runner_available: false,
      note: 'no naming runner announced; heuristic provisional names were used',
    },
  },
};

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

/** Wire the whole mocked owner. `scanBehavior` scripts skeleton_candidate call-by-
 *  call; once one scan RESOLVES ok, the snapshot flips to the candidate store
 *  (exactly what the real owner would serve on the post-scan reload). */
function mockOwner(
  page: Page,
  scanBehavior: (call: number, route: Route, markScanned: () => void) => Promise<void>,
) {
  let scanned = false;
  let scanCalls = 0;
  // Predicate, NOT a glob: '**/api/**' would also swallow Vite's served source
  // modules under /src/api/… and the app would never mount. Only the owner's
  // REST surface (/api/*) is mocked; every dev-server asset passes through.
  return page.route(
    (url) => url.pathname.startsWith('/api/'),
    async (route) => {
      const path = new URL(route.request().url()).pathname;
      try {
        if (path === '/api/health') return await json(route, health);
        if (path === '/api/instance/self') return await json(route, selfEnvelope);
        if (path === '/api/instances') return await json(route, { instances: [instance] });
        if (path === '/api/graph/snapshot')
          return await json(route, { version: 1, nodes: [], edges: [] });
        if (path === '/api/graph/stats')
          return await json(route, { node_count: NODE_COUNT, edge_count: 9_000 });
        if (path === '/api/runnerd/status') return await json(route, { runners: [] });
        if (path === '/api/mailbox') return await json(route, { missions: [] });
        if (path === '/api/events')
          return await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
        if (path === '/api/tools/system_blocks_snapshot') {
          return await json(
            route,
            scanned
              ? { result: { present: true, store_version: 2, block_count: 2, store: candidateStore } }
              : { result: { present: false, honest: 'no skeleton yet — import a seed or run a scan' } },
          );
        }
        if (path === '/api/tools/skeleton_candidate') {
          scanCalls += 1;
          return await scanBehavior(scanCalls, route, () => {
            scanned = true;
          });
        }
        // Any other tool (north, trust, tremor…) answers a harmless empty result.
        if (path.startsWith('/api/tools/')) return await json(route, { result: {} });
        return await json(route, {});
      } catch {
        // The page may abort a held request (the cancel spec) — that is the point.
      }
    },
  );
}

/** Land on the Build Map's honest EMPTY state through the real shell route:
 *  tree landing (no skeleton) → the top bar's "Build Map" door. */
async function openEmptyMap(page: Page) {
  await page.goto('/');
  await page.locator('[data-role="open-map"]').click();
  await expect(page.locator('[data-role="build-map-empty"]')).toBeVisible();
  await expect(page.locator('[data-role="scan-repo"]')).toBeEnabled();
}

const ARTIFACTS = 'e2e/artifacts';

// ── 1. the wait panel is alive and resolves into candidate dress ─────────────

test('scan shows a living wait panel (phase + counting clock) and lands the candidate', async ({
  page,
}) => {
  await mockOwner(page, async (_call, route, markScanned) => {
    await sleep(3_500); // the owner legitimately holds the POST — reproduce it
    markScanned();
    await json(route, { result: scanResult });
  });
  await openEmptyMap(page);

  await page.locator('[data-role="scan-repo"]').click();

  // The panel appears with the named phase, the REAL node count and a clock.
  const wait = page.locator('[data-role="scan-wait"]');
  await expect(wait).toBeVisible();
  await expect(page.locator('[data-role="scan-phase-label"]')).toHaveText(/clustering…/);
  await expect(wait).toContainText('clustering 3,210 nodes into a candidate map');
  await expect(page.locator('[data-role="scan-repo"]')).toBeDisabled();

  // NEVER-DEAD: the clock visibly counts (0:00 → at least 0:02) — real ticks.
  const elapsed = page.locator('[data-role="scan-elapsed"]');
  await expect(elapsed).toHaveText(/^0:0[0-1]$/);
  await expect(elapsed).toHaveText(/^0:0[2-9]$/, { timeout: 5_000 });
  await page.screenshot({ path: `${ARTIFACTS}/01-scan-wait-clustering.png`, fullPage: true });

  // The resolve reloads into the candidate dress — the banner that can never be
  // mistaken for a ratified map.
  await expect(page.locator('[data-role="candidate-banner"]')).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('[data-role="scan-wait"]')).toHaveCount(0);
  await page.screenshot({ path: `${ARTIFACTS}/02-scan-candidate-ready.png`, fullPage: true });
});

// ── 2. the slow threshold: the panel SAYS it is long and keeps counting ──────

test('past 10s the panel adds the honest slow note and the clock keeps counting', async ({
  page,
}) => {
  await mockOwner(page, async (_call, route, markScanned) => {
    await sleep(12_000); // past SCAN_SLOW_AFTER_MS — the real long wait
    markScanned();
    await json(route, { result: scanResult });
  });
  await openEmptyMap(page);

  await page.locator('[data-role="scan-repo"]').click();
  await expect(page.locator('[data-role="scan-wait"]')).toBeVisible();

  // The slow note is earned by real elapsed time (≥10s), never faked early.
  await expect(page.locator('[data-role="scan-slow-note"]')).toBeVisible({ timeout: 13_000 });
  await expect(page.locator('[data-role="scan-slow-note"]')).toContainText(
    '3,210 nodes usually takes a while',
  );
  await expect(page.locator('[data-role="scan-elapsed"]')).toHaveText(/^0:1\d$/);
  await page.screenshot({ path: `${ARTIFACTS}/03-scan-wait-slow.png`, fullPage: true });

  await expect(page.locator('[data-role="candidate-banner"]')).toBeVisible({ timeout: 10_000 });
});

// ── 3. error + retry ──────────────────────────────────────────────────────────

test('a failed scan surfaces the owner message verbatim and the retry lands', async ({ page }) => {
  await mockOwner(page, async (call, route, markScanned) => {
    if (call === 1) {
      await sleep(800);
      return json(route, { error: 'tool_error', message: 'clustering exploded' }, 500);
    }
    await sleep(500);
    markScanned();
    await json(route, { result: scanResult });
  });
  await openEmptyMap(page);

  await page.locator('[data-role="scan-repo"]').click();
  await expect(page.locator('[data-role="scan-wait"]')).toBeVisible();

  // The refusal lands the honest toast (owner string verbatim), panel gone,
  // and the gesture stays open — the retry IS the scan button.
  const toast = page.locator('[data-role="scan-toast"]');
  await expect(toast).toBeVisible({ timeout: 5_000 });
  await expect(toast).toHaveAttribute('data-toast-kind', 'error');
  await expect(toast).toContainText('clustering exploded');
  await expect(page.locator('[data-role="scan-wait"]')).toHaveCount(0);
  const button = page.locator('[data-role="scan-repo"]');
  await expect(button).toBeEnabled();
  await expect(button).toHaveText(/Scan this repo/);
  await page.screenshot({ path: `${ARTIFACTS}/04-scan-error-retry.png`, fullPage: true });

  await button.click();
  await expect(page.locator('[data-role="candidate-banner"]')).toBeVisible({ timeout: 10_000 });
});

// ── 4. stop waiting (the honest abort) ────────────────────────────────────────

test('"Stop waiting" aborts the browser wait with the honest canceled note', async ({ page }) => {
  await mockOwner(page, async (_call, route, markScanned) => {
    await sleep(30_000); // a wait the user will not sit through
    markScanned();
    await json(route, { result: scanResult });
  });
  await openEmptyMap(page);

  await page.locator('[data-role="scan-repo"]').click();
  await expect(page.locator('[data-role="scan-wait"]')).toBeVisible();
  await expect(page.locator('[data-role="scan-cancel"]')).toBeVisible();
  await sleep(1_500);

  await page.locator('[data-role="scan-cancel"]').click();

  // Idle again, with the honest note: the OWNER was not stopped.
  await expect(page.locator('[data-role="scan-wait"]')).toHaveCount(0);
  const toast = page.locator('[data-role="scan-toast"]');
  await expect(toast).toBeVisible();
  await expect(toast).toHaveAttribute('data-toast-kind', 'canceled');
  await expect(toast).toContainText('owner may still finish');
  await expect(page.locator('[data-role="scan-repo"]')).toBeEnabled();
  await page.screenshot({ path: `${ARTIFACTS}/05-scan-canceled.png`, fullPage: true });
});

// ── 5. the owner narrates the REAL phase on the SSE channel (slice 2) ─────────

test('the wait panel shows the REAL server phase as the owner narrates it (SSE)', async ({
  page,
}) => {
  // A deterministic fake EventSource, driven frame-by-frame from the test — the
  // owner's `scan_progress` narration with NO live owner and NO wall-clock race.
  // (The whole /api/* surface is still mocked below; EventSource just never hits
  // the network here.)
  await page.addInitScript(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const w = window as any;
    w.__sse = { instances: [] };
    class FakeES {
      url: string;
      closed = false;
      listeners: Record<string, Array<(e: { data: string }) => void>> = {};
      onmessage: ((e: { data: string }) => void) | null = null;
      onerror: ((e: unknown) => void) | null = null;
      constructor(url: string) {
        this.url = url;
        w.__sse.instances.push(this);
      }
      addEventListener(type: string, cb: (e: { data: string }) => void) {
        (this.listeners[type] = this.listeners[type] || []).push(cb);
      }
      removeEventListener(type: string, cb: (e: { data: string }) => void) {
        this.listeners[type] = (this.listeners[type] || []).filter((f) => f !== cb);
      }
      close() {
        this.closed = true;
      }
    }
    w.EventSource = FakeES;
    w.__emitScanProgress = (payload: unknown) => {
      for (const es of w.__sse.instances) {
        if (es.closed) continue;
        for (const cb of es.listeners['scan_progress'] || []) cb({ data: JSON.stringify(payload) });
      }
    };
    w.__openSseCount = () => w.__sse.instances.filter((es: { closed: boolean }) => !es.closed).length;
  });

  await mockOwner(page, async (_call, route, markScanned) => {
    await sleep(4_000); // the owner holds the POST while it narrates on the side
    markScanned();
    await json(route, { result: scanResult });
  });
  await openEmptyMap(page);

  await page.locator('[data-role="scan-repo"]').click();
  const wait = page.locator('[data-role="scan-wait"]');
  await expect(wait).toBeVisible();
  // Before any SSE, the static client label shows — the honest degradation baseline.
  await expect(page.locator('[data-role="scan-phase-label"]')).toHaveText(/clustering…/);

  // The Build Map opened its own in-flight SSE subscription (app-level + scan = 2).
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await page.waitForFunction(() => (window as any).__openSseCount() >= 2);

  const label = page.locator('[data-role="scan-phase-label"]');
  const emit = (payload: Record<string, unknown>) =>
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    page.evaluate((p) => (window as any).__emitScanProgress(p), payload);

  // The owner narrates its real phases; the label tracks each one as it changes.
  await emit({ phase: 'file_list', file_count: 321 });
  await expect(label).toHaveText(/listing 321 files…/);

  await emit({ phase: 'clustering', node_count: 3210, edge_count: 9000 });
  await expect(label).toHaveText(/clustering 3,210 nodes…/);

  await emit({ phase: 'naming', block_count: 2, naming_waves: 1 });
  await expect(label).toHaveText(/naming 2 blocks…/);
  await expect(wait).toHaveAttribute('data-scan-server-phase', 'naming');
  await page.screenshot({ path: `${ARTIFACTS}/06-scan-server-phase.png`, fullPage: true });

  // The HTTP response still drives the terminal — the candidate lands, panel gone.
  await expect(page.locator('[data-role="candidate-banner"]')).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('[data-role="scan-wait"]')).toHaveCount(0);
});
