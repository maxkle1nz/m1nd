/*
 * The living map — the browser proof (the uiproof standard: deterministic
 * `npx playwright test`, zero agents in the loop). The suite boots its OWN Vite
 * dev server (playwright.config) and mocks every /api/* route IN-PAGE — nothing
 * reaches a live owner (the served :1338 is out of bounds for tests, AGENTS.md).
 *
 * Proven, in a real browser against the real bundle, that the Build Map stopped
 * being a photograph (Commit 1 made the block verbs emit `graph_changed`; this
 * surface now subscribes):
 *  1. a `graph_changed` for the VIEWED brain refetches the map in place — the
 *     snapshot is re-read, the map NEVER unmounts to the loading screen, and an
 *     open block panel stays open on its block (selection preserved, #372);
 *  2. a `graph_changed` for ANOTHER brain (§4A.9.6) refetches NOTHING — a hosted
 *     brain's mutation must not disturb the bound map on screen.
 *
 * SSE is a deterministic in-page FakeES (the scan-loading.spec pattern): no live
 * owner, no wall-clock race — the test drives `graph_changed` frame by frame.
 * Neutral fixtures only (no-leak law): no real project/agent name.
 */
import { expect, test, type Page, type Route } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
// The real ratified seed (12 blocks) — a full Build Map with clickable cards.
const blockSnapshot = JSON.parse(
  readFileSync(join(HERE, '..', 'src', '__fixtures__', 'system_blocks_snapshot.json'), 'utf8'),
);

const now = () => Date.now();
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

const instance = {
  instance_id: 'inst_e2e_livemap',
  workspace_root: '/work/repo-alpha',
  runtime_root: '/work/.m1nd',
  graph_source: 'ingest',
  plasticity_state: 'stable',
  pid: 4242,
  bind: null,
  port: null,
  started_at_ms: 1_700_000_000_000,
  last_heartbeat_ms: now(),
  mode: 'serve',
  status: 'running',
  owner_live: true,
  stale: false,
  conflicts: [],
  brain_kind: null,
  display_name: 'repo-alpha',
  project_root: '/work/repo-alpha',
  node_count: 3210,
  edge_count: 9000,
};

const health = {
  status: 'ok',
  uptime_secs: 60,
  node_count: 3210,
  edge_count: 9000,
  queries_processed: 0,
  agent_sessions: [],
  domain: 'code',
  graph_generation: 1,
  plasticity_generation: 1,
};

const EMPTY_UNIVERSE = {
  schema: 'm1nd-universe-v0',
  worlds: [],
  owner: { alerts_pending: 0 },
  totals: { worlds: 0, awake: 0, pending: 0 },
};

/** The whole mocked owner. Zero worlds + a present bound skeleton → the Build Map
 *  is the front door. The snapshot is stable (same store every read), so a refetch
 *  is observable purely by its COUNT, and the map re-renders identical content. */
function mockOwner(page: Page) {
  return page.route(
    (url) => url.pathname.startsWith('/api/'),
    async (route) => {
      const path = new URL(route.request().url()).pathname;
      try {
        if (path === '/api/health') return await json(route, health);
        if (path === '/api/universe') return await json(route, EMPTY_UNIVERSE);
        if (path === '/api/instances') return await json(route, { instances: [instance] });
        if (path === '/api/instance/self')
          return await json(route, {
            instance,
            graph_state: {
              node_count: 3210,
              edge_count: 9000,
              finalized: true,
              graph_generation: 1,
              plasticity_generation: 1,
              cache_generation: 1,
              ingest_root_count: 1,
              ingest_roots: ['/work/repo-alpha'],
              workspace_root: '/work/repo-alpha',
              runtime_root: '/work/.m1nd',
            },
            active_agent_sessions: 0,
            queries_processed: 0,
            last_persist_secs_ago: 10,
            display_name: 'repo-alpha',
            project_root: '/work/repo-alpha',
          });
        if (path === '/api/presences') return await json(route, { presences: [], collisions: [] });
        if (path === '/api/tools') return await json(route, { tools: [], rest_brain_selector: true });
        if (path === '/api/mailbox') return await json(route, { missions: [] });
        if (path === '/api/tools/system_blocks_snapshot')
          return await json(route, { result: blockSnapshot });
        if (path === '/api/graph/snapshot') return await json(route, { version: 1, nodes: [], edges: [] });
        if (path === '/api/graph/stats') return await json(route, { node_count: 3210, edge_count: 9000 });
        if (path === '/api/runnerd/status') return await json(route, { runners: [] });
        if (path === '/api/events')
          return await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
        if (path.startsWith('/api/tools/')) return await json(route, { result: {} });
        return await json(route, {});
      } catch {
        /* the page may abort a held request on teardown */
      }
    },
  );
}

/** A deterministic in-page EventSource: the app's useSSE registers a named
 *  `graph_changed` listener; the test emits frames into it with no network and no
 *  clock race. Mirrors scan-loading.spec's FakeES, generalized to emit any class. */
async function installFakeSse(page: Page) {
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
    w.__emitGraphChanged = (payload: unknown) => {
      for (const es of w.__sse.instances) {
        if (es.closed) continue;
        for (const cb of es.listeners['graph_changed'] || []) cb({ data: JSON.stringify(payload) });
      }
    };
    // How many live EventSources carry a `graph_changed` listener — i.e. the map's
    // live subscription is armed. The test waits on this before emitting.
    w.__graphChangedListeners = () =>
      w.__sse.instances.filter(
        (es: { closed: boolean; listeners: Record<string, unknown[]> }) =>
          !es.closed && (es.listeners['graph_changed'] || []).length > 0,
      ).length;
  });
}

/** Land on the Build Map (the front door for a bound skeleton + zero worlds) and
 *  open the first block's detail panel. Returns the selected block id. */
async function openMapWithPanel(page: Page): Promise<string> {
  await page.goto('/');
  await expect(page.locator('[data-surface="map"]')).toBeVisible();
  // The map's live subscription arms once the first read resolves (status leaves
  // 'loading'); wait for its graph_changed listener before driving the wire.
  await page.waitForFunction(() => (window as unknown as { __graphChangedListeners: () => number }).__graphChangedListeners() >= 1);
  const card = page.locator('[data-role="block-card"]').first();
  const blockId = await card.getAttribute('data-block-card');
  await card.click();
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toBeVisible();
  return blockId ?? '';
}

/** Count of snapshot reads so far — the observable proof a refetch did (or did not)
 *  happen. Only the map surface's read is subscribed; the App front-door read fires
 *  once at load and never again, so a post-baseline delta is the map's alone. */
function countSnapshotReads(page: Page): () => number {
  let n = 0;
  page.on('request', (req) => {
    if (req.url().includes('/api/tools/system_blocks_snapshot')) n += 1;
  });
  return () => n;
}

const emitGraphChanged = (page: Page, payload: Record<string, unknown>) =>
  page.evaluate(
    (p) => (window as unknown as { __emitGraphChanged: (x: unknown) => void }).__emitGraphChanged(p),
    payload,
  );

const ARTIFACTS = 'e2e/artifacts';

// ── 1. a graph_changed for the viewed brain refreshes in place ────────────────
test('a graph_changed for the viewed brain refetches the map without unmounting; the open panel stays open', async ({
  page,
}) => {
  await installFakeSse(page);
  await mockOwner(page);
  const snapshotReads = countSnapshotReads(page);

  const blockId = await openMapWithPanel(page);
  await page.screenshot({ path: `${ARTIFACTS}/livemap-01-panel-open.png`, fullPage: true });
  const baseline = snapshotReads();

  // An agent ratifies on THIS (bound) brain: the owner emits an unscoped
  // graph_changed. The map re-reads exactly once (debounced), and never blanks.
  await emitGraphChanged(page, { event: 'system_blocks_ratify' });
  await expect.poll(() => snapshotReads(), { timeout: 3_000 }).toBe(baseline + 1);

  // NEVER-BLANK: the refresh rode the 'refreshing' path (stale-while-revalidate),
  // so the loading screen never showed and the panel stayed open on its block.
  await expect(page.locator('[data-role="build-map-loading"]')).toHaveCount(0);
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toBeVisible();
  await page.screenshot({ path: `${ARTIFACTS}/livemap-02-refreshed-panel-kept.png`, fullPage: true });

  // A second event on the same brain refetches again (proves it is durable, not a
  // one-shot), and STILL keeps the panel.
  await emitGraphChanged(page, { event: 'skeleton_candidate' });
  await expect.poll(() => snapshotReads(), { timeout: 3_000 }).toBe(baseline + 2);
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toBeVisible();
});

// ── 2. a graph_changed for another brain refetches nothing (§4A.9.6) ──────────
test('a graph_changed for ANOTHER brain leaves the bound map untouched', async ({ page }) => {
  await installFakeSse(page);
  await mockOwner(page);
  const snapshotReads = countSnapshotReads(page);

  const blockId = await openMapWithPanel(page);
  const baseline = snapshotReads();

  // A mutation scoped to a DIFFERENT brain root — the bound viewer must ignore it.
  await emitGraphChanged(page, { event: 'system_blocks_ratify', brain_root: '/work/other-repo' });
  // Wait well past the ~500 ms debounce; the count must not move.
  await page.waitForTimeout(1_000);
  expect(snapshotReads()).toBe(baseline);
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toBeVisible();

  // And an unscoped event on THIS brain still works from the same state (control:
  // the wire is live; only the foreign scope was filtered).
  await emitGraphChanged(page, { event: 'system_blocks_ratify' });
  await expect.poll(() => snapshotReads(), { timeout: 3_000 }).toBe(baseline + 1);
});
