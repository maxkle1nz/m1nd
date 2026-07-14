/*
 * Honest doors and exits — the browser proof (the uiproof standard: deterministic
 * `npx playwright test`, zero agents in the loop). The suite boots its OWN Vite dev
 * server (playwright.config) and mocks every /api/* route IN-PAGE — nothing reaches a
 * live owner (the served :1338 is out of bounds for tests, AGENTS.md).
 *
 * Proven, in a real browser against the real bundle:
 *  1. the map's Reconcile asks first — a click opens the two-step confirm, and NO write
 *     leaves the browser until the human confirms (every human write asks);
 *  2. the Landing's owner item lands ON the Hall's owner-alerts panel, and an ack calls
 *     alerts_ack (bound-session, no ?brain=);
 *  3. a stagnant executing head is dismissable — the ✕ + honest confirm hide the card
 *     from the tray (presentation-only; no server write);
 *  4. a transient /api/universe blip keeps the last-good sky lit + flies the retry note;
 *  5. the block detail panel finally closes — the ✕, and a re-click on the card, both
 *     deselect it.
 *
 * Neutral fixtures only (no-leak law): no real project/agent name of the owner.
 */
import { expect, test, type Page, type Route } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
// The real ratified seed (12 blocks) — a full Build Map to drive reconcile + the panel.
const blockSnapshot = JSON.parse(
  readFileSync(join(HERE, '..', 'src', '__fixtures__', 'system_blocks_snapshot.json'), 'utf8'),
);

const now = () => Date.now();
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

const instance = {
  instance_id: 'inst_e2e_doors',
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

/** One world + a pending owner alert count → the Landing carries an owner item. */
function universeWithOwnerAlerts(alertsPending: number) {
  return {
    schema: 'm1nd-universe-v0',
    worlds: [
      {
        key: '/work/repo-alpha',
        root: '/work/repo-alpha',
        name: 'repo-alpha',
        node_count: 3210,
        edge_count: 9000,
        updated_ms: now() - 60_000,
        awake: false,
        presences: [],
        pending: { stamps: 0, ratifies: 0 },
        letters: { merge_wait: 0, total: 0 },
      },
    ],
    owner: { alerts_pending: alertsPending },
    totals: { worlds: 1, awake: 0, pending: alertsPending },
  };
}

/** Two owner daemon alerts (bound-session scope). */
function ownerAlerts() {
  return [
    {
      alert_id: 'al_1',
      severity: 'critical',
      kind: 'trust_risk',
      message: 'trust risk climbing on the core',
      confidence: 0.9,
      evidence: ['src/core.rs:12'],
      created_at_ms: now() - 30_000,
      acked: false,
    },
    {
      alert_id: 'al_2',
      severity: 'warning',
      kind: 'drift',
      message: 'a member drifted from its block',
      confidence: 0.7,
      evidence: [],
      created_at_ms: now() - 90_000,
      acked: false,
    },
  ];
}

/** A mailbox with one STAGNANT executing head (updated 3 days ago). */
function stagnantMissions() {
  const stale = new Date(now() - 3 * 24 * 60 * 60 * 1000).toISOString();
  return {
    served_brain: { project_root: '/work/repo-alpha', display_name: 'repo-alpha' },
    missions: [
      {
        mission_id: 'msn_00000000e999',
        head_letter_id: 'eeee11110001',
        head: {
          schema: 'm1nd-mission-letter-v0',
          mission_id: 'msn_00000000e999',
          mission_seq: 1,
          block_id: 'sb_probe',
          brain_ref: 'repo-alpha',
          seat: 'hand',
          capability: 'build-runner',
          phase: 'executing',
          tokens_total: 0,
          started_at: stale,
          updated_at: stale,
        },
        superseded_count: 0,
      },
    ],
  };
}

interface MockOpts {
  universe?: unknown;
  /** first universe read OK, then every later poll fails (a non-404 blip). */
  universeFailAfterFirst?: boolean;
  snapshotPresent?: boolean;
  alerts?: unknown[];
  missions?: unknown;
}

function mockOwner(page: Page, opts: MockOpts = {}) {
  const universe = opts.universe ?? EMPTY_UNIVERSE;
  const alerts = opts.alerts ?? [];
  let universeCalls = 0;
  return page.route(
    (url) => url.pathname.startsWith('/api/'),
    async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname;
      try {
        if (path === '/api/health') return await json(route, health);
        if (path === '/api/universe') {
          universeCalls += 1;
          if (opts.universeFailAfterFirst && universeCalls > 1) {
            return await json(route, { error: 'server', detail: 'transient' }, 500);
          }
          return await json(route, universe);
        }
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
        if (path === '/api/mailbox') {
          if (url.searchParams.get('kind') === 'mission')
            return await json(route, opts.missions ?? { missions: [] });
          return await json(route, { missions: [] });
        }
        if (path === '/api/tools/system_blocks_snapshot')
          return await json(
            route,
            opts.snapshotPresent
              ? { result: blockSnapshot }
              : { result: { present: false, honest: 'no skeleton yet' } },
          );
        if (path === '/api/tools/alerts_list')
          return await json(route, { result: { alerts, total: alerts.length, active: true } });
        if (path === '/api/tools/alerts_ack')
          return await json(route, { result: { acked: 1, requested: 1, acked_at_ms: now() } });
        if (path === '/api/tools/system_blocks_reconcile')
          return await json(route, {
            result: { store_version: 2, moved: [], unmapped_total: 0, unmapped_files: [], conflict: false },
          });
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

const ARTIFACTS = 'e2e/artifacts';

// ── 1. the map's Reconcile asks first — no write before the confirm ───────────
test('Reconcile opens a two-step confirm; no write leaves the browser until confirmed', async ({
  page,
}) => {
  await mockOwner(page, { snapshotPresent: true });
  let reconcileWrites = 0;
  page.on('request', (req) => {
    if (req.url().includes('/api/tools/system_blocks_reconcile')) reconcileWrites += 1;
  });
  await page.goto('/');

  // Zero worlds + a bound skeleton → the Build Map is the front door.
  await expect(page.locator('[data-surface="map"]')).toBeVisible();
  const reconcile = page.locator('[data-role="reconcile"]');
  await expect(reconcile).toBeVisible();

  // A click opens the confirm — it does NOT fire the write.
  await reconcile.click();
  await expect(page.locator('[data-role="reconcile-confirm"]')).toBeVisible();
  expect(reconcileWrites).toBe(0);

  // Cancel closes it, still no write.
  await page.locator('[data-role="reconcile-confirm-cancel"]').click();
  await expect(page.locator('[data-role="reconcile-confirm"]')).toHaveCount(0);
  expect(reconcileWrites).toBe(0);

  // Confirming DOES fire the one write.
  await reconcile.click();
  await page.locator('[data-role="reconcile-confirm-go"]').click();
  await expect.poll(() => reconcileWrites).toBe(1);

  await page.screenshot({ path: `${ARTIFACTS}/doors-01-reconcile-confirm.png`, fullPage: true });
});

// ── 2. the Landing's owner item lands on the Hall's alerts panel ──────────────
test('the Landing owner item opens the Hall alerts panel; an ack calls alerts_ack', async ({ page }) => {
  await mockOwner(page, { universe: universeWithOwnerAlerts(2), alerts: ownerAlerts() });
  let ackCalls = 0;
  page.on('request', (req) => {
    if (req.url().includes('/api/tools/alerts_ack')) ackCalls += 1;
  });
  await page.goto('/');

  const alertItem = page.locator('[data-role="the-landing"] [data-landing-kind="alert"]');
  await expect(alertItem).toBeVisible();
  await expect(alertItem).toContainText('2 daemon alerts to acknowledge');
  await alertItem.click();

  // The Hall opens ON the owner-alerts panel (auto-opened), listing the two alerts.
  const panel = page.locator('[data-role="owner-alerts-panel"]');
  await expect(panel).toBeVisible();
  await expect(panel.locator('[data-role="owner-alert"]')).toHaveCount(2);
  await expect(panel).toContainText('trust risk climbing on the core');

  await page.screenshot({ path: `${ARTIFACTS}/doors-02-owner-alerts.png`, fullPage: true });

  // Acknowledging one calls alerts_ack (bound-session; no ?brain= on the URL).
  await panel.locator('[data-role="alert-ack"]').first().click();
  await expect.poll(() => ackCalls).toBe(1);
});

// ── 3. a stagnant executing head is dismissable (presentation-only) ───────────
test('a stagnant executing head can be dismissed from the tray without a server write', async ({
  page,
}) => {
  await mockOwner(page, { snapshotPresent: true, missions: stagnantMissions() });
  let missionWrites = 0;
  page.on('request', (req) => {
    if (req.url().includes('/api/tools/mission_post')) missionWrites += 1;
  });
  await page.goto('/');

  await expect(page.locator('[data-surface="map"]')).toBeVisible();
  // Expand the tray so the card (and its stagnant dismiss) is visible.
  await page.locator('[data-role="mission-tray-toggle"]').first().click();
  const card = page.locator('[data-role="mission-card"][data-mission-id="msn_00000000e999"]');
  await expect(card).toBeVisible();

  // The stagnant ✕ opens an honest confirm; confirming hides the card, no server write.
  await card.locator('[data-role="mission-stagnant-dismiss"]').click();
  await expect(card.locator('[data-role="dismiss-confirm"]')).toBeVisible();
  await card.locator('[data-role="dismiss-confirm-go"]').click();
  await expect(page.locator('[data-mission-id="msn_00000000e999"]')).toHaveCount(0);
  expect(missionWrites).toBe(0);
});

// ── 4. a transient universe blip keeps the sky lit + flies the retry note ─────
test('a /api/universe blip after a good read keeps the last-good sky + a retry note', async ({
  page,
}) => {
  await mockOwner(page, { universe: universeWithOwnerAlerts(0), universeFailAfterFirst: true });
  await page.goto('/');

  // The first read lit the sky.
  await expect(page.locator('[data-role="universe"]')).toBeVisible();
  await expect(page.locator('[data-world-name="repo-alpha"]')).toBeVisible();

  // A later poll fails — the sky STAYS and a discreet note tells the truth.
  await expect(page.locator('[data-role="universe-note"]')).toBeVisible({ timeout: 9000 });
  await expect(page.locator('[data-role="universe-note"]')).toContainText('read failed — retrying');
  await expect(page.locator('[data-world-name="repo-alpha"]')).toBeVisible();
});

// ── 5. the block detail panel finally closes ──────────────────────────────────
test('the block panel closes via the ✕ and via a re-click on the same card', async ({ page }) => {
  await mockOwner(page, { snapshotPresent: true });
  await page.goto('/');

  await expect(page.locator('[data-surface="map"]')).toBeVisible();
  const card = page.locator('[data-role="block-card"]').first();
  const blockId = await card.getAttribute('data-block-card');
  await card.click();

  const panel = page.locator(`[data-block-panel="${blockId}"]`);
  await expect(panel).toBeVisible();

  // (a) the header ✕ closes it.
  await panel.locator('[data-role="block-panel-close"]').click();
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toHaveCount(0);
  await expect(page.locator('[data-role="block-panel-empty"]')).toBeVisible();

  // (c) a re-click on the same card toggles it open then closed.
  await card.click();
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toBeVisible();
  await card.click();
  await expect(page.locator(`[data-block-panel="${blockId}"]`)).toHaveCount(0);

  await page.screenshot({ path: `${ARTIFACTS}/doors-05-panel-exit.png`, fullPage: true });
});
