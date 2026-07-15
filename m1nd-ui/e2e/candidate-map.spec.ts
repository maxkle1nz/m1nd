/*
 * The candidate Build Map stays alive to the click (the uiproof standard:
 * deterministic `npx playwright test`, zero agents). Boots the config's own Vite
 * dev server and mocks every /api/* IN-PAGE — nothing reaches a live owner (the
 * served :1338 is out of bounds for tests, AGENTS.md). Neutral fixtures only (the
 * m1nd seed, this repo's own name; no other project/agent name).
 *
 * The bug this guards (a candidate map went "dead to clicks"): a first-run scan
 * writes a CANDIDATE store, then keeps mutating the graph as it settles, so the
 * living map (#376) re-reads on the `graph_changed` burst. A re-read that catches
 * the store MID-REWRITE reads it absent (present:false) and back. #372's stale-
 * while-revalidate kept the map mounted across a present→present reload, but the
 * present→absent→present blink flipped "last snapshot present" false, dropping the
 * NEXT read to the cold 'loading' screen — which UNMOUNTS the map and resets the
 * human's selection. Ratified maps have a stable, always-present store, so they
 * never hit it (why the same click "worked on m1nd, died on a candidate").
 */
import { expect, test, type Page, type Route } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
// The real seed (12 blocks), flipped to a candidate skeleton: dashed cards, the
// first-run banner, and — the point — clickable candidate cards.
const candidateSnapshot = JSON.parse(
  readFileSync(join(HERE, '..', 'src', '__fixtures__', 'system_blocks_snapshot.json'), 'utf8'),
);
candidateSnapshot.store.skeleton.state = 'candidate';
for (const b of candidateSnapshot.store.blocks) b.state = 'candidate';
/** The honest "no store" a re-read sees while the scan is REPLACING the store. */
const STORE_MID_REWRITE = { present: false, honest: 'no skeleton yet — scan settling' };

// Flipped from the test body (Node side) so exactly ONE re-read catches the store
// mid-rewrite; the initial load and every other read serve the candidate store.
let storeMidRewrite = false;

const now = () => Date.now();
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

const instance = {
  instance_id: 'inst_e2e_cand', workspace_root: '/work/repo-alpha', runtime_root: '/work/.m1nd',
  graph_source: 'ingest', plasticity_state: 'stable', pid: 4242, bind: null, port: null,
  started_at_ms: 1_700_000_000_000, last_heartbeat_ms: now(), mode: 'serve', status: 'running',
  owner_live: true, stale: false, conflicts: [], brain_kind: null, display_name: 'repo-alpha',
  project_root: '/work/repo-alpha', node_count: 3210, edge_count: 9000,
};
const health = {
  status: 'ok', uptime_secs: 60, node_count: 3210, edge_count: 9000, queries_processed: 0,
  agent_sessions: [], domain: 'code', graph_generation: 1, plasticity_generation: 1,
};
const EMPTY_UNIVERSE = {
  schema: 'm1nd-universe-v0', worlds: [], owner: { alerts_pending: 0 },
  totals: { worlds: 0, awake: 0, pending: 0 },
};

/** Zero worlds + a present bound (candidate) skeleton → the Build Map is the front
 *  door. `storeMidRewrite` makes the snapshot read serve the honest absence. */
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
              node_count: 3210, edge_count: 9000, finalized: true, graph_generation: 1,
              plasticity_generation: 1, cache_generation: 1, ingest_root_count: 1,
              ingest_roots: ['/work/repo-alpha'], workspace_root: '/work/repo-alpha',
              runtime_root: '/work/.m1nd',
            },
            active_agent_sessions: 0, queries_processed: 0, last_persist_secs_ago: 10,
            display_name: 'repo-alpha', project_root: '/work/repo-alpha',
          });
        if (path === '/api/presences') return await json(route, { presences: [], collisions: [] });
        if (path === '/api/tools') return await json(route, { tools: [], rest_brain_selector: true });
        if (path === '/api/mailbox') return await json(route, { missions: [] });
        if (path === '/api/tools/system_blocks_snapshot')
          return await json(route, { result: storeMidRewrite ? STORE_MID_REWRITE : candidateSnapshot });
        if (path === '/api/graph/snapshot') return await json(route, { version: 1, nodes: [], edges: [] });
        if (path === '/api/graph/stats') return await json(route, { node_count: 3210, edge_count: 9000 });
        if (path === '/api/runnerd/status') return await json(route, { runners: [] });
        if (path === '/api/events')
          return await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
        if (path.startsWith('/api/tools/')) return await json(route, { result: {} });
        return await json(route, {});
      } catch { /* the page may abort a held request on teardown */ }
    },
  );
}

/** A deterministic in-page EventSource (the living-map.spec pattern): the app's
 *  useSSE registers a `graph_changed` listener; the test drives the wire with no
 *  network and no clock race. */
async function installFakeSse(page: Page) {
  await page.addInitScript(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const w = window as any;
    w.__sse = { instances: [] };
    class FakeES {
      url: string; closed = false;
      listeners: Record<string, Array<(e: { data: string }) => void>> = {};
      onmessage: ((e: { data: string }) => void) | null = null;
      onerror: ((e: unknown) => void) | null = null;
      constructor(url: string) { this.url = url; w.__sse.instances.push(this); }
      addEventListener(type: string, cb: (e: { data: string }) => void) {
        (this.listeners[type] = this.listeners[type] || []).push(cb);
      }
      removeEventListener(type: string, cb: (e: { data: string }) => void) {
        this.listeners[type] = (this.listeners[type] || []).filter((f) => f !== cb);
      }
      close() { this.closed = true; }
    }
    w.EventSource = FakeES;
    w.__emitGraphChanged = (payload: unknown) => {
      for (const es of w.__sse.instances) {
        if (es.closed) continue;
        for (const cb of es.listeners['graph_changed'] || []) cb({ data: JSON.stringify(payload) });
      }
    };
    w.__graphChangedListeners = () =>
      w.__sse.instances.filter(
        (es: { closed: boolean; listeners: Record<string, unknown[]> }) =>
          !es.closed && (es.listeners['graph_changed'] || []).length > 0,
      ).length;
  });
}

const emitGraphChanged = (page: Page, payload: Record<string, unknown>) =>
  page.evaluate(
    (p) => (window as unknown as { __emitGraphChanged: (x: unknown) => void }).__emitGraphChanged(p),
    payload,
  );

/** Land on the candidate Build Map and wait for its live subscription to arm. */
async function landCandidateMap(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.locator('[data-surface="map"]')).toBeVisible();
  await expect(page.locator('[data-role="candidate-banner"]')).toBeVisible();
  await page.waitForFunction(
    () => (window as unknown as { __graphChangedListeners: () => number }).__graphChangedListeners() >= 1,
  );
}

// ── 1. the core contract the bug denied ───────────────────────────────────────
test('a candidate block opens its detail panel on click', async ({ page }) => {
  storeMidRewrite = false;
  await installFakeSse(page);
  await mockOwner(page);
  await landCandidateMap(page);

  // Nothing is selected yet — the calm hint aside, no panel.
  await expect(page.locator('[data-role="block-panel-empty"]')).toBeVisible();
  const card = page.locator('[data-role="block-card"]').first();
  const blockId = await card.getAttribute('data-block-card');
  await card.click();

  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toBeVisible();
  await expect(page.locator('[data-role="block-panel-empty"]')).toHaveCount(0);
});

// ── 2. the regression guard: a fresh-scan store rewrite never drops the panel ──
test('the living map keeps the open candidate panel across a store rewrite (present→absent→present)', async ({
  page,
}) => {
  storeMidRewrite = false;
  await installFakeSse(page);
  await mockOwner(page);
  await landCandidateMap(page);

  const card = page.locator('[data-role="block-card"]').first();
  const blockId = await card.getAttribute('data-block-card');
  await card.click();
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toBeVisible();

  // Phase 1 — the scan REPLACES the store; a re-read on the graph_changed burst
  // catches it mid-rewrite and reads it absent. Waiting for the honest empty screen
  // both proves the transient landed AND keeps it a SEPARATE re-read from phase 2
  // (a single debounce window would collapse the two emits into one present read).
  storeMidRewrite = true;
  await emitGraphChanged(page, { event: 'skeleton_candidate' });
  await expect(page.locator('[data-role="build-map-empty"]')).toBeVisible();

  // Phase 2 — the store settles; the next re-read serves it again. The map had
  // ALREADY painted before phase 1, so this re-read must ride the discreet
  // 'refreshing' path (never the cold 'loading' remount). Selection survives the
  // whole present→absent→present blink → the panel is open on its block again.
  storeMidRewrite = false;
  await emitGraphChanged(page, { event: 'skeleton_candidate' });
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toBeVisible();
  await expect(page.locator('[data-role="build-map-loading"]')).toHaveCount(0);
});
