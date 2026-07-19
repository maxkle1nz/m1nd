/*
 * The Universe L0 landing surface — the browser proof (HUMAN-VIEW-V2 F30; the
 * uiproof standard: deterministic `npx playwright test`, zero agents in the loop).
 * The suite boots its OWN Vite dev server (playwright.config) and mocks every
 * /api/* route IN-PAGE — nothing reaches a live owner (the served :1338 is out of
 * bounds for tests, AGENTS.md).
 *
 * Proven, in a real browser against the real bundle:
 *  1. the entry rule: ≥1 world → the Universe leads; a labelled world disc + the
 *     serif FACTS header render (never a cross-brain vital);
 *  2. the entry rule falls through with zero worlds: zero brains → the honest
 *     fail-closed Threshold, a bound-brain skeleton → the Build Map front door;
 *  3. the Landing lists a merge_wait stamp and a click NAVIGATES to that world's
 *     room (its tray card), composing no write;
 *  4. anti-regression: a world click reaches the map; the existing rooms stay live.
 *
 * Neutral fixtures only (no-leak law): no real project/agent name of the owner.
 */
import { expect, test, type Page, type Route } from '@playwright/test';

const now = () => Date.now();
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

// ── neutral owner envelopes (example names only) ──────────────────────────────
const instance = {
  instance_id: 'inst_e2e_uni',
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
  node_count: null,
  edge_count: null,
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

/** A two-world panorama: repo-alpha awake with pending gestures, repo-beta quiet
 *  and stale. The Landing therefore carries a stamp + a ratify on repo-alpha. */
function twoWorldUniverse() {
  return {
    schema: 'm1nd-universe-v0',
    worlds: [
      {
        key: '/work/repo-alpha',
        root: '/work/repo-alpha',
        name: 'repo-alpha',
        node_count: 3210,
        edge_count: 9000,
        updated_ms: now() - 2 * 60 * 60 * 1000, // 2h stale — shown honestly
        awake: true,
        presences: [
          {
            agent_id: 'atlas',
            root: '/work/repo-alpha',
            first_seen_ms: now() - 12 * 60_000,
            last_seen_ms: now() - 20_000,
            query_count: 9,
            mutation: { observed_at_ms: now() - 3_000 },
            task_ref: 'wire the panorama',
          },
        ],
        pending: { stamps: 2, ratifies: 1 },
        letters: { merge_wait: 2, total: 5 },
      },
      {
        key: '/work/repo-beta',
        root: '/work/repo-beta',
        name: 'repo-beta',
        node_count: 120,
        edge_count: 200,
        updated_ms: now() - 5 * 24 * 60 * 60 * 1000, // 5d — dims, never dark
        awake: false,
        presences: [],
        pending: { stamps: 0, ratifies: 0 },
        letters: { merge_wait: 0, total: 0 },
      },
    ],
    owner: { alerts_pending: 0 },
    totals: { worlds: 2, awake: 1, pending: 3 },
  };
}

const EMPTY_UNIVERSE = {
  schema: 'm1nd-universe-v0',
  worlds: [],
  owner: { alerts_pending: 0 },
  totals: { worlds: 0, awake: 0, pending: 0 },
};

/** One merge_wait mission head for repo-alpha's tray (the room a stamp opens). */
function mergeWaitMissions() {
  return {
    served_brain: { project_root: '/work/repo-alpha', display_name: 'repo-alpha' },
    missions: [
      {
        mission_id: 'msn_00000000a001',
        head_letter_id: 'aaaa11110002',
        head: {
          schema: 'm1nd-mission-letter-v0',
          mission_id: 'msn_00000000a001',
          mission_seq: 2,
          prev_letter_id: 'aaaa11110001',
          block_id: 'sb_probe',
          brain_ref: 'repo-alpha',
          seat: 'hand',
          capability: 'build-runner',
          phase: 'merge_wait',
          gate: { command: 'cargo test', exit_status: 0, artifact_hash: 'sha256:deadbeef' },
          packet_ref: 'sha256:packet',
          tokens_total: 0,
          started_at: '2026-07-14T09:00:00Z',
          updated_at: '2026-07-14T10:00:00Z',
        },
        superseded_count: 0,
      },
    ],
  };
}

interface MockOpts {
  universe?: unknown;
  instances?: unknown[];
  skeletonPresent?: boolean;
}

/** Wire the mocked owner. Every /api/* route answers in-page; unknowns fall to
 *  an empty JSON so no request ever hangs the landing decision. */
function mockOwner(page: Page, opts: MockOpts = {}) {
  const universe = opts.universe ?? twoWorldUniverse();
  const instances = opts.instances ?? [instance];
  const skeleton = opts.skeletonPresent ?? false;
  return page.route(
    (url) => url.pathname.startsWith('/api/'),
    async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname;
      try {
        if (path === '/api/health') return await json(route, health);
        if (path === '/api/universe') return await json(route, universe);
        if (path === '/api/instances') return await json(route, { instances });
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
          if (url.searchParams.get('kind') === 'mission') return await json(route, mergeWaitMissions());
          return await json(route, { missions: [] });
        }
        if (path === '/api/tools/system_blocks_snapshot')
          return await json(
            route,
            skeleton
              ? {
                  result: {
                    present: true,
                    store_version: 3,
                    block_count: 0,
                    store: {
                      schema: 'm1nd-system-block-store-v0',
                      store_version: 3,
                      skeleton: {
                        skeleton_id: 'sk_probe',
                        version: 1,
                        state: 'ratified',
                        ratification: {
                          method: 'human-ui',
                          ratifier: 'owner',
                          ratified_at: '2026-07-12T00:00:00Z',
                          commit: '',
                        },
                      },
                      blocks: [],
                      unmapped_policy: { visible: true, default_action: 'leave_unmapped_until_ratified' },
                    },
                  },
                }
              : { result: { present: false, honest: 'no skeleton yet' } },
          );
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

// ── 1. the entry rule: ≥1 world → the Universe leads ──────────────────────────
test('with ≥1 world the Universe surface renders — a labelled disc + the serif FACTS header', async ({
  page,
}) => {
  await mockOwner(page);
  await page.goto('/');

  const universe = page.locator('[data-role="universe"]');
  await expect(universe).toBeVisible();
  // The L0 header states universe FACTS (counts) — never a cross-brain vital.
  await expect(page.locator('[data-role="universe-headline"]')).toHaveText(
    '2 worlds · 1 awake · 3 await your hand',
  );
  // Both worlds paint as labelled discs; the stale one shows its age as DATA.
  await expect(page.locator('[data-role="universe-canvas"]')).toBeVisible();
  await expect(page.locator('[data-world-name="repo-alpha"]')).toBeVisible();
  await expect(page.locator('[data-world-name="repo-beta"]')).toBeVisible();
  await expect(page.locator('[data-role="universe-satellite"]').first()).toBeVisible();
  // The tray does NOT ride the L0 — the Landing is its aggregate here.
  await expect(page.locator('[data-role="mission-tray"]')).toHaveCount(0);

  await page.screenshot({ path: `${ARTIFACTS}/universe-01-panorama.png`, fullPage: true });
});

// ── 2. the entry rule falls through UNCHANGED with zero worlds ─────────────────
test('zero worlds + zero brains → the Threshold is fail-closed with no mutation affordance', async ({ page }) => {
  let ingestCalls = 0;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/tools/ingest') ingestCalls += 1;
  });
  await mockOwner(page, { universe: EMPTY_UNIVERSE, instances: [], skeletonPresent: false });
  await page.goto('/');

  const threshold = page.locator('[data-surface="threshold"]');
  await expect(threshold.locator('[data-role="bootstrap-unavailable"]')).toBeVisible();
  await expect(threshold).toContainText('brain_bootstrap_consumer_not_installed');
  await expect(threshold.locator('[data-role="read-first-repo"], [data-role="threshold-path"], form')).toHaveCount(0);
  expect(ingestCalls).toBe(0);
  await expect(page.locator('[data-role="universe"]')).toHaveCount(0);
});

test('zero worlds but a bound-brain skeleton → the Build Map front door (prior doctrine intact)', async ({
  page,
}) => {
  await mockOwner(page, { universe: EMPTY_UNIVERSE, instances: [instance], skeletonPresent: true });
  await page.goto('/');

  // The Build Map leads (never the Universe, never the Threshold) — the F1
  // front-door behaviour is byte-preserved for a zero-world owner.
  await expect(page.locator('[data-role="mission-tray"]')).toBeVisible();
  await expect(page.locator('[data-role="universe"]')).toHaveCount(0);
  await expect(page.locator('[data-role="read-first-repo"]')).toHaveCount(0);
});

// ── 3. the Landing: a merge_wait stamp, and a click navigates to the room ──────
test('the Landing lists a merge_wait stamp; clicking it navigates to that world’s tray card', async ({
  page,
}) => {
  await mockOwner(page);
  await page.goto('/');

  const landing = page.locator('[data-role="the-landing"]');
  await expect(landing).toBeVisible();
  await expect(landing.locator('[data-role="landing-badge"]')).toContainText('3 await your hand');

  const stamp = landing.locator('[data-landing-kind="stamp"]');
  await expect(stamp).toHaveCount(1);
  await expect(stamp).toContainText('2 receipts await your stamp');
  await expect(stamp.locator('[data-role="landing-item-chip"]')).toHaveText('repo-alpha');

  // Click NAVIGATES to the existing per-type flow — the world's room, where the
  // mission tray (and its merge_wait card) live. It composes no write; the L0 is
  // left behind. The tray's own card render is proven in mission-tray.test.
  await stamp.click();
  await expect(page.locator('[data-role="universe"]')).toHaveCount(0);
  await expect(page.locator('[data-role="mission-tray"]')).toBeVisible();

  await page.screenshot({ path: `${ARTIFACTS}/universe-02-landing-nav.png`, fullPage: true });
});

// ── 4. anti-regression: a world click reaches its map room ────────────────────
test('clicking a world opens its room (the map), leaving the L0 — existing surfaces stay reachable', async ({
  page,
}) => {
  await mockOwner(page);
  await page.goto('/');

  await expect(page.locator('[data-role="universe"]')).toBeVisible();
  await page.locator('[data-world-name="repo-alpha"]').click();

  // Off the L0, into the world's room: the tray (a working-surface fixture) is
  // back, and the wordmark home button can return to the Universe.
  await expect(page.locator('[data-role="universe"]')).toHaveCount(0);
  await expect(page.locator('[data-role="mission-tray"]')).toBeVisible();
  const home = page.locator('[data-role="open-universe"]');
  await expect(home).toBeVisible();
  await home.click();
  await expect(page.locator('[data-role="universe"]')).toBeVisible();
});
